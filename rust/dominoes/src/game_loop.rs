use std::sync::{Arc, Mutex};
use std::time::Duration;

use share_type_public::{DominoesActionSource, DominoesPhase};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{Dispatch, RoomService, SessionSenders};

use crate::action;
use crate::core::{DISCONNECTED_TURN_SECONDS, DominoesRoundState};
use crate::game::{
    StateHandle, StateRegistry, append_action_outcome, append_round_start_bundle,
    append_snapshot_and_turn, append_starter_selected, round_start_bundle,
};

const LOOP_POLL: Duration = Duration::from_millis(100);
const COUNTDOWN_TICK: Duration = Duration::from_secs(1);
const AI_TURN_SECONDS: u32 = 1;

fn room_matches_common(
    room_service: &RoomService,
    room_key: &str,
    expected_common: &Arc<Mutex<ws_common::CommonGameState>>,
) -> bool {
    room_service
        .room_common_state(room_key)
        .is_some_and(|current| Arc::ptr_eq(&current, expected_common))
}

fn state_should_stop(state: &DominoesRoundState) -> bool {
    if state.phase == DominoesPhase::GameOver {
        return true;
    }
    let common = state.base.lock().expect("dominoes common state lock");
    common.stop_requested || common.players.len() != state.positions.len()
}

fn automatic_source_and_cap(state: &DominoesRoundState) -> (Option<DominoesActionSource>, u32) {
    let common = state.base.lock().expect("dominoes common state lock");
    let position = state.current_position;
    if common.is_ai_position(position) {
        return (Some(DominoesActionSource::NativeAi), AI_TURN_SECONDS);
    }
    if common.is_ai_takeover_position(position) {
        return (Some(DominoesActionSource::AiTakeover), AI_TURN_SECONDS);
    }
    if common.is_away(position) || common.is_disconnected(position) {
        return (None, DISCONNECTED_TURN_SECONDS);
    }
    (None, state.remaining_seconds)
}

fn is_paused(state: &DominoesRoundState) -> bool {
    state
        .base
        .lock()
        .expect("dominoes common state lock")
        .paused
}

async fn send_dispatch(dispatch: Dispatch, senders: &SessionSenders) {
    let mut encoded = Vec::with_capacity(dispatch.messages.len());
    for delivery in dispatch.messages {
        if let Ok(frame) = ws_common::to_text_message(&delivery.payload) {
            encoded.push((delivery.recipient, frame));
        }
    }
    let senders = senders.lock().await;
    for (recipient, frame) in encoded {
        if let Some(sender) = senders.get(&recipient) {
            let _ = sender.send(frame);
        }
    }
}

fn remove_registry_state(room_key: &str, state: &StateHandle, states: &StateRegistry) {
    // GameOver 仍允许玩家重连查看最终结算；下一次 START 会主动清理这份终局状态。
    if state.lock().expect("dominoes state lock").phase == DominoesPhase::GameOver {
        return;
    }
    let mut states = states.lock().expect("dominoes registry lock");
    if states
        .get(room_key)
        .is_some_and(|current| Arc::ptr_eq(current, state))
    {
        states.remove(room_key);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopStep {
    Stop,
    Again,
    Poll,
    Countdown(i32),
}

/// 调用方持有房间锁和牌局锁；状态变更、快照及私牌属于同一次提交。
/// 等待倒计时在锁外进行，唤醒后只扣减仍属于同一决策窗口的时间。
fn advance_loop(
    state: &mut DominoesRoundState,
    room_service: &RoomService,
    room_key: &str,
    tick_revision: Option<i32>,
    dispatch: &mut Dispatch,
) -> LoopStep {
    if state_should_stop(state) {
        return LoopStep::Stop;
    }
    if is_paused(state) {
        return LoopStep::Poll;
    }
    let mut changed = if let Some(revision) = tick_revision {
        match state.phase {
            DominoesPhase::StarterSelection => state.tick_starter_selection(revision),
            DominoesPhase::Play => state.tick_remaining_seconds(revision),
            DominoesPhase::RoundOver => state.tick_round_transition(revision),
            DominoesPhase::GameOver => false,
        }
    } else {
        false
    };

    if let Ok(Some(outcome)) = action::forced_no_playable_turn(state) {
        append_action_outcome(room_service, room_key, outcome, dispatch);
        return LoopStep::Again;
    }
    let (source, cap) = automatic_source_and_cap(state);
    if matches!(
        state.phase,
        DominoesPhase::Play | DominoesPhase::StarterSelection
    ) {
        changed |= state.cap_remaining_seconds(cap);
    }
    if state.remaining_seconds == 0 {
        match state.phase {
            DominoesPhase::StarterSelection => {
                if let Some(slot) = state.automatic_starter_slot()
                    && let Ok(result) = state.select_starter_slot(state.current_position, slot)
                {
                    append_starter_selected(
                        room_service,
                        room_key,
                        result,
                        source.unwrap_or(DominoesActionSource::Timeout),
                        dispatch,
                    );
                    if result.starter_position.is_some() {
                        append_round_start_bundle(
                            room_service,
                            room_key,
                            round_start_bundle(state),
                            dispatch,
                        );
                    } else {
                        append_snapshot_and_turn(
                            room_service,
                            room_key,
                            &state.table_snapshot(),
                            dispatch,
                        );
                    }
                    return LoopStep::Again;
                }
            }
            DominoesPhase::Play => {
                let source = source.unwrap_or(DominoesActionSource::Timeout);
                match action::automatic_turn(state, source, |state, position, legal_plays| {
                    if source == DominoesActionSource::Timeout {
                        legal_plays[0]
                    } else {
                        crate::ai::choose_play(state, position, legal_plays)
                    }
                }) {
                    Ok(outcome) => {
                        append_action_outcome(room_service, room_key, outcome, dispatch);
                        return LoopStep::Again;
                    }
                    Err(error) => ws_common::dlog!(
                        ws_common::tracing::Level::WARN,
                        "[dominoes] automatic turn failed: {}",
                        error.message()
                    ),
                }
            }
            DominoesPhase::RoundOver => {
                if state.start_next_round().is_ok() {
                    append_round_start_bundle(
                        room_service,
                        room_key,
                        round_start_bundle(state),
                        dispatch,
                    );
                    return LoopStep::Again;
                }
            }
            DominoesPhase::GameOver => return LoopStep::Stop,
        }
        // 错误状态不能形成占满执行器的零等待循环。
        state.remaining_seconds = 1;
        changed = true;
    }
    if changed {
        append_snapshot_and_turn(room_service, room_key, &state.table_snapshot(), dispatch);
    }
    LoopStep::Countdown(state.turn_revision)
}

pub(crate) fn start_game_loop(
    room_key: String,
    state: StateHandle,
    room_service: Arc<AsyncMutex<RoomService>>,
    senders: SessionSenders,
    states: StateRegistry,
) {
    tokio::spawn(async move {
        let expected_common = Arc::clone(&state.lock().expect("dominoes state lock").base);
        let mut tick_revision = None;
        loop {
            let room = room_service.lock().await;
            if !room_matches_common(&room, &room_key, &expected_common) {
                break;
            }
            let mut dispatch = Dispatch::default();
            let step = {
                let mut state = state.lock().expect("dominoes state lock");
                advance_loop(&mut state, &room, &room_key, tick_revision, &mut dispatch)
            };
            // 与通用请求相同，消息全部写入有界队列后才允许下一次房间操作。
            // 不持有同步牌局锁跨越 await，也不在房间锁内等待网络或倒计时。
            if !dispatch.messages.is_empty() {
                send_dispatch(dispatch, &senders).await;
            }
            drop(room);
            tick_revision = None;
            match step {
                LoopStep::Stop => break,
                LoopStep::Again => {}
                LoopStep::Poll => tokio::time::sleep(LOOP_POLL).await,
                LoopStep::Countdown(revision) => {
                    tokio::time::sleep(COUNTDOWN_TICK).await;
                    tick_revision = Some(revision);
                }
            }
        }
        remove_registry_state(&room_key, &state, &states);
    });
}

#[cfg(test)]
#[path = "game_loop/tests.rs"]
mod tests;

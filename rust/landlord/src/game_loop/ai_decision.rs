//! 计算线程仅持有公开观察，禁止在持有牌局锁时等待 AI 推理。

use super::*;

#[cfg(feature = "official")]
use crate::ai::{AiObservation, choose_bid_from_observation, choose_play_from_observation};

pub(super) struct PreparedAiAction {
    phase: LandlordPhase,
    position: usize,
    #[cfg(feature = "official")]
    observation: AiObservation,
    pub(super) plan: PlannedAiAction,
    pub(super) bid: Option<u8>,
}

impl PreparedAiAction {
    pub(super) fn is_current(&self, state: &LandlordLoopState) -> bool {
        let same_turn = active_ai_turn(state)
            && state.phase == self.phase
            && state.current_position == self.position;
        #[cfg(feature = "official")]
        return same_turn
            && AiObservation::from_state(state, self.position).as_ref() == Some(&self.observation);
        #[cfg(not(feature = "official"))]
        same_turn
    }
}

fn active_ai_turn(state: &LandlordLoopState) -> bool {
    !state.stop_requested()
        && !state.is_paused()
        && !state.action_received()
        && state.players_snapshot().len() == 3
        && state.is_ai_controlled_position(state.current_position)
        && matches!(
            state.phase,
            LandlordPhase::CallLandlord | LandlordPhase::Play
        )
}

#[cfg(feature = "official")]
fn calculation_slots() -> Arc<tokio::sync::Semaphore> {
    static SLOTS: std::sync::OnceLock<Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    Arc::clone(SLOTS.get_or_init(|| {
        // 先用有界 CPU 并行度，容量结论仍需真实房间压力评估。
        let count = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .clamp(1, 8);
        Arc::new(tokio::sync::Semaphore::new(count))
    }))
}

#[cfg(feature = "official")]
pub(super) async fn prepare_ai_action(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
) -> Option<PreparedAiAction> {
    prepare_with(state, calculation_slots(), |observation| {
        if observation.phase == LandlordPhase::CallLandlord {
            (Some(choose_bid_from_observation(&observation)), None)
        } else {
            (None, Some(choose_play_from_observation(&observation)))
        }
    })
    .await
}

#[cfg(feature = "official")]
async fn prepare_with(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    slots: Arc<tokio::sync::Semaphore>,
    calculate: impl FnOnce(AiObservation) -> (Option<u8>, Option<Vec<i32>>) + Send + 'static,
) -> Option<PreparedAiAction> {
    let observation = {
        let state = state.lock().unwrap();
        if !active_ai_turn(&state) {
            return None;
        }
        AiObservation::from_state(&state, state.current_position)?
    };
    let mut prepared = PreparedAiAction {
        phase: observation.phase,
        position: observation.position,
        observation: observation.clone(),
        plan: PlannedAiAction {
            delay: AI_ACTION_DELAY,
            bomb_signal: false,
            play: None,
        },
        bid: None,
    };
    let pending_state = Arc::clone(state);
    let pending_observation = observation.clone();
    let work = async move {
        let permit = slots.acquire_owned().await.ok()?;
        {
            let state = pending_state.lock().unwrap();
            if !active_ai_turn(&state)
                || AiObservation::from_state(&state, pending_observation.position).as_ref()
                    != Some(&pending_observation)
            {
                return None;
            }
        }
        tokio::task::spawn_blocking(move || {
            // 即使房间停止并丢弃结果，计算结束前仍占用许可，不能突破上限。
            let _permit = permit;
            calculate(observation)
        })
        .await
        .map_err(|error| {
            ws_common::dlog!(
                ws_common::tracing::Level::WARN,
                "[landlord] AI calculation failed: {}",
                error
            );
        })
        .ok()
    };
    tokio::pin!(work);
    let (bid, play) = loop {
        tokio::select! {
            result = &mut work => break result?,
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if !prepared.is_current(&state.lock().unwrap()) { return None; }
            }
        }
    };
    let state = state.lock().unwrap();
    if !prepared.is_current(&state) {
        return None;
    }
    let valid = match prepared.phase {
        LandlordPhase::CallLandlord => {
            bid.is_some_and(|score| score <= 3 && (score == 0 || u32::from(score) > state.score))
                && play.is_none()
        }
        LandlordPhase::Play => {
            bid.is_none()
                && play.as_deref().is_some_and(|cards| {
                    crate::play_validator::validate_play_request(&state, prepared.position, cards)
                })
        }
        _ => false,
    };
    if !valid {
        ws_common::dlog!(
            ws_common::tracing::Level::WARN,
            "[landlord] rejecting invalid AI calculation result"
        );
        return None;
    }
    prepared.bid = bid;
    prepared.plan.bomb_signal = play
        .as_deref()
        .is_some_and(|cards| should_signal_ai_bomb(&state, prepared.position, cards));
    prepared.plan.delay = if prepared.plan.bomb_signal {
        AI_BOMB_SIGNAL_DELAY
    } else {
        AI_ACTION_DELAY
    };
    prepared.plan.play = play;
    Some(prepared)
}

#[cfg(not(feature = "official"))]
pub(super) async fn prepare_ai_action(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
) -> Option<PreparedAiAction> {
    let state = state.lock().unwrap();
    if !active_ai_turn(&state) {
        return None;
    }
    Some(PreparedAiAction {
        phase: state.phase,
        position: state.current_position,
        plan: plan_ai_action(&state, state.current_position),
        bid: (state.phase == LandlordPhase::CallLandlord)
            .then(|| choose_bid(&state, state.current_position)),
    })
}

#[cfg(all(test, feature = "official"))]
mod tests;

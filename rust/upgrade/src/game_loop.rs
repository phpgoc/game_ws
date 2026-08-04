use std::{sync::Arc, time::Duration};

use share_type_public::{
    CommonEvent, UpgradePhase, UpgradeWsCode, WsCode, WsUpgradeBottomBuriedEvent,
    WsUpgradeHandEvent, WsUpgradePlayEvent,
};
use tokio::sync::Mutex;
use ws_common::{
    Delivery, Dispatch, GameState, OutboundPayload, RoomService, SessionSenders, to_text_message,
};

use crate::state::UpgradeStateHandle;

const SETTLEMENT_DELAY: Duration = Duration::from_secs(3);

pub fn start_upgrade_game_loop(
    room_key: String,
    state: UpgradeStateHandle,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
) {
    tokio::spawn(async move {
        let configs = room_service
            .lock()
            .await
            .room_configs(&room_key)
            .unwrap_or_default();
        let play_time = configs.get("play_time").copied().unwrap_or(30).max(1) as u32;
        loop {
            let phase = {
                let guard = state.lock().unwrap();
                if guard.stop_requested() {
                    break;
                }
                guard.phase
            };
            match phase {
                UpgradePhase::Bury => {
                    let dispatch =
                        timeout_bury_dispatch(&room_key, &state, &room_service, play_time).await;
                    deliver(dispatch, &senders).await;
                }
                UpgradePhase::Play => {
                    let dispatch =
                        timeout_play_dispatch(&room_key, &state, &room_service, play_time).await;
                    deliver(dispatch, &senders).await;
                }
                UpgradePhase::Settlement => {
                    tokio::time::sleep(SETTLEMENT_DELAY).await;
                    let mut dispatch = Dispatch::default();
                    let advanced = {
                        let mut guard = state.lock().unwrap();
                        if guard.stop_requested() || guard.phase != UpgradePhase::Settlement {
                            false
                        } else {
                            guard.advance_after_settlement().unwrap_or(false)
                        }
                    };
                    if advanced {
                        let snapshot = state.lock().unwrap().snapshot();
                        let room = room_service.lock().await;
                        room.broadcast(
                            &room_key,
                            WsCode::START as i32,
                            serde_json::json!({}),
                            &mut dispatch,
                        );
                        room.broadcast(
                            &room_key,
                            WsCode::TABLE_SNAPSHOT as i32,
                            snapshot,
                            &mut dispatch,
                        );
                    } else {
                        break;
                    }
                    deliver(dispatch, &senders).await;
                }
                UpgradePhase::Start | UpgradePhase::Deal => break,
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn timeout_bury_dispatch(
    room_key: &str,
    state: &UpgradeStateHandle,
    room_service: &Arc<Mutex<RoomService>>,
    play_time: u32,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let result = {
        let mut guard = state.lock().unwrap();
        if guard.base.lock().unwrap().turn_countdown > 0 {
            let countdown = guard.base.lock().unwrap().turn_countdown.saturating_sub(1);
            guard.set_turn_countdown(countdown);
            None
        } else if guard.timeout_bury().unwrap_or(false) {
            guard.set_turn_countdown(play_time);
            Some((
                guard.dealer_position,
                guard.player_name(guard.dealer_position),
                guard.rules.bottom_card_count,
                guard.private_hand(guard.dealer_position),
                guard.snapshot(),
            ))
        } else {
            None
        }
    };
    let Some((position, name, bottom_card_count, hand, snapshot)) = result else {
        return dispatch;
    };
    let room = room_service.lock().await;
    room.broadcast(
        room_key,
        UpgradeWsCode::BOTTOM_BURIED as i32,
        WsUpgradeBottomBuriedEvent {
            position: position as i32,
            name,
            bottom_card_count: bottom_card_count as i32,
        },
        &mut dispatch,
    );
    send_private(
        &room,
        room_key,
        position,
        UpgradeWsCode::HAND_UPDATED,
        WsUpgradeHandEvent {
            position: position as i32,
            cards: hand,
        },
        &mut dispatch,
    );
    room.broadcast(
        room_key,
        WsCode::TABLE_SNAPSHOT as i32,
        snapshot,
        &mut dispatch,
    );
    dispatch
}

async fn timeout_play_dispatch(
    room_key: &str,
    state: &UpgradeStateHandle,
    room_service: &Arc<Mutex<RoomService>>,
    play_time: u32,
) -> Dispatch {
    let mut dispatch = Dispatch::default();
    let result = {
        let mut guard = state.lock().unwrap();
        if guard.base.lock().unwrap().turn_countdown > 0 {
            let countdown = guard.base.lock().unwrap().turn_countdown.saturating_sub(1);
            guard.set_turn_countdown(countdown);
            None
        } else {
            match guard.timeout_play() {
                Ok(Some(resolution)) => {
                    let position = guard.current_position;
                    if !resolution.finished {
                        guard.set_turn_countdown(play_time);
                    }
                    Some((
                        position,
                        guard.player_name(position),
                        resolution,
                        guard.current_position,
                        guard.trick_index,
                        guard.private_hand(position).len(),
                        guard.snapshot(),
                    ))
                }
                _ => None,
            }
        }
    };
    let Some((position, name, resolution, next_position, trick_index, remaining, snapshot)) =
        result
    else {
        return dispatch;
    };
    let room = room_service.lock().await;
    room.broadcast(
        room_key,
        WsCode::PLAY as i32,
        WsUpgradePlayEvent {
            position: position as i32,
            name,
            cards: resolution.played_cards,
            trick_index,
            next_position: next_position as i32,
            remaining_hand_count: remaining as i32,
            failed_throw: resolution.failed_throw,
        },
        &mut dispatch,
    );
    room.broadcast(
        room_key,
        WsCode::TABLE_SNAPSHOT as i32,
        snapshot,
        &mut dispatch,
    );
    if resolution.finished {
        let settlement = state.lock().unwrap().settlement_event();
        room.broadcast(
            room_key,
            WsCode::GAME_OVER as i32,
            settlement,
            &mut dispatch,
        );
    }
    dispatch
}

fn send_private<T: serde::Serialize>(
    room: &RoomService,
    room_key: &str,
    position: usize,
    code: UpgradeWsCode,
    payload: T,
    dispatch: &mut Dispatch,
) {
    let Some((session_id, _, _, _)) = room
        .room_members(room_key)
        .into_iter()
        .find(|(_, _, member_position, _)| *member_position == position)
    else {
        return;
    };
    dispatch.messages.push(Delivery {
        recipient: session_id,
        payload: OutboundPayload::Event(CommonEvent {
            code: code as i32,
            data: serde_json::to_value(payload).unwrap_or_default(),
        }),
    });
}

async fn deliver(dispatch: Dispatch, senders: &SessionSenders) {
    let senders = senders.lock().await;
    for delivery in dispatch.messages {
        let Some(sender) = senders.get(&delivery.recipient) else {
            continue;
        };
        if let Ok(frame) = to_text_message(&delivery.payload) {
            let _ = sender.send(frame);
        }
    }
}

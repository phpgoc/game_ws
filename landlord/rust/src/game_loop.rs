use std::sync::Arc;
use std::time::Duration;

use share_type_public::{WsCode, WsDealEvent, WsDealFaceDownCardsEvent};
use tokio::sync::Mutex;
use share_type_public::games::landlord::LandlordRoomSettings;
use ws_common::{RoomService, SessionSenders};

use crate::game_state::LandlordLoopState;
use share_type_public::LandlordPhase;

fn get_player_name(state: &LandlordLoopState, position: usize) -> String {
    state.base.player_name(position)
}

fn current_turn_secs(state: &LandlordLoopState,setting :&LandlordRoomSettings) -> i32 {

    if state.base.is_away(state.current_position) {
        setting.away_time.current
    } else {
        setting.play_time.current
    }
}


async fn handle_start_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    let positions_hands: Vec<(usize, Vec<i32>, String)>;
    let hidden: Vec<i32>;
    {
        let mut s = state.lock().unwrap();
        s.generate_card();
        s.next_phase(); // Start -> CallLandlord
        s.call_round_count = 0;
        s.base.action_received = false;
        s.base.turn_countdown = 0;
        hidden = s.hidden_cards.clone();
        positions_hands = sorted_positions
            .iter()
            .filter_map(|&pos| {
                let hand = s.hands.get(&pos)?.clone();
                let name = get_player_name(&s, pos);
                Some((pos, hand, name))
            })
            .collect();
    }

    for (pos, hand, name) in positions_hands {
        ws_common::send_to_position(
            room_key,
            pos,
            WsCode::DEAL as i32,
            WsDealEvent { name, cards: hand },
            room_service,
            senders,
        )
        .await;
    }
    ws_common::send_all(
        room_key,
        WsCode::DEAL_FACE_DOWN_CARDS as i32,
        WsDealFaceDownCardsEvent { cards: hidden },
        room_service,
        senders,
    )
    .await;
}

async fn handle_call_landlord_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
) {
    let (pos, name) = {
        let s = state.lock().unwrap();
        (s.current_position, get_player_name(&s, s.current_position))
    };

    let all_passed = {
        let mut s = state.lock().unwrap();
        s.call_round_count += 1;
        s.call_round_count >= sorted_positions.len()
    };
    if all_passed {
        let mut s = state.lock().unwrap();
        s.redeal();
        return;
    }

    {
        let mut s = state.lock().unwrap();
        if let Some(idx) = sorted_positions.iter().position(|&p| p == pos) {
            s.current_position = sorted_positions[(idx + 1) % sorted_positions.len()];
        }
    }
}

async fn handle_play_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    sorted_positions: &[usize],
) -> bool {
    let (pos, name, cards_played) = {
        let s = state.lock().unwrap();
        (
            s.current_position,
            get_player_name(&s, s.current_position),
            s.current_play.clone(),
        )
    };

    {
        let mut s = state.lock().unwrap();
        if !cards_played.is_empty() {
            s.last_play_position = pos;
            s.last_play = cards_played.clone();
            s.current_play = cards_played.clone();
        }
        if let Some(hand) = s.hands.get_mut(&pos) {
            for card in &cards_played {
                if let Some(idx) = hand.iter().position(|c| c == card) {
                    hand.remove(idx);
                }
            }
        }
    }

    false
}

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<LandlordLoopState>>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
) {
    tokio::spawn(async move {
        let sorted_positions: Vec<usize> = {
            let s = state.lock().unwrap();
            let mut p: Vec<usize> = s.base.players.keys().copied().collect();
            p.sort();
            p
        };

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            if state.clone().lock().unwrap().base.paused {
                continue;
            }

            let should_wait = {
                let mut s = state.lock().unwrap();
                if matches!(s.phase, LandlordPhase::CallLandlord | LandlordPhase::Play)
                    && !s.base.action_received
                    && s.base.turn_countdown != 0
                {
                    s.base.turn_countdown = s.base.turn_countdown.saturating_sub(1);
                    true
                } else {
                    false
                }
            };
            if should_wait {
                continue;
            }

            {
                let mut s = state.lock().unwrap();
                if matches!(s.phase, LandlordPhase::CallLandlord | LandlordPhase::Play)
                    && !s.base.action_received
                    && s.base.turn_countdown == 0
                {
                    let pos = s.current_position;
                    s.base.mark_away(pos);
                    s.base.action_received = true;
                }
            }

            let phase = { state.lock().unwrap().phase };
            match phase {
                LandlordPhase::Start => {
                    handle_start_phase(
                        &room_key,
                        &state,
                        &sorted_positions,
                        &room_service,
                        &senders,
                    )
                    .await;
                }
                LandlordPhase::CallLandlord => {
                    handle_call_landlord_phase(&room_key, &state, &sorted_positions).await;
                }
                LandlordPhase::Play => {
                    if handle_play_phase(&room_key, &state, &sorted_positions).await {
                        break;
                    }
                }
                LandlordPhase::Settlement => break,
            }
        }

        room_service.lock().await.clear_room_game_state(&room_key);
    });
}

use std::sync::Arc;
use std::time::Duration;

use share_type_public::{
    LandlordWsCode, WsCode, WsDealEvent, WsDealFaceDownCardsEvent, WsDealOpenCardsEvent, WsLandlordGameOverEvent,
    WsPlayEvent, WsPositionEvent, games::landlord::WsCallLandlordEvent,
};
use tokio::sync::{Mutex, mpsc};
use ws_common::{RoomService, SessionSenders};

use crate::game_state::{LandlordLoopState, LandlordPhase};

pub(crate) struct PlayerInput {
    position: usize,
    score: Option<u8>,
    cards: Option<Vec<i32>>,
    away: bool,
}

impl PlayerInput {
    pub(crate) fn call_landlord(position: usize, score: u8) -> Self {
        Self { position, score: Some(score), cards: None, away: false }
    }

    pub(crate) fn play(position: usize, cards: Vec<i32>) -> Self {
        Self { position, score: None, cards: Some(cards), away: false }
    }

    fn away(position: usize) -> Self {
        Self { position, score: None, cards: None, away: true }
    }

    fn matches_phase(&self, phase: LandlordPhase) -> bool {
        if self.away {
            return true;
        }
        match phase {
            LandlordPhase::CallLandlord => self.score.is_some(),
            LandlordPhase::Play => self.cards.is_some(),
            _ => false,
        }
    }
}

struct TickState {
    pending_input: Option<PlayerInput>,
    turn_announced: bool,
    turn_remaining: u32,
    call_count: usize,
}

fn get_player_name(state: &LandlordLoopState, position: usize) -> String {
    state.base.player_name(position)
}

fn collect_pending_input(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    actions: &mut mpsc::Receiver<PlayerInput>,
    tick: &mut TickState,
) {
    if tick.pending_input.is_some() {
        return;
    }
    let (pos, phase) = {
        let s = state.lock().unwrap();
        (s.current_position, s.phase)
    };
    while let Ok(input) = actions.try_recv() {
        if input.position == pos && input.matches_phase(phase) {
            tick.pending_input = Some(input);
            state.lock().unwrap().action_received = true;
            break;
        }
    }
}

async fn handle_start_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    turn_timeout_secs: u32,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    tick: &mut TickState,
) {
    let positions_hands: Vec<(usize, Vec<i32>, String)>;
    let hidden: Vec<i32>;
    {
        let mut s = state.lock().unwrap();
        s.generate_card();
        s.next_phase(); // Start -> CallLandlord
        s.action_received = false;
        hidden = s.hidden_cards.clone();
        positions_hands = sorted_positions.iter().filter_map(|&pos| {
            let hand = s.hands.get(&pos)?.clone();
            let name = get_player_name(&s, pos);
            Some((pos, hand, name))
        }).collect();
    }

    for (pos, hand, name) in positions_hands {
        ws_common::send_to_position(
            room_key, pos, WsCode::DEAL as i32,
            WsDealEvent { name, cards: hand },
            room_service, senders,
        ).await;
    }
    ws_common::send_all(
        room_key, WsCode::DEAL_FACE_DOWN_CARDS as i32,
        WsDealFaceDownCardsEvent { cards: hidden },
        room_service, senders,
    ).await;

    tick.pending_input = None;
    tick.turn_announced = false;
    tick.turn_remaining = turn_timeout_secs;
    tick.call_count = 0;
}

async fn handle_call_landlord_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    turn_timeout_secs: u32,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    tick: &mut TickState,
) {
    let (pos, name, action_received) = {
        let s = state.lock().unwrap();
        (s.current_position, get_player_name(&s, s.current_position), s.action_received)
    };

    if !tick.turn_announced {
        ws_common::send_all(
            room_key, WsCode::CHANGE_DEAL as i32,
            WsPositionEvent { position: pos as i32 },
            room_service, senders,
        ).await;
        tick.turn_announced = true;
        tick.turn_remaining = turn_timeout_secs;
    }

    if !action_received {
        tick.turn_remaining = tick.turn_remaining.saturating_sub(1);
        if tick.turn_remaining == 0 {
            state.lock().unwrap().base.mark_away(pos);
            tick.pending_input = Some(PlayerInput::away(pos));
            state.lock().unwrap().action_received = true;
        }
        return;
    }

    let input = tick.pending_input.take().unwrap_or_else(|| PlayerInput::away(pos));
    let score = if input.away { 0 } else { input.score.unwrap_or(0) };
    if input.away {
        ws_common::send_all(
            room_key, WsCode::AWAY as i32,
            WsPositionEvent { position: pos as i32 },
            room_service, senders,
        ).await;
    }

    ws_common::send_all(
        room_key, LandlordWsCode::CALL_LANDLORD as i32,
        WsCallLandlordEvent { name: name.clone(), score },
        room_service, senders,
    ).await;

    if score > 0 {
        let hidden_cards = {
            let mut s = state.lock().unwrap();
            s.landlord_position = Some(pos);
            s.score = score as u32;
            let hidden = s.hidden_cards.clone();
            s.hands.entry(pos).or_default().extend(hidden.iter().copied());
            s.next_phase(); // CallLandlord -> Play
            s.action_received = false;
            hidden
        };
        ws_common::send_all(
            room_key, WsCode::DEAL_OPEN_CARDS as i32,
            WsDealOpenCardsEvent { name, cards: hidden_cards },
            room_service, senders,
        ).await;
        tick.turn_announced = false;
        tick.turn_remaining = turn_timeout_secs;
        tick.call_count = 0;
        return;
    }

    tick.call_count += 1;
    if tick.call_count >= sorted_positions.len() {
        let mut s = state.lock().unwrap();
        s.redeal();
        s.action_received = false;
        tick.pending_input = None;
        tick.turn_announced = false;
        tick.turn_remaining = turn_timeout_secs;
        tick.call_count = 0;
        return;
    }

    {
        let mut s = state.lock().unwrap();
        if let Some(idx) = sorted_positions.iter().position(|&p| p == pos) {
            s.current_position = sorted_positions[(idx + 1) % sorted_positions.len()];
        }
        s.action_received = false;
    }
    tick.turn_announced = false;
    tick.turn_remaining = turn_timeout_secs;
}

async fn handle_play_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    turn_timeout_secs: u32,
    sorted_positions: &[usize],
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
    tick: &mut TickState,
) -> bool {
    let (pos, name, action_received) = {
        let s = state.lock().unwrap();
        (s.current_position, get_player_name(&s, s.current_position), s.action_received)
    };

    if !tick.turn_announced {
        ws_common::send_all(
            room_key, WsCode::CHANGE_DEAL as i32,
            WsPositionEvent { position: pos as i32 },
            room_service, senders,
        ).await;
        tick.turn_announced = true;
        tick.turn_remaining = turn_timeout_secs;
    }

    if !action_received {
        tick.turn_remaining = tick.turn_remaining.saturating_sub(1);
        if tick.turn_remaining == 0 {
            state.lock().unwrap().base.mark_away(pos);
            tick.pending_input = Some(PlayerInput::away(pos));
            state.lock().unwrap().action_received = true;
        }
        return false;
    }

    let input = tick.pending_input.take().unwrap_or_else(|| PlayerInput::away(pos));
    let cards_played: Vec<i32> = if input.away {
        ws_common::send_all(
            room_key, WsCode::AWAY as i32,
            WsPositionEvent { position: pos as i32 },
            room_service, senders,
        ).await;
        let s = state.lock().unwrap();
        s.hands.get(&pos).and_then(|h| h.first().copied()).into_iter().collect()
    } else {
        input.cards.unwrap_or_default()
    };

    {
        let mut s = state.lock().unwrap();
        if let Some(hand) = s.hands.get_mut(&pos) {
            for card in &cards_played {
                if let Some(idx) = hand.iter().position(|c| c == card) {
                    hand.remove(idx);
                }
            }
        }
    }

    ws_common::send_all(
        room_key, WsCode::PLAY as i32,
        WsPlayEvent { name: name.clone(), cards: cards_played },
        room_service, senders,
    ).await;

    let (hand_empty, landlord_pos) = {
        let s = state.lock().unwrap();
        (
            s.hands.get(&pos).map(|h| h.is_empty()).unwrap_or(false),
            s.landlord_position,
        )
    };

    if hand_empty {
        let is_landlord = landlord_pos == Some(pos);
        state.lock().unwrap().next_phase(); // Play -> Settlement
        ws_common::send_all(
            room_key, WsCode::GAME_OVER as i32,
            WsLandlordGameOverEvent { winner: name, is_landlord },
            room_service, senders,
        ).await;
        return true;
    }

    {
        let mut s = state.lock().unwrap();
        if let Some(idx) = sorted_positions.iter().position(|&p| p == pos) {
            s.current_position = sorted_positions[(idx + 1) % sorted_positions.len()];
        }
        s.action_received = false;
    }
    tick.turn_announced = false;
    tick.turn_remaining = turn_timeout_secs;
    false
}

pub(crate) fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<LandlordLoopState>>,
    turn_timeout_secs: u32,
    mut actions: mpsc::Receiver<PlayerInput>,
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

        let mut tick = TickState {
            pending_input: None,
            turn_announced: false,
            turn_remaining: turn_timeout_secs,
            call_count: 0,
        };

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            if state.lock().unwrap().base.paused {
                continue;
            }

            collect_pending_input(&state, &mut actions, &mut tick);

            let phase = { state.lock().unwrap().phase };
            match phase {
                LandlordPhase::Start => {
                    handle_start_phase(
                        &room_key,
                        &state,
                        turn_timeout_secs,
                        &sorted_positions,
                        &room_service,
                        &senders,
                        &mut tick,
                    ).await;
                }
                LandlordPhase::CallLandlord => {
                    handle_call_landlord_phase(
                        &room_key,
                        &state,
                        turn_timeout_secs,
                        &sorted_positions,
                        &room_service,
                        &senders,
                        &mut tick,
                    ).await;
                }
                LandlordPhase::Play => {
                    if handle_play_phase(
                        &room_key,
                        &state,
                        turn_timeout_secs,
                        &sorted_positions,
                        &room_service,
                        &senders,
                        &mut tick,
                    ).await {
                        break;
                    }
                }
                LandlordPhase::Settlement => break,
            }
        }

        room_service.lock().await.clear_room_game_state(&room_key);
    });
}

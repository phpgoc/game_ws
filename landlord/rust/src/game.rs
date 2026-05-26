use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use share_type_public::{
    Routes, WsCode, GameSettings, WsPositionEvent,
    LandlordRoutes, LandlordWsCode,
    WsDealEvent, WsDealFaceDownCardsEvent, WsDealOpenCardsEvent, WsPlayEvent, WsLandlordGameOverEvent,
    games::landlord::{LandlordRoomSettings, WsCallLandlordEvent},
};
use tokio::sync::{mpsc, Mutex};
use ws_common::{ClientRequest, Dispatch, GameHandler, RoomService, SessionId, SessionSenders};

use crate::game_state::{LandlordGameState, LandlordLoopState};

enum PlayerAction {
    CallLandlord { position: usize, score: u8 },
    Play { position: usize, cards: Vec<i32> },
    Away { position: usize },
}

impl PlayerAction {
    fn position(&self) -> usize {
        match self {
            PlayerAction::CallLandlord { position, .. } => *position,
            PlayerAction::Play { position, .. } => *position,
            PlayerAction::Away { position } => *position,
        }
    }
}

pub struct LandlordGameHandler {
    room_service: Option<Arc<Mutex<RoomService>>>,
    senders: Option<SessionSenders>,
    action_senders: HashMap<String, mpsc::Sender<PlayerAction>>,
}

impl Default for LandlordGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
            action_senders: HashMap::new(),
        }
    }
}

const MIN_PLAYERS: usize = 3;
const MAX_PLAYERS: usize = 3;

pub fn build_room_settings(_room_key: &str) -> Box<dyn ws_common::GameSettings> {
    Box::new(LandlordRoomSettings::default())
}

/// 每秒 tick 一次，等待当前位置的有效操作。
/// `turn_timeout_secs` 来自房间设置，不存在 state 里。
/// - 收到有效操作 → 设 action_received = true，返回 Some(action)
/// - paused 期间不递减倒计时
/// - 倒计时归零 → mark_away，返回 None
async fn wait_for_turn(
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    actions: &mut mpsc::Receiver<PlayerAction>,
    turn_timeout_secs: u32,
) -> Option<PlayerAction> {
    state.lock().unwrap().action_received = false;
    let mut remaining = turn_timeout_secs;
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let pos = state.lock().unwrap().current_position;

        // 非阻塞取当前位置的有效动作
        let mut received: Option<PlayerAction> = None;
        while let Ok(action) = actions.try_recv() {
            if action.position() == pos {
                received = Some(action);
                break;
            }
        }

        if let Some(action) = received {
            state.lock().unwrap().action_received = true;
            return Some(action);
        }

        // 暂停时不递减
        if state.lock().unwrap().base.paused {
            continue;
        }

        remaining = remaining.saturating_sub(1);
        if remaining == 0 {
            let mut s = state.lock().unwrap();
            let pos = s.current_position;
            s.base.mark_away(pos);
            return None;
        }
    }
}

fn get_player_name(state: &LandlordLoopState, position: usize) -> String {
    state.base.player_name(position)
}

async fn deal_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    // generate_card shuffles and fills state.hands + state.hidden_cards
    let positions_hands: Vec<(usize, Vec<i32>, String)>;
    let hidden: Vec<i32>;
    {
        let mut s = state.lock().unwrap();
        s.generate_card();
        s.next_phase(); // Start → CallLandlord, sets current_position
        hidden = s.hidden_cards.clone();
        let mut sorted: Vec<usize> = s.base.players.keys().copied().collect();
        sorted.sort();
        positions_hands = sorted.iter().filter_map(|&pos| {
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
}

/// Returns true if a landlord was chosen, false if all passed (reshuffle needed).
async fn call_landlord_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    actions: &mut mpsc::Receiver<PlayerAction>,
    turn_timeout_secs: u32,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) -> bool {
    let sorted_positions: Vec<usize> = {
        let s = state.lock().unwrap();
        let mut pos: Vec<usize> = s.base.players.keys().copied().collect();
        pos.sort();
        pos
    };

    for &pos in &sorted_positions {
        let name = {
            let mut s = state.lock().unwrap();
            s.current_position = pos;
            get_player_name(&s, pos)
        };

        // Announce whose turn it is
        ws_common::send_all(
            room_key, WsCode::CHANGE_ROUND as i32,
            WsPositionEvent { position: pos as i32 },
            room_service, senders,
        ).await;

        let action = wait_for_turn(state, actions, turn_timeout_secs).await;

        let score = match action {
            Some(PlayerAction::CallLandlord { score, .. }) => score,
            _ => {
                // Timeout — auto-pass, already marked away in wait_for_turn
                ws_common::send_all(
                    room_key, WsCode::AWAY as i32,
                    WsPositionEvent { position: pos as i32 },
                    room_service, senders,
                ).await;
                0
            }
        };

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
                s.next_phase(); // CallLandlord → Play, sets current_position = landlord
                hidden
            };

            ws_common::send_all(
                room_key, WsCode::DEAL_OPEN_CARDS as i32,
                WsDealOpenCardsEvent { name: name.clone(), cards: hidden_cards },
                room_service, senders,
            ).await;

            return true;
        }
    }

    false // all passed
}

/// Returns the winner name if the game is over.
async fn play_phase(
    room_key: &str,
    state: &Arc<std::sync::Mutex<LandlordLoopState>>,
    actions: &mut mpsc::Receiver<PlayerAction>,
    turn_timeout_secs: u32,
    room_service: &Arc<Mutex<RoomService>>,
    senders: &SessionSenders,
) {
    loop {
        let (pos, name) = {
            let s = state.lock().unwrap();
            let pos = s.current_position;
            let name = get_player_name(&s, pos);
            (pos, name)
        };

        // Check if current player already has empty hand (shouldn't happen, but guard)
        let hand_empty = {
            let s = state.lock().unwrap();
            s.hands.get(&pos).map(|h| h.is_empty()).unwrap_or(true)
        };
        if hand_empty {
            break;
        }

        ws_common::send_all(
            room_key, WsCode::CHANGE_ROUND as i32,
            WsPositionEvent { position: pos as i32 },
            room_service, senders,
        ).await;

        let action = wait_for_turn(state, actions, turn_timeout_secs).await;

        let cards_played: Vec<i32> = match action {
            Some(PlayerAction::Play { cards, .. }) => cards,
            _ => {
                // Timeout — auto-play first card, already marked away
                ws_common::send_all(
                    room_key, WsCode::AWAY as i32,
                    WsPositionEvent { position: pos as i32 },
                    room_service, senders,
                ).await;
                let s = state.lock().unwrap();
                s.hands.get(&pos).and_then(|h| h.first().copied()).into_iter().collect()
            }
        };

        // Remove played cards from hand
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

        // Check win condition
        let (hand_empty, landlord_pos) = {
            let s = state.lock().unwrap();
            let empty = s.hands.get(&pos).map(|h| h.is_empty()).unwrap_or(false);
            (empty, s.landlord_position)
        };

        if hand_empty {
            let is_landlord = landlord_pos == Some(pos);
            {
                let mut s = state.lock().unwrap();
                s.next_phase(); // Play → Settlement
            }
            ws_common::send_all(
                room_key, WsCode::GAME_OVER as i32,
                WsLandlordGameOverEvent { winner: name, is_landlord },
                room_service, senders,
            ).await;
            return;
        }

        // Advance to next position
        {
            let mut s = state.lock().unwrap();
            let sorted: Vec<usize> = {
                let mut p: Vec<usize> = s.base.players.keys().copied().collect();
                p.sort();
                p
            };
            if let Some(idx) = sorted.iter().position(|&p| p == pos) {
                s.current_position = sorted[(idx + 1) % sorted.len()];
            }
        }
    }
}

fn start_game_loop(
    room_key: String,
    state: Arc<std::sync::Mutex<LandlordLoopState>>,
    turn_timeout_secs: u32,
    mut actions: mpsc::Receiver<PlayerAction>,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
) {
    tokio::spawn(async move {
        loop {
            deal_phase(&room_key, &state, &room_service, &senders).await;

            let landlord_found = call_landlord_phase(
                &room_key, &state, &mut actions,
                turn_timeout_secs, &room_service, &senders,
            ).await;

            if !landlord_found {
                state.lock().unwrap().redeal();
                continue;
            }

            play_phase(
                &room_key, &state, &mut actions,
                turn_timeout_secs, &room_service, &senders,
            ).await;

            break;
        }

        room_service.lock().await.clear_room_game_state(&room_key);
    });
}

impl GameHandler for LandlordGameHandler {
    fn build_room_settings(&self, room_key: &str) -> Box<dyn GameSettings> {
        build_room_settings(room_key)
    }

    fn get_player_limits(&self) -> (usize, usize) {
        (MIN_PLAYERS, MAX_PLAYERS)
    }

    fn build_game_state(&self) -> Option<Box<dyn ws_common::game_state::GameState>> {
        Some(Box::new(LandlordGameState::new()))
    }

    fn set_context(&mut self, senders: SessionSenders, room_service: Arc<Mutex<RoomService>>) {
        self.senders = Some(senders);
        self.room_service = Some(room_service);
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        match request.route {
            r if r == Routes::START as i32 => {
                if let Some(position) = room_service.session_position(session_id) {
                    if position != 0 {
                        return room_service.permission_denied_response(session_id, Routes::START as i32);
                    }
                } else {
                    return room_service.unsupported_response(session_id, Routes::START as i32);
                }

                let mut dispatch = Dispatch::default();
                if !room_service.ensure_in_room(session_id, Routes::START as i32, &mut dispatch) {
                    return dispatch;
                }
                if !room_service.room_ready_to_start(session_id) {
                    return room_service.unsupported_response(session_id, Routes::START as i32);
                }

                if let Some(room_key) = room_service.room_key_of(session_id) {
                    let turn_timeout_secs = room_service
                        .get_room_settings_full(&room_key)
                        .and_then(|json| serde_json::from_value::<LandlordRoomSettings>(json).ok())
                        .unwrap_or_default()
                        .away_time.current as u32;

                    // Build loop state from the game state that's been tracking players since CREATE
                    let players = room_service.get_game_state_players(&room_key);
                    let loop_state = Arc::new(std::sync::Mutex::new(LandlordLoopState::new(players)));

                    // Create action channel
                    let (tx, rx) = mpsc::channel::<PlayerAction>(32);

                    // Store sender for this room
                    self.action_senders.insert(room_key.clone(), tx);

                    if let (Some(room_service_arc), Some(senders_arc)) =
                        (self.room_service.as_ref(), self.senders.as_ref())
                    {
                        start_game_loop(
                            room_key.clone(),
                            loop_state,
                            turn_timeout_secs,
                            rx,
                            Arc::clone(room_service_arc),
                            Arc::clone(senders_arc),
                        );
                    }

                    room_service.send_all(
                        session_id,
                        WsCode::START as i32,
                        serde_json::json!({}),
                        &mut dispatch,
                    );
                    room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
                    return dispatch;
                }

                room_service.unsupported_response(session_id, Routes::START as i32)
            }

            r if r == LandlordRoutes::CALL_LANDLORD as i32 => {
                let Some(pos) = room_service.session_position(session_id) else {
                    return room_service.unsupported_response(session_id, LandlordRoutes::CALL_LANDLORD as i32);
                };
                let Some(room_key) = room_service.room_key_of(session_id) else {
                    return room_service.unsupported_response(session_id, LandlordRoutes::CALL_LANDLORD as i32);
                };

                let score: u8 = request.data
                    .get("score")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u8)
                    .unwrap_or(0);

                if let Some(tx) = self.action_senders.get(&room_key) {
                    let _ = tx.try_send(PlayerAction::CallLandlord { position: pos, score });
                }

                let mut dispatch = Dispatch::default();
                room_service.push_ok_response(&mut dispatch, session_id, LandlordRoutes::CALL_LANDLORD as i32);
                dispatch
            }

            r if r == Routes::PLAY as i32 => {
                let Some(pos) = room_service.session_position(session_id) else {
                    return room_service.unsupported_response(session_id, Routes::PLAY as i32);
                };
                let Some(room_key) = room_service.room_key_of(session_id) else {
                    return room_service.unsupported_response(session_id, Routes::PLAY as i32);
                };

                let cards: Vec<i32> = request.data
                    .get("cards")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                if let Some(tx) = self.action_senders.get(&room_key) {
                    let _ = tx.try_send(PlayerAction::Play { position: pos, cards });
                }

                let mut dispatch = Dispatch::default();
                room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
                dispatch
            }

            r if r == Routes::AWAY as i32 => {
                let Some(pos) = room_service.session_position(session_id) else {
                    return room_service.unsupported_response(session_id, Routes::AWAY as i32);
                };
                let Some(room_key) = room_service.room_key_of(session_id) else {
                    return room_service.unsupported_response(session_id, Routes::AWAY as i32);
                };

                if let Some(tx) = self.action_senders.get(&room_key) {
                    let _ = tx.try_send(PlayerAction::Away { position: pos });
                }

                let mut dispatch = Dispatch::default();
                room_service.push_ok_response(&mut dispatch, session_id, Routes::AWAY as i32);
                dispatch
            }

            _ => room_service.unsupported_response(session_id, request.route),
        }
    }
}


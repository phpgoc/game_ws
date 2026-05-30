use std::collections::HashMap;
use std::sync::Arc;

use share_type_public::games::landlord::{
    WsCallLandlordEvent, WsCallLandlordRequest, WsPlayEvent,
};
use share_type_public::{
    GameSettings, LandlordRoutes, Routes, WsCode, WsResponseCode,
    games::landlord::LandlordRoomSettings,
};
use tokio::sync::Mutex;
use ws_common::{ClientRequest, Dispatch, GameHandler, RoomService, SessionId, SessionSenders};

use crate::game_loop::start_game_loop;
use crate::game_state::{LandlordGameState, LandlordLoopState};
use share_type_public::LandlordPhase;

use crate::play_validator::validate_play_request;

pub struct LandlordGameHandler {
    room_service: Option<Arc<Mutex<RoomService>>>,
    senders: Option<SessionSenders>,
    loop_states: HashMap<String, Arc<std::sync::Mutex<LandlordLoopState>>>,
}

impl Default for LandlordGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
            loop_states: HashMap::new(),
        }
    }
}

const MIN_PLAYERS: usize = 3;
const MAX_PLAYERS: usize = 3;

fn build_room_settings() -> Box<dyn ws_common::GameSettings> {
    Box::new(LandlordRoomSettings::default())
}

impl GameHandler for LandlordGameHandler {
    fn build_room_settings(&self) -> Box<dyn GameSettings> {
        build_room_settings()
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
                self.handle_start(room_service, session_id)
            }

            r if r == LandlordRoutes::CALL_LANDLORD as i32 => {
                self.handle_call_landlord(room_service, session_id, request.data)
            }

            r if r == Routes::PLAY as i32 => {
                self.handle_play(room_service, session_id, request.data)
            }

            _ => room_service.unsupported_response(session_id, request.route),
        }
    }
}

// ─── START ────────────────────────────────────────────────────────────
impl LandlordGameHandler {
    fn handle_start(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
    ) -> Dispatch {
        // Only the creator (position 0) may start
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.unsupported_response(session_id, Routes::START as i32);
        };
        if position != 0 {
            return room_service.permission_denied_response(session_id, Routes::START as i32);
        }

        let mut dispatch = Dispatch::default();
        if !room_service.ensure_in_room(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        if !room_service.room_ready_to_start(session_id) {
            return room_service.unsupported_response(session_id, Routes::START as i32);
        }

        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.unsupported_response(session_id, Routes::START as i32);
        };

        // Prevent re-starting if game loop is already running
        if self.loop_states.contains_key(&room_key) {
            return room_service.permission_denied_response(session_id, Routes::START as i32);
        }

        let players = room_service.get_game_state_players(&room_key);
        let loop_state = Arc::new(std::sync::Mutex::new(LandlordLoopState::new(players)));

        self.loop_states
            .insert(room_key.clone(), Arc::clone(&loop_state));

        if let (Some(room_service_arc), Some(senders_arc)) =
            (self.room_service.as_ref(), self.senders.as_ref())
        {
            start_game_loop(
                room_key.clone(),
                loop_state,
                Arc::clone(room_service_arc),
                Arc::clone(senders_arc),
            );
        }

        room_service.send_all(
            &room_key,
            WsCode::START as i32,
            serde_json::json!({}),
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
        dispatch
    }
}

// ─── CALL_LANDLORD ────────────────────────────────────────────────────
impl LandlordGameHandler {
    fn handle_call_landlord(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: serde_json::Value,
    ) -> Dispatch {
        let Some(pos) = room_service.session_position(session_id) else {
            return room_service.permission_denied_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
            );
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.permission_denied_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
            );
        };

        let Ok(payload) = RoomService::parse::<WsCallLandlordRequest>(data) else {
            return RoomService::error_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };

        let score: u8 = payload.score;

        let Some(loop_state) = self.loop_states.get(&room_key) else {
            return room_service.permission_denied_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
            );
        };

        let name;
        {
            let mut s = loop_state.lock().unwrap();
            if s.phase != LandlordPhase::CallLandlord {
                return room_service.permission_denied_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                );
            }
            if s.current_position != pos {
                return room_service.permission_denied_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                );
            }
            if score > 3 {
                return room_service.permission_denied_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                );
            }
            if score > 0 && score <= s.score as u8 {
                return room_service.permission_denied_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                );
            }

            // 0 = 不叫，>0 = 叫分
            if score > 0 {
                s.score = score as u32;
            }
            // Record the call in history for game loop's landlord determination
            s.call_history.push((pos, score));
            // 本轮叫分/不叫已收到
            s.base.action_received = true;
            name = s.base.player_name(pos);
        }

        // 广播叫分事件给所有人（含自己，方便前端统一处理）
        let mut dispatch = Dispatch::default();
        room_service.send_all(
            &room_key,
            WsCode::CALL_LANDLORD as i32,
            WsCallLandlordEvent {
                name,
                score,
            },
            &mut dispatch,
        );
        room_service.push_ok_response(
            &mut dispatch,
            session_id,
            LandlordRoutes::CALL_LANDLORD as i32,
        );
        dispatch
    }
}

// ─── PLAY ─────────────────────────────────────────────────────────────
impl LandlordGameHandler {
    fn handle_play(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: serde_json::Value,
    ) -> Dispatch {
        let Some(pos) = room_service.session_position(session_id) else {
            return room_service.unsupported_response(session_id, Routes::PLAY as i32);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.unsupported_response(session_id, Routes::PLAY as i32);
        };

        let cards: Vec<i32> = data
            .get("cards")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let Some(loop_state) = self.loop_states.get(&room_key) else {
            return room_service.permission_denied_response(session_id, Routes::PLAY as i32);
        };

        let name;
        {
            let mut s = loop_state.lock().unwrap();
            if !validate_play_request_inner(&s, pos, &cards) {
                return room_service.permission_denied_response(session_id, Routes::PLAY as i32);
            }

            // Record the play in the loop state
            s.current_play = cards.clone();
            s.base.action_received = true;
            name = s.base.player_name(pos);
        }

        let mut dispatch = Dispatch::default();
        room_service.send_all(
            &room_key,
            WsCode::PLAY as i32,
            WsPlayEvent { name, cards },
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
        dispatch
    }
}

/// Pure check shared between game handler and play_validator
fn validate_play_request_inner(
    s: &LandlordLoopState,
    position: usize,
    cards: &[i32],
) -> bool {
    if s.phase != LandlordPhase::Play || s.current_position != position {
        return false;
    }
    validate_play_request(s, position, cards)
}

// Cleanup hook for game loop to remove loop_state when a room ends
impl LandlordGameHandler {
    pub fn remove_loop_state(&mut self, room_key: &str) {
        self.loop_states.remove(room_key);
    }
}

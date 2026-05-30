use std::collections::HashMap;
use std::sync::Arc;

use share_type_public::games::landlord::WsCallLandlordRequest;
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

pub fn build_room_settings(_room_key: &str) -> Box<dyn ws_common::GameSettings> {
    Box::new(LandlordRoomSettings::default())
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
                        return room_service
                            .permission_denied_response(session_id, Routes::START as i32);
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
                    let room_settings = room_service
                        .get_room_settings_full(&room_key)
                        .and_then(|json| serde_json::from_value::<LandlordRoomSettings>(json).ok())
                        .unwrap_or_default();
                    let play_time_secs = room_settings.play_time.current as u32;
                    let away_time_secs = room_settings.away_time.current as u32;

                    let players = room_service.get_game_state_players(&room_key);
                    let loop_state =
                        Arc::new(std::sync::Mutex::new(LandlordLoopState::new(players)));

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

                let Ok(payload) = RoomService::parse::<WsCallLandlordRequest>(request.data) else {
                    return RoomService::error_response(
                        session_id,
                        Routes::CALL_LANDLORD as i32,
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
                    if score > 0 {
                        s.score = score as u32;
                    }
                }

                let mut dispatch = Dispatch::default();
                room_service.push_ok_response(
                    &mut dispatch,
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                );
                dispatch
            }

            r if r == Routes::PLAY as i32 => {
                let Some(pos) = room_service.session_position(session_id) else {
                    return room_service.unsupported_response(session_id, Routes::PLAY as i32);
                };
                let Some(room_key) = room_service.room_key_of(session_id) else {
                    return room_service.unsupported_response(session_id, Routes::PLAY as i32);
                };

                let cards: Vec<i32> = request
                    .data
                    .get("cards")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let Some(loop_state) = self.loop_states.get(&room_key) else {
                    return room_service
                        .permission_denied_response(session_id, Routes::PLAY as i32);
                };
                if !validate_play_request(loop_state, pos, &cards) {
                    return room_service
                        .permission_denied_response(session_id, Routes::PLAY as i32);
                }

                let mut dispatch = Dispatch::default();
                room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
                dispatch
            }

            _ => room_service.unsupported_response(session_id, request.route),
        }
    }
}

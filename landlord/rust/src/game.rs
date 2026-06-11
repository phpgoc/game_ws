use std::collections::HashMap;
use std::sync::Arc;

use share_type_public::games::landlord::{WsCallLandlordEvent, WsCallLandlordRequest, WsPlayEvent};
use share_type_public::{LandlordRoutes, Routes, WsCode, WsReJoinResponse, WsResponseCode};
use tokio::sync::Mutex;
use ws_common::{
    ClientRequest, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService, SessionId,
    SessionSenders,
};

use crate::game_loop::start_game_loop;
use crate::game_setting::build_landlord_settings;
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

impl GameHandler for LandlordGameHandler {
    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_landlord_settings()
    }

    fn build_game_state(&self) -> Box<dyn ws_common::game_state::GameState> {
        Box::new(LandlordGameState::new())
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
            r if r == Routes::START as i32 => self.handle_start(room_service, session_id),

            r if r == LandlordRoutes::CALL_LANDLORD as i32 => {
                self.handle_call_landlord(room_service, session_id, request.data)
            }

            r if r == Routes::PLAY as i32 => {
                self.handle_play(room_service, session_id, request.data)
            }

            _ => {
                room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
            }
        }
    }

    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        if request.route != Routes::JOIN as i32 || !join_succeeded(dispatch, session_id) {
            return;
        }

        let Some(room_key) = room_service.room_key_of(session_id) else {
            return;
        };
        let Some(loop_state) = self.loop_states.get(&room_key) else {
            return;
        };

        let current_position = room_service.session_position(session_id);
        let rejoin_data = {
            let state = loop_state.lock().unwrap();
            if !matches!(
                state.phase,
                LandlordPhase::CallLandlord | LandlordPhase::Play
            ) {
                None
            } else {
                let my_cards = current_position
                    .and_then(|position| state.hands.get(&position).cloned())
                    .unwrap_or_default();
                let other_cards_numbers = state
                    .hands
                    .iter()
                    .filter(|(position, _)| Some(**position) != current_position)
                    .map(|(position, cards)| (*position as i32, cards.len() as i32))
                    .collect();
                Some(WsReJoinResponse {
                    other_cards_numbers,
                    my_cards,
                    now_playing: state.current_position as i32,
                    phase: state.phase as i32,
                    landlord_position: state.landlord_position.map(|position| position as i32),
                    score: state.score,
                    hidden_cards: if state.phase == LandlordPhase::Play {
                        state.hidden_cards.clone()
                    } else {
                        Vec::new()
                    },
                    last_play_position: if state.last_play.is_empty() {
                        None
                    } else {
                        Some(state.last_play_position as i32)
                    },
                    last_play: state.last_play.clone(),
                })
            }
        };

        for message in dispatch.messages.iter_mut() {
            if message.recipient != session_id {
                continue;
            }
            let OutboundPayload::Response(RequestResponse::WithData(response)) =
                &mut message.payload
            else {
                continue;
            };
            if response.route == Routes::JOIN as i32
                && response.code as i32 == WsResponseCode::JOINED as i32
            {
                response.data["rejoin_data"] =
                    serde_json::to_value(&rejoin_data).unwrap_or(serde_json::Value::Null);
            }
        }
    }
}

fn join_succeeded(dispatch: &Dispatch, session_id: SessionId) -> bool {
    dispatch.messages.iter().any(|message| {
        if message.recipient != session_id {
            return false;
        }
        matches!(
            &message.payload,
            OutboundPayload::Response(RequestResponse::WithData(response))
                if response.route == Routes::JOIN as i32
                    && response.code as i32 == WsResponseCode::JOINED as i32
        )
    })
}

// ─── START ────────────────────────────────────────────────────────────
impl LandlordGameHandler {
    fn handle_start(&mut self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
        // Only the creator (position 0) may start
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        if position != 0 {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }

        let mut dispatch = Dispatch::default();
        if !room_service.ensure_in_room(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        if !room_service.room_ready_to_start(session_id) {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        };

        let Some(shared_common_state) = room_service.get_room_common_state_handle(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        // Prevent re-starting if the current room loop is already running.
        // If an old room with the same key left a stale loop_state, remove it.
        if let Some(existing) = self.loop_states.get(&room_key) {
            let same_state = {
                let s = existing.lock().unwrap();
                Arc::ptr_eq(&s.base, &shared_common_state)
            };
            if same_state {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            self.loop_states.remove(&room_key);
        }
        let loop_state = Arc::new(std::sync::Mutex::new(LandlordLoopState::new(
            shared_common_state,
        )));

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
            return room_service.error_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };

        let Ok(payload) = RoomService::parse::<WsCallLandlordRequest>(data) else {
            return room_service.error_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };

        let score: u8 = payload.score;

        let Some(loop_state) = self.loop_states.get(&room_key) else {
            return room_service.error_response(
                session_id,
                LandlordRoutes::CALL_LANDLORD as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let name;
        {
            let mut s = loop_state.lock().unwrap();
            if s.phase != LandlordPhase::CallLandlord {
                return room_service.error_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            if s.current_position != pos {
                return room_service.error_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            if score > 3 {
                return room_service.error_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            if score > 0 && score <= s.score as u8 {
                return room_service.error_response(
                    session_id,
                    LandlordRoutes::CALL_LANDLORD as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }

            // 0 = 不叫，>0 = 叫分
            if score > 0 {
                s.score = score as u32;
            }
            // Record the call in history for game loop's landlord determination
            s.call_history.push((pos, score));
            // 本轮叫分/不叫已收到
            s.set_action_received(true);
            name = s.player_name(pos);
            println!(
                "[landlord][call] room={} pos={} name={} score={} current_max={} history_len={}",
                room_key,
                pos,
                name,
                score,
                s.score,
                s.call_history.len()
            );
        }

        // 广播叫分事件给所有人（含自己，方便前端统一处理）
        let mut dispatch = Dispatch::default();
        room_service.send_all(
            &room_key,
            WsCode::CALL_LANDLORD as i32,
            WsCallLandlordEvent { name, score },
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
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };

        let cards: Vec<i32> = data
            .get("cards")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let Some(loop_state) = self.loop_states.get(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let name;
        {
            let mut s = loop_state.lock().unwrap();
            if !validate_play_request_inner(&s, pos, &cards) {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }

            // Record the play in the loop state
            s.current_play = cards.clone();
            s.set_action_received(true);
            name = s.player_name(pos);
            println!(
                "[landlord][play] room={} pos={} name={} cards={:?}",
                room_key, pos, name, cards
            );
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
fn validate_play_request_inner(s: &LandlordLoopState, position: usize, cards: &[i32]) -> bool {
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

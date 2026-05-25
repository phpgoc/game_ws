use std::sync::Arc;

use share_type_public::{
    Routes, WsCode, GameSettings,
    games::{GameParam, landlord::LandlordRoomSettings},
    ws::WsStartEvent,
};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use ws_common::{ClientRequest, Dispatch, GameHandler, RoomService, SessionId, SessionSenders};

pub struct LandlordGameHandler {
    room_service: Option<Arc<Mutex<RoomService>>>,
    senders: Option<SessionSenders>,
}

impl Default for LandlordGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
        }
    }
}

// Game constants
const MIN_PLAYERS: usize = 3;
const MAX_PLAYERS: usize = 3;

pub fn build_room_settings(_room_key: &str) -> Box<dyn ws_common::GameSettings> {
    let settings = LandlordRoomSettings {
        round_time: GameParam {
            current: 30,
            min: 20,
            max: 40,
        },
        away_time: GameParam {
            current: 5,
            min: 2,
            max: 5,
        },
        play_time: GameParam {
            current: 300,
            min: 100,
            max: 500,
        },
        deal_time: GameParam {
            current: 3000,
            min: 500,
            max: 4000,
        },
    };

    Box::new(settings)
}

/// Called every tick when the room is active. Returns false to stop the loop.
async fn on_game_tick(
    counter: u64,
    members: &[(SessionId, String, usize)],
    senders: &SessionSenders,
) {
    let payload = serde_json::json!({ "code": 999, "data": { "count": counter } });
    if let Ok(msg_str) = serde_json::to_string(&payload) {
        let frame = Message::text(msg_str);
        let senders = senders.lock().await;
        for (session_id, _, _) in members {
            if let Some(tx) = senders.get(session_id) {
                let _ = tx.send(frame.clone());
            }
        }
    }
}

/// Spawns the per-room game event loop. Stops when any player leaves.
fn start_game_loop(
    room_key: String,
    initial_member_count: usize,
    room_service: Arc<Mutex<RoomService>>,
    senders: SessionSenders,
) {
    tokio::spawn(async move {
        let mut counter = 0u64;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            interval.tick().await;

            let (members, is_paused) = {
                let room_svc = room_service.lock().await;
                let members = room_svc.get_room_members(&room_key);
                let is_paused = room_svc.is_room_paused(&room_key);
                (members, is_paused)
            };

            // Stop if room is empty or any player has left
            if members.is_empty() || members.len() < initial_member_count {
                break;
            }

            if is_paused {
                continue;
            }

            counter += 1;
            on_game_tick(counter, &members, &senders).await;
        }
    });
}

impl GameHandler for LandlordGameHandler {
    fn build_room_settings(&self, room_key: &str) -> Box<dyn GameSettings> {
        build_room_settings(room_key)
    }

    fn get_player_limits(&self) -> (usize, usize) {
        (MIN_PLAYERS, MAX_PLAYERS)
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
            Routes::START => {
                if let Some(position) = room_service.session_position(session_id) {
                    if position != 0 {
                        return room_service.permission_denied_response(session_id, Routes::START);
                    }
                } else {
                    return room_service.unsupported_response(session_id, Routes::START);
                }

                let mut dispatch = Dispatch::default();
                if !room_service.ensure_in_room(session_id, Routes::START, &mut dispatch) {
                    return dispatch;
                }
                if !room_service.room_ready_to_start(session_id) {
                    return room_service.unsupported_response(session_id, Routes::START);
                }

                if let Some(room_key) = room_service.room_key_of(session_id) {
                    let initial_member_count = room_service.get_room_members(&room_key).len();
                    if let (Some(room_service_arc), Some(senders_arc)) =
                        (self.room_service.as_ref(), self.senders.as_ref())
                    {
                        start_game_loop(
                            room_key,
                            initial_member_count,
                            Arc::clone(room_service_arc),
                            Arc::clone(senders_arc),
                        );
                    }
                }

                let actor = room_service.session_name(session_id);
                room_service.send_all(
                    session_id,
                    WsCode::START,
                    WsStartEvent { name: actor.clone() },
                    &mut dispatch,
                );
                room_service.push_ok_response(&mut dispatch, session_id, Routes::START);
                dispatch
            }
            Routes::SETTING => {
                if let Some(position) = room_service.session_position(session_id) {
                    if position != 0 {
                        return room_service.permission_denied_response(session_id, Routes::SETTING);
                    }
                } else {
                    return room_service.unsupported_response(session_id, Routes::SETTING);
                }

                match room_service.update_room_settings(session_id, &request.data) {
                    Ok(()) => {
                        let mut dispatch = Dispatch::default();
                        if let Some(current_settings) = room_service.get_room_settings_current(session_id) {
                            room_service.push_ok_response(&mut dispatch, session_id, Routes::SETTING);
                            let player_name = room_service.session_name(session_id);
                            let _ = room_service.send_other(
                                session_id,
                                WsCode::SETTING,
                                serde_json::json!({"name": player_name, "settings": current_settings}),
                                &mut dispatch,
                            );
                        } else {
                            return room_service.unsupported_response(session_id, Routes::SETTING);
                        }
                        dispatch
                    }
                    Err(_) => room_service.error_response(
                        session_id,
                        Routes::SETTING,
                        share_type_public::WsResponseCode::ERROR_FORMAT,
                    ),
                }
            }
            Routes::DISBAND => {
                if let Some(position) = room_service.session_position(session_id) {
                    if position != 0 {
                        return room_service.permission_denied_response(session_id, Routes::DISBAND);
                    }
                } else {
                    return room_service.unsupported_response(session_id, Routes::DISBAND);
                }
                room_service.unsupported_response(session_id, Routes::DISBAND)
            }
            _ => room_service.unsupported_response(session_id, request.route),
        }
    }
}


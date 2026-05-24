use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, Routes, WsCode, WsCreateRequest, WsJoinRequest, WsMessageRequest, WsRequest,
    WsResponse,
    ws::{WsDisbandEvent, WsMessageEvent, WsPauseEvent, WsQuitEvent},
};

pub type SessionId = u64;
pub type ClientRequest = WsRequest<Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestResponse {
    pub code: Routes,
    pub response: WsResponse,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OutboundPayload {
    Response(RequestResponse),
    Event(CommonEvent<Value>),
}

#[derive(Debug, Clone, Serialize)]
pub struct Delivery {
    pub recipient: SessionId,
    pub payload: OutboundPayload,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Dispatch {
    pub messages: Vec<Delivery>,
}

#[derive(Debug, Default)]
pub struct RoomService {
    sessions: HashMap<SessionId, SessionState>,
    rooms: HashMap<String, RoomState>,
}

#[derive(Debug, Default)]
struct SessionState {
    name: Option<String>,
    room_key: Option<String>,
}

#[derive(Debug, Default)]
struct RoomState {
    members: HashSet<SessionId>,
    settings: Value,
    _min_players: usize,
    max_players: usize,
}

impl RoomService {
    pub fn connect(&mut self, session_id: SessionId) {
        self.sessions.entry(session_id).or_default();
    }

    pub fn disconnect(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        let Some(mut session) = self.sessions.remove(&session_id) else {
            return dispatch;
        };
        self.remove_from_current_room(session_id, &mut session, &mut dispatch, WsCode::QUIT);
        dispatch
    }

    pub fn handle_common_request<F>(
        &mut self,
        session_id: SessionId,
        request: &ClientRequest,
        room_settings_builder: F,
    ) -> Option<Dispatch>
    where
        F: Fn(&str) -> Value,
    {
        self.sessions.entry(session_id).or_default();
        match request.route {
            Routes::CREATE => {
                let Ok(payload) = Self::parse::<WsCreateRequest>(request.data.clone()) else {
                    return Some(self.error_response(session_id, Routes::CREATE, WsResponse::ERROR_FORMAT));
                };
                let settings = room_settings_builder(&payload.password);
                Some(self.enter_room(
                    session_id,
                    Routes::CREATE,
                    payload.name,
                    payload.password,
                    settings,
                ))
            }
            Routes::JOIN => {
                let Ok(payload) = Self::parse::<WsJoinRequest>(request.data.clone()) else {
                    return Some(self.error_response(session_id, Routes::JOIN, WsResponse::ERROR_FORMAT));
                };
                let settings = room_settings_builder(&payload.password);
                Some(self.enter_room(
                    session_id,
                    Routes::JOIN,
                    payload.name,
                    payload.password,
                    settings,
                ))
            }
            Routes::QUIT => Some(self.handle_quit_request(session_id)),
            Routes::DISBAND => Some(self.handle_disband_request(session_id)),
            Routes::MESSAGE => Some(self.handle_message_request(session_id, request.data.clone())),
            Routes::PAUSE => Some(self.handle_pause_request(session_id)),
            _ => None,
        }
    }

    pub fn unsupported_response(&self, session_id: SessionId, route: Routes) -> Dispatch {
        self.error_response(session_id, route, WsResponse::NOT_IN_RANGE)
    }

    pub fn format_error_response(&self, session_id: SessionId, route: Routes) -> Dispatch {
        self.error_response(session_id, route, WsResponse::ERROR_FORMAT)
    }

    fn enter_room(
        &mut self,
        session_id: SessionId,
        route: Routes,
        name: String,
        room_key: String,
        settings: Value,
    ) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if room_key.is_empty() {
            dispatch.messages.push(Self::direct_response(
                session_id,
                route,
                WsResponse::ERROR_FORMAT,
            ));
            return dispatch;
        }

        let old_room = self.sessions.get(&session_id).and_then(|item| item.room_key.clone());
        if old_room.as_ref() != Some(&room_key) {
            let mut tmp = self.sessions.remove(&session_id).unwrap_or_default();
            self.remove_from_current_room(session_id, &mut tmp, &mut dispatch, WsCode::QUIT);
            self.sessions.insert(session_id, tmp);
        }

        let room_settings = if let Some(room) = self.rooms.get_mut(&room_key) {
            if room.members.len() >= room.max_players && !room.members.contains(&session_id) {
                return self.error_response(session_id, route, WsResponse::NO_PERMISSION);
            }
            room.members.insert(session_id);
            room.settings.clone()
        } else {
            let Some((min_players, max_players)) = Self::extract_player_limits(&settings) else {
                return self.error_response(session_id, route, WsResponse::ERROR_FORMAT);
            };
            if max_players == 0 || min_players == 0 || min_players > max_players {
                return self.error_response(session_id, route, WsResponse::ERROR_FORMAT);
            }

            let mut room = RoomState {
                members: HashSet::new(),
                settings: settings.clone(),
                _min_players: min_players,
                max_players,
            };
            room.members.insert(session_id);
            self.rooms.insert(room_key.clone(), room);
            settings
        };

        {
            let session = self.sessions.entry(session_id).or_default();
            session.name = Some(name.clone());
            session.room_key = Some(room_key);
        }

        self.broadcast_room_event(
            session_id,
            WsCode::JOIN,
            json!({"name": name, "settings": room_settings}),
            &mut dispatch,
        );
        dispatch
            .messages
            .push(Self::direct_response(session_id, route, WsResponse::OK));
        dispatch
    }

    fn handle_quit_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::QUIT, &mut dispatch) {
            return dispatch;
        }
        self.quit_room(session_id, &mut dispatch, WsCode::QUIT);
        dispatch.messages.push(Self::direct_response(
            session_id,
            Routes::QUIT,
            WsResponse::OK,
        ));
        dispatch
    }

    fn handle_disband_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::DISBAND, &mut dispatch) {
            return dispatch;
        }
        self.disband_room(session_id, &mut dispatch);
        dispatch.messages.push(Self::direct_response(
            session_id,
            Routes::DISBAND,
            WsResponse::OK,
        ));
        dispatch
    }

    fn handle_message_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::MESSAGE, &mut dispatch) {
            return dispatch;
        }
        let Ok(payload) = Self::parse::<WsMessageRequest>(data) else {
            return self.error_response(session_id, Routes::MESSAGE, WsResponse::ERROR_FORMAT);
        };
        self.broadcast_room_event(
            session_id,
            WsCode::MESSAGE,
            WsMessageEvent {
                name: self.session_name(session_id),
                message: payload.message,
            },
            &mut dispatch,
        );
        dispatch.messages.push(Self::direct_response(
            session_id,
            Routes::MESSAGE,
            WsResponse::OK,
        ));
        dispatch
    }

    fn handle_pause_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::PAUSE, &mut dispatch) {
            return dispatch;
        }
        self.broadcast_room_event(
            session_id,
            WsCode::PAUSE,
            WsPauseEvent {
                name: self.session_name(session_id),
            },
            &mut dispatch,
        );
        dispatch.messages.push(Self::direct_response(
            session_id,
            Routes::PAUSE,
            WsResponse::OK,
        ));
        dispatch
    }

    fn parse<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn extract_player_limits(settings: &Value) -> Option<(usize, usize)> {
        #[derive(Deserialize)]
        struct Limits {
            min_players: usize,
            max_players: usize,
        }
        let parsed: Limits = serde_json::from_value(settings.clone()).ok()?;
        Some((parsed.min_players, parsed.max_players))
    }

    fn require_login(&self, session_id: SessionId, route: Routes, dispatch: &mut Dispatch) -> bool {
        let logged_in = self
            .sessions
            .get(&session_id)
            .and_then(|item| item.room_key.as_ref())
            .is_some();
        if !logged_in {
            dispatch.messages.push(Self::direct_response(
                session_id,
                route,
                WsResponse::NOT_LOGIN,
            ));
        }
        logged_in
    }

    fn session_name(&self, session_id: SessionId) -> String {
        self.sessions
            .get(&session_id)
            .and_then(|item| item.name.as_ref())
            .cloned()
            .unwrap_or_default()
    }

    fn quit_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch, code: WsCode) {
        let Some(mut session) = self.sessions.remove(&session_id) else {
            return;
        };
        self.remove_from_current_room(session_id, &mut session, dispatch, code);
        self.sessions.insert(session_id, session);
    }

    fn disband_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch) {
        let Some(room_key) = self
            .sessions
            .get(&session_id)
            .and_then(|item| item.room_key.clone())
        else {
            return;
        };
        let Some(room) = self.rooms.remove(&room_key) else {
            return;
        };

        let actor = self.session_name(session_id);
        let event = CommonEvent {
            code: WsCode::DISBAND,
            data: serde_json::to_value(WsDisbandEvent { name: actor }).unwrap_or(Value::Null),
        };

        for member in room.members {
            if let Some(session) = self.sessions.get_mut(&member) {
                session.room_key = None;
            }
            dispatch.messages.push(Delivery {
                recipient: member,
                payload: OutboundPayload::Event(event.clone()),
            });
        }
    }

    fn remove_from_current_room(
        &mut self,
        session_id: SessionId,
        session: &mut SessionState,
        dispatch: &mut Dispatch,
        code: WsCode,
    ) {
        let Some(room_key) = session.room_key.take() else {
            return;
        };
        let Some(name) = session.name.clone() else {
            return;
        };

        let mut recipients = Vec::new();
        if let Some(room) = self.rooms.get_mut(&room_key) {
            room.members.remove(&session_id);
            recipients.extend(room.members.iter().copied());
            if room.members.is_empty() {
                self.rooms.remove(&room_key);
            }
        }

        let event = match code {
            WsCode::QUIT => CommonEvent {
                code,
                data: serde_json::to_value(WsQuitEvent { name }).unwrap_or(Value::Null),
            },
            _ => return,
        };

        for recipient in recipients {
            dispatch.messages.push(Delivery {
                recipient,
                payload: OutboundPayload::Event(event.clone()),
            });
        }
    }

    fn broadcast_room_event<T: serde::Serialize>(
        &self,
        session_id: SessionId,
        code: WsCode,
        event: T,
        dispatch: &mut Dispatch,
    ) {
        let Some(room_key) = self
            .sessions
            .get(&session_id)
            .and_then(|item| item.room_key.as_ref())
        else {
            return;
        };
        let Some(room) = self.rooms.get(room_key) else {
            return;
        };

        let payload = OutboundPayload::Event(CommonEvent {
            code,
            data: serde_json::to_value(event).unwrap_or(Value::Null),
        });
        for member in &room.members {
            dispatch.messages.push(Delivery {
                recipient: *member,
                payload: payload.clone(),
            });
        }
    }

    fn error_response(&self, session_id: SessionId, route: Routes, code: WsResponse) -> Dispatch {
        Dispatch {
            messages: vec![Self::direct_response(session_id, route, code)],
        }
    }

    fn direct_response(recipient: SessionId, code: Routes, response: WsResponse) -> Delivery {
        Delivery {
            recipient,
            payload: OutboundPayload::Response(RequestResponse { code, response }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use share_type_public::WsCode;

    use super::{Dispatch, OutboundPayload, RoomService};
    use share_type_public::{Routes, WsRequest, WsResponse};

    fn recipients_of(code: WsCode, dispatch: &Dispatch) -> HashSet<u64> {
        dispatch
            .messages
            .iter()
            .filter_map(|item| match &item.payload {
                OutboundPayload::Event(event) if event.code as i32 == code as i32 => Some(item.recipient),
                _ => None,
            })
            .collect()
    }

    fn settings(room_key: &str) -> serde_json::Value {
        serde_json::json!({ "name": room_key, "min_players": 3, "max_players": 3 })
    }

    #[test]
    fn common_routes_are_room_scoped() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);

        let create1 = WsRequest {
            code: Routes::CREATE,
            data: serde_json::json!({"name":"u1","password":"p1"}),
        };
        let create2 = WsRequest {
            code: Routes::CREATE,
            data: serde_json::json!({"name":"u3","password":"p2"}),
        };
        let _ = service.handle_common_request(1, &create1, settings);
        let _ = service.handle_common_request(3, &create2, settings);
        let join = WsRequest {
            code: Routes::JOIN,
            data: serde_json::json!({"name":"u2","password":"p1"}),
        };
        let _ = service.handle_common_request(2, &join, settings);

        let message = WsRequest {
            code: Routes::MESSAGE,
            data: serde_json::json!({"message":"hi"}),
        };
        let dispatch = service
            .handle_common_request(1, &message, settings)
            .expect("message should be common route");
        assert_eq!(
            recipients_of(WsCode::MESSAGE, &dispatch),
            [1_u64, 2_u64].into_iter().collect()
        );

        let pause = WsRequest {
            code: Routes::PAUSE,
            data: serde_json::json!({}),
        };
        let pause_dispatch = service
            .handle_common_request(1, &pause, settings)
            .expect("pause should be common route");
        assert_eq!(
            recipients_of(WsCode::PAUSE, &pause_dispatch),
            [1_u64, 2_u64].into_iter().collect()
        );
    }

    #[test]
    fn join_respects_max_players_limit() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);
        service.connect(4);

        let create = WsRequest {
            code: Routes::CREATE,
            data: serde_json::json!({"name":"u1","password":"p1"}),
        };
        let _ = service.handle_common_request(1, &create, settings);

        for (sid, name) in [(2_u64, "u2"), (3_u64, "u3")] {
            let join = WsRequest {
                code: Routes::JOIN,
                data: serde_json::json!({"name":name,"password":"p1"}),
            };
            let _ = service.handle_common_request(sid, &join, settings);
        }

        let join4 = WsRequest {
            code: Routes::JOIN,
            data: serde_json::json!({"name":"u4","password":"p1"}),
        };
        let dispatch = service
            .handle_common_request(4, &join4, settings)
            .expect("join should be common route");
        let denied = dispatch.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(resp) => {
                item.recipient == 4
                    && resp.code as i32 == Routes::JOIN as i32
                    && resp.response as i32 == WsResponse::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(denied);
    }
}

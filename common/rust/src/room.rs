use std::collections::HashMap;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, Routes, WsCode, WsCreateRequest, WsJoinRequest, WsMessageRequest, WsRequest,
    WsResponseCode, WsWithoutDataResponse, GameSettings,
    ws::WsResponse,
    ws::{WsDisbandEvent, WsMessageEvent, WsPauseEvent, WsQuitEvent, WsResumeEvent},
};

pub type SessionId = u64;
pub type ClientRequest = WsRequest<Value>;

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RequestResponse {
    WithoutData(WsWithoutDataResponse),
    WithData(WsResponse<Value>),
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
    position: Option<usize>,
}

struct RoomState {
    slots: HashMap<usize, SessionId>,
    settings: Box<dyn GameSettings>,
    min_players: usize,
    max_players: usize,
    paused: bool,
    game: Option<Box<dyn crate::game_state::GameState>>,
}

impl std::fmt::Debug for RoomState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomState")
            .field("slots", &self.slots)
            .field("min_players", &self.min_players)
            .field("max_players", &self.max_players)
            .field("paused", &self.paused)
            .field("game", &self.game.as_ref().map(|_| "<GameState>"))
            .finish()
    }
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
        self.remove_from_current_room(session_id, &mut session, &mut dispatch, WsCode::QUIT as i32);
        dispatch
    }

    pub fn handle_common_request<F>(
        &mut self,
        session_id: SessionId,
        request: &ClientRequest,
        room_settings_builder: F,
        get_player_limits: impl Fn() -> (usize, usize),
    ) -> Option<Dispatch>
    where
        F: Fn(&str) -> Box<dyn GameSettings>,
    {
        self.sessions.entry(session_id).or_default();
        match request.route {
            r if r == Routes::CREATE as i32 => Some(self.handle_create_request(session_id, request.data.clone(), room_settings_builder, get_player_limits)),
            r if r == Routes::JOIN as i32 => Some(self.handle_join_request(session_id, request.data.clone(), room_settings_builder, get_player_limits)),
            r if r == Routes::QUIT as i32 => Some(self.handle_quit_request(session_id)),
            r if r == Routes::DISBAND as i32 => Some(self.handle_disband_request(session_id)),
            r if r == Routes::SETTING as i32 => Some(self.handle_setting_request(session_id, &request.data)),
            r if r == Routes::MESSAGE as i32 => Some(self.handle_message_request(session_id, request.data.clone())),
            r if r == Routes::PAUSE as i32 => Some(self.handle_pause_request(session_id)),
            r if r == Routes::RESUME as i32 => Some(self.handle_resume_request(session_id)),
            _ => None,
        }
    }

    pub fn unsupported_response(&self, session_id: SessionId, route: i32) -> Dispatch {
        self.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE)
    }

    pub fn format_error_response(&self, session_id: SessionId, route: i32) -> Dispatch {
        self.error_response(session_id, route, WsResponseCode::ERROR_FORMAT)
    }

    pub fn permission_denied_response(&self, session_id: SessionId, route: i32) -> Dispatch {
        self.error_response(session_id, route, WsResponseCode::NO_PERMISSION)
    }

    pub fn ensure_in_room(&self, session_id: SessionId, route: i32, dispatch: &mut Dispatch) -> bool {
        self.require_login(session_id, route, dispatch)
    }

    pub fn push_ok_response(&self, dispatch: &mut Dispatch, session_id: SessionId, route: i32) {
        dispatch
            .messages
            .push(Self::direct_response(session_id, route, WsResponseCode::OK));
    }

    pub fn room_ready_to_start(&self, session_id: SessionId) -> bool {
        let Some(room_key) = self.room_key_of(session_id) else {
            return false;
        };
        let Some(room) = self.rooms.get(&room_key) else {
            return false;
        };
        room.slots.len() >= room.min_players
    }

    pub fn is_room_paused(&self, room_key: &str) -> bool {
        self.rooms.get(room_key).map(|r| r.paused).unwrap_or(true)
    }

    pub fn get_room_members(&self, room_key: &str) -> Vec<(SessionId, String, usize)> {
        self.rooms
            .get(room_key)
            .map(|room| {
                room.slots
                    .iter()
                    .filter_map(|(pos, session_id)| {
                        self.sessions
                            .get(session_id)
                            .and_then(|s| s.name.as_ref())
                            .map(|name| (*session_id, name.clone(), *pos))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn room_key_of(&self, session_id: SessionId) -> Option<String> {
        self.sessions
            .get(&session_id)
            .and_then(|item| item.room_key.as_ref())
            .cloned()
    }

    pub fn session_name(&self, session_id: SessionId) -> String {
        self.sessions
            .get(&session_id)
            .and_then(|item| item.name.as_ref())
            .cloned()
            .unwrap_or_default()
    }

    pub fn session_position(&self, session_id: SessionId) -> Option<usize> {
        self.sessions
            .get(&session_id)
            .and_then(|item| item.position)
    }

    pub fn get_room_settings_current(&self, session_id: SessionId) -> Option<Value> {
        let room_key = self.room_key_of(session_id)?;
        let room = self.rooms.get(&room_key)?;
        Some(room.settings.to_current_json())
    }

    pub fn get_room_settings_full(&self, room_key: &str) -> Option<Value> {
        let room = self.rooms.get(room_key)?;
        Some(room.settings.to_full_json())
    }

    pub fn set_room_game_state(&mut self, room_key: &str, game: Box<dyn crate::game_state::GameState>) {
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.game = Some(game);
        }
    }

    pub fn clear_room_game_state(&mut self, room_key: &str) {
        if let Some(room) = self.rooms.get_mut(room_key) {
            room.game = None;
        }
    }

    /// Returns a snapshot of players tracked in the room's game state.
    pub fn get_game_state_players(&self, room_key: &str) -> std::collections::HashMap<usize, (SessionId, String)> {
        self.rooms.get(room_key)
            .and_then(|r| r.game.as_ref())
            .map(|g| g.players().clone())
            .unwrap_or_default()
    }

    pub fn update_room_settings(&mut self, session_id: SessionId, data: &Value) -> Result<(), String> {
        let room_key = self.room_key_of(session_id)
            .ok_or_else(|| "Not in any room".to_string())?;
        let room = self.rooms.get_mut(&room_key)
            .ok_or_else(|| "Room not found".to_string())?;
        room.settings.update_from_json(data)?;
        Ok(())
    }

    pub fn send_all<T: serde::Serialize>(
        &self,
        actor_session_id: SessionId,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) -> bool {
        self.broadcast_room_event(actor_session_id, code, payload, true, dispatch)
    }

    pub fn send_other<T: serde::Serialize>(
        &self,
        actor_session_id: SessionId,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) -> bool {
        self.broadcast_room_event(actor_session_id, code, payload, false, dispatch)
    }

    pub fn send_one_by_name<T: serde::Serialize>(
        &self,
        actor_session_id: SessionId,
        name: &str,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) -> bool {
        let Some(room_key) = self.room_key_of(actor_session_id) else {
            return false;
        };
        let Some(room) = self.rooms.get(&room_key) else {
            return false;
        };
        let Some(target_session_id) = room
            .slots
            .values()
            .find(|session_id| {
                self.sessions
                    .get(session_id)
                    .and_then(|item| item.name.as_deref())
                    == Some(name)
            })
            .copied()
        else {
            return false;
        };

        self.emit_to_recipient(target_session_id, code, payload, dispatch)
    }

    pub fn send_one_by_position<T: serde::Serialize>(
        &self,
        actor_session_id: SessionId,
        position: usize,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) -> bool {
        let Some(room_key) = self.room_key_of(actor_session_id) else {
            return false;
        };
        let Some(room) = self.rooms.get(&room_key) else {
            return false;
        };
        let Some(target_session_id) = room.slots.get(&position).copied() else {
            return false;
        };
        self.emit_to_recipient(target_session_id, code, payload, dispatch)
    }

    fn handle_create_request<F>(
        &mut self,
        session_id: SessionId,
        data: Value,
        room_settings_builder: F,
        get_player_limits: impl Fn() -> (usize, usize),
    ) -> Dispatch
    where
        F: Fn(&str) -> Box<dyn GameSettings>,
    {
        if self.room_key_of(session_id).is_some() {
            return self.error_response(session_id, Routes::CREATE as i32, WsResponseCode::NO_PERMISSION);
        }
        let Ok(payload) = Self::parse::<WsCreateRequest>(data) else {
            return self.error_response(session_id, Routes::CREATE as i32, WsResponseCode::ERROR_FORMAT);
        };
        if self.rooms.contains_key(&payload.password) {
            return self.error_response(session_id, Routes::CREATE as i32, WsResponseCode::NO_PERMISSION);
        }
        let settings = room_settings_builder(&payload.password);
        self.enter_room(
            session_id,
            Routes::CREATE,
            payload.name,
            payload.password,
            settings,
            get_player_limits,
        )
    }

    fn handle_join_request<F>(
        &mut self,
        session_id: SessionId,
        data: Value,
        room_settings_builder: F,
        get_player_limits: impl Fn() -> (usize, usize),
    ) -> Dispatch
    where
        F: Fn(&str) -> Box<dyn GameSettings>,
    {
        if self.room_key_of(session_id).is_some() {
            return self.error_response(session_id, Routes::JOIN as i32, WsResponseCode::NO_PERMISSION);
        }
        let Ok(payload) = Self::parse::<WsJoinRequest>(data) else {
            return self.error_response(session_id, Routes::JOIN as i32, WsResponseCode::ERROR_FORMAT);
        };
        if !self.rooms.contains_key(&payload.password) {
            return self.error_response(session_id, Routes::JOIN as i32, WsResponseCode::NO_PERMISSION);
        }
        let settings = room_settings_builder(&payload.password);
        self.enter_room(
            session_id,
            Routes::JOIN,
            payload.name,
            payload.password,
            settings,
            get_player_limits,
        )
    }

    fn enter_room(
        &mut self,
        session_id: SessionId,
        route: Routes,
        name: String,
        room_key: String,
        settings: Box<dyn GameSettings>,
        get_player_limits: impl Fn() -> (usize, usize),
    ) -> Dispatch {
        if room_key.is_empty() || name.is_empty() {
            return self.error_response(session_id, route as i32, WsResponseCode::ERROR_FORMAT);
        }

        // Validate everything BEFORE any state mutation or event dispatch.
        let (room_settings, position, min_players, max_players) = if let Some(room) = self.rooms.get(&room_key) {
            if self.name_taken_in_room(&room_key, &name, Some(session_id)) {
                return self.error_response(session_id, route as i32, WsResponseCode::NO_PERMISSION);
            }
            let Some(position) = self.select_position(room, session_id) else {
                return self.error_response(session_id, route as i32, WsResponseCode::NO_PERMISSION);
            };
            (room.settings.clone(), position, room.min_players, room.max_players)
        } else {
            let (min_players, max_players) = get_player_limits();
            if max_players == 0 || min_players == 0 || min_players > max_players {
                return self.error_response(session_id, route as i32, WsResponseCode::ERROR_FORMAT);
            }
            (settings.clone(), 0, min_players, max_players)
        };

        // All checks passed — now mutate state and build dispatch.
        let mut dispatch = Dispatch::default();
        let old_room = self.sessions.get(&session_id).and_then(|item| item.room_key.clone());
        if old_room.as_ref() != Some(&room_key) {
            let mut tmp = self.sessions.remove(&session_id).unwrap_or_default();
            self.remove_from_current_room(session_id, &mut tmp, &mut dispatch, WsCode::QUIT as i32);
            self.sessions.insert(session_id, tmp);
        }

        let room = self.rooms.entry(room_key.clone()).or_insert_with(|| RoomState {
            slots: HashMap::new(),
            settings: room_settings.clone(),
            min_players,
            max_players,
            paused: false,
            game: None,
        });
        room.slots.insert(position, session_id);

        // Hook: notify game state of new player
        if let Some(game) = room.game.as_mut() {
            game.add_player(position, session_id, &name);
        }

        {
            let session = self.sessions.entry(session_id).or_default();
            session.name = Some(name.clone());
            session.room_key = Some(room_key);
            session.position = Some(position);
        }

        self.send_all(
            session_id,
            WsCode::JOIN as i32,
            json!({"name": name, "position": position as i32}),
            &mut dispatch,
        );
        if route as i32 == Routes::CREATE as i32 {
            // CREATE response includes full settings (with min/max/current)
            dispatch.messages.push(Self::direct_response_with_data(
                session_id,
                route as i32,
                WsResponseCode::OK,
                room_settings.to_full_json(),
            ));
        } else if route as i32 == Routes::JOIN as i32 {
            // JOIN response only includes current values
            dispatch.messages.push(Self::direct_response_with_data(
                session_id,
                route as i32,
                WsResponseCode::JOINED,
                room_settings.to_current_json(),
            ));
        } else {
            self.push_ok_response(&mut dispatch, session_id, route as i32);
        }
        dispatch
    }

    fn handle_quit_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::QUIT as i32, &mut dispatch) {
            return dispatch;
        }
        self.quit_room(session_id, &mut dispatch, WsCode::QUIT as i32);
        self.push_ok_response(&mut dispatch, session_id, Routes::QUIT as i32);
        dispatch
    }

    fn handle_disband_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::DISBAND as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.permission_denied_response(session_id, Routes::DISBAND as i32);
        }
        self.disband_room(session_id, &mut dispatch);
        self.push_ok_response(&mut dispatch, session_id, Routes::DISBAND as i32);
        dispatch
    }

    fn handle_setting_request(&mut self, session_id: SessionId, data: &Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::SETTING as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.permission_denied_response(session_id, Routes::SETTING as i32);
        }
        match self.update_room_settings(session_id, data) {
            Ok(()) => {
                let Some(current_settings) = self.get_room_settings_current(session_id) else {
                    return self.error_response(session_id, Routes::SETTING as i32, WsResponseCode::NOT_LOGIN);
                };
                self.push_ok_response(&mut dispatch, session_id, Routes::SETTING as i32);
                let player_name = self.session_name(session_id);
                self.send_other(
                    session_id,
                    WsCode::SETTING as i32,
                    json!({"name": player_name, "settings": current_settings}),
                    &mut dispatch,
                );
                dispatch
            }
            Err(_) => self.error_response(session_id, Routes::SETTING as i32, WsResponseCode::ERROR_FORMAT),
        }
    }

    fn handle_message_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::MESSAGE as i32, &mut dispatch) {
            return dispatch;
        }
        let Ok(payload) = Self::parse::<WsMessageRequest>(data) else {
            return self.error_response(session_id, Routes::MESSAGE as i32, WsResponseCode::ERROR_FORMAT);
        };
        self.send_other(
            session_id,
            WsCode::MESSAGE as i32,
            WsMessageEvent {
                name: self.session_name(session_id),
                message: payload.message,
            },
            &mut dispatch,
        );
        self.push_ok_response(&mut dispatch, session_id, Routes::MESSAGE as i32);
        dispatch
    }

    fn handle_pause_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::PAUSE as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(session_id, Routes::PAUSE as i32, WsResponseCode::NOT_LOGIN);
        };
        {
            let Some(room) = self.rooms.get_mut(&room_key) else {
                return self.error_response(session_id, Routes::PAUSE as i32, WsResponseCode::NOT_LOGIN);
            };
            if room.paused {
                return self.error_response(session_id, Routes::PAUSE as i32, WsResponseCode::NO_PERMISSION);
            }
            room.paused = true;
        }
        self.send_other(
            session_id,
            WsCode::PAUSE as i32,
            WsPauseEvent {
                name: self.session_name(session_id),
            },
            &mut dispatch,
        );
        self.push_ok_response(&mut dispatch, session_id, Routes::PAUSE as i32);
        dispatch
    }

    fn handle_resume_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::RESUME as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(session_id, Routes::RESUME as i32, WsResponseCode::NOT_LOGIN);
        };
        {
            let Some(room) = self.rooms.get_mut(&room_key) else {
                return self.error_response(session_id, Routes::RESUME as i32, WsResponseCode::NOT_LOGIN);
            };
            if !room.paused {
                return self.error_response(session_id, Routes::RESUME as i32, WsResponseCode::NO_PERMISSION);
            }
            room.paused = false;
        }
        self.send_other(
            session_id,
            WsCode::RESUME as i32,
            WsResumeEvent {
                name: self.session_name(session_id),
            },
            &mut dispatch,
        );
        self.push_ok_response(&mut dispatch, session_id, Routes::RESUME as i32);
        dispatch
    }

    fn parse<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn require_login(&self, session_id: SessionId, route: i32, dispatch: &mut Dispatch) -> bool {
        let logged_in = self
            .sessions
            .get(&session_id)
            .and_then(|item| item.room_key.as_ref())
            .is_some();
        if !logged_in {
            dispatch.messages.push(Self::direct_response(
                session_id,
                route,
                WsResponseCode::NOT_LOGIN,
            ));
        }
        logged_in
    }

    fn name_taken_in_room(
        &self,
        room_key: &str,
        name: &str,
        exclude_session_id: Option<SessionId>,
    ) -> bool {
        let Some(room) = self.rooms.get(room_key) else {
            return false;
        };
        room.slots.values().any(|member| {
            if exclude_session_id == Some(*member) {
                return false;
            }
            self.sessions
                .get(member)
                .and_then(|item| item.name.as_deref())
                == Some(name)
        })
    }

    fn select_position(&self, room: &RoomState, session_id: SessionId) -> Option<usize> {
        if let Some(existing) = room
            .slots
            .iter()
            .find_map(|(pos, sid)| if *sid == session_id { Some(*pos) } else { None })
        {
            return Some(existing);
        }
        (0..room.max_players).find(|pos| !room.slots.contains_key(pos))
    }

    fn quit_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch, code: i32) {
        let Some(mut session) = self.sessions.remove(&session_id) else {
            return;
        };
        self.remove_from_current_room(session_id, &mut session, dispatch, code);
        self.sessions.insert(session_id, session);
    }

    fn disband_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch) {
        let Some(room_key) = self.room_key_of(session_id) else {
            return;
        };
        let Some(room) = self.rooms.remove(&room_key) else {
            return;
        };

        let actor = self.session_name(session_id);
        let event = CommonEvent {
            code: WsCode::DISBAND as i32,
            data: serde_json::to_value(WsDisbandEvent { name: actor }).unwrap_or(Value::Null),
        };

        for member in room.slots.values() {
            if let Some(session) = self.sessions.get_mut(member) {
                session.room_key = None;
                session.position = None;
            }
            if *member == session_id {
                continue;
            }
            dispatch.messages.push(Delivery {
                recipient: *member,
                payload: OutboundPayload::Event(event.clone()),
            });
        }
    }

    fn remove_from_current_room(
        &mut self,
        _session_id: SessionId,
        session: &mut SessionState,
        dispatch: &mut Dispatch,
        code: i32,
    ) {
        let Some(room_key) = session.room_key.take() else {
            return;
        };
        let Some(name) = session.name.clone() else {
            return;
        };
        let Some(position) = session.position.take() else {
            return;
        };

        let mut recipients = Vec::new();
        if let Some(room) = self.rooms.get_mut(&room_key) {
            room.slots.remove(&position);
            // Hook: notify game state of player removal
            if let Some(game) = room.game.as_mut() {
                game.remove_player(position);
            }
            recipients.extend(room.slots.values().copied());
            if room.slots.is_empty() {
                self.rooms.remove(&room_key);
            }
        }

        let event = if code == WsCode::QUIT as i32 {
            CommonEvent {
                code,
                data: serde_json::to_value(WsQuitEvent { name }).unwrap_or(Value::Null),
            }
        } else {
            return;
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
        actor_session_id: SessionId,
        code: i32,
        payload: T,
        include_self: bool,
        dispatch: &mut Dispatch,
    ) -> bool {
        let Some(room_key) = self.room_key_of(actor_session_id) else {
            return false;
        };
        let Some(room) = self.rooms.get(&room_key) else {
            return false;
        };
        let data = serde_json::to_value(payload).unwrap_or(Value::Null);
        for recipient in room.slots.values() {
            if !include_self && *recipient == actor_session_id {
                continue;
            }
            dispatch.messages.push(Delivery {
                recipient: *recipient,
                payload: OutboundPayload::Event(CommonEvent { code, data: data.clone() }),
            });
        }
        true
    }

    fn emit_to_recipient<T: serde::Serialize>(
        &self,
        recipient: SessionId,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) -> bool {
        let data = serde_json::to_value(payload).unwrap_or(Value::Null);
        dispatch.messages.push(Delivery {
            recipient,
            payload: OutboundPayload::Event(CommonEvent { code, data }),
        });
        true
    }

    pub fn error_response(&self, session_id: SessionId, route: i32, code: WsResponseCode) -> Dispatch {
        Dispatch {
            messages: vec![Self::direct_response(session_id, route, code)],
        }
    }

    fn direct_response(recipient: SessionId, _route: i32, code: WsResponseCode) -> Delivery {
        Delivery {
            recipient,
            payload: OutboundPayload::Response(RequestResponse::WithoutData(WsWithoutDataResponse {
                code,
            })),
        }
    }

    pub fn direct_response_with_data(
        recipient: SessionId,
        _route: i32,
        code: WsResponseCode,
        data: Value,
    ) -> Delivery {
        Delivery {
            recipient,
            payload: OutboundPayload::Response(RequestResponse::WithData(WsResponse { code, data })),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use share_type_public::{Routes, WsCode, WsRequest, WsResponseCode};

    use super::{Dispatch, OutboundPayload, RequestResponse, RoomService};

    fn recipients_of(code: i32, dispatch: &Dispatch) -> HashSet<u64> {
        dispatch
            .messages
            .iter()
            .filter_map(|item| match &item.payload {
                OutboundPayload::Event(event) if event.code == code => Some(item.recipient),
                _ => None,
            })
            .collect()
    }

    #[derive(Debug, Clone)]
    struct TestSettings;

    impl share_type_public::GameSettings for TestSettings {
        fn to_full_json(&self) -> serde_json::Value {
            serde_json::json!({"min_players": 3, "max_players": 3})
        }
        fn to_current_json(&self) -> serde_json::Value {
            serde_json::json!({"min_players": 3, "max_players": 3})
        }
        fn update_from_json(&mut self, _data: &serde_json::Value) -> Result<(), String> {
            Ok(())
        }
        fn clone_box(&self) -> Box<dyn share_type_public::GameSettings> {
            Box::new(self.clone())
        }
    }

    fn settings(_room_key: &str) -> Box<dyn share_type_public::GameSettings> {
        Box::new(TestSettings)
    }

    #[test]
    fn message_pause_resume_go_to_other_only() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let join_dispatch = service
            .handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
            || (3, 3),
        )
            .expect("join common");
        let join_response_has_settings = join_dispatch.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                item.recipient == 2
                    && resp.code as i32 == WsResponseCode::JOINED as i32
                    && resp.data.get("min_players").is_some()
                    && resp.data.get("name").is_none()
            }
            _ => false,
        });
        assert!(join_response_has_settings);
        let join_event_has_no_settings = join_dispatch.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
                event.data.get("settings").is_none() && event.data.get("position").is_some()
            }
            _ => false,
        });
        assert!(join_event_has_no_settings);
        let _ = service.handle_common_request(
            3,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u3","password":"p2"}),
            },
            settings,
            || (3, 3),
        );

        let message = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::MESSAGE as i32,
                    data: serde_json::json!({"message":"hi"}),
                },
                settings,
                || (3, 3),
            )
            .expect("message common");
        assert_eq!(recipients_of(WsCode::MESSAGE as i32, &message), [2_u64].into_iter().collect());

        let pause = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::PAUSE as i32,
                    data: serde_json::json!({}),
                },
                settings,
                || (3, 3),
            )
            .expect("pause common");
        assert_eq!(recipients_of(WsCode::PAUSE as i32, &pause), [2_u64].into_iter().collect());

        let resume = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::RESUME as i32,
                    data: serde_json::json!({}),
                },
                settings,
                || (3, 3),
            )
            .expect("resume common");
        assert_eq!(recipients_of(WsCode::RESUME as i32, &resume), [2_u64].into_iter().collect());
    }

    #[test]
    fn join_rejects_duplicate_name_and_overflow() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);
        service.connect(4);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
            || (3, 3),
        );

        let duplicate = service
            .handle_common_request(
                2,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u1","password":"p1"}),
                },
                settings,
                || (3, 3),
            )
            .expect("join common");
        let duplicate_denied = duplicate.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                item.recipient == 2 && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(duplicate_denied);

        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let _ = service.handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let overflow = service
            .handle_common_request(
                4,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u4","password":"p1"}),
                },
                settings,
                || (3, 3),
            )
            .expect("join common");
        let overflow_denied = overflow.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                item.recipient == 4 && resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(overflow_denied);
    }

    #[test]
    fn join_and_create_reject_when_already_in_room() {
        let mut service = RoomService::default();
        service.connect(1);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
            || (3, 3),
        );

        let rejoin = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u1","password":"p1"}),
                },
                settings,
                || (3, 3),
            )
            .expect("join common");
        let rejoin_denied = rejoin.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(rejoin_denied);

        let recreate = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::CREATE as i32,
                    data: serde_json::json!({"name":"u1","password":"p2"}),
                },
                settings,
                || (3, 3),
            )
            .expect("create common");
        let recreate_denied = recreate.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(recreate_denied);
    }

    #[test]
    fn position_hole_reused_after_quit() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);
        service.connect(4);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let _ = service.handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p1"}),
            },
            settings,
            || (3, 3),
        );

        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::QUIT as i32,
                data: serde_json::json!({}),
            },
            settings,
            || (3, 3),
        );

        let join4 = service
            .handle_common_request(
                4,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u4","password":"p1"}),
                },
                settings,
                || (3, 3),
            )
            .expect("join common");

        let reused = join4.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
                event.data.get("position").and_then(|v| v.as_i64()) == Some(1)
            }
            _ => false,
        });
        assert!(reused);
    }

    #[test]
    fn pause_resume_must_follow_state() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
            || (3, 3),
        );

        let resume_before_pause = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::RESUME as i32,
                    data: serde_json::json!({}),
                },
                settings,
                || (3, 3),
            )
            .expect("resume common");
        let resume_denied = resume_before_pause.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(resume_denied);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::PAUSE as i32,
                data: serde_json::json!({}),
            },
            settings,
            || (3, 3),
        );
        let pause_again = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::PAUSE as i32,
                    data: serde_json::json!({}),
                },
                settings,
                || (3, 3),
            )
            .expect("pause common");
        let pause_denied = pause_again.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(pause_denied);
    }

    #[test]
    fn disband_requires_new_create_before_join() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::CREATE as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
            || (3, 3),
        );
        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::DISBAND as i32,
                data: serde_json::json!({}),
            },
            settings,
            || (3, 3),
        );

        let join_after_disband = service
            .handle_common_request(
                3,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u3","password":"p1"}),
                },
                settings,
                || (3, 3),
            )
            .expect("join common");
        let denied = join_after_disband.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
            }
            _ => false,
        });
        assert!(denied);

        let recreate = service
            .handle_common_request(
                3,
                &WsRequest {
                    route: Routes::CREATE as i32,
                    data: serde_json::json!({"name":"u3","password":"p1"}),
                },
                settings,
                || (3, 3),
            )
            .expect("create common");
        let recreated_ok = recreate.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                resp.code as i32 == WsResponseCode::OK as i32
            }
            _ => false,
        });
        assert!(recreated_ok);
    }
}

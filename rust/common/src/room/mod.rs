//! Room membership, settings, and common request handling.

mod model;

use std::{collections::HashMap, sync::Arc};

use crate::official::OfficialPlayerSession;
use serde::de::DeserializeOwned;
use serde_json::Value;
use share_type_public::{
    CommonEvent, GameId, GameParam, Routes, WsAddAiRequest, WsCode, WsJoinRequest,
    WsMessageRequest, WsPositionEvent, WsRemoveAiRequest, WsResponseCode, WsSwapPositionPayload,
    WsWithoutDataResponse,
    ws::WsResponse,
    ws::{WsMessageEvent, WsNameEvent},
};

pub use model::{
    ClientRequest, Delivery, Dispatch, OutboundPayload, RequestResponse, SessionId,
    SettingsBuilderResult,
};

const AI_SESSION_ID_BASE: SessionId = 9_000_000_000_000_000_000;
const MAX_ROOMS: usize = 1_000;
const MAX_ROOM_KEY_BYTES: usize = 128;
const MAX_PLAYER_NAME_BYTES: usize = 64;
const MAX_AVATAR_URL_BYTES: usize = 1_024;
const MAX_OFFICIAL_SESSION_ID_BYTES: usize = 512;
const MAX_CHAT_MESSAGE_BYTES: usize = 4 * 1024;

/// 一个房间，由 password（room_key）标识。
/// `configs` — 可配置参数的当前值（HashMap<String, i32>）。
/// `param_descriptions` — 参数描述（GameParam），创建时由游戏提供。
/// `state` — 游戏状态，始终存在（首个 JOIN 时创建），玩家列表在 CommonGameState.players 里。
struct RoomEntry {
    game_id: GameId,
    configs: HashMap<String, i32>,
    param_descriptions: HashMap<String, GameParam>,
    min_players: usize,
    max_players: usize,
    state: Box<dyn crate::game_state::GameState>,
    official_match_id: Option<i64>,
    official_user_ids_by_position: HashMap<usize, i64>,
}

#[derive(Debug, Default)]
pub struct RoomService {
    sessions: HashMap<SessionId, SessionState>,
    rooms: HashMap<String, RoomEntry>,
    next_ai_sequence: u64,
    ai_players_enabled: bool,
}

#[derive(Debug, Default)]
struct SessionState {
    name: Option<String>,
    room_key: Option<String>,
    position: Option<usize>,
    official_session_id: Option<String>,
}

fn config_value(configs: &HashMap<String, i32>, key: &str, default: i32) -> i32 {
    configs.get(key).copied().unwrap_or(default)
}

impl std::fmt::Debug for RoomEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomEntry")
            .field("game_id", &self.game_id)
            .field("configs", &self.configs)
            .field("param_descriptions", &self.param_descriptions.len())
            .field("min_players", &self.min_players)
            .field("max_players", &self.max_players)
            .field("official_match_id", &self.official_match_id)
            .field(
                "official_user_ids_by_position",
                &self.official_user_ids_by_position,
            )
            .field("state", &format_args!("<GameState>"))
            .finish()
    }
}

impl RoomService {
    pub fn with_ai_players_enabled(ai_players_enabled: bool) -> Self {
        Self {
            ai_players_enabled,
            ..Self::default()
        }
    }

    fn allocate_ai_session_id(&mut self) -> SessionId {
        loop {
            self.next_ai_sequence = self.next_ai_sequence.saturating_add(1);
            let candidate = AI_SESSION_ID_BASE.saturating_add(self.next_ai_sequence);
            let in_sessions = self.sessions.contains_key(&candidate);
            let in_players = self.rooms.values().any(|entry| {
                entry
                    .state
                    .players()
                    .values()
                    .any(|(sid, _)| *sid == candidate)
            });
            if !in_sessions && !in_players {
                return candidate;
            }
        }
    }

    pub fn broadcast<T: serde::Serialize>(
        &self,
        room_key: &str,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) {
        let Some(entry) = self.rooms.get(room_key) else {
            return;
        };
        let data = serde_json::to_value(payload).unwrap_or(Value::Null);
        for (_, (sid, _)) in entry.state.players() {
            dispatch.messages.push(Delivery {
                recipient: sid,
                payload: OutboundPayload::Event(CommonEvent {
                    code,
                    data: data.clone(),
                }),
            });
        }
    }

    /// Broadcast to currently connected sessions in a room without reading game state.
    ///
    /// This is useful inside game request handlers: the runtime already holds the
    /// `RoomService` lock, while some game loops may hold game-state locks and then
    /// wait for `RoomService`. Reading `entry.state.players()` here can invert that
    /// lock order and deadlock.
    pub fn broadcast_connected<T: serde::Serialize>(
        &self,
        room_key: &str,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) {
        let data = serde_json::to_value(payload).unwrap_or(Value::Null);
        for sid in self.connected_session_ids(room_key) {
            dispatch.messages.push(Delivery {
                recipient: sid,
                payload: OutboundPayload::Event(CommonEvent {
                    code,
                    data: data.clone(),
                }),
            });
        }
    }

    pub fn broadcast_except<T: serde::Serialize>(
        &self,
        room_key: &str,
        exclude: SessionId,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) {
        let Some(entry) = self.rooms.get(room_key) else {
            return;
        };
        let data = serde_json::to_value(payload).unwrap_or(Value::Null);
        for (_, (sid, _)) in entry.state.players() {
            if sid == exclude {
                continue;
            }
            dispatch.messages.push(Delivery {
                recipient: sid,
                payload: OutboundPayload::Event(CommonEvent {
                    code,
                    data: data.clone(),
                }),
            });
        }
    }

    /// 清除 game state（游戏结束时调用）。
    pub fn clear_room_game_state(&mut self, room_key: &str) {
        if let Some(entry) = self.rooms.get_mut(room_key) {
            let common = entry.state.shared_common_state();
            entry.state = Box::new(crate::game_state::SharedGameState::from_common(common));
            entry.official_match_id = None;
            entry.official_user_ids_by_position.clear();
        }
    }

    /// 清除 game state，但只在当前房间仍然是同一个 common state 时执行。
    /// 避免旧 loop 退出时误清理同名新房间的状态。
    pub fn clear_room_game_state_if_same(
        &mut self,
        room_key: &str,
        common: &Arc<std::sync::Mutex<crate::game_state::CommonGameState>>,
    ) {
        if let Some(entry) = self.rooms.get_mut(room_key) {
            let current = entry.state.shared_common_state();
            if Arc::ptr_eq(&current, common) {
                entry.state = Box::new(crate::game_state::SharedGameState::from_common(current));
                entry.official_match_id = None;
                entry.official_user_ids_by_position.clear();
            }
        }
    }

    pub fn connect(&mut self, session_id: SessionId) {
        self.sessions.entry(session_id).or_default();
    }

    pub fn connected_session_ids(&self, room_key: &str) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter_map(|(sid, session)| {
                (session.room_key.as_deref() == Some(room_key) && session.position.is_some())
                    .then_some(*sid)
            })
            .collect()
    }

    pub fn connected_session_ids_for_position(
        &self,
        room_key: &str,
        position: usize,
    ) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter_map(|(sid, session)| {
                (session.room_key.as_deref() == Some(room_key)
                    && session.position == Some(position))
                .then_some(*sid)
            })
            .collect()
    }

    fn direct_response(recipient: SessionId, route: i32, code: WsResponseCode) -> Delivery {
        Delivery {
            recipient,
            payload: OutboundPayload::Response(RequestResponse::WithoutData(
                WsWithoutDataResponse { route, code },
            )),
        }
    }

    fn disband_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch) {
        let Some(room_key) = self.room_key_of(session_id) else {
            return;
        };
        dlog!(
            tracing::Level::WARN,
            "Session {} disbands room '{}'",
            session_id,
            room_key
        );
        if let Some(entry) = self.rooms.get_mut(&room_key) {
            entry.state.request_stop();
        }
        let Some(entry) = self.rooms.remove(&room_key) else {
            return;
        };

        let actor = self.session_name(session_id);
        let event = CommonEvent {
            code: WsCode::DISBAND as i32,
            data: serde_json::to_value(WsNameEvent { name: actor }).unwrap_or(Value::Null),
        };

        // 从 state 获取所有成员
        for (sid, _) in entry.state.players().values() {
            if let Some(session) = self.sessions.get_mut(sid) {
                session.room_key = None;
                session.position = None;
                session.official_session_id = None;
            }
            if *sid == session_id {
                continue;
            }
            dispatch.messages.push(Delivery {
                recipient: *sid,
                payload: OutboundPayload::Event(event.clone()),
            });
        }
    }

    pub fn disconnect(&mut self, session_id: SessionId) -> Dispatch {
        self.disconnect_with_ai_takeover(session_id, false)
    }

    pub fn disconnect_with_ai_takeover(
        &mut self,
        session_id: SessionId,
        ai_takeover_authorized: bool,
    ) -> Dispatch {
        let mut dispatch = Dispatch::default();
        let Some(mut session) = self.sessions.remove(&session_id) else {
            return dispatch;
        };
        self.mark_disconnected(
            session_id,
            &mut session,
            ai_takeover_authorized,
            &mut dispatch,
        );
        dispatch
    }

    pub fn error_response(
        &self,
        session_id: SessionId,
        route: i32,
        code: WsResponseCode,
    ) -> Dispatch {
        Dispatch {
            messages: vec![Self::direct_response(session_id, route, code)],
        }
    }

    fn handle_add_ai_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        if !self.ai_players_enabled {
            return self.error_response(
                session_id,
                Routes::ADD_AI as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::ADD_AI as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.error_response(
                session_id,
                Routes::ADD_AI as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::ADD_AI as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Ok(payload) = Self::parse_payload::<WsAddAiRequest>(data) else {
            return self.error_response(
                session_id,
                Routes::ADD_AI as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let requested_count = if payload.count <= 0 {
            1
        } else {
            payload.count.min(8) as usize
        };

        let (game_id, max_players, can_accept_players, existing_players, existing_ai_count) = {
            let Some(entry) = self.rooms.get(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::ADD_AI as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            (
                entry.game_id,
                entry.max_players,
                entry.state.can_accept_players(),
                entry.state.players(),
                (0..entry.max_players)
                    .filter(|position| entry.state.is_ai_position(*position))
                    .count(),
            )
        };
        if !can_accept_players || existing_players.len() >= max_players {
            return self.error_response(
                session_id,
                Routes::ADD_AI as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }

        let mut added = 0usize;
        for position in 0..max_players {
            if added >= requested_count {
                break;
            }
            let occupied = self
                .rooms
                .get(&room_key)
                .map(|entry| entry.state.players().contains_key(&position))
                .unwrap_or(true);
            if occupied {
                continue;
            }

            let ai_session_id = self.allocate_ai_session_id();
            let ai_name = self.next_ai_name(&room_key, game_id, existing_ai_count + added + 1);
            {
                let Some(entry) = self.rooms.get_mut(&room_key) else {
                    break;
                };
                entry.state.add_player(position, ai_session_id, &ai_name);
                entry.state.mark_ai_position(position);
            }
            self.broadcast(
                &room_key,
                WsCode::JOIN as i32,
                share_type_public::WsMemberInfo {
                    name: ai_name,
                    avatar_url: String::new(),
                    position: position as i32,
                    is_active: true,
                    is_ai: true,
                    away: false,
                    is_ai_takeover: false,
                },
                &mut dispatch,
            );
            added += 1;
        }

        if added == 0 {
            return self.error_response(
                session_id,
                Routes::ADD_AI as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        self.push_ok_response(&mut dispatch, session_id, Routes::ADD_AI as i32);
        dispatch
    }

    fn handle_away_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::AWAY as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(session_id, Routes::AWAY as i32, WsResponseCode::NOT_LOGIN);
        };
        let Some(position) = self.session_position(session_id) else {
            return self.error_response(session_id, Routes::AWAY as i32, WsResponseCode::NOT_LOGIN);
        };
        {
            let Some(entry) = self.rooms.get_mut(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::AWAY as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            if entry.state.is_away(position) {
                return self.error_response(
                    session_id,
                    Routes::AWAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            entry.state.mark_away(position);
        }
        self.broadcast(
            &room_key,
            WsCode::AWAY as i32,
            WsPositionEvent {
                position: position as i32,
                is_ai_takeover: false,
            },
            &mut dispatch,
        );
        dispatch
    }

    fn handle_remove_ai_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        if !self.ai_players_enabled {
            return self.error_response(
                session_id,
                Routes::REMOVE_AI as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::REMOVE_AI as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.error_response(
                session_id,
                Routes::REMOVE_AI as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::REMOVE_AI as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Ok(payload) = Self::parse_payload::<WsRemoveAiRequest>(data) else {
            return self.error_response(
                session_id,
                Routes::REMOVE_AI as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Ok(position) = usize::try_from(payload.position) else {
            return self.error_response(
                session_id,
                Routes::REMOVE_AI as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        };

        let ai_name = {
            let Some(entry) = self.rooms.get(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::REMOVE_AI as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            if !entry.state.can_accept_players()
                || position >= entry.max_players
                || !entry.state.is_ai_position(position)
            {
                return self.error_response(
                    session_id,
                    Routes::REMOVE_AI as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let Some((_, name)) = entry.state.players().get(&position).cloned() else {
                return self.error_response(
                    session_id,
                    Routes::REMOVE_AI as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            name
        };

        if let Some(entry) = self.rooms.get_mut(&room_key) {
            entry.state.remove_player(position);
        }
        self.broadcast(
            &room_key,
            WsCode::QUIT as i32,
            WsNameEvent { name: ai_name },
            &mut dispatch,
        );
        self.push_ok_response(&mut dispatch, session_id, Routes::REMOVE_AI as i32);
        dispatch
    }

    fn handle_back_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::BACK as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(session_id, Routes::BACK as i32, WsResponseCode::NOT_LOGIN);
        };
        let Some(position) = self.session_position(session_id) else {
            return self.error_response(session_id, Routes::BACK as i32, WsResponseCode::NOT_LOGIN);
        };
        {
            let Some(entry) = self.rooms.get_mut(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::BACK as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            if !entry.state.is_away(position) {
                return self.error_response(
                    session_id,
                    Routes::BACK as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            entry.state.clear_away_position(position);
        }
        self.broadcast(
            &room_key,
            WsCode::BACK as i32,
            WsPositionEvent {
                position: position as i32,
                is_ai_takeover: false,
            },
            &mut dispatch,
        );
        dispatch
    }

    pub fn handle_common_request<F>(
        &mut self,
        session_id: SessionId,
        request: &ClientRequest,
        game_id: GameId,
        room_settings_builder: F,
    ) -> Option<Dispatch>
    where
        F: Fn() -> SettingsBuilderResult,
    {
        self.handle_common_request_with_game_acceptance(
            session_id,
            request,
            |requested| requested == game_id,
            room_settings_builder,
        )
    }

    pub fn handle_common_request_with_game_acceptance<F, A>(
        &mut self,
        session_id: SessionId,
        request: &ClientRequest,
        accepts_game_id: A,
        room_settings_builder: F,
    ) -> Option<Dispatch>
    where
        F: Fn() -> SettingsBuilderResult,
        A: Fn(GameId) -> bool,
    {
        self.sessions.entry(session_id).or_default();
        match request.route {
            r if r == Routes::JOIN as i32 => Some(self.handle_join_request(
                session_id,
                request.data.clone(),
                accepts_game_id,
                room_settings_builder,
            )),
            r if r == Routes::QUIT as i32 => Some(self.handle_quit_request(session_id)),
            r if r == Routes::DISBAND as i32 => Some(self.handle_disband_request(session_id)),
            r if r == Routes::SETTING as i32 => {
                Some(self.handle_setting_request(session_id, &request.data))
            }
            r if r == Routes::MESSAGE as i32 => {
                Some(self.handle_message_request(session_id, request.data.clone()))
            }
            r if r == Routes::PAUSE as i32 => Some(self.handle_pause_request(session_id)),
            r if r == Routes::RESUME as i32 => Some(self.handle_resume_request(session_id)),
            r if r == Routes::AWAY as i32 => Some(self.handle_away_request(session_id)),
            r if r == Routes::BACK as i32 => Some(self.handle_back_request(session_id)),
            r if r == Routes::ADD_AI as i32 => {
                Some(self.handle_add_ai_request(session_id, request.data.clone()))
            }
            r if r == Routes::REMOVE_AI as i32 => {
                Some(self.handle_remove_ai_request(session_id, request.data.clone()))
            }
            r if r == Routes::SWAP as i32 => {
                Some(self.handle_swap_request(session_id, request.data.clone()))
            }
            _ => None,
        }
    }

    fn handle_disband_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::DISBAND as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.error_response(
                session_id,
                Routes::DISBAND as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        self.disband_room(session_id, &mut dispatch);
        self.push_ok_response(&mut dispatch, session_id, Routes::DISBAND as i32);
        dispatch
    }

    fn handle_join_request<F>(
        &mut self,
        session_id: SessionId,
        data: Value,
        accepts_game_id: impl Fn(GameId) -> bool,
        room_settings_builder: F,
    ) -> Dispatch
    where
        F: Fn() -> SettingsBuilderResult,
    {
        let Ok(payload) = Self::parse_payload::<WsJoinRequest>(data) else {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let password = payload.password;
        let name = payload.name;
        let official_session_id = (!payload.session_id.is_empty()).then_some(payload.session_id);
        let avatar_url = payload.avatar_url;
        if !accepts_game_id(payload.game_id) {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::WRONG_GAME,
            );
        }
        if password.is_empty()
            || name.is_empty()
            || password.len() > MAX_ROOM_KEY_BYTES
            || name.len() > MAX_PLAYER_NAME_BYTES
            || avatar_url.len() > MAX_AVATAR_URL_BYTES
            || official_session_id
                .as_ref()
                .is_some_and(|value| value.len() > MAX_OFFICIAL_SESSION_ID_BYTES)
        {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        }
        if let Some(current_room) = self.room_key_of(session_id) {
            let current_name = self.session_name(session_id);
            if current_room == password && current_name == name {
                let mut dispatch = Dispatch::default();
                let Some(position) = self.session_position(session_id) else {
                    return self.error_response(
                        session_id,
                        Routes::JOIN as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                };
                if let Some(entry) = self.rooms.get(&password) {
                    let existing_members: Vec<share_type_public::WsMemberInfo> = entry
                        .state
                        .players()
                        .iter()
                        .filter(|(p, _)| **p != position)
                        .map(|(p, (_, n))| share_type_public::WsMemberInfo {
                            name: n.clone(),
                            avatar_url: entry.state.player_avatar(*p),
                            position: *p as i32,
                            is_active: entry.state.is_ai_position(*p)
                                || !entry.state.is_disconnected(*p),
                            is_ai: entry.state.is_ai_position(*p),
                            away: entry.state.is_away(*p) || entry.state.is_disconnected(*p),
                            is_ai_takeover: entry.state.is_ai_takeover_position(*p),
                        })
                        .collect();
                    self.push_response_with_data(
                        session_id,
                        Routes::JOIN as i32,
                        WsResponseCode::JOINED,
                        share_type_public::WsJoinResponse {
                            self_position: position as i32,
                            current_configs: entry.configs.clone(),
                            existing_members,
                            param_descriptions: Some(entry.param_descriptions.clone()),
                            rejoin_data: None,
                        },
                        &mut dispatch,
                    );
                    return dispatch;
                }
            }
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }

        if !self.rooms.contains_key(&password) {
            if self.rooms.len() >= MAX_ROOMS {
                return self.error_response(
                    session_id,
                    Routes::JOIN as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let (settings, param_descriptions) = room_settings_builder();
            self.rooms.insert(
                password.clone(),
                RoomEntry {
                    game_id: payload.game_id,
                    configs: settings.values,
                    param_descriptions,
                    min_players: settings.min_players,
                    max_players: settings.max_players,
                    state: Box::new(crate::game_state::SharedGameState::new()),
                    official_match_id: None,
                    official_user_ids_by_position: HashMap::new(),
                },
            );
        } else if self
            .rooms
            .get(&password)
            .map(|entry| entry.game_id != payload.game_id)
            .unwrap_or(false)
        {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::WRONG_GAME,
            );
        }

        let mut dispatch = Dispatch::default();

        if let Some((position, existing_session_id)) = self.player_by_name(&password, &name) {
            if self
                .rooms
                .get(&password)
                .map(|entry| entry.state.is_ai_position(position))
                .unwrap_or(false)
            {
                return self.error_response(
                    session_id,
                    Routes::JOIN as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            if self.session_active_in_room(existing_session_id, &password) {
                return self.error_response(
                    session_id,
                    Routes::JOIN as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }

            {
                let entry = self.rooms.get_mut(&password).unwrap();
                entry.state.add_player(position, session_id, &name);
                entry.state.set_avatar(position, &avatar_url);
                entry.state.clear_disconnected_position(position);
            }
            {
                let session = self.sessions.entry(session_id).or_default();
                session.name = Some(name.clone());
                session.room_key = Some(password.clone());
                session.position = Some(position);
                session.official_session_id = official_session_id.clone();
            }

            self.broadcast_except(
                &password,
                session_id,
                WsCode::JOIN as i32,
                share_type_public::WsMemberInfo {
                    name: name.clone(),
                    avatar_url: avatar_url.clone(),
                    position: position as i32,
                    is_active: true,
                    is_ai: false,
                    away: false,
                    is_ai_takeover: false,
                },
                &mut dispatch,
            );

            let entry = self.rooms.get(&password).unwrap();
            let existing_members: Vec<share_type_public::WsMemberInfo> = entry
                .state
                .players()
                .iter()
                .filter(|(p, _)| **p != position)
                .map(|(p, (_, n))| share_type_public::WsMemberInfo {
                    name: n.clone(),
                    avatar_url: entry.state.player_avatar(*p),
                    position: *p as i32,
                    is_active: entry.state.is_ai_position(*p) || !entry.state.is_disconnected(*p),
                    is_ai: entry.state.is_ai_position(*p),
                    away: entry.state.is_away(*p) || entry.state.is_disconnected(*p),
                    is_ai_takeover: entry.state.is_ai_takeover_position(*p),
                })
                .collect();
            self.push_response_with_data(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::JOINED,
                share_type_public::WsJoinResponse {
                    self_position: position as i32,
                    current_configs: entry.configs.clone(),
                    existing_members,
                    param_descriptions: Some(entry.param_descriptions.clone()),
                    rejoin_data: None,
                },
                &mut dispatch,
            );
            return dispatch;
        }

        // — 离开旧房间 —
        let old_room = self
            .sessions
            .get(&session_id)
            .and_then(|item| item.room_key.clone());
        if old_room.as_ref() != Some(&password) {
            let mut tmp = self.sessions.remove(&session_id).unwrap_or_default();
            self.remove_from_current_room(session_id, &mut tmp, &mut dispatch);
            self.sessions.insert(session_id, tmp);
        }

        // — 检查名字唯一性 & 空位 —
        let max_players = self
            .rooms
            .get(&password)
            .map(|e| e.max_players)
            .unwrap_or(2);
        let can_join_new_position = self
            .rooms
            .get(&password)
            .map(|entry| entry.state.can_join_players())
            .unwrap_or(false);
        let has_disconnected_position = self.rooms.get(&password).is_some_and(|entry| {
            (0..max_players).any(|position| entry.state.is_disconnected(position))
        });
        if !can_join_new_position && !has_disconnected_position {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        if self.name_taken_in_room(&password, &name, Some(session_id)) {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Some(position) =
            self.select_position(&password, max_players, session_id, can_join_new_position)
        else {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        // — 加入 —
        let name_for_event = name.clone();
        {
            let entry = self.rooms.get_mut(&password).unwrap();
            if entry.state.is_disconnected(position) {
                // A different name replaces the disconnected room member at
                // this position. Removing the old roster entry first also
                // clears its avatar and away state without touching the
                // game-specific, position-keyed hand state.
                entry.state.remove_player(position);
            }
            entry.state.add_player(position, session_id, &name);
            entry.state.set_avatar(position, &avatar_url);
        }

        {
            let session = self.sessions.entry(session_id).or_default();
            session.name = Some(name.clone());
            session.room_key = Some(password.clone());
            session.position = Some(position);
            session.official_session_id = official_session_id;
        }

        // — 广播 JOIN 事件给其他人 —
        self.broadcast_except(
            &password,
            session_id,
            WsCode::JOIN as i32,
            share_type_public::WsMemberInfo {
                name: name_for_event,
                avatar_url: avatar_url.clone(),
                position: position as i32,
                is_active: true,
                is_ai: false,
                away: false,
                is_ai_takeover: false,
            },
            &mut dispatch,
        );

        // — JOIN 响应（含 current_configs + existing_members） —
        {
            let entry = self.rooms.get(&password).unwrap();
            let existing_members: Vec<share_type_public::WsMemberInfo> = entry
                .state
                .players()
                .iter()
                .filter(|(p, _)| **p != position)
                .map(|(p, (_, n))| share_type_public::WsMemberInfo {
                    name: n.clone(),
                    avatar_url: entry.state.player_avatar(*p),
                    position: *p as i32,
                    is_active: entry.state.is_ai_position(*p) || !entry.state.is_disconnected(*p),
                    is_ai: entry.state.is_ai_position(*p),
                    away: entry.state.is_away(*p) || entry.state.is_disconnected(*p),
                    is_ai_takeover: entry.state.is_ai_takeover_position(*p),
                })
                .collect();
            self.push_response_with_data(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::JOINED,
                share_type_public::WsJoinResponse {
                    self_position: position as i32,
                    current_configs: entry.configs.clone(),
                    existing_members,
                    param_descriptions: Some(entry.param_descriptions.clone()),
                    rejoin_data: None,
                },
                &mut dispatch,
            );
        }
        dispatch
    }

    fn handle_message_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::MESSAGE as i32, &mut dispatch) {
            return dispatch;
        }
        let Ok(payload) = Self::parse_payload::<WsMessageRequest>(data) else {
            return self.error_response(
                session_id,
                Routes::MESSAGE as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        if payload.message.len() > MAX_CHAT_MESSAGE_BYTES {
            return self.error_response(
                session_id,
                Routes::MESSAGE as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::MESSAGE as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        self.broadcast_except(
            &room_key,
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
        if !self.require_room_membership(session_id, Routes::PAUSE as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::PAUSE as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        {
            let Some(entry) = self.rooms.get_mut(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::PAUSE as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            if entry.state.is_paused() {
                return self.error_response(
                    session_id,
                    Routes::PAUSE as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            entry.state.pause();
        }
        self.broadcast_except(
            &room_key,
            session_id,
            WsCode::PAUSE as i32,
            WsNameEvent {
                name: self.session_name(session_id),
            },
            &mut dispatch,
        );
        self.push_ok_response(&mut dispatch, session_id, Routes::PAUSE as i32);
        dispatch
    }

    fn handle_quit_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::QUIT as i32, &mut dispatch) {
            return dispatch;
        }
        self.quit_room(session_id, &mut dispatch);
        self.push_ok_response(&mut dispatch, session_id, Routes::QUIT as i32);
        dispatch
    }

    fn handle_resume_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::RESUME as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::RESUME as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        {
            let Some(entry) = self.rooms.get_mut(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::RESUME as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            if !entry.state.is_paused() {
                return self.error_response(
                    session_id,
                    Routes::RESUME as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            entry.state.resume();
        }
        self.broadcast_except(
            &room_key,
            session_id,
            WsCode::RESUME as i32,
            WsNameEvent {
                name: self.session_name(session_id),
            },
            &mut dispatch,
        );
        self.push_ok_response(&mut dispatch, session_id, Routes::RESUME as i32);
        dispatch
    }

    fn handle_setting_request(&mut self, session_id: SessionId, data: &Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::SETTING as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.error_response(
                session_id,
                Routes::SETTING as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::SETTING as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Ok(payload) = Self::parse_payload::<share_type_public::WsSettingPayload>(data.clone())
        else {
            return self.error_response(
                session_id,
                Routes::SETTING as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        match self.update_room_settings(session_id, &payload) {
            Ok(()) => {
                let current_configs = self
                    .rooms
                    .get(&room_key)
                    .map(|e| e.configs.clone())
                    .unwrap_or_default();
                self.push_response_with_data(
                    session_id,
                    Routes::SETTING as i32,
                    WsResponseCode::OK,
                    share_type_public::WsSettingPayload {
                        current_configs: current_configs.clone(),
                    },
                    &mut dispatch,
                );
                self.broadcast_except(
                    &room_key,
                    session_id,
                    WsCode::SETTING as i32,
                    share_type_public::WsSettingPayload { current_configs },
                    &mut dispatch,
                );
                dispatch
            }
            Err(_) => self.error_response(
                session_id,
                Routes::SETTING as i32,
                WsResponseCode::ERROR_FORMAT,
            ),
        }
    }

    fn handle_swap_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_room_membership(session_id, Routes::SWAP as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Ok(payload) = Self::parse_payload::<WsSwapPositionPayload>(data) else {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let pos_a = payload.a;
        let pos_b = payload.b;
        if pos_a == pos_b {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(session_id, Routes::SWAP as i32, WsResponseCode::NOT_LOGIN);
        };
        if !self.room_supports_official_swap(&room_key) {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        // Collect session IDs before mutating
        let sid_a;
        let sid_b;
        {
            let Some(entry) = self.rooms.get(&room_key) else {
                return self.error_response(
                    session_id,
                    Routes::SWAP as i32,
                    WsResponseCode::NOT_LOGIN,
                );
            };
            if !entry.state.can_accept_players()
                || entry.state.is_ai_position(pos_a)
                || entry.state.is_ai_position(pos_b)
            {
                return self.error_response(
                    session_id,
                    Routes::SWAP as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let players = entry.state.players();
            let (sid_a_ref, _) = match players.get(&pos_a) {
                Some(val) => val,
                None => {
                    return self.error_response(
                        session_id,
                        Routes::SWAP as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
            };
            let (sid_b_ref, _) = match players.get(&pos_b) {
                Some(val) => val,
                None => {
                    return self.error_response(
                        session_id,
                        Routes::SWAP as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
            };
            sid_a = *sid_a_ref;
            sid_b = *sid_b_ref;
        }
        // Update state
        if let Some(entry) = self.rooms.get_mut(&room_key) {
            entry.state.swap_player(pos_a, pos_b);
        }
        // Update session positions
        if let Some(s) = self.sessions.get_mut(&sid_a) {
            s.position = Some(pos_b);
        }
        if let Some(s) = self.sessions.get_mut(&sid_b) {
            s.position = Some(pos_a);
        }

        self.broadcast(
            &room_key,
            WsCode::SWAP as i32,
            WsSwapPositionPayload { a: pos_a, b: pos_b },
            &mut dispatch,
        );

        // 如果 position 0 (房主) 换了新人，给新房主发建房参数响应。
        if pos_a == 0 || pos_b == 0 {
            let entry = self.rooms.get(&room_key).unwrap();
            let owner_sid = if pos_a == 0 { sid_b } else { sid_a };
            self.push_response_with_data(
                owner_sid,
                Routes::SWAP as i32,
                WsResponseCode::OK,
                share_type_public::WsCreateResponse {
                    param_descriptions: entry.param_descriptions.clone(),
                    settlement_time: config_value(&entry.configs, "settlement_time", 5),
                },
                &mut dispatch,
            );
        }

        self.push_ok_response(&mut dispatch, session_id, Routes::SWAP as i32);
        dispatch
    }

    /// 房间是否暂停。
    pub fn is_room_paused(&self, room_key: &str) -> bool {
        self.rooms
            .get(room_key)
            .map(|e| e.state.is_paused())
            .unwrap_or(false)
    }

    fn mark_disconnected(
        &mut self,
        session_id: SessionId,
        session: &mut SessionState,
        ai_takeover_authorized: bool,
        dispatch: &mut Dispatch,
    ) {
        let Some(room_key) = session.room_key.take() else {
            return;
        };
        session.official_session_id = None;
        let mut name = session.name.clone().unwrap_or_default();
        let mut position = session.position.take();
        let mut recipients = Vec::new();
        // `disconnect` removes the current session before reaching here. AI
        // players and disconnected roster entries do not have live sessions,
        // so they must not keep an otherwise abandoned room alive.
        let has_connected_human = !self.connected_session_ids(&room_key).is_empty();
        let mut should_remove_room = false;

        if let Some(entry) = self.rooms.get_mut(&room_key) {
            let players = entry.state.players();
            if position.is_none()
                && let Some((pos, (_, player_name))) =
                    players.iter().find(|(_, (sid, _))| *sid == session_id)
            {
                position = Some(*pos);
                if name.is_empty() {
                    name = player_name.clone();
                }
            }

            if let Some(pos) = position {
                entry.state.mark_disconnected(pos);
                if ai_takeover_authorized && !entry.state.is_ai_position(pos) {
                    entry.state.mark_ai_takeover_position(pos);
                } else {
                    entry.state.clear_ai_takeover_position(pos);
                }
            }

            if !has_connected_human {
                entry.state.set_turn_countdown(0);
                entry.state.request_stop();
                should_remove_room = true;
            }

            if should_remove_room {
                // There is nobody connected to receive an inactive-member
                // event. Remove the entry immediately so the room name can be
                // reused while the old loop observes `stop_requested`.
                dlog!(
                    tracing::Level::WARN,
                    "All human sessions disconnected from room '{}'; removing room",
                    room_key
                );
            } else {
                let Some(pos) = position else {
                    return;
                };
                recipients.extend(
                    entry
                        .state
                        .players()
                        .values()
                        .filter_map(|(sid, _)| (*sid != session_id).then_some(*sid)),
                );
                let event = CommonEvent {
                    code: WsCode::JOIN as i32,
                    data: serde_json::to_value(share_type_public::WsMemberInfo {
                        name,
                        avatar_url: entry.state.player_avatar(pos),
                        position: pos as i32,
                        is_active: false,
                        is_ai: false,
                        away: true,
                        is_ai_takeover: entry.state.is_ai_takeover_position(pos),
                    })
                    .unwrap_or(Value::Null),
                };
                for recipient in recipients {
                    dispatch.messages.push(Delivery {
                        recipient,
                        payload: OutboundPayload::Event(event.clone()),
                    });
                }
            }
        }

        if should_remove_room {
            self.rooms.remove(&room_key);
        }
    }

    fn name_taken_in_room(
        &self,
        room_key: &str,
        name: &str,
        exclude_session_id: Option<SessionId>,
    ) -> bool {
        let Some(entry) = self.rooms.get(room_key) else {
            return false;
        };
        entry.state.players().values().any(|(sid, n)| {
            if exclude_session_id == Some(*sid) {
                return false;
            }
            n == name
        })
    }

    fn next_ai_name(&self, room_key: &str, game_id: GameId, preferred_index: usize) -> String {
        let mut index = preferred_index.max(1);
        let prefix = match game_id {
            GameId::TEXAS_HOLD_EM
            | GameId::OPEN_HOLD_EM
            | GameId::SHORT_DECK_HOLD_EM
            | GameId::OMAHA_HOLD_EM => "Bot",
            _ => "AI",
        };
        loop {
            let name = format!("{} {}", prefix, index);
            if !self.name_taken_in_room(room_key, &name, None) {
                return name;
            }
            index += 1;
        }
    }

    pub fn parse_payload<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn player_by_name(&self, room_key: &str, name: &str) -> Option<(usize, SessionId)> {
        let entry = self.rooms.get(room_key)?;
        entry
            .state
            .players()
            .into_iter()
            .find_map(|(position, (sid, player_name))| {
                if player_name == name {
                    Some((position, sid))
                } else {
                    None
                }
            })
    }

    pub fn push_ok_response(&self, dispatch: &mut Dispatch, session_id: SessionId, route: i32) {
        dispatch
            .messages
            .push(Self::direct_response(session_id, route, WsResponseCode::OK));
    }

    /// 给指定 session 发响应（带 data）。
    pub fn push_response_with_data<T: serde::Serialize>(
        &self,
        session_id: SessionId,
        route: i32,
        code: WsResponseCode,
        data: T,
        dispatch: &mut Dispatch,
    ) {
        dispatch.messages.push(Delivery {
            recipient: session_id,
            payload: OutboundPayload::Response(RequestResponse::WithData(WsResponse {
                route,
                code,
                data: serde_json::to_value(data).unwrap_or(Value::Null),
            })),
        });
    }

    fn quit_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch) {
        let Some(mut session) = self.sessions.remove(&session_id) else {
            return;
        };
        self.remove_from_current_room(session_id, &mut session, dispatch);
        self.sessions.insert(session_id, session);
    }

    fn remove_from_current_room(
        &mut self,
        session_id: SessionId,
        session: &mut SessionState,
        dispatch: &mut Dispatch,
    ) {
        let Some(room_key) = session.room_key.take() else {
            return;
        };
        session.official_session_id = None;
        let mut leave_name = session.name.clone().unwrap_or_default();
        // `quit_room` temporarily removes this session, so only other live
        // human sessions remain in the room service lookup.
        let has_other_connected_human = !self.connected_session_ids(&room_key).is_empty();

        let mut recipients = Vec::new();
        if let Some(entry) = self.rooms.get_mut(&room_key) {
            let players = entry.state.players();
            let mut position = session.position.take();
            if position.is_none()
                && let Some((pos, (_, name))) =
                    players.iter().find(|(_, (sid, _))| *sid == session_id)
            {
                position = Some(*pos);
                if leave_name.is_empty() {
                    leave_name = name.clone();
                }
            }
            if let Some(pos) = position {
                entry.state.remove_player(pos);
            }
            recipients.extend(entry.state.players().values().map(|(sid, _)| *sid));
            // `/quit` is a permanent departure and always terminates the
            // current game loop. A normal disconnect follows the separate
            // away/reconnect path in `mark_disconnected`.
            entry.state.set_turn_countdown(0);
            entry.state.request_stop();
            if !has_other_connected_human {
                dlog!(
                    tracing::Level::WARN,
                    "Last connected human quit room '{}'; removing room",
                    room_key
                );
                self.rooms.remove(&room_key);
            }
        }

        let event = CommonEvent {
            code: WsCode::QUIT as i32,
            data: serde_json::to_value(WsNameEvent { name: leave_name }).unwrap_or(Value::Null),
        };

        for recipient in recipients {
            dispatch.messages.push(Delivery {
                recipient,
                payload: OutboundPayload::Event(event.clone()),
            });
        }
    }

    pub fn require_room_membership(
        &self,
        session_id: SessionId,
        route: i32,
        dispatch: &mut Dispatch,
    ) -> bool {
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

    pub fn reset_room_common_state_for_new_game(
        &mut self,
        room_key: &str,
    ) -> Option<Arc<std::sync::Mutex<crate::game_state::CommonGameState>>> {
        let entry = self.rooms.get_mut(room_key)?;
        let current = entry.state.shared_common_state();
        let current = current.lock().unwrap();
        let mut next = crate::game_state::CommonGameState::new();
        next.players = current.players.clone();
        next.avatars = current.avatars.clone();
        next.away_positions = current.away_positions.clone();
        next.disconnected_positions = current.disconnected_positions.clone();
        next.ai_positions = current.ai_positions.clone();
        next.ai_takeover_positions = current.ai_takeover_positions.clone();
        let next = Arc::new(std::sync::Mutex::new(next));
        entry.state = Box::new(crate::game_state::SharedGameState::from_common(Arc::clone(
            &next,
        )));
        Some(next)
    }

    /// 获取房间共享 CommonGameState 句柄（供游戏 loop 与 common 同步访问）。
    pub fn room_common_state(
        &self,
        room_key: &str,
    ) -> Option<std::sync::Arc<std::sync::Mutex<crate::game_state::CommonGameState>>> {
        self.rooms
            .get(room_key)
            .map(|entry| entry.state.shared_common_state())
    }

    /// 获取当前 configs（HashMap 形式，给游戏逻辑用）。
    pub fn room_configs(&self, room_key: &str) -> Option<HashMap<String, i32>> {
        self.rooms.get(room_key).map(|e| e.configs.clone())
    }

    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }

    pub fn room_exists(&self, room_key: &str) -> bool {
        self.rooms.contains_key(room_key)
    }

    pub fn room_game_id(&self, room_key: &str) -> Option<GameId> {
        self.rooms.get(room_key).map(|entry| entry.game_id)
    }

    /// 房间人数是否达到下限（可以开始了）。
    pub fn room_is_ready_to_start(&self, room_key: &str) -> bool {
        let Some(entry) = self.rooms.get(room_key) else {
            return false;
        };
        let count = entry.state.players().len();
        count >= entry.min_players
    }

    pub fn room_key_of(&self, session_id: SessionId) -> Option<String> {
        self.sessions
            .get(&session_id)
            .and_then(|item| item.room_key.as_ref())
            .cloned()
    }

    pub fn session_official_session_id(&self, session_id: SessionId) -> Option<String> {
        self.sessions
            .get(&session_id)
            .and_then(|session| session.official_session_id.clone())
    }

    pub fn session_is_away(&self, session_id: SessionId) -> bool {
        let Some(session) = self.sessions.get(&session_id) else {
            return false;
        };
        let (Some(room_key), Some(position)) = (session.room_key.as_ref(), session.position) else {
            return false;
        };
        self.rooms
            .get(room_key)
            .is_some_and(|entry| entry.state.is_away(position))
    }

    pub fn set_session_ai_takeover(&mut self, session_id: SessionId, enabled: bool) -> bool {
        let Some(session) = self.sessions.get(&session_id) else {
            return false;
        };
        let (Some(room_key), Some(position)) = (session.room_key.clone(), session.position) else {
            return false;
        };
        let Some(entry) = self.rooms.get_mut(&room_key) else {
            return false;
        };
        if !entry.state.is_away(position) || entry.state.is_ai_position(position) {
            return false;
        }
        if enabled {
            entry.state.mark_ai_takeover_position(position);
        } else {
            entry.state.clear_ai_takeover_position(position);
        }
        true
    }

    pub fn room_position_is_ai_takeover(&self, room_key: &str, position: usize) -> bool {
        self.rooms
            .get(room_key)
            .is_some_and(|entry| entry.state.is_ai_takeover_position(position))
    }

    /// 返回房间内所有成员 (SessionId, name, position, avatar_url)。
    pub fn room_members(&self, room_key: &str) -> Vec<(SessionId, String, usize, String)> {
        let Some(entry) = self.rooms.get(room_key) else {
            return Vec::new();
        };
        entry
            .state
            .players()
            .iter()
            .map(|(pos, (sid, name))| (*sid, name.clone(), *pos, entry.state.player_avatar(*pos)))
            .collect()
    }

    pub fn room_official_match_id(&self, room_key: &str) -> Option<i64> {
        self.rooms
            .get(room_key)
            .and_then(|entry| entry.official_match_id)
    }

    /// Only official landlord, Shenyang Mahjong, and tractor rooms support
    /// seat swapping. Custom WS rooms and P2P games cannot expose it.
    pub fn room_supports_official_swap(&self, room_key: &str) -> bool {
        let Some(entry) = self.rooms.get(room_key) else {
            return false;
        };
        if !matches!(
            entry.game_id,
            GameId::LANDLORD | GameId::SHENYANG_MAHJONG | GameId::TRACTOR
        ) {
            return false;
        }
        let human_session_ids = entry
            .state
            .players()
            .into_iter()
            .filter(|(position, _)| !entry.state.is_ai_position(*position))
            .map(|(_, (session_id, _))| session_id)
            .collect::<Vec<_>>();
        !human_session_ids.is_empty()
            && human_session_ids.into_iter().all(|session_id| {
                self.sessions
                    .get(&session_id)
                    .and_then(|session| session.official_session_id.as_deref())
                    .is_some_and(|value| !value.is_empty())
            })
    }

    pub fn room_official_player_sessions(&self, room_key: &str) -> Vec<OfficialPlayerSession> {
        let Some(entry) = self.rooms.get(room_key) else {
            return Vec::new();
        };
        let mut players = entry
            .state
            .players()
            .into_iter()
            .filter_map(|(position, (session_id, _))| {
                if entry.state.is_ai_position(position) {
                    return None;
                }
                let session_id = self
                    .sessions
                    .get(&session_id)
                    .and_then(|session| session.official_session_id.as_ref())
                    .filter(|session_id| !session_id.is_empty())?
                    .clone();
                Some(OfficialPlayerSession {
                    position,
                    session_id,
                })
            })
            .collect::<Vec<_>>();
        players.sort_by_key(|player| player.position);
        players
    }

    pub fn room_position_official_session_id(
        &self,
        room_key: &str,
        position: usize,
    ) -> Option<String> {
        let entry = self.rooms.get(room_key)?;
        if entry.state.is_ai_position(position) {
            return None;
        }
        let session_id = entry.state.players().get(&position)?.0;
        self.sessions
            .get(&session_id)
            .and_then(|session| session.official_session_id.as_ref())
            .filter(|session_id| !session_id.is_empty())
            .cloned()
    }

    pub fn room_official_user_id(&self, room_key: &str, position: usize) -> Option<i64> {
        self.rooms
            .get(room_key)
            .and_then(|entry| entry.official_user_ids_by_position.get(&position).copied())
    }

    fn select_position(
        &self,
        room_key: &str,
        max_players: usize,
        session_id: SessionId,
        can_join_new_position: bool,
    ) -> Option<usize> {
        let Some(entry) = self.rooms.get(room_key) else {
            return Some(0);
        };
        let players = entry.state.players();
        // 如果已经在房间中，返回现有位置
        if let Some(pos) = players
            .iter()
            .find_map(|(p, (sid, _))| if *sid == session_id { Some(*p) } else { None })
        {
            return Some(pos);
        }
        // Disconnected humans never consume room capacity. Prefer replacing
        // one before assigning a genuinely new seat. A running game may lock
        // new seats and reserve its hand positions, but an explicit
        // disconnected roster entry is still replaceable.
        (0..max_players)
            .find(|position| entry.state.is_disconnected(*position))
            .or_else(|| {
                if !can_join_new_position {
                    return None;
                }
                (0..max_players).find(|position| {
                    !players.contains_key(position)
                        && !entry.state.position_reserved_for_join(*position)
                })
            })
    }

    fn session_active_in_room(&self, session_id: SessionId, room_key: &str) -> bool {
        self.sessions
            .get(&session_id)
            .and_then(|session| session.room_key.as_deref())
            == Some(room_key)
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

    /// 设置 game state（游戏开始时调用）。
    pub fn set_room_game_state(
        &mut self,
        room_key: &str,
        game: Box<dyn crate::game_state::GameState>,
    ) {
        if let Some(entry) = self.rooms.get_mut(room_key) {
            entry.state = game;
        }
    }

    pub fn set_room_official_match(
        &mut self,
        room_key: &str,
        match_id: i64,
        user_ids_by_position: HashMap<usize, i64>,
    ) {
        if let Some(entry) = self.rooms.get_mut(room_key) {
            entry.official_match_id = Some(match_id);
            entry.official_user_ids_by_position = user_ids_by_position;
        }
    }

    /// 更新房间设置（只能由 position 0 调用）。
    /// 参数来自 SETTING 请求的 `WsSettingPayload`（`{ current_configs: { key: value } }`）。
    /// 验证每个参数：
    ///   - Range：值在 [min, max] 内
    ///   - Enum：值在 options 索引范围内
    ///     验证通过后同步更新 `configs` 和 `param_descriptions` 中的 `default`。
    pub fn update_room_settings(
        &mut self,
        session_id: SessionId,
        payload: &share_type_public::WsSettingPayload,
    ) -> Result<(), String> {
        let room_key = self
            .room_key_of(session_id)
            .ok_or_else(|| "Not in any room".to_string())?;
        let entry = self
            .rooms
            .get_mut(&room_key)
            .ok_or_else(|| "Room not found".to_string())?;
        if !entry.state.can_accept_players() {
            return Err("Room settings are locked after the game starts".to_string());
        }
        for (key, val) in &payload.current_configs {
            let Some(param) = entry.param_descriptions.get(key) else {
                return Err(format!("Unknown setting: {}", key));
            };
            match param {
                share_type_public::GameParam::Range(range) => {
                    if *val < range.min || *val > range.max {
                        return Err(format!(
                            "Value {} for '{}' out of range [{}, {}]",
                            val, key, range.min, range.max
                        ));
                    }
                }
                share_type_public::GameParam::Enum(e) => {
                    if *val < 0 || *val as usize >= e.options.len() {
                        return Err(format!(
                            "Value {} for '{}' is not a valid enum index (0..{})",
                            val,
                            key,
                            e.options.len().saturating_sub(1)
                        ));
                    }
                }
            }
            entry.configs.insert(key.clone(), *val);
            // 同步更新 param_descriptions 中的 default
            if let Some(param) = entry.param_descriptions.get_mut(key) {
                match param {
                    share_type_public::GameParam::Range(r) => r.default = *val,
                    share_type_public::GameParam::Enum(e) => e.default = *val as usize,
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

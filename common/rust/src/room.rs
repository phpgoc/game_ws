use std::collections::HashMap;

use crate::dlog;
use crate::game_setting::GameSettings;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, GameParam, Routes, WsCode, WsJoinRequest, WsMessageRequest, WsPositionEvent,
    WsRequest, WsResponseCode, WsSwapPositionPayload, WsWithoutDataResponse,
    ws::WsResponse,
    ws::{WsMessageEvent, WsNameEvent},
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
    rooms: HashMap<String, RoomEntry>,
}

#[derive(Debug, Default)]
struct SessionState {
    name: Option<String>,
    room_key: Option<String>,
    position: Option<usize>,
}

/// 一个房间，由 password（room_key）标识。
/// `configs` — 可配置参数的当前值（HashMap<String, i32>）。
/// `param_descriptions` — 参数描述（GameParam），创建时由游戏提供。
/// `state` — 游戏状态，始终存在（首个 JOIN 时创建），玩家列表在 CommonGameState.players 里。
struct RoomEntry {
    configs: HashMap<String, i32>,
    param_descriptions: HashMap<String, GameParam>,
    min_players: usize,
    max_players: usize,
    state: Box<dyn crate::game_state::GameState>,
}

impl std::fmt::Debug for RoomEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomEntry")
            .field("configs", &self.configs)
            .field("param_descriptions", &self.param_descriptions.len())
            .field("min_players", &self.min_players)
            .field("max_players", &self.max_players)
            .field("state", &format_args!("<GameState>"))
            .finish()
    }
}

/// 构建房间设置的结果：GameSettings（含默认值和人数限制）和参数描述。
pub type SettingsBuilderResult = (GameSettings, HashMap<String, GameParam>);

fn config_value(configs: &HashMap<String, i32>, key: &str, default: i32) -> i32 {
    configs.get(key).copied().unwrap_or(default)
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
        self.mark_disconnected(session_id, &mut session, &mut dispatch);
        dispatch
    }

    pub fn handle_common_request<F>(
        &mut self,
        session_id: SessionId,
        request: &ClientRequest,
        room_settings_builder: F,
    ) -> Option<Dispatch>
    where
        F: Fn() -> SettingsBuilderResult,
    {
        self.sessions.entry(session_id).or_default();
        match request.route {
            r if r == Routes::JOIN as i32 => Some(self.handle_join_request(
                session_id,
                request.data.clone(),
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
            r if r == Routes::SWAP as i32 => {
                Some(self.handle_swap_request(session_id, request.data.clone()))
            }
            _ => None,
        }
    }

    pub fn ensure_in_room(
        &self,
        session_id: SessionId,
        route: i32,
        dispatch: &mut Dispatch,
    ) -> bool {
        self.require_login(session_id, route, dispatch)
    }

    pub fn push_ok_response(&self, dispatch: &mut Dispatch, session_id: SessionId, route: i32) {
        dispatch
            .messages
            .push(Self::direct_response(session_id, route, WsResponseCode::OK));
    }

    /// 房间人数是否达到下限（可以开始了）。
    pub fn room_ready_to_start(&self, session_id: SessionId) -> bool {
        let Some(room_key) = self.room_key_of(session_id) else {
            return false;
        };
        let Some(entry) = self.rooms.get(&room_key) else {
            return false;
        };
        let count = entry.state.players().len();
        count >= entry.min_players
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

    /// 获取当前 configs JSON（用于 SETTING/JOIN 响应）。
    fn current_configs_json(&self, room_key: &str) -> Option<Value> {
        self.rooms.get(room_key).map(|e| json!(e.configs))
    }

    /// 房间是否暂停。
    pub fn is_room_paused(&self, room_key: &str) -> bool {
        self.rooms
            .get(room_key)
            .map(|e| e.state.is_paused())
            .unwrap_or(false)
    }

    /// 返回房间内所有成员 (SessionId, name, position)。
    pub fn get_room_members(&self, room_key: &str) -> Vec<(SessionId, String, usize)> {
        let Some(entry) = self.rooms.get(room_key) else {
            return Vec::new();
        };
        entry
            .state
            .players()
            .iter()
            .filter_map(|(pos, (sid, name))| Some((*sid, name.clone(), *pos)))
            .collect()
    }

    pub fn room_key_of(&self, session_id: SessionId) -> Option<String> {
        self.sessions
            .get(&session_id)
            .and_then(|item| item.room_key.as_ref())
            .cloned()
    }

    pub fn room_exists(&self, room_key: &str) -> bool {
        self.rooms.contains_key(room_key)
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

    /// 获取当前 configs（HashMap 形式，给游戏逻辑用）。
    pub fn get_room_configs(&self, room_key: &str) -> Option<HashMap<String, i32>> {
        self.rooms.get(room_key).map(|e| e.configs.clone())
    }

    /// 获取当前 configs 的 JSON 值。
    pub fn get_room_configs_json(&self, session_id: SessionId) -> Option<Value> {
        let room_key = self.room_key_of(session_id)?;
        self.current_configs_json(&room_key)
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

    /// 清除 game state（游戏结束时调用）。
    pub fn clear_room_game_state(&mut self, room_key: &str) {
        if let Some(entry) = self.rooms.get_mut(room_key) {
            let common = entry.state.shared_common_state();
            entry.state = Box::new(crate::game_state::SharedGameState::from_common(common));
        }
    }

    /// 获取 game state 里的玩家快照。
    pub fn get_game_state_players(
        &self,
        room_key: &str,
    ) -> std::collections::HashMap<usize, (SessionId, String)> {
        self.rooms
            .get(room_key)
            .map(|e| e.state.players())
            .unwrap_or_default()
    }

    /// 获取房间共享 CommonGameState 句柄（供游戏 loop 与 common 同步访问）。
    pub fn get_room_common_state_handle(
        &self,
        room_key: &str,
    ) -> Option<std::sync::Arc<std::sync::Mutex<crate::game_state::CommonGameState>>> {
        self.rooms
            .get(room_key)
            .map(|entry| entry.state.shared_common_state())
    }

    /// 更新房间设置（只能由 position 0 调用）。
    /// 参数来自 SETTING 请求的 `WsSettingPayload`（`{ current_configs: { key: value } }`）。
    /// 验证每个参数：
    ///   - Range：值在 [min, max] 内
    ///   - Enum：值在 options 索引范围内
    /// 验证通过后同步更新 `configs` 和 `param_descriptions` 中的 `default`。
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

    pub fn send_all<T: serde::Serialize>(
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

    pub fn send_other<T: serde::Serialize>(
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

    fn handle_join_request<F>(
        &mut self,
        session_id: SessionId,
        data: Value,
        room_settings_builder: F,
    ) -> Dispatch
    where
        F: Fn() -> SettingsBuilderResult,
    {
        let Ok(payload) = Self::parse::<WsJoinRequest>(data) else {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let password = payload.password;
        let name = payload.name;
        if password.is_empty() || name.is_empty() {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        }
        dlog!(
            tracing::Level::INFO,
            "Session {} attempts to join room '{}' with name '{}'",
            session_id,
            password,
            name
        );

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
                            position: *p as i32,
                            is_active: !entry.state.is_disconnected(*p),
                        })
                        .collect();
                    self.push_response_with_data(
                        session_id,
                        Routes::JOIN as i32,
                        WsResponseCode::JOINED,
                        share_type_public::WsJoinResponse {
                            current_configs: entry.configs.clone(),
                            existing_members,
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
            let (settings, param_descriptions) = room_settings_builder();
            self.rooms.insert(
                password.clone(),
                RoomEntry {
                    configs: settings.values,
                    param_descriptions,
                    min_players: settings.min_players,
                    max_players: settings.max_players,
                    state: Box::new(crate::game_state::SharedGameState::new()),
                },
            );
        }

        let mut dispatch = Dispatch::default();

        if let Some((position, existing_session_id)) = self.player_by_name(&password, &name) {
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
                entry.state.clear_disconnected_position(position);
            }
            {
                let session = self.sessions.entry(session_id).or_default();
                session.name = Some(name.clone());
                session.room_key = Some(password.clone());
                session.position = Some(position);
            }

            self.send_other(
                &password,
                session_id,
                WsCode::JOIN as i32,
                share_type_public::WsMemberInfo {
                    name: name.clone(),
                    position: position as i32,
                    is_active: true,
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
                    position: *p as i32,
                    is_active: !entry.state.is_disconnected(*p),
                })
                .collect();
            self.push_response_with_data(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::JOINED,
                share_type_public::WsJoinResponse {
                    current_configs: entry.configs.clone(),
                    existing_members,
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
            self.remove_from_current_room(session_id, &mut tmp, &mut dispatch, WsCode::QUIT as i32);
            self.sessions.insert(session_id, tmp);
        }

        // — 检查名字唯一性 & 空位 —
        let max_players = self
            .rooms
            .get(&password)
            .map(|e| e.max_players)
            .unwrap_or(2);
        if self.name_taken_in_room(&password, &name, Some(session_id)) {
            return self.error_response(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Some(position) = self.select_position(&password, max_players, session_id) else {
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
            entry.state.add_player(position, session_id, &name);
        }

        {
            let session = self.sessions.entry(session_id).or_default();
            session.name = Some(name.clone());
            session.room_key = Some(password.clone());
            session.position = Some(position);
        }

        // — 广播 JOIN 事件给其他人 —
        self.send_other(
            &password,
            session_id,
            WsCode::JOIN as i32,
            share_type_public::WsMemberInfo {
                name: name_for_event,
                position: position as i32,
                is_active: true,
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
                    position: *p as i32,
                    is_active: !entry.state.is_disconnected(*p),
                })
                .collect();
            self.push_response_with_data(
                session_id,
                Routes::JOIN as i32,
                WsResponseCode::JOINED,
                share_type_public::WsJoinResponse {
                    current_configs: entry.configs.clone(),
                    existing_members,
                    rejoin_data: None,
                },
                &mut dispatch,
            );
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

    fn handle_setting_request(&mut self, session_id: SessionId, data: &Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::SETTING as i32, &mut dispatch) {
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
        let Ok(payload) = Self::parse::<share_type_public::WsSettingPayload>(data.clone()) else {
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
                self.push_ok_response(&mut dispatch, session_id, Routes::SETTING as i32);
                self.send_other(
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

    fn handle_message_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::MESSAGE as i32, &mut dispatch) {
            return dispatch;
        }
        let Ok(payload) = Self::parse::<WsMessageRequest>(data) else {
            return self.error_response(
                session_id,
                Routes::MESSAGE as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(
                session_id,
                Routes::MESSAGE as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        self.send_other(
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
        if !self.require_login(session_id, Routes::PAUSE as i32, &mut dispatch) {
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
        self.send_other(
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

    fn handle_resume_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::RESUME as i32, &mut dispatch) {
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
        self.send_other(
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

    fn handle_away_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::AWAY as i32, &mut dispatch) {
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
        self.send_all(
            &room_key,
            WsCode::AWAY as i32,
            WsPositionEvent {
                position: position as i32,
            },
            &mut dispatch,
        );
        dispatch
    }

    fn handle_back_request(&mut self, session_id: SessionId) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::BACK as i32, &mut dispatch) {
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
        self.send_all(
            &room_key,
            WsCode::BACK as i32,
            WsPositionEvent {
                position: position as i32,
            },
            &mut dispatch,
        );
        dispatch
    }

    fn handle_swap_request(&mut self, session_id: SessionId, data: Value) -> Dispatch {
        let mut dispatch = Dispatch::default();
        if !self.require_login(session_id, Routes::SWAP as i32, &mut dispatch) {
            return dispatch;
        }
        if self.session_position(session_id) != Some(0) {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::NO_PERMISSION,
            );
        }
        let Ok(payload) = Self::parse::<WsSwapPositionPayload>(data) else {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let pos_a: usize = 0;
        let pos_b = payload.b;
        if pos_b == pos_a {
            return self.error_response(
                session_id,
                Routes::SWAP as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        }
        let Some(room_key) = self.room_key_of(session_id) else {
            return self.error_response(session_id, Routes::SWAP as i32, WsResponseCode::NOT_LOGIN);
        };
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

        self.send_all(
            &room_key,
            WsCode::SWAP as i32,
            WsSwapPositionPayload { a: pos_a, b: pos_b },
            &mut dispatch,
        );

        // 如果 position 0 (房主) 换了新人（sid_b 成为了新的 0），给新人发 param_descriptions
        {
            let entry = self.rooms.get(&room_key).unwrap();
            self.push_response_with_data(
                sid_b,
                Routes::SWAP as i32,
                WsResponseCode::OK,
                share_type_public::WsCreateResponse {
                    param_descriptions: entry.param_descriptions.clone(),
                    start_time: config_value(&entry.configs, "start_time", 1),
                    settlement_time: config_value(&entry.configs, "settlement_time", 5),
                },
                &mut dispatch,
            );
        }

        self.push_ok_response(&mut dispatch, session_id, Routes::SWAP as i32);
        dispatch
    }

    pub fn parse<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
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

    fn session_active_in_room(&self, session_id: SessionId, room_key: &str) -> bool {
        self.sessions
            .get(&session_id)
            .and_then(|session| session.room_key.as_deref())
            == Some(room_key)
    }

    fn select_position(
        &self,
        room_key: &str,
        max_players: usize,
        session_id: SessionId,
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
        (0..max_players).find(|pos| !players.contains_key(pos))
    }

    fn quit_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch, code: i32) {
        let Some(mut session) = self.sessions.remove(&session_id) else {
            return;
        };
        self.remove_from_current_room(session_id, &mut session, dispatch, code);
        self.sessions.insert(session_id, session);
    }

    fn mark_disconnected(
        &mut self,
        session_id: SessionId,
        session: &mut SessionState,
        dispatch: &mut Dispatch,
    ) {
        let Some(room_key) = session.room_key.take() else {
            return;
        };
        let mut name = session.name.clone().unwrap_or_default();
        let mut position = session.position.take();
        let mut recipients = Vec::new();

        if let Some(entry) = self.rooms.get_mut(&room_key) {
            let players = entry.state.players();
            if position.is_none() {
                if let Some((pos, (_, player_name))) =
                    players.iter().find(|(_, (sid, _))| *sid == session_id)
                {
                    position = Some(*pos);
                    if name.is_empty() {
                        name = player_name.clone();
                    }
                }
            }
            let Some(pos) = position else {
                return;
            };
            entry.state.mark_disconnected(pos);
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
                    position: pos as i32,
                    is_active: false,
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

    fn disband_room(&mut self, session_id: SessionId, dispatch: &mut Dispatch) {
        let Some(room_key) = self.room_key_of(session_id) else {
            return;
        };
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

    fn remove_from_current_room(
        &mut self,
        session_id: SessionId,
        session: &mut SessionState,
        dispatch: &mut Dispatch,
        code: i32,
    ) {
        let Some(room_key) = session.room_key.take() else {
            return;
        };
        let mut leave_name = session.name.clone().unwrap_or_default();

        let mut recipients = Vec::new();
        if let Some(entry) = self.rooms.get_mut(&room_key) {
            let players = entry.state.players();
            let mut position = session.position.take();
            if position.is_none() {
                if let Some((pos, (_, name))) =
                    players.iter().find(|(_, (sid, _))| *sid == session_id)
                {
                    position = Some(*pos);
                    if leave_name.is_empty() {
                        leave_name = name.clone();
                    }
                }
            }
            if let Some(pos) = position {
                entry.state.remove_player(pos);
            }
            recipients.extend(entry.state.players().values().map(|(sid, _)| *sid));
            // 如果房间里没人了，删除房间
            if entry.state.players().is_empty() {
                self.rooms.remove(&room_key);
            }
        }

        let event = if code == WsCode::QUIT as i32 {
            CommonEvent {
                code,
                data: serde_json::to_value(WsNameEvent { name: leave_name }).unwrap_or(Value::Null),
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

    fn direct_response(recipient: SessionId, route: i32, code: WsResponseCode) -> Delivery {
        Delivery {
            recipient,
            payload: OutboundPayload::Response(RequestResponse::WithoutData(
                WsWithoutDataResponse { route, code },
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use share_type_public::{GameParam, GameParamRange, Routes, WsCode, WsRequest, WsResponseCode};

    use super::{Dispatch, OutboundPayload, RequestResponse, RoomService};
    use crate::game_setting::GameSettings;

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

    fn settings() -> super::SettingsBuilderResult {
        let params: HashMap<String, GameParam> = [(
            "test_param".into(),
            GameParam::Range(GameParamRange {
                default: 200,
                min: 50,
                max: 2000,
            }),
        )]
        .into_iter()
        .collect();

        let mut s = GameSettings::new(3, 3);
        for (key, param) in &params {
            if let GameParam::Range(r) = param {
                s.values.insert(key.clone(), r.default);
            }
        }

        (s, params)
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
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );
        let join_dispatch = service
            .handle_common_request(
                2,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u2","password":"p1"}),
                },
                settings,
            )
            .expect("join common");
        let join_response_has_settings =
            join_dispatch
                .messages
                .iter()
                .any(|item| match &item.payload {
                    OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                        item.recipient == 2
                            && resp.code as i32 == WsResponseCode::JOINED as i32
                            && resp.data.get("current_configs").is_some()
                            && resp.data.get("name").is_none()
                    }
                    _ => false,
                });
        assert!(join_response_has_settings);
        let join_event_has_no_settings =
            join_dispatch
                .messages
                .iter()
                .any(|item| match &item.payload {
                    OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
                        event.data.get("settings").is_none() && event.data.get("position").is_some()
                    }
                    _ => false,
                });
        assert!(join_event_has_no_settings);
        let _ = service.handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p2"}),
            },
            settings,
        );

        let message = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::MESSAGE as i32,
                    data: serde_json::json!({"message":"hi"}),
                },
                settings,
            )
            .expect("message common");
        assert_eq!(
            recipients_of(WsCode::MESSAGE as i32, &message),
            [2_u64].into_iter().collect()
        );

        let pause = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::PAUSE as i32,
                    data: serde_json::json!({}),
                },
                settings,
            )
            .expect("pause common");
        assert_eq!(
            recipients_of(WsCode::PAUSE as i32, &pause),
            [2_u64].into_iter().collect()
        );

        let resume = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::RESUME as i32,
                    data: serde_json::json!({}),
                },
                settings,
            )
            .expect("resume common");
        assert_eq!(
            recipients_of(WsCode::RESUME as i32, &resume),
            [2_u64].into_iter().collect()
        );
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
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );

        let duplicate = service
            .handle_common_request(
                2,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u1","password":"p1"}),
                },
                settings,
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
        );
        let _ = service.handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p1"}),
            },
            settings,
        );
        let overflow = service
            .handle_common_request(
                4,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u4","password":"p1"}),
                },
                settings,
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
    fn join_idempotent_for_same_room_same_name() {
        let mut service = RoomService::default();
        service.connect(1);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );

        let rejoin = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u1","password":"p1"}),
                },
                settings,
            )
            .expect("join common");
        let rejoin_joined = rejoin.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                resp.code as i32 == WsResponseCode::JOINED as i32
            }
            _ => false,
        });
        assert!(rejoin_joined);

        let join_other_room = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u1","password":"p2"}),
                },
                settings,
            )
            .expect("join common");
        let join_other_room_denied =
            join_other_room
                .messages
                .iter()
                .any(|item| match &item.payload {
                    OutboundPayload::Response(RequestResponse::WithoutData(resp)) => {
                        resp.code as i32 == WsResponseCode::NO_PERMISSION as i32
                    }
                    _ => false,
                });
        assert!(join_other_room_denied);
    }

    #[test]
    fn disconnected_name_can_rejoin_same_position() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
        );

        let disconnect = service.disconnect(1);
        let inactive_event = disconnect.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
                item.recipient == 2
                    && event.data.get("name").and_then(|v| v.as_str()) == Some("u1")
                    && event.data.get("position").and_then(|v| v.as_i64()) == Some(0)
                    && event.data.get("is_active").and_then(|v| v.as_bool()) == Some(false)
            }
            _ => false,
        });
        assert!(inactive_event);

        let rejoin = service
            .handle_common_request(
                3,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u1","password":"p1"}),
                },
                settings,
            )
            .expect("join common");

        assert_eq!(service.session_position(3), Some(0));
        assert_eq!(
            service
                .get_game_state_players("p1")
                .get(&0)
                .map(|(sid, _)| *sid),
            Some(3)
        );
        let active_event = rejoin.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Event(event) if event.code == WsCode::JOIN as i32 => {
                item.recipient == 2
                    && event.data.get("name").and_then(|v| v.as_str()) == Some("u1")
                    && event.data.get("position").and_then(|v| v.as_i64()) == Some(0)
                    && event.data.get("is_active").and_then(|v| v.as_bool()) == Some(true)
            }
            _ => false,
        });
        assert!(active_event);
        let joined = rejoin.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                item.recipient == 3 && resp.code as i32 == WsResponseCode::JOINED as i32
            }
            _ => false,
        });
        assert!(joined);
    }

    #[test]
    fn clearing_game_state_preserves_room_members() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
        );
        let _ = service.disconnect(2);

        service.clear_room_game_state("p1");

        let players = service.get_game_state_players("p1");
        assert_eq!(players.len(), 2);
        assert_eq!(players.get(&0).map(|(_, name)| name.as_str()), Some("u1"));
        assert_eq!(players.get(&1).map(|(_, name)| name.as_str()), Some("u2"));

        let rejoin = service
            .handle_common_request(
                2,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u2","password":"p1"}),
                },
                settings,
            )
            .expect("join common");
        let joined = rejoin.messages.iter().any(|item| match &item.payload {
            OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                item.recipient == 2 && resp.code as i32 == WsResponseCode::JOINED as i32
            }
            _ => false,
        });
        assert!(joined);
        assert_eq!(service.session_position(2), Some(1));
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
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            3,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u3","password":"p1"}),
            },
            settings,
        );

        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::QUIT as i32,
                data: serde_json::json!({}),
            },
            settings,
        );

        let join4 = service
            .handle_common_request(
                4,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u4","password":"p1"}),
                },
                settings,
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
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
        );

        let resume_before_pause = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::RESUME as i32,
                    data: serde_json::json!({}),
                },
                settings,
            )
            .expect("resume common");
        let resume_denied = resume_before_pause
            .messages
            .iter()
            .any(|item| match &item.payload {
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
        );
        let pause_again = service
            .handle_common_request(
                1,
                &WsRequest {
                    route: Routes::PAUSE as i32,
                    data: serde_json::json!({}),
                },
                settings,
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
    fn disband_allows_join_recreate_room() {
        let mut service = RoomService::default();
        service.connect(1);
        service.connect(2);
        service.connect(3);

        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u1","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            2,
            &WsRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({"name":"u2","password":"p1"}),
            },
            settings,
        );
        let _ = service.handle_common_request(
            1,
            &WsRequest {
                route: Routes::DISBAND as i32,
                data: serde_json::json!({}),
            },
            settings,
        );

        let join_after_disband = service
            .handle_common_request(
                3,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({"name":"u3","password":"p1"}),
                },
                settings,
            )
            .expect("join common");
        let joined_ok = join_after_disband
            .messages
            .iter()
            .any(|item| match &item.payload {
                OutboundPayload::Response(RequestResponse::WithData(resp)) => {
                    resp.code as i32 == WsResponseCode::JOINED as i32
                }
                _ => false,
            });
        assert!(joined_ok);
    }
}

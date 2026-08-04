use std::{collections::HashMap, sync::Arc};

use serde_json::Value;
use share_type_public::{
    CommonEvent, GameId, Routes, UpgradeRoutes, UpgradeWsCode, WsCode, WsResponseCode,
    WsUpgradeBottomBuriedEvent, WsUpgradeBottomCardsEvent, WsUpgradeBuryBottomRequest,
    WsUpgradeDealEvent, WsUpgradeDeclareTrumpRequest, WsUpgradeHandEvent, WsUpgradePlayEvent,
    WsUpgradePlayRequest, WsUpgradeSelectTrumpRequest, WsUpgradeTableSnapshotEvent,
};
use tokio::sync::Mutex;
use ws_common::GameState;
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
    SessionId, SessionSenders,
};

use crate::{
    game_setting::{
        KEY_ATTACKING_WIN_SCORE, KEY_DECK_COUNT, KEY_PLAY_TIME, KEY_SCORE_PER_LEVEL,
        KEY_SHUTOUT_BONUS_LEVELS, build_upgrade_settings, deck_count_from_setting,
    },
    state::{PLAYER_COUNT, UpgradeGameState, UpgradeRules, UpgradeStateHandle},
};

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, UpgradeStateHandle>>>;

pub struct UpgradeGameHandler {
    room_service: Option<Arc<Mutex<RoomService>>>,
    senders: Option<SessionSenders>,
    states: StateRegistry,
}

struct UpgradeGameStateHandle {
    inner: UpgradeStateHandle,
}

fn join_succeeded(dispatch: &Dispatch, session_id: SessionId) -> bool {
    dispatch.messages.iter().any(|message| {
        message.recipient == session_id
            && matches!(
                &message.payload,
                OutboundPayload::Response(RequestResponse::WithData(response))
                    if response.route == Routes::JOIN as i32
                        && response.code as i32 == WsResponseCode::JOINED as i32
            )
    })
}

impl UpgradeGameHandler {
    fn play_time(configs: &HashMap<String, i32>) -> u32 {
        configs.get(KEY_PLAY_TIME).copied().unwrap_or(30).max(1) as u32
    }

    fn bury_window_time(configs: &HashMap<String, i32>) -> u32 {
        Self::play_time(configs).saturating_mul(3)
    }

    fn configs_to_rules(configs: &HashMap<String, i32>) -> UpgradeRules {
        let deck_count = deck_count_from_setting(configs.get(KEY_DECK_COUNT).copied().unwrap_or(0));
        let total_cards = usize::from(deck_count.get()) * 54;
        let bottom_card_count = if total_cards.is_multiple_of(4) { 8 } else { 10 };
        UpgradeRules {
            deck_count,
            target_rank: upgrade_common::Rank::Three,
            final_target_rank: upgrade_common::Rank::Ace,
            attacking_win_score: configs
                .get(KEY_ATTACKING_WIN_SCORE)
                .copied()
                .unwrap_or(80)
                .max(1),
            score_per_level: configs
                .get(KEY_SCORE_PER_LEVEL)
                .copied()
                .unwrap_or(40)
                .max(1),
            shutout_bonus_levels: configs
                .get(KEY_SHUTOUT_BONUS_LEVELS)
                .copied()
                .unwrap_or(1)
                .clamp(0, 3) as u8,
            bottom_card_count,
            trump_suit: None,
        }
    }

    fn state(&self, room_key: &str) -> Option<UpgradeStateHandle> {
        self.states.lock().unwrap().get(room_key).cloned()
    }

    fn current_state(
        &self,
        room_service: &RoomService,
        room_key: &str,
    ) -> Option<UpgradeStateHandle> {
        let state = self.state(room_key)?;
        let state_common = Arc::clone(&state.lock().unwrap().base);
        let room_common = room_service.room_common_state(room_key)?;
        let running = !state_common.lock().unwrap().stop_requested();
        (running && Arc::ptr_eq(&state_common, &room_common)).then_some(state)
    }

    fn push_private_event<T: serde::Serialize>(
        dispatch: &mut Dispatch,
        recipient: SessionId,
        code: UpgradeWsCode,
        payload: T,
    ) {
        dispatch.messages.push(Delivery {
            recipient,
            payload: OutboundPayload::Event(CommonEvent {
                code: code as i32,
                data: serde_json::to_value(payload).unwrap_or(Value::Null),
            }),
        });
    }

    fn push_private_hand(
        &self,
        dispatch: &mut Dispatch,
        room_service: &RoomService,
        session_id: SessionId,
        state: &UpgradeStateHandle,
    ) {
        let Some(position) = room_service.session_position(session_id) else {
            return;
        };
        let hand = state.lock().unwrap().private_hand(position);
        Self::push_private_event(
            dispatch,
            session_id,
            UpgradeWsCode::HAND_UPDATED,
            WsUpgradeHandEvent {
                position: position as i32,
                cards: hand,
            },
        );
    }

    fn push_snapshot(
        &self,
        dispatch: &mut Dispatch,
        room_service: &RoomService,
        room_key: &str,
        state: &UpgradeStateHandle,
    ) {
        let snapshot: WsUpgradeTableSnapshotEvent = state.lock().unwrap().snapshot();
        room_service.broadcast(room_key, WsCode::TABLE_SNAPSHOT as i32, snapshot, dispatch);
    }

    fn handle_start(&self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
        let route = Routes::START as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        if position != 0 {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        }
        let mut dispatch = Dispatch::default();
        if !room_service.require_room_membership(session_id, route, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        }
        if let Some(state) = self.current_state(room_service, &room_key) {
            let can_advance =
                state.lock().unwrap().phase == share_type_public::UpgradePhase::Settlement;
            if !can_advance {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let advanced = state.lock().unwrap().advance_after_settlement();
            if !matches!(advanced, Ok(true)) {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let configs = room_service.room_configs(&room_key).unwrap_or_default();
            state
                .lock()
                .unwrap()
                .set_turn_countdown(Self::bury_window_time(&configs));
            room_service.broadcast(
                &room_key,
                WsCode::START as i32,
                serde_json::json!({}),
                &mut dispatch,
            );
            self.broadcast_deal(room_service, &room_key, &state, &mut dispatch);
            self.push_snapshot(&mut dispatch, room_service, &room_key, &state);
            room_service.push_ok_response(&mut dispatch, session_id, route);
            return dispatch;
        }
        let Some(common) = room_service.reset_room_common_state_for_new_game(&room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let configs = room_service.room_configs(&room_key).unwrap_or_default();
        let rules = Self::configs_to_rules(&configs);
        let mut game_state = UpgradeGameState::from_common(Arc::clone(&common));
        if game_state.deal_new_round(rules).is_err() {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        }
        game_state.set_turn_countdown(Self::bury_window_time(&configs));
        let state = Arc::new(std::sync::Mutex::new(game_state));
        room_service.set_room_game_state(
            &room_key,
            Box::new(UpgradeGameStateHandle {
                inner: Arc::clone(&state),
            }),
        );
        self.states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&state));
        if let (Some(room_service_arc), Some(senders_arc)) =
            (self.room_service.as_ref(), self.senders.as_ref())
        {
            crate::game_loop::start_upgrade_game_loop(
                room_key.clone(),
                Arc::clone(&state),
                Arc::clone(room_service_arc),
                Arc::clone(senders_arc),
            );
        }

        room_service.broadcast(
            &room_key,
            WsCode::START as i32,
            serde_json::json!({}),
            &mut dispatch,
        );
        self.broadcast_deal(room_service, &room_key, &state, &mut dispatch);
        self.push_snapshot(&mut dispatch, room_service, &room_key, &state);
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }

    fn broadcast_deal(
        &self,
        room_service: &RoomService,
        room_key: &str,
        state: &UpgradeStateHandle,
        dispatch: &mut Dispatch,
    ) {
        // Read the room roster before locking the game state. `room_members`
        // reaches the same state through RoomService and would otherwise
        // attempt to lock it recursively.
        let members = room_service.room_members(room_key);
        let state_guard = state.lock().unwrap();
        let deck_count = i32::from(state_guard.rules.deck_count.get());
        let hand_count = state_guard.hand_count() as i32;
        let bottom_count = state_guard.rules.bottom_card_count as i32;
        if let Some(declaration) = state_guard.declaration.clone() {
            room_service.broadcast_connected(
                room_key,
                UpgradeWsCode::TRUMP_DECLARED as i32,
                declaration,
                dispatch,
            );
        }
        for position in 0..PLAYER_COUNT {
            let Some((session_id, _, _, _)) = members
                .iter()
                .find(|(_, _, member_position, _)| *member_position == position)
                .cloned()
            else {
                continue;
            };
            let cards = state_guard.private_hand(position);
            Self::push_private_event(
                dispatch,
                session_id,
                UpgradeWsCode::HAND_UPDATED,
                WsUpgradeHandEvent {
                    position: position as i32,
                    cards: cards.clone(),
                },
            );
            Self::push_private_event(
                dispatch,
                session_id,
                UpgradeWsCode::BOTTOM_CARDS,
                WsUpgradeBottomCardsEvent {
                    position: state_guard.dealer_position as i32,
                    cards: if position == state_guard.dealer_position {
                        state_guard.exposed_bottom()
                    } else {
                        Vec::new()
                    },
                    required_count: bottom_count,
                },
            );
        }
        room_service.broadcast_connected(
            room_key,
            WsCode::DEAL as i32,
            WsUpgradeDealEvent {
                position: state_guard.dealer_position as i32,
                cards: Vec::new(),
                deck_count,
                hand_count,
                bottom_card_count: bottom_count,
                target_rank: state_guard.target_rank_protocol(),
                dealt_count: state_guard.dealt_count as i32,
                total_deal_count: state_guard.total_deal_count as i32,
            },
            dispatch,
        );
    }

    fn handle_bury_bottom(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = UpgradeRoutes::BURY_BOTTOM as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(request) = RoomService::parse_payload::<WsUpgradeBuryBottomRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let play_time = Self::play_time(&room_service.room_configs(&room_key).unwrap_or_default());
        let result = {
            let mut state_guard = state.lock().unwrap();
            let result = state_guard.bury_bottom(position, request.cards);
            if result.is_ok() && state_guard.phase == share_type_public::UpgradePhase::Play {
                state_guard.set_turn_countdown(play_time);
            }
            result
        };
        if result.is_err() {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        }
        let state_guard = state.lock().unwrap();
        let event = WsUpgradeBottomBuriedEvent {
            position: position as i32,
            name: state_guard.player_name(position),
            bottom_card_count: state_guard.rules.bottom_card_count as i32,
        };
        let snapshot = state_guard.snapshot();
        let hand = state_guard.private_hand(position);
        drop(state_guard);
        let mut dispatch = Dispatch::default();
        room_service.broadcast(
            &room_key,
            UpgradeWsCode::BOTTOM_BURIED as i32,
            event,
            &mut dispatch,
        );
        Self::push_private_event(
            &mut dispatch,
            session_id,
            UpgradeWsCode::HAND_UPDATED,
            WsUpgradeHandEvent {
                position: position as i32,
                cards: hand,
            },
        );
        room_service.broadcast(
            &room_key,
            WsCode::TABLE_SNAPSHOT as i32,
            snapshot,
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }

    fn handle_select_trump(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = UpgradeRoutes::SELECT_TRUMP as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(request) = RoomService::parse_payload::<WsUpgradeSelectTrumpRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let play_time = Self::play_time(&room_service.room_configs(&room_key).unwrap_or_default());
        let result = {
            let mut state_guard = state.lock().unwrap();
            let result = state_guard.select_trump(position, request.trump_suit);
            if result.is_ok() {
                state_guard.set_turn_countdown(play_time);
            }
            result
        };
        if result.is_err() {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        }
        let snapshot = state.lock().unwrap().snapshot();
        let mut dispatch = Dispatch::default();
        room_service.broadcast(
            &room_key,
            WsCode::TABLE_SNAPSHOT as i32,
            snapshot,
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }

    fn handle_play(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = Routes::PLAY as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(request) = RoomService::parse_payload::<WsUpgradePlayRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let play_time = Self::play_time(&room_service.room_configs(&room_key).unwrap_or_default());
        let (resolution, event, snapshot, settlement) = {
            let mut state_guard = state.lock().unwrap();
            let resolution = match state_guard.play_cards(position, request.cards) {
                Ok(resolution) => resolution,
                Err(_) => {
                    return room_service.error_response(
                        session_id,
                        route,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
            };
            if !resolution.finished {
                state_guard.set_turn_countdown(play_time);
            }
            let event = WsUpgradePlayEvent {
                position: position as i32,
                name: state_guard.player_name(position),
                cards: resolution.played_cards.clone(),
                trick_index: state_guard.trick_index,
                next_position: state_guard.current_position as i32,
                remaining_hand_count: state_guard.private_hand(position).len() as i32,
                failed_throw: resolution.failed_throw.clone(),
            };
            let snapshot = state_guard.snapshot();
            let settlement = resolution.finished.then(|| state_guard.settlement_event());
            (resolution, event, snapshot, settlement)
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast(&room_key, WsCode::PLAY as i32, event, &mut dispatch);
        room_service.broadcast(
            &room_key,
            WsCode::TABLE_SNAPSHOT as i32,
            snapshot,
            &mut dispatch,
        );
        if let Some(settlement) = settlement {
            room_service.broadcast(
                &room_key,
                WsCode::GAME_OVER as i32,
                settlement,
                &mut dispatch,
            );
        }
        // Marking a move as accepted belongs to the state transition, while
        // this response confirms only the caller's own request.
        let _ = resolution;
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }
}

impl Default for UpgradeGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
            states: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for UpgradeGameHandler {
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
        let Some(state) = self.current_state(room_service, &room_key) else {
            return;
        };
        self.push_private_hand(dispatch, room_service, session_id, &state);
        self.push_snapshot(dispatch, room_service, &room_key, &state);
    }

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(ws_common::SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_upgrade_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::UPGRADE
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        match request.route {
            route if route == Routes::START as i32 => self.handle_start(room_service, session_id),
            route if route == Routes::PLAY as i32 => {
                self.handle_play(room_service, session_id, request.data)
            }
            route if route == UpgradeRoutes::BURY_BOTTOM as i32 => {
                self.handle_bury_bottom(room_service, session_id, request.data)
            }
            route if route == UpgradeRoutes::SELECT_TRUMP as i32 => {
                self.handle_select_trump(room_service, session_id, request.data)
            }
            route if route == UpgradeRoutes::DECLARE_TRUMP as i32 => {
                let _ = RoomService::parse_payload::<WsUpgradeDeclareTrumpRequest>(request.data);
                room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION)
            }
            _ => {
                room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
            }
        }
    }

    fn set_context(&mut self, senders: SessionSenders, room_service: Arc<Mutex<RoomService>>) {
        self.senders = Some(senders);
        self.room_service = Some(room_service);
    }
}

impl ws_common::GameState for UpgradeGameStateHandle {
    fn can_accept_players(&self) -> bool {
        self.inner.lock().unwrap().phase == share_type_public::UpgradePhase::Start
    }

    fn shared_common_state(&self) -> Arc<std::sync::Mutex<ws_common::CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

#[cfg(test)]
#[path = "game/tests.rs"]
mod tests;

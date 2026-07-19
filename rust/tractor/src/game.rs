use std::{collections::HashMap, sync::Arc};

use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, GameId, Routes, TractorRank, TractorRoutes, TractorWsCode, WsCode, WsResponseCode,
    games::tractor::{
        WsTractorBottomBuriedEvent, WsTractorBuryBottomRequest, WsTractorDeclareTrumpRequest,
        WsTractorHandEvent, WsTractorPlayEvent, WsTractorPlayRequest, WsTractorSelectTrumpRequest,
        WsTractorSettlementEvent,
    },
};
use tokio::sync::Mutex;
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
    SessionId, SessionSenders,
};

use crate::{
    game_setting::{
        KEY_BLOOD_ENABLED, KEY_BLOOD_SCORE_PER_UNIT, KEY_BLOOD_START_SCORE, KEY_BOTTOM_CARD_COUNT,
        KEY_DECK_COUNT, KEY_PLAY_TIME, KEY_REMOVED_RANK_COUNT, KEY_TARGET_RANK,
        build_tractor_settings,
    },
    game_state::{
        TractorGameState, TractorRules, TractorStateHandle, tractor_rank_from_setting_index,
    },
};

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, TractorStateHandle>>>;

pub struct TractorGameHandler {
    room_service: Option<Arc<Mutex<RoomService>>>,
    senders: Option<SessionSenders>,
    states: StateRegistry,
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

struct TractorGameStateHandle {
    inner: TractorStateHandle,
}

impl TractorGameHandler {
    fn configs_to_rules(configs: &HashMap<String, i32>) -> TractorRules {
        TractorRules {
            blood_enabled: configs.get(KEY_BLOOD_ENABLED).copied().unwrap_or(1) != 0,
            blood_score_per_unit: configs
                .get(KEY_BLOOD_SCORE_PER_UNIT)
                .copied()
                .unwrap_or(40)
                .max(1),
            blood_start_score: configs.get(KEY_BLOOD_START_SCORE).copied().unwrap_or(80),
            bottom_card_count: configs
                .get(KEY_BOTTOM_CARD_COUNT)
                .copied()
                .unwrap_or(8)
                .max(0) as usize,
            deck_count: configs
                .get(KEY_DECK_COUNT)
                .copied()
                .unwrap_or(2)
                .clamp(2, 4) as usize,
            final_target_rank: tractor_rank_from_setting_index(
                configs.get(KEY_TARGET_RANK).copied().unwrap_or(12),
            ),
            removed_rank_count: configs
                .get(KEY_REMOVED_RANK_COUNT)
                .copied()
                .unwrap_or(0)
                .clamp(0, 9) as usize,
            target_rank: TractorRank::TWO,
            trump_suit: None,
        }
    }

    fn current_state(
        &self,
        room_service: &RoomService,
        room_key: &str,
    ) -> Option<TractorStateHandle> {
        let state = self.state(room_key)?;
        let state_common = Arc::clone(&state.lock().unwrap().base);
        let room_common = room_service.room_common_state(room_key)?;
        let is_running = !state_common.lock().unwrap().stop_requested();
        (is_running && Arc::ptr_eq(&state_common, &room_common)).then_some(state)
    }

    fn handle_bury_bottom(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = TractorRoutes::BURY_BOTTOM as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(payload) = RoomService::parse_payload::<WsTractorBuryBottomRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let (event, hand, snapshot) = {
            let mut state = state.lock().unwrap();
            if state.bury_bottom(position, payload.cards).is_err() {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let play_time = room_service
                .room_configs(&room_key)
                .unwrap_or_default()
                .get(KEY_PLAY_TIME)
                .copied()
                .unwrap_or(30)
                .max(1) as u32;
            state.set_turn_countdown(play_time);
            (
                WsTractorBottomBuriedEvent {
                    position: position as i32,
                    name: state.player_name(position),
                    bottom_card_count: state.rules.bottom_card_count as i32,
                },
                state.hands.get(&position).cloned().unwrap_or_default(),
                state.snapshot(),
            )
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast(
            &room_key,
            TractorWsCode::BOTTOM_BURIED as i32,
            event,
            &mut dispatch,
        );
        Self::push_private_event(
            &mut dispatch,
            session_id,
            TractorWsCode::HAND_UPDATED,
            WsTractorHandEvent {
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

    fn handle_declare_trump(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = TractorRoutes::DECLARE_TRUMP as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(payload) = RoomService::parse_payload::<WsTractorDeclareTrumpRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let (declaration, snapshot) = {
            let mut state = state.lock().unwrap();
            let Ok(declaration) = state.declare_trump(position, payload.cards) else {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            (declaration, state.snapshot())
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast(
            &room_key,
            TractorWsCode::TRUMP_DECLARED as i32,
            declaration,
            &mut dispatch,
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

    fn handle_play(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let Some(position) = room_service.session_position(session_id) else {
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
        let Ok(payload) = RoomService::parse_payload::<WsTractorPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let (play_event, snapshot, finished) = {
            let mut s = state.lock().unwrap();
            let name = s.player_name(position);
            let Ok(played) = s.play_cards(position, name.clone(), payload.cards) else {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            let play_time = room_service
                .room_configs(&room_key)
                .unwrap_or_default()
                .get(KEY_PLAY_TIME)
                .copied()
                .unwrap_or(30)
                .max(1) as u32;
            s.set_turn_countdown(play_time);
            let play_event = WsTractorPlayEvent {
                position: played.position,
                name,
                cards: played.cards,
                trick_index: s.trick_index,
                next_position: s.current_position as i32,
                remaining_hand_count: s.remaining_hand_count(position),
            };
            let snapshot = s.snapshot();
            let finished = s.is_finished();
            (play_event, snapshot, finished)
        };

        let mut dispatch = Dispatch::default();
        room_service.broadcast(&room_key, WsCode::PLAY as i32, play_event, &mut dispatch);
        room_service.broadcast(
            &room_key,
            WsCode::TABLE_SNAPSHOT as i32,
            snapshot,
            &mut dispatch,
        );
        if finished {
            let (settlement, trump_suit) = {
                let s = state.lock().unwrap();
                let score = s.settlement_score();
                (
                    WsTractorSettlementEvent {
                        winner_positions: s.winner_positions(),
                        score,
                        blood_units: s.rules.blood_units(score),
                        target_rank: s.rules.target_rank,
                        match_finished: s.match_finished(),
                        next_target_rank: s.next_target_rank(),
                        player_scores: s.player_scores_snapshot(),
                    },
                    s.rules.trump_suit,
                )
            };
            crate::official::settle_round(
                room_service,
                &room_key,
                &settlement.winner_positions,
                settlement.score,
                settlement.target_rank,
                trump_suit,
            );
            room_service.broadcast(
                &room_key,
                WsCode::GAME_OVER as i32,
                settlement,
                &mut dispatch,
            );
        }
        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
        dispatch
    }

    fn handle_select_trump(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: Value,
    ) -> Dispatch {
        let route = TractorRoutes::SELECT_TRUMP as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(payload) = RoomService::parse_payload::<WsTractorSelectTrumpRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let (declaration, snapshot) = {
            let mut state = state.lock().unwrap();
            let Ok(declaration) = state.select_dealer_trump(position, payload.trump_suit) else {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            (declaration, state.snapshot())
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast(
            &room_key,
            TractorWsCode::TRUMP_DECLARED as i32,
            declaration,
            &mut dispatch,
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

    fn handle_start(&self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
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
        if !room_service.require_room_membership(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_LOGIN,
            );
        };
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        let Some(room_common) = room_service.room_common_state(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        if let Some(existing) = self.state(&room_key) {
            let existing_common = Arc::clone(&existing.lock().unwrap().base);
            let existing_stopped = existing_common.lock().unwrap().stop_requested();
            if Arc::ptr_eq(&existing_common, &room_common) && !existing_stopped {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            let mut states = self.states.lock().unwrap();
            if states
                .get(&room_key)
                .is_some_and(|current| Arc::ptr_eq(current, &existing))
            {
                states.remove(&room_key);
            }
        }
        let Some(common) = room_service.reset_room_common_state_for_new_game(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        let configs = room_service.room_configs(&room_key).unwrap_or_default();
        let rules = Self::configs_to_rules(&configs);
        let mut game_state = TractorGameState::from_common(Arc::clone(&common));
        if game_state.deal_new_round(rules).is_err() {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        game_state.set_turn_countdown(0);
        let state = Arc::new(std::sync::Mutex::new(game_state));
        room_service.set_room_game_state(
            &room_key,
            Box::new(TractorGameStateHandle {
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
            crate::game_loop::start_game_loop(
                room_key.clone(),
                Arc::clone(&state),
                Arc::clone(room_service_arc),
                Arc::clone(senders_arc),
                Arc::clone(&self.states),
            );
        }

        crate::official::create_match(room_service, &room_key);
        room_service.broadcast(&room_key, WsCode::START as i32, json!({}), &mut dispatch);
        self.push_table_snapshot(&room_key, room_service, &state, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
        dispatch
    }

    fn prune_stopped_states(&self, room_service: &mut RoomService) {
        let states = self
            .states
            .lock()
            .unwrap()
            .iter()
            .map(|(room_key, state)| (room_key.clone(), Arc::clone(state)))
            .collect::<Vec<_>>();
        for (room_key, state) in states {
            let common = Arc::clone(&state.lock().unwrap().base);
            if !common.lock().unwrap().stop_requested() {
                continue;
            }
            self.remove_state_if_same(&room_key, &state);
            room_service.clear_room_game_state_if_same(&room_key, &common);
        }
    }

    fn push_private_event<T: serde::Serialize>(
        dispatch: &mut Dispatch,
        recipient: SessionId,
        code: TractorWsCode,
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

    fn push_table_snapshot(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &TractorStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let snapshot = state.lock().unwrap().snapshot();
        room_service.broadcast(room_key, WsCode::TABLE_SNAPSHOT as i32, snapshot, dispatch);
    }

    fn remove_state_if_same(&self, room_key: &str, expected: &TractorStateHandle) -> bool {
        let mut states = self.states.lock().unwrap();
        let is_same = states
            .get(room_key)
            .is_some_and(|current| Arc::ptr_eq(current, expected));
        if is_same {
            states.remove(room_key);
        }
        is_same
    }

    fn state(&self, room_key: &str) -> Option<TractorStateHandle> {
        self.states.lock().unwrap().get(room_key).cloned()
    }
}

impl Default for TractorGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
            states: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for TractorGameHandler {
    #[cfg(feature = "official")]
    fn authorize_room_creation(
        &self,
        join: &share_type_public::WsJoinRequest,
    ) -> ws_common::MembershipAuthorization {
        Box::pin(crate::official::has_active_membership(
            join.session_id.clone(),
        ))
    }

    #[cfg(feature = "official")]
    fn authorize_ai_takeover(
        &self,
        official_session_id: String,
    ) -> ws_common::MembershipAuthorization {
        Box::pin(crate::official::has_active_membership(official_session_id))
    }

    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        if matches!(request.route, r if r == Routes::QUIT as i32 || r == Routes::DISBAND as i32) {
            self.prune_stopped_states(room_service);
        }
        if request.route != Routes::JOIN as i32 || !join_succeeded(dispatch, session_id) {
            return;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return;
        };
        let Some(position) = room_service.session_position(session_id) else {
            return;
        };
        let Some(state) = self.current_state(room_service, &room_key) else {
            return;
        };
        let (hand, snapshot, settlement) = {
            let state = state.lock().unwrap();
            (
                state.hands.get(&position).cloned().unwrap_or_default(),
                state.snapshot(),
                (state.phase == share_type_public::TractorPhase::Settlement).then(|| {
                    let score = state.settlement_score();
                    WsTractorSettlementEvent {
                        winner_positions: state.winner_positions(),
                        score,
                        blood_units: state.rules.blood_units(score),
                        target_rank: state.rules.target_rank,
                        match_finished: state.match_finished(),
                        next_target_rank: state.next_target_rank(),
                        player_scores: state.player_scores_snapshot(),
                    }
                }),
            )
        };
        Self::push_private_event(
            dispatch,
            session_id,
            TractorWsCode::HAND_UPDATED,
            WsTractorHandEvent {
                position: position as i32,
                cards: hand,
            },
        );
        dispatch.messages.push(Delivery {
            recipient: session_id,
            payload: OutboundPayload::Event(CommonEvent {
                code: WsCode::TABLE_SNAPSHOT as i32,
                data: serde_json::to_value(snapshot).unwrap_or(Value::Null),
            }),
        });
        if let Some(settlement) = settlement {
            dispatch.messages.push(Delivery {
                recipient: session_id,
                payload: OutboundPayload::Event(CommonEvent {
                    code: WsCode::GAME_OVER as i32,
                    data: serde_json::to_value(settlement).unwrap_or(Value::Null),
                }),
            });
        }
    }

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(ws_common::SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_tractor_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::TRACTOR
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        match request.route {
            r if r == Routes::START as i32 => self.handle_start(room_service, session_id),
            r if r == Routes::PLAY as i32 => {
                self.handle_play(room_service, session_id, request.data)
            }
            r if r == TractorRoutes::DECLARE_TRUMP as i32 => {
                self.handle_declare_trump(room_service, session_id, request.data)
            }
            r if r == TractorRoutes::BURY_BOTTOM as i32 => {
                self.handle_bury_bottom(room_service, session_id, request.data)
            }
            r if r == TractorRoutes::SELECT_TRUMP as i32 => {
                self.handle_select_trump(room_service, session_id, request.data)
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

impl ws_common::GameState for TractorGameStateHandle {
    fn can_accept_players(&self) -> bool {
        self.inner.lock().unwrap().phase == share_type_public::TractorPhase::Start
    }

    fn shared_common_state(&self) -> Arc<std::sync::Mutex<ws_common::CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use share_type_public::{WsJoinRequest, games::tractor::WsTractorTableSnapshotEvent};

    fn join_request(name: &str) -> ClientRequest {
        ClientRequest {
            route: Routes::JOIN as i32,
            data: serde_json::to_value(WsJoinRequest {
                name: name.to_string(),
                password: "room".to_string(),
                game_id: GameId::TRACTOR,
                session_id: String::new(),
                avatar_url: String::new(),
            })
            .unwrap(),
        }
    }

    fn join_with_hook(
        handler: &mut TractorGameHandler,
        room: &mut RoomService,
        session_id: SessionId,
        name: &str,
    ) -> Dispatch {
        let request = join_request(name);
        let mut dispatch = room
            .handle_common_request(session_id, &request, handler.game_id(), || {
                handler.build_room_settings()
            })
            .expect("common JOIN dispatch");
        handler.after_common_request(room, session_id, &request, &mut dispatch);
        dispatch
    }

    fn private_hand(dispatch: &Dispatch, session_id: SessionId) -> WsTractorHandEvent {
        dispatch
            .messages
            .iter()
            .find_map(|message| {
                if message.recipient != session_id {
                    return None;
                }
                let OutboundPayload::Event(event) = &message.payload else {
                    return None;
                };
                (event.code == TractorWsCode::HAND_UPDATED as i32)
                    .then(|| serde_json::from_value(event.data.clone()).unwrap())
            })
            .expect("private hand event")
    }

    fn table_snapshot(dispatch: &Dispatch, session_id: SessionId) -> WsTractorTableSnapshotEvent {
        dispatch
            .messages
            .iter()
            .find_map(|message| {
                if message.recipient != session_id {
                    return None;
                }
                let OutboundPayload::Event(event) = &message.payload else {
                    return None;
                };
                (event.code == WsCode::TABLE_SNAPSHOT as i32)
                    .then(|| serde_json::from_value(event.data.clone()).unwrap())
            })
            .expect("table snapshot event")
    }

    #[test]
    fn disconnected_player_can_rejoin_or_be_replaced_during_play() {
        let mut handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            join_with_hook(
                &mut handler,
                &mut room,
                session_id,
                &format!("u{session_id}"),
            );
        }
        handler.handle_start(&mut room, 1);
        let state = handler.state("room").expect("tractor state");
        let expected_hand = vec![4, 5, 6];
        {
            let mut state = state.lock().unwrap();
            state.phase = share_type_public::TractorPhase::Play;
            state.current_position = 1;
            state.hands.insert(0, vec![7, 8, 9]);
            state.hands.insert(1, expected_hand.clone());
            state.hands.insert(2, vec![10, 11, 12]);
            state.hands.insert(3, vec![13, 14, 15]);
            state.base.lock().unwrap().mark_away(1);
        }

        room.disconnect(2);
        let rejoined = join_with_hook(&mut handler, &mut room, 5, "u2");

        assert_eq!(room.session_position(5), Some(1));
        assert_eq!(private_hand(&rejoined, 5).cards, expected_hand);
        assert_eq!(
            table_snapshot(&rejoined, 5).phase,
            share_type_public::TractorPhase::Play
        );
        {
            let base = Arc::clone(&state.lock().unwrap().base);
            let common = base.lock().unwrap();
            assert!(!common.is_disconnected(1));
            assert!(!common.is_away(1));
        }

        room.disconnect(5);
        let replaced = join_with_hook(&mut handler, &mut room, 6, "replacement");

        assert_eq!(room.session_position(6), Some(1));
        assert_eq!(private_hand(&replaced, 6).cards, vec![4, 5, 6]);
        assert_eq!(state.lock().unwrap().player_name(1), "replacement");
        assert!(
            room.room_members("room")
                .iter()
                .all(|(_, name, _, _)| name != "u2")
        );

        state.lock().unwrap().phase = share_type_public::TractorPhase::Settlement;
        room.disconnect(6);
        let settlement_rejoin = join_with_hook(&mut handler, &mut room, 7, "replacement");
        assert!(settlement_rejoin.messages.iter().any(|message| {
            message.recipient == 7
                && matches!(
                    &message.payload,
                    OutboundPayload::Event(event) if event.code == WsCode::GAME_OVER as i32
                )
        }));
    }

    #[test]
    fn play_to_finish_settles_by_collected_scores() {
        let handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let _ = handler.handle_start(&mut room, 1);
        let state = handler.state("room").expect("tractor state");
        {
            let mut s = state.lock().unwrap();
            s.phase = share_type_public::TractorPhase::Play;
            s.hands.insert(0, vec![4]);
            s.hands.insert(1, vec![13]);
            s.hands.insert(2, vec![5]);
            s.hands.insert(3, vec![6]);
            s.bottom_cards = vec![4, 9, 12, 109, 112, 209, 212, 309];
            s.current_position = 0;
        }

        for (session_id, card) in [(1_u64, 4), (2, 13), (3, 5), (4, 6)] {
            let dispatch = handler.handle_play(
                &mut room,
                session_id,
                serde_json::json!({ "cards": [card] }),
            );
            if session_id == 4 {
                let settlement =
                    dispatch
                        .messages
                        .iter()
                        .find_map(|message| match &message.payload {
                            OutboundPayload::Event(event)
                                if event.code == WsCode::GAME_OVER as i32 =>
                            {
                                serde_json::from_value::<WsTractorSettlementEvent>(
                                    event.data.clone(),
                                )
                                .ok()
                            }
                            _ => None,
                        });
                let settlement = settlement.expect("settlement event");
                assert_eq!(settlement.winner_positions, vec![1, 3]);
                assert_eq!(settlement.score, 80);
                assert_eq!(settlement.blood_units, 1);
            }
        }
    }

    #[test]
    fn quit_stops_active_match_and_does_not_block_restart() {
        let mut handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let _ = handler.handle_start(&mut room, 1);
        let old_state = handler.state("room").expect("active tractor state");
        let old_common = Arc::clone(&old_state.lock().unwrap().base);

        let quit_request = ClientRequest {
            route: Routes::QUIT as i32,
            data: Value::Null,
        };
        let mut quit = room
            .handle_common_request(2, &quit_request, handler.game_id(), || {
                handler.build_room_settings()
            })
            .expect("common quit route");
        handler.after_common_request(&mut room, 2, &quit_request, &mut quit);
        assert!(quit.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Response(ws_common::RequestResponse::WithoutData(response))
                    if response.route == Routes::QUIT as i32
                        && response.code as i32 == WsResponseCode::OK as i32
            )
        }));
        assert!(old_common.lock().unwrap().stop_requested());
        assert!(handler.current_state(&room, "room").is_none());

        room.handle_common_request(5, &join_request("u5"), handler.game_id(), || {
            handler.build_room_settings()
        });
        let restarted = handler.handle_start(&mut room, 1);

        assert!(restarted.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Response(ws_common::RequestResponse::WithoutData(response))
                    if response.route == Routes::START as i32
                        && response.code as i32 == WsResponseCode::OK as i32
            )
        }));
        let new_state = handler.state("room").expect("restarted tractor state");
        let new_common = Arc::clone(&new_state.lock().unwrap().base);
        assert!(!Arc::ptr_eq(&old_common, &new_common));
        assert!(!new_common.lock().unwrap().stop_requested());
    }

    #[test]
    fn setting_is_locked_after_start() {
        let handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let _ = handler.handle_start(&mut room, 1);
        let dispatch = room
            .handle_common_request(
                1,
                &ClientRequest {
                    route: Routes::SETTING as i32,
                    data: serde_json::json!({
                        "current_configs": {
                            KEY_DECK_COUNT: 3
                        }
                    }),
                },
                handler.game_id(),
                || handler.build_room_settings(),
            )
            .expect("setting handled by common route");
        let locked = dispatch.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Response(ws_common::RequestResponse::WithoutData(response))
                    if response.route == Routes::SETTING as i32
                        && response.code as i32 == WsResponseCode::ERROR_FORMAT as i32
            )
        });
        assert!(locked);
    }

    #[test]
    fn stale_same_name_state_does_not_block_recreated_room_start() {
        let handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("old-{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let _ = handler.handle_start(&mut room, 1);
        let old_state = handler.state("room").expect("old tractor state");
        let old_common = Arc::clone(&old_state.lock().unwrap().base);

        for session_id in 1..=4 {
            let _ = room.disconnect(session_id);
        }
        assert!(!room.room_exists("room"));
        assert!(old_common.lock().unwrap().stop_requested());

        for session_id in 5..=8 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("new-{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let recreated_common = room
            .room_common_state("room")
            .expect("recreated room common state");
        assert!(!Arc::ptr_eq(&old_common, &recreated_common));

        let dispatch = handler.handle_start(&mut room, 5);

        assert!(dispatch.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Response(ws_common::RequestResponse::WithoutData(response))
                    if response.route == Routes::START as i32
                        && response.code as i32 == WsResponseCode::OK as i32
            )
        }));
        let new_state = handler.state("room").expect("new tractor state");
        let new_common = Arc::clone(&new_state.lock().unwrap().base);
        assert!(Arc::ptr_eq(
            &new_common,
            &room
                .room_common_state("room")
                .expect("current room common state")
        ));
        assert!(!Arc::ptr_eq(&old_common, &new_common));
    }

    #[test]
    fn start_is_rejected_after_match_begins() {
        let handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let _ = handler.handle_start(&mut room, 1);
        let dispatch = handler.handle_start(&mut room, 1);
        let rejected = dispatch.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Response(ws_common::RequestResponse::WithoutData(response))
                    if response.route == Routes::START as i32
                        && response.code as i32 == WsResponseCode::NO_PERMISSION as i32
            )
        });
        assert!(rejected);
    }

    #[test]
    fn start_prepares_incremental_deal_instead_of_exposing_full_hands() {
        let handler = TractorGameHandler::default();
        let mut room = RoomService::default();
        for session_id in 1..=4 {
            room.handle_common_request(
                session_id,
                &join_request(&format!("u{session_id}")),
                handler.game_id(),
                || handler.build_room_settings(),
            );
        }
        let dispatch = handler.handle_start(&mut room, 1);
        let deal_events = dispatch
            .messages
            .iter()
            .filter(|message| {
                matches!(
                    &message.payload,
                    OutboundPayload::Event(event) if event.code == WsCode::DEAL as i32
                )
            })
            .count();
        assert_eq!(deal_events, 0);
        let state = handler.state("room").expect("tractor state");
        let state = state.lock().unwrap();
        assert_eq!(state.phase, share_type_public::TractorPhase::Deal);
        assert!(state.hands.values().all(Vec::is_empty));
        assert!(state.total_deal_count > 0);
        assert_eq!(state.deal_queue.len(), state.total_deal_count);
    }
}

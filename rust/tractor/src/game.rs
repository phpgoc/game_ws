use std::{collections::HashMap, sync::Arc};

use serde_json::{Value, json};
use share_type_public::{
    CommonEvent, GameId, Routes, TractorRank, WsCode, WsResponseCode,
    games::tractor::{
        WsTractorDealEvent, WsTractorPlayEvent, WsTractorPlayRequest, WsTractorSettlementEvent,
    },
};
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RoomService, SessionId,
};

use crate::{
    game_setting::{
        KEY_BLOOD_ENABLED, KEY_BLOOD_SCORE_PER_UNIT, KEY_BLOOD_START_SCORE, KEY_BOTTOM_CARD_COUNT,
        KEY_DECK_COUNT, KEY_PLAY_TIME, KEY_TARGET_RANK, build_tractor_settings,
    },
    game_state::{TractorGameState, TractorRules, TractorStateHandle},
};

type StateRegistry = Arc<std::sync::Mutex<HashMap<String, TractorStateHandle>>>;

pub struct TractorGameHandler {
    states: StateRegistry,
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
            target_rank: match configs.get(KEY_TARGET_RANK).copied().unwrap_or(3) {
                0 => TractorRank::J,
                1 => TractorRank::Q,
                2 => TractorRank::K,
                _ => TractorRank::A,
            },
        }
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
        let Ok(payload) = RoomService::parse::<WsTractorPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(state) = self.state(&room_key) else {
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
                .get_room_configs(&room_key)
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
        room_service.send_all(&room_key, WsCode::PLAY as i32, play_event, &mut dispatch);
        room_service.send_all(
            &room_key,
            WsCode::TABLE_SNAPSHOT as i32,
            snapshot,
            &mut dispatch,
        );
        if finished {
            let settlement = {
                let s = state.lock().unwrap();
                let score = s.settlement_score();
                WsTractorSettlementEvent {
                    winner_positions: s.winner_positions(),
                    score,
                    blood_units: s.rules.blood_units(score),
                    target_rank: s.rules.target_rank,
                }
            };
            crate::official::settle_round(
                room_service,
                &room_key,
                &settlement.winner_positions,
                settlement.score,
                settlement.target_rank,
            );
            room_service.send_all(
                &room_key,
                WsCode::GAME_OVER as i32,
                settlement,
                &mut dispatch,
            );
            self.states.lock().unwrap().remove(&room_key);
            room_service.clear_room_game_state(&room_key);
        }
        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
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
                WsResponseCode::NOT_LOGIN,
            );
        };
        let Some(common) = room_service.reset_room_common_state_for_new_game(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        let configs = room_service.get_room_configs(&room_key).unwrap_or_default();
        let rules = Self::configs_to_rules(&configs);
        let play_time = configs.get(KEY_PLAY_TIME).copied().unwrap_or(30).max(1) as u32;

        let mut game_state = TractorGameState::from_common(Arc::clone(&common));
        if game_state.deal_new_round(rules).is_err() {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        game_state.set_turn_countdown(play_time);
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

        crate::official::create_match(room_service, &room_key);
        room_service.send_all(&room_key, WsCode::START as i32, json!({}), &mut dispatch);
        self.push_private_deals(&room_key, room_service, &state, &mut dispatch);
        self.push_table_snapshot(&room_key, room_service, &state, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
        dispatch
    }

    fn push_private_deals(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &TractorStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let members = room_service.get_room_members(room_key);
        let s = state.lock().unwrap();
        for (session_id, _, position, _) in members {
            let payload = WsTractorDealEvent {
                position: position as i32,
                cards: s.hands.get(&position).cloned().unwrap_or_default(),
                deck_count: s.rules.deck_count as i32,
                hand_count: s.hand_count() as i32,
                bottom_card_count: s.bottom_cards.len() as i32,
                target_rank: s.rules.target_rank,
            };
            dispatch.messages.push(Delivery {
                recipient: session_id,
                payload: OutboundPayload::Event(CommonEvent {
                    code: WsCode::DEAL as i32,
                    data: serde_json::to_value(payload).unwrap_or(Value::Null),
                }),
            });
        }
    }

    fn push_table_snapshot(
        &self,
        room_key: &str,
        room_service: &RoomService,
        state: &TractorStateHandle,
        dispatch: &mut Dispatch,
    ) {
        let snapshot = state.lock().unwrap().snapshot();
        room_service.send_all(room_key, WsCode::TABLE_SNAPSHOT as i32, snapshot, dispatch);
    }

    fn state(&self, room_key: &str) -> Option<TractorStateHandle> {
        self.states.lock().unwrap().get(room_key).cloned()
    }
}

impl Default for TractorGameHandler {
    fn default() -> Self {
        Self {
            states: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for TractorGameHandler {
    fn build_game_state(&self) -> Box<dyn ws_common::game_state::GameState> {
        Box::new(ws_common::game_state::SharedGameState::new())
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
            _ => {
                room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
            }
        }
    }
}

impl ws_common::game_state::GameState for TractorGameStateHandle {
    fn can_accept_players(&self) -> bool {
        self.inner.lock().unwrap().phase == share_type_public::TractorPhase::Start
    }

    fn shared_common_state(&self) -> Arc<std::sync::Mutex<ws_common::game_state::CommonGameState>> {
        Arc::clone(&self.inner.lock().unwrap().base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use share_type_public::WsJoinRequest;

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

    #[test]
    fn start_deals_equal_private_hands() {
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
        let deals: Vec<_> = dispatch
            .messages
            .iter()
            .filter_map(|message| match &message.payload {
                OutboundPayload::Event(event) if event.code == WsCode::DEAL as i32 => {
                    serde_json::from_value::<WsTractorDealEvent>(event.data.clone()).ok()
                }
                _ => None,
            })
            .collect();
        assert_eq!(deals.len(), 4);
        assert!(
            deals
                .iter()
                .all(|deal| deal.hand_count == deals[0].hand_count)
        );
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
}

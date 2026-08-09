use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use share_type_public::{
    DominoesNoPlayableTiles, DominoesPhase, DominoesRoutes, DominoesRule, DominoesWsCode, GameId,
    Routes, WsCode, WsDominoesDealEvent, WsDominoesDrawEvent, WsDominoesDrawnTileEvent,
    WsDominoesGameOverEvent, WsDominoesHandState, WsDominoesPassEvent, WsDominoesPlayEvent,
    WsDominoesPlayRequest, WsDominoesRoundOverEvent, WsDominoesRoundStartEvent,
    WsDominoesTableSnapshotEvent, WsDominoesTurnEvent, WsReJoinResponse, WsResponseCode,
};
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
    SessionId, SharedGameState,
};

use crate::core::{CoreError, DominoesRoundState, RoundResult};
use crate::game_setting::{
    KEY_NO_PLAYABLE_TILES, KEY_RULE, KEY_TARGET_SCORE, build_dominoes_settings,
    no_playable_from_config, rule_from_config, target_from_config,
};
use crate::game_state::DominoesGameState;

type StateHandle = Arc<Mutex<DominoesRoundState>>;

pub struct DominoesGameHandler {
    states: Arc<Mutex<HashMap<String, StateHandle>>>,
}

fn state_matches_common(
    state: &StateHandle,
    common: &Arc<Mutex<ws_common::CommonGameState>>,
) -> bool {
    Arc::ptr_eq(&state.lock().expect("dominoes state lock").base, common)
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

impl DominoesGameHandler {
    fn state_for_room(&self, room_service: &RoomService, room_key: &str) -> Option<StateHandle> {
        let state = self
            .states
            .lock()
            .expect("dominoes registry lock")
            .get(room_key)
            .cloned()?;
        let common = room_service.room_common_state(room_key)?;
        if state_matches_common(&state, &common) {
            Some(state)
        } else {
            self.states
                .lock()
                .expect("dominoes registry lock")
                .remove(room_key);
            None
        }
    }

    fn remove_state(&self, room_key: &str) {
        self.states
            .lock()
            .expect("dominoes registry lock")
            .remove(room_key);
    }

    fn configs(
        room_service: &RoomService,
        room_key: &str,
    ) -> (DominoesRule, DominoesNoPlayableTiles, i32) {
        let configs = room_service.room_configs(room_key).unwrap_or_default();
        (
            rule_from_config(configs.get(KEY_RULE).copied().unwrap_or_default()),
            no_playable_from_config(
                configs
                    .get(KEY_NO_PLAYABLE_TILES)
                    .copied()
                    .unwrap_or_default(),
            ),
            target_from_config(configs.get(KEY_TARGET_SCORE).copied().unwrap_or_default()),
        )
    }

    fn start(&mut self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
        let route = Routes::START as i32;
        if room_service.session_position(session_id) != Some(0) {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        }
        let mut dispatch = Dispatch::default();
        if !room_service.require_room_membership(session_id, route, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        };

        if let Some(state) = self.state_for_room(room_service, &room_key) {
            let phase = state.lock().expect("dominoes state lock").phase;
            match phase {
                DominoesPhase::RoundOver => {
                    let (starter, round, hand_size, boneyard_count) = {
                        let mut state = state.lock().expect("dominoes state lock");
                        let starter = match state.start_next_round() {
                            Ok(starter) => starter,
                            Err(_) => {
                                return room_service.error_response(
                                    session_id,
                                    route,
                                    WsResponseCode::NO_PERMISSION,
                                );
                            }
                        };
                        (
                            starter,
                            state.round,
                            state.hand_size(),
                            state.boneyard.len(),
                        )
                    };
                    self.broadcast_round_start(
                        room_service,
                        &room_key,
                        WsDominoesRoundStartEvent {
                            round,
                            starter_position: starter as i32,
                            hand_size: hand_size as i32,
                            boneyard_count: boneyard_count as i32,
                        },
                        &mut dispatch,
                    );
                    room_service.push_ok_response(&mut dispatch, session_id, route);
                    return dispatch;
                }
                DominoesPhase::Play => {
                    return room_service.error_response(
                        session_id,
                        route,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
                DominoesPhase::GameOver => {
                    self.remove_state(&room_key);
                    room_service.clear_room_game_state(&room_key);
                }
            }
        }
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        }
        let Some(common) = room_service.room_common_state(&room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        };
        let (rule, no_playable_tiles, target_score) = Self::configs(room_service, &room_key);
        let mut round = match DominoesRoundState::new(common, rule, no_playable_tiles, target_score)
        {
            Ok(round) => round,
            Err(_) => {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NOT_IN_RANGE,
                );
            }
        };
        let starter = match round.start_new_game() {
            Ok(starter) => starter,
            Err(_) => {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NOT_IN_RANGE,
                );
            }
        };
        let round = DominoesGameState::new(round);
        let state = Arc::clone(&round.inner);
        room_service.set_room_game_state(&room_key, Box::new(round));
        self.states
            .lock()
            .expect("dominoes registry lock")
            .insert(room_key.clone(), Arc::clone(&state));
        let (round_number, hand_size, boneyard_count) = {
            let state = state.lock().expect("dominoes state lock");
            (state.round, state.hand_size(), state.boneyard.len())
        };
        self.broadcast_round_start(
            room_service,
            &room_key,
            WsDominoesRoundStartEvent {
                round: round_number,
                starter_position: starter as i32,
                hand_size: hand_size as i32,
                boneyard_count: boneyard_count as i32,
            },
            &mut dispatch,
        );
        room_service.broadcast_connected(
            &room_key,
            WsCode::START as i32,
            serde_json::json!({}),
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }

    fn play_tile(
        &self,
        room_service: &mut RoomService,
        session_id: SessionId,
        data: serde_json::Value,
    ) -> Dispatch {
        let route = DominoesRoutes::PLAY_TILE as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Ok(request) = RoomService::parse_payload::<WsDominoesPlayRequest>(data) else {
            return room_service.error_response(session_id, route, WsResponseCode::ERROR_FORMAT);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        };
        let Some(state) = self.state_for_room(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let (placement, score, total_score, round_result, snapshot, hand) = {
            let mut state = state.lock().expect("dominoes state lock");
            let (placement, score, round_result) =
                match state.play_tile(position, request.tile_id, request.endpoint_id) {
                    Ok(result) => result,
                    Err(error) => {
                        return room_service.error_response(session_id, route, map_error(error));
                    }
                };
            let total_score = state.scores.get(&position).copied().unwrap_or_default();
            let snapshot = state.table_snapshot();
            let hand = state.hand_state(position);
            (placement, score, total_score, round_result, snapshot, hand)
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast_connected(
            &room_key,
            DominoesWsCode::PLAY_TILE as i32,
            WsDominoesPlayEvent {
                position: position as i32,
                placement: placement.into(),
                score,
                total_score,
            },
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, route);
        self.broadcast_snapshot_and_turn(room_service, &room_key, &snapshot, &mut dispatch);
        self.send_hand_state(room_service, &room_key, position, hand, &mut dispatch);
        if let Some(result) = round_result {
            self.broadcast_round_result(room_service, &room_key, result, &mut dispatch);
        }
        dispatch
    }

    fn draw_tile(&self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
        let route = DominoesRoutes::DRAW_TILE as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        };
        let Some(state) = self.state_for_room(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let (result, snapshot, hand) = {
            let mut state = state.lock().expect("dominoes state lock");
            let result = match state.draw_tile(position) {
                Ok(result) => result,
                Err(error) => {
                    return room_service.error_response(session_id, route, map_error(error));
                }
            };
            (result, state.table_snapshot(), state.hand_state(position))
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast_connected(
            &room_key,
            DominoesWsCode::DRAW_TILE as i32,
            WsDominoesDrawEvent {
                position: position as i32,
                boneyard_count: snapshot.boneyard_count,
            },
            &mut dispatch,
        );
        if let Some(tile) = result.tile {
            self.send_to_position(
                room_service,
                &room_key,
                position,
                DominoesWsCode::DRAWN_TILE as i32,
                WsDominoesDrawnTileEvent {
                    tile: tile.into(),
                    playable: result.playable,
                },
                &mut dispatch,
            );
        }
        room_service.push_ok_response(&mut dispatch, session_id, route);
        if result.passed {
            room_service.broadcast_connected(
                &room_key,
                DominoesWsCode::PASS as i32,
                WsDominoesPassEvent {
                    position: position as i32,
                    consecutive_passes: snapshot.consecutive_passes,
                },
                &mut dispatch,
            );
        }
        if let Some(round_result) = result.round_result {
            self.broadcast_round_result(room_service, &room_key, round_result, &mut dispatch);
        } else {
            self.broadcast_snapshot_and_turn(room_service, &room_key, &snapshot, &mut dispatch);
            self.send_hand_state(room_service, &room_key, position, hand, &mut dispatch);
        }
        dispatch
    }

    fn pass(&self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
        let route = DominoesRoutes::PASS as i32;
        let Some(position) = room_service.session_position(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_LOGIN);
        };
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        };
        let Some(state) = self.state_for_room(room_service, &room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NO_PERMISSION);
        };
        let (round_result, snapshot, consecutive_passes) = {
            let mut state = state.lock().expect("dominoes state lock");
            let round_result = match state.pass(position) {
                Ok(result) => result,
                Err(error) => {
                    return room_service.error_response(session_id, route, map_error(error));
                }
            };
            (
                round_result,
                state.table_snapshot(),
                state.consecutive_passes as i32,
            )
        };
        let mut dispatch = Dispatch::default();
        room_service.broadcast_connected(
            &room_key,
            DominoesWsCode::PASS as i32,
            WsDominoesPassEvent {
                position: position as i32,
                consecutive_passes,
            },
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, route);
        if let Some(result) = round_result {
            self.broadcast_round_result(room_service, &room_key, result, &mut dispatch);
        } else {
            self.broadcast_snapshot_and_turn(room_service, &room_key, &snapshot, &mut dispatch);
        }
        dispatch
    }

    fn broadcast_round_start(
        &self,
        room_service: &RoomService,
        room_key: &str,
        event: WsDominoesRoundStartEvent,
        dispatch: &mut Dispatch,
    ) {
        room_service.broadcast_connected(
            room_key,
            DominoesWsCode::ROUND_START as i32,
            event.clone(),
            dispatch,
        );
        let Some(state) = self
            .states
            .lock()
            .expect("dominoes registry lock")
            .get(room_key)
            .cloned()
        else {
            return;
        };
        let state = state.lock().expect("dominoes state lock");
        for position in &state.positions {
            self.send_to_position(
                room_service,
                room_key,
                *position,
                DominoesWsCode::DEAL as i32,
                WsDominoesDealEvent {
                    position: *position as i32,
                    hand: state
                        .hands
                        .get(position)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                },
                dispatch,
            );
        }
        self.broadcast_snapshot_and_turn(room_service, room_key, &state.table_snapshot(), dispatch);
        self.send_hand_state(
            room_service,
            room_key,
            event.starter_position as usize,
            state.hand_state(event.starter_position as usize),
            dispatch,
        );
    }

    fn broadcast_snapshot_and_turn(
        &self,
        room_service: &RoomService,
        room_key: &str,
        snapshot: &WsDominoesTableSnapshotEvent,
        dispatch: &mut Dispatch,
    ) {
        room_service.broadcast_connected(
            room_key,
            DominoesWsCode::TABLE_SNAPSHOT as i32,
            snapshot,
            dispatch,
        );
        if snapshot.phase == DominoesPhase::Play {
            room_service.broadcast_connected(
                room_key,
                DominoesWsCode::TURN as i32,
                WsDominoesTurnEvent {
                    position: snapshot.current_position,
                    boneyard_count: snapshot.boneyard_count,
                },
                dispatch,
            );
        }
    }

    fn send_hand_state(
        &self,
        room_service: &RoomService,
        room_key: &str,
        position: usize,
        hand: WsDominoesHandState,
        dispatch: &mut Dispatch,
    ) {
        self.send_to_position(
            room_service,
            room_key,
            position,
            DominoesWsCode::HAND_STATE as i32,
            hand,
            dispatch,
        );
    }

    fn send_to_position<T: serde::Serialize>(
        &self,
        room_service: &RoomService,
        room_key: &str,
        position: usize,
        code: i32,
        payload: T,
        dispatch: &mut Dispatch,
    ) {
        let data = serde_json::to_value(payload).unwrap_or_default();
        for recipient in room_service.connected_session_ids_for_position(room_key, position) {
            dispatch.messages.push(Delivery {
                recipient,
                payload: OutboundPayload::Event(share_type_public::CommonEvent {
                    code,
                    data: data.clone(),
                }),
            });
        }
    }

    fn broadcast_round_result(
        &self,
        room_service: &mut RoomService,
        room_key: &str,
        result: RoundResult,
        dispatch: &mut Dispatch,
    ) {
        let remaining_hands = result
            .remaining_hands
            .into_iter()
            .map(|(position, hand)| (position as i32, hand.into_iter().map(Into::into).collect()))
            .collect();
        room_service.broadcast_connected(
            room_key,
            DominoesWsCode::ROUND_OVER as i32,
            WsDominoesRoundOverEvent {
                round: self
                    .state_for_room(room_service, room_key)
                    .map(|state| state.lock().expect("dominoes state lock").round)
                    .unwrap_or_default(),
                winner_position: result.winner_position as i32,
                blocked: result.blocked,
                round_score: result.round_score,
                scores: result
                    .scores
                    .iter()
                    .map(|(position, score)| (*position as i32, *score))
                    .collect(),
                remaining_hands,
            },
            dispatch,
        );
        if result.game_over {
            room_service.broadcast_connected(
                room_key,
                DominoesWsCode::GAME_OVER as i32,
                WsDominoesGameOverEvent {
                    winner_positions: result
                        .winner_positions
                        .iter()
                        .map(|position| *position as i32)
                        .collect(),
                    target_score: self
                        .state_for_room(room_service, room_key)
                        .map(|state| state.lock().expect("dominoes state lock").target_score)
                        .unwrap_or_default(),
                    scores: result
                        .scores
                        .iter()
                        .map(|(position, score)| (*position as i32, *score))
                        .collect(),
                },
                dispatch,
            );
        }
    }
}

impl Default for DominoesGameHandler {
    fn default() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for DominoesGameHandler {
    fn authorize_join(
        &self,
        join: &share_type_public::WsJoinRequest,
    ) -> ws_common::JoinAuthorizationFuture {
        #[cfg(feature = "official")]
        {
            return Box::pin(crate::official::authorize_join(join.session_id.clone()));
        }
        #[cfg(not(feature = "official"))]
        {
            let _ = join;
            Box::pin(async { ws_common::JoinAuthorization::ALLOW_NONMEMBER })
        }
    }

    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        if request.route == Routes::QUIT as i32 || request.route == Routes::DISBAND as i32 {
            if let Some(room_key) = room_service.room_key_of(session_id) {
                self.remove_state(&room_key);
            }
            return;
        }
        if request.route != Routes::JOIN as i32 || !join_succeeded(dispatch, session_id) {
            return;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return;
        };
        let Some(state) = self.state_for_room(room_service, &room_key) else {
            return;
        };
        let position = room_service
            .session_position(session_id)
            .unwrap_or_default();
        let (table, hand) = {
            let state = state.lock().expect("dominoes state lock");
            (state.table_snapshot(), state.hand_state(position))
        };
        let rejoin = WsReJoinResponse {
            other_cards_numbers: HashMap::new(),
            player_scores: HashMap::new(),
            my_cards: Vec::new(),
            now_playing: table.current_position,
            phase: table.phase as i32,
            landlord_position: None,
            score: 0,
            hidden_cards: Vec::new(),
            last_play_position: table.last_play_position,
            last_play: Vec::new(),
            dominoes: Some(share_type_public::WsDominoesReJoinResponse { table, hand }),
        };
        for message in &mut dispatch.messages {
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
                    serde_json::to_value(rejoin.clone()).unwrap_or_default();
            }
        }
    }

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_dominoes_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::DOMINOES
    }

    fn handle_game_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        match request.route {
            route if route == Routes::START as i32 => self.start(room_service, session_id),
            route if route == DominoesRoutes::PLAY_TILE as i32 => {
                self.play_tile(room_service, session_id, request.data)
            }
            route if route == DominoesRoutes::DRAW_TILE as i32 => {
                self.draw_tile(room_service, session_id)
            }
            route if route == DominoesRoutes::PASS as i32 => self.pass(room_service, session_id),
            _ => {
                room_service.error_response(session_id, request.route, WsResponseCode::NOT_IN_RANGE)
            }
        }
    }
}

fn map_error(error: CoreError) -> WsResponseCode {
    match error {
        CoreError::InvalidTile | CoreError::InvalidEndpoint | CoreError::TileDoesNotMatch => {
            WsResponseCode::ERROR_FORMAT
        }
        _ => WsResponseCode::NO_PERMISSION,
    }
}

#[cfg(test)]
#[path = "game/tests.rs"]
mod tests;

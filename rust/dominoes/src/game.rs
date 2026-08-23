use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use share_type_public::{
    DominoesActionSource, DominoesNoPlayableTiles, DominoesPhase, DominoesRoutes, DominoesRule,
    DominoesWsCode, GameId, Routes, WsCode, WsDominoesDealEvent, WsDominoesDrawEvent,
    WsDominoesDrawnTileEvent, WsDominoesGameOverEvent, WsDominoesHandState, WsDominoesPassEvent,
    WsDominoesPlayEvent, WsDominoesPlayRequest, WsDominoesRoundOverEvent,
    WsDominoesRoundStartEvent, WsDominoesTableSnapshotEvent, WsDominoesTurnEvent, WsReJoinResponse,
    WsResponseCode,
};
use tokio::sync::Mutex as AsyncMutex;
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
    SessionId, SessionSenders, SharedGameState,
};

use crate::action::{self, ActionEvent, ActionOutcome};
use crate::core::{CoreError, DominoesRoundState, RoundResult};
use crate::game_loop::start_game_loop;
use crate::game_setting::{
    KEY_NO_PLAYABLE_TILES, KEY_RULE, KEY_SETTLEMENT_TIME, KEY_TARGET_SCORE,
    build_dominoes_settings, no_playable_from_config, rule_from_config,
    settlement_time_from_config, target_from_config,
};
use crate::game_state::DominoesGameState;

pub(crate) type StateHandle = Arc<Mutex<DominoesRoundState>>;
pub(crate) type StateRegistry = Arc<Mutex<HashMap<String, StateHandle>>>;

pub struct DominoesGameHandler {
    states: StateRegistry,
    senders: Option<SessionSenders>,
    room_service: Option<Arc<AsyncMutex<RoomService>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct RoundStartBundle {
    pub event: WsDominoesRoundStartEvent,
    pub deals: Vec<(usize, WsDominoesDealEvent)>,
    pub snapshot: WsDominoesTableSnapshotEvent,
    pub hands: Vec<(usize, WsDominoesHandState)>,
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
            self.remove_state_if_same(room_key, &state);
            None
        }
    }

    fn remove_state_if_same(&self, room_key: &str, expected: &StateHandle) {
        let mut states = self.states.lock().expect("dominoes registry lock");
        if states
            .get(room_key)
            .is_some_and(|state| Arc::ptr_eq(state, expected))
        {
            states.remove(room_key);
        }
    }

    fn prune_stale_states(&self, room_service: &RoomService) {
        self.states
            .lock()
            .expect("dominoes registry lock")
            .retain(|room_key, state| {
                room_service
                    .room_common_state(room_key)
                    .is_some_and(|common| state_matches_common(state, &common))
                    && !state
                        .lock()
                        .expect("dominoes state lock")
                        .base
                        .lock()
                        .expect("dominoes common state lock")
                        .stop_requested
            });
    }

    fn configs(
        room_service: &RoomService,
        room_key: &str,
    ) -> (DominoesRule, DominoesNoPlayableTiles, i32, u32) {
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
            settlement_time_from_config(
                configs
                    .get(KEY_SETTLEMENT_TIME)
                    .copied()
                    .unwrap_or(4),
            ),
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
            if phase != DominoesPhase::GameOver {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            self.remove_state_if_same(&room_key, &state);
            room_service.clear_room_game_state(&room_key);
        }
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        }
        let Some(common) = room_service.room_common_state(&room_key) else {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        };
        let (rule, no_playable_tiles, target_score, settlement_time) =
            Self::configs(room_service, &room_key);
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
        if round.start_new_game().is_err() {
            return room_service.error_response(session_id, route, WsResponseCode::NOT_IN_RANGE);
        }
        round.set_settlement_time_seconds(settlement_time);
        let round = DominoesGameState::new(round);
        let state = Arc::clone(&round.inner);
        room_service.set_room_game_state(&room_key, Box::new(round));
        self.states
            .lock()
            .expect("dominoes registry lock")
            .insert(room_key.clone(), Arc::clone(&state));
        crate::official::create_match(room_service, &room_key);

        if let (Some(room_service_arc), Some(senders)) =
            (self.room_service.as_ref(), self.senders.as_ref())
        {
            start_game_loop(
                room_key.clone(),
                Arc::clone(&state),
                Arc::clone(room_service_arc),
                Arc::clone(senders),
                Arc::clone(&self.states),
            );
        }

        let bundle = round_start_bundle(&state.lock().expect("dominoes state lock"));
        append_round_start_bundle(room_service, &room_key, bundle, &mut dispatch);
        room_service.broadcast_connected(
            &room_key,
            WsCode::START as i32,
            serde_json::json!({}),
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }

    fn human_action_allowed(state: &DominoesRoundState, position: usize) -> bool {
        let common = state.base.lock().expect("dominoes common state lock");
        !common.is_ai_position(position)
            && !common.is_ai_takeover_position(position)
            && !common.is_away(position)
            && !common.is_disconnected(position)
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
        let outcome = {
            let mut state = state.lock().expect("dominoes state lock");
            if !Self::human_action_allowed(&state, position) {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            match action::play(
                &mut state,
                position,
                request.tile_id,
                request.endpoint_id,
                DominoesActionSource::Human,
            ) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return room_service.error_response(session_id, route, map_error(error));
                }
            }
        };
        let mut dispatch = Dispatch::default();
        append_action_outcome(room_service, &room_key, outcome, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, route);
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
        let outcome = {
            let mut state = state.lock().expect("dominoes state lock");
            if !Self::human_action_allowed(&state, position) {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            match action::draw(&mut state, position, DominoesActionSource::Human) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return room_service.error_response(session_id, route, map_error(error));
                }
            }
        };
        let mut dispatch = Dispatch::default();
        append_action_outcome(room_service, &room_key, outcome, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, route);
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
        let outcome = {
            let mut state = state.lock().expect("dominoes state lock");
            if !Self::human_action_allowed(&state, position) {
                return room_service.error_response(
                    session_id,
                    route,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            match action::pass(&mut state, position, DominoesActionSource::Human) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return room_service.error_response(session_id, route, map_error(error));
                }
            }
        };
        let mut dispatch = Dispatch::default();
        append_action_outcome(room_service, &room_key, outcome, &mut dispatch);
        room_service.push_ok_response(&mut dispatch, session_id, route);
        dispatch
    }

    fn append_current_snapshot(
        &self,
        room_service: &RoomService,
        room_key: &str,
        dispatch: &mut Dispatch,
    ) {
        let Some(state) = self.state_for_room(room_service, room_key) else {
            return;
        };
        let (snapshot, hands) = {
            let state = state.lock().expect("dominoes state lock");
            (
                state.table_snapshot(),
                state
                    .positions
                    .iter()
                    .map(|position| (*position, state.hand_state(*position)))
                    .collect(),
            )
        };
        append_snapshot_and_turn(room_service, room_key, &snapshot, dispatch);
        append_hand_states(room_service, room_key, hands, dispatch);
    }
}

impl Default for DominoesGameHandler {
    fn default() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
            senders: None,
            room_service: None,
        }
    }
}

impl GameHandler for DominoesGameHandler {
    fn supports_ai_players(&self) -> bool {
        true
    }

    fn authorize_join(
        &self,
        join: &share_type_public::WsJoinRequest,
    ) -> ws_common::JoinAuthorizationFuture {
        #[cfg(feature = "official")]
        {
            Box::pin(crate::official::authorize_join(join.session_id.clone()))
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
        if matches!(
            request.route,
            route if route == Routes::QUIT as i32 || route == Routes::DISBAND as i32
        ) {
            self.prune_stale_states(room_service);
            return;
        }

        if request.route == Routes::JOIN as i32 && join_succeeded(dispatch, session_id) {
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
            return;
        }

        if matches!(
            request.route,
            route if route == Routes::AWAY as i32
                || route == Routes::BACK as i32
                || route == Routes::PAUSE as i32
                || route == Routes::RESUME as i32
        ) && let Some(room_key) = room_service.room_key_of(session_id)
        {
            self.append_current_snapshot(room_service, &room_key, dispatch);
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

    fn set_context(&mut self, senders: SessionSenders, room_service: Arc<AsyncMutex<RoomService>>) {
        self.senders = Some(senders);
        self.room_service = Some(room_service);
    }
}

pub(crate) fn round_start_bundle(state: &DominoesRoundState) -> RoundStartBundle {
    RoundStartBundle {
        event: WsDominoesRoundStartEvent {
            round: state.round,
            starter_position: state.current_position as i32,
            hand_size: state.hand_size() as i32,
            boneyard_count: state.boneyard.len() as i32,
            remaining_seconds: state.remaining_seconds as i32,
            turn_revision: state.turn_revision,
        },
        deals: state
            .positions
            .iter()
            .map(|position| {
                (
                    *position,
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
                )
            })
            .collect(),
        snapshot: state.table_snapshot(),
        hands: state
            .positions
            .iter()
            .map(|position| (*position, state.hand_state(*position)))
            .collect(),
    }
}

pub(crate) fn append_round_start_bundle(
    room_service: &RoomService,
    room_key: &str,
    bundle: RoundStartBundle,
    dispatch: &mut Dispatch,
) {
    room_service.broadcast_connected(
        room_key,
        DominoesWsCode::ROUND_START as i32,
        bundle.event,
        dispatch,
    );
    for (position, deal) in bundle.deals {
        send_to_position(
            room_service,
            room_key,
            position,
            DominoesWsCode::DEAL as i32,
            deal,
            dispatch,
        );
    }
    append_snapshot_and_turn(room_service, room_key, &bundle.snapshot, dispatch);
    append_hand_states(room_service, room_key, bundle.hands, dispatch);
}

pub(crate) fn append_action_outcome(
    room_service: &RoomService,
    room_key: &str,
    outcome: ActionOutcome,
    dispatch: &mut Dispatch,
) {
    for event in outcome.events {
        match event {
            ActionEvent::Play {
                position,
                placement,
                score,
                total_score,
                source,
            } => room_service.broadcast_connected(
                room_key,
                DominoesWsCode::PLAY_TILE as i32,
                WsDominoesPlayEvent {
                    position: position as i32,
                    placement: placement.into(),
                    score,
                    total_score,
                    source,
                },
                dispatch,
            ),
            ActionEvent::Draw {
                position,
                boneyard_count,
                tile,
                playable,
                source,
            } => {
                room_service.broadcast_connected(
                    room_key,
                    DominoesWsCode::DRAW_TILE as i32,
                    WsDominoesDrawEvent {
                        position: position as i32,
                        boneyard_count: boneyard_count as i32,
                        source,
                    },
                    dispatch,
                );
                if let Some(tile) = tile {
                    send_to_position(
                        room_service,
                        room_key,
                        position,
                        DominoesWsCode::DRAWN_TILE as i32,
                        WsDominoesDrawnTileEvent {
                            tile: tile.into(),
                            playable,
                        },
                        dispatch,
                    );
                }
            }
            ActionEvent::Pass {
                position,
                consecutive_passes,
                source,
            } => room_service.broadcast_connected(
                room_key,
                DominoesWsCode::PASS as i32,
                WsDominoesPassEvent {
                    position: position as i32,
                    consecutive_passes: consecutive_passes as i32,
                    source,
                },
                dispatch,
            ),
        }
    }
    append_snapshot_and_turn(room_service, room_key, &outcome.snapshot, dispatch);
    append_hand_states(room_service, room_key, outcome.hands, dispatch);
    if let Some(result) = outcome.round_result {
        append_round_result(room_service, room_key, result, &outcome.snapshot, dispatch);
    }
}

pub(crate) fn append_snapshot_and_turn(
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
                remaining_seconds: snapshot.remaining_seconds,
                turn_revision: snapshot.turn_revision,
            },
            dispatch,
        );
    }
}

fn append_hand_states(
    room_service: &RoomService,
    room_key: &str,
    hands: Vec<(usize, WsDominoesHandState)>,
    dispatch: &mut Dispatch,
) {
    for (position, hand) in hands {
        send_to_position(
            room_service,
            room_key,
            position,
            DominoesWsCode::HAND_STATE as i32,
            hand,
            dispatch,
        );
    }
}

fn send_to_position<T: serde::Serialize>(
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

fn append_round_result(
    room_service: &RoomService,
    room_key: &str,
    result: RoundResult,
    snapshot: &WsDominoesTableSnapshotEvent,
    dispatch: &mut Dispatch,
) {
    crate::official::settle_round(
        room_service,
        room_key,
        snapshot.round,
        snapshot.rule,
        &result,
    );
    let remaining_hands = result
        .remaining_hands
        .iter()
        .map(|(position, hand)| {
            (
                *position as i32,
                hand.iter().copied().map(Into::into).collect(),
            )
        })
        .collect();
    room_service.broadcast_connected(
        room_key,
        DominoesWsCode::ROUND_OVER as i32,
        WsDominoesRoundOverEvent {
            round: snapshot.round,
            winner_position: result.winner_position as i32,
            blocked: result.blocked,
            round_score: result.round_score,
            scores: result
                .scores
                .iter()
                .map(|(position, score)| (*position as i32, *score))
                .collect(),
            remaining_hands,
            remaining_seconds: snapshot.remaining_seconds,
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
                target_score: snapshot.target_score,
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

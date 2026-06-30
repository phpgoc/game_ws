use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};
use share_type_public::games::shenyang_mahjong::{
    ShenyangMahjongAction, ShenyangMahjongMeldKind, ShenyangMahjongPhase,
    WsShenyangMahjongClaimWindowEvent, WsShenyangMahjongDealEvent, WsShenyangMahjongPlayEvent,
    WsShenyangMahjongPlayRequest, WsShenyangMahjongPlayerSnapshot,
    WsShenyangMahjongSettlementEvent,
};
use share_type_public::{GameId, Routes, WsCode, WsResponseCode};
use tokio::sync::Mutex;
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
    SessionId, SessionSenders, game_state::SharedGameState,
};

use crate::game_loop::start_game_loop;
use crate::game_setting::build_shenyang_mahjong_settings;
use crate::game_state::{
    ClaimResponse, ClaimWindowState, ShenyangMahjongGameState, ShenyangMahjongLoopState, build_meld,
};
use crate::rules::{can_chi, can_peng, is_standard_win, tiles_in_hand};

pub(crate) type LoopStateHandle = Arc<std::sync::Mutex<ShenyangMahjongLoopState>>;
pub(crate) type LoopStateRegistry = Arc<std::sync::Mutex<HashMap<String, LoopStateHandle>>>;

pub struct ShenyangMahjongGameHandler {
    room_service: Option<Arc<Mutex<RoomService>>>,
    senders: Option<SessionSenders>,
    loop_states: LoopStateRegistry,
}

pub(crate) fn advance_to_next_turn(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) {
    let next_position = state.next_position(state.current_position);
    if let Some(tile) = state.draw_for_position(next_position) {
        state.set_turn_countdown(current_play_time(configs));
        push_draw_events(room_service, room_key, state, dispatch, next_position, tile);
        push_phase_change(
            room_service,
            room_key,
            dispatch,
            state.phase,
            state.current_position,
        );
        return;
    }

    state.enter_settlement(Vec::new(), None, None, false);
    push_phase_change(
        room_service,
        room_key,
        dispatch,
        ShenyangMahjongPhase::Settlement,
        state.current_position,
    );
    if let Some(event) = build_settlement_event(state) {
        push_room_event(
            room_service,
            room_key,
            dispatch,
            WsCode::GAME_OVER as i32,
            event,
        );
    }
}

pub(crate) fn allow_multi_hu(configs: &HashMap<String, i32>) -> bool {
    config_value(configs, "multi_hu_mode", 1) == 1
}

pub(crate) fn build_settlement_event(
    state: &ShenyangMahjongLoopState,
) -> Option<WsShenyangMahjongSettlementEvent> {
    let settlement = state.settlement.as_ref()?;
    let players = state.players_snapshot();
    let mut snapshots = Vec::new();
    let mut positions: Vec<usize> = players.keys().copied().collect();
    positions.sort_unstable();

    for position in positions {
        let (_, name) = players.get(&position).cloned().unwrap_or_default();
        let mut hand_tiles = state.hands.get(&position).cloned().unwrap_or_default();
        if !settlement.is_self_draw
            && settlement.winner_positions.contains(&position)
            && let Some(tile) = settlement.win_tile
        {
            hand_tiles.push(tile);
            hand_tiles.sort_unstable();
        }
        snapshots.push(WsShenyangMahjongPlayerSnapshot {
            position: position as i32,
            name,
            hand_tiles,
            discards: state.discards.get(&position).cloned().unwrap_or_default(),
            melds: state.melds.get(&position).cloned().unwrap_or_default(),
        });
    }

    Some(WsShenyangMahjongSettlementEvent {
        winner_positions: settlement
            .winner_positions
            .iter()
            .map(|position| *position as i32)
            .collect(),
        from_position: settlement.from_position.map(|position| position as i32),
        win_tile: settlement.win_tile,
        is_self_draw: settlement.is_self_draw,
        players: snapshots,
    })
}

fn config_value(configs: &HashMap<String, i32>, key: &str, fallback: i32) -> i32 {
    configs.get(key).copied().unwrap_or(fallback)
}

pub(crate) fn current_claim_time(configs: &HashMap<String, i32>) -> u32 {
    config_value(configs, "claim_time", 5).max(1) as u32
}

pub(crate) fn current_play_time(configs: &HashMap<String, i32>) -> u32 {
    config_value(configs, "play_time", 20).max(1) as u32
}

pub(crate) fn determine_claim_eligible_positions(
    state: &ShenyangMahjongLoopState,
    tile: i32,
    from_position: usize,
) -> Vec<usize> {
    let mut positions: Vec<usize> = state.players_snapshot().keys().copied().collect();
    positions.sort_unstable();
    let next_position = state.next_position(from_position);
    let mut eligible = Vec::new();
    for position in positions {
        if position == from_position {
            continue;
        }
        let hand = state.hands.get(&position).cloned().unwrap_or_default();
        let can_hu = {
            let mut test = hand.clone();
            test.push(tile);
            test.sort_unstable();
            is_standard_win(&test)
        };
        let can_peng_now = can_peng(&hand, tile);
        let can_chi_now = position == next_position
            && ([
                [tile - 2, tile - 1],
                [tile - 1, tile + 1],
                [tile + 1, tile + 2],
            ]
            .into_iter()
            .any(|sequence| can_chi(&hand, tile, &sequence)));
        if can_hu || can_peng_now || can_chi_now {
            eligible.push(position);
        }
    }
    eligible
}

fn join_succeeded(dispatch: &Dispatch, session_id: SessionId) -> bool {
    dispatch.messages.iter().any(|message| {
        if message.recipient != session_id {
            return false;
        }
        matches!(
            &message.payload,
            OutboundPayload::Response(RequestResponse::WithData(response))
                if response.route == Routes::JOIN as i32
                    && response.code as i32 == WsResponseCode::JOINED as i32
        )
    })
}

pub(crate) fn push_direct_event<T: serde::Serialize>(
    dispatch: &mut Dispatch,
    session_id: SessionId,
    code: i32,
    payload: T,
) {
    dispatch.messages.push(Delivery {
        recipient: session_id,
        payload: OutboundPayload::Event(share_type_public::CommonEvent {
            code,
            data: serde_json::to_value(payload).unwrap_or(Value::Null),
        }),
    });
}

pub(crate) fn push_draw_events(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    position: usize,
    tile: i32,
) {
    let name = state.player_name(position);
    for (session_id, _, member_position, _) in room_service.get_room_members(room_key) {
        let tiles = if member_position == position {
            vec![tile]
        } else {
            Vec::new()
        };
        push_direct_event(
            dispatch,
            session_id,
            WsCode::PLAY as i32,
            WsShenyangMahjongPlayEvent {
                name: name.clone(),
                position: position as i32,
                action: ShenyangMahjongAction::DRAW,
                tiles,
                target_tile: Some(tile),
                from_position: None,
                wall_count: state.wall_count() as i32,
            },
        );
    }
}

pub(crate) fn push_phase_change(
    room_service: &RoomService,
    room_key: &str,
    dispatch: &mut Dispatch,
    phase: ShenyangMahjongPhase,
    current_position: usize,
) {
    push_room_event(
        room_service,
        room_key,
        dispatch,
        WsCode::CHANGE_PHASE as i32,
        json!({
            "phase": phase as i32,
            "position": current_position as i32,
        }),
    );
}

pub(crate) fn push_private_deal_events(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
) {
    for (session_id, _, position, _) in room_service.get_room_members(room_key) {
        let my_tiles = state.hands.get(&position).cloned().unwrap_or_default();
        push_direct_event(
            dispatch,
            session_id,
            WsCode::DEAL as i32,
            WsShenyangMahjongDealEvent {
                my_tiles,
                dealer_position: state.dealer_position as i32,
                current_position: state.current_position as i32,
                wall_count: state.wall_count() as i32,
            },
        );
    }
}

pub(crate) fn push_room_event<T: serde::Serialize>(
    room_service: &RoomService,
    room_key: &str,
    dispatch: &mut Dispatch,
    code: i32,
    payload: T,
) {
    room_service.send_all(room_key, code, payload, dispatch);
}

pub(crate) fn resolve_claim_window(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) {
    let Some(claim_window) = state.claim_window.clone() else {
        return;
    };
    let mut hu_positions = Vec::new();
    let mut peng_positions = Vec::new();
    let mut chi_positions = Vec::new();
    let ordered_positions = {
        let mut ordered = Vec::new();
        let mut cursor = state.next_position(claim_window.from_position);
        for _ in 0..state.players_snapshot().len() {
            ordered.push(cursor);
            cursor = state.next_position(cursor);
        }
        ordered
    };

    for position in &claim_window.eligible_positions {
        match claim_window.responses.get(position) {
            Some(ClaimResponse::Hu) => hu_positions.push(*position),
            Some(ClaimResponse::Peng) => peng_positions.push(*position),
            Some(ClaimResponse::Chi { consume_tiles }) => {
                chi_positions.push((*position, consume_tiles.clone()));
            }
            _ => {}
        }
    }

    if !hu_positions.is_empty() {
        let chosen_hu = ordered_positions
            .iter()
            .copied()
            .find(|position| hu_positions.contains(position));
        state.remove_last_discard(claim_window.from_position);
        let winners = if allow_multi_hu(configs) {
            let mut winners = hu_positions.clone();
            winners.sort_by_key(|position| {
                ordered_positions
                    .iter()
                    .position(|item| item == position)
                    .unwrap_or(usize::MAX)
            });
            winners
        } else {
            vec![chosen_hu.unwrap_or(hu_positions[0])]
        };
        for winner in &winners {
            push_room_event(
                room_service,
                room_key,
                dispatch,
                WsCode::PLAY as i32,
                WsShenyangMahjongPlayEvent {
                    name: state.player_name(*winner),
                    position: *winner as i32,
                    action: ShenyangMahjongAction::HU,
                    tiles: Vec::new(),
                    target_tile: Some(claim_window.tile),
                    from_position: Some(claim_window.from_position as i32),
                    wall_count: state.wall_count() as i32,
                },
            );
        }
        state.enter_settlement(
            winners,
            Some(claim_window.from_position),
            Some(claim_window.tile),
            false,
        );
        push_phase_change(
            room_service,
            room_key,
            dispatch,
            ShenyangMahjongPhase::Settlement,
            state.current_position,
        );
        if let Some(event) = build_settlement_event(state) {
            push_room_event(
                room_service,
                room_key,
                dispatch,
                WsCode::GAME_OVER as i32,
                event,
            );
        }
        return;
    }

    if let Some(winner) = ordered_positions
        .iter()
        .copied()
        .find(|position| peng_positions.contains(position))
    {
        if state.remove_tiles_from_hand(winner, &[claim_window.tile, claim_window.tile]) {
            state.remove_last_discard(claim_window.from_position);
            state.melds.entry(winner).or_default().push(build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![claim_window.tile, claim_window.tile, claim_window.tile],
                Some(claim_window.from_position),
            ));
            state.current_position = winner;
            state.last_drawn_tile = None;
            state.claim_window = None;
            state.set_turn_countdown(current_play_time(configs));
            push_room_event(
                room_service,
                room_key,
                dispatch,
                WsCode::PLAY as i32,
                WsShenyangMahjongPlayEvent {
                    name: state.player_name(winner),
                    position: winner as i32,
                    action: ShenyangMahjongAction::PENG,
                    tiles: vec![claim_window.tile, claim_window.tile],
                    target_tile: Some(claim_window.tile),
                    from_position: Some(claim_window.from_position as i32),
                    wall_count: state.wall_count() as i32,
                },
            );
            push_phase_change(
                room_service,
                room_key,
                dispatch,
                state.phase,
                state.current_position,
            );
        }
        return;
    }

    if let Some((winner, consume_tiles)) = chi_positions
        .into_iter()
        .find(|(position, _)| *position == state.next_position(claim_window.from_position))
    {
        if state.remove_tiles_from_hand(winner, &consume_tiles) {
            state.remove_last_discard(claim_window.from_position);
            let mut meld_tiles = consume_tiles.clone();
            meld_tiles.push(claim_window.tile);
            state.melds.entry(winner).or_default().push(build_meld(
                ShenyangMahjongMeldKind::CHI,
                meld_tiles.clone(),
                Some(claim_window.from_position),
            ));
            state.current_position = winner;
            state.last_drawn_tile = None;
            state.claim_window = None;
            state.set_turn_countdown(current_play_time(configs));
            push_room_event(
                room_service,
                room_key,
                dispatch,
                WsCode::PLAY as i32,
                WsShenyangMahjongPlayEvent {
                    name: state.player_name(winner),
                    position: winner as i32,
                    action: ShenyangMahjongAction::CHI,
                    tiles: consume_tiles,
                    target_tile: Some(claim_window.tile),
                    from_position: Some(claim_window.from_position as i32),
                    wall_count: state.wall_count() as i32,
                },
            );
            push_phase_change(
                room_service,
                room_key,
                dispatch,
                state.phase,
                state.current_position,
            );
        }
        return;
    }

    state.claim_window = None;
    state.current_position = claim_window.from_position;
    advance_to_next_turn(room_service, room_key, state, configs, dispatch);
}

pub(crate) fn settlement_time(configs: &HashMap<String, i32>) -> u64 {
    config_value(configs, "settlement_time", 5).max(1) as u64
}

pub(crate) fn start_time(configs: &HashMap<String, i32>) -> u64 {
    config_value(configs, "start_time", 1).max(0) as u64
}

impl ShenyangMahjongGameHandler {
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
        let Ok(payload) = RoomService::parse::<WsShenyangMahjongPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(loop_state) = self.loop_state(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let configs = room_service.get_room_configs(&room_key).unwrap_or_default();
        let mut dispatch = Dispatch::default();

        {
            let mut state = loop_state.lock().unwrap();
            if state.phase != ShenyangMahjongPhase::Play {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }

            if state.claim_window.is_some() {
                let (claim_tile, from_position, eligible_positions, already_responded) = {
                    let claim_window = state.claim_window.as_ref().unwrap();
                    (
                        claim_window.tile,
                        claim_window.from_position,
                        claim_window.eligible_positions.clone(),
                        claim_window.responses.contains_key(&position),
                    )
                };
                if !eligible_positions.contains(&position) || already_responded {
                    return room_service.error_response(
                        session_id,
                        Routes::PLAY as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
                let hand = state.hands.get(&position).cloned().unwrap_or_default();
                let response = match payload.action {
                    ShenyangMahjongAction::PASS => ClaimResponse::Pass,
                    ShenyangMahjongAction::HU => {
                        let mut tiles = hand.clone();
                        tiles.push(claim_tile);
                        tiles.sort_unstable();
                        if !is_standard_win(&tiles) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Hu
                    }
                    ShenyangMahjongAction::PENG => {
                        if !can_peng(&hand, claim_tile) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Peng
                    }
                    ShenyangMahjongAction::CHI => {
                        if position != state.next_position(from_position)
                            || !can_chi(&hand, claim_tile, &payload.tiles)
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Chi {
                            consume_tiles: payload.tiles.clone(),
                        }
                    }
                    _ => {
                        return room_service.error_response(
                            session_id,
                            Routes::PLAY as i32,
                            WsResponseCode::NO_PERMISSION,
                        );
                    }
                };

                let all_received = {
                    let claim_window = state.claim_window.as_mut().unwrap();
                    claim_window.responses.insert(position, response);
                    claim_window
                        .eligible_positions
                        .iter()
                        .all(|item| claim_window.responses.contains_key(item))
                };
                state.set_action_received(true);
                if all_received {
                    resolve_claim_window(
                        room_service,
                        &room_key,
                        &mut state,
                        &configs,
                        &mut dispatch,
                    );
                }
            } else {
                if state.current_position != position {
                    return room_service.error_response(
                        session_id,
                        Routes::PLAY as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }

                match payload.action {
                    ShenyangMahjongAction::DISCARD => {
                        let tile = payload
                            .target_tile
                            .or_else(|| payload.tiles.first().copied())
                            .unwrap_or_default();
                        let Some(hand) = state.hands.get(&position).cloned() else {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        };
                        if hand.len() % 3 != 2 || !tiles_in_hand(&hand, &[tile]) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        if !state.remove_tiles_from_hand(position, &[tile]) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        state.discards.entry(position).or_default().push(tile);
                        state.last_drawn_tile = None;
                        push_room_event(
                            room_service,
                            &room_key,
                            &mut dispatch,
                            WsCode::PLAY as i32,
                            WsShenyangMahjongPlayEvent {
                                name: state.player_name(position),
                                position: position as i32,
                                action: ShenyangMahjongAction::DISCARD,
                                tiles: vec![tile],
                                target_tile: Some(tile),
                                from_position: None,
                                wall_count: state.wall_count() as i32,
                            },
                        );

                        let eligible_positions =
                            determine_claim_eligible_positions(&state, tile, position);
                        if eligible_positions.is_empty() {
                            advance_to_next_turn(
                                room_service,
                                &room_key,
                                &mut state,
                                &configs,
                                &mut dispatch,
                            );
                        } else {
                            state.claim_window = Some(ClaimWindowState {
                                tile,
                                from_position: position,
                                eligible_positions: eligible_positions.clone(),
                                responses: HashMap::new(),
                            });
                            state.set_turn_countdown(current_claim_time(&configs));
                            push_room_event(
                                room_service,
                                &room_key,
                                &mut dispatch,
                                WsCode::CLAIM_WINDOW as i32,
                                WsShenyangMahjongClaimWindowEvent {
                                    tile,
                                    from_position: position as i32,
                                    eligible_positions: eligible_positions
                                        .iter()
                                        .map(|item| *item as i32)
                                        .collect(),
                                    seconds: current_claim_time(&configs) as i32,
                                },
                            );
                        }
                    }
                    ShenyangMahjongAction::HU => {
                        let hand = state.hands.get(&position).cloned().unwrap_or_default();
                        if !is_standard_win(&hand) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        push_room_event(
                            room_service,
                            &room_key,
                            &mut dispatch,
                            WsCode::PLAY as i32,
                            WsShenyangMahjongPlayEvent {
                                name: state.player_name(position),
                                position: position as i32,
                                action: ShenyangMahjongAction::HU,
                                tiles: Vec::new(),
                                target_tile: state.last_drawn_tile,
                                from_position: None,
                                wall_count: state.wall_count() as i32,
                            },
                        );
                        let win_tile = state.last_drawn_tile;
                        state.enter_settlement(vec![position], None, win_tile, true);
                        push_phase_change(
                            room_service,
                            &room_key,
                            &mut dispatch,
                            ShenyangMahjongPhase::Settlement,
                            state.current_position,
                        );
                        if let Some(event) = build_settlement_event(&state) {
                            push_room_event(
                                room_service,
                                &room_key,
                                &mut dispatch,
                                WsCode::GAME_OVER as i32,
                                event,
                            );
                        }
                    }
                    _ => {
                        return room_service.error_response(
                            session_id,
                            Routes::PLAY as i32,
                            WsResponseCode::NO_PERMISSION,
                        );
                    }
                }
            }
        }

        room_service.push_ok_response(&mut dispatch, session_id, Routes::PLAY as i32);
        dispatch
    }

    fn handle_start(&mut self, room_service: &mut RoomService, session_id: SessionId) -> Dispatch {
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
                WsResponseCode::NOT_IN_RANGE,
            );
        };
        let Some(shared_common_state) = room_service.get_room_common_state_handle(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        if let Some(existing) = self.loop_state(&room_key) {
            let same_state = {
                let state = existing.lock().unwrap();
                Arc::ptr_eq(&state.base, &shared_common_state)
            };
            if same_state {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }
            self.loop_states.lock().unwrap().remove(&room_key);
        }

        let loop_state = Arc::new(std::sync::Mutex::new(ShenyangMahjongLoopState::new(
            Arc::clone(&shared_common_state),
        )));
        room_service.set_room_game_state(
            &room_key,
            Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
                &loop_state,
            ))),
        );
        self.loop_states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&loop_state));

        {
            let state = loop_state.lock().unwrap();
            state.set_turn_countdown(0);
        }

        if let (Some(room_service_arc), Some(senders_arc)) =
            (self.room_service.as_ref(), self.senders.as_ref())
        {
            start_game_loop(
                room_key.clone(),
                loop_state,
                Arc::clone(room_service_arc),
                Arc::clone(senders_arc),
                Arc::clone(&self.loop_states),
            );
        }

        room_service.send_all(
            &room_key,
            WsCode::START as i32,
            serde_json::json!({}),
            &mut dispatch,
        );
        room_service.push_ok_response(&mut dispatch, session_id, Routes::START as i32);
        dispatch
    }

    fn loop_state(&self, room_key: &str) -> Option<LoopStateHandle> {
        self.loop_states.lock().unwrap().get(room_key).cloned()
    }

    fn prune_stopped_loop_states(&self) {
        self.loop_states.lock().unwrap().retain(|_, state| {
            let state = state.lock().unwrap();
            !state.stop_requested()
        });
    }
}

impl Default for ShenyangMahjongGameHandler {
    fn default() -> Self {
        Self {
            room_service: None,
            senders: None,
            loop_states: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }
}

impl GameHandler for ShenyangMahjongGameHandler {
    fn game_id(&self) -> GameId {
        GameId::SHENYANG_MAHJONG
    }

    fn after_common_request(
        &mut self,
        _room_service: &mut RoomService,
        _session_id: SessionId,
        request: &ClientRequest,
        _dispatch: &mut Dispatch,
    ) {
        if matches!(request.route, r if r == Routes::QUIT as i32 || r == Routes::DISBAND as i32) {
            self.prune_stopped_loop_states();
        }
        let _ = join_succeeded;
    }

    fn build_game_state(&self) -> Box<dyn ws_common::game_state::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_shenyang_mahjong_settings()
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

    fn set_context(&mut self, senders: SessionSenders, room_service: Arc<Mutex<RoomService>>) {
        self.senders = Some(senders);
        self.room_service = Some(room_service);
    }
}

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};
use share_type_public::games::shenyang_mahjong::{
    ShenyangMahjongAction, ShenyangMahjongMeldKind, ShenyangMahjongPhase,
    WsShenyangMahjongClaimOption, WsShenyangMahjongClaimWindowEvent, WsShenyangMahjongDealEvent,
    WsShenyangMahjongPlayEvent, WsShenyangMahjongPlayRequest, WsShenyangMahjongPlayerSnapshot,
    WsShenyangMahjongPublicPlayerSnapshot, WsShenyangMahjongSettlementEvent,
    WsShenyangMahjongTableSnapshotEvent,
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
use crate::rules::{can_chi, can_gang, can_peng, is_standard_win, tiles_in_hand};

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
            state.turn_countdown(),
        );
        return;
    }

    state.enter_settlement(Vec::new(), None, None, false);
    maybe_record_settlement(room_service, room_key, state);
    push_phase_change(
        room_service,
        room_key,
        dispatch,
        ShenyangMahjongPhase::Settlement,
        state.current_position,
        0,
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

pub(crate) fn build_claim_options(
    state: &ShenyangMahjongLoopState,
    tile: i32,
    from_position: usize,
) -> Vec<WsShenyangMahjongClaimOption> {
    let mut positions: Vec<usize> = state.players_snapshot().keys().copied().collect();
    positions.sort_unstable();
    let next_position = state.next_position(from_position);
    let mut options = Vec::new();

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
        let can_gang_now = can_gang(&hand, tile);
        let chi_options = if position == next_position {
            chi_options_for_hand(&hand, tile)
        } else {
            Vec::new()
        };

        if can_hu || can_peng_now || can_gang_now || !chi_options.is_empty() {
            options.push(WsShenyangMahjongClaimOption {
                position: position as i32,
                can_hu,
                can_peng: can_peng_now,
                can_gang: can_gang_now,
                chi_options,
            });
        }
    }

    options
}

pub(crate) fn build_claim_window_event(
    state: &ShenyangMahjongLoopState,
    tile: i32,
    from_position: usize,
    seconds: i32,
) -> WsShenyangMahjongClaimWindowEvent {
    let options = build_claim_options(state, tile, from_position);
    let eligible_positions = options.iter().map(|option| option.position).collect();
    WsShenyangMahjongClaimWindowEvent {
        tile,
        from_position: from_position as i32,
        eligible_positions,
        seconds,
        options,
    }
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

fn maybe_record_settlement(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
) {
    if let Some(settlement) = state.settlement.as_ref() {
        crate::official::settle_round(room_service, room_key, settlement);
    }
}

pub(crate) fn build_table_snapshot_event(
    state: &ShenyangMahjongLoopState,
    viewer_position: usize,
) -> WsShenyangMahjongTableSnapshotEvent {
    let players = state.players_snapshot();
    let mut positions: Vec<usize> = players.keys().copied().collect();
    positions.sort_unstable();
    let mut snapshots = Vec::new();

    for position in positions {
        let (_, name) = players.get(&position).cloned().unwrap_or_default();
        snapshots.push(WsShenyangMahjongPublicPlayerSnapshot {
            position: position as i32,
            name,
            hand_count: state
                .hands
                .get(&position)
                .map(|hand| hand.len())
                .unwrap_or(0) as i32,
            discards: state.discards.get(&position).cloned().unwrap_or_default(),
            melds: state.melds.get(&position).cloned().unwrap_or_default(),
        });
    }

    WsShenyangMahjongTableSnapshotEvent {
        my_tiles: state
            .hands
            .get(&viewer_position)
            .cloned()
            .unwrap_or_default(),
        players: snapshots,
        phase: state.phase,
        current_position: state.current_position as i32,
        dealer_position: state.dealer_position as i32,
        wall_count: state.wall_count() as i32,
        turn_countdown: state.turn_countdown() as i32,
        claim_window: state.claim_window.as_ref().map(|window| {
            build_claim_window_event(
                state,
                window.tile,
                window.from_position,
                state.turn_countdown() as i32,
            )
        }),
    }
}

fn chi_options_for_hand(hand: &[i32], tile: i32) -> Vec<Vec<i32>> {
    [
        [tile - 2, tile - 1],
        [tile - 1, tile + 1],
        [tile + 1, tile + 2],
    ]
    .into_iter()
    .filter(|sequence| can_chi(hand, tile, sequence))
    .map(|sequence| sequence.to_vec())
    .collect()
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

pub(crate) fn perform_discard(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
    tile: i32,
) -> bool {
    if !state.remove_tiles_from_hand(position, &[tile]) {
        return false;
    }
    state.discards.entry(position).or_default().push(tile);
    state.last_drawn_tile = None;
    push_room_event(
        room_service,
        room_key,
        dispatch,
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

    let claim_seconds = current_claim_time(configs) as i32;
    let claim_event = build_claim_window_event(state, tile, position, claim_seconds);
    let eligible_positions: Vec<usize> = claim_event
        .eligible_positions
        .iter()
        .map(|position| *position as usize)
        .collect();
    if eligible_positions.is_empty() {
        advance_to_next_turn(room_service, room_key, state, configs, dispatch);
    } else {
        state.claim_window = Some(ClaimWindowState {
            tile,
            from_position: position,
            eligible_positions: eligible_positions.clone(),
            responses: HashMap::new(),
        });
        state.set_turn_countdown(current_claim_time(configs));
        push_room_event(
            room_service,
            room_key,
            dispatch,
            WsCode::CLAIM_WINDOW as i32,
            claim_event,
        );
    }
    true
}

pub(crate) fn perform_self_draw_hu(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    position: usize,
) {
    push_room_event(
        room_service,
        room_key,
        dispatch,
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
    maybe_record_settlement(room_service, room_key, state);
    push_phase_change(
        room_service,
        room_key,
        dispatch,
        ShenyangMahjongPhase::Settlement,
        state.current_position,
        0,
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
    _room_service: &RoomService,
    _room_key: &str,
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    position: usize,
    tile: i32,
) {
    let name = state.player_name(position);
    let players = state.players_snapshot();
    for (member_position, (session_id, _)) in players {
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
    turn_countdown: u32,
) {
    push_room_event(
        room_service,
        room_key,
        dispatch,
        WsCode::CHANGE_PHASE as i32,
        json!({
            "phase": phase as i32,
            "position": current_position as i32,
            "turn_countdown": turn_countdown as i32,
        }),
    );
}

pub(crate) fn push_private_deal_events(
    _room_service: &RoomService,
    _room_key: &str,
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
) {
    let players = state.players_snapshot();
    for (position, (session_id, _)) in players {
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
                turn_countdown: state.turn_countdown() as i32,
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
    room_service.send_all_connected(room_key, code, payload, dispatch);
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
    let mut meld_claims = Vec::new();
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
            Some(ClaimResponse::Peng) => meld_claims.push((*position, ClaimResponse::Peng)),
            Some(ClaimResponse::Gang) => meld_claims.push((*position, ClaimResponse::Gang)),
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
        maybe_record_settlement(room_service, room_key, state);
        push_phase_change(
            room_service,
            room_key,
            dispatch,
            ShenyangMahjongPhase::Settlement,
            state.current_position,
            0,
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

    if let Some((winner, claim_response)) = ordered_positions.iter().find_map(|position| {
        meld_claims
            .iter()
            .find(|(claim_position, _)| claim_position == position)
            .cloned()
    }) {
        match claim_response {
            ClaimResponse::Peng => {
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
                        state.turn_countdown(),
                    );
                }
            }
            ClaimResponse::Gang => {
                if state.remove_tiles_from_hand(
                    winner,
                    &[claim_window.tile, claim_window.tile, claim_window.tile],
                ) {
                    state.remove_last_discard(claim_window.from_position);
                    state.melds.entry(winner).or_default().push(build_meld(
                        ShenyangMahjongMeldKind::GANG,
                        vec![
                            claim_window.tile,
                            claim_window.tile,
                            claim_window.tile,
                            claim_window.tile,
                        ],
                        Some(claim_window.from_position),
                    ));
                    state.current_position = winner;
                    state.last_drawn_tile = None;
                    state.claim_window = None;
                    push_room_event(
                        room_service,
                        room_key,
                        dispatch,
                        WsCode::PLAY as i32,
                        WsShenyangMahjongPlayEvent {
                            name: state.player_name(winner),
                            position: winner as i32,
                            action: ShenyangMahjongAction::GANG,
                            tiles: vec![claim_window.tile, claim_window.tile, claim_window.tile],
                            target_tile: Some(claim_window.tile),
                            from_position: Some(claim_window.from_position as i32),
                            wall_count: state.wall_count() as i32,
                        },
                    );
                    if let Some(tile) = state.draw_for_position(winner) {
                        state.set_turn_countdown(current_play_time(configs));
                        push_draw_events(room_service, room_key, state, dispatch, winner, tile);
                        push_phase_change(
                            room_service,
                            room_key,
                            dispatch,
                            state.phase,
                            state.current_position,
                            state.turn_countdown(),
                        );
                    } else {
                        state.enter_settlement(Vec::new(), None, None, false);
                        maybe_record_settlement(room_service, room_key, state);
                        push_phase_change(
                            room_service,
                            room_key,
                            dispatch,
                            ShenyangMahjongPhase::Settlement,
                            state.current_position,
                            0,
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
                }
            }
            _ => {}
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
                state.turn_countdown(),
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
                    ShenyangMahjongAction::GANG => {
                        if !can_gang(&hand, claim_tile) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Gang
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
                        if !tiles_in_hand(&hand, &[tile]) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        if !perform_discard(
                            room_service,
                            &room_key,
                            &mut state,
                            &configs,
                            &mut dispatch,
                            position,
                            tile,
                        ) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
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
                        perform_self_draw_hu(
                            room_service,
                            &room_key,
                            &mut state,
                            &mut dispatch,
                            position,
                        );
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

        crate::official::create_match(room_service, &room_key);
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
    fn after_common_request(
        &mut self,
        room_service: &mut RoomService,
        session_id: SessionId,
        request: &ClientRequest,
        dispatch: &mut Dispatch,
    ) {
        if matches!(request.route, r if r == Routes::QUIT as i32 || r == Routes::DISBAND as i32) {
            self.prune_stopped_loop_states();
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
        let Some(loop_state) = self.loop_state(&room_key) else {
            return;
        };
        let state = loop_state.lock().unwrap();
        if state.phase == ShenyangMahjongPhase::Start {
            return;
        }
        push_direct_event(
            dispatch,
            session_id,
            WsCode::TABLE_SNAPSHOT as i32,
            build_table_snapshot_event(&state, position),
        );
    }

    fn build_game_state(&self) -> Box<dyn ws_common::game_state::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> ws_common::SettingsBuilderResult {
        build_shenyang_mahjong_settings()
    }

    fn game_id(&self) -> GameId {
        GameId::SHENYANG_MAHJONG
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use ws_common::game_state::CommonGameState;

    use super::*;

    #[test]
    fn claim_options_list_concrete_actions() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 2, 3, 3, 3, 4, 11, 12, 13, 21, 22, 23]);
        state
            .hands
            .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31]);
        state
            .hands
            .insert(3, vec![1, 5, 7, 9, 11, 13, 15, 17, 21, 23, 25, 31, 35]);

        let options = build_claim_options(&state, 3, 0);
        let next_player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("next player should have claim options");

        assert!(next_player.can_peng);
        assert!(next_player.can_gang);
        assert!(next_player.chi_options.contains(&vec![1, 2]));
        assert!(next_player.chi_options.contains(&vec![2, 4]));
        assert!(!options.iter().any(|option| option.position == 3));
    }

    fn playable_state() -> ShenyangMahjongLoopState {
        let base = Arc::new(StdMutex::new(CommonGameState::default()));
        {
            let mut common = base.lock().unwrap();
            for position in 0..4 {
                common.add_player(position, position as u64 + 1, &format!("P{}", position));
            }
        }
        let mut state = ShenyangMahjongLoopState::new(base);
        state.phase = ShenyangMahjongPhase::Play;
        state.current_position = 0;
        state.dealer_position = 0;
        state
    }

    #[test]
    fn resolve_claim_window_gang_consumes_three_tiles_and_draws_replacement() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![35];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Gang)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.wall_count(), 0);
        assert_eq!(
            state
                .hands
                .get(&1)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            0,
        );
        assert!(state.hands.get(&1).unwrap().contains(&35));
        assert!(state.discards.get(&0).unwrap().is_empty());

        let meld = state.melds.get(&1).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
    }
}

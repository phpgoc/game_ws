use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Value, json};
use share_type_public::games::shenyang_mahjong::{
    ShenyangMahjongAction, ShenyangMahjongMeldKind, ShenyangMahjongPhase,
    ShenyangMahjongWinPattern, WsShenyangMahjongClaimOption, WsShenyangMahjongClaimWindowEvent,
    WsShenyangMahjongDealEvent, WsShenyangMahjongMeld, WsShenyangMahjongPlayEvent,
    WsShenyangMahjongPlayRequest, WsShenyangMahjongPlayerSnapshot,
    WsShenyangMahjongPublicPlayerSnapshot, WsShenyangMahjongScoreChange,
    WsShenyangMahjongSettlementEvent, WsShenyangMahjongTableSnapshotEvent,
    WsShenyangMahjongWinnerDetail,
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
    ClaimResponse, ClaimWindowKind, ClaimWindowState, ShenyangMahjongGameState,
    ShenyangMahjongLoopState, build_meld,
};
use crate::rules::{
    can_chi, can_concealed_gang, can_gang, can_peng, is_complete_win_with_melds,
    is_seven_pairs_win, tiles_in_hand, win_rule_from_configs,
};

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
    configs: &HashMap<String, i32>,
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
            is_complete_win_with_melds(
                &test,
                state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]),
                win_rule_from_configs(configs),
            )
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
    configs: &HashMap<String, i32>,
) -> WsShenyangMahjongClaimWindowEvent {
    let options = build_claim_options(state, tile, from_position, configs);
    build_claim_window_event_with_options(tile, from_position, seconds, options, false)
}

fn build_claim_window_event_with_options(
    tile: i32,
    from_position: usize,
    seconds: i32,
    options: Vec<WsShenyangMahjongClaimOption>,
    is_rob_gang: bool,
) -> WsShenyangMahjongClaimWindowEvent {
    let eligible_positions = options.iter().map(|option| option.position).collect();
    WsShenyangMahjongClaimWindowEvent {
        tile,
        from_position: from_position as i32,
        eligible_positions,
        seconds,
        is_rob_gang,
        options,
    }
}

fn build_rob_gang_claim_window_event(
    state: &ShenyangMahjongLoopState,
    tile: i32,
    from_position: usize,
    seconds: i32,
    configs: &HashMap<String, i32>,
) -> WsShenyangMahjongClaimWindowEvent {
    let mut positions: Vec<usize> = state.players_snapshot().keys().copied().collect();
    positions.sort_unstable();
    let options = positions
        .into_iter()
        .filter(|position| *position != from_position)
        .filter_map(|position| {
            let mut hand = state.hands.get(&position).cloned().unwrap_or_default();
            hand.push(tile);
            hand.sort_unstable();
            is_complete_win_with_melds(
                &hand,
                state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]),
                win_rule_from_configs(configs),
            )
            .then_some(WsShenyangMahjongClaimOption {
                position: position as i32,
                can_hu: true,
                can_peng: false,
                can_gang: false,
                chi_options: Vec::new(),
            })
        })
        .collect();
    build_claim_window_event_with_options(tile, from_position, seconds, options, true)
}

pub(crate) fn build_settlement_event(
    state: &ShenyangMahjongLoopState,
) -> Option<WsShenyangMahjongSettlementEvent> {
    let settlement = state.settlement.as_ref()?;
    let players = state.players_snapshot();
    let mut snapshots = Vec::new();
    let mut positions: Vec<usize> = players.keys().copied().collect();
    positions.sort_unstable();

    for position in &positions {
        let (_, name) = players.get(position).cloned().unwrap_or_default();
        let mut hand_tiles = state.hands.get(position).cloned().unwrap_or_default();
        if !settlement.is_self_draw
            && settlement.winner_positions.contains(position)
            && let Some(tile) = settlement.win_tile
        {
            hand_tiles.push(tile);
            hand_tiles.sort_unstable();
        }
        snapshots.push(WsShenyangMahjongPlayerSnapshot {
            position: *position as i32,
            name,
            hand_tiles,
            discards: state.discards.get(position).cloned().unwrap_or_default(),
            melds: state.melds.get(position).cloned().unwrap_or_default(),
        });
    }

    let score_changes = settlement_score_changes_for_positions(
        &positions,
        &settlement.winner_positions,
        settlement.from_position,
        settlement.is_self_draw,
    );
    let winner_details = build_winner_details(state, settlement, &score_changes);

    Some(WsShenyangMahjongSettlementEvent {
        winner_positions: settlement
            .winner_positions
            .iter()
            .map(|position| *position as i32)
            .collect(),
        from_position: settlement.from_position.map(|position| position as i32),
        win_tile: settlement.win_tile,
        is_self_draw: settlement.is_self_draw,
        is_reverse_win: settlement.is_reverse_win,
        score_changes,
        winner_details,
        players: snapshots,
    })
}

#[cfg(test)]
pub(crate) fn build_table_snapshot_event(
    state: &ShenyangMahjongLoopState,
    viewer_position: usize,
) -> WsShenyangMahjongTableSnapshotEvent {
    build_table_snapshot_event_with_configs(state, viewer_position, &HashMap::new())
}

pub(crate) fn build_table_snapshot_event_with_configs(
    state: &ShenyangMahjongLoopState,
    viewer_position: usize,
    configs: &HashMap<String, i32>,
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
            away: state.is_away(position) || state.is_disconnected(position),
            is_ai: state.is_ai_position(position),
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
        last_drawn_tile: state.last_drawn_tile,
        settlement: build_settlement_event(state),
        claim_window: state.claim_window.as_ref().map(|window| match window.kind {
            ClaimWindowKind::Discard => build_claim_window_event(
                state,
                window.tile,
                window.from_position,
                state.turn_countdown() as i32,
                configs,
            ),
            ClaimWindowKind::RobGang => build_rob_gang_claim_window_event(
                state,
                window.tile,
                window.from_position,
                state.turn_countdown() as i32,
                configs,
            ),
        }),
    }
}

fn build_winner_details(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    score_changes: &[WsShenyangMahjongScoreChange],
) -> Vec<WsShenyangMahjongWinnerDetail> {
    let score_by_position = score_changes
        .iter()
        .map(|change| (change.position as usize, change.score))
        .collect::<HashMap<_, _>>();

    settlement
        .winner_positions
        .iter()
        .map(|position| {
            let mut hand_tiles = state.hands.get(position).cloned().unwrap_or_default();
            if !settlement.is_self_draw
                && let Some(tile) = settlement.win_tile
            {
                hand_tiles.push(tile);
                hand_tiles.sort_unstable();
            }
            let meld_count = state.melds.get(position).map(Vec::len).unwrap_or(0);
            let pattern = if meld_count == 0 && is_seven_pairs_win(&hand_tiles) {
                ShenyangMahjongWinPattern::SevenPairs
            } else {
                ShenyangMahjongWinPattern::Standard
            };
            WsShenyangMahjongWinnerDetail {
                position: *position as i32,
                pattern,
                is_self_draw: settlement.is_self_draw,
                is_reverse_win: settlement.is_reverse_win,
                score: score_by_position.get(position).copied().unwrap_or(0),
            }
        })
        .collect()
}

fn can_added_gang(hand: &[i32], melds: &[WsShenyangMahjongMeld], target_tile: i32) -> bool {
    tiles_in_hand(hand, &[target_tile])
        && melds
            .iter()
            .any(|meld| peng_meld_tile(meld) == Some(target_tile))
}

#[cfg(test)]
pub(crate) fn can_self_draw_hu(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    can_self_draw_hu_with_configs(state, position, &HashMap::new())
}

pub(crate) fn can_self_draw_hu_with_configs(
    state: &ShenyangMahjongLoopState,
    position: usize,
    configs: &HashMap<String, i32>,
) -> bool {
    if state.current_position != position || state.last_drawn_tile.is_none() {
        return false;
    }
    let hand = state.hands.get(&position).cloned().unwrap_or_default();
    is_complete_win_with_melds(
        &hand,
        state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]),
        win_rule_from_configs(configs),
    )
}

pub(crate) fn can_self_gang(
    state: &ShenyangMahjongLoopState,
    position: usize,
    target_tile: i32,
) -> bool {
    if state.current_position != position || state.last_drawn_tile.is_none() {
        return false;
    }
    let hand = state.hands.get(&position).cloned().unwrap_or_default();
    let melds = state.melds.get(&position).cloned().unwrap_or_default();
    can_concealed_gang(&hand, target_tile) || can_added_gang(&hand, &melds, target_tile)
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

fn draw_after_gang_or_settle(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
) {
    if let Some(tile) = state.draw_for_position(position) {
        state.set_turn_countdown(current_play_time(configs));
        push_draw_events(room_service, room_key, state, dispatch, position, tile);
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

fn finish_added_gang(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
    target_tile: i32,
) -> bool {
    let hand = state.hands.get(&position).cloned().unwrap_or_default();
    let melds = state.melds.get(&position).cloned().unwrap_or_default();
    if !can_added_gang(&hand, &melds, target_tile)
        || !state.remove_tiles_from_hand(position, &[target_tile])
    {
        return false;
    }

    let Some(meld) = state
        .melds
        .entry(position)
        .or_default()
        .iter_mut()
        .find(|meld| peng_meld_tile(meld) == Some(target_tile))
    else {
        return false;
    };
    meld.kind = ShenyangMahjongMeldKind::GANG;
    meld.tiles = vec![target_tile, target_tile, target_tile, target_tile];
    let from_position = meld.from_position;

    state.current_position = position;
    state.last_drawn_tile = None;
    state.claim_window = None;
    push_room_event(
        room_service,
        room_key,
        dispatch,
        WsCode::PLAY as i32,
        WsShenyangMahjongPlayEvent {
            name: state.player_name(position),
            position: position as i32,
            action: ShenyangMahjongAction::GANG,
            tiles: vec![target_tile],
            target_tile: Some(target_tile),
            from_position,
            wall_count: state.wall_count() as i32,
        },
    );
    draw_after_gang_or_settle(room_service, room_key, state, configs, dispatch, position);
    true
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

fn maybe_record_settlement(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
) {
    crate::official::settle_round(room_service, room_key, state);
}

fn peng_meld_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    if meld.kind != ShenyangMahjongMeldKind::PENG {
        return None;
    }
    let tile = *meld.tiles.first()?;
    if meld.tiles.iter().all(|item| *item == tile) {
        Some(tile)
    } else {
        None
    }
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
    let claim_event = build_claim_window_event(state, tile, position, claim_seconds, configs);
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
            kind: ClaimWindowKind::Discard,
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

pub(crate) fn perform_self_gang(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
    target_tile: i32,
) -> bool {
    if state.last_drawn_tile.is_none() {
        return false;
    }

    let hand = state.hands.get(&position).cloned().unwrap_or_default();
    let melds = state.melds.get(&position).cloned().unwrap_or_default();
    let event_tiles = if can_concealed_gang(&hand, target_tile) {
        if !state.remove_tiles_from_hand(
            position,
            &[target_tile, target_tile, target_tile, target_tile],
        ) {
            return false;
        }
        state.melds.entry(position).or_default().push(build_meld(
            ShenyangMahjongMeldKind::GANG,
            vec![target_tile, target_tile, target_tile, target_tile],
            None,
        ));
        vec![target_tile, target_tile, target_tile, target_tile]
    } else if can_added_gang(&hand, &melds, target_tile) {
        let claim_seconds = current_claim_time(configs) as i32;
        let claim_event =
            build_rob_gang_claim_window_event(state, target_tile, position, claim_seconds, configs);
        let eligible_positions = claim_event
            .eligible_positions
            .iter()
            .map(|position| *position as usize)
            .collect::<Vec<_>>();
        if !eligible_positions.is_empty() {
            state.claim_window = Some(ClaimWindowState {
                tile: target_tile,
                from_position: position,
                kind: ClaimWindowKind::RobGang,
                eligible_positions,
                responses: HashMap::new(),
            });
            state.set_turn_countdown(current_claim_time(configs));
            state.set_action_received(false);
            push_room_event(
                room_service,
                room_key,
                dispatch,
                WsCode::CLAIM_WINDOW as i32,
                claim_event,
            );
            return true;
        }
        return finish_added_gang(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            position,
            target_tile,
        );
    } else {
        return false;
    };

    state.current_position = position;
    state.last_drawn_tile = None;
    push_room_event(
        room_service,
        room_key,
        dispatch,
        WsCode::PLAY as i32,
        WsShenyangMahjongPlayEvent {
            name: state.player_name(position),
            position: position as i32,
            action: ShenyangMahjongAction::GANG,
            tiles: event_tiles,
            target_tile: Some(target_tile),
            from_position: None,
            wall_count: state.wall_count() as i32,
        },
    );
    draw_after_gang_or_settle(room_service, room_key, state, configs, dispatch, position);
    true
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
    let is_rob_gang = matches!(claim_window.kind, ClaimWindowKind::RobGang);
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
            Some(ClaimResponse::Peng) if !is_rob_gang => {
                meld_claims.push((*position, ClaimResponse::Peng));
            }
            Some(ClaimResponse::Gang) if !is_rob_gang => {
                meld_claims.push((*position, ClaimResponse::Gang));
            }
            Some(ClaimResponse::Chi { consume_tiles }) if !is_rob_gang => {
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
        if is_rob_gang {
            let _ = state.remove_tiles_from_hand(claim_window.from_position, &[claim_window.tile]);
        } else {
            state.remove_last_discard(claim_window.from_position);
        }
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
        state.enter_settlement_with_reverse_win(
            winners,
            Some(claim_window.from_position),
            Some(claim_window.tile),
            false,
            is_rob_gang,
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

    if is_rob_gang {
        let _ = finish_added_gang(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            claim_window.from_position,
            claim_window.tile,
        );
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

pub(crate) fn settlement_score_changes_for_positions(
    positions: &[usize],
    winner_positions: &[usize],
    from_position: Option<usize>,
    is_self_draw: bool,
) -> Vec<WsShenyangMahjongScoreChange> {
    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();

    if winner_positions.is_empty() {
        return sorted_positions
            .into_iter()
            .map(|position| WsShenyangMahjongScoreChange {
                position: position as i32,
                score: 0,
            })
            .collect();
    }

    let winner_set = winner_positions.iter().copied().collect::<HashSet<_>>();
    let winner_count = winner_set.len() as i32;
    let loser_count = sorted_positions
        .iter()
        .filter(|position| !winner_set.contains(position))
        .count() as i32;

    sorted_positions
        .into_iter()
        .map(|position| {
            let score = if winner_set.contains(&position) {
                if is_self_draw { loser_count } else { 1 }
            } else if is_self_draw {
                -1
            } else if Some(position) == from_position {
                -winner_count
            } else {
                0
            };
            WsShenyangMahjongScoreChange {
                position: position as i32,
                score,
            }
        })
        .collect()
}

pub(crate) fn settlement_time(configs: &HashMap<String, i32>) -> u64 {
    config_value(configs, "settlement_time", 5).max(1) as u64
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
            if state.is_away(position) {
                return room_service.error_response(
                    session_id,
                    Routes::PLAY as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            }

            if state.claim_window.is_some() {
                let (claim_tile, from_position, is_rob_gang, eligible_positions, already_responded) = {
                    let claim_window = state.claim_window.as_ref().unwrap();
                    (
                        claim_window.tile,
                        claim_window.from_position,
                        matches!(claim_window.kind, ClaimWindowKind::RobGang),
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
                        if !is_complete_win_with_melds(
                            &tiles,
                            state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]),
                            win_rule_from_configs(&configs),
                        ) {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Hu
                    }
                    ShenyangMahjongAction::PENG => {
                        if is_rob_gang {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
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
                        if is_rob_gang {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
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
                        if is_rob_gang {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
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
                        if !can_self_draw_hu_with_configs(&state, position, &configs) {
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
                    ShenyangMahjongAction::GANG => {
                        let tile = payload
                            .target_tile
                            .or_else(|| payload.tiles.first().copied())
                            .unwrap_or_default();
                        if !can_self_gang(&state, position, tile)
                            || !perform_self_gang(
                                room_service,
                                &room_key,
                                &mut state,
                                &configs,
                                &mut dispatch,
                                position,
                                tile,
                            )
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
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

    fn prune_stopped_loop_states(&self, room_service: &mut RoomService) {
        let stopped = {
            let mut states = self.loop_states.lock().unwrap();
            let mut stopped = Vec::new();
            states.retain(|room_key, state| {
                let state = state.lock().unwrap();
                if state.stop_requested() {
                    stopped.push((room_key.clone(), Arc::clone(&state.base)));
                    false
                } else {
                    true
                }
            });
            stopped
        };
        for (room_key, common) in stopped {
            room_service.clear_room_game_state_if_same(&room_key, &common);
        }
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
            self.prune_stopped_loop_states(room_service);
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
        let configs = room_service.get_room_configs(&room_key).unwrap_or_default();
        let state = loop_state.lock().unwrap();
        if state.phase == ShenyangMahjongPhase::Start {
            return;
        }
        push_direct_event(
            dispatch,
            session_id,
            WsCode::TABLE_SNAPSHOT as i32,
            build_table_snapshot_event_with_configs(&state, position, &configs),
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
    fn added_gang_opens_rob_gang_claim_window_before_replacement_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        let mut dispatch = Dispatch::default();

        assert!(perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            3,
        ));

        let claim_window = state.claim_window.as_ref().unwrap();
        assert!(matches!(claim_window.kind, ClaimWindowKind::RobGang));
        assert_eq!(claim_window.tile, 3);
        assert_eq!(claim_window.from_position, 0);
        assert_eq!(claim_window.eligible_positions, vec![1]);
        assert_eq!(state.last_drawn_tile, Some(3));
        assert!(state.hands.get(&0).unwrap().contains(&3));
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().kind,
            ShenyangMahjongMeldKind::PENG
        );
    }

    #[test]
    fn added_gang_upgrades_peng_and_draws_replacement() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let mut dispatch = Dispatch::default();

        assert!(can_self_gang(&state, 0, 3));
        assert!(perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.last_drawn_tile, Some(35));
        assert_eq!(
            state
                .hands
                .get(&0)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            0,
        );

        let meld = state.melds.get(&0).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
        assert_eq!(meld.from_position, Some(2));
    }

    #[test]
    fn claim_options_allow_hu_after_open_meld() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![1, 1, 1],
                Some(2),
            )],
        );

        let options = build_claim_options(&state, 35, 0, &HashMap::new());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("open meld player should be able to hu with remaining hand");

        assert!(player.can_hu);
    }

    #[test]
    fn claim_options_allow_seven_pairs_hu() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);

        let options = build_claim_options(&state, 35, 0, &HashMap::new());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("seven pairs player should be able to hu");

        assert!(player.can_hu);
    }

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

        let options = build_claim_options(&state, 3, 0, &HashMap::new());
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

    #[test]
    fn claim_options_respect_shenyang_basic_win_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        let options = build_claim_options(&state, 35, 0, &configs);

        assert!(!options.iter().any(|option| option.position == 1));
    }

    fn has_room_event(dispatch: &Dispatch, code: WsCode) -> bool {
        dispatch.messages.iter().any(|item| {
            matches!(&item.payload, OutboundPayload::Event(event) if event.code == code as i32)
        })
    }

    fn play_request(
        action: ShenyangMahjongAction,
        tiles: Vec<i32>,
        target_tile: Option<i32>,
        from_position: Option<usize>,
    ) -> ClientRequest {
        ClientRequest {
            route: Routes::PLAY as i32,
            data: serde_json::json!({
                "action": action as i32,
                "tiles": tiles,
                "target_tile": target_tile,
                "from_position": from_position,
            }),
        }
    }

    #[test]
    fn play_request_allows_multiple_hu_by_default() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(1, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state
                .hands
                .insert(2, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1, 2],
                responses: HashMap::new(),
            });
        }

        let first_hu = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
        );
        let second_hu = handler.handle_game_request(
            &mut room_service,
            3,
            play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&first_hu, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&first_hu, WsCode::GAME_OVER));
        assert_eq!(
            response_code(&second_hu, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&second_hu, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(settlement.winner_positions, vec![1, 2]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
    }

    #[test]
    fn play_request_chi_consumes_tiles_and_keeps_turn_with_chi_player() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.wall = vec![36];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::CHI as i32,
                    "tiles": [1, 2],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.last_drawn_tile, None);
        assert_eq!(state.wall, vec![36]);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(!state.hands.get(&1).unwrap().contains(&1));
        assert!(!state.hands.get(&1).unwrap().contains(&2));
        let meld = state.melds.get(&1).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::CHI);
        assert_eq!(meld.tiles, vec![1, 2, 3]);
        assert_eq!(meld.from_position, Some(0));
    }

    #[test]
    fn play_request_chi_rejects_invalid_sequence() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(1, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 36]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::CHI as i32,
                    "tiles": [1, 4],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).unwrap().is_empty());
    }

    #[test]
    fn play_request_chi_rejects_non_next_player() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(2, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            3,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::CHI as i32,
                    "tiles": [1, 2],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&2).unwrap().is_empty());
    }

    #[test]
    fn play_request_discard_opens_claim_window_when_claimable() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
                state.hands.insert(
                    position,
                    vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33],
                );
            }
            state
                .hands
                .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
            state
                .hands
                .insert(1, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state
                .hands
                .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.wall = vec![36];
            state.last_drawn_tile = Some(3);
        }

        let response = handler.handle_game_request(
            &mut room_service,
            1,
            play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(3), None),
        );

        assert_eq!(
            response_code(&response, 1, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&response, WsCode::CLAIM_WINDOW));
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(matches!(claim_window.kind, ClaimWindowKind::Discard));
        assert_eq!(claim_window.tile, 3);
        assert_eq!(claim_window.from_position, 0);
        assert_eq!(claim_window.eligible_positions, vec![1, 2]);
        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, None);
        assert_eq!(state.wall, vec![36]);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
    }

    #[test]
    fn play_request_discard_rejects_tile_not_in_hand() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state
                .hands
                .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
            state.wall = vec![36];
            state.last_drawn_tile = Some(1);
        }

        let response = handler.handle_game_request(
            &mut room_service,
            1,
            play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(9), None),
        );

        assert_eq!(
            response_code(&response, 1, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 0);
        assert_eq!(state.wall, vec![36]);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.hands.get(&0).unwrap().len(), 14);
    }

    #[test]
    fn play_request_discard_without_claim_draws_next_player() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
                state.hands.insert(
                    position,
                    vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33],
                );
            }
            state
                .hands
                .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
            state.wall = vec![36];
            state.last_drawn_tile = Some(1);
        }

        let response = handler.handle_game_request(
            &mut room_service,
            1,
            play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(1), None),
        );

        assert_eq!(
            response_code(&response, 1, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&response, WsCode::CLAIM_WINDOW));
        assert!(!has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.last_drawn_tile, Some(36));
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.discards.get(&0), Some(&vec![1]));
        assert!(!state.hands.get(&0).unwrap().contains(&1));
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn play_request_gang_consumes_triplet_and_draws_replacement() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(2, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
            state.wall = vec![36];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            3,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::GANG as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 2);
        assert_eq!(state.last_drawn_tile, Some(36));
        assert_eq!(state.wall_count(), 0);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(
            state
                .hands
                .get(&2)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            0,
        );
        assert!(state.hands.get(&2).unwrap().contains(&36));
        let meld = state.melds.get(&2).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
        assert_eq!(meld.from_position, Some(0));
    }

    #[test]
    fn play_request_gang_rejects_without_triplet() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            3,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::GANG as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&2).unwrap().is_empty());
    }

    #[test]
    fn play_request_gang_settles_draw_when_replacement_tile_is_unavailable() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(2, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
            state.wall = Vec::new();
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            3,
            play_request(ShenyangMahjongAction::GANG, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 2);
        assert_eq!(state.wall_count(), 0);
        assert!(state.discards.get(&0).unwrap().is_empty());
        let settlement = state.settlement.as_ref().expect("settlement");
        assert!(settlement.winner_positions.is_empty());
        assert_eq!(settlement.from_position, None);
        assert_eq!(settlement.win_tile, None);
        let meld = state.melds.get(&2).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
        assert_eq!(meld.from_position, Some(0));
    }

    #[test]
    fn play_request_nearest_hu_mode_keeps_only_first_winner_in_turn_order() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        let _ = room_service.handle_common_request(
            1,
            &ClientRequest {
                route: Routes::SETTING as i32,
                data: serde_json::json!({"current_configs":{"multi_hu_mode":0}}),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(1, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state
                .hands
                .insert(2, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1, 2],
                responses: HashMap::new(),
            });
        }

        let later_hu = handler.handle_game_request(
            &mut room_service,
            3,
            play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
        );
        let nearest_hu = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&later_hu, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&later_hu, WsCode::GAME_OVER));
        assert_eq!(
            response_code(&nearest_hu, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&nearest_hu, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(settlement.winner_positions, vec![1]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
    }

    #[test]
    fn play_request_pass_rejects_duplicate_claim_response() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1, 2],
                responses: HashMap::from([(1, ClaimResponse::Pass)]),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert_eq!(claim_window.responses.len(), 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
    }

    #[test]
    fn play_request_pass_resolves_after_all_claims_and_draws_next_player() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
                state.hands.insert(
                    position,
                    vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33],
                );
            }
            state.discards.insert(0, vec![3]);
            state.wall = vec![36];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1, 2],
                responses: HashMap::new(),
            });
        }

        let first_pass = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
        );
        let second_pass = handler.handle_game_request(
            &mut room_service,
            3,
            play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&first_pass, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert_eq!(
            response_code(&second_pass, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&second_pass, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.last_drawn_tile, Some(36));
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.hands.get(&1).unwrap().contains(&36));
        assert!(state.settlement.is_none());
    }

    #[test]
    fn play_request_pass_resolves_to_draw_when_wall_is_empty() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
                state.hands.insert(
                    position,
                    vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33],
                );
            }
            state.discards.insert(0, vec![3]);
            state.wall = Vec::new();
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(state.claim_window.is_none());
        let settlement = state.settlement.as_ref().expect("settlement");
        assert!(settlement.winner_positions.is_empty());
        assert_eq!(settlement.from_position, None);
        assert_eq!(settlement.win_tile, None);
        assert!(!settlement.is_self_draw);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
    }

    #[test]
    fn play_request_pass_waits_for_remaining_claim_responses() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state.wall = vec![36];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1, 2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::PASS, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert_eq!(claim_window.responses.len(), 1);
        assert_eq!(state.current_position, 0);
        assert_eq!(state.wall, vec![36]);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.settlement.is_none());
    }

    #[test]
    fn play_request_peng_consumes_pair_and_keeps_turn_with_peng_player() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.wall = vec![36];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            3,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::PENG as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 2);
        assert_eq!(state.last_drawn_tile, None);
        assert_eq!(state.wall, vec![36]);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(
            state
                .hands
                .get(&2)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            0,
        );
        let meld = state.melds.get(&2).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::PENG);
        assert_eq!(meld.tiles, vec![3, 3, 3]);
        assert_eq!(meld.from_position, Some(0));
    }

    #[test]
    fn play_request_peng_rejects_without_pair() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(2, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 36]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![2],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            3,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::PENG as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&2).unwrap().is_empty());
    }

    #[test]
    fn play_request_rejects_manual_action_while_away() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            state.base.lock().unwrap().mark_away(0);
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state
                .hands
                .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
            state.wall = vec![36];
            state.last_drawn_tile = Some(1);
        }

        let response = handler.handle_game_request(
            &mut room_service,
            1,
            play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(1), None),
        );

        assert_eq!(
            response_code(&response, 1, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert_eq!(state.current_position, 0);
        assert_eq!(state.discards.get(&0), Some(&Vec::<i32>::new()));
        assert!(state.hands.get(&0).unwrap().contains(&1));
    }

    #[test]
    fn play_request_rejects_manual_claim_response_while_away() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            state.base.lock().unwrap().mark_away(1);
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::HU, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(claim_window.responses.is_empty());
        assert!(state.settlement.is_none());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
    }

    #[test]
    fn play_request_rejects_self_hu_without_draw_and_accepts_after_draw() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state
                .hands
                .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        }

        let denied = handler.handle_game_request(
            &mut room_service,
            1,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::HU as i32,
                    "tiles": [],
                    "target_tile": null,
                    "from_position": null,
                }),
            },
        );

        assert_eq!(
            response_code(&denied, 1, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        assert!(loop_state.lock().unwrap().settlement.is_none());

        {
            loop_state.lock().unwrap().last_drawn_tile = Some(35);
        }
        let accepted = handler.handle_game_request(
            &mut room_service,
            1,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::HU as i32,
                    "tiles": [],
                    "target_tile": null,
                    "from_position": null,
                }),
            },
        );

        assert_eq!(
            response_code(&accepted, 1, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&accepted, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(
            state
                .settlement
                .as_ref()
                .map(|settlement| settlement.winner_positions.clone()),
            Some(vec![0])
        );
    }

    #[test]
    fn play_request_respects_shenyang_basic_win_rule() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        let _ = room_service.handle_common_request(
            1,
            &ClientRequest {
                route: Routes::SETTING as i32,
                data: serde_json::json!({"current_configs":{"win_rule":1}}),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![35]);
            state
                .hands
                .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 35,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let denied = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::HU, Vec::new(), Some(35), Some(0)),
        );

        assert_eq!(
            response_code(&denied, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        assert!(loop_state.lock().unwrap().settlement.is_none());
    }

    #[test]
    fn play_request_rob_gang_pass_finishes_added_gang() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state
                .hands
                .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
            state.melds.insert(
                0,
                vec![build_meld(
                    ShenyangMahjongMeldKind::PENG,
                    vec![3, 3, 3],
                    Some(2),
                )],
            );
            state
                .hands
                .insert(1, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state.wall = vec![36];
            state.last_drawn_tile = Some(3);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::RobGang,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let pass_response = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::PASS as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&pass_response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&pass_response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, Some(36));
        assert!(state.hands.get(&0).unwrap().contains(&36));
        assert!(!state.hands.get(&0).unwrap().contains(&3));
        assert!(state.settlement.is_none());
        let meld = state.melds.get(&0).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
    }

    #[test]
    fn play_request_rob_gang_rejects_peng_and_accepts_hu() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state
                .hands
                .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
            state.melds.insert(
                0,
                vec![build_meld(
                    ShenyangMahjongMeldKind::PENG,
                    vec![3, 3, 3],
                    Some(2),
                )],
            );
            state
                .hands
                .insert(1, vec![3, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state.last_drawn_tile = Some(3);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::RobGang,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let rejected_peng = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::PENG as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&rejected_peng, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        assert!(loop_state.lock().unwrap().claim_window.is_some());

        let accepted_hu = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::HU as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&accepted_hu, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&accepted_hu, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(settlement.winner_positions, vec![1]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert!(settlement.is_reverse_win);
        assert!(!state.hands.get(&0).unwrap().contains(&3));
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().kind,
            ShenyangMahjongMeldKind::PENG
        );
    }

    #[test]
    fn play_request_waits_for_claims_and_hu_beats_peng() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![3]);
            state
                .hands
                .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state
                .hands
                .insert(2, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1, 2],
                responses: HashMap::new(),
            });
        }

        let peng_response = handler.handle_game_request(
            &mut room_service,
            3,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::PENG as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&peng_response, 3, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(!has_room_event(&peng_response, WsCode::GAME_OVER));
        {
            let state = loop_state.lock().unwrap();
            assert!(state.claim_window.is_some());
            assert!(state.settlement.is_none());
            assert!(state.melds.get(&2).unwrap().is_empty());
        }

        let hu_response = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::HU as i32,
                    "tiles": [],
                    "target_tile": 3,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&hu_response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        assert!(has_room_event(&hu_response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(
            state
                .settlement
                .as_ref()
                .map(|settlement| settlement.winner_positions.clone()),
            Some(vec![1])
        );
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(state.melds.get(&2).unwrap().is_empty());
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
    fn pruning_stopped_loop_state_restores_room_acceptance() {
        let mut room_service = RoomService::default();
        for session_id in 1..=3 {
            room_service.connect(session_id);
        }
        for (session_id, name) in [(1_u64, "P1"), (2, "P2")] {
            let _ = room_service.handle_common_request(
                session_id,
                &ClientRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": name,
                        "password": "room",
                        "game_id": GameId::SHENYANG_MAHJONG as i32
                    }),
                },
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            );
        }
        let room_key = room_service.room_key_of(1).expect("room key");
        let common = room_service
            .get_room_common_state_handle(&room_key)
            .expect("common state");
        let loop_state = Arc::new(StdMutex::new(ShenyangMahjongLoopState::new(Arc::clone(
            &common,
        ))));
        room_service.set_room_game_state(
            &room_key,
            Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
                &loop_state,
            ))),
        );
        let handler = ShenyangMahjongGameHandler::default();
        handler
            .loop_states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&loop_state));
        loop_state.lock().unwrap().request_stop();

        handler.prune_stopped_loop_states(&mut room_service);
        let join_after_prune = room_service
            .handle_common_request(
                3,
                &ClientRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": "P3",
                        "password": "room",
                        "game_id": GameId::SHENYANG_MAHJONG as i32
                    }),
                },
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            )
            .expect("join common");
        let joined = join_after_prune
            .messages
            .iter()
            .any(|item| match &item.payload {
                OutboundPayload::Response(RequestResponse::WithData(response)) => {
                    response.code as i32 == WsResponseCode::JOINED as i32
                }
                _ => false,
            });

        assert!(joined);
        assert_eq!(room_service.session_position(3), Some(2));
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
            kind: ClaimWindowKind::Discard,
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

    fn response_code(dispatch: &Dispatch, recipient: SessionId, route: Routes) -> Option<i32> {
        dispatch
            .messages
            .iter()
            .find_map(|item| match &item.payload {
                OutboundPayload::Response(RequestResponse::WithData(response))
                    if item.recipient == recipient && response.route == route as i32 =>
                {
                    Some(response.code as i32)
                }
                OutboundPayload::Response(RequestResponse::WithoutData(response))
                    if item.recipient == recipient && response.route == route as i32 =>
                {
                    Some(response.code as i32)
                }
                _ => None,
            })
    }

    #[test]
    fn rob_gang_claim_pass_finishes_added_gang_and_draws_replacement() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Pass)]),
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
        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, Some(36));
        assert!(!state.hands.get(&0).unwrap().contains(&3));
        let meld = state.melds.get(&0).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
    }

    #[test]
    fn rob_gang_hu_settles_without_upgrading_peng() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Hu)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        );

        let settlement = state.settlement.as_ref().unwrap();
        assert_eq!(settlement.winner_positions, vec![1]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert!(settlement.is_reverse_win);
        assert!(!state.hands.get(&0).unwrap().contains(&3));
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().kind,
            ShenyangMahjongMeldKind::PENG
        );
    }

    #[test]
    fn self_draw_hu_rejects_complete_open_hand_without_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![1, 1, 1],
                Some(2),
            )],
        );

        assert!(!can_self_draw_hu(&state, 0));
    }

    #[test]
    fn self_draw_hu_requires_a_drawn_turn() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);

        assert!(!can_self_draw_hu(&state, 0));

        state.last_drawn_tile = Some(35);

        assert!(can_self_draw_hu(&state, 0));
    }

    #[test]
    fn self_draw_hu_requires_current_position() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.last_drawn_tile = Some(35);

        assert!(!can_self_draw_hu(&state, 0));
    }

    #[test]
    fn self_draw_hu_respects_shenyang_basic_win_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.last_drawn_tile = Some(35);
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));

        state
            .hands
            .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![1, 2, 3],
                Some(1),
            )],
        );

        assert!(can_self_draw_hu_with_configs(&state, 0, &configs));
    }

    #[test]
    fn self_gang_consumes_four_tiles_and_draws_replacement() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let mut dispatch = Dispatch::default();

        assert!(can_self_gang(&state, 0, 3));
        assert!(perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.current_position, 0);
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.last_drawn_tile, Some(35));
        assert!(state.hands.get(&0).unwrap().contains(&35));
        assert_eq!(
            state
                .hands
                .get(&0)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            0,
        );

        let meld = state.melds.get(&0).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::GANG);
        assert_eq!(meld.tiles, vec![3, 3, 3, 3]);
        assert_eq!(meld.from_position, None);
    }

    #[test]
    fn self_gang_requires_current_position() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.last_drawn_tile = Some(3);

        assert!(!can_self_gang(&state, 0, 3));
    }

    #[test]
    fn settlement_score_changes_cover_discard_self_draw_and_draw() {
        assert_eq!(
            settlement_score_changes_for_positions(&[0, 1, 2, 3], &[0, 2], Some(1), false)
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 1), (1, -2), (2, 1), (3, 0)]
        );
        assert_eq!(
            settlement_score_changes_for_positions(&[0, 1, 2, 3], &[2], None, true)
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -1), (1, -1), (2, 3), (3, -1)]
        );
        assert_eq!(
            settlement_score_changes_for_positions(&[0, 1, 2, 3], &[], None, false)
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_winner_details_describe_seven_pairs_self_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![2], None, Some(35), true);

        let event = build_settlement_event(&state).unwrap();

        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(event.winner_details[0].position, 2);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::SevenPairs
        );
        assert!(event.winner_details[0].is_self_draw);
        assert_eq!(event.winner_details[0].score, 3);
    }

    #[test]
    fn settlement_winner_details_include_reverse_win_and_score() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(3), false, true);

        let event = build_settlement_event(&state).unwrap();

        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(event.winner_details[0].position, 1);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::Standard
        );
        assert!(event.winner_details[0].is_reverse_win);
        assert_eq!(event.winner_details[0].score, 1);
    }

    fn setup_request_room() -> (
        RoomService,
        ShenyangMahjongGameHandler,
        String,
        LoopStateHandle,
    ) {
        let mut room_service = RoomService::default();
        for session_id in 1..=4 {
            room_service.connect(session_id);
            let _ = room_service.handle_common_request(
                session_id,
                &ClientRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": format!("P{}", session_id),
                        "password": "mahjong-request-room",
                        "game_id": GameId::SHENYANG_MAHJONG as i32
                    }),
                },
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            );
        }
        let room_key = room_service.room_key_of(1).expect("room key");
        let common = room_service
            .get_room_common_state_handle(&room_key)
            .expect("common state");
        let loop_state = Arc::new(StdMutex::new(ShenyangMahjongLoopState::new(Arc::clone(
            &common,
        ))));
        room_service.set_room_game_state(
            &room_key,
            Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
                &loop_state,
            ))),
        );
        let handler = ShenyangMahjongGameHandler::default();
        handler
            .loop_states
            .lock()
            .unwrap()
            .insert(room_key.clone(), Arc::clone(&loop_state));

        (room_service, handler, room_key, loop_state)
    }

    #[test]
    fn table_snapshot_includes_settlement_for_rejoin() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.discards.insert(0, vec![3]);
        state.enter_settlement_with_reverse_win(vec![1], Some(0), Some(3), false, true);

        let snapshot = build_table_snapshot_event(&state, 1);
        let settlement = snapshot.settlement.expect("settlement");

        assert_eq!(snapshot.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![1]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert!(settlement.is_reverse_win);
        assert_eq!(settlement.winner_details.len(), 1);
        assert_eq!(settlement.winner_details[0].position, 1);
        assert_eq!(settlement.winner_details[0].score, 1);
        assert_eq!(
            settlement
                .score_changes
                .iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -1), (1, 1), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn table_snapshot_marks_disconnected_player_as_away() {
        let state = playable_state();
        state.base.lock().unwrap().mark_disconnected(2);

        let snapshot = build_table_snapshot_event(&state, 1);
        let player = snapshot
            .players
            .iter()
            .find(|player| player.position == 2)
            .expect("player snapshot");

        assert!(player.away);
        assert!(!player.is_ai);
    }

    #[test]
    fn table_snapshot_preserves_drawn_tile_and_claim_options() {
        let mut state = playable_state();
        state.current_position = 0;
        state.last_drawn_tile = Some(9);
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 9, 11, 12, 13, 21, 22, 23, 31]);
        state
            .hands
            .insert(1, vec![1, 2, 3, 3, 3, 4, 11, 12, 13, 21, 22, 23, 31]);
        state.discards.insert(0, vec![3]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
        state.set_turn_countdown(4);

        let snapshot = build_table_snapshot_event(&state, 1);
        let claim_window = snapshot.claim_window.expect("claim window");
        let option = claim_window
            .options
            .iter()
            .find(|option| option.position == 1)
            .expect("claim option");

        assert_eq!(snapshot.last_drawn_tile, Some(9));
        assert_eq!(claim_window.tile, 3);
        assert_eq!(claim_window.from_position, 0);
        assert_eq!(claim_window.eligible_positions, vec![1]);
        assert_eq!(claim_window.seconds, 4);
        assert!(!claim_window.is_rob_gang);
        assert!(option.can_peng);
        assert!(option.can_gang);
        assert!(option.chi_options.contains(&vec![1, 2]));
        assert!(option.chi_options.contains(&vec![2, 4]));
    }
}

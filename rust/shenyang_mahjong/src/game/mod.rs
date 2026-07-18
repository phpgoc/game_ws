use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Value, json};
use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongAction, ShenyangMahjongMeldKind,
    ShenyangMahjongPhase, ShenyangMahjongWinPattern, WsShenyangMahjongClaimOption,
    WsShenyangMahjongClaimWindowEvent, WsShenyangMahjongDealEvent, WsShenyangMahjongMeld,
    WsShenyangMahjongPlayEvent, WsShenyangMahjongPlayRequest, WsShenyangMahjongPlayerSnapshot,
    WsShenyangMahjongPublicPlayerSnapshot, WsShenyangMahjongScoreChange,
    WsShenyangMahjongSettlementEvent, WsShenyangMahjongTableSnapshotEvent,
    WsShenyangMahjongWinnerDetail,
};
use share_type_public::{GameId, Routes, WsCode, WsResponseCode};
use tokio::sync::Mutex;
use ws_common::{
    ClientRequest, Delivery, Dispatch, GameHandler, OutboundPayload, RequestResponse, RoomService,
    SessionId, SessionSenders, SharedGameState,
};

use crate::game_loop::start_game_loop;
use crate::game_setting::build_shenyang_mahjong_settings;
use crate::game_state::{
    ClaimResponse, ClaimWindowKind, ClaimWindowState, ShenyangMahjongGameState,
    ShenyangMahjongLoopState, build_meld, meld_source_is_valid_for_positions,
};
use crate::rules::{
    ShenyangMahjongWinContext, XI_GANG_WINDS, can_chi, can_concealed_gang, can_gang, can_peng,
    is_complete_win_with_melds_with_context, is_door_opening_meld, is_valid_meld, is_xi_gang_tiles,
    remove_tiles, shenyang_payment_fan, shenyang_score_for_fan_with_cap,
    shenyang_score_visible_win_fan, shenyang_win_pattern, tiles_in_hand,
};
#[cfg(test)]
use crate::rules::{
    is_legal_single_wait_shape, is_seven_pairs_win,
    is_single_wait_shape_with_known_unavailable_tiles_with_context, shenyang_score_four_gui_yi_fan,
    shenyang_score_meld_fan,
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
    if let Some(tile) = state.draw_for_next_turn(next_position) {
        state.set_turn_countdown(current_play_time(configs));
        push_draw_events(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            next_position,
            tile,
        );
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

    settle_draw(room_service, room_key, state, configs, dispatch);
}

fn allow_first_chi(configs: &HashMap<String, i32>) -> bool {
    ShenyangMahjongWinContext::from_configs(configs).allows_first_chi()
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
    let has_impossible_tile_count = has_impossible_known_tile_count(state, tile);
    let mut options = Vec::new();

    for position in positions {
        if position == from_position {
            continue;
        }
        if has_impossible_tile_count {
            continue;
        }
        let hand = state.hands.get(&position).cloned().unwrap_or_default();
        let can_hu = can_claim_hu_with_configs(state, position, tile, configs);
        let can_claim_meld = position_can_claim_meld(state, position);
        let can_peng_now = can_claim_meld && can_peng(&hand, tile);
        let can_gang_now = can_claim_meld && state.wall_count() > 0 && can_gang(&hand, tile);
        let chi_options = if can_claim_meld
            && position_can_chi(state, position, configs)
            && position == next_position
        {
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
    let has_impossible_tile_count = has_impossible_known_tile_count(state, tile);
    let options = positions
        .into_iter()
        .filter(|position| *position != from_position)
        .filter_map(|position| {
            if has_impossible_tile_count {
                return None;
            }
            can_claim_hu_with_configs(state, position, tile, configs).then_some(
                WsShenyangMahjongClaimOption {
                    position: position as i32,
                    can_hu: true,
                    can_peng: false,
                    can_gang: false,
                    chi_options: Vec::new(),
                },
            )
        })
        .collect();
    build_claim_window_event_with_options(tile, from_position, seconds, options, true)
}

#[cfg(test)]
pub(crate) fn build_settlement_event(
    state: &ShenyangMahjongLoopState,
) -> Option<WsShenyangMahjongSettlementEvent> {
    build_settlement_event_with_configs(state, &HashMap::new())
}

pub(crate) fn build_settlement_event_with_configs(
    state: &ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
) -> Option<WsShenyangMahjongSettlementEvent> {
    let settlement = state.settlement.as_ref()?;
    let players = state.players_snapshot();
    let mut snapshots = Vec::new();
    let mut positions: Vec<usize> = players.keys().copied().collect();
    positions.sort_unstable();
    let player_positions = positions.iter().copied().collect::<HashSet<_>>();
    let score_changes = settlement_score_changes_for_state(state, &positions, settlement, configs);
    let winner_positions =
        positive_winner_positions_from_scores(settlement, &score_changes).collect::<Vec<_>>();
    let has_effective_winner = !winner_positions.is_empty();
    let winner_position_set = winner_positions.iter().copied().collect::<HashSet<_>>();

    for position in &positions {
        let (_, name) = players.get(position).cloned().unwrap_or_default();
        let mut hand_tiles = state.hands.get(position).cloned().unwrap_or_default();
        if !settlement.is_self_draw
            && winner_position_set.contains(position)
            && let Some(tile) = settlement.win_tile
        {
            hand_tiles.push(tile);
            hand_tiles.sort_unstable();
        }
        snapshots.push(WsShenyangMahjongPlayerSnapshot {
            position: *position as i32,
            name,
            hand_tiles,
            discards: public_discards_for_position(state, *position),
            melds: public_melds_for_position(state, *position, &player_positions),
            is_ting: Some(state.is_ting(*position)),
        });
    }

    let winner_details = build_winner_details(state, settlement, &score_changes, configs);

    Some(WsShenyangMahjongSettlementEvent {
        winner_positions: winner_positions
            .iter()
            .map(|position| *position as i32)
            .collect(),
        from_position: if has_effective_winner {
            settlement_from_position(settlement).map(|position| position as i32)
        } else {
            None
        },
        win_tile: if has_effective_winner {
            settlement.win_tile
        } else {
            None
        },
        is_self_draw: has_effective_winner && settlement.is_self_draw,
        is_reverse_win: has_effective_winner && settlement_is_reverse_win(state, settlement),
        is_gang_draw: has_effective_winner && settlement_is_gang_draw(state, settlement),
        is_haidilao: has_effective_winner && settlement_is_haidilao(state, settlement),
        score_changes,
        winner_details,
        players: snapshots,
    })
}

pub(crate) fn build_table_snapshot_event_with_configs(
    state: &ShenyangMahjongLoopState,
    viewer_position: usize,
    configs: &HashMap<String, i32>,
) -> WsShenyangMahjongTableSnapshotEvent {
    let players = state.players_snapshot();
    let mut positions: Vec<usize> = players.keys().copied().collect();
    positions.sort_unstable();
    let player_positions = positions.iter().copied().collect::<HashSet<_>>();
    let mut snapshots = Vec::new();

    for position in positions {
        let (_, name) = players.get(&position).cloned().unwrap_or_default();
        snapshots.push(WsShenyangMahjongPublicPlayerSnapshot {
            position: position as i32,
            name,
            away: state.is_away(position) || state.is_disconnected(position),
            is_ai: state.is_ai_position(position),
            is_ai_takeover: state.base.lock().unwrap().is_ai_takeover_position(position),
            hand_count: state
                .hands
                .get(&position)
                .map(|hand| hand.len())
                .unwrap_or(0) as i32,
            discards: public_discards_for_position(state, position),
            melds: public_melds_for_position(state, position, &player_positions),
            is_ting: Some(state.is_ting(position)),
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
        last_drawn_tile: if state.current_position == viewer_position
            && position_owns_last_drawn_tile(state, viewer_position)
        {
            state.last_drawn_tile
        } else {
            None
        },
        settlement: build_settlement_event_with_configs(state, configs),
        claim_window: state
            .claim_window
            .as_ref()
            .filter(|window| {
                state.phase == ShenyangMahjongPhase::Play
                    && claim_window_matches_source(state, window)
            })
            .map(|window| {
                let event = match window.kind {
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
                };
                let can_respond = window.eligible_positions.contains(&viewer_position)
                    && !window.responses.contains_key(&viewer_position);
                claim_window_event_for_viewer(&event, viewer_position, can_respond)
            }),
        xi_gang_options: if state.phase == ShenyangMahjongPhase::Play
            && state.current_position == viewer_position
            && state.claim_window.is_none()
        {
            state.xi_gang_options_for_position(viewer_position)
        } else {
            Vec::new()
        },
        ting_discard_tiles: ting_discard_tiles_for_position(state, viewer_position, configs),
    }
}

fn build_winner_details(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    score_changes: &[WsShenyangMahjongScoreChange],
    configs: &HashMap<String, i32>,
) -> Vec<WsShenyangMahjongWinnerDetail> {
    let context = ShenyangMahjongWinContext::from_configs(configs);
    let score_by_position = score_changes
        .iter()
        .map(|change| (change.position as usize, change.score))
        .collect::<HashMap<_, _>>();

    settlement
        .unique_winner_positions()
        .into_iter()
        .filter_map(|position| {
            let score = score_by_position.get(&position).copied().unwrap_or(0);
            if score <= 0 {
                return None;
            }
            let hand_tiles = winner_final_hand_tiles(state, settlement, position);
            let melds = state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]);
            let pattern = winner_pattern_with_context(&hand_tiles, melds, context);
            Some(WsShenyangMahjongWinnerDetail {
                position: position as i32,
                pattern,
                is_self_draw: settlement.is_self_draw,
                is_reverse_win: settlement_is_reverse_win(state, settlement),
                is_gang_draw: settlement_winner_is_gang_draw(state, settlement, position),
                is_haidilao: settlement_is_haidilao(state, settlement),
                score,
            })
        })
        .collect()
}

fn can_added_gang(hand: &[i32], melds: &[WsShenyangMahjongMeld], target_tile: i32) -> bool {
    is_valid_tile(target_tile)
        && hand_tiles_are_valid(hand)
        && hand.iter().filter(|tile| **tile == target_tile).count() == 1
        && melds
            .iter()
            .filter(|meld| is_door_opening_meld(meld) && peng_meld_tile(meld) == Some(target_tile))
            .count()
            == 1
}

fn can_claim_hu_with_configs(
    state: &ShenyangMahjongLoopState,
    position: usize,
    tile: i32,
    configs: &HashMap<String, i32>,
) -> bool {
    if has_impossible_known_tile_count(state, tile)
        || !position_hand_tiles_are_valid(state, position)
        || position_has_impossible_known_tile_count(state, position)
        || !position_meld_shapes_are_valid(state, position)
        || !position_meld_sources_are_valid(state, position)
    {
        return false;
    }
    let mut hand = state.hands.get(&position).cloned().unwrap_or_default();
    hand.push(tile);
    hand.sort_unstable();
    is_complete_win_with_configs(
        &hand,
        state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]),
        configs,
    )
}

pub(crate) fn can_declare_xi_gang(
    state: &ShenyangMahjongLoopState,
    position: usize,
    tiles: &[i32],
) -> bool {
    let mut tiles = tiles.to_vec();
    tiles.sort_unstable();
    state.phase == ShenyangMahjongPhase::Play
        && state.current_position == position
        && position != state.dealer_position
        && state.claim_window.is_none()
        && !state.is_ting(position)
        && position_has_discardable_tile_count(state, position)
        && position_hand_tiles_are_valid(state, position)
        && !position_has_impossible_known_tile_count(state, position)
        && position_meld_shapes_are_valid(state, position)
        && position_meld_sources_are_valid(state, position)
        && is_xi_gang_tiles(&tiles)
        && state
            .xi_gang_options_for_position(position)
            .contains(&tiles)
        && state
            .hands
            .get(&position)
            .is_some_and(|hand| tiles_in_hand(hand, &tiles))
        && (tiles.as_slice() != XI_GANG_WINDS || state.has_drawable_wall_tile())
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
    if state.phase != ShenyangMahjongPhase::Play
        || state.current_position != position
        || !position_owns_last_drawn_tile(state, position)
        || state.claim_window.is_some()
        || !position_hand_tiles_are_valid(state, position)
        || position_has_impossible_known_tile_count(state, position)
        || !position_meld_shapes_are_valid(state, position)
        || !position_meld_sources_are_valid(state, position)
    {
        return false;
    }
    let hand = state.hands.get(&position).cloned().unwrap_or_default();
    is_complete_win_with_configs(
        &hand,
        state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]),
        configs,
    )
}

pub(crate) fn can_self_gang(
    state: &ShenyangMahjongLoopState,
    position: usize,
    target_tile: i32,
) -> bool {
    if state.phase != ShenyangMahjongPhase::Play
        || state.current_position != position
        || !position_owns_last_drawn_tile(state, position)
        || state.claim_window.is_some()
        || state.is_ting(position)
        || state.wall_count() == 0
        || !position_has_discardable_tile_count(state, position)
        || !position_hand_tiles_are_valid(state, position)
        || position_has_impossible_known_tile_count(state, position)
        || !position_meld_shapes_are_valid(state, position)
        || !position_meld_sources_are_valid(state, position)
    {
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

fn claim_window_event_for_viewer(
    event: &WsShenyangMahjongClaimWindowEvent,
    viewer_position: usize,
    can_respond: bool,
) -> WsShenyangMahjongClaimWindowEvent {
    let viewer_position = viewer_position as i32;
    let mut event = event.clone();
    if !can_respond {
        event.eligible_positions.clear();
        event.options.clear();
        return event;
    }
    event
        .eligible_positions
        .retain(|position| *position == viewer_position);
    event
        .options
        .retain(|option| option.position == viewer_position);
    event
}

pub(crate) fn claim_window_matches_source(
    state: &ShenyangMahjongLoopState,
    claim_window: &ClaimWindowState,
) -> bool {
    if !is_valid_tile(claim_window.tile)
        || has_impossible_known_tile_count(state, claim_window.tile)
        || !claim_window_participants_are_valid(state, claim_window)
        || state.current_position != claim_window.from_position
        || !state
            .players_snapshot()
            .contains_key(&claim_window.from_position)
    {
        return false;
    }
    match claim_window.kind {
        ClaimWindowKind::Discard => {
            discard_claim_matches_source(state, claim_window.tile, claim_window.from_position)
        }
        ClaimWindowKind::RobGang => {
            rob_gang_claim_matches_source(state, claim_window.tile, claim_window.from_position)
        }
    }
}

fn claim_window_participants_are_valid(
    state: &ShenyangMahjongLoopState,
    claim_window: &ClaimWindowState,
) -> bool {
    let player_positions = state
        .players_snapshot()
        .keys()
        .copied()
        .collect::<HashSet<_>>();
    let eligible_positions = claim_window
        .eligible_positions
        .iter()
        .copied()
        .collect::<HashSet<_>>();

    !eligible_positions.is_empty()
        && eligible_positions.len() == claim_window.eligible_positions.len()
        && eligible_positions.iter().all(|position| {
            *position != claim_window.from_position && player_positions.contains(position)
        })
        && claim_window
            .responses
            .keys()
            .all(|position| eligible_positions.contains(position))
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

fn discard_claim_matches_source(
    state: &ShenyangMahjongLoopState,
    tile: i32,
    from_position: usize,
) -> bool {
    state
        .discards
        .get(&from_position)
        .and_then(|discards| discards.last())
        .copied()
        == Some(tile)
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
        state.pending_gang_draw = true;
        state.set_turn_countdown(current_play_time(configs));
        push_draw_events(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            position,
            tile,
        );
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

    settle_draw(room_service, room_key, state, configs, dispatch);
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
            xi_gang_options: Vec::new(),
            ting_discard_tiles: Vec::new(),
            is_ting: None,
        },
    );
    draw_after_gang_or_settle(room_service, room_key, state, configs, dispatch, position);
    true
}

#[cfg(test)]
fn four_gui_yi_fan(hand_tiles: &[i32], melds: &[WsShenyangMahjongMeld]) -> i32 {
    shenyang_score_four_gui_yi_fan(hand_tiles, melds)
}

fn hand_tiles_are_valid(hand: &[i32]) -> bool {
    hand.iter().all(|tile| is_valid_tile(*tile))
}

fn has_impossible_known_tile_count(state: &ShenyangMahjongLoopState, tile: i32) -> bool {
    is_valid_tile(tile) && known_tile_count(state, tile) > 4
}

fn is_complete_win_with_configs(
    tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    configs: &HashMap<String, i32>,
) -> bool {
    is_complete_win_with_melds_with_context(
        tiles,
        melds,
        ShenyangMahjongWinContext::from_configs(configs),
    )
}

fn ting_wait_tiles_after_discard(
    state: &ShenyangMahjongLoopState,
    position: usize,
    discard_tile: i32,
    configs: &HashMap<String, i32>,
) -> Vec<i32> {
    ting_shape_wait_tiles_after_discard(state, position, discard_tile, configs)
        .into_iter()
        .filter(|tile| visible_known_tile_count_for_position(state, position, *tile) < 4)
        .collect()
}

fn visible_known_tile_count_for_position(
    state: &ShenyangMahjongLoopState,
    position: usize,
    tile: i32,
) -> usize {
    let hand_count = state
        .hands
        .get(&position)
        .map(|hand| hand.iter().filter(|item| **item == tile).count())
        .unwrap_or(0);
    let own_meld_count = state
        .melds
        .get(&position)
        .into_iter()
        .flatten()
        .filter(|meld| meld_source_is_valid_for_position(state, position, meld))
        .filter(|meld| meld_shape_is_valid(meld))
        .flat_map(|meld| meld.tiles.iter())
        .filter(|item| **item == tile)
        .count();
    let public_count = public_unavailable_tiles_for_winner(state, position)
        .into_iter()
        .filter(|item| *item == tile)
        .count();
    hand_count + own_meld_count + public_count
}

fn ting_shape_wait_tiles_after_discard(
    state: &ShenyangMahjongLoopState,
    position: usize,
    discard_tile: i32,
    configs: &HashMap<String, i32>,
) -> Vec<i32> {
    let Some(mut hand) = state.hands.get(&position).cloned() else {
        return Vec::new();
    };
    if !remove_tiles(&mut hand, &[discard_tile]) {
        return Vec::new();
    }
    let melds = state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]);
    SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .filter(|tile| {
            let mut completed = hand.clone();
            completed.push(*tile);
            completed.sort_unstable();
            is_complete_win_with_configs(&completed, melds, configs)
        })
        .collect()
}

pub(crate) fn ting_discard_tiles_for_position(
    state: &ShenyangMahjongLoopState,
    position: usize,
    configs: &HashMap<String, i32>,
) -> Vec<i32> {
    if state.phase != ShenyangMahjongPhase::Play
        || state.current_position != position
        || state.claim_window.is_some()
        || state.is_ting(position)
        || state.is_ai_position(position)
        || !position_has_discardable_tile_count(state, position)
        || !position_hand_tiles_are_valid(state, position)
        || position_has_impossible_known_tile_count(state, position)
        || !position_meld_shapes_are_valid(state, position)
        || !position_meld_sources_are_valid(state, position)
    {
        return Vec::new();
    }
    let Some(hand) = state.hands.get(&position) else {
        return Vec::new();
    };
    let mut candidates = hand.clone();
    candidates.sort_unstable();
    candidates.dedup();
    candidates
        .into_iter()
        .filter(|tile| !ting_wait_tiles_after_discard(state, position, *tile, configs).is_empty())
        .collect()
}

#[cfg(test)]
fn is_single_wait_win(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
) -> bool {
    let Some(win_tile) = win_tile else {
        return false;
    };
    is_legal_single_wait_shape(hand_tiles, melds, win_tile)
}

#[cfg(test)]
fn is_single_wait_win_with_known_unavailable_tiles(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    known_unavailable_tiles: &[i32],
) -> bool {
    is_single_wait_win_with_known_unavailable_tiles_with_context(
        hand_tiles,
        melds,
        win_tile,
        ShenyangMahjongWinContext::new(),
        known_unavailable_tiles,
    )
}

#[cfg(test)]
fn is_single_wait_win_with_known_unavailable_tiles_with_context(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    context: ShenyangMahjongWinContext,
    known_unavailable_tiles: &[i32],
) -> bool {
    let Some(win_tile) = win_tile else {
        return false;
    };
    is_single_wait_shape_with_known_unavailable_tiles_with_context(
        hand_tiles,
        melds,
        win_tile,
        context,
        known_unavailable_tiles,
    )
}

fn is_valid_tile(tile: i32) -> bool {
    SHENYANG_MAHJONG_TILE_KINDS.contains(&tile)
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

fn known_tile_count(state: &ShenyangMahjongLoopState, tile: i32) -> usize {
    state.known_tile_count(tile)
}

fn score_cap_from_configs(configs: &HashMap<String, i32>) -> Option<i32> {
    configs
        .get("max_fan")
        .copied()
        .filter(|score_cap| *score_cap > 0)
}

fn maybe_record_settlement(
    room_service: &RoomService,
    room_key: &str,
    state: &ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
) {
    crate::official::settle_round(room_service, room_key, state, configs);
}

fn meld_primary_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    let expected_len = match meld.kind {
        ShenyangMahjongMeldKind::PENG => 3,
        ShenyangMahjongMeldKind::GANG => 4,
        ShenyangMahjongMeldKind::CHI | ShenyangMahjongMeldKind::XI_GANG => return None,
    };
    if meld.tiles.len() != expected_len {
        return None;
    }
    let tile = *meld.tiles.first()?;
    (is_valid_tile(tile) && meld.tiles.iter().all(|item| *item == tile)).then_some(tile)
}

fn meld_shape_is_valid(meld: &WsShenyangMahjongMeld) -> bool {
    is_valid_meld(meld)
}

fn meld_source_is_valid_for_position(
    state: &ShenyangMahjongLoopState,
    position: usize,
    meld: &WsShenyangMahjongMeld,
) -> bool {
    let player_positions = state
        .players_snapshot()
        .keys()
        .copied()
        .collect::<HashSet<_>>();
    meld_source_is_valid_for_positions(meld, position, &player_positions)
}

fn peng_meld_tile(meld: &WsShenyangMahjongMeld) -> Option<i32> {
    if meld.kind != ShenyangMahjongMeldKind::PENG {
        return None;
    }
    meld_primary_tile(meld)
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
    perform_discard_with_ting(
        room_service,
        room_key,
        state,
        configs,
        dispatch,
        position,
        tile,
        false,
    )
}

pub(crate) fn perform_discard_with_ting(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
    tile: i32,
    declare_ting: bool,
) -> bool {
    if state.phase != ShenyangMahjongPhase::Play
        || state.current_position != position
        || state.claim_window.is_some()
        || !position_has_discardable_tile_count(state, position)
        || !is_valid_tile(tile)
        || !position_hand_tiles_are_valid(state, position)
        || position_has_impossible_known_tile_count(state, position)
        || !position_meld_shapes_are_valid(state, position)
        || !position_meld_sources_are_valid(state, position)
    {
        return false;
    }
    if state.is_ting(position) {
        if state.last_drawn_tile != Some(tile)
            || ting_shape_wait_tiles_after_discard(state, position, tile, configs).is_empty()
        {
            return false;
        }
    } else if declare_ting
        && !ting_discard_tiles_for_position(state, position, configs).contains(&tile)
    {
        return false;
    }
    if !state.remove_tiles_from_hand(position, &[tile]) {
        return false;
    }
    if declare_ting {
        state.declare_ting(position);
    }
    state.clear_xi_gang_options(position);
    state.discards.entry(position).or_default().push(tile);
    state.last_drawn_tile = None;
    state.pending_gang_draw = false;
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
            xi_gang_options: Vec::new(),
            ting_discard_tiles: Vec::new(),
            is_ting: Some(state.is_ting(position)),
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
        push_claim_window_events(state, dispatch, &claim_event);
    }
    true
}

pub(crate) fn perform_self_draw_hu(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
) {
    if !can_self_draw_hu_with_configs(state, position, configs) {
        return;
    }

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
            xi_gang_options: Vec::new(),
            ting_discard_tiles: Vec::new(),
            is_ting: None,
        },
    );
    let win_tile = state.last_drawn_tile;
    let is_gang_draw = state.pending_gang_draw;
    let is_haidilao = state.wall_count() == 0;
    state.enter_settlement_with_reverse_win(
        vec![position],
        None,
        win_tile,
        true,
        false,
        is_gang_draw,
        is_haidilao,
    );
    maybe_record_settlement(room_service, room_key, state, configs);
    push_phase_change(
        room_service,
        room_key,
        dispatch,
        ShenyangMahjongPhase::Settlement,
        state.current_position,
        0,
    );
    if let Some(event) = build_settlement_event_with_configs(state, configs) {
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
    if !can_self_gang(state, position, target_tile) {
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
            push_claim_window_events(state, dispatch, &claim_event);
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
            xi_gang_options: Vec::new(),
            ting_discard_tiles: Vec::new(),
            is_ting: None,
        },
    );
    draw_after_gang_or_settle(room_service, room_key, state, configs, dispatch, position);
    true
}

pub(crate) fn perform_xi_gang(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
    tiles: &[i32],
) -> bool {
    let mut tiles = tiles.to_vec();
    tiles.sort_unstable();
    if !can_declare_xi_gang(state, position, &tiles)
        || !state.remove_tiles_from_hand(position, &tiles)
    {
        return false;
    }

    state.melds.entry(position).or_default().push(build_meld(
        ShenyangMahjongMeldKind::XI_GANG,
        tiles.clone(),
        None,
    ));
    if let Some(options) = state.xi_gang_options.get_mut(&position) {
        options.retain(|option| option != &tiles);
        if options.is_empty() {
            state.xi_gang_options.remove(&position);
        }
    }

    state.pending_gang_draw = false;
    let replacement_tile = if tiles.as_slice() == XI_GANG_WINDS {
        let Some(tile) = state.draw_for_position(position) else {
            return false;
        };
        Some(tile)
    } else {
        None
    };
    state.current_position = position;
    state.set_turn_countdown(current_play_time(configs));
    push_xi_gang_events(state, dispatch, position, &tiles);
    if let Some(tile) = replacement_tile {
        push_draw_events(
            room_service,
            room_key,
            state,
            configs,
            dispatch,
            position,
            tile,
        );
    }
    true
}

fn position_can_chi(
    state: &ShenyangMahjongLoopState,
    position: usize,
    configs: &HashMap<String, i32>,
) -> bool {
    allow_first_chi(configs) || position_has_open_meld(state, position)
}

fn position_can_claim_meld(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    !state.is_ting(position)
        && position_has_claimable_tile_count(state, position)
        && position_hand_tiles_are_valid(state, position)
        && !position_has_impossible_known_tile_count(state, position)
        && position_meld_shapes_are_valid(state, position)
        && position_meld_sources_are_valid(state, position)
}

fn position_hand_tiles_are_valid(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    state
        .hands
        .get(&position)
        .is_some_and(|hand| hand_tiles_are_valid(hand))
}

fn position_has_claimable_tile_count(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    position_has_virtual_tile_count(state, position, 13)
}

fn position_has_discardable_tile_count(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    position_has_virtual_tile_count(state, position, 14)
}

fn position_has_impossible_known_tile_count(
    state: &ShenyangMahjongLoopState,
    position: usize,
) -> bool {
    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let owns_tile = state
            .hands
            .get(&position)
            .is_some_and(|hand| hand.contains(&tile))
            || state.melds.get(&position).is_some_and(|melds| {
                melds
                    .iter()
                    .filter(|meld| meld_shape_is_valid(meld))
                    .any(|meld| meld.tiles.contains(&tile))
            });
        owns_tile && has_impossible_known_tile_count(state, tile)
    })
}

fn position_has_open_meld(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    state.melds.get(&position).is_some_and(|melds| {
        melds.iter().any(|meld| {
            meld_source_is_valid_for_position(state, position, meld) && is_door_opening_meld(meld)
        })
    })
}

fn position_has_valid_gang_meld(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    state.melds.get(&position).is_some_and(|melds| {
        melds.iter().any(|meld| {
            meld.kind == ShenyangMahjongMeldKind::GANG && meld_primary_tile(meld).is_some()
        })
    })
}

fn position_has_virtual_tile_count(
    state: &ShenyangMahjongLoopState,
    position: usize,
    expected_count: usize,
) -> bool {
    let Some(hand_count) = state.hands.get(&position).map(Vec::len) else {
        return false;
    };
    let meld_count = state
        .melds
        .get(&position)
        .map(|melds| {
            melds
                .iter()
                .filter(|meld| meld_shape_is_valid(meld))
                .count()
        })
        .unwrap_or_default();
    hand_count + meld_count * 3 == expected_count
}

fn position_meld_shapes_are_valid(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    state
        .melds
        .get(&position)
        .is_none_or(|melds| melds.iter().all(meld_shape_is_valid))
}

fn position_meld_sources_are_valid(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    state.melds.get(&position).is_none_or(|melds| {
        melds
            .iter()
            .all(|meld| meld_source_is_valid_for_position(state, position, meld))
    })
}

fn position_owns_last_drawn_tile(state: &ShenyangMahjongLoopState, position: usize) -> bool {
    state.last_drawn_tile.is_some_and(|last_drawn_tile| {
        is_valid_tile(last_drawn_tile)
            && (state
                .hands
                .get(&position)
                .is_some_and(|hand| hand.contains(&last_drawn_tile))
                || state.melds.get(&position).is_some_and(|melds| {
                    melds.iter().any(|meld| {
                        meld.kind == ShenyangMahjongMeldKind::XI_GANG
                            && meld.tiles.contains(&last_drawn_tile)
                    })
                }))
    })
}

fn positive_winner_positions_for_state(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    configs: &HashMap<String, i32>,
) -> Vec<usize> {
    let players = state.players_snapshot();
    let mut positions: Vec<usize> = players.keys().copied().collect();
    positions.sort_unstable();
    let score_changes = settlement_score_changes_for_state(state, &positions, settlement, configs);
    positive_winner_positions_from_scores(settlement, &score_changes).collect()
}

fn positive_winner_positions_from_scores<'a>(
    settlement: &'a crate::game_state::SettlementState,
    score_changes: &'a [WsShenyangMahjongScoreChange],
) -> impl Iterator<Item = usize> + 'a {
    let score_by_position = score_changes
        .iter()
        .map(|change| (change.position as usize, change.score))
        .collect::<HashMap<_, _>>();

    settlement
        .unique_winner_positions()
        .into_iter()
        .filter(move |position| score_by_position.get(position).copied().unwrap_or(0) > 0)
}

pub(crate) fn public_discards_for_position(
    state: &ShenyangMahjongLoopState,
    position: usize,
) -> Vec<i32> {
    state
        .discards
        .get(&position)
        .into_iter()
        .flatten()
        .copied()
        .filter(|tile| is_valid_tile(*tile))
        .collect()
}

fn public_melds_for_position(
    state: &ShenyangMahjongLoopState,
    position: usize,
    player_positions: &HashSet<usize>,
) -> Vec<WsShenyangMahjongMeld> {
    state
        .melds
        .get(&position)
        .into_iter()
        .flatten()
        .filter(|meld| meld_source_is_valid_for_positions(meld, position, player_positions))
        .filter(|meld| meld_shape_is_valid(meld))
        .cloned()
        .collect()
}

fn public_unavailable_tiles_for_winner(
    state: &ShenyangMahjongLoopState,
    winner: usize,
) -> Vec<i32> {
    let player_positions = state
        .players_snapshot()
        .keys()
        .copied()
        .collect::<HashSet<_>>();
    let mut tiles = Vec::new();
    for discards in state.discards.values() {
        tiles.extend(discards.iter().copied().filter(|tile| is_valid_tile(*tile)));
    }
    for (position, melds) in &state.melds {
        if *position == winner {
            continue;
        }
        for meld in melds
            .iter()
            .filter(|meld| meld_source_is_valid_for_positions(meld, *position, &player_positions))
            .filter(|meld| meld_shape_is_valid(meld))
        {
            tiles.extend(
                meld.tiles
                    .iter()
                    .copied()
                    .filter(|tile| is_valid_tile(*tile)),
            );
        }
    }
    tiles
}

fn push_claim_window_events(
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    event: &WsShenyangMahjongClaimWindowEvent,
) {
    for (position, (session_id, _)) in state.players_snapshot() {
        let can_respond = event.eligible_positions.contains(&(position as i32));
        push_direct_event(
            dispatch,
            session_id,
            WsCode::CLAIM_WINDOW as i32,
            claim_window_event_for_viewer(event, position, can_respond),
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
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
    tile: i32,
) {
    let name = state.player_name(position);
    let players = state.players_snapshot();
    for (member_position, (session_id, _)) in players {
        let is_drawing_player = member_position == position;
        let tiles = if is_drawing_player {
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
                target_tile: is_drawing_player.then_some(tile),
                from_position: None,
                wall_count: state.wall_count() as i32,
                xi_gang_options: if is_drawing_player {
                    state.xi_gang_options_for_position(position)
                } else {
                    Vec::new()
                },
                ting_discard_tiles: if is_drawing_player {
                    ting_discard_tiles_for_position(state, position, configs)
                } else {
                    Vec::new()
                },
                is_ting: state.is_ting(position).then_some(true),
            },
        );
    }
}

fn push_private_table_snapshot(
    state: &ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
    position: usize,
) {
    if let Some((session_id, _)) = state.players_snapshot().get(&position) {
        push_direct_event(
            dispatch,
            *session_id,
            WsCode::TABLE_SNAPSHOT as i32,
            build_table_snapshot_event_with_configs(state, position, configs),
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
    configs: &HashMap<String, i32>,
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
                last_drawn_tile: if position == state.current_position {
                    state.last_drawn_tile
                } else {
                    None
                },
                ting_discard_tiles: ting_discard_tiles_for_position(state, position, configs),
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
    room_service.broadcast_connected(room_key, code, payload, dispatch);
}

fn push_xi_gang_events(
    state: &ShenyangMahjongLoopState,
    dispatch: &mut Dispatch,
    position: usize,
    tiles: &[i32],
) {
    let name = state.player_name(position);
    for (member_position, (session_id, _)) in state.players_snapshot() {
        push_direct_event(
            dispatch,
            session_id,
            WsCode::PLAY as i32,
            WsShenyangMahjongPlayEvent {
                name: name.clone(),
                position: position as i32,
                action: ShenyangMahjongAction::XI_GANG,
                tiles: tiles.to_vec(),
                target_tile: None,
                from_position: None,
                wall_count: state.wall_count() as i32,
                xi_gang_options: if member_position == position {
                    state.xi_gang_options_for_position(position)
                } else {
                    Vec::new()
                },
                ting_discard_tiles: Vec::new(),
                is_ting: None,
            },
        );
    }
}

pub(crate) fn redeal_after_settlement_with_configs(
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
) {
    if let Some(settlement) = state.settlement.as_ref() {
        let effective_winner_positions =
            positive_winner_positions_for_state(state, settlement, configs);
        if let Some(settlement) = state.settlement.as_mut() {
            settlement.winner_positions = effective_winner_positions;
        }
    }
    state.redeal();
}

pub(crate) fn resolve_claim_window(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) {
    if state.phase != ShenyangMahjongPhase::Play {
        return;
    }
    let Some(claim_window) = state.claim_window.clone() else {
        return;
    };
    let is_rob_gang = matches!(claim_window.kind, ClaimWindowKind::RobGang);
    let claim_matches_source = claim_window_matches_source(state, &claim_window);
    let invalid_claim_tile_count = has_impossible_known_tile_count(state, claim_window.tile);
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
        if *position == claim_window.from_position {
            continue;
        }
        let hand = state.hands.get(position).cloned().unwrap_or_default();
        let can_claim_meld = position_can_claim_meld(state, *position);
        match claim_window.responses.get(position) {
            Some(ClaimResponse::Hu) => {
                if claim_matches_source
                    && can_claim_hu_with_configs(state, *position, claim_window.tile, configs)
                {
                    hu_positions.push(*position);
                }
            }
            Some(ClaimResponse::Peng)
                if !is_rob_gang
                    && claim_matches_source
                    && !invalid_claim_tile_count
                    && can_claim_meld
                    && can_peng(&hand, claim_window.tile) =>
            {
                meld_claims.push((*position, ClaimResponse::Peng));
            }
            Some(ClaimResponse::Gang)
                if !is_rob_gang
                    && claim_matches_source
                    && !invalid_claim_tile_count
                    && can_claim_meld
                    && state.wall_count() > 0
                    && can_gang(&hand, claim_window.tile) =>
            {
                meld_claims.push((*position, ClaimResponse::Gang));
            }
            Some(ClaimResponse::Chi { consume_tiles })
                if !is_rob_gang
                    && claim_matches_source
                    && !invalid_claim_tile_count
                    && can_claim_meld
                    && position_can_chi(state, *position, configs)
                    && *position == state.next_position(claim_window.from_position)
                    && can_chi(&hand, claim_window.tile, consume_tiles) =>
            {
                chi_positions.push((*position, consume_tiles.clone()));
            }
            _ => {}
        }
    }

    if !hu_positions.is_empty() {
        if is_rob_gang {
            let _ = state.remove_tiles_from_hand(claim_window.from_position, &[claim_window.tile]);
        } else {
            state.remove_last_discard(claim_window.from_position);
        }
        let mut winners = hu_positions.clone();
        winners.sort_by_key(|position| {
            ordered_positions
                .iter()
                .position(|item| item == position)
                .unwrap_or(usize::MAX)
        });
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
                    xi_gang_options: Vec::new(),
                    ting_discard_tiles: Vec::new(),
                    is_ting: None,
                },
            );
        }
        state.enter_settlement_with_reverse_win(
            winners,
            Some(claim_window.from_position),
            Some(claim_window.tile),
            false,
            is_rob_gang,
            false,
            false,
        );
        maybe_record_settlement(room_service, room_key, state, configs);
        push_phase_change(
            room_service,
            room_key,
            dispatch,
            ShenyangMahjongPhase::Settlement,
            state.current_position,
            0,
        );
        if let Some(event) = build_settlement_event_with_configs(state, configs) {
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
        if !claim_matches_source {
            state.claim_window = None;
            state.set_turn_countdown(current_play_time(configs));
            state.set_action_received(false);
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
                            xi_gang_options: Vec::new(),
                            ting_discard_tiles: Vec::new(),
                            is_ting: None,
                        },
                    );
                    push_private_table_snapshot(state, configs, dispatch, winner);
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
                            xi_gang_options: Vec::new(),
                            ting_discard_tiles: Vec::new(),
                            is_ting: None,
                        },
                    );
                    if let Some(tile) = state.draw_for_position(winner) {
                        state.pending_gang_draw = true;
                        state.set_turn_countdown(current_play_time(configs));
                        push_draw_events(
                            room_service,
                            room_key,
                            state,
                            configs,
                            dispatch,
                            winner,
                            tile,
                        );
                        push_phase_change(
                            room_service,
                            room_key,
                            dispatch,
                            state.phase,
                            state.current_position,
                            state.turn_countdown(),
                        );
                    } else {
                        settle_draw(room_service, room_key, state, configs, dispatch);
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
                    xi_gang_options: Vec::new(),
                    ting_discard_tiles: Vec::new(),
                    is_ting: None,
                },
            );
            push_private_table_snapshot(state, configs, dispatch, winner);
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
    advance_to_next_turn(room_service, room_key, state, configs, dispatch);
}

fn rob_gang_claim_matches_source(
    state: &ShenyangMahjongLoopState,
    tile: i32,
    from_position: usize,
) -> bool {
    if state.wall_count() == 0
        || state.last_drawn_tile != Some(tile)
        || !position_has_discardable_tile_count(state, from_position)
        || has_impossible_known_tile_count(state, tile)
    {
        return false;
    }
    let hand = state
        .hands
        .get(&from_position)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let melds = state
        .melds
        .get(&from_position)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    can_added_gang(hand, melds, tile)
}

pub(crate) fn settle_draw(
    room_service: &RoomService,
    room_key: &str,
    state: &mut ShenyangMahjongLoopState,
    configs: &HashMap<String, i32>,
    dispatch: &mut Dispatch,
) {
    state.enter_settlement(Vec::new(), None, None, false);
    maybe_record_settlement(room_service, room_key, state, configs);
    push_phase_change(
        room_service,
        room_key,
        dispatch,
        ShenyangMahjongPhase::Settlement,
        state.current_position,
        0,
    );
    if let Some(event) = build_settlement_event_with_configs(state, configs) {
        push_room_event(
            room_service,
            room_key,
            dispatch,
            WsCode::GAME_OVER as i32,
            event,
        );
    }
}

pub(crate) fn settlement_from_position(
    settlement: &crate::game_state::SettlementState,
) -> Option<usize> {
    if settlement.is_self_draw {
        None
    } else {
        settlement.from_position
    }
}

fn settlement_is_gang_draw(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
) -> bool {
    settlement
        .winner_positions
        .iter()
        .any(|winner| settlement_winner_is_gang_draw(state, settlement, *winner))
}

fn settlement_is_haidilao(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
) -> bool {
    settlement.is_self_draw && settlement.is_haidilao && state.wall_count() == 0
}

pub(crate) fn settlement_is_reverse_win(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
) -> bool {
    let Some(from_position) = settlement_from_position(settlement) else {
        return false;
    };
    let Some(win_tile) = settlement.win_tile else {
        return false;
    };
    settlement.is_reverse_win
        && position_meld_sources_are_valid(state, from_position)
        && state.melds.get(&from_position).is_some_and(|melds| {
            melds
                .iter()
                .any(|meld| is_door_opening_meld(meld) && peng_meld_tile(meld) == Some(win_tile))
        })
}

pub(crate) fn settlement_score_changes_for_state(
    state: &ShenyangMahjongLoopState,
    positions: &[usize],
    settlement: &crate::game_state::SettlementState,
    configs: &HashMap<String, i32>,
) -> Vec<WsShenyangMahjongScoreChange> {
    let mut sorted_positions = positions.to_vec();
    sorted_positions.sort_unstable();
    let winner_positions = valid_settlement_winner_positions(&sorted_positions, settlement);

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
    let loser_positions = sorted_positions
        .iter()
        .copied()
        .filter(|position| !winner_set.contains(position))
        .collect::<Vec<_>>();
    let payers: Vec<usize> = if settlement.is_self_draw {
        loser_positions.clone()
    } else {
        settlement.from_position.into_iter().collect()
    };
    let all_losers_closed = loser_positions.len() >= 3
        && loser_positions
            .iter()
            .all(|position| !position_has_open_meld(state, *position));
    let mut totals = sorted_positions
        .iter()
        .map(|position| (*position, 0))
        .collect::<HashMap<_, _>>();

    for winner in &winner_positions {
        let winner_fan = winner_hand_fan_with_configs(state, settlement, *winner, configs);
        if winner_fan <= 0 {
            continue;
        }
        for payer in &payers {
            if payer == winner {
                continue;
            }
            let payer_is_closed = !position_has_open_meld(state, *payer);
            let payment_fan = shenyang_payment_fan(
                winner_fan,
                *winner == state.dealer_position,
                *payer == state.dealer_position,
                payer_is_closed,
                all_losers_closed,
            );
            let payment = shenyang_score_for_fan_with_cap(
                payment_fan.max(0),
                score_cap_from_configs(configs),
            );
            *totals.entry(*winner).or_default() += payment;
            *totals.entry(*payer).or_default() -= payment;
        }
    }

    sorted_positions
        .into_iter()
        .map(|position| WsShenyangMahjongScoreChange {
            position: position as i32,
            score: totals.get(&position).copied().unwrap_or(0),
        })
        .collect()
}

pub(crate) fn settlement_time(configs: &HashMap<String, i32>) -> u64 {
    config_value(configs, "settlement_time", 5).max(1) as u64
}

fn settlement_winner_has_valid_win_tile(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
) -> bool {
    match settlement.win_tile {
        Some(win_tile) if !is_valid_tile(win_tile) => false,
        Some(win_tile) if settlement.is_self_draw => {
            state
                .hands
                .get(&winner)
                .is_some_and(|hand| hand.contains(&win_tile))
                || state.melds.get(&winner).is_some_and(|melds| {
                    melds.iter().any(|meld| {
                        meld.kind == ShenyangMahjongMeldKind::XI_GANG
                            && meld.tiles.contains(&win_tile)
                    })
                })
        }
        Some(_) => true,
        None => settlement.is_self_draw,
    }
}

fn settlement_winner_is_gang_draw(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
) -> bool {
    settlement.is_self_draw
        && settlement.is_gang_draw
        && settlement.winner_positions.contains(&winner)
        && position_has_valid_gang_meld(state, winner)
}

fn valid_settlement_winner_positions(
    positions: &[usize],
    settlement: &crate::game_state::SettlementState,
) -> Vec<usize> {
    let position_set = positions.iter().copied().collect::<HashSet<_>>();
    let winner_positions = settlement
        .unique_winner_positions()
        .into_iter()
        .filter(|position| position_set.contains(position))
        .collect::<Vec<_>>();

    if settlement.is_self_draw {
        return (winner_positions.len() == 1)
            .then_some(winner_positions)
            .unwrap_or_default();
    }

    settlement
        .from_position
        .filter(|from_position| {
            position_set.contains(from_position) && !winner_positions.contains(from_position)
        })
        .map(|_| winner_positions)
        .unwrap_or_default()
}

fn winner_final_hand_tiles(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    position: usize,
) -> Vec<i32> {
    let mut hand_tiles = state.hands.get(&position).cloned().unwrap_or_default();
    if !settlement.is_self_draw
        && settlement.winner_positions.contains(&position)
        && let Some(tile) = settlement.win_tile
    {
        hand_tiles.push(tile);
        hand_tiles.sort_unstable();
    }
    hand_tiles
}

#[cfg(test)]
fn winner_hand_fan(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
) -> i32 {
    winner_hand_fan_with_context(state, settlement, winner, ShenyangMahjongWinContext::new())
}

fn winner_hand_fan_with_configs(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
    configs: &HashMap<String, i32>,
) -> i32 {
    let mut fan = winner_hand_fan_with_context(
        state,
        settlement,
        winner,
        ShenyangMahjongWinContext::from_configs(configs),
    );
    if fan > 0 && configs.get("ting_fan").copied() == Some(1) && state.is_ting(winner) {
        fan += 1;
    }
    fan
}

fn winner_hand_fan_with_context(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
    context: ShenyangMahjongWinContext,
) -> i32 {
    if !settlement_winner_has_valid_win_tile(state, settlement, winner)
        || winner_has_impossible_known_tile_count(state, settlement, winner)
        || !position_meld_sources_are_valid(state, winner)
    {
        return 0;
    }
    let hand_tiles = winner_final_hand_tiles(state, settlement, winner);
    let melds = state.melds.get(&winner).map(Vec::as_slice).unwrap_or(&[]);
    if !is_complete_win_with_melds_with_context(&hand_tiles, melds, context) {
        return 0;
    }
    let known_unavailable_tiles = public_unavailable_tiles_for_winner(state, winner);
    let mut fan = shenyang_score_visible_win_fan(
        &hand_tiles,
        melds,
        settlement.win_tile,
        context,
        &known_unavailable_tiles,
    );
    if settlement_is_reverse_win(state, settlement) {
        fan += 1;
    }
    if settlement_winner_is_gang_draw(state, settlement, winner) {
        fan += 1;
    }
    if settlement_is_haidilao(state, settlement) {
        fan += 1;
    }
    fan
}

fn winner_has_impossible_known_tile_count(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    position: usize,
) -> bool {
    let hand_tiles = winner_final_hand_tiles(state, settlement, position);
    let melds = state.melds.get(&position).map(Vec::as_slice).unwrap_or(&[]);
    let unrepresented_claimed_tile =
        if settlement.is_self_draw || !settlement.winner_positions.contains(&position) {
            None
        } else {
            settlement.win_tile.filter(|tile| {
                settlement
                    .from_position
                    .and_then(|from_position| state.discards.get(&from_position))
                    .and_then(|discards| discards.last())
                    .copied()
                    != Some(*tile)
            })
        };

    SHENYANG_MAHJONG_TILE_KINDS.into_iter().any(|tile| {
        let owns_tile = hand_tiles.contains(&tile)
            || melds
                .iter()
                .filter(|meld| meld_shape_is_valid(meld))
                .any(|meld| meld.tiles.contains(&tile));
        let known_count =
            known_tile_count(state, tile) + usize::from(unrepresented_claimed_tile == Some(tile));
        owns_tile && known_count > 4
    })
}

pub(crate) fn winner_pattern_with_context(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    context: ShenyangMahjongWinContext,
) -> ShenyangMahjongWinPattern {
    if !is_complete_win_with_melds_with_context(hand_tiles, melds, context) {
        return ShenyangMahjongWinPattern::Standard;
    }
    shenyang_win_pattern(hand_tiles, melds)
}

mod handler;

#[cfg(test)]
mod tests;

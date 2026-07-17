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
    ShenyangMahjongWinRules, WIN_RULE_SHENYANG_BASIC, XI_GANG_WINDS, can_chi, can_concealed_gang,
    can_gang, can_peng, is_complete_win_with_melds_for_rules, is_door_opening_meld, is_valid_meld,
    is_xi_gang_tiles, shenyang_score_visible_win_fan, shenyang_win_pattern, tiles_in_hand,
};
#[cfg(test)]
use crate::rules::{
    is_seven_pairs_win, is_single_wait_shape_with_known_unavailable_tiles_for_rules,
    is_single_wait_shape_with_rule, shenyang_score_four_gui_yi_fan, shenyang_score_meld_fan,
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

    settle_draw(room_service, room_key, state, configs, dispatch);
}

fn allow_first_chi(configs: &HashMap<String, i32>) -> bool {
    config_value(configs, "allow_first_chi", 1) == 1
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
            hand_count: state
                .hands
                .get(&position)
                .map(|hand| hand.len())
                .unwrap_or(0) as i32,
            discards: public_discards_for_position(state, position),
            melds: public_melds_for_position(state, position, &player_positions),
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
    }
}

fn build_winner_details(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    score_changes: &[WsShenyangMahjongScoreChange],
    configs: &HashMap<String, i32>,
) -> Vec<WsShenyangMahjongWinnerDetail> {
    let rules = ShenyangMahjongWinRules::from_configs(configs);
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
            let pattern = winner_pattern_with_rules(&hand_tiles, melds, rules);
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

fn capped_winner_hand_fan(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
    configs: &HashMap<String, i32>,
) -> i32 {
    let fan = winner_hand_fan_with_configs(state, settlement, winner, configs);
    max_fan_from_configs(configs)
        .map(|max_fan| fan.min(max_fan))
        .unwrap_or(fan)
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
    is_complete_win_with_melds_for_rules(
        tiles,
        melds,
        ShenyangMahjongWinRules::from_configs(configs),
    )
}

#[cfg(test)]
fn is_single_wait_win(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    win_rule: i32,
) -> bool {
    let Some(win_tile) = win_tile else {
        return false;
    };
    is_single_wait_shape_with_rule(hand_tiles, melds, win_tile, win_rule)
}

#[cfg(test)]
fn is_single_wait_win_with_known_unavailable_tiles(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    win_rule: i32,
    known_unavailable_tiles: &[i32],
) -> bool {
    is_single_wait_win_with_known_unavailable_tiles_for_rules(
        hand_tiles,
        melds,
        win_tile,
        ShenyangMahjongWinRules::new(win_rule),
        known_unavailable_tiles,
    )
}

#[cfg(test)]
fn is_single_wait_win_with_known_unavailable_tiles_for_rules(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: Option<i32>,
    rules: ShenyangMahjongWinRules,
    known_unavailable_tiles: &[i32],
) -> bool {
    let Some(win_tile) = win_tile else {
        return false;
    };
    is_single_wait_shape_with_known_unavailable_tiles_for_rules(
        hand_tiles,
        melds,
        win_tile,
        rules,
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

fn max_fan_from_configs(configs: &HashMap<String, i32>) -> Option<i32> {
    configs.get("max_fan").copied().filter(|fan| *fan > 0)
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
    if !state.remove_tiles_from_hand(position, &[tile]) {
        return false;
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
        push_draw_events(room_service, room_key, state, dispatch, position, tile);
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
    position_has_claimable_tile_count(state, position)
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
                    xi_gang_options: Vec::new(),
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
                            xi_gang_options: Vec::new(),
                        },
                    );
                    if let Some(tile) = state.draw_for_position(winner) {
                        state.pending_gang_draw = true;
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

#[cfg(test)]
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
        let winner_fan = capped_winner_hand_fan(state, settlement, *winner, configs);
        if winner_fan <= 0 {
            continue;
        }
        for payer in &payers {
            if payer == winner {
                continue;
            }
            let mut payment = winner_fan;
            if *winner == state.dealer_position {
                payment += 1;
            }
            if *payer == state.dealer_position {
                payment += 1;
            }
            if !position_has_open_meld(state, *payer) {
                payment += if all_losers_closed { 2 } else { 1 };
            }
            payment = payment.max(1);
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
    winner_hand_fan_with_rule(state, settlement, winner, crate::rules::WIN_RULE_RELAXED)
}

fn winner_hand_fan_with_configs(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
    configs: &HashMap<String, i32>,
) -> i32 {
    winner_hand_fan_with_rules(
        state,
        settlement,
        winner,
        ShenyangMahjongWinRules::from_configs(configs),
    )
}

#[cfg(test)]
fn winner_hand_fan_with_rule(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
    win_rule: i32,
) -> i32 {
    winner_hand_fan_with_rules(
        state,
        settlement,
        winner,
        ShenyangMahjongWinRules::new(win_rule),
    )
}

fn winner_hand_fan_with_rules(
    state: &ShenyangMahjongLoopState,
    settlement: &crate::game_state::SettlementState,
    winner: usize,
    rules: ShenyangMahjongWinRules,
) -> i32 {
    if !settlement_winner_has_valid_win_tile(state, settlement, winner)
        || winner_has_impossible_known_tile_count(state, settlement, winner)
        || !position_meld_sources_are_valid(state, winner)
    {
        return 0;
    }
    let hand_tiles = winner_final_hand_tiles(state, settlement, winner);
    let melds = state.melds.get(&winner).map(Vec::as_slice).unwrap_or(&[]);
    if !is_complete_win_with_melds_for_rules(&hand_tiles, melds, rules) {
        return 0;
    }
    let known_unavailable_tiles = public_unavailable_tiles_for_winner(state, winner);
    let mut fan = shenyang_score_visible_win_fan(
        &hand_tiles,
        melds,
        settlement.win_tile,
        rules,
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

pub(crate) fn winner_pattern_with_rules(
    hand_tiles: &[i32],
    melds: &[WsShenyangMahjongMeld],
    rules: ShenyangMahjongWinRules,
) -> ShenyangMahjongWinPattern {
    if rules.win_rule == WIN_RULE_SHENYANG_BASIC
        && !is_complete_win_with_melds_for_rules(hand_tiles, melds, rules)
    {
        return ShenyangMahjongWinPattern::Standard;
    }
    shenyang_win_pattern(hand_tiles, melds)
}

impl ShenyangMahjongGameHandler {
    fn current_loop_state(
        &self,
        room_service: &RoomService,
        room_key: &str,
    ) -> Option<LoopStateHandle> {
        let state = self.loop_state(room_key)?;
        let state_common = Arc::clone(&state.lock().unwrap().base);
        let room_common = room_service.room_common_state(room_key)?;
        let is_running = !state_common.lock().unwrap().stop_requested();
        (is_running && Arc::ptr_eq(&state_common, &room_common)).then_some(state)
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
        let Ok(payload) = RoomService::parse_payload::<WsShenyangMahjongPlayRequest>(data) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::ERROR_FORMAT,
            );
        };
        let Some(loop_state) = self.current_loop_state(room_service, &room_key) else {
            return room_service.error_response(
                session_id,
                Routes::PLAY as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };

        let configs = room_service.room_configs(&room_key).unwrap_or_default();
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
                let (
                    claim_tile,
                    from_position,
                    is_rob_gang,
                    eligible_positions,
                    already_responded,
                    claim_matches_source,
                ) = {
                    let claim_window = state.claim_window.as_ref().unwrap();
                    (
                        claim_window.tile,
                        claim_window.from_position,
                        matches!(claim_window.kind, ClaimWindowKind::RobGang),
                        claim_window.eligible_positions.clone(),
                        claim_window.responses.contains_key(&position),
                        claim_window_matches_source(&state, claim_window),
                    )
                };
                if position == from_position
                    || !eligible_positions.contains(&position)
                    || already_responded
                {
                    return room_service.error_response(
                        session_id,
                        Routes::PLAY as i32,
                        WsResponseCode::NO_PERMISSION,
                    );
                }
                let hand = state.hands.get(&position).cloned().unwrap_or_default();
                let invalid_claim_tile_count = has_impossible_known_tile_count(&state, claim_tile);
                let can_claim_meld = position_can_claim_meld(&state, position);
                let response = match payload.action {
                    ShenyangMahjongAction::PASS => ClaimResponse::Pass,
                    ShenyangMahjongAction::HU => {
                        if !claim_matches_source
                            || !can_claim_hu_with_configs(&state, position, claim_tile, &configs)
                        {
                            return room_service.error_response(
                                session_id,
                                Routes::PLAY as i32,
                                WsResponseCode::NO_PERMISSION,
                            );
                        }
                        ClaimResponse::Hu
                    }
                    ShenyangMahjongAction::PENG => {
                        if is_rob_gang
                            || !claim_matches_source
                            || invalid_claim_tile_count
                            || !can_claim_meld
                        {
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
                        if is_rob_gang
                            || !claim_matches_source
                            || invalid_claim_tile_count
                            || !can_claim_meld
                            || state.wall_count() == 0
                        {
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
                        if is_rob_gang
                            || !claim_matches_source
                            || invalid_claim_tile_count
                            || !can_claim_meld
                            || !position_can_chi(&state, position, &configs)
                        {
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
                            &configs,
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
                    ShenyangMahjongAction::XI_GANG => {
                        if !perform_xi_gang(
                            room_service,
                            &room_key,
                            &mut state,
                            &configs,
                            &mut dispatch,
                            position,
                            &payload.tiles,
                        ) {
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
        if !room_service.require_room_membership(session_id, Routes::START as i32, &mut dispatch) {
            return dispatch;
        }
        let Some(room_key) = room_service.room_key_of(session_id) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        };
        if !room_service.room_is_ready_to_start(&room_key) {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NOT_IN_RANGE,
            );
        }
        let Some(mut shared_common_state) = room_service.room_common_state(&room_key) else {
            return room_service.error_response(
                session_id,
                Routes::START as i32,
                WsResponseCode::NO_PERMISSION,
            );
        };
        if shared_common_state.lock().unwrap().stop_requested() {
            let Some(next_common_state) =
                room_service.reset_room_common_state_for_new_game(&room_key)
            else {
                return room_service.error_response(
                    session_id,
                    Routes::START as i32,
                    WsResponseCode::NO_PERMISSION,
                );
            };
            shared_common_state = next_common_state;
        }

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

        room_service.broadcast(
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
        let Some(loop_state) = self.current_loop_state(room_service, &room_key) else {
            return;
        };
        let configs = room_service.room_configs(&room_key).unwrap_or_default();
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

    fn build_game_state(&self) -> Box<dyn ws_common::GameState> {
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

    use crate::rules::WIN_RULE_RELAXED;
    use ws_common::CommonGameState;

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
            &relaxed_configs(),
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
    fn added_gang_rejects_concealed_peng_source() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                None,
            )],
        );
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let original_melds = state.melds.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        let melds = state.melds.get(&0).expect("melds should stay");
        assert_eq!(melds.len(), original_melds.len());
        assert_eq!(melds[0].kind, original_melds[0].kind);
        assert_eq!(melds[0].tiles, original_melds[0].tiles);
        assert_eq!(melds[0].from_position, original_melds[0].from_position);
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn added_gang_rejects_extra_copy_after_peng() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
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
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let original_melds = state.melds.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        let melds = state.melds.get(&0).expect("melds should stay");
        assert_eq!(melds.len(), original_melds.len());
        assert_eq!(melds[0].kind, original_melds[0].kind);
        assert_eq!(melds[0].tiles, original_melds[0].tiles);
        assert_eq!(melds[0].from_position, original_melds[0].from_position);
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
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
    fn claim_options_allow_closed_sequence_dragon_pair_win_when_first_chi_disabled() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35]);
        let default_configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let disabled_configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);

        let default_options = build_claim_options(&state, 35, 0, &default_configs);
        let disabled_options = build_claim_options(&state, 35, 0, &disabled_configs);

        assert!(!default_options.iter().any(|option| option.position == 1));
        assert!(
            disabled_options
                .iter()
                .any(|option| option.position == 1 && option.can_hu)
        );
    }

    #[test]
    fn claim_options_allow_closed_pure_one_suit_for_basic_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9]);
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        let options = build_claim_options(&state, 9, 0, &configs);

        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("closed pure one suit should be allowed");
        assert!(player.can_hu);
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
    fn claim_options_allow_open_pure_one_suit_for_basic_rule() {
        let mut state = playable_state();
        state.hands.insert(1, vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![2, 3, 4],
                Some(0),
            )],
        );
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        let options = build_claim_options(&state, 8, 0, &configs);
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("open pure one suit player should be able to hu");

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
    fn claim_options_block_only_first_chi_when_configured() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        let configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);

        let options = build_claim_options(&state, 3, 0, &configs);

        assert!(
            options
                .iter()
                .all(|option| option.position != 1 || option.chi_options.is_empty())
        );

        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::XI_GANG,
                vec![35, 36, 37],
                None,
            )],
        );

        let options = build_claim_options(&state, 3, 0, &configs);
        assert!(
            options
                .iter()
                .all(|option| option.position != 1 || option.chi_options.is_empty())
        );

        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![9, 9, 9, 9],
                Some(2),
            )],
        );

        let options = build_claim_options(&state, 3, 0, &configs);
        let gang_opened_player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("open gang next player should retain chi options");
        assert!(gang_opened_player.chi_options.contains(&vec![1, 2]));

        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![21, 21, 21, 21],
                None,
            )],
        );

        let options = build_claim_options(&state, 3, 0, &configs);
        assert!(
            options
                .iter()
                .all(|option| option.position != 1 || option.chi_options.is_empty())
        );

        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![21, 21, 21],
                Some(2),
            )],
        );

        let options = build_claim_options(&state, 3, 0, &configs);
        let opened_player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("opened next player should retain chi options");
        assert!(opened_player.chi_options.contains(&vec![1, 2]));
    }

    #[test]
    fn claim_options_count_existing_gang_as_three_virtual_tiles() {
        let mut state = playable_state();
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 12, 13, 21, 22, 23]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![11, 11, 11, 11],
                Some(2),
            )],
        );

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());
        let option = options
            .iter()
            .find(|option| option.position == 1)
            .expect("existing Gang should count as one virtual set");

        assert!(option.can_peng);
    }

    #[test]
    fn claim_options_do_not_count_concealed_gang_as_open_for_basic_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![1, 1, 1, 1],
                None,
            )],
        );
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        let options = build_claim_options(&state, 35, 0, &configs);

        assert!(!options.iter().any(|option| option.position == 1));
    }

    #[test]
    fn claim_options_hide_gang_when_only_impossible_fifth_wall_copy_remains() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.discards.insert(2, vec![9, 9, 9, 9]);
        state.wall = vec![9];

        let options = build_claim_options(&state, 3, 0, &HashMap::new());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("player should still be able to peng");

        assert!(player.can_peng);
        assert!(!player.can_gang);
    }

    #[test]
    fn claim_options_hide_gang_when_only_invalid_wall_tiles_remain() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![99, -1];

        let options = build_claim_options(&state, 3, 0, &HashMap::new());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("player should still be able to peng");

        assert!(player.can_peng);
        assert!(!player.can_gang);
    }

    #[test]
    fn claim_options_hide_gang_when_replacement_tile_is_unavailable() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall.clear();

        let options = build_claim_options(&state, 3, 0, &HashMap::new());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("player should still be able to peng");

        assert!(player.can_peng);
        assert!(!player.can_gang);
    }

    #[test]
    fn claim_options_ignore_malformed_melds_for_known_tile_count() {
        let mut state = playable_state();
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![3, 3, 3],
                Some(1),
            )],
        );

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("malformed meld should not block legal claim options");

        assert_eq!(known_tile_count(&state, 3), 3);
        assert!(player.can_peng);
    }

    #[test]
    fn claim_options_ignore_melds_with_invalid_sources_for_known_tile_count() {
        let mut state = playable_state();
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());
        let player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("invalid-source meld should not block legal claim options");

        assert_eq!(known_tile_count(&state, 3), 3);
        assert!(player.can_peng);
    }

    #[test]
    fn claim_options_list_chi_for_shenyang_basic_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        let options = build_claim_options(&state, 3, 0, &configs);
        let next_player = options
            .iter()
            .find(|option| option.position == 1)
            .expect("next player should have chi options");

        assert!(next_player.chi_options.contains(&vec![1, 2]));
        assert!(next_player.chi_options.contains(&vec![2, 4]));
    }

    #[test]
    fn claim_options_list_concrete_actions() {
        let mut state = playable_state();
        state.wall = vec![36];
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![1, 2, 2, 3, 3, 3, 4, 11, 12, 13, 21, 22, 23]);
        state
            .hands
            .insert(2, vec![4, 5, 6, 7, 8, 11, 12, 13, 21, 22, 23, 31, 31]);
        state
            .hands
            .insert(3, vec![1, 5, 7, 9, 11, 13, 15, 17, 21, 23, 25, 31, 35]);

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());
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
    fn claim_options_reject_impossible_fifth_tile_chi() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 3, 3, 3, 7, 8, 9, 11, 12, 13, 21]);

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert!(!options.iter().any(|option| option.position == 1));
    }

    #[test]
    fn claim_options_reject_impossible_fifth_tile_claims() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 3, 7, 8, 9, 11, 12, 13, 21, 22, 31]);

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert!(!options.iter().any(|option| option.position == 1));
    }

    #[test]
    fn claim_options_reject_impossible_table_known_tile_claims() {
        let mut state = playable_state();
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state
            .hands
            .insert(2, vec![3, 3, 7, 8, 9, 14, 15, 16, 24, 25, 26, 32, 36]);

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert!(known_tile_count(&state, 3) > 4);
        assert!(options.is_empty());
    }

    #[test]
    fn claim_options_reject_melds_from_impossible_known_tile_state() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13]);
        state.discards.insert(0, vec![3]);
        state.discards.insert(2, vec![9]);
        state.wall = vec![37];

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert_eq!(known_tile_count(&state, 9), 5);
        assert!(position_has_impossible_known_tile_count(&state, 1));
        assert!(!options.iter().any(|option| option.position == 1));
    }

    #[test]
    fn claim_options_reject_player_with_invalid_hand_tile() {
        let mut state = playable_state();
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 99]);

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert!(!options.iter().any(|option| option.position == 1));
    }

    #[test]
    fn claim_options_reject_player_with_malformed_owned_meld() {
        let mut state = playable_state();
        state.discards.insert(0, vec![3]);
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![9, 9],
                Some(0),
            )],
        );

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert!(!options.iter().any(|option| option.position == 1));
    }

    #[test]
    fn claim_options_reject_public_fifth_copy_used_by_winner() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
        state.discards.insert(0, vec![6]);
        state.discards.insert(3, vec![1]);

        let invalid_options = build_claim_options(&state, 6, 0, &relaxed_configs());

        assert_eq!(known_tile_count(&state, 1), 5);
        assert!(position_has_impossible_known_tile_count(&state, 2));
        assert!(
            !invalid_options
                .iter()
                .any(|option| option.position == 2 && option.can_hu)
        );

        state.discards.insert(3, vec![9, 9, 9, 9, 9]);
        let unrelated_options = build_claim_options(&state, 6, 0, &relaxed_configs());

        assert_eq!(known_tile_count(&state, 9), 5);
        assert!(!position_has_impossible_known_tile_count(&state, 2));
        assert!(
            unrelated_options
                .iter()
                .any(|option| option.position == 2 && option.can_hu)
        );
    }

    #[test]
    fn claim_options_require_thirteen_virtual_tiles_for_melds() {
        let mut state = playable_state();
        state.wall = vec![36];
        state.discards.insert(0, vec![3]);
        state.hands.insert(1, vec![1, 2, 3, 3, 3]);

        let options = build_claim_options(&state, 3, 0, &relaxed_configs());

        assert!(!options.iter().any(|option| option.position == 1));
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

    #[test]
    fn claim_window_rejects_impossible_fifth_copy_with_matching_discard() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, vec![3]);
        state.hands.insert(1, vec![3, 3, 3, 3]);
        let claim_window = ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        };

        assert!(has_impossible_known_tile_count(&state, 3));
        assert!(!claim_window_matches_source(&state, &claim_window));
    }

    #[test]
    fn claim_window_rejects_invalid_tile_with_matching_discard() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, vec![99]);
        let claim_window = ClaimWindowState {
            tile: 99,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        };

        assert!(!claim_window_matches_source(&state, &claim_window));
    }

    #[test]
    fn claim_window_rejects_malformed_participants() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, vec![3]);

        for (eligible_positions, responses) in [
            (vec![], HashMap::new()),
            (vec![0], HashMap::new()),
            (vec![1, 1], HashMap::new()),
            (vec![1, 9], HashMap::new()),
            (vec![1], HashMap::from([(2, ClaimResponse::Pass)])),
        ] {
            let claim_window = ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions,
                responses,
            };

            assert!(!claim_window_matches_source(&state, &claim_window));
        }
    }

    #[test]
    fn claim_window_rejects_non_current_source_with_matching_discard() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(1, vec![3]);
        let claim_window = ClaimWindowState {
            tile: 3,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![2],
            responses: HashMap::new(),
        };

        assert!(!claim_window_matches_source(&state, &claim_window));
    }

    #[test]
    fn claim_window_rejects_unknown_source_with_matching_discard() {
        let mut state = playable_state();
        state.discards.insert(9, vec![3]);
        let claim_window = ClaimWindowState {
            tile: 3,
            from_position: 9,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        };

        assert!(!claim_window_matches_source(&state, &claim_window));
    }

    #[test]
    fn dragon_xi_gang_is_exposed_without_opening_or_replacement_draw() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 36, 37]);
        state.melds.insert(1, Vec::new());
        state.last_drawn_tile = Some(37);
        state.wall = vec![34];
        state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
        let mut dispatch = Dispatch::default();

        assert!(perform_xi_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            1,
            &[37, 35, 36],
        ));

        assert_eq!(state.wall, vec![34]);
        assert_eq!(state.last_drawn_tile, Some(37));
        assert_eq!(state.hands.get(&1).unwrap().len(), 11);
        assert_eq!(state.melds.get(&1).unwrap().len(), 1);
        assert_eq!(
            state.melds.get(&1).unwrap()[0].kind,
            ShenyangMahjongMeldKind::XI_GANG
        );
        assert!(!position_has_open_meld(&state, 1));
        assert!(state.xi_gang_options_for_position(1).is_empty());
        assert!(can_self_draw_hu_with_configs(&state, 1, &relaxed_configs()));

        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            1,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        assert!(!settlement.is_gang_draw);
        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn draw_event_hides_tile_from_other_players() {
        let mut state = playable_state();
        state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
        let mut dispatch = Dispatch::default();

        push_draw_events(
            &RoomService::default(),
            "room",
            &state,
            &mut dispatch,
            1,
            35,
        );

        assert_eq!(dispatch.messages.len(), 4);
        for message in &dispatch.messages {
            let OutboundPayload::Event(common_event) = &message.payload else {
                panic!("draw delivery should be an event");
            };
            assert_eq!(common_event.code, WsCode::PLAY as i32);
            let event: WsShenyangMahjongPlayEvent =
                serde_json::from_value(common_event.data.clone()).expect("draw event payload");
            assert_eq!(event.action, ShenyangMahjongAction::DRAW);
            assert_eq!(event.position, 1);
            assert_eq!(event.wall_count, state.wall_count() as i32);
            if message.recipient == 2 {
                assert_eq!(event.tiles, vec![35]);
                assert_eq!(event.target_tile, Some(35));
                assert_eq!(event.xi_gang_options, vec![vec![35, 36, 37]]);
            } else {
                assert!(event.tiles.is_empty());
                assert_eq!(event.target_tile, None);
                assert!(event.xi_gang_options.is_empty());
            }
        }
    }

    fn has_room_event(dispatch: &Dispatch, code: WsCode) -> bool {
        dispatch.messages.iter().any(|item| {
            matches!(&item.payload, OutboundPayload::Event(event) if event.code == code as i32)
        })
    }

    fn open_peng_meld(tile: i32, from_position: usize) -> WsShenyangMahjongMeld {
        build_meld(
            ShenyangMahjongMeldKind::PENG,
            vec![tile, tile, tile],
            Some(from_position),
        )
    }

    #[test]
    fn perform_discard_rejects_during_claim_window() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        state.discards.insert(1, vec![35]);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 1,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::new(),
        });
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(state.claim_window.is_some());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_rejects_invalid_owned_tile() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(99);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            99,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_rejects_malformed_owned_meld() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 34]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3],
                Some(1),
            )],
        );
        state.wall = vec![36];
        state.last_drawn_tile = Some(4);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            4,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_rejects_outside_play_phase() {
        let mut state = playable_state();
        state.phase = ShenyangMahjongPhase::Settlement;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        state.wall = vec![36];
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_rejects_public_fifth_copy() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state.discards.insert(1, vec![3]);
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert_eq!(known_tile_count(&state, 3), 5);
        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.discards.get(&1), Some(&vec![3]));
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_rejects_self_sourced_open_meld() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(0),
            )],
        );
        state.wall = vec![36];
        state.last_drawn_tile = Some(4);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            4,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_rejects_valid_target_with_invalid_hand_tile() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(4);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            4,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_requires_current_position() {
        let mut state = playable_state();
        state.current_position = 1;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_discard_requires_fourteen_virtual_tiles() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, Vec::new());
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 32, 33]);
        state.wall = vec![36];
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_discard(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_self_draw_hu_rejects_during_claim_window() {
        let mut state = playable_state();
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.last_drawn_tile = Some(35);
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
        let mut dispatch = Dispatch::default();

        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &relaxed_configs()
        ));
        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_some());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_self_draw_hu_requires_current_position() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.last_drawn_tile = Some(35);
        let mut dispatch = Dispatch::default();

        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        assert!(state.settlement.is_none());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_self_draw_hu_requires_legal_win() {
        let mut state = playable_state();
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.last_drawn_tile = Some(35);
        let mut dispatch = Dispatch::default();

        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &relaxed_configs()
        ));
        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        assert!(state.settlement.is_none());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_self_draw_hu_respects_win_rule_configs() {
        let mut state = playable_state();
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.last_drawn_tile = Some(35);
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let mut dispatch = Dispatch::default();

        assert!(can_self_draw_hu_with_configs(&state, 0, &relaxed_configs()));
        assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
            0,
        );

        assert!(state.settlement.is_none());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_self_gang_rejects_during_claim_window() {
        let mut state = playable_state();
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert!(state.claim_window.is_some());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn perform_self_gang_requires_current_position() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert!(dispatch.messages.is_empty());
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
                .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state
                .hands
                .insert(2, vec![1, 2, 4, 5, 6, 14, 15, 16, 24, 25, 26, 32, 32]);
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
    fn play_request_blocks_only_first_chi_when_configured() {
        let (mut room_service, mut handler, _room_key, loop_state) =
            setup_request_room_with_configs(serde_json::json!({"allow_first_chi":0,"win_rule":0}));
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
            play_request(ShenyangMahjongAction::CHI, vec![1, 2], Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).unwrap().is_empty());
        drop(state);

        {
            let mut state = loop_state.lock().unwrap();
            state
                .hands
                .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 31, 35]);
            state.melds.insert(
                1,
                vec![build_meld(
                    ShenyangMahjongMeldKind::PENG,
                    vec![21, 21, 21],
                    Some(2),
                )],
            );
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::CHI, vec![1, 2], Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_none());
        assert_eq!(state.melds.get(&1).unwrap().len(), 2);
        assert_eq!(
            state.melds.get(&1).unwrap()[1].kind,
            ShenyangMahjongMeldKind::CHI
        );
    }

    #[test]
    fn play_request_chi_allows_shenyang_basic_rule() {
        let (mut room_service, mut handler, _room_key, loop_state) =
            setup_request_room_with_configs(serde_json::json!({"win_rule":1}));
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
            play_request(ShenyangMahjongAction::CHI, vec![1, 2], Some(3), Some(0)),
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
        assert!(state.discards.get(&0).unwrap().is_empty());
        let meld = state.melds.get(&1).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::CHI);
        assert_eq!(meld.tiles, vec![1, 2, 3]);
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
    fn play_request_declares_only_frozen_first_draw_xi_gang_option() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 1;
            state
                .hands
                .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 35, 36, 37]);
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.last_drawn_tile = Some(37);
            state.wall = vec![34];
            state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);
        }

        let response = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::XI_GANG, vec![37, 35, 36], None, None),
        );
        assert_eq!(
            response_code(&response, 2, Routes::PLAY),
            Some(WsResponseCode::OK as i32)
        );
        {
            let state = loop_state.lock().unwrap();
            assert_eq!(state.melds.get(&1).unwrap().len(), 1);
            assert!(state.xi_gang_options_for_position(1).is_empty());
        }

        let duplicate = handler.handle_game_request(
            &mut room_service,
            2,
            play_request(ShenyangMahjongAction::XI_GANG, vec![35, 36, 37], None, None),
        );
        assert_eq!(
            response_code(&duplicate, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
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
            }
            state
                .hands
                .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
            state
                .hands
                .insert(1, vec![1, 2, 7, 9, 14, 16, 18, 24, 26, 28, 34, 35, 36]);
            state
                .hands
                .insert(2, vec![3, 3, 4, 6, 8, 11, 13, 15, 17, 21, 23, 25, 27]);
            state
                .hands
                .insert(3, vec![5, 7, 9, 12, 14, 16, 18, 22, 24, 26, 28, 32, 37]);
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
        for recipient in 1..=4 {
            let common_event = response
                .messages
                .iter()
                .find_map(|message| match &message.payload {
                    OutboundPayload::Event(event)
                        if message.recipient == recipient
                            && event.code == WsCode::CLAIM_WINDOW as i32 =>
                    {
                        Some(event)
                    }
                    _ => None,
                })
                .expect("each player should receive the public claim window");
            let event: WsShenyangMahjongClaimWindowEvent =
                serde_json::from_value(common_event.data.clone()).expect("claim window payload");
            let viewer_position = recipient as i32 - 1;
            assert_eq!(event.tile, 3);
            assert_eq!(event.from_position, 0);
            if matches!(viewer_position, 1 | 2) {
                assert_eq!(event.eligible_positions, vec![viewer_position]);
                assert_eq!(event.options.len(), 1);
                assert_eq!(event.options[0].position, viewer_position);
            } else {
                assert!(event.eligible_positions.is_empty());
                assert!(event.options.is_empty());
            }
        }
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
    fn play_request_discard_rejects_invalid_owned_tile() {
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
                .insert(0, vec![4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33, 99]);
            state.wall = vec![36];
            state.last_drawn_tile = Some(99);
        }

        let response = handler.handle_game_request(
            &mut room_service,
            1,
            play_request(ShenyangMahjongAction::DISCARD, Vec::new(), Some(99), None),
        );

        assert_eq!(
            response_code(&response, 1, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(state.hands.get(&0).unwrap().contains(&99));
        assert_eq!(state.wall, vec![36]);
    }

    #[test]
    fn play_request_discard_rejects_public_fifth_copy() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(1, vec![3]);
            state
                .hands
                .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
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
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert_eq!(known_tile_count(&state, 3), 5);
        assert_eq!(state.discards.get(&1), Some(&vec![3]));
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(
            state
                .hands
                .get(&0)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            4
        );
        assert_eq!(state.wall, vec![36]);
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
            }
            state
                .hands
                .insert(0, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 31, 32, 33]);
            state
                .hands
                .insert(1, vec![2, 4, 7, 9, 14, 16, 18, 24, 26, 28, 34, 35, 37]);
            state
                .hands
                .insert(2, vec![3, 5, 8, 11, 13, 15, 17, 21, 23, 25, 27, 32, 36]);
            state
                .hands
                .insert(3, vec![6, 7, 9, 12, 14, 16, 18, 22, 24, 26, 28, 33, 34]);
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
    fn play_request_gang_rejects_when_replacement_tile_is_unavailable() {
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
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        assert!(!has_room_event(&response, WsCode::GAME_OVER));
        let state = loop_state.lock().unwrap();
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert!(state.claim_window.is_some());
        assert_eq!(state.current_position, 0);
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert_eq!(
            state
                .hands
                .get(&2)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            3,
        );
        assert!(state.melds.get(&2).unwrap().is_empty());
        assert!(state.settlement.is_none());
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
    fn play_request_nearest_hu_mode_keeps_only_first_winner_in_turn_order() {
        let (mut room_service, mut handler, _room_key, loop_state) =
            setup_request_room_with_configs(serde_json::json!({"multi_hu_mode":0,"win_rule":0}));
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
                .insert(2, vec![1, 2, 4, 5, 6, 14, 15, 16, 24, 25, 26, 32, 32]);
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
    fn play_request_peng_requires_thirteen_virtual_tiles() {
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
            state.hands.insert(2, vec![3, 3]);
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
            play_request(ShenyangMahjongAction::PENG, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(claim_window.responses.is_empty());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert_eq!(state.hands.get(&2), Some(&vec![3, 3]));
        assert!(state.melds.get(&2).unwrap().is_empty());
    }

    #[test]
    fn play_request_rejects_claim_from_source_position() {
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
                .insert(0, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state.wall = vec![36];
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![0],
                responses: HashMap::new(),
            });
        }

        let response = handler.handle_game_request(
            &mut room_service,
            1,
            play_request(ShenyangMahjongAction::PENG, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 1, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(claim_window.responses.is_empty());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&0).unwrap().is_empty());
    }

    #[test]
    fn play_request_rejects_claim_when_source_discard_does_not_match() {
        let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
        {
            let mut state = loop_state.lock().unwrap();
            state.phase = ShenyangMahjongPhase::Play;
            state.current_position = 0;
            for position in 0..4 {
                state.discards.insert(position, Vec::new());
                state.melds.insert(position, Vec::new());
            }
            state.discards.insert(0, vec![4]);
            state
                .hands
                .insert(2, vec![3, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35]);
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
            play_request(ShenyangMahjongAction::PENG, Vec::new(), Some(3), Some(0)),
        );

        assert_eq!(
            response_code(&response, 3, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(claim_window.responses.is_empty());
        assert_eq!(state.discards.get(&0), Some(&vec![4]));
        assert!(state.melds.get(&2).unwrap().is_empty());
    }

    #[test]
    fn play_request_rejects_impossible_fifth_tile_peng_claim() {
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
                .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
            state
                .hands
                .insert(2, vec![3, 3, 7, 8, 9, 14, 15, 16, 24, 25, 26, 32, 36]);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
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
        let state = loop_state.lock().unwrap();
        assert!(state.claim_window.is_some());
        assert!(state.melds.get(&1).unwrap().is_empty());
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
    fn play_request_rejects_melds_from_impossible_known_tile_state() {
        for (action, tiles, hand) in [
            (
                ShenyangMahjongAction::PENG,
                Vec::new(),
                vec![3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
            ),
            (
                ShenyangMahjongAction::GANG,
                Vec::new(),
                vec![3, 3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13],
            ),
            (
                ShenyangMahjongAction::CHI,
                vec![1, 2],
                vec![1, 2, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
            ),
        ] {
            let (mut room_service, mut handler, _room_key, loop_state) = setup_request_room();
            {
                let mut state = loop_state.lock().unwrap();
                state.phase = ShenyangMahjongPhase::Play;
                state.current_position = 0;
                for position in 0..4 {
                    state.discards.insert(position, Vec::new());
                    state.melds.insert(position, Vec::new());
                }
                state.hands.insert(1, hand.clone());
                state.discards.insert(0, vec![3]);
                state.discards.insert(2, vec![9]);
                state.wall = vec![37];
                state.claim_window = Some(ClaimWindowState {
                    tile: 3,
                    from_position: 0,
                    kind: ClaimWindowKind::Discard,
                    eligible_positions: vec![1],
                    responses: HashMap::new(),
                });
                assert_eq!(known_tile_count(&state, 9), 5);
                assert!(position_has_impossible_known_tile_count(&state, 1));
            }

            let response = handler.handle_game_request(
                &mut room_service,
                2,
                play_request(action, tiles, Some(3), Some(0)),
            );

            assert_eq!(
                response_code(&response, 2, Routes::PLAY),
                Some(WsResponseCode::NO_PERMISSION as i32)
            );
            let state = loop_state.lock().unwrap();
            assert!(
                state
                    .claim_window
                    .as_ref()
                    .is_some_and(|window| { window.responses.is_empty() })
            );
            assert_eq!(state.hands.get(&1), Some(&hand));
            assert!(state.melds.get(&1).unwrap().is_empty());
            assert_eq!(state.discards.get(&0), Some(&vec![3]));
        }
    }

    #[test]
    fn play_request_rejects_public_fifth_copy_used_by_hu_winner() {
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
                .insert(1, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
            state.discards.insert(0, vec![6]);
            state.discards.insert(2, vec![1]);
            state.claim_window = Some(ClaimWindowState {
                tile: 6,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let rejected_hu = handler.handle_game_request(
            &mut room_service,
            2,
            ClientRequest {
                route: Routes::PLAY as i32,
                data: serde_json::json!({
                    "action": ShenyangMahjongAction::HU as i32,
                    "tiles": [],
                    "target_tile": 6,
                    "from_position": 0,
                }),
            },
        );

        assert_eq!(
            response_code(&rejected_hu, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_some());
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
        let (mut room_service, mut handler, _room_key, loop_state) =
            setup_request_room_with_configs(serde_json::json!({"win_rule":1}));
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
    fn play_request_rob_gang_hu_requires_added_gang_source() {
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
                .insert(0, vec![3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 22, 23, 31]);
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
        assert!(state.settlement.is_none());
        let claim_window = state.claim_window.as_ref().expect("claim window");
        assert!(claim_window.responses.is_empty());
        assert!(state.hands.get(&0).unwrap().contains(&3));
        assert!(state.melds.get(&0).unwrap().is_empty());
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
                .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
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
    fn play_request_rob_gang_rejects_impossible_fifth_tile_hu() {
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
                .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
            state.discards.insert(2, vec![3]);
            state.last_drawn_tile = Some(3);
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::RobGang,
                eligible_positions: vec![1],
                responses: HashMap::new(),
            });
        }

        let rejected_hu = handler.handle_game_request(
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
            response_code(&rejected_hu, 2, Routes::PLAY),
            Some(WsResponseCode::NO_PERMISSION as i32)
        );
        let state = loop_state.lock().unwrap();
        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_some());
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
                .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
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
    fn pregame_quit_does_not_poison_the_next_start() {
        let mut room_service = RoomService::default();
        let mut handler = ShenyangMahjongGameHandler::default();
        for session_id in 1..=4 {
            let _ = room_service.handle_common_request(
                session_id,
                &ClientRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": format!("P{session_id}"),
                        "password": "pregame-quit",
                        "game_id": GameId::SHENYANG_MAHJONG as i32
                    }),
                },
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            );
        }
        let quit_request = ClientRequest {
            route: Routes::QUIT as i32,
            data: Value::Null,
        };
        let mut quit_dispatch = room_service
            .handle_common_request(
                2,
                &quit_request,
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            )
            .expect("common quit route");
        handler.after_common_request(&mut room_service, 2, &quit_request, &mut quit_dispatch);
        assert!(
            room_service
                .room_common_state("pregame-quit")
                .expect("stopped pregame state")
                .lock()
                .unwrap()
                .stop_requested()
        );
        let _ = room_service.handle_common_request(
            5,
            &ClientRequest {
                route: Routes::JOIN as i32,
                data: serde_json::json!({
                    "name": "P5",
                    "password": "pregame-quit",
                    "game_id": GameId::SHENYANG_MAHJONG as i32
                }),
            },
            GameId::SHENYANG_MAHJONG,
            build_shenyang_mahjong_settings,
        );

        let started = handler.handle_start(&mut room_service, 1);

        assert_eq!(
            response_code(&started, 1, Routes::START),
            Some(WsResponseCode::OK as i32)
        );
        let state = handler
            .loop_state("pregame-quit")
            .expect("started loop state");
        assert!(!state.lock().unwrap().stop_requested());
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
            .room_common_state(&room_key)
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
    fn redeal_uses_only_positive_score_winners_for_dealer_rotation() {
        let mut state = playable_state();
        state.dealer_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(2),
            )],
        );
        state.hands.insert(1, vec![1, 1, 35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
            ],
        );
        state.enter_settlement_with_reverse_win(
            vec![0, 1],
            Some(2),
            Some(1),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 0), 0);
        assert!(winner_hand_fan(&state, settlement, 1) > 0);
        assert_eq!(
            positive_winner_positions_for_state(&state, settlement, &HashMap::new()),
            vec![1]
        );

        redeal_after_settlement_with_configs(&mut state, &HashMap::new());

        assert_eq!(state.dealer_position, 1);
        assert_eq!(state.current_position, 1);
        assert!(state.settlement.is_none());
    }

    fn relaxed_configs() -> HashMap<String, i32> {
        HashMap::from([("win_rule".to_owned(), WIN_RULE_RELAXED)])
    }

    #[test]
    fn resolve_claim_window_allows_chi_for_shenyang_basic_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(
                1,
                ClaimResponse::Chi {
                    consume_tiles: vec![1, 2],
                },
            )]),
        });
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert!(!state.hands.get(&1).unwrap().contains(&1));
        assert!(!state.hands.get(&1).unwrap().contains(&2));
        assert!(!state.hands.get(&1).unwrap().contains(&36));
        let meld = state.melds.get(&1).unwrap().first().unwrap();
        assert_eq!(meld.kind, ShenyangMahjongMeldKind::CHI);
        assert_eq!(meld.tiles, vec![1, 2, 3]);
    }

    #[test]
    fn resolve_claim_window_blocks_only_first_chi_when_configured() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(
                1,
                ClaimResponse::Chi {
                    consume_tiles: vec![1, 2],
                },
            )]),
        });
        let configs = HashMap::from([("allow_first_chi".to_owned(), 0)]);
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
        assert!(state.hands.get(&1).unwrap().contains(&36));

        state
            .hands
            .insert(1, vec![1, 2, 4, 5, 6, 11, 12, 13, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![21, 21, 21],
                Some(2),
            )],
        );
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(
                1,
                ClaimResponse::Chi {
                    consume_tiles: vec![1, 2],
                },
            )]),
        });

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &configs,
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert!(state.discards.get(&0).unwrap().is_empty());
        assert_eq!(state.melds.get(&1).unwrap().len(), 2);
        assert_eq!(
            state.melds.get(&1).unwrap()[1].kind,
            ShenyangMahjongMeldKind::CHI
        );
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

    #[test]
    fn resolve_claim_window_ignores_gang_without_replacement_tile() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.discards.insert(0, vec![3]);
        state.wall.clear();
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Gang)]),
        });
        let original_hand = state.hands.get(&1).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        );

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert!(state.claim_window.is_none());
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert_eq!(state.hands.get(&1), Some(&original_hand));
        assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
        assert!(
            state
                .settlement
                .as_ref()
                .is_some_and(|settlement| settlement.winner_positions.is_empty())
        );
    }

    #[test]
    fn resolve_claim_window_ignores_illegal_gang_response() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
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
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
        assert_eq!(
            state
                .hands
                .get(&1)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            2,
        );
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_ignores_illegal_hu_response() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.discards.insert(0, vec![35]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 35,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Hu)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![35]));
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_ignores_illegal_peng_response() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Peng)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
        assert_eq!(
            state
                .hands
                .get(&1)
                .unwrap()
                .iter()
                .filter(|&&tile| tile == 3)
                .count(),
            1,
        );
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_ignores_impossible_fifth_tile_peng_response() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state
            .hands
            .insert(2, vec![3, 3, 7, 8, 9, 14, 15, 16, 24, 25, 26, 32, 36]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![37];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Peng)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
        assert!(state.hands.get(&1).unwrap().contains(&37));
    }

    #[test]
    fn resolve_claim_window_ignores_invalid_chi_sequence() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(
                1,
                ClaimResponse::Chi {
                    consume_tiles: vec![1, 4],
                },
            )]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).map(Vec::is_empty).unwrap_or(true));
        assert!(state.hands.get(&1).unwrap().contains(&1));
        assert!(state.hands.get(&1).unwrap().contains(&4));
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_ignores_meld_responses_without_thirteen_virtual_tiles() {
        for (hand, response) in [
            (vec![3, 3], ClaimResponse::Peng),
            (vec![3, 3, 3], ClaimResponse::Gang),
            (
                vec![1, 2],
                ClaimResponse::Chi {
                    consume_tiles: vec![1, 2],
                },
            ),
        ] {
            let mut state = playable_state();
            state.hands.insert(1, hand);
            state.discards.insert(0, vec![3]);
            state.wall = vec![36];
            state.current_position = 0;
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::from([(1, response)]),
            });
            let mut dispatch = Dispatch::default();

            resolve_claim_window(
                &RoomService::default(),
                "room",
                &mut state,
                &relaxed_configs(),
                &mut dispatch,
            );

            assert!(state.claim_window.is_none());
            assert_eq!(state.discards.get(&0), Some(&vec![3]));
            assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
        }
    }

    #[test]
    fn resolve_claim_window_ignores_melds_from_impossible_known_tile_state() {
        for (hand, response) in [
            (
                vec![3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
                ClaimResponse::Peng,
            ),
            (
                vec![3, 3, 3, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13],
                ClaimResponse::Gang,
            ),
            (
                vec![1, 2, 4, 5, 6, 9, 9, 9, 9, 11, 12, 13, 21],
                ClaimResponse::Chi {
                    consume_tiles: vec![1, 2],
                },
            ),
        ] {
            let mut state = playable_state();
            state.hands.insert(1, hand.clone());
            state.discards.insert(0, vec![3]);
            state.discards.insert(2, vec![9]);
            state.wall = vec![37];
            state.current_position = 0;
            state.claim_window = Some(ClaimWindowState {
                tile: 3,
                from_position: 0,
                kind: ClaimWindowKind::Discard,
                eligible_positions: vec![1],
                responses: HashMap::from([(1, response)]),
            });
            let mut dispatch = Dispatch::default();

            assert_eq!(known_tile_count(&state, 9), 5);
            assert!(position_has_impossible_known_tile_count(&state, 1));
            resolve_claim_window(
                &RoomService::default(),
                "room",
                &mut state,
                &relaxed_configs(),
                &mut dispatch,
            );

            assert!(state.claim_window.is_none());
            assert_eq!(state.discards.get(&0), Some(&vec![3]));
            assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
            assert!(
                hand.iter()
                    .all(|tile| state.hands.get(&1).unwrap().contains(tile))
            );
        }
    }

    #[test]
    fn resolve_claim_window_ignores_mismatched_source_discard() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(0, vec![4]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Peng)]),
        });
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![4]));
        assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_ignores_public_fifth_copy_used_by_hu_winner() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6]);
        state.discards.insert(0, vec![6]);
        state.discards.insert(2, vec![1]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 6,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Hu)]),
        });
        let mut dispatch = Dispatch::default();

        assert_eq!(known_tile_count(&state, 1), 5);
        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![6]));
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_ignores_response_from_source_position() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.current_position = 0;
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![0],
            responses: HashMap::from([(0, ClaimResponse::Peng)]),
        });
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
    }

    #[test]
    fn resolve_claim_window_recovers_from_unknown_source() {
        let mut state = playable_state();
        state.current_position = 0;
        state
            .hands
            .insert(1, vec![3, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(9, vec![3]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 9,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Peng)]),
        });
        let original_hand = state.hands.get(&1).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 1);
        assert_eq!(state.discards.get(&9), Some(&vec![3]));
        assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
        assert_eq!(state.hands.get(&1).unwrap().len(), original_hand.len() + 1);
        assert!(state.hands.get(&1).unwrap().contains(&36));
    }

    #[test]
    fn resolve_claim_window_rejects_outside_play_phase() {
        let mut state = playable_state();
        state.phase = ShenyangMahjongPhase::Settlement;
        state
            .hands
            .insert(1, vec![3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.discards.insert(0, vec![3]);
        state.wall = vec![36];
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Peng)]),
        });
        let original_hand = state.hands.get(&1).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
        );

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(state.hands.get(&1), Some(&original_hand));
        assert_eq!(state.discards.get(&0), Some(&vec![3]));
        assert!(state.melds.get(&1).is_none_or(Vec::is_empty));
        assert!(state.claim_window.as_ref().is_some_and(|window| {
            matches!(window.responses.get(&1), Some(ClaimResponse::Peng))
        }));
        assert_eq!(state.wall, vec![36]);
        assert!(dispatch.messages.is_empty());
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
    fn rob_gang_hu_ignores_invalid_added_gang_source() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 22, 23, 31]);
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
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert_eq!(state.current_position, 0);
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert_eq!(state.wall, vec![36]);
    }

    #[test]
    fn rob_gang_hu_rejects_impossible_fifth_tile_response() {
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
        state.discards.insert(2, vec![3]);
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
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, Some(3));
        assert!(state.hands.get(&0).unwrap().contains(&3));
        assert_eq!(state.wall, vec![36]);
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().kind,
            ShenyangMahjongMeldKind::PENG
        );
    }

    #[test]
    fn rob_gang_hu_respects_basic_open_requirement() {
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

        assert!(state.settlement.is_none());
        assert!(state.claim_window.is_none());
        assert_eq!(state.current_position, 0);
        assert_eq!(state.last_drawn_tile, Some(36));
        assert_eq!(
            state.melds.get(&0).unwrap().first().unwrap().kind,
            ShenyangMahjongMeldKind::GANG
        );
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
            &relaxed_configs(),
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
    fn rob_gang_options_do_not_count_concealed_gang_as_open_for_basic_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![31, 31, 31, 31],
                None,
            )],
        );
        let relaxed = build_rob_gang_claim_window_event(&state, 3, 0, 5, &relaxed_configs());
        let basic_configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let basic = build_rob_gang_claim_window_event(&state, 3, 0, 5, &basic_configs);

        assert!(relaxed.eligible_positions.contains(&1));
        assert!(!basic.eligible_positions.contains(&1));
    }

    #[test]
    fn rob_gang_options_reject_impossible_fifth_tile() {
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
        state.discards.insert(2, vec![3]);

        let claim_window = build_rob_gang_claim_window_event(&state, 3, 0, 5, &relaxed_configs());

        assert!(!claim_window.eligible_positions.contains(&1));
        assert!(claim_window.options.is_empty());
    }

    #[test]
    fn rob_gang_options_reject_public_fifth_copy_used_by_winner() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![6, 7, 8, 11, 12, 13, 21, 22, 23, 31, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![6, 6, 6],
                Some(2),
            )],
        );
        state
            .hands
            .insert(1, vec![1, 1, 1, 1, 2, 3, 7, 8, 11, 12, 13, 35, 35]);
        state.discards.insert(2, vec![1]);

        let claim_window = build_rob_gang_claim_window_event(&state, 6, 0, 5, &relaxed_configs());

        assert_eq!(known_tile_count(&state, 1), 5);
        assert!(!claim_window.eligible_positions.contains(&1));
        assert!(claim_window.options.is_empty());
    }

    #[test]
    fn rob_gang_pass_clears_invalid_added_gang_source() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![36];
        state.last_drawn_tile = Some(3);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::RobGang,
            eligible_positions: vec![1],
            responses: HashMap::from([(1, ClaimResponse::Pass)]),
        });
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        resolve_claim_window(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
        );

        assert!(state.claim_window.is_none());
        assert_eq!(state.phase, ShenyangMahjongPhase::Play);
        assert_eq!(state.current_position, 0);
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert_eq!(state.wall, vec![36]);
    }

    #[test]
    fn self_draw_closed_sequence_dragon_pair_win_stays_available_after_xi_gang() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::XI_GANG,
                vec![31, 32, 33, 34],
                None,
            )],
        );
        state.last_drawn_tile = Some(35);
        let default_configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let disabled_configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);

        assert!(!can_self_draw_hu_with_configs(&state, 1, &default_configs));
        assert!(can_self_draw_hu_with_configs(&state, 1, &disabled_configs));
    }

    #[test]
    fn self_draw_hu_allows_closed_sequence_dragon_pair_win_when_first_chi_disabled() {
        let mut state = playable_state();
        state.current_position = 0;
        state
            .hands
            .insert(0, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.last_drawn_tile = Some(35);
        let default_configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let disabled_configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);

        assert!(!can_self_draw_hu_with_configs(&state, 0, &default_configs));
        assert!(can_self_draw_hu_with_configs(&state, 0, &disabled_configs));
    }

    #[test]
    fn self_draw_hu_does_not_count_concealed_gang_as_open_for_basic_rule() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![1, 1, 1, 1],
                None,
            )],
        );
        state.last_drawn_tile = Some(35);
        let configs = HashMap::from([("win_rule".to_owned(), 1)]);

        assert!(can_self_draw_hu_with_configs(&state, 0, &relaxed_configs()));
        assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
    }

    #[test]
    fn self_draw_hu_rejects_chi_from_non_previous_position() {
        let mut state = playable_state();
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
        state.last_drawn_tile = Some(35);
        let configs = HashMap::from([("win_rule".to_owned(), WIN_RULE_SHENYANG_BASIC)]);

        assert!(is_complete_win_with_configs(
            state.hands.get(&0).unwrap(),
            state.melds.get(&0).unwrap(),
            &configs
        ));
        assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));
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
    fn self_draw_hu_rejects_outside_play_phase() {
        let mut state = playable_state();
        state.phase = ShenyangMahjongPhase::Start;
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.last_drawn_tile = Some(35);
        let mut dispatch = Dispatch::default();

        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &relaxed_configs()
        ));
        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        assert_eq!(state.phase, ShenyangMahjongPhase::Start);
        assert!(state.settlement.is_none());
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn self_draw_hu_rejects_public_fifth_copy() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]);
        state.discards.insert(1, vec![1]);
        state.last_drawn_tile = Some(6);
        let mut dispatch = Dispatch::default();

        assert!(is_seven_pairs_win(state.hands.get(&0).unwrap()));
        assert_eq!(known_tile_count(&state, 1), 5);
        assert!(position_has_impossible_known_tile_count(&state, 0));
        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &relaxed_configs()
        ));

        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        assert!(state.settlement.is_none());
        assert!(dispatch.messages.is_empty());

        state.discards.insert(1, vec![9, 9, 9, 9, 9]);
        assert_eq!(known_tile_count(&state, 9), 5);
        assert!(!position_has_impossible_known_tile_count(&state, 0));
        assert!(can_self_draw_hu_with_configs(&state, 0, &relaxed_configs()));
    }

    #[test]
    fn self_draw_hu_rejects_self_sourced_open_meld() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![1, 2, 3],
                Some(0),
            )],
        );
        state.last_drawn_tile = Some(35);
        let configs = HashMap::from([("win_rule".to_owned(), WIN_RULE_SHENYANG_BASIC)]);

        assert!(is_complete_win_with_configs(
            state.hands.get(&0).unwrap(),
            state.melds.get(&0).unwrap(),
            &configs
        ));
        assert!(!can_self_draw_hu_with_configs(&state, 0, &configs));

        state.enter_settlement(vec![0], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 0, WIN_RULE_SHENYANG_BASIC),
            0
        );
    }

    #[test]
    fn self_draw_hu_requires_a_drawn_turn() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);

        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &relaxed_configs()
        ));

        state.last_drawn_tile = Some(9);

        assert!(!can_self_draw_hu_with_configs(
            &state,
            0,
            &relaxed_configs()
        ));

        state.last_drawn_tile = Some(35);

        assert!(can_self_draw_hu_with_configs(&state, 0, &relaxed_configs()));
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
            .insert(0, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9, 9]);
        state.melds.insert(0, Vec::new());
        state.last_drawn_tile = Some(9);

        assert!(can_self_draw_hu_with_configs(&state, 0, &configs));

        state.hands.insert(0, vec![3, 4, 5, 4, 5, 6, 5, 6, 7, 8, 8]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![2, 3, 4],
                Some(3),
            )],
        );
        state.last_drawn_tile = Some(8);

        assert!(can_self_draw_hu_with_configs(&state, 0, &configs));

        state
            .hands
            .insert(0, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![1, 2, 3],
                Some(3),
            )],
        );
        state.last_drawn_tile = Some(35);

        assert!(can_self_draw_hu_with_configs(&state, 0, &configs));
        let first_chi_disabled_configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);
        assert!(can_self_draw_hu_with_configs(
            &state,
            0,
            &first_chi_disabled_configs
        ));
    }

    #[test]
    fn self_draw_last_wall_tile_counts_haidilao_without_gang_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.wall = vec![35];

        assert_eq!(state.draw_for_position(0), Some(35));
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.last_drawn_tile, Some(35));
        assert!(!state.pending_gang_draw);
        assert!(can_self_draw_hu_with_configs(&state, 0, &relaxed_configs()));

        let mut dispatch = Dispatch::default();
        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        assert!(settlement.is_self_draw);
        assert!(!settlement.is_gang_draw);
        assert!(settlement.is_haidilao);
        assert_eq!(settlement.win_tile, Some(35));
        assert_eq!(winner_hand_fan(&state, settlement, 0), 3);

        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(!event.is_gang_draw);
        assert!(event.is_haidilao);
        assert_eq!(event.winner_details.len(), 1);
        assert!(!event.winner_details[0].is_gang_draw);
        assert!(event.winner_details[0].is_haidilao);
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
            &relaxed_configs(),
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
    fn self_gang_last_replacement_self_draw_counts_gang_draw_and_haidilao() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![31];
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
        assert_eq!(state.wall_count(), 0);
        assert_eq!(state.last_drawn_tile, Some(31));
        assert!(state.pending_gang_draw);
        assert!(can_self_draw_hu_with_configs(&state, 0, &relaxed_configs()));

        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        assert!(settlement.is_self_draw);
        assert!(settlement.is_gang_draw);
        assert!(settlement.is_haidilao);
        assert_eq!(settlement.win_tile, Some(31));
        assert_eq!(winner_hand_fan(&state, settlement, 0), 6);

        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(event.is_gang_draw);
        assert!(event.is_haidilao);
        assert_eq!(event.winner_details.len(), 1);
        assert!(event.winner_details[0].is_gang_draw);
        assert!(event.winner_details[0].is_haidilao);
    }

    #[test]
    fn self_gang_rejects_malformed_owned_meld() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![9, 9],
                Some(1),
            )],
        );
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        let melds = state.melds.get(&0).unwrap();
        assert_eq!(melds.len(), 1);
        assert_eq!(melds[0].kind, ShenyangMahjongMeldKind::PENG);
        assert_eq!(melds[0].tiles, vec![9, 9]);
        assert_eq!(melds[0].from_position, Some(1));
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn self_gang_rejects_outside_play_phase() {
        let mut state = playable_state();
        state.phase = ShenyangMahjongPhase::Settlement;
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall = vec![35];
        state.last_drawn_tile = Some(31);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn self_gang_rejects_public_fifth_copy() {
        let mut concealed_gang_state = playable_state();
        concealed_gang_state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        concealed_gang_state.discards.insert(1, vec![3]);
        concealed_gang_state.wall = vec![35];
        concealed_gang_state.last_drawn_tile = Some(3);
        let concealed_hand = concealed_gang_state.hands.get(&0).cloned().unwrap();
        let mut concealed_dispatch = Dispatch::default();

        assert_eq!(known_tile_count(&concealed_gang_state, 3), 5);
        assert!(!can_self_gang(&concealed_gang_state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut concealed_gang_state,
            &HashMap::new(),
            &mut concealed_dispatch,
            0,
            3,
        ));
        assert_eq!(concealed_gang_state.hands.get(&0), Some(&concealed_hand));
        assert!(concealed_gang_state.melds.get(&0).is_none_or(Vec::is_empty));
        assert_eq!(concealed_gang_state.wall, vec![35]);
        assert!(concealed_dispatch.messages.is_empty());

        let mut added_gang_state = playable_state();
        added_gang_state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        added_gang_state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        added_gang_state.discards.insert(1, vec![3]);
        added_gang_state.wall = vec![35];
        added_gang_state.last_drawn_tile = Some(3);
        let added_hand = added_gang_state.hands.get(&0).cloned().unwrap();
        let added_melds = added_gang_state.melds.get(&0).cloned().unwrap();
        let mut added_dispatch = Dispatch::default();

        assert_eq!(known_tile_count(&added_gang_state, 3), 5);
        assert!(!can_self_gang(&added_gang_state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut added_gang_state,
            &HashMap::new(),
            &mut added_dispatch,
            0,
            3,
        ));
        assert_eq!(added_gang_state.hands.get(&0), Some(&added_hand));
        let actual_melds = added_gang_state.melds.get(&0).expect("melds should stay");
        assert_eq!(actual_melds.len(), added_melds.len());
        assert_eq!(actual_melds[0].kind, added_melds[0].kind);
        assert_eq!(actual_melds[0].tiles, added_melds[0].tiles);
        assert_eq!(actual_melds[0].from_position, added_melds[0].from_position);
        assert_eq!(added_gang_state.wall, vec![35]);
        assert!(added_dispatch.messages.is_empty());
    }

    #[test]
    fn self_gang_rejects_unrelated_invalid_hand_tile() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 99]);
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
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let original_melds = state.melds.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        let actual_melds = state.melds.get(&0).expect("melds should stay");
        assert_eq!(actual_melds.len(), original_melds.len());
        assert_eq!(actual_melds[0].kind, original_melds[0].kind);
        assert_eq!(actual_melds[0].tiles, original_melds[0].tiles);
        assert_eq!(
            actual_melds[0].from_position,
            original_melds[0].from_position
        );
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn self_gang_rejects_unrelated_public_fifth_copy() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 9, 11, 12, 13, 21, 22, 23]);
        state.discards.insert(1, vec![9, 9, 9, 9]);
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert_eq!(known_tile_count(&state, 3), 4);
        assert_eq!(known_tile_count(&state, 9), 5);
        assert!(position_has_impossible_known_tile_count(&state, 0));
        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn self_gang_rejects_when_replacement_tile_is_unavailable() {
        let mut state = playable_state();
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.wall.clear();
        state.last_drawn_tile = Some(31);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));

        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert!(state.settlement.is_none());
        assert!(dispatch.messages.is_empty());
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
    fn self_gang_requires_fourteen_virtual_tiles() {
        let mut state = playable_state();
        state.hands.insert(0, vec![3, 3, 3, 3]);
        state.wall = vec![35];
        state.last_drawn_tile = Some(3);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());
    }

    #[test]
    fn self_gang_requires_owned_last_drawn_tile() {
        let mut state = playable_state();
        state.wall = vec![35];
        state
            .hands
            .insert(0, vec![3, 3, 3, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 31]);
        state.last_drawn_tile = Some(9);
        let original_hand = state.hands.get(&0).cloned().unwrap();
        let mut dispatch = Dispatch::default();

        assert!(!can_self_gang(&state, 0, 3));
        assert!(!perform_self_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            0,
            3,
        ));
        assert_eq!(state.hands.get(&0), Some(&original_hand));
        assert!(state.melds.get(&0).is_none_or(Vec::is_empty));
        assert_eq!(state.wall, vec![35]);
        assert!(dispatch.messages.is_empty());

        state.last_drawn_tile = Some(31);

        assert!(can_self_gang(&state, 0, 3));
    }

    #[test]
    fn settlement_deduplicates_restored_winner_positions() {
        let mut state = playable_state();
        state.dealer_position = 2;
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![2, 2], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, -8), (2, 24), (3, -8)]
        );

        let event = build_settlement_event(&state).expect("settlement event");
        assert_eq!(event.winner_positions, vec![2]);
        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(event.winner_details[0].position, 2);
    }

    #[test]
    fn settlement_event_normalizes_invalid_gang_haidilao_as_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 8, 11, 12, 13, 31, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![35, 35, 35, 35],
                None,
            )],
        );
        state.wall.clear();
        state.enter_settlement_with_reverse_win(vec![1], None, Some(31), true, false, true, true);

        let event = build_settlement_event_with_configs(&state, &relaxed_configs())
            .expect("settlement event");

        assert!(event.winner_positions.is_empty());
        assert!(event.winner_details.is_empty());
        assert_eq!(event.from_position, None);
        assert_eq!(event.win_tile, None);
        assert!(!event.is_self_draw);
        assert!(!event.is_reverse_win);
        assert!(!event.is_gang_draw);
        assert!(!event.is_haidilao);
    }

    #[test]
    fn settlement_event_normalizes_invalid_reverse_win_as_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 8, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![4, 4, 4],
                Some(2),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            true,
            false,
            false,
        );

        let event = build_settlement_event_with_configs(&state, &relaxed_configs())
            .expect("settlement event");

        assert!(event.winner_positions.is_empty());
        assert!(event.winner_details.is_empty());
        assert_eq!(event.from_position, None);
        assert_eq!(event.win_tile, None);
        assert!(!event.is_self_draw);
        assert!(!event.is_reverse_win);
        assert!(!event.is_gang_draw);
        assert!(!event.is_haidilao);
    }

    #[test]
    fn settlement_event_skips_zero_score_winners() {
        let mut state = playable_state();
        state.hands.insert(1, vec![1, 1, 35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
            ],
        );
        state
            .hands
            .insert(2, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(0),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1, 2],
            Some(0),
            Some(1),
            false,
            false,
            false,
            false,
        );

        let event = build_settlement_event(&state).expect("settlement event");

        assert_eq!(event.winner_positions, vec![1]);
        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(event.winner_details[0].position, 1);
        assert!(event.winner_details[0].score > 0);
        assert_eq!(
            event
                .score_changes
                .iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -5), (1, 5), (2, 0), (3, 0)]
        );

        let valid_winner_snapshot = event
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("valid winner snapshot");
        assert_eq!(
            valid_winner_snapshot
                .hand_tiles
                .iter()
                .filter(|tile| **tile == 1)
                .count(),
            3
        );

        let invalid_winner_snapshot = event
            .players
            .iter()
            .find(|player| player.position == 2)
            .expect("invalid winner snapshot");
        assert_eq!(
            invalid_winner_snapshot
                .hand_tiles
                .iter()
                .filter(|tile| **tile == 1)
                .count(),
            1
        );
    }

    #[test]
    fn settlement_fan_accepts_only_dragon_pair_for_closed_piao() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 31, 35, 35]);
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_pattern_with_rules(
                state.hands.get(&1).unwrap(),
                &[],
                ShenyangMahjongWinRules::new(WIN_RULE_SHENYANG_BASIC)
            ),
            ShenyangMahjongWinPattern::PiaoHu
        );
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            3
        );

        state
            .hands
            .insert(1, vec![1, 1, 1, 11, 11, 11, 21, 21, 21, 31, 31, 35, 35, 35]);
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            0
        );
    }

    #[test]
    fn settlement_fan_counts_chi_as_opening_meld() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![1, 2, 3],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], Some(0), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            2
        );
    }

    #[test]
    fn settlement_fan_counts_concealed_dragon_triplet() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35, 35, 31, 31]);
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_counts_configured_closed_sequence_dragon_pair_win_as_standard() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 4, 5, 6, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");
        let default_configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let disabled_configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);

        assert_eq!(
            winner_hand_fan_with_configs(&state, settlement, 1, &default_configs),
            0
        );
        assert_eq!(
            winner_hand_fan_with_configs(&state, settlement, 1, &disabled_configs),
            1
        );
        assert_eq!(
            shenyang_win_pattern(state.hands.get(&1).unwrap(), &[]),
            ShenyangMahjongWinPattern::Standard
        );
    }

    #[test]
    fn settlement_fan_counts_dragon_concealed_gang() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![35, 35, 35, 35],
                None,
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    }

    #[test]
    fn settlement_fan_counts_dragon_open_gang() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![35, 35, 35, 35],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
    }

    #[test]
    fn settlement_fan_counts_dragon_peng() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![35, 35, 35],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_counts_four_concealed_dragons_as_triplet_and_four_gui_yi() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 35, 35, 35, 35]);
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 6);
    }

    #[test]
    fn settlement_fan_counts_four_gui_yi_across_chi_meld_and_hand() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 2, 2, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![2, 3, 4],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_counts_four_gui_yi_across_peng_meld_and_hand() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 4, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![2, 2, 2],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
    }

    #[test]
    fn settlement_fan_counts_four_gui_yi_and_single_wait_on_seven_pairs() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 1, 1, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![1], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 6);
    }

    #[test]
    fn settlement_fan_counts_honor_single_wait_once() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(35),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);

        assert!(is_single_wait_win(
            &hand_tiles,
            &[],
            settlement.win_tile,
            WIN_RULE_RELAXED
        ));
        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_counts_middle_tile_single_wait_on_seven_pairs() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 3, 3, 4, 4, 5, 11, 11, 21, 21]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(5),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            5
        );
    }

    #[test]
    fn settlement_fan_counts_ordinary_concealed_gang() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![2, 2, 2, 2],
                None,
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
    }

    #[test]
    fn settlement_fan_counts_ordinary_open_gang() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![2, 2, 2, 2],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_counts_piao_hu_with_concealed_gang() {
        let mut state = playable_state();
        state.hands.insert(1, vec![35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    }

    #[test]
    fn settlement_fan_counts_piao_hu_with_open_gang() {
        let mut state = playable_state();
        state.hands.insert(1, vec![35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 4);
    }

    #[test]
    fn settlement_fan_counts_pure_one_suit_with_concealed_gang_and_single_wait() {
        let mut state = playable_state();
        state.hands.insert(1, vec![5, 5, 6, 7, 8, 9, 9, 9]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::GANG, vec![1, 1, 1, 1], None),
                build_meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(0)),
            ],
        );
        state.enter_settlement(vec![1], None, Some(7), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 7);
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            7
        );
    }

    #[test]
    fn settlement_fan_counts_shou_ba_yi_for_piao_hu() {
        let mut state = playable_state();
        state.hands.insert(1, vec![35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.enter_settlement(vec![1], Some(0), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 5);
    }

    #[test]
    fn settlement_fan_counts_single_middle_pair_wait() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 25]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(25),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);

        assert_eq!(
            hand_tiles,
            vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 25, 25, 31, 31, 31]
        );
        assert!(is_single_wait_win(
            &hand_tiles,
            &[],
            settlement.win_tile,
            WIN_RULE_RELAXED
        ));
        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_counts_terminal_single_wait_once() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![11, 11, 13, 14, 15, 16, 17, 17, 17, 17, 18, 18, 19]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(11),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);

        assert!(is_single_wait_win(
            &hand_tiles,
            &[],
            settlement.win_tile,
            WIN_RULE_RELAXED
        ));
        assert_eq!(winner_hand_fan(&state, settlement, 1), 6);
    }

    #[test]
    fn settlement_fan_counts_terminal_single_wait_when_other_wait_is_discarded_out() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 21, 22, 23, 25, 25, 31, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![11, 12, 13],
                Some(0),
            )],
        );
        for position in 0..4 {
            state.discards.insert(position, vec![4]);
        }
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(1),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
        let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);
        let public_unavailable = public_unavailable_tiles_for_winner(&state, 1);

        assert!(!is_single_wait_win(
            &hand_tiles,
            melds,
            settlement.win_tile,
            WIN_RULE_SHENYANG_BASIC
        ));
        assert_eq!(
            public_unavailable.iter().filter(|tile| **tile == 4).count(),
            4
        );
        assert!(is_single_wait_win_with_known_unavailable_tiles(
            &hand_tiles,
            melds,
            settlement.win_tile,
            WIN_RULE_SHENYANG_BASIC,
            &public_unavailable
        ));
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            2
        );
    }

    #[test]
    fn settlement_fan_counts_terminal_single_wait_when_other_wait_is_exhausted() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![4, 4, 4, 4],
                Some(0),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(1),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
        let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);

        assert!(is_single_wait_win(
            &hand_tiles,
            melds,
            settlement.win_tile,
            WIN_RULE_RELAXED
        ));
        assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
    }

    #[test]
    fn settlement_fan_does_not_count_closed_middle_shape_with_multiple_waits() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![6, 7, 7, 8, 9, 11, 12, 13, 15, 15, 15, 22, 22]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(8),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    }

    #[test]
    fn settlement_fan_does_not_count_four_gui_yi_for_gang_meld() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![3, 4, 5, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![2, 2, 2, 2],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_does_not_count_open_two_sided_wait_as_single_wait() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
    }

    #[test]
    fn settlement_fan_does_not_count_shou_ba_yi_for_standard_hand() {
        let mut state = playable_state();
        state.hands.insert(1, vec![35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::CHI, vec![1, 2, 3], Some(0)),
                build_meld(ShenyangMahjongMeldKind::CHI, vec![11, 12, 13], Some(0)),
                build_meld(ShenyangMahjongMeldKind::CHI, vec![21, 22, 23], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.enter_settlement(vec![1], Some(0), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_does_not_count_terminal_triplet_completion_as_single_wait() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 35, 35, 35]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(1),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
    }

    #[test]
    fn settlement_fan_ignores_gang_draw_flag_on_discard_win() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            true,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(!event.is_gang_draw);
        assert!(!event.winner_details[0].is_gang_draw);
    }

    #[test]
    fn settlement_fan_ignores_haidilao_flag_on_discard_win() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            true,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(!event.is_haidilao);
        assert!(!event.winner_details[0].is_haidilao);
    }

    #[test]
    fn settlement_fan_ignores_invalid_source_melds_for_single_wait() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 21, 22, 23, 25, 25, 31, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![11, 12, 13],
                Some(0),
            )],
        );
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![4, 4, 4, 4],
                Some(2),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(1),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
        let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);
        let public_unavailable = public_unavailable_tiles_for_winner(&state, 1);

        assert_eq!(
            public_unavailable.iter().filter(|tile| **tile == 4).count(),
            0
        );
        assert!(!is_single_wait_win_with_known_unavailable_tiles(
            &hand_tiles,
            melds,
            settlement.win_tile,
            WIN_RULE_SHENYANG_BASIC,
            &public_unavailable
        ));
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            1
        );
    }

    #[test]
    fn settlement_fan_ignores_malformed_melds_for_four_gui_yi() {
        assert_eq!(
            four_gui_yi_fan(
                &[2, 2],
                &[build_meld(
                    ShenyangMahjongMeldKind::PENG,
                    vec![2, 2],
                    Some(0)
                )]
            ),
            0
        );
        assert_eq!(
            four_gui_yi_fan(
                &[2],
                &[build_meld(
                    ShenyangMahjongMeldKind::PENG,
                    vec![2, 2, 2],
                    Some(0)
                )]
            ),
            1
        );
        assert_eq!(
            four_gui_yi_fan(
                &[2],
                &[build_meld(
                    ShenyangMahjongMeldKind::CHI,
                    vec![2, 2, 2],
                    Some(0)
                )]
            ),
            0
        );
        assert_eq!(
            four_gui_yi_fan(
                &[2, 2, 2],
                &[build_meld(
                    ShenyangMahjongMeldKind::CHI,
                    vec![2, 3, 4],
                    Some(0)
                )]
            ),
            1
        );
        assert_eq!(
            four_gui_yi_fan(
                &[99],
                &[build_meld(
                    ShenyangMahjongMeldKind::PENG,
                    vec![99, 99, 99],
                    Some(0)
                )]
            ),
            0
        );
        assert_eq!(four_gui_yi_fan(&[99, 99, 99, 99], &[]), 0);
    }

    #[test]
    fn settlement_fan_ignores_reverse_win_flag_on_self_draw() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 4, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.enter_settlement_with_reverse_win(vec![1], Some(0), None, true, true, false, false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert_eq!(event.from_position, None);
        assert!(!event.is_reverse_win);
        assert!(!event.winner_details[0].is_reverse_win);
    }

    #[test]
    fn settlement_fan_rejects_invalid_meld_for_single_wait() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(0),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(35),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");
        let hand_tiles = winner_final_hand_tiles(&state, settlement, 1);
        let melds = state.melds.get(&1).map(Vec::as_slice).unwrap_or(&[]);

        assert!(!is_single_wait_win(
            &hand_tiles,
            melds,
            settlement.win_tile,
            WIN_RULE_RELAXED
        ));
        assert_eq!(winner_hand_fan(&state, settlement, 1), 0);
    }

    #[test]
    fn settlement_fan_rejects_invalid_tile_melds() {
        let mut invalid_gang_state = playable_state();
        invalid_gang_state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
        invalid_gang_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![99, 99, 99, 99],
                None,
            )],
        );
        invalid_gang_state.enter_settlement(vec![1], None, None, true);
        let invalid_gang_settlement = invalid_gang_state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_hand_fan(&invalid_gang_state, invalid_gang_settlement, 1),
            0
        );
    }

    #[test]
    fn settlement_fan_rejects_short_dragon_melds() {
        let mut short_gang_state = playable_state();
        short_gang_state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
        short_gang_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![35, 35, 35],
                None,
            )],
        );
        short_gang_state.enter_settlement(vec![1], None, None, true);
        let short_gang_settlement = short_gang_state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_hand_fan(&short_gang_state, short_gang_settlement, 1),
            0
        );

        let mut short_peng_state = playable_state();
        short_peng_state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31]);
        short_peng_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![35, 35],
                Some(0),
            )],
        );
        short_peng_state.enter_settlement(vec![1], None, None, true);
        let short_peng_settlement = short_peng_state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_hand_fan(&short_peng_state, short_peng_settlement, 1),
            0
        );
    }

    #[test]
    fn settlement_fan_requires_gang_meld_and_empty_wall_for_draw_bonuses() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.enter_settlement_with_reverse_win(vec![1], None, None, true, false, true, true);
        let settlement = state.settlement.clone().expect("settlement");

        assert_eq!(winner_hand_fan(&state, &settlement, 1), 2);
        let no_gang_event =
            build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(!no_gang_event.is_gang_draw);
        assert!(no_gang_event.is_haidilao);
        assert!(!no_gang_event.winner_details[0].is_gang_draw);
        assert!(no_gang_event.winner_details[0].is_haidilao);

        state
            .hands
            .insert(1, vec![3, 4, 5, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![2, 2, 2, 2],
                None,
            )],
        );

        assert_eq!(winner_hand_fan(&state, &settlement, 1), 5);
        let valid_event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(valid_event.is_gang_draw);
        assert!(valid_event.is_haidilao);
        assert!(valid_event.winner_details[0].is_gang_draw);
        assert!(valid_event.winner_details[0].is_haidilao);

        state.wall = vec![35];

        assert_eq!(winner_hand_fan(&state, &settlement, 1), 4);
        let nonempty_wall_event =
            build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(nonempty_wall_event.is_gang_draw);
        assert!(!nonempty_wall_event.is_haidilao);
        assert!(nonempty_wall_event.winner_details[0].is_gang_draw);
        assert!(!nonempty_wall_event.winner_details[0].is_haidilao);
    }

    #[test]
    fn settlement_fan_requires_open_peng_source_for_rob_gang() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 21, 22, 23, 31, 31]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            true,
            false,
            false,
        );
        let settlement = state.settlement.clone().expect("settlement");

        assert_eq!(winner_hand_fan(&state, &settlement, 1), 1);
        let invalid_event =
            build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(!invalid_event.is_reverse_win);
        assert!(!invalid_event.winner_details[0].is_reverse_win);

        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![4, 4, 4],
                Some(2),
            )],
        );

        assert_eq!(winner_hand_fan(&state, &settlement, 1), 2);
        let valid_event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        assert!(valid_event.is_reverse_win);
        assert!(valid_event.winner_details[0].is_reverse_win);
        assert!(!valid_event.is_gang_draw);
    }

    #[test]
    fn settlement_fan_requires_win_tile_for_shou_ba_yi() {
        let mut state = playable_state();
        state.hands.insert(1, vec![35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.enter_settlement(vec![1], None, None, true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 3);
    }

    #[test]
    fn settlement_fan_uses_win_rule_for_single_wait() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(35),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
        assert_eq!(
            winner_hand_fan_with_rule(&state, settlement, 1, WIN_RULE_SHENYANG_BASIC),
            0
        );
    }

    #[test]
    fn settlement_rejects_missing_discard_win_tile() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![1], Some(0), None, false);
        let invalid_settlement = state.settlement.clone().expect("settlement");

        assert_eq!(winner_hand_fan(&state, &invalid_settlement, 1), 0);
        assert!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions
                .is_empty()
        );

        state.hands.get_mut(&1).unwrap().pop();
        state.settlement.as_mut().unwrap().win_tile = Some(35);
        let valid_settlement = state.settlement.as_ref().expect("settlement");

        assert!(winner_hand_fan(&state, valid_settlement, 1) > 0);
        assert_eq!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions,
            vec![1]
        );
    }

    #[test]
    fn settlement_rejects_multiple_self_draw_winners() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state
            .hands
            .insert(2, vec![3, 3, 4, 4, 13, 13, 14, 14, 23, 23, 24, 24, 35, 35]);
        state.enter_settlement(vec![1, 2], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        assert!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions
                .is_empty()
        );
    }

    #[test]
    fn settlement_rejects_public_fifth_claim_tile() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 1, 1, 2, 3, 7, 8, 11, 12, 13, 35, 35]);
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![6, 6, 6, 6],
                None,
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(6),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.clone().expect("settlement");

        assert_eq!(known_tile_count(&state, 6), 4);
        assert!(!position_has_impossible_known_tile_count(&state, 1));
        assert!(winner_has_impossible_known_tile_count(
            &state,
            &settlement,
            1
        ));
        assert_eq!(winner_hand_fan(&state, &settlement, 1), 0);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], &settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );

        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![6, 6, 6],
                Some(3),
            )],
        );
        state.discards.insert(0, vec![6]);

        assert_eq!(known_tile_count(&state, 6), 4);
        assert!(!winner_has_impossible_known_tile_count(
            &state,
            &settlement,
            1
        ));
        assert!(winner_hand_fan(&state, &settlement, 1) > 0);
    }

    #[test]
    fn settlement_rejects_public_fifth_copy_used_by_self_draw_winner() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6]);
        state.discards.insert(2, vec![1]);
        state.enter_settlement(vec![1], None, Some(6), true);
        let settlement = state.settlement.clone().expect("settlement");

        assert_eq!(known_tile_count(&state, 1), 5);
        assert!(position_has_impossible_known_tile_count(&state, 1));
        assert!(winner_has_impossible_known_tile_count(
            &state,
            &settlement,
            1
        ));
        assert_eq!(winner_hand_fan(&state, &settlement, 1), 0);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], &settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        assert!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions
                .is_empty()
        );

        state.discards.insert(2, vec![9, 9, 9, 9, 9]);

        assert_eq!(known_tile_count(&state, 9), 5);
        assert!(!position_has_impossible_known_tile_count(&state, 1));
        assert!(!winner_has_impossible_known_tile_count(
            &state,
            &settlement,
            1
        ));
        assert!(winner_hand_fan(&state, &settlement, 1) > 0);
        assert_eq!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions,
            vec![1]
        );
    }

    #[test]
    fn settlement_rejects_unknown_discard_payer() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);
        state.enter_settlement(vec![1], Some(9), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        assert!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions
                .is_empty()
        );
    }

    #[test]
    fn settlement_rejects_unknown_self_draw_winner() {
        let mut state = playable_state();
        state
            .hands
            .insert(9, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![9], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        assert!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions
                .is_empty()
        );
    }

    #[test]
    fn settlement_rejects_unowned_self_draw_win_tile() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![1], None, Some(9), true);
        let invalid_settlement = state.settlement.clone().expect("settlement");

        assert_eq!(winner_hand_fan(&state, &invalid_settlement, 1), 0);
        assert!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions
                .is_empty()
        );

        state.settlement.as_mut().unwrap().win_tile = Some(35);
        let valid_settlement = state.settlement.as_ref().expect("settlement");

        assert!(winner_hand_fan(&state, valid_settlement, 1) > 0);
        assert_eq!(
            build_settlement_event(&state)
                .expect("settlement event")
                .winner_positions,
            vec![1]
        );
    }

    #[test]
    fn settlement_score_adds_closed_fan_when_discard_payer_has_not_opened() {
        let open_non_payer_meld = || vec![open_peng_meld(31, 2)];
        let mut closed_payer_state = playable_state();
        closed_payer_state.dealer_position = 2;
        closed_payer_state.melds.insert(3, open_non_payer_meld());
        closed_payer_state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        closed_payer_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        closed_payer_state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let closed_settlement = closed_payer_state.settlement.as_ref().expect("settlement");

        assert_eq!(
            winner_hand_fan(&closed_payer_state, closed_settlement, 1),
            1
        );
        assert_eq!(
            settlement_score_changes_for_state(
                &closed_payer_state,
                &[0, 1, 2, 3],
                closed_settlement,
                &HashMap::new()
            )
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
            vec![(0, -2), (1, 2), (2, 0), (3, 0)]
        );

        for invalid_source in [0, 9] {
            let mut invalid_source_state = playable_state();
            invalid_source_state.dealer_position = 2;
            invalid_source_state.melds.insert(3, open_non_payer_meld());
            invalid_source_state
                .hands
                .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
            invalid_source_state.melds.insert(
                0,
                vec![build_meld(
                    ShenyangMahjongMeldKind::CHI,
                    vec![1, 2, 3],
                    Some(invalid_source),
                )],
            );
            invalid_source_state.melds.insert(
                1,
                vec![build_meld(
                    ShenyangMahjongMeldKind::CHI,
                    vec![21, 22, 23],
                    Some(0),
                )],
            );
            invalid_source_state.enter_settlement_with_reverse_win(
                vec![1],
                Some(0),
                Some(4),
                false,
                false,
                false,
                false,
            );
            let settlement = invalid_source_state
                .settlement
                .as_ref()
                .expect("settlement");

            assert_eq!(
                settlement_score_changes_for_state(
                    &invalid_source_state,
                    &[0, 1, 2, 3],
                    settlement,
                    &HashMap::new()
                )
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
                vec![(0, -2), (1, 2), (2, 0), (3, 0)]
            );
        }

        let mut malformed_open_payer_state = playable_state();
        malformed_open_payer_state.dealer_position = 2;
        malformed_open_payer_state
            .melds
            .insert(3, open_non_payer_meld());
        malformed_open_payer_state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        malformed_open_payer_state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![1, 1],
                Some(1),
            )],
        );
        malformed_open_payer_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        malformed_open_payer_state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let malformed_open_settlement = malformed_open_payer_state
            .settlement
            .as_ref()
            .expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(
                &malformed_open_payer_state,
                &[0, 1, 2, 3],
                malformed_open_settlement,
                &HashMap::new()
            )
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
            vec![(0, -2), (1, 2), (2, 0), (3, 0)]
        );

        let mut invalid_tile_open_payer_state = playable_state();
        invalid_tile_open_payer_state.dealer_position = 2;
        invalid_tile_open_payer_state
            .melds
            .insert(3, open_non_payer_meld());
        invalid_tile_open_payer_state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        invalid_tile_open_payer_state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(1),
            )],
        );
        invalid_tile_open_payer_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        invalid_tile_open_payer_state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let invalid_tile_open_settlement = invalid_tile_open_payer_state
            .settlement
            .as_ref()
            .expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(
                &invalid_tile_open_payer_state,
                &[0, 1, 2, 3],
                invalid_tile_open_settlement,
                &HashMap::new()
            )
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
            vec![(0, -2), (1, 2), (2, 0), (3, 0)]
        );

        let mut open_payer_state = playable_state();
        open_payer_state.dealer_position = 2;
        open_payer_state.melds.insert(3, open_non_payer_meld());
        open_payer_state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        open_payer_state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![1, 2, 3],
                Some(3),
            )],
        );
        open_payer_state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        open_payer_state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let open_settlement = open_payer_state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&open_payer_state, open_settlement, 1), 1);
        assert_eq!(
            settlement_score_changes_for_state(
                &open_payer_state,
                &[0, 1, 2, 3],
                open_settlement,
                &HashMap::new()
            )
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
            vec![(0, -1), (1, 1), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_score_adds_dealer_fan_when_dealer_self_draws() {
        let mut state = playable_state();
        state.dealer_position = 2;
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![2], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, -8), (2, 24), (3, -8)]
        );
    }

    #[test]
    fn settlement_score_adds_dealer_fan_when_payer_is_open_dealer() {
        let mut state = playable_state();
        state.dealer_position = 0;
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![9, 9, 9],
                Some(1),
            )],
        );
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -2), (1, 2), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_score_adds_dealer_fan_when_winner_is_dealer() {
        let mut state = playable_state();
        state.dealer_position = 0;
        state
            .hands
            .insert(0, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![31, 31, 31],
                Some(0),
            )],
        );
        state.enter_settlement(vec![0], Some(1), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 0), 5);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 6), (1, -6), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_score_adds_payer_state_after_hand_fan_cap() {
        let mut state = playable_state();
        state.dealer_position = 2;
        state.hands.insert(1, vec![35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![9, 9, 9],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], Some(2), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 5), (2, -5), (3, 0)]
        );
    }

    #[test]
    fn settlement_score_caps_winner_hand_fan() {
        let mut state = playable_state();
        state.hands.insert(1, vec![35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![1, 1, 1], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(3)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(0)),
            ],
        );
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![9, 9, 9],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], Some(2), Some(35), false);
        let settlement = state.settlement.as_ref().expect("settlement");
        let configs = HashMap::from([("max_fan".to_owned(), 4)]);

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &configs)
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 4), (2, -4), (3, 0)]
        );
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
    fn settlement_score_counts_concealed_gang_discard_payer_as_closed() {
        let mut state = playable_state();
        state.dealer_position = 2;
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![31, 31, 31, 31],
                None,
            )],
        );
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        state.melds.insert(3, vec![open_peng_meld(34, 2)]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -2), (1, 2), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_score_counts_three_closed_losers_on_discard_win() {
        let mut state = playable_state();
        state.dealer_position = 2;
        state
            .hands
            .insert(1, vec![2, 3, 5, 6, 7, 11, 12, 13, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![21, 22, 23],
                Some(0),
            )],
        );
        state.enter_settlement(vec![1], Some(0), Some(4), false);
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 1);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -3), (1, 3), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_score_ignores_illegal_winner_hand() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![99, 99, 99],
                Some(0),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(35),
            false,
            false,
            false,
            false,
        );
        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(winner_hand_fan(&state, settlement, 1), 0);
        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn settlement_scores_closed_sequence_dragon_pair_winner_after_xi_gang() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::XI_GANG,
                vec![31, 32, 33, 34],
                None,
            )],
        );
        state.enter_settlement(vec![1], None, Some(35), true);
        let settlement = state.settlement.as_ref().expect("settlement");
        let default_configs = HashMap::from([("win_rule".to_owned(), 1)]);
        let disabled_configs = HashMap::from([
            ("win_rule".to_owned(), 1),
            ("allow_first_chi".to_owned(), 0),
        ]);

        assert!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &default_configs)
                .iter()
                .all(|change| change.score == 0)
        );
        assert_eq!(
            settlement_score_changes_for_state(
                &state,
                &[0, 1, 2, 3],
                settlement,
                &disabled_configs
            )
            .into_iter()
            .map(|change| (change.position, change.score))
            .collect::<Vec<_>>(),
            vec![(0, -6), (1, 16), (2, -5), (3, -5)]
        );
        let event = build_settlement_event_with_configs(&state, &disabled_configs)
            .expect("configured closed win settlement event");
        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::Standard
        );
        assert_eq!(event.winner_details[0].score, 16);
    }

    #[test]
    fn settlement_self_draw_counts_all_three_closed_payers() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.enter_settlement(vec![2], None, Some(35), true);

        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, -7), (2, 22), (3, -7)]
        );
    }

    #[test]
    fn settlement_self_draw_counts_concealed_gang_payer_as_closed() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::GANG,
                vec![31, 31, 31, 31],
                None,
            )],
        );
        state.enter_settlement(vec![2], None, Some(35), true);

        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, -7), (2, 22), (3, -7)]
        );
    }

    #[test]
    fn settlement_self_draw_counts_xi_gang_payer_as_closed() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::XI_GANG,
                vec![31, 32, 33, 34],
                None,
            )],
        );
        state.enter_settlement(vec![2], None, Some(35), true);

        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, -7), (2, 22), (3, -7)]
        );
    }

    #[test]
    fn settlement_self_draw_treats_chi_only_payer_as_open() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::CHI,
                vec![11, 12, 13],
                Some(0),
            )],
        );
        state.enter_settlement(vec![2], None, Some(35), true);

        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -7), (1, -5), (2, 18), (3, -6)]
        );
    }

    #[test]
    fn settlement_self_draw_uses_single_closed_fan_when_any_payer_opened() {
        let mut state = playable_state();
        state
            .hands
            .insert(2, vec![1, 1, 2, 2, 11, 11, 12, 12, 21, 21, 22, 22, 35, 35]);
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![31, 31, 31],
                Some(0),
            )],
        );
        state.enter_settlement(vec![2], None, Some(35), true);

        let settlement = state.settlement.as_ref().expect("settlement");

        assert_eq!(
            settlement_score_changes_for_state(&state, &[0, 1, 2, 3], settlement, &HashMap::new())
                .into_iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -7), (1, -5), (2, 18), (3, -6)]
        );
    }

    #[test]
    fn settlement_winner_details_describe_piao_hu() {
        let mut state = playable_state();
        state.hands.insert(1, vec![1, 1, 35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![31, 31, 31], Some(3)),
            ],
        );
        state.melds.insert(3, vec![open_peng_meld(34, 2)]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(1),
            false,
            false,
            false,
            false,
        );

        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();

        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::PiaoHu
        );
        assert_eq!(event.winner_details[0].score, 5);
    }

    #[test]
    fn settlement_winner_details_describe_pure_one_suit() {
        let mut state = playable_state();
        state.hands.insert(1, vec![1, 2, 3, 4, 5, 6, 7]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::CHI, vec![2, 3, 4], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![9, 9, 9], Some(2)),
            ],
        );
        state.melds.insert(3, vec![open_peng_meld(34, 2)]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(7),
            false,
            false,
            false,
            false,
        );

        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();

        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::PureOneSuit
        );
        assert_eq!(event.winner_details[0].score, 6);
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
        assert_eq!(event.winner_details[0].score, 22);
        assert_eq!(
            event
                .score_changes
                .iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -8), (1, -7), (2, 22), (3, -7)]
        );
    }

    #[test]
    fn settlement_winner_details_do_not_describe_sequence_remainder_as_piao_hu() {
        let mut state = playable_state();
        state.dealer_position = 2;
        state.hands.insert(1, vec![1, 1, 2, 3, 35, 35, 35]);
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![11, 11, 11], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![21, 21, 21], Some(2)),
            ],
        );
        state.melds.insert(3, vec![open_peng_meld(34, 2)]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(4),
            false,
            false,
            false,
            false,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();

        assert_eq!(winner_hand_fan(&state, settlement, 1), 2);
        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::Standard
        );
        assert_eq!(event.winner_details[0].score, 3);
    }

    #[test]
    fn settlement_winner_details_include_reverse_win_and_score() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(3),
            false,
            true,
            false,
            false,
        );

        let event = build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();

        assert_eq!(event.winner_details.len(), 1);
        assert_eq!(event.winner_details[0].position, 1);
        assert_eq!(
            event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::Standard
        );
        assert!(event.winner_details[0].is_reverse_win);
        assert_eq!(event.winner_details[0].score, 4);
    }

    #[test]
    fn settlement_winner_details_use_win_rule_for_closed_pure_one_suit() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 3, 2, 3, 4, 4, 5, 6, 7, 7, 7, 9]);
        state.melds.insert(3, vec![open_peng_meld(34, 2)]);
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(9),
            false,
            false,
            false,
            false,
        );

        let relaxed_event =
            build_settlement_event_with_configs(&state, &relaxed_configs()).unwrap();
        let basic_event = build_settlement_event_with_configs(
            &state,
            &HashMap::from([("win_rule".to_owned(), 1)]),
        )
        .unwrap();

        assert_eq!(
            relaxed_event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::PureOneSuit
        );
        assert_eq!(
            basic_event.winner_details[0].pattern,
            ShenyangMahjongWinPattern::PureOneSuit
        );
        assert_eq!(
            winner_hand_fan_with_rule(
                &state,
                state.settlement.as_ref().expect("settlement"),
                1,
                WIN_RULE_SHENYANG_BASIC
            ),
            4
        );
        assert_eq!(basic_event.winner_details[0].score, 6);
    }

    fn setup_request_room() -> (
        RoomService,
        ShenyangMahjongGameHandler,
        String,
        LoopStateHandle,
    ) {
        setup_request_room_with_configs(serde_json::json!({"win_rule":0}))
    }

    fn setup_request_room_with_configs(
        configs: serde_json::Value,
    ) -> (
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
        if configs.as_object().is_some_and(|items| !items.is_empty()) {
            let _ = room_service.handle_common_request(
                1,
                &ClientRequest {
                    route: Routes::SETTING as i32,
                    data: serde_json::json!({ "current_configs": configs }),
                },
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            );
        }
        let room_key = room_service.room_key_of(1).expect("room key");
        let common = room_service
            .room_common_state(&room_key)
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
    fn stale_same_name_loop_state_does_not_block_recreated_room_start() {
        let mut room_service = RoomService::default();
        let join = |room: &mut RoomService, session_id: SessionId, prefix: &str| {
            room.handle_common_request(
                session_id,
                &ClientRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": format!("{prefix}-{session_id}"),
                        "password": "same-name",
                        "game_id": GameId::SHENYANG_MAHJONG as i32
                    }),
                },
                GameId::SHENYANG_MAHJONG,
                build_shenyang_mahjong_settings,
            )
        };
        for session_id in 1..=4 {
            let _ = join(&mut room_service, session_id, "old");
        }
        let old_common = room_service
            .room_common_state("same-name")
            .expect("old room common state");
        let old_loop_state = Arc::new(StdMutex::new(ShenyangMahjongLoopState::new(Arc::clone(
            &old_common,
        ))));
        room_service.set_room_game_state(
            "same-name",
            Box::new(ShenyangMahjongGameState::from_loop_state(Arc::clone(
                &old_loop_state,
            ))),
        );
        let mut handler = ShenyangMahjongGameHandler::default();
        handler
            .loop_states
            .lock()
            .unwrap()
            .insert("same-name".to_string(), Arc::clone(&old_loop_state));

        for session_id in 1..=4 {
            let _ = room_service.disconnect(session_id);
        }
        assert!(!room_service.room_exists("same-name"));
        assert!(old_common.lock().unwrap().stop_requested());

        for session_id in 5..=8 {
            let _ = join(&mut room_service, session_id, "new");
        }
        let recreated_common = room_service
            .room_common_state("same-name")
            .expect("recreated room common state");
        assert!(!Arc::ptr_eq(&old_common, &recreated_common));
        assert!(
            handler
                .current_loop_state(&room_service, "same-name")
                .is_none()
        );

        let started = handler.handle_start(&mut room_service, 5);

        assert_eq!(
            response_code(&started, 5, Routes::START),
            Some(WsResponseCode::OK as i32)
        );
        let new_state = handler
            .loop_state("same-name")
            .expect("new mahjong loop state");
        let new_common = Arc::clone(&new_state.lock().unwrap().base);
        assert!(Arc::ptr_eq(
            &new_common,
            &room_service
                .room_common_state("same-name")
                .expect("current room common state")
        ));
        assert!(!Arc::ptr_eq(&old_common, &new_common));
    }

    #[test]
    fn table_and_settlement_snapshots_filter_invalid_discards() {
        let mut state = playable_state();
        state.discards.insert(1, vec![3, 99, 35, -1]);
        state.enter_settlement(Vec::new(), None, None, false);

        let snapshot = build_table_snapshot_event_with_configs(&state, 0, &relaxed_configs());
        let public_discards = &snapshot
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("public seat 1")
            .discards;
        let settlement = snapshot.settlement.expect("settlement");
        let settlement_discards = &settlement
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("settlement seat 1")
            .discards;

        assert_eq!(public_discards, &vec![3, 35]);
        assert_eq!(settlement_discards, &vec![3, 35]);
    }

    #[test]
    fn table_snapshot_exposes_xi_gang_options_only_to_current_player() {
        let mut state = playable_state();
        state.current_position = 1;
        state.xi_gang_options.insert(1, vec![vec![35, 36, 37]]);

        let owner = build_table_snapshot_event_with_configs(&state, 1, &HashMap::new());
        let opponent = build_table_snapshot_event_with_configs(&state, 0, &HashMap::new());

        assert_eq!(owner.xi_gang_options, vec![vec![35, 36, 37]]);
        assert!(opponent.xi_gang_options.is_empty());
    }

    #[test]
    fn table_snapshot_filters_drawn_tile_and_claim_options() {
        let mut state = playable_state();
        state.current_position = 0;
        state.last_drawn_tile = Some(9);
        state.wall = vec![36];
        state
            .hands
            .insert(0, vec![1, 2, 4, 5, 6, 7, 9, 11, 12, 13, 21, 22, 23, 31]);
        state
            .hands
            .insert(1, vec![1, 2, 3, 3, 3, 4, 11, 12, 13, 21, 22, 23, 31]);
        state
            .hands
            .insert(2, vec![1, 2, 11, 12, 13, 21, 22, 23, 32, 32, 32, 35, 35]);
        state.discards.insert(0, vec![3]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 2],
            responses: HashMap::new(),
        });
        state.set_turn_countdown(4);

        let drawer_snapshot =
            build_table_snapshot_event_with_configs(&state, 0, &relaxed_configs());
        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());
        let claim_window = snapshot.claim_window.expect("claim window");
        let option = claim_window
            .options
            .iter()
            .find(|option| option.position == 1)
            .expect("claim option");

        assert_eq!(drawer_snapshot.last_drawn_tile, Some(9));
        assert_eq!(snapshot.last_drawn_tile, None);
        assert_eq!(claim_window.tile, 3);
        assert_eq!(claim_window.from_position, 0);
        assert_eq!(claim_window.eligible_positions, vec![1]);
        assert_eq!(claim_window.seconds, 4);
        assert!(!claim_window.is_rob_gang);
        assert!(option.can_peng);
        assert!(option.can_gang);
        assert!(option.chi_options.contains(&vec![1, 2]));
        assert!(option.chi_options.contains(&vec![2, 4]));
        assert_eq!(claim_window.options.len(), 1);

        let observer_snapshot =
            build_table_snapshot_event_with_configs(&state, 3, &relaxed_configs());
        let observer_claim_window = observer_snapshot.claim_window.expect("claim window");
        assert_eq!(observer_claim_window.tile, 3);
        assert_eq!(observer_claim_window.from_position, 0);
        assert!(observer_claim_window.eligible_positions.is_empty());
        assert!(observer_claim_window.options.is_empty());

        state
            .claim_window
            .as_mut()
            .unwrap()
            .responses
            .insert(1, ClaimResponse::Pass);
        let responded_snapshot =
            build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());
        let responded_claim_window = responded_snapshot.claim_window.expect("claim window");
        assert_eq!(responded_claim_window.tile, 3);
        assert!(responded_claim_window.eligible_positions.is_empty());
        assert!(responded_claim_window.options.is_empty());

        let pending_snapshot =
            build_table_snapshot_event_with_configs(&state, 2, &relaxed_configs());
        let pending_claim_window = pending_snapshot.claim_window.expect("claim window");
        assert_eq!(pending_claim_window.eligible_positions, vec![2]);
        assert_eq!(pending_claim_window.options.len(), 1);
        assert_eq!(pending_claim_window.options[0].position, 2);
        assert!(pending_claim_window.options[0].can_hu);

        state.claim_window.as_mut().unwrap().eligible_positions = vec![1];
        let excluded_snapshot =
            build_table_snapshot_event_with_configs(&state, 2, &relaxed_configs());
        let excluded_claim_window = excluded_snapshot.claim_window.expect("claim window");
        assert!(excluded_claim_window.eligible_positions.is_empty());
        assert!(excluded_claim_window.options.is_empty());
    }

    #[test]
    fn table_snapshot_filters_malformed_meld_shapes() {
        let mut state = playable_state();
        state.melds.insert(
            1,
            vec![
                build_meld(ShenyangMahjongMeldKind::PENG, vec![3, 3], Some(0)),
                build_meld(ShenyangMahjongMeldKind::PENG, vec![4, 4, 4], Some(2)),
            ],
        );
        state.enter_settlement(Vec::new(), None, None, false);

        let snapshot = build_table_snapshot_event_with_configs(&state, 0, &relaxed_configs());
        let public_melds = &snapshot
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("public seat 1")
            .melds;
        let settlement = snapshot.settlement.as_ref().expect("settlement");
        let settlement_melds = &settlement
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("settlement seat 1")
            .melds;

        assert_eq!(public_melds.len(), 1);
        assert_eq!(public_melds[0].tiles, vec![4, 4, 4]);
        assert_eq!(settlement_melds.len(), 1);
        assert_eq!(settlement_melds[0].tiles, vec![4, 4, 4]);
    }

    #[test]
    fn table_snapshot_filters_melds_with_invalid_source_positions() {
        let mut state = playable_state();
        state.melds.insert(
            1,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(1),
            )],
        );
        state.melds.insert(
            2,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![4, 4, 4],
                Some(3),
            )],
        );
        state.enter_settlement(Vec::new(), None, None, false);

        let snapshot = build_table_snapshot_event_with_configs(&state, 0, &relaxed_configs());
        let public_invalid = snapshot
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("public seat 1");
        let public_valid = snapshot
            .players
            .iter()
            .find(|player| player.position == 2)
            .expect("public seat 2");
        let settlement = snapshot.settlement.expect("settlement");
        let settlement_invalid = settlement
            .players
            .iter()
            .find(|player| player.position == 1)
            .expect("settlement seat 1");
        let settlement_valid = settlement
            .players
            .iter()
            .find(|player| player.position == 2)
            .expect("settlement seat 2");

        assert!(public_invalid.melds.is_empty());
        assert_eq!(public_valid.melds.len(), 1);
        assert!(settlement_invalid.melds.is_empty());
        assert_eq!(settlement_valid.melds.len(), 1);
    }

    #[test]
    fn table_snapshot_hides_claim_window_outside_play_phase() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, vec![3]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });
        state.phase = ShenyangMahjongPhase::Settlement;

        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());

        assert!(snapshot.claim_window.is_none());
    }

    #[test]
    fn table_snapshot_hides_claim_window_with_invalid_source() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(9, vec![3]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 9,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });

        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());

        assert!(snapshot.claim_window.is_none());
    }

    #[test]
    fn table_snapshot_hides_claim_window_with_invalid_tile() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, vec![99]);
        state.claim_window = Some(ClaimWindowState {
            tile: 99,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1],
            responses: HashMap::new(),
        });

        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());

        assert!(snapshot.claim_window.is_none());
    }

    #[test]
    fn table_snapshot_hides_claim_window_with_malformed_participants() {
        let mut state = playable_state();
        state.current_position = 0;
        state.discards.insert(0, vec![3]);
        state.claim_window = Some(ClaimWindowState {
            tile: 3,
            from_position: 0,
            kind: ClaimWindowKind::Discard,
            eligible_positions: vec![1, 9],
            responses: HashMap::new(),
        });

        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());

        assert!(snapshot.claim_window.is_none());
    }

    #[test]
    fn table_snapshot_includes_settlement_for_rejoin() {
        let mut state = playable_state();
        state
            .hands
            .insert(1, vec![1, 2, 11, 12, 13, 21, 22, 23, 31, 31, 31, 35, 35]);
        state.melds.insert(
            0,
            vec![build_meld(
                ShenyangMahjongMeldKind::PENG,
                vec![3, 3, 3],
                Some(2),
            )],
        );
        state.enter_settlement_with_reverse_win(
            vec![1],
            Some(0),
            Some(3),
            false,
            true,
            false,
            false,
        );

        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());
        let settlement = snapshot.settlement.expect("settlement");

        assert_eq!(snapshot.phase, ShenyangMahjongPhase::Settlement);
        assert_eq!(settlement.winner_positions, vec![1]);
        assert_eq!(settlement.from_position, Some(0));
        assert_eq!(settlement.win_tile, Some(3));
        assert!(settlement.is_reverse_win);
        assert_eq!(settlement.winner_details.len(), 1);
        assert_eq!(settlement.winner_details[0].position, 1);
        assert_eq!(settlement.winner_details[0].score, 4);
        assert_eq!(
            settlement
                .score_changes
                .iter()
                .map(|change| (change.position, change.score))
                .collect::<Vec<_>>(),
            vec![(0, -4), (1, 4), (2, 0), (3, 0)]
        );
    }

    #[test]
    fn table_snapshot_marks_disconnected_player_as_away() {
        let state = playable_state();
        state.base.lock().unwrap().mark_disconnected(2);

        let snapshot = build_table_snapshot_event_with_configs(&state, 1, &relaxed_configs());
        let player = snapshot
            .players
            .iter()
            .find(|player| player.position == 2)
            .expect("player snapshot");

        assert!(player.away);
        assert!(!player.is_ai);
    }

    #[test]
    fn two_xi_gangs_stack_two_fan_and_keep_hand_closed() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 31, 32, 33, 34, 35, 36, 37]);
        state.melds.insert(1, Vec::new());
        state.last_drawn_tile = Some(37);
        state.wall = vec![22];
        state
            .xi_gang_options
            .insert(1, vec![vec![31, 32, 33, 34], vec![35, 36, 37]]);
        let mut dispatch = Dispatch::default();

        assert!(perform_xi_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            1,
            &[31, 32, 33, 34],
        ));
        assert!(perform_xi_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            1,
            &[35, 36, 37],
        ));

        let melds = state.melds.get(&1).unwrap();
        assert_eq!(melds.len(), 2);
        assert_eq!(shenyang_score_meld_fan(melds), 2);
        assert!(!position_has_open_meld(&state, 1));
        assert_eq!(state.hands.get(&1).unwrap().len(), 8);
    }

    #[test]
    fn wind_xi_gang_draws_replacement_without_creating_gang_draw() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 21, 22, 23, 35, 31, 32, 33, 34]);
        state.melds.insert(1, Vec::new());
        state.last_drawn_tile = Some(34);
        state.pending_gang_draw = true;
        state.wall = vec![36];
        state.xi_gang_options.insert(1, vec![vec![31, 32, 33, 34]]);
        let mut dispatch = Dispatch::default();

        assert!(perform_xi_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &HashMap::new(),
            &mut dispatch,
            1,
            &[31, 32, 33, 34],
        ));

        assert!(state.wall.is_empty());
        assert_eq!(state.last_drawn_tile, Some(36));
        assert!(!state.pending_gang_draw);
        assert_eq!(state.hands.get(&1).unwrap().len(), 11);
        assert!(state.hands.get(&1).unwrap().contains(&36));
        assert!(!position_has_open_meld(&state, 1));
        assert!(dispatch.messages.iter().any(|message| {
            matches!(
                &message.payload,
                OutboundPayload::Event(event)
                    if event.code == WsCode::PLAY as i32
                        && event.data.get("action") == Some(&json!(ShenyangMahjongAction::XI_GANG as i32))
            )
        }));
    }

    #[test]
    fn wind_xi_gang_last_replacement_win_is_haidilao_not_gang_draw() {
        let mut state = playable_state();
        state.current_position = 1;
        state
            .hands
            .insert(1, vec![1, 2, 3, 11, 12, 13, 22, 23, 31, 32, 33, 34, 35, 35]);
        state.melds.insert(1, Vec::new());
        state.last_drawn_tile = Some(34);
        state.wall = vec![24];
        state.xi_gang_options.insert(1, vec![vec![31, 32, 33, 34]]);
        let mut dispatch = Dispatch::default();

        assert!(perform_xi_gang(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            1,
            &[31, 32, 33, 34],
        ));
        assert!(can_self_draw_hu_with_configs(&state, 1, &relaxed_configs()));
        perform_self_draw_hu(
            &RoomService::default(),
            "room",
            &mut state,
            &relaxed_configs(),
            &mut dispatch,
            1,
        );

        let settlement = state.settlement.as_ref().expect("settlement");
        assert!(settlement.is_haidilao);
        assert!(!settlement.is_gang_draw);
    }
}

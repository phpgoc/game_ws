mod claim;
mod defense;
mod discard;
mod hand;
mod meld;
mod piao;
mod pure_one_suit;
mod round;
mod score;
mod self_gang;
mod seven_pairs;
mod shenyang_rule;
mod table;

#[cfg(test)]
mod tests;
mod tile;
mod types;
mod xi_gang;

use std::cmp::Ordering;
use std::collections::HashMap;

pub use claim::{choose_claim_from_view, should_pass_self_draw_hu_from_view};
pub use discard::{choose_discard_from_view, choose_forced_discard_from_view};
pub use self_gang::choose_self_gang_from_view;
pub use types::AiClaimChoice;
pub use xi_gang::choose_xi_gang_from_view;

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

use crate::rules::{
    ShenyangMahjongWinRules, WIN_RULE_SHENYANG_BASIC, can_chi, can_gang, can_peng,
    has_edge_wait_decomposition, is_complete_win_with_melds_for_rules, is_piao_hu_win,
    is_pure_one_suit_win, is_seven_pairs_win,
    is_single_wait_shape_with_known_unavailable_tiles_for_rules,
    shenyang_score_concealed_dragon_triplet_fan, shenyang_score_four_gui_yi_fan,
    shenyang_score_meld_fan, sort_tiles,
};
#[cfg(test)]
use crate::rules::{
    has_triplet_in_standard_decomposition, is_complete_win, is_complete_win_with_melds,
};

use super::observation::{AiClaimView, AiPublicTable, AiSeatView};
#[cfg(test)]
use claim::*;
use defense::*;
#[cfg(test)]
use discard::*;
use hand::{
    hand_power, has_terminal_or_honor_with_extra, has_triplet_like_group,
    has_triplet_or_dragon_pair, has_triplet_or_dragon_pair_with_extra, is_seven_pairs_wait_shape,
    missing_suits, neighbor_count, pair_count, remove_n_tiles, single_tile, suit_presence,
    suit_presence_with_extra, suited_tile_count_for_suit, terminal_or_honor_count,
    tile_is_core_closed_middle_wait_member, tile_is_core_two_sided_wait_member,
    tile_is_middle_of_sequence, tile_is_part_of_complete_sequence, tile_is_weak_edge_wait_terminal,
};
use meld::{
    claim_gang_meld, claim_peng_meld, has_closed_meld, has_open_meld, has_peng_meld,
    has_virtual_tile_count, is_closed_meld, is_open_meld, is_open_peng_meld, is_sequence_meld,
    is_triplet_like_meld, is_valid_meld, meld_primary_tile, promoted_added_gang_melds,
    valid_meld_count, valid_meld_tiles,
};
use piao::*;
use pure_one_suit::*;
use round::*;
use score::*;
use self_gang::*;
use seven_pairs::*;
use shenyang_rule::*;
#[cfg(test)]
use table::remaining_tile_count_with_melds;
use table::{
    claim_tile_already_visible, exposed_meld_tile_count, known_unavailable_tiles_for_claimed_win,
    known_unavailable_tiles_with_simulated_discards, live_terminal_or_honor_count,
    live_terminal_or_honor_count_after_discard, live_tile_count_for_suit,
    live_tile_count_for_suit_after_discard, next_position_after, open_meld_tile_count,
    open_opponent_exists_for_tile, own_previous_discard_count, public_discard_count,
    public_discard_seat_count, remaining_tile_count,
    remaining_tile_count_with_melds_after_discards, seat_has_open_meld_tile, visible_tile_count,
};
use tile::{
    is_dragon, is_honor, is_suited, is_valid_tile, is_wind, tile_is_terminal, tile_rank, tile_suit,
    unique_tiles,
};

pub(crate) fn position_known_tile_counts_are_possible(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
) -> bool {
    table::position_known_tile_counts_are_possible(hand, melds, table)
}

pub(crate) fn claim_known_tile_counts_are_possible(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    claim: &AiClaimView,
    table: &AiPublicTable,
) -> bool {
    if !is_valid_tile(claim.tile) || !position_known_tile_counts_are_possible(hand, melds, table) {
        return false;
    }
    let represented_claim = claim_tile_already_visible(table, claim.tile);
    hand.iter().filter(|tile| **tile == claim.tile).count()
        + visible_tile_count(table, claim.tile) as usize
        + usize::from(!represented_claim)
        <= 4
}

pub(in crate::ai::decision) fn has_door_opening_meld(
    melds: &[WsShenyangMahjongMeld],
    _table: &AiPublicTable,
) -> bool {
    has_open_meld(melds)
}

pub(in crate::ai::decision) fn win_rules_for_table(
    table: &AiPublicTable,
    win_rule: i32,
) -> ShenyangMahjongWinRules {
    ShenyangMahjongWinRules {
        win_rule,
        allow_closed_dragon_pair_win: !table.allow_first_chi,
    }
}

pub(crate) fn is_complete_win_for_table(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    win_rule: i32,
) -> bool {
    is_complete_win_with_melds_for_rules(hand, melds, win_rules_for_table(table, win_rule))
}

pub(in crate::ai::decision) fn is_single_wait_shape_for_table(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    win_tile: i32,
    table: &AiPublicTable,
    win_rule: i32,
    known_unavailable_tiles: &[i32],
) -> bool {
    is_single_wait_shape_with_known_unavailable_tiles_for_rules(
        hand,
        melds,
        win_tile,
        win_rules_for_table(table, win_rule),
        known_unavailable_tiles,
    )
}

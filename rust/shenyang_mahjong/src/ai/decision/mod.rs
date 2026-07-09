use std::cmp::Ordering;
use std::collections::HashMap;

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
mod tile;
mod types;

pub use claim::choose_claim_from_view;
pub use discard::choose_discard_from_view;
pub use self_gang::choose_self_gang_from_view;
pub use types::AiClaimChoice;

use share_type_public::games::shenyang_mahjong::{
    SHENYANG_MAHJONG_TILE_KINDS, ShenyangMahjongMeldKind, WsShenyangMahjongMeld,
};

use crate::rules::{
    WIN_RULE_SHENYANG_BASIC, can_chi, can_gang, can_peng, is_complete_win_with_melds,
    is_piao_hu_win, is_pure_one_suit_win, is_seven_pairs_win,
    is_single_wait_shape_with_known_unavailable_tiles, shenyang_score_concealed_dragon_triplet_fan,
    shenyang_score_four_gui_yi_fan, shenyang_score_meld_fan, sort_tiles,
};
#[cfg(test)]
use crate::rules::{has_triplet_in_standard_decomposition, is_complete_win};

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
    claim_gang_meld, claim_peng_meld, has_concealed_gang_meld, has_open_meld, has_peng_meld,
    is_open_meld, is_sequence_meld, is_triplet_like_meld, is_valid_meld, meld_primary_tile,
    promoted_added_gang_melds, valid_meld_tiles,
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
#[cfg(test)]
use table::visible_tile_count;
use table::{
    exposed_meld_tile_count, known_unavailable_tiles_with_simulated_discards,
    live_terminal_or_honor_count, live_terminal_or_honor_count_after_discard,
    live_tile_count_for_suit, live_tile_count_for_suit_after_discard, next_position_after,
    open_meld_tile_count, open_opponent_exists_for_tile, own_previous_discard_count,
    public_discard_count, public_discard_seat_count, remaining_tile_count,
    remaining_tile_count_after_discard, remaining_tile_count_with_melds_after_discards,
    seat_has_open_meld_tile,
};
use tile::{
    is_dragon, is_honor, is_suited, is_valid_tile, is_wind, tile_is_terminal, tile_rank, tile_suit,
    unique_tiles,
};

#[cfg(test)]
mod tests;

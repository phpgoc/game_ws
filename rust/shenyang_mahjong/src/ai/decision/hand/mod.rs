use std::collections::{HashMap, HashSet};

use share_type_public::games::shenyang_mahjong::WsShenyangMahjongMeld;

use crate::rules::{
    has_dragon_pair_as_standard_pair, has_triplet_in_standard_decomposition, is_complete_win,
    sort_tiles,
};

use super::meld::{is_triplet_like_meld, valid_meld_tiles};
use super::tile::{is_honor, is_suited, tile_is_terminal, tile_rank, tile_suit, unique_tiles};

mod counts;
mod power;
mod requirements;
mod sequences;
mod suits;

pub(super) use counts::*;
pub(super) use power::*;
pub(super) use requirements::*;
pub(super) use sequences::*;
pub(super) use suits::*;

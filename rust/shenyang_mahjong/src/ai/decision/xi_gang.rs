use super::*;
use crate::rules::{XI_GANG_DRAGONS, XI_GANG_WINDS, is_xi_gang_tiles, tiles_in_hand};

pub fn choose_xi_gang_from_view(
    hand: &[i32],
    candidate_options: &[Vec<i32>],
    table: &AiPublicTable,
    position: usize,
    _win_rule: i32,
) -> Option<Vec<i32>> {
    let melds = table
        .seats
        .get(&position)
        .map(|seat| seat.melds.as_slice())
        .unwrap_or(&[]);
    if !has_virtual_tile_count(hand, melds, 14)
        || !position_known_tile_counts_are_possible(hand, melds, table)
    {
        return None;
    }

    let dragon_pairs = XI_GANG_DRAGONS
        .into_iter()
        .filter(|dragon| hand.iter().filter(|tile| **tile == *dragon).count() == 2)
        .count();
    [XI_GANG_WINDS.as_slice(), XI_GANG_DRAGONS.as_slice()]
        .into_iter()
        .find_map(|expected| {
            let option = candidate_options.iter().find(|option| {
                let mut option = option.to_vec();
                option.sort_unstable();
                option == expected && is_xi_gang_tiles(&option) && tiles_in_hand(hand, &option)
            })?;
            if (expected == XI_GANG_WINDS && table.wall_count == 0)
                || (expected == XI_GANG_DRAGONS && dragon_pairs >= 2)
            {
                return None;
            }
            let mut option = option.clone();
            option.sort_unstable();
            Some(option)
        })
}

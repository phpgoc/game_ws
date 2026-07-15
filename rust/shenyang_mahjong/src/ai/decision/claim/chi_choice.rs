use super::*;

pub(super) fn choose_chi_claim(
    hand: &[i32],
    current_melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
    tile: i32,
    from_position: usize,
    current_ready_score: f64,
) -> Option<AiClaimChoice> {
    if !table.allow_chi {
        return Some(AiClaimChoice::Pass);
    }
    if position != next_position_after(from_position, table) {
        return None;
    }
    if should_preserve_seven_pairs_plan_for_context(hand, current_melds, table, position, win_rule)
    {
        return Some(AiClaimChoice::Pass);
    }
    if should_preserve_piao_plan_for_chi(hand, current_melds, table, position, win_rule) {
        return Some(AiClaimChoice::Pass);
    }
    if current_ready_score > 0.0 {
        return Some(AiClaimChoice::Pass);
    }
    if !is_late_round(table) {
        return Some(AiClaimChoice::Pass);
    }

    let defensive_open = should_claim_chi_to_open_broken_hand_for_defense(
        hand,
        current_melds,
        table,
        position,
        win_rule,
    );
    let pure_chi_suit =
        (pure_one_suit_plan_score_for_context(hand, current_melds, table, position, win_rule)
            > 0.0)
            .then(|| dominant_pure_suit(hand, current_melds))
            .flatten();
    let mut best_ready_chi: Option<(f64, f64, Vec<i32>)> = None;
    let mut best_progress_chi: Option<(f64, Vec<i32>)> = None;
    for consume_tiles in chi_options(hand, tile) {
        if let Some(main_suit) = pure_chi_suit {
            let preserves_pure_suit = std::iter::once(tile)
                .chain(consume_tiles.iter().copied())
                .all(|meld_tile| is_suited(meld_tile) && tile_suit(meld_tile) == main_suit);
            if !preserves_pure_suit {
                continue;
            }
        }
        let mut next = hand.to_vec();
        for consume in &consume_tiles {
            if let Some(index) = next.iter().position(|item| item == consume) {
                next.remove(index);
            }
        }
        next.sort_unstable();
        let mut melds = current_melds.to_vec();
        melds.push(WsShenyangMahjongMeld {
            kind: ShenyangMahjongMeldKind::CHI,
            tiles: {
                let mut tiles = vec![tile, consume_tiles[0], consume_tiles[1]];
                tiles.sort_unstable();
                tiles
            },
            from_position: Some(from_position as i32),
        });
        let after = best_score_after_forced_discard(&next, &melds, table, position, win_rule);
        let after_ready = best_ready_score_after_discard(&next, &melds, table, position, win_rule);
        if after_ready > 0.0 {
            match &best_ready_chi {
                None => best_ready_chi = Some((after_ready, after, consume_tiles)),
                Some((best_ready, best_after, best_tiles)) => {
                    if after_ready > *best_ready
                        || (after_ready == *best_ready
                            && (after > *best_after
                                || (after == *best_after && consume_tiles < *best_tiles)))
                    {
                        best_ready_chi = Some((after_ready, after, consume_tiles));
                    }
                }
            }
            continue;
        }
        match &best_progress_chi {
            None => best_progress_chi = Some((after, consume_tiles)),
            Some((best_after, best_tiles)) => {
                if after > *best_after || (after == *best_after && consume_tiles < *best_tiles) {
                    best_progress_chi = Some((after, consume_tiles));
                }
            }
        }
    }
    if let Some((_, _, consume_tiles)) = best_ready_chi {
        return Some(AiClaimChoice::Chi { consume_tiles });
    }
    if !defensive_open {
        return Some(AiClaimChoice::Pass);
    }
    if let Some((_, consume_tiles)) = best_progress_chi {
        return Some(AiClaimChoice::Chi { consume_tiles });
    }
    Some(AiClaimChoice::Pass)
}

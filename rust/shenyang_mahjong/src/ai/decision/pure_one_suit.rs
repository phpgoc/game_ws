use super::*;

pub(super) fn dominant_pure_suit(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> Option<i32> {
    let mut suit_counts = [0usize; 3];
    for tile in hand.iter().copied().chain(valid_meld_tiles(melds)) {
        if is_suited(tile) {
            suit_counts[tile_suit(tile) as usize] += 1;
        }
    }
    suit_counts
        .into_iter()
        .enumerate()
        .max_by_key(|(_, count)| *count)
        .and_then(|(suit, count)| (count > 0).then_some(suit as i32))
}

fn has_pending_main_suit_claim_opportunity(
    table: &AiPublicTable,
    position: usize,
    main_suit: i32,
) -> bool {
    table.claim_window.as_ref().is_some_and(|claim| {
        claim.from_position != position
            && claim.eligible_positions.contains(&position)
            && is_suited(claim.tile)
            && tile_suit(claim.tile) == main_suit
            && claim_tile_already_visible(table, claim.tile)
    })
}

pub(super) fn has_established_pure_one_suit_route(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    let Some(main_suit) = dominant_pure_suit(hand, melds) else {
        return false;
    };
    hand.iter()
        .all(|tile| is_suited(*tile) && tile_suit(*tile) == main_suit)
        && melds.iter().all(|meld| {
            is_valid_meld(meld)
                && meld
                    .tiles
                    .iter()
                    .all(|tile| is_suited(*tile) && tile_suit(*tile) == main_suit)
        })
}

pub(super) fn is_main_pure_suit_tile(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    tile: i32,
) -> bool {
    dominant_pure_suit(hand, melds).is_some_and(|suit| is_suited(tile) && tile_suit(tile) == suit)
}

pub(super) fn pure_one_suit_discard_bias(
    hand: &[i32],
    tile: i32,
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let current_score = pure_one_suit_plan_score_for_context(hand, melds, table, position);
    let after_discard = remove_n_tiles(hand, tile, 1);
    let after_score = if after_discard.len() + 1 == hand.len() {
        pure_one_suit_plan_score_for_context(&after_discard, melds, table, position)
    } else {
        0.0
    };
    if current_score <= 0.0 && after_score <= 0.0 {
        return 0.0;
    }
    if is_honor(tile) {
        return 72.0;
    }
    if is_main_pure_suit_tile(hand, melds, tile) {
        -26.0
    } else {
        64.0
    }
}

pub(super) fn pure_one_suit_plan_score(hand: &[i32], melds: &[WsShenyangMahjongMeld]) -> f64 {
    let Some((main_suit, main_count, blockers)) = pure_one_suit_shape(hand, melds) else {
        return 0.0;
    };
    if melds.iter().any(|meld| {
        if !is_valid_meld(meld) {
            return false;
        }
        meld.tiles
            .iter()
            .any(|tile| !is_suited(*tile) || tile_suit(*tile) != main_suit)
    }) {
        return 0.0;
    }
    if main_count >= 8 && blockers <= 6 {
        12.0 + main_count as f64 * 2.0 - blockers as f64 * 3.0
    } else {
        0.0
    }
}

pub(super) fn pure_one_suit_plan_score_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> f64 {
    let score = pure_one_suit_plan_score(hand, melds);
    if score <= 0.0 {
        return 0.0;
    }
    let Some((main_suit, _, blockers)) = pure_one_suit_shape(hand, melds) else {
        return 0.0;
    };
    let pending_claim = usize::from(has_pending_main_suit_claim_opportunity(
        table, position, main_suit,
    ));
    let required_main_suit_tiles = blockers.saturating_add(usize::from(hand.len() % 3 == 1));
    if table.wall_count.saturating_add(pending_claim) < required_main_suit_tiles
        || live_tile_count_for_suit(hand, table, main_suit) + (pending_claim as i32)
            < required_main_suit_tiles as i32
    {
        return 0.0;
    }
    if one_fan_reaches_score_cap(table) {
        return 0.0;
    }
    if capped_normal_route_visible_fan_exceeds_half_cap(hand, melds, table, position) {
        return 0.0;
    }
    if capped_normal_route_visible_fan_reaches_cap(hand, melds, table, position) {
        return 0.0;
    }
    if capped_open_normal_route_visible_fan_reaches_cap(hand, melds, table, position) {
        return 0.0;
    }
    let marginal_closed_plan_against_dealer_threat =
        valid_meld_count(melds) == 0 && dealer_opponent_has_major_threat(table, position);
    if table.dealer_position != position && !marginal_closed_plan_against_dealer_threat {
        return score;
    }
    pure_one_suit_shape(hand, melds)
        .filter(|(_, main_count, blockers)| *main_count >= 11 && *blockers <= 2)
        .map(|_| score)
        .unwrap_or(0.0)
}

pub(super) fn pure_one_suit_shape(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> Option<(i32, usize, usize)> {
    let all_tiles = hand
        .iter()
        .copied()
        .filter(|tile| is_valid_tile(*tile))
        .chain(valid_meld_tiles(melds))
        .collect::<Vec<_>>();
    let main_suit = dominant_pure_suit(hand, melds)?;
    let main_count = all_tiles
        .iter()
        .filter(|tile| is_suited(**tile) && tile_suit(**tile) == main_suit)
        .count();
    let blockers = all_tiles.len().saturating_sub(main_count);
    Some((main_suit, main_count, blockers))
}

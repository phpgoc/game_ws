use super::*;

pub(in crate::ai::decision) fn piao_committed_group_count(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> usize {
    let open_triplets = melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    open_triplets + counts.values().filter(|count| **count >= 3).count()
}

fn pending_piao_claim_tile(
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> Option<i32> {
    let claim = table.claim_window.as_ref()?;
    if claim.from_position == position
        || !claim.eligible_positions.contains(&position)
        || !claim_tile_already_visible(table, claim.tile)
    {
        return None;
    }
    let current_meld_tile_count = table
        .seats
        .get(&position)
        .map(|seat| {
            valid_meld_tiles(&seat.melds)
                .filter(|tile| *tile == claim.tile)
                .count()
        })
        .unwrap_or(0);
    let projected_meld_tile_count = valid_meld_tiles(melds)
        .filter(|tile| *tile == claim.tile)
        .count();
    (projected_meld_tile_count <= current_meld_tile_count).then_some(claim.tile)
}

fn piao_tile_acquisition_costs(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    pending_claim_tile: Option<i32>,
) -> (Option<usize>, Option<usize>, Option<usize>) {
    let own_count = hand.iter().filter(|item| **item == tile).count();
    let available =
        remaining_tile_count_with_melds_after_discards(hand, melds, table, position, tile, &[])
            as usize
            + usize::from(pending_claim_tile == Some(tile));
    let cost_for = |target_count: usize| {
        let required = target_count.saturating_sub(own_count);
        (required <= available).then_some(required)
    };
    let open_triplet_required = 1 + 2usize.saturating_sub(own_count);
    (
        cost_for(2),
        cost_for(3),
        (open_triplet_required <= available).then_some(open_triplet_required),
    )
}

fn minimum_acquisitions_for_piao_shape(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> Option<usize> {
    let meld_groups = piao_threat_level(melds);
    if meld_groups > 4 {
        return None;
    }
    let needed_triplets = 4 - meld_groups;
    let door_is_open = has_door_opening_meld(melds, table);
    if !door_is_open && needed_triplets == 0 {
        return None;
    }
    let pending_claim_tile = pending_piao_claim_tile(melds, table, position);
    let tile_costs = SHENYANG_MAHJONG_TILE_KINDS
        .into_iter()
        .map(|tile| {
            let (pair_cost, triplet_cost, open_triplet_cost) =
                piao_tile_acquisition_costs(hand, melds, table, position, tile, pending_claim_tile);
            (tile, pair_cost, triplet_cost, open_triplet_cost)
        })
        .collect::<Vec<_>>();
    let mut best = None;
    for (pair_tile, pair_cost, _, _) in &tile_costs {
        let Some(pair_cost) = pair_cost else {
            continue;
        };
        let mut triplet_costs = tile_costs
            .iter()
            .filter(|(tile, _, _, _)| tile != pair_tile)
            .filter_map(|(tile, _, triplet_cost, _)| triplet_cost.map(|cost| (*tile, cost)))
            .collect::<Vec<_>>();
        triplet_costs.sort_unstable_by_key(|(_, cost)| *cost);
        if door_is_open {
            if triplet_costs.len() < needed_triplets {
                continue;
            }
            let total = *pair_cost
                + triplet_costs
                    .iter()
                    .take(needed_triplets)
                    .map(|(_, cost)| *cost)
                    .sum::<usize>();
            best = Some(best.map_or(total, |current: usize| current.min(total)));
            continue;
        }

        for (open_tile, _, _, open_triplet_cost) in &tile_costs {
            if open_tile == pair_tile {
                continue;
            }
            let Some(open_triplet_cost) = open_triplet_cost else {
                continue;
            };
            let remaining_triplets = needed_triplets - 1;
            let mut normal_count = 0;
            let normal_cost = triplet_costs
                .iter()
                .filter(|(tile, _)| tile != open_tile)
                .take(remaining_triplets)
                .map(|(_, cost)| {
                    normal_count += 1;
                    *cost
                })
                .sum::<usize>();
            if normal_count < remaining_triplets {
                continue;
            }
            let follow_up_draw = usize::from(*pair_cost + normal_cost == 0);
            let total = *pair_cost + *open_triplet_cost + normal_cost + follow_up_draw;
            best = Some(best.map_or(total, |current: usize| current.min(total)));
        }
    }
    best
}

fn piao_plan_has_enough_group_opportunities(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
) -> bool {
    let pending_claim_opportunity =
        usize::from(pending_piao_claim_tile(melds, table, position).is_some());
    minimum_acquisitions_for_piao_shape(hand, melds, table, position).is_some_and(|required| {
        required <= table.wall_count.saturating_add(pending_claim_opportunity)
    })
}

pub(in crate::ai::decision) fn piao_plan_score(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> f64 {
    if melds.iter().any(is_sequence_meld) {
        return 0.0;
    }
    let open_triplets = melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count();
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &tile in hand.iter().filter(|tile| is_valid_tile(**tile)) {
        *counts.entry(tile).or_default() += 1;
    }
    let triplets = counts.values().filter(|count| **count >= 3).count();
    let pairs = counts.values().filter(|count| **count >= 2).count();
    let score = open_triplets as f64 * 18.0 + triplets as f64 * 14.0 + pairs as f64 * 5.0;
    if open_triplets + triplets >= 2 || pairs >= 3 || (open_triplets >= 1 && pairs >= 2) {
        score
    } else {
        0.0
    }
}

pub(in crate::ai::decision) fn piao_plan_score_for_context(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> f64 {
    let score = piao_plan_score(hand, melds);
    if score <= 0.0
        || !piao_plan_has_enough_group_opportunities(hand, melds, table, position)
        || piao_plan_is_capped(table)
        || !has_piao_route_basics(hand, melds)
        || capped_open_basic_route_visible_fan_reaches_cap(hand, melds, table)
        || capped_basic_route_foundation_visible_fan_exceeds_half_cap(
            hand,
            melds,
            table,
            WIN_RULE_SHENYANG_BASIC,
        )
        || capped_basic_route_foundation_visible_fan_reaches_cap(
            hand,
            melds,
            table,
            WIN_RULE_SHENYANG_BASIC,
        )
    {
        return 0.0;
    }
    let marginal_closed_plan_against_dealer_threat =
        valid_meld_count(melds) == 0 && dealer_opponent_has_major_threat(table, position, win_rule);
    if score < 40.0
        && (table.dealer_position == position || marginal_closed_plan_against_dealer_threat)
    {
        score * 0.35
    } else {
        score
    }
}

pub(in crate::ai::decision) fn piao_plan_is_capped(table: &AiPublicTable) -> bool {
    table.max_fan.is_some_and(|max_fan| max_fan <= 1)
}

pub(in crate::ai::decision) fn has_piao_route_basics(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
) -> bool {
    missing_suits(hand, melds).is_empty() && has_terminal_or_honor_with_extra(hand, melds, None)
}

pub(in crate::ai::decision) fn piao_threat_level(melds: &[WsShenyangMahjongMeld]) -> usize {
    if melds.iter().any(is_sequence_meld) {
        return 0;
    }
    melds
        .iter()
        .filter(|meld| is_triplet_like_meld(meld))
        .count()
}

pub(in crate::ai::decision) fn piao_missing_suits_from_melds(
    melds: &[WsShenyangMahjongMeld],
) -> Vec<i32> {
    if piao_threat_level(melds) < 3 {
        return Vec::new();
    }
    let mut suits = [false; 3];
    for meld in melds.iter().filter(|meld| is_triplet_like_meld(meld)) {
        if let Some(tile) = meld_primary_tile(meld)
            && is_suited(tile)
        {
            suits[tile_suit(tile) as usize] = true;
        }
    }
    suits
        .into_iter()
        .enumerate()
        .filter_map(|(suit, present)| (!present).then_some(suit as i32))
        .collect()
}

pub(in crate::ai::decision) fn is_closed_early_piao_candidate(
    hand: &[i32],
    melds: &[WsShenyangMahjongMeld],
    table: &AiPublicTable,
    position: usize,
    win_rule: i32,
) -> bool {
    valid_meld_count(melds) == 0
        && pair_count(hand) >= 3
        && piao_plan_has_enough_group_opportunities(hand, melds, table, position)
        && table.dealer_position != position
        && !dealer_opponent_has_major_threat(table, position, win_rule)
        && !piao_plan_is_capped(table)
        && !capped_basic_route_foundation_visible_fan_exceeds_half_cap(
            hand,
            melds,
            table,
            WIN_RULE_SHENYANG_BASIC,
        )
        && !capped_basic_route_foundation_visible_fan_reaches_cap(
            hand,
            melds,
            table,
            WIN_RULE_SHENYANG_BASIC,
        )
        && has_piao_route_basics(hand, melds)
}

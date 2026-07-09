use super::*;

pub(in crate::ai::decision) fn pure_one_suit_threat_discard_bias(
    table: &AiPublicTable,
    position: usize,
    tile: i32,
    own_tile_count: usize,
) -> f64 {
    if table.wall_count > 52 || !is_suited(tile) || public_discard_count(table, tile) > 0 {
        return 0.0;
    }
    if pure_one_suit_threat_tile_fully_accounted(table, tile, own_tile_count) {
        return 0.0;
    }
    let suit = tile_suit(tile);
    table
        .seats
        .iter()
        .filter(|(seat_position, _)| **seat_position != position)
        .filter_map(|(_, seat)| {
            let (threat_suit, open_melds) = pure_one_suit_threat_suit(seat)?;
            (threat_suit == suit && !seat_has_open_meld_tile(seat, tile)).then_some((
                seat,
                open_melds,
                threat_suit,
            ))
        })
        .map(|(seat, open_melds, threat_suit)| {
            let base = if tile_is_terminal(tile) { 7.0 } else { 10.0 };
            let pair_penalty = if own_tile_count >= 2 {
                if tile_is_terminal(tile) { 5.0 } else { 7.0 }
            } else {
                0.0
            };
            let meld_pressure = pure_one_suit_threat_meld_pressure(open_melds);
            let late_pressure = if table.wall_count <= 20 {
                1.35
            } else if table.wall_count <= 42 {
                1.15
            } else {
                1.0
            };
            let hand_pressure = if seat.hand_count <= 4 {
                1.3
            } else if seat.hand_count <= 7 {
                1.15
            } else {
                1.0
            };
            let exposed_discount = (exposed_meld_tile_count(table, tile) as f64 * 4.0).min(8.0);
            let discard_scale = pure_one_suit_threat_discard_scale(seat, threat_suit);
            -((base + pair_penalty) * meld_pressure * late_pressure * hand_pressure * discard_scale
                - exposed_discount)
                .max(2.0)
        })
        .sum()
}

pub(in crate::ai::decision) fn pure_one_suit_threat_discard_scale(
    seat: &AiSeatView,
    threat_suit: i32,
) -> f64 {
    let same_suit_discards = valid_discards(seat)
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == threat_suit)
        .count();
    match same_suit_discards {
        0 => {
            let off_suit_discards = valid_discards(seat)
                .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
                .count();
            if off_suit_discards >= 4 {
                1.25
            } else if off_suit_discards >= 2 {
                1.1
            } else {
                1.0
            }
        }
        1 => 0.7,
        2 => 0.45,
        _ => 0.25,
    }
}

pub(in crate::ai::decision) fn pure_one_suit_threat_meld_pressure(open_melds: usize) -> f64 {
    if open_melds <= 1 {
        0.55
    } else {
        (open_melds as f64 - 1.0).min(2.0)
    }
}

pub(in crate::ai::decision) fn pure_one_suit_threat_tile_fully_accounted(
    table: &AiPublicTable,
    tile: i32,
    own_tile_count: usize,
) -> bool {
    exposed_meld_tile_count(table, tile) + public_discard_count(table, tile) + own_tile_count >= 4
}

pub(in crate::ai::decision) fn pure_one_suit_threat_suit(
    seat: &AiSeatView,
) -> Option<(i32, usize)> {
    let mut open_meld_count = 0usize;
    let mut threat_suit = None;
    for meld in seat.melds.iter().filter(|meld| is_open_meld(meld)) {
        open_meld_count += 1;
        for tile in meld.tiles.iter().copied() {
            if !is_suited(tile) {
                return None;
            }
            let suit = tile_suit(tile);
            match threat_suit {
                Some(current) if current != suit => return None,
                Some(_) => {}
                None => threat_suit = Some(suit),
            }
        }
    }
    if open_meld_count == 0 {
        return pure_one_suit_closed_discard_threat_suit(seat).map(|suit| (suit, 0));
    }
    threat_suit.and_then(|suit| {
        (open_meld_count >= 2 || pure_one_suit_single_meld_discard_evidence(seat, suit))
            .then_some((suit, open_meld_count))
    })
}

pub(in crate::ai::decision) fn pure_one_suit_closed_discard_threat_suit(
    seat: &AiSeatView,
) -> Option<i32> {
    if has_open_meld(&seat.melds) || valid_discards(seat).count() < 5 {
        return None;
    }

    let mut suit_discards = [0usize; 3];
    for discard in valid_discards(seat)
        .copied()
        .filter(|tile| is_suited(*tile))
    {
        suit_discards[tile_suit(discard) as usize] += 1;
    }
    let untouched_suits = suit_discards
        .iter()
        .enumerate()
        .filter_map(|(suit, count)| (*count == 0).then_some(suit as i32))
        .collect::<Vec<_>>();
    if untouched_suits.len() != 1 {
        return None;
    }
    let threat_suit = untouched_suits[0];
    let off_suit_discards = valid_discards(seat)
        .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
        .count();
    (off_suit_discards >= 5).then_some(threat_suit)
}

pub(in crate::ai::decision) fn pure_one_suit_single_meld_discard_evidence(
    seat: &AiSeatView,
    threat_suit: i32,
) -> bool {
    let same_suit_discards = valid_discards(seat)
        .filter(|discard| is_suited(**discard) && tile_suit(**discard) == threat_suit)
        .count();
    let off_suit_discards = valid_discards(seat)
        .filter(|discard| !is_suited(**discard) || tile_suit(**discard) != threat_suit)
        .count();
    same_suit_discards == 0 && off_suit_discards >= 4
}

fn valid_discards(seat: &AiSeatView) -> impl Iterator<Item = &i32> {
    seat.discards
        .iter()
        .filter(|discard| is_valid_tile(**discard))
}

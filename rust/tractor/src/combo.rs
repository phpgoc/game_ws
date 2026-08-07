//! Card-combination logic for tractor (拖拉机 / 升级).
//!
//! The trump group is made of every card of the current target rank plus both
//! jokers; all other cards are "plain" and belong to their natural suit. A legal
//! play is a single group of one of six shapes:
//!   - Single: one card.
//!   - Pair:   two identical cards (same base card, regardless of deck copy).
//!   - Triple: three identical cards in a three-deck game (三同张).
//!   - Tractor: two or more consecutive pairs in the same group (连对).
//!   - Titanic: two or more consecutive triples in a three-deck game (连三张).
//!   - Throw: multiple same-group components released together (甩牌).
//!
//! Pairs are matched by card *identity* (base card), never by rank alone, so
//! `5♠ + 5♥` is two singles, not a pair.

use std::collections::HashMap;

use share_type_public::WsTractorPlayedCards;

use crate::game_state::{
    TractorRules, base_card, card_rank, card_score, card_suit, is_trump_card, tractor_card_value,
};

#[derive(Debug, Clone, Copy)]
pub struct Combo {
    pub kind: ComboKind,
    /// `None` when the combo is trump, otherwise the plain suit index.
    pub suit: Option<i32>,
    /// Structural resources carried by this play. Throw competition uses the
    /// lead's requirements, so stronger structure may be broken down but
    /// weaker structure cannot ruff it.
    pub pair_count: usize,
    pub tractor_pair_count: usize,
    pub triple_count: usize,
    pub titanic_triple_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboKind {
    Single,
    Pair,
    Triple,
    /// A run of `n` consecutive pairs (n >= 2), so `2 * n` cards.
    Tractor(usize),
    /// A run of `n` consecutive triples (n >= 2), so `3 * n` cards.
    Titanic(usize),
    /// A same-group composite lead. `pairs` records the minimum pair structure
    /// followers must preserve when they have it.
    Throw {
        cards: usize,
        pairs: usize,
    },
}

fn capped_choose(n: usize, k: usize, cap: usize) -> usize {
    if k > n {
        return cap.saturating_add(1);
    }
    let k = k.min(n - k);
    let mut result = 1usize;
    for index in 0..k {
        result = result
            .saturating_mul(n - index)
            .checked_div(index + 1)
            .unwrap_or(cap.saturating_add(1));
        if result > cap {
            return cap.saturating_add(1);
        }
    }
    result
}

/// Whether `card` belongs to the group implied by `lead_suit`
/// (`None` => trump group, `Some(suit)` => that plain suit).
pub fn card_in_group(card: i32, lead_suit: Option<i32>, rules: &TractorRules) -> bool {
    match lead_suit {
        None => is_trump_card(card, rules),
        Some(suit) => !is_trump_card(card, rules) && card_suit(card) == Some(suit),
    }
}

/// Classify `cards` as a legal combo shape, or `None` if it is not a single
/// group of the same suit forming a single / pair / tractor.
pub fn classify(cards: &[i32], rules: &TractorRules) -> Option<Combo> {
    if cards.is_empty() {
        return None;
    }
    let trump = is_trump_card(cards[0], rules);
    // Every card must sit in the same group (all trump, or all one plain suit).
    let suit = if trump {
        if !cards.iter().all(|card| is_trump_card(*card, rules)) {
            return None;
        }
        None
    } else {
        let suit = card_suit(cards[0])?;
        if !cards
            .iter()
            .all(|card| !is_trump_card(*card, rules) && card_suit(*card) == Some(suit))
        {
            return None;
        }
        Some(suit)
    };

    if cards.len() == 1 {
        return Some(Combo {
            kind: ComboKind::Single,
            suit,
            pair_count: 0,
            tractor_pair_count: 0,
            triple_count: 0,
            titanic_triple_count: 0,
        });
    }

    let counts = identity_counts(cards);
    let pair_count = counts.values().map(|count| count / 2).sum();
    let triple_count = counts.values().map(|count| count / 3).sum();
    let tractor_pair_count = multiplicity_run_unit_count(cards, 2, rules);
    let titanic_triple_count = multiplicity_run_unit_count(cards, 3, rules);
    if cards.len() == 2 && counts.len() == 1 {
        return Some(Combo {
            kind: ComboKind::Pair,
            suit,
            pair_count,
            tractor_pair_count,
            triple_count,
            titanic_triple_count,
        });
    }

    if rules.deck_count >= 3 && cards.len() == 3 && counts.len() == 1 {
        return Some(Combo {
            kind: ComboKind::Triple,
            suit,
            pair_count,
            tractor_pair_count,
            triple_count,
            titanic_triple_count,
        });
    }

    if counts.values().all(|count| *count == 2) {
        let mut positions: Vec<i32> = counts
            .keys()
            .map(|base| pair_position(*base, rules))
            .collect();
        positions.sort_unstable();
        // Distinct, strictly consecutive pair positions => tractor.
        if positions.windows(2).all(|w| w[1] == w[0] + 1) {
            return Some(Combo {
                kind: ComboKind::Tractor(positions.len()),
                suit,
                pair_count,
                tractor_pair_count,
                triple_count,
                titanic_triple_count,
            });
        }
    }

    if rules.deck_count >= 3 && counts.values().all(|count| *count == 3) {
        let mut positions: Vec<i32> = counts
            .keys()
            .map(|base| pair_position(*base, rules))
            .collect();
        positions.sort_unstable();
        if positions.len() >= 2
            && positions
                .windows(2)
                .all(|window| window[1] == window[0] + 1)
        {
            return Some(Combo {
                kind: ComboKind::Titanic(positions.len()),
                suit,
                pair_count,
                tractor_pair_count,
                triple_count,
                titanic_triple_count,
            });
        }
    }

    Some(Combo {
        kind: ComboKind::Throw {
            cards: cards.len(),
            pairs: counts.values().map(|count| count / 2).sum(),
        },
        suit,
        pair_count,
        tractor_pair_count,
        triple_count,
        titanic_triple_count,
    })
}

#[cfg(test)]
fn combinations(cards: &[i32], count: usize) -> Vec<Vec<i32>> {
    let mut out = Vec::new();
    for_each_combination(cards, count, |current| {
        out.push(current.to_vec());
        true
    });
    out
}

fn for_each_combination(
    cards: &[i32],
    count: usize,
    mut callback: impl FnMut(&[i32]) -> bool,
) -> bool {
    fn visit(
        cards: &[i32],
        count: usize,
        start: usize,
        current: &mut Vec<i32>,
        callback: &mut impl FnMut(&[i32]) -> bool,
    ) -> bool {
        if current.len() == count {
            return callback(current);
        }
        let needed = count - current.len();
        if cards.len().saturating_sub(start) < needed {
            return true;
        }
        for index in start..=cards.len() - needed {
            current.push(cards[index]);
            if !visit(cards, count, index + 1, current, callback) {
                current.pop();
                return false;
            }
            current.pop();
        }
        true
    }

    if count > cards.len() {
        return true;
    }
    visit(
        cards,
        count,
        0,
        &mut Vec::with_capacity(count),
        &mut callback,
    )
}

/// Ranking value of a played combo *if* it can beat the current lead, else
/// `None`. Higher wins. A play only competes when it matches the lead shape and
/// is either trump or the exact lead plain suit.
pub fn combo_win_value(cards: &[i32], lead: &Combo, rules: &TractorRules) -> Option<i32> {
    let combo = classify(cards, rules)?;
    let shape_matches = match lead.kind {
        ComboKind::Throw {
            cards: required_cards,
            ..
        } => {
            combo.kind.card_count() == required_cards
                && combo.pair_count >= lead.pair_count
                && combo.tractor_pair_count >= lead.tractor_pair_count
                && combo.triple_count >= lead.triple_count
                && combo.titanic_triple_count >= lead.titanic_triple_count
        }
        _ => combo.kind == lead.kind,
    };
    if !shape_matches {
        return None;
    }
    match combo.suit {
        None => {} // trump always competes
        // A plain follow only competes when it repeats the lead's plain suit.
        Some(suit) if lead.suit == Some(suit) => {}
        Some(_) => return None,
    }
    let counts = identity_counts(cards);
    let structured_bases = if lead.titanic_triple_count > 0 {
        multiplicity_run_bases(cards, 3, rules)
    } else if lead.tractor_pair_count > 0 {
        multiplicity_run_bases(cards, 2, rules)
    } else if lead.triple_count > 0 {
        counts
            .iter()
            .filter(|(_, count)| **count >= 3)
            .map(|(base, _)| *base)
            .collect()
    } else if lead.pair_count > 0 {
        counts
            .iter()
            .filter(|(_, count)| **count >= 2)
            .map(|(base, _)| *base)
            .collect()
    } else {
        Vec::new()
    };
    cards
        .iter()
        .filter(|card| structured_bases.is_empty() || structured_bases.contains(&base_card(**card)))
        .map(|card| tractor_card_value(*card, rules, lead.suit))
        .max()
}

fn multiplicity_run_bases(cards: &[i32], copies: usize, rules: &TractorRules) -> Vec<i32> {
    let qualified = identity_counts(cards)
        .into_iter()
        .filter(|(_, count)| *count >= copies)
        .map(|(base, _)| (pair_position(base, rules), base))
        .collect::<Vec<_>>();
    qualified
        .iter()
        .filter(|(position, _)| {
            qualified.iter().any(|(other, _)| {
                *other == position.saturating_sub(1) || *other == position.saturating_add(1)
            })
        })
        .map(|(_, base)| *base)
        .collect()
}

/// Number of full identity-pairs available in `cards` for the given group.
pub fn count_group_pairs(cards: &[i32], lead_suit: Option<i32>, rules: &TractorRules) -> usize {
    let group: Vec<i32> = cards
        .iter()
        .copied()
        .filter(|card| card_in_group(*card, lead_suit, rules))
        .collect();
    identity_counts(&group)
        .values()
        .map(|count| count / 2)
        .sum()
}

pub fn count_group_triples(cards: &[i32], lead_suit: Option<i32>, rules: &TractorRules) -> usize {
    let group: Vec<i32> = cards
        .iter()
        .copied()
        .filter(|card| card_in_group(*card, lead_suit, rules))
        .collect();
    identity_counts(&group)
        .values()
        .map(|count| count / 3)
        .sum()
}

fn multiplicity_run_unit_count(cards: &[i32], copies: usize, rules: &TractorRules) -> usize {
    let mut positions = identity_counts(cards)
        .into_iter()
        .filter(|(_, count)| *count >= copies)
        .map(|(base, _)| pair_position(base, rules))
        .collect::<Vec<_>>();
    positions.sort_unstable();
    positions.dedup();
    let mut total = 0;
    let mut current = 0;
    let mut previous = None;
    for position in positions {
        if previous.is_some_and(|value| position == value + 1) {
            current += 1;
        } else {
            if current >= 2 {
                total += current;
            }
            current = 1;
        }
        previous = Some(position);
    }
    if current >= 2 {
        total += current;
    }
    total
}

fn longest_multiplicity_run(
    cards: &[i32],
    lead_suit: Option<i32>,
    copies: usize,
    rules: &TractorRules,
) -> usize {
    let group = cards
        .iter()
        .copied()
        .filter(|card| card_in_group(*card, lead_suit, rules))
        .collect::<Vec<_>>();
    let mut positions = identity_counts(&group)
        .into_iter()
        .filter(|(_, count)| *count >= copies)
        .map(|(base, _)| pair_position(base, rules))
        .collect::<Vec<_>>();
    positions.sort_unstable();
    positions.dedup();
    let mut longest = 0;
    let mut current = 0;
    let mut previous = None;
    for position in positions {
        current = if previous.is_some_and(|value| position == value + 1) {
            current + 1
        } else {
            1
        };
        longest = longest.max(current);
        previous = Some(position);
    }
    longest
}

fn titanic_follow_priority(cards: &[i32], lead_suit: Option<i32>, rules: &TractorRules) -> u8 {
    let triple_run = longest_multiplicity_run(cards, lead_suit, 3, rules);
    if triple_run >= 2 {
        return 7;
    }
    let pair_run = longest_multiplicity_run(cards, lead_suit, 2, rules);
    if pair_run >= 2 {
        return 6;
    }
    let triples = count_group_triples(cards, lead_suit, rules);
    let pairs = count_group_pairs(cards, lead_suit, rules);
    if triples >= 2 {
        5
    } else if triples >= 1 && pairs >= 2 {
        4
    } else if triples >= 1 {
        3
    } else if pairs >= 2 {
        2
    } else if pairs >= 1 {
        1
    } else {
        0
    }
}

/// Enumerate strategically distinct legal replies to a lead. Same-shape
/// winners come from [`enumerate_leads`]; when a player cannot reproduce the
/// shape, bounded subset enumeration also exposes alternative legal discards
/// (for example avoiding a five while following a pair with two singles).
pub fn enumerate_follows(hand: &[i32], lead: &Combo, rules: &TractorRules) -> Vec<Vec<i32>> {
    const SUBSET_LIMIT: usize = 4_096;

    let mut out = Vec::new();
    if let Some(cards) = forced_follow(hand, lead, rules)
        && follow_is_legal(hand, &cards, lead, rules)
    {
        out.push(cards);
    }
    for cards in enumerate_leads(hand, rules) {
        if classify(&cards, rules).map(|combo| combo.kind) == Some(lead.kind)
            && follow_is_legal(hand, &cards, lead, rules)
            && !out.contains(&cards)
        {
            out.push(cards);
        }
    }

    let lead_len = lead.kind.card_count();
    let group: Vec<_> = hand
        .iter()
        .copied()
        .filter(|card| card_in_group(*card, lead.suit, rules))
        .collect();
    let outside: Vec<_> = hand
        .iter()
        .copied()
        .filter(|card| !card_in_group(*card, lead.suit, rules))
        .collect();
    let group_count = group.len().min(lead_len);
    let outside_count = lead_len - group_count;
    let subset_count = capped_choose(group.len(), group_count, SUBSET_LIMIT)
        .saturating_mul(capped_choose(outside.len(), outside_count, SUBSET_LIMIT));
    if subset_count <= SUBSET_LIMIT {
        for_each_combination(&group, group_count, |group_cards| {
            for_each_combination(&outside, outside_count, |outside_cards| {
                let mut cards = Vec::with_capacity(lead_len);
                cards.extend_from_slice(group_cards);
                cards.extend_from_slice(outside_cards);
                if follow_is_legal(hand, &cards, lead, rules) && !out.contains(&cards) {
                    out.push(cards);
                }
                true
            });
            true
        });
    }
    out
}

/// Enumerate legal lead plays (singles, pairs, tractors) from a hand.
pub fn enumerate_leads(hand: &[i32], rules: &TractorRules) -> Vec<Vec<i32>> {
    let mut out: Vec<Vec<i32>> = hand.iter().map(|card| vec![*card]).collect();

    // Group cards by (group, base) so pairs use identical cards.
    let mut groups: HashMap<Option<i32>, HashMap<i32, Vec<i32>>> = HashMap::new();
    for card in hand {
        let group = if is_trump_card(*card, rules) {
            None
        } else {
            card_suit(*card)
        };
        groups
            .entry(group)
            .or_default()
            .entry(base_card(*card))
            .or_default()
            .push(*card);
    }

    for (group, by_base) in &groups {
        // Pairs.
        // A three- or four-deck table can hold more than one pair of the same
        // identity. Keep every disjoint pair, plus the odd leftover singleton,
        // so the AI can consider legal pair/single and multi-pair throws.
        let mut pairs: Vec<(i32, Vec<i32>)> = by_base
            .iter()
            .flat_map(|(base, cards)| {
                cards
                    .chunks_exact(2)
                    .map(move |pair| (*base, pair.to_vec()))
            })
            .collect();
        pairs.sort_by_key(|(base, pair)| (pair_position(*base, rules), *base, pair[0]));
        // Extra copies cannot extend a tractor at the same rank. Retain one
        // representative pair per identity for ordinary mid-hand tractors and
        // throws, then expose duplicate pairs only for short-hand exits.
        let mut primary_pairs: Vec<(i32, Vec<i32>)> = Vec::new();
        for (base, pair) in &pairs {
            if primary_pairs
                .last()
                .is_none_or(|(previous_base, _)| previous_base != base)
            {
                primary_pairs.push((*base, pair.clone()));
            }
        }
        let mut singles: Vec<i32> = by_base
            .values()
            .flat_map(|cards| cards.chunks_exact(2).remainder().iter().copied())
            .collect();
        singles.sort_unstable();
        for (_, pair) in &pairs {
            out.push(pair.clone());
        }
        // Tractors: consecutive pair positions.
        let positions: Vec<i32> = primary_pairs
            .iter()
            .map(|(base, _)| pair_position(*base, rules))
            .collect();
        let mut start = 0;
        while start < primary_pairs.len() {
            let mut end = start;
            while end + 1 < primary_pairs.len() && positions[end + 1] == positions[end] + 1 {
                end += 1;
            }
            if end > start {
                // Every sub-run of length >= 2 within this maximal run.
                for from in start..end {
                    for to in (from + 1)..=end {
                        let mut cards = Vec::new();
                        for (_, pair) in &primary_pairs[from..=to] {
                            cards.extend_from_slice(pair);
                        }
                        out.push(cards);
                    }
                }
            }
            start = end + 1;
        }
        if rules.deck_count >= 3 {
            let mut triples: Vec<(i32, Vec<i32>)> = by_base
                .iter()
                .filter(|(_, cards)| cards.len() >= 3)
                .map(|(base, cards)| (*base, cards[..3].to_vec()))
                .collect();
            triples.sort_by_key(|(base, triple)| (pair_position(*base, rules), *base, triple[0]));
            for (_, triple) in &triples {
                if !out.contains(triple) {
                    out.push(triple.clone());
                }
            }
            let positions: Vec<i32> = triples
                .iter()
                .map(|(base, _)| pair_position(*base, rules))
                .collect();
            let mut start = 0;
            while start < triples.len() {
                let mut end = start;
                while end + 1 < triples.len() && positions[end + 1] == positions[end] + 1 {
                    end += 1;
                }
                if end > start {
                    for from in start..end {
                        for to in (from + 1)..=end {
                            let mut cards = Vec::new();
                            for (_, triple) in &triples[from..=to] {
                                cards.extend_from_slice(triple);
                            }
                            out.push(cards);
                        }
                    }
                }
                start = end + 1;
            }
        }
        let _ = group;

        // Useful throw candidates are kept deliberately bounded. Pair/single
        // and duplicate-pair throws become relevant only in a short-hand exit;
        // a bare two-single throw stays out so normal low-single probing wins.
        let throw_pairs = if hand.len() <= 8 {
            &pairs
        } else {
            &primary_pairs
        };
        if hand.len() <= 8 {
            for (_, pair) in throw_pairs {
                for single in &singles {
                    let mut cards = pair.clone();
                    cards.push(*single);
                    if matches!(
                        classify(&cards, rules).map(|combo| combo.kind),
                        Some(ComboKind::Throw { .. })
                    ) && !out.contains(&cards)
                    {
                        out.push(cards);
                    }
                }
            }
        }
        if throw_pairs.len() >= 2 {
            for left in 0..throw_pairs.len() {
                for right in (left + 1)..throw_pairs.len() {
                    let mut cards = throw_pairs[left].1.clone();
                    cards.extend_from_slice(&throw_pairs[right].1);
                    if matches!(
                        classify(&cards, rules).map(|combo| combo.kind),
                        Some(ComboKind::Throw { .. })
                    ) {
                        out.push(cards);
                    }
                }
            }
            let mut all_pairs = Vec::new();
            for (_, pair) in throw_pairs {
                all_pairs.extend_from_slice(pair);
            }
            if matches!(
                classify(&all_pairs, rules).map(|combo| combo.kind),
                Some(ComboKind::Throw { .. })
            ) && !out.contains(&all_pairs)
            {
                out.push(all_pairs);
            }
        }
    }
    out
}

fn fill_from(
    chosen: &mut Vec<i32>,
    remaining: &mut Vec<i32>,
    target_len: usize,
    accept: impl Fn(i32) -> bool,
) {
    let mut idx = 0;
    while chosen.len() < target_len && idx < remaining.len() {
        if accept(remaining[idx]) {
            chosen.push(remaining.remove(idx));
        } else {
            idx += 1;
        }
    }
}

/// Validate a follow against the established lead, given the full hand.
///
/// Rules enforced:
///   - same card count as the lead;
///   - the cards actually exist in the hand;
///   - the player uses as many cards of the lead group as they hold (up to the
///     lead length): if they can fully follow suit they must;
///   - if the lead is a pair/tractor and the hand still holds pairs of the lead
///     group, the follow must include as many pairs as required/available.
pub fn follow_is_legal(hand: &[i32], cards: &[i32], lead: &Combo, rules: &TractorRules) -> bool {
    let lead_len = lead.kind.card_count();
    if cards.len() != lead_len || !hand_contains(hand, cards) {
        return false;
    }
    let lead_suit = lead.suit;

    let group_in_hand = hand
        .iter()
        .filter(|card| card_in_group(**card, lead_suit, rules))
        .count();
    let group_in_play = cards
        .iter()
        .filter(|card| card_in_group(**card, lead_suit, rules))
        .count();
    let required_group = group_in_hand.min(lead_len);
    if group_in_play < required_group {
        return false;
    }

    // Pair preservation: when the lead needs pairs, honour available group pairs.
    let required_pairs = match lead.kind {
        ComboKind::Single => 0,
        ComboKind::Pair => 1,
        ComboKind::Triple => 1,
        ComboKind::Tractor(n) => n,
        ComboKind::Titanic(_) => 0,
        ComboKind::Throw { pairs, .. } => pairs,
    };
    if required_pairs > 0 {
        let pairs_in_hand = count_group_pairs(hand, lead_suit, rules);
        let must_use_pairs = required_pairs.min(pairs_in_hand);
        if must_use_pairs > 0 {
            let group_cards: Vec<i32> = cards
                .iter()
                .copied()
                .filter(|card| card_in_group(*card, lead_suit, rules))
                .collect();
            let pairs_in_play = identity_counts(&group_cards)
                .values()
                .map(|count| count / 2)
                .sum::<usize>();
            if pairs_in_play < must_use_pairs {
                return false;
            }
        }
    }

    match lead.kind {
        ComboKind::Triple
            if count_group_triples(hand, lead_suit, rules) > 0
                && count_group_triples(cards, lead_suit, rules) == 0 =>
        {
            return false;
        }
        ComboKind::Tractor(_)
            if longest_multiplicity_run(hand, lead_suit, 2, rules) >= 2
                && longest_multiplicity_run(cards, lead_suit, 2, rules) < 2 =>
        {
            return false;
        }
        ComboKind::Titanic(_)
            if titanic_follow_priority(cards, lead_suit, rules)
                < titanic_follow_priority(hand, lead_suit, rules) =>
        {
            return false;
        }
        _ => {}
    }
    true
}

fn multiplicity_units(
    cards: &[i32],
    lead_suit: Option<i32>,
    copies: usize,
    rules: &TractorRules,
) -> Vec<(i32, i32, Vec<i32>)> {
    let mut by_base: HashMap<i32, Vec<i32>> = HashMap::new();
    for card in cards
        .iter()
        .copied()
        .filter(|card| card_in_group(*card, lead_suit, rules))
    {
        by_base.entry(base_card(card)).or_default().push(card);
    }
    let mut units = by_base
        .into_iter()
        .filter_map(|(base, mut cards)| {
            cards.sort_unstable();
            (cards.len() >= copies).then(|| {
                (
                    pair_position(base, rules),
                    base,
                    cards.into_iter().take(copies).collect(),
                )
            })
        })
        .collect::<Vec<_>>();
    units.sort_by_key(|(position, base, _)| (*position, *base));
    units
}

fn take_units(
    chosen: &mut Vec<i32>,
    remaining: &mut Vec<i32>,
    units: impl IntoIterator<Item = Vec<i32>>,
    target_len: usize,
) {
    for unit in units {
        if chosen.len() + unit.len() > target_len {
            break;
        }
        for card in unit {
            take_card(remaining, card);
            chosen.push(card);
        }
    }
}

fn take_lowest_multiplicity_units(
    chosen: &mut Vec<i32>,
    remaining: &mut Vec<i32>,
    lead_suit: Option<i32>,
    copies: usize,
    count: usize,
    target_len: usize,
    rules: &TractorRules,
) {
    let units = multiplicity_units(remaining, lead_suit, copies, rules)
        .into_iter()
        .take(count)
        .map(|(_, _, cards)| cards);
    take_units(chosen, remaining, units, target_len);
}

fn take_best_consecutive_units(
    chosen: &mut Vec<i32>,
    remaining: &mut Vec<i32>,
    lead_suit: Option<i32>,
    copies: usize,
    max_units: usize,
    target_len: usize,
    rules: &TractorRules,
) {
    if max_units < 2 {
        return;
    }
    let units = multiplicity_units(remaining, lead_suit, copies, rules);
    let mut best = Vec::new();
    let mut current = Vec::new();
    let mut previous = None;
    for (position, _, cards) in units {
        if previous.is_some_and(|value| position == value + 1) {
            current.push(cards);
        } else {
            if current.len() > best.len() {
                best = current;
            }
            current = vec![cards];
        }
        previous = Some(position);
    }
    if current.len() > best.len() {
        best = current;
    }
    if best.len() >= 2 {
        best.truncate(max_units);
        take_units(chosen, remaining, best, target_len);
    }
}

/// Build one guaranteed-legal follow to `lead`, preferring the lowest cards and
/// honouring group / pair-preservation rules. Returns `None` only if the hand
/// cannot supply enough cards (should not happen at a live table).
pub fn forced_follow(hand: &[i32], lead: &Combo, rules: &TractorRules) -> Option<Vec<i32>> {
    let lead_len = lead.kind.card_count();
    if hand.len() < lead_len {
        return None;
    }
    let lead_suit = lead.suit;
    let value = |card: &i32| tractor_card_value(*card, rules, lead_suit);

    let mut chosen: Vec<i32> = Vec::with_capacity(lead_len);
    let mut remaining: Vec<i32> = hand.to_vec();
    remaining.sort_by_key(&value);

    match lead.kind {
        ComboKind::Triple => take_lowest_multiplicity_units(
            &mut chosen,
            &mut remaining,
            lead_suit,
            3,
            1,
            lead_len,
            rules,
        ),
        ComboKind::Tractor(pair_count) => take_best_consecutive_units(
            &mut chosen,
            &mut remaining,
            lead_suit,
            2,
            pair_count,
            lead_len,
            rules,
        ),
        ComboKind::Titanic(triple_count) => {
            match titanic_follow_priority(&remaining, lead_suit, rules) {
                7 => take_best_consecutive_units(
                    &mut chosen,
                    &mut remaining,
                    lead_suit,
                    3,
                    triple_count,
                    lead_len,
                    rules,
                ),
                6 => take_best_consecutive_units(
                    &mut chosen,
                    &mut remaining,
                    lead_suit,
                    2,
                    lead_len / 2,
                    lead_len,
                    rules,
                ),
                5 => take_lowest_multiplicity_units(
                    &mut chosen,
                    &mut remaining,
                    lead_suit,
                    3,
                    triple_count,
                    lead_len,
                    rules,
                ),
                4 => {
                    take_lowest_multiplicity_units(
                        &mut chosen,
                        &mut remaining,
                        lead_suit,
                        3,
                        1,
                        lead_len,
                        rules,
                    );
                    take_lowest_multiplicity_units(
                        &mut chosen,
                        &mut remaining,
                        lead_suit,
                        2,
                        1,
                        lead_len,
                        rules,
                    );
                }
                3 => take_lowest_multiplicity_units(
                    &mut chosen,
                    &mut remaining,
                    lead_suit,
                    3,
                    1,
                    lead_len,
                    rules,
                ),
                2 => take_lowest_multiplicity_units(
                    &mut chosen,
                    &mut remaining,
                    lead_suit,
                    2,
                    2,
                    lead_len,
                    rules,
                ),
                1 => take_lowest_multiplicity_units(
                    &mut chosen,
                    &mut remaining,
                    lead_suit,
                    2,
                    1,
                    lead_len,
                    rules,
                ),
                _ => {}
            }
        }
        _ => {}
    }

    // 1. Satisfy required pairs from the lead group, lowest first.
    let required_pairs = match lead.kind {
        ComboKind::Single => 0,
        ComboKind::Pair => 1,
        ComboKind::Triple => 1,
        ComboKind::Tractor(n) => n,
        ComboKind::Titanic(_) => 0,
        ComboKind::Throw { pairs, .. } => pairs,
    };
    let already_chosen_pairs = count_group_pairs(&chosen, lead_suit, rules);
    let remaining_required_pairs = required_pairs.saturating_sub(already_chosen_pairs);
    let mut group_pairs: Vec<Vec<i32>> = {
        let group: Vec<i32> = remaining
            .iter()
            .copied()
            .filter(|card| card_in_group(*card, lead_suit, rules))
            .collect();
        let mut by_base: HashMap<i32, Vec<i32>> = HashMap::new();
        for card in group {
            by_base.entry(base_card(card)).or_default().push(card);
        }
        by_base
            .into_values()
            .flat_map(|cards| {
                cards
                    .chunks_exact(2)
                    .map(|pair| pair.to_vec())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    group_pairs.sort_by_key(|pair| pair.iter().map(&value).max().unwrap_or(0));
    for pair in group_pairs.into_iter().take(remaining_required_pairs) {
        for card in pair {
            if chosen.len() < lead_len {
                take_card(&mut remaining, card);
                chosen.push(card);
            }
        }
    }

    // 2. Fill remaining slots with lowest group singles.
    fill_from(&mut chosen, &mut remaining, lead_len, |card| {
        card_in_group(card, lead_suit, rules)
    });
    // 3. Fill any leftover with the lowest cards outside the group.
    fill_from(&mut chosen, &mut remaining, lead_len, |_| true);

    (chosen.len() == lead_len).then_some(chosen)
}

/// Whether `hand` can supply `cards` (multiset containment).
fn hand_contains(hand: &[i32], cards: &[i32]) -> bool {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for card in hand {
        *counts.entry(*card).or_default() += 1;
    }
    for card in cards {
        let slot = counts.entry(*card).or_default();
        if *slot == 0 {
            return false;
        }
        *slot -= 1;
    }
    true
}

/// Group cards by base card, returning `base -> count`.
fn identity_counts(cards: &[i32]) -> HashMap<i32, usize> {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for card in cards {
        *counts.entry(base_card(*card)).or_default() += 1;
    }
    counts
}

/// Position of a pair (identified by base card) within its group's ordering, used
/// to decide tractor consecutiveness. Only meaningful within a single group.
///
/// Plain suits are ordered by rank with the trump rank squeezed out so the two
/// ranks bordering the trump rank are consecutive. Trump ordering places every
/// level-rank pair together, then the small-joker pair, then the big-joker pair.
fn pair_position(base: i32, rules: &TractorRules) -> i32 {
    if base == 54 {
        return 102; // big joker
    }
    if base == 53 {
        return 101; // small joker
    }
    let rank = card_rank(base);
    if rank == rules.target_rank as i32 {
        return 100; // level rank (all suits share one slot)
    }
    // Plain rank: shift ranks above the trump rank down by one so the gap closes.
    if rank > rules.target_rank as i32 {
        rank - 1
    } else {
        rank
    }
}

/// Suit of a lead play: `None` when it is trump, otherwise the plain suit.
pub fn play_suit(cards: &[i32], rules: &TractorRules) -> Option<i32> {
    if cards.iter().any(|card| is_trump_card(*card, rules)) {
        None
    } else {
        cards.first().and_then(|card| card_suit(*card))
    }
}

fn take_card(remaining: &mut Vec<i32>, card: i32) {
    if let Some(idx) = remaining.iter().position(|c| *c == card) {
        remaining.remove(idx);
    }
}

/// Decompose a throw into maximal tractors, remaining pairs and singles. The
/// weakest beatable component is the card group forced out when a throw fails.
pub fn throw_components(cards: &[i32], rules: &TractorRules) -> Option<Vec<Vec<i32>>> {
    let classified = classify(cards, rules)?;
    if !matches!(classified.kind, ComboKind::Throw { .. }) {
        return None;
    }

    let mut by_base: HashMap<i32, Vec<i32>> = HashMap::new();
    for card in cards {
        by_base.entry(base_card(*card)).or_default().push(*card);
    }
    let mut pairs_by_position: HashMap<i32, Vec<Vec<i32>>> = HashMap::new();
    let mut triples_by_position: HashMap<i32, Vec<Vec<i32>>> = HashMap::new();
    let mut singles = Vec::new();
    for (base, mut copies) in by_base {
        copies.sort_unstable();
        if rules.deck_count >= 3 {
            while copies.len() >= 3 {
                let triple = vec![copies.remove(0), copies.remove(0), copies.remove(0)];
                triples_by_position
                    .entry(pair_position(base, rules))
                    .or_default()
                    .push(triple);
            }
        }
        while copies.len() >= 2 {
            let pair = vec![copies.remove(0), copies.remove(0)];
            pairs_by_position
                .entry(pair_position(base, rules))
                .or_default()
                .push(pair);
        }
        singles.extend(copies);
    }

    let mut components = Vec::new();
    loop {
        let mut positions: Vec<_> = triples_by_position
            .iter()
            .filter(|(_, triples)| !triples.is_empty())
            .map(|(position, _)| *position)
            .collect();
        positions.sort_unstable();
        let mut best_run: Vec<i32> = Vec::new();
        let mut current: Vec<i32> = Vec::new();
        for position in positions {
            if current
                .last()
                .is_some_and(|previous| position == *previous + 1)
            {
                current.push(position);
            } else {
                if current.len() > best_run.len() {
                    best_run = current;
                }
                current = vec![position];
            }
        }
        if current.len() > best_run.len() {
            best_run = current;
        }
        if best_run.len() < 2 {
            break;
        }
        let mut titanic = Vec::new();
        for position in best_run {
            if let Some(triple) = triples_by_position.get_mut(&position).and_then(Vec::pop) {
                titanic.extend(triple);
            }
        }
        components.push(titanic);
    }
    for triples in triples_by_position.into_values() {
        components.extend(triples);
    }
    loop {
        let mut positions: Vec<_> = pairs_by_position
            .iter()
            .filter(|(_, pairs)| !pairs.is_empty())
            .map(|(position, _)| *position)
            .collect();
        positions.sort_unstable();
        let mut best_run: Vec<i32> = Vec::new();
        let mut current: Vec<i32> = Vec::new();
        for position in positions {
            if current
                .last()
                .is_some_and(|previous| position == *previous + 1)
            {
                current.push(position);
            } else {
                if current.len() > best_run.len() {
                    best_run = current;
                }
                current = vec![position];
            }
        }
        if current.len() > best_run.len() {
            best_run = current;
        }
        if best_run.len() < 2 {
            break;
        }
        let mut tractor = Vec::new();
        for position in best_run {
            if let Some(pair) = pairs_by_position.get_mut(&position).and_then(Vec::pop) {
                tractor.extend(pair);
            }
        }
        components.push(tractor);
    }
    for pairs in pairs_by_position.into_values() {
        components.extend(pairs);
    }
    components.extend(singles.into_iter().map(|card| vec![card]));
    components.sort_by_key(|component| {
        combo_win_value(
            component,
            &classify(component, rules).expect("throw component is valid"),
            rules,
        )
        .unwrap_or_default()
    });
    Some(components)
}

/// Standard Tractor bottom multiplier for the winning play of the last trick.
///
/// A successful throw is scored by its strongest component, never by the
/// total number of cards in the throw. Each independent winning shape scores
/// `2 ^ card_count`, capped at 64.
pub fn bottom_multiplier(cards: &[i32], rules: &TractorRules) -> i32 {
    let Some(combo) = classify(cards, rules) else {
        return 1;
    };
    match combo.kind {
        ComboKind::Single
        | ComboKind::Pair
        | ComboKind::Triple
        | ComboKind::Tractor(_)
        | ComboKind::Titanic(_) => shape_bottom_multiplier(cards.len()),
        ComboKind::Throw { .. } => throw_components(cards, rules)
            .into_iter()
            .flatten()
            .map(|component| shape_bottom_multiplier(component.len()))
            .max()
            .unwrap_or(1),
    }
}

fn shape_bottom_multiplier(card_count: usize) -> i32 {
    if card_count >= 6 {
        64
    } else {
        1_i32 << card_count
    }
}

/// Total point value collected in a trick.
pub fn trick_points(trick: &[WsTractorPlayedCards]) -> i32 {
    trick
        .iter()
        .flat_map(|played| played.cards.iter())
        .map(|card| card_score(*card))
        .sum()
}

/// Winner (position) of a completed or in-progress trick.
pub fn trick_winner(trick: &[WsTractorPlayedCards], rules: &TractorRules) -> Option<usize> {
    let lead = trick.first()?;
    let lead_combo = classify(&lead.cards, rules)?;
    let mut best_position = usize::try_from(lead.position).ok()?;
    let mut best_value = combo_win_value(&lead.cards, &lead_combo, rules)?;
    for played in trick.iter().skip(1) {
        let Ok(position) = usize::try_from(played.position) else {
            continue;
        };
        if let Some(value) = combo_win_value(&played.cards, &lead_combo, rules)
            && value > best_value
        {
            best_value = value;
            best_position = position;
        }
    }
    Some(best_position)
}

impl ComboKind {
    pub fn card_count(self) -> usize {
        match self {
            ComboKind::Single => 1,
            ComboKind::Pair => 2,
            ComboKind::Triple => 3,
            ComboKind::Tractor(n) => 2 * n,
            ComboKind::Titanic(n) => 3 * n,
            ComboKind::Throw { cards, .. } => cards,
        }
    }
}

#[cfg(test)]
#[path = "combo/coverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
#[path = "combo/tests.rs"]
mod tests;

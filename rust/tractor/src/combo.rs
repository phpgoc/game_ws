//! Card-combination logic for tractor (拖拉机 / 升级).
//!
//! The trump group is made of every card of the current target rank plus both
//! jokers; all other cards are "plain" and belong to their natural suit. A legal
//! play is a single group of one of four shapes:
//!   - Single: one card.
//!   - Pair:   two identical cards (same base card, regardless of deck copy).
//!   - Tractor: two or more consecutive pairs in the same group (连对).
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboKind {
    Single,
    Pair,
    /// A run of `n` consecutive pairs (n >= 2), so `2 * n` cards.
    Tractor(usize),
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
        });
    }

    let counts = identity_counts(cards);
    if cards.len() == 2 && counts.len() == 1 {
        return Some(Combo {
            kind: ComboKind::Pair,
            suit,
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
            });
        }
    }

    Some(Combo {
        kind: ComboKind::Throw {
            cards: cards.len(),
            pairs: counts.values().map(|count| count / 2).sum(),
        },
        suit,
    })
}

fn combinations(cards: &[i32], count: usize) -> Vec<Vec<i32>> {
    fn visit(
        cards: &[i32],
        count: usize,
        start: usize,
        current: &mut Vec<i32>,
        out: &mut Vec<Vec<i32>>,
    ) {
        if current.len() == count {
            out.push(current.clone());
            return;
        }
        let needed = count - current.len();
        if cards.len().saturating_sub(start) < needed {
            return;
        }
        for index in start..=cards.len() - needed {
            current.push(cards[index]);
            visit(cards, count, index + 1, current, out);
            current.pop();
        }
    }

    if count > cards.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    visit(cards, count, 0, &mut Vec::with_capacity(count), &mut out);
    out
}

/// Ranking value of a played combo *if* it can beat the current lead, else
/// `None`. Higher wins. A play only competes when it matches the lead shape and
/// is either trump or the exact lead plain suit.
pub fn combo_win_value(cards: &[i32], lead: &Combo, rules: &TractorRules) -> Option<i32> {
    let combo = classify(cards, rules)?;
    if combo.kind != lead.kind {
        return None;
    }
    match combo.suit {
        None => {} // trump always competes
        // A plain follow only competes when it repeats the lead's plain suit.
        Some(suit) if lead.suit == Some(suit) => {}
        Some(_) => return None,
    }
    cards
        .iter()
        .map(|card| tractor_card_value(*card, rules, lead.suit))
        .max()
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
        let group_subsets = combinations(&group, group_count);
        let outside_subsets = combinations(&outside, outside_count);
        for group_cards in &group_subsets {
            for outside_cards in &outside_subsets {
                let mut cards = Vec::with_capacity(lead_len);
                cards.extend_from_slice(group_cards);
                cards.extend_from_slice(outside_cards);
                if follow_is_legal(hand, &cards, lead, rules) && !out.contains(&cards) {
                    out.push(cards);
                }
            }
        }
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
        ComboKind::Tractor(n) => n,
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
    true
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

    // 1. Satisfy required pairs from the lead group, lowest first.
    let required_pairs = match lead.kind {
        ComboKind::Single => 0,
        ComboKind::Pair => 1,
        ComboKind::Tractor(n) => n,
        ComboKind::Throw { pairs, .. } => pairs,
    };
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
    for pair in group_pairs.into_iter().take(required_pairs) {
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
    let mut singles = Vec::new();
    for (base, mut copies) in by_base {
        copies.sort_unstable();
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
    let mut best_value = lead
        .cards
        .iter()
        .map(|card| tractor_card_value(*card, rules, lead_combo.suit))
        .max()?;
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
            ComboKind::Tractor(n) => 2 * n,
            ComboKind::Throw { cards, .. } => cards,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use share_type_public::TractorRank;

    #[test]
    fn ace_pair_and_queen_pair_form_throw_with_weak_pair_first() {
        let rules = rules(TractorRank::TWO);
        let cards = vec![13, 113, 11, 111];
        assert_eq!(
            classify(&cards, &rules).map(|combo| combo.kind),
            Some(ComboKind::Throw { cards: 4, pairs: 2 })
        );
        let components = throw_components(&cards, &rules).expect("throw components");
        assert_eq!(components, vec![vec![11, 111], vec![13, 113]]);
    }

    #[test]
    fn declared_trump_suit_cards_beat_plain_cards() {
        let mut rules = rules(TractorRank::TWO);
        rules.trump_suit = Some(share_type_public::TractorSuit::HEART);
        let trick = [
            played(0, vec![13]), // spade A leads
            played(1, vec![15]), // heart 3 is trump and ruffs
        ];
        assert_eq!(trick_winner(&trick, &rules), Some(1));
    }

    #[test]
    fn enumerate_leads_finds_pairs_and_tractors() {
        let rules = rules(TractorRank::TWO);
        let hand = vec![2, 102, 3, 103, 20];
        let leads = enumerate_leads(&hand, &rules);
        let has_tractor = leads.iter().any(|cards| {
            matches!(
                classify(cards, &rules).map(|c| c.kind),
                Some(ComboKind::Tractor(2))
            )
        });
        let has_pair = leads
            .iter()
            .any(|cards| classify(cards, &rules).map(|c| c.kind) == Some(ComboKind::Pair));
        assert!(has_tractor);
        assert!(has_pair);
        assert!(leads.iter().any(|cards| cards == &vec![20]));
    }

    #[test]
    fn enumerate_leads_keeps_pair_single_and_multi_deck_pair_throws() {
        let rules = rules(TractorRank::TWO);
        let pair_single = enumerate_leads(&[11, 111, 13], &rules);
        assert!(pair_single.iter().any(|cards| {
            cards == &vec![11, 111, 13]
                && classify(cards, &rules).map(|combo| combo.kind)
                    == Some(ComboKind::Throw { cards: 3, pairs: 1 })
        }));

        let mut four_deck_rules = rules.clone();
        four_deck_rules.deck_count = 4;
        let two_identical_pairs = enumerate_leads(&[2, 102, 202, 302], &four_deck_rules);
        assert!(two_identical_pairs.iter().any(|cards| {
            cards == &vec![2, 102, 202, 302]
                && classify(cards, &four_deck_rules).map(|combo| combo.kind)
                    == Some(ComboKind::Throw { cards: 4, pairs: 2 })
        }));
    }

    #[test]
    fn multi_deck_duplicate_pair_throw_follows_stay_legal() {
        let mut rules = rules(TractorRank::TWO);
        rules.deck_count = 4;
        let lead = classify(&[2, 102, 202, 302], &rules).expect("two-pair throw");
        let hand = vec![3, 103, 203, 303];
        let candidates = enumerate_follows(&hand, &lead, &rules);

        assert!(candidates.contains(&hand));
        assert!(
            candidates
                .iter()
                .all(|cards| follow_is_legal(&hand, cards, &lead, &rules))
        );
    }

    #[test]
    fn follow_candidates_include_point_avoiding_single_combinations() {
        let rules = rules(TractorRank::TWO);
        let lead = classify(&[8, 108], &rules).expect("pair lead");
        let candidates = enumerate_follows(&[4, 5, 6], &lead, &rules);

        assert!(candidates.contains(&vec![4, 5]));
        assert!(candidates.contains(&vec![5, 6]));
        assert!(
            candidates
                .iter()
                .all(|cards| follow_is_legal(&[4, 5, 6], cards, &lead, &rules))
        );
    }

    #[test]
    fn forced_follow_is_always_legal() {
        let rules = rules(TractorRank::TWO);
        let lead = classify(&[2, 102], &rules).unwrap();
        let hand = vec![3, 103, 20, 21];
        let follow = forced_follow(&hand, &lead, &rules).expect("forced follow");
        assert!(follow_is_legal(&hand, &follow, &lead, &rules));
        // Must reuse the held pair.
        assert_eq!(follow, vec![3, 103]);
    }

    #[test]
    fn forced_tractor_follow_uses_multiple_pairs_of_one_identity() {
        let rules = rules(TractorRank::TWO);
        let lead = classify(&[16, 116, 17, 117, 18, 118], &rules).expect("three-pair tractor");
        let hand = vec![15, 20, 120, 320, 21, 22, 122, 222, 322, 23];
        let follow = forced_follow(&hand, &lead, &rules).expect("forced follow");

        assert!(follow_is_legal(&hand, &follow, &lead, &rules));
        assert_eq!(count_group_pairs(&follow, lead.suit, &rules), 3);
    }

    #[test]
    fn higher_pair_beats_lower_pair() {
        let rules = rules(TractorRank::TWO);
        let trick = [
            played(0, vec![2, 102]), // suit0 rank3 pair
            played(1, vec![5, 105]), // suit0 rank6 pair beats
        ];
        assert_eq!(trick_winner(&trick, &rules), Some(1));
    }

    #[test]
    fn higher_same_suit_single_wins_but_off_suit_cannot() {
        let rules = rules(TractorRank::TWO);
        let trick = [
            played(0, vec![5]),  // suit0 rank6 leads
            played(1, vec![6]),  // suit0 rank7 beats
            played(2, vec![18]), // suit1: off-suit, cannot win
            played(3, vec![4]),  // suit0 rank5, below the lead
        ];
        assert_eq!(trick_winner(&trick, &rules), Some(1));
    }

    #[test]
    fn identical_cards_form_a_pair_but_same_rank_does_not() {
        let rules = rules(TractorRank::TWO);
        // 2 and 102 are both (suit 0, rank 3): a real pair.
        assert!(matches!(
            classify(&[2, 102], &rules).map(|c| c.kind),
            Some(ComboKind::Pair)
        ));
        // 4 (suit 0, rank 5) and 17 (suit 1, rank 5): same rank, different suits.
        assert!(classify(&[4, 17], &rules).is_none());
    }

    #[test]
    fn must_follow_suit_and_preserve_pairs() {
        let rules = rules(TractorRank::TWO);
        // Lead a suit-0 pair; hand holds a suit-0 pair plus off-suit cards.
        let lead = classify(&[2, 102], &rules).unwrap();
        let hand = vec![3, 103, 20, 21];
        // Two suit-0 singles instead of the available pair: illegal.
        assert!(!follow_is_legal(&hand, &[3, 20], &lead, &rules));
        // Playing the suit-0 pair: legal.
        assert!(follow_is_legal(&hand, &[3, 103], &lead, &rules));
    }

    fn played(position: i32, cards: Vec<i32>) -> WsTractorPlayedCards {
        WsTractorPlayedCards {
            position,
            name: String::new(),
            cards,
        }
    }

    // Card encoding (deck copy 0): base 1..13 = suit 0 ranks 2..A, 14..26 = suit 1,
    // 27..39 = suit 2, 40..52 = suit 3, 53 = small joker, 54 = big joker. A second
    // deck copy adds 100, so 2 and 102 are the identical card (suit 0, rank 3).
    fn rules(target: TractorRank) -> TractorRules {
        TractorRules {
            blood_enabled: true,
            blood_score_per_unit: 40,
            blood_start_score: 80,
            bottom_card_count: 8,
            deck_count: 2,
            final_target_rank: TractorRank::A,
            removed_rank_count: 0,
            target_rank: target,
            trump_suit: None,
        }
    }

    #[test]
    fn tractor_needs_consecutive_identity_pairs() {
        let rules = rules(TractorRank::TWO);
        // (suit0 rank3)² + (suit0 rank4)² = a length-2 tractor.
        assert!(matches!(
            classify(&[2, 102, 3, 103], &rules).map(|c| c.kind),
            Some(ComboKind::Tractor(2))
        ));
        // rank3² + rank5² leaves a gap: a two-pair throw, not a tractor.
        assert_eq!(
            classify(&[2, 102, 4, 104], &rules).map(|combo| combo.kind),
            Some(ComboKind::Throw { cards: 4, pairs: 2 })
        );
    }

    #[test]
    fn trump_beats_any_plain_lead() {
        let rules = rules(TractorRank::TWO);
        let trick = [
            played(0, vec![6]), // suit0 rank7 leads
            played(1, vec![1]), // suit0 rank2 = trump, ruffs in
        ];
        assert_eq!(trick_winner(&trick, &rules), Some(1));
    }

    #[test]
    fn trump_rank_closes_the_tractor_gap() {
        // Target rank 5 is trump, so suit-0 rank4 and rank6 become adjacent.
        let rules = rules(TractorRank::FIVE);
        // rank4 = base 3, rank6 = base 5.
        assert!(matches!(
            classify(&[3, 103, 5, 105], &rules).map(|c| c.kind),
            Some(ComboKind::Tractor(2))
        ));
    }

    #[test]
    fn void_in_led_suit_allows_any_cards() {
        let rules = rules(TractorRank::TWO);
        let lead = classify(&[2, 102], &rules).unwrap();
        // Hand has no suit-0 cards, so any two cards follow.
        let hand = vec![20, 21, 34];
        assert!(follow_is_legal(&hand, &[20, 34], &lead, &rules));
    }
}

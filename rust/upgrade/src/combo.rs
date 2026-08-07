use std::collections::HashMap;

use upgrade_common::{Card, Rank, Suit, largest_identity_group_size};

/// 升级服务用于牌型比较的主牌信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpgradeComboRules {
    pub target_rank: Rank,
    pub trump_suit: Option<Suit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboKind {
    Single,
    Pair,
    Triple,
    /// 甩牌只记录张数和最长的相同牌面组件；连续对子不会被合并。
    Throw {
        cards: usize,
        max_multiplicity: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    pub kind: ComboKind,
    pub group: Option<Suit>,
    /// Same-identity component sizes, largest first. They carry no
    /// consecutive-run meaning in Upgrade.
    pub multiplicities: Vec<usize>,
}

pub fn card_group(card: Card, rules: UpgradeComboRules) -> Option<Suit> {
    if card.suit().is_none() || card.rank() == rules.target_rank || rules.trump_suit == card.suit()
    {
        None
    } else {
        card.suit()
    }
}

/// Comparison value inside an upgrade trick. Level cards sit above ordinary
/// trump cards; the level card in the selected trump suit sits above the
/// off-suit level cards, and jokers remain highest.
pub fn card_strength(card: Card, rules: UpgradeComboRules) -> i32 {
    if card_group(card, rules).is_none() {
        if card.suit().is_none() {
            return 1_200 + card.rank() as i32;
        }
        if card.rank() == rules.target_rank {
            return if card.suit() == rules.trump_suit {
                1_100
            } else {
                1_000
            };
        }
        return 900 + card.rank() as i32;
    }
    card.rank() as i32
}

pub fn same_group(cards: &[Card], rules: UpgradeComboRules) -> Option<Option<Suit>> {
    let first = *cards.first()?;
    let group = card_group(first, rules);
    cards
        .iter()
        .all(|card| card_group(*card, rules) == group)
        .then_some(group)
}

fn identity_groups(cards: &[Card]) -> HashMap<u8, Vec<Card>> {
    let mut groups = HashMap::new();
    for card in cards {
        groups
            .entry(card.identity())
            .or_insert_with(Vec::new)
            .push(*card);
    }
    groups
}

/// 按升级规则识别一手牌。连续对子故意归入 `Throw`。
pub fn classify(cards: &[Card], rules: UpgradeComboRules) -> Option<Combo> {
    let group = same_group(cards, rules)?;
    let counts = identity_groups(cards);
    let max_multiplicity = counts.values().map(Vec::len).max()?;
    let mut multiplicities = counts.values().map(Vec::len).collect::<Vec<_>>();
    multiplicities.sort_unstable_by(|left, right| right.cmp(left));
    let kind = match cards.len() {
        1 => ComboKind::Single,
        2 if max_multiplicity == 2 && counts.len() == 1 => ComboKind::Pair,
        3 if max_multiplicity == 3 && counts.len() == 1 => ComboKind::Triple,
        cards => ComboKind::Throw {
            cards,
            max_multiplicity,
        },
    };
    Some(Combo {
        kind,
        group,
        multiplicities,
    })
}

/// 将甩牌拆成独立的同身份组件，不产生拖拉机牌型。
pub fn throw_components(cards: &[Card], rules: UpgradeComboRules) -> Option<Vec<Vec<Card>>> {
    if !matches!(classify(cards, rules)?.kind, ComboKind::Throw { .. }) {
        return None;
    }

    let mut components: Vec<Vec<Card>> = identity_groups(cards).into_values().collect();
    for component in &mut components {
        component.sort_by_key(|card| card.encoded());
    }
    components.sort_by_key(|component| {
        (
            component.len(),
            component
                .first()
                .map(|card| card_strength(*card, rules))
                .unwrap_or_default(),
            component
                .first()
                .map(|card| card.encoded())
                .unwrap_or_default(),
        )
    });
    Some(components)
}

/// 成功扣底时只按最长相同牌面组件计算倍率。
pub fn bottom_multiplier(cards: &[Card]) -> usize {
    largest_identity_group_size(cards).max(1)
}

fn component_beats(lead: &[Card], candidate: &[Card], rules: UpgradeComboRules) -> bool {
    if candidate.len() < lead.len() {
        return false;
    }
    let Some(lead_group) = same_group(lead, rules) else {
        return false;
    };
    let Some(candidate_group) = same_group(candidate, rules) else {
        return false;
    };
    if lead_group.is_none() != candidate_group.is_none() || lead_group != candidate_group {
        return candidate_group.is_none() && lead_group.is_some();
    }
    candidate
        .first()
        .zip(lead.first())
        .is_some_and(|(candidate, lead)| {
            card_strength(*candidate, rules) > card_strength(*lead, rules)
        })
}

fn hand_components(hand: &[Card], group: Option<Suit>, rules: UpgradeComboRules) -> Vec<Vec<Card>> {
    let mut groups = identity_groups(
        &hand
            .iter()
            .copied()
            .filter(|card| card_group(*card, rules) == group)
            .collect::<Vec<_>>(),
    )
    .into_values()
    .collect::<Vec<_>>();
    groups.sort_by_key(|component| (component.len(), component[0].rank()));
    groups
}

/// 返回甩牌失败后必须打出的最弱、但确实会被对手手牌顶回的组件。
pub fn failed_throw_component(
    attempted: &[Card],
    opponent_hand: &[Card],
    rules: UpgradeComboRules,
) -> Option<Vec<Card>> {
    let combo = classify(attempted, rules)?;
    if !matches!(combo.kind, ComboKind::Throw { .. }) {
        return None;
    }
    let components = throw_components(attempted, rules)?;
    let opponent_components = hand_components(opponent_hand, combo.group, rules);
    components.into_iter().find(|component| {
        opponent_components
            .iter()
            .any(|candidate| component_beats(component, candidate, rules))
    })
}

pub fn follow_is_legal(
    hand: &[Card],
    cards: &[Card],
    lead: &Combo,
    rules: UpgradeComboRules,
) -> bool {
    if cards.len() != lead_card_count(lead) {
        return false;
    }
    let mut available = hand.to_vec();
    for card in cards {
        let Some(index) = available.iter().position(|candidate| candidate == card) else {
            return false;
        };
        available.remove(index);
    }
    let group_in_hand = hand
        .iter()
        .filter(|card| card_group(**card, rules) == lead.group)
        .count();
    let group_in_play = cards
        .iter()
        .filter(|card| card_group(**card, rules) == lead.group)
        .count();
    if group_in_play < group_in_hand.min(cards.len()) {
        return false;
    }

    let grouped_hand = hand
        .iter()
        .copied()
        .filter(|card| card_group(*card, rules) == lead.group)
        .collect::<Vec<_>>();
    let grouped_play = cards
        .iter()
        .copied()
        .filter(|card| card_group(*card, rules) == lead.group)
        .collect::<Vec<_>>();
    component_follow_score(&grouped_play, &lead.multiplicities)
        == component_follow_score(&grouped_hand, &lead.multiplicities)
}

/// Build one deterministic legal follow while preserving as much of every
/// same-identity component required by the lead as the hand can supply.
pub fn forced_follow(hand: &[Card], lead: &Combo, rules: UpgradeComboRules) -> Option<Vec<Card>> {
    let card_count = lead_card_count(lead);
    if hand.len() < card_count {
        return None;
    }

    let mut grouped = identity_groups(
        &hand
            .iter()
            .copied()
            .filter(|card| card_group(*card, rules) == lead.group)
            .collect::<Vec<_>>(),
    )
    .into_values()
    .collect::<Vec<_>>();
    for component in &mut grouped {
        component.sort_by_key(|card| (card_strength(*card, rules), card.encoded()));
    }
    grouped.sort_by_key(|component| {
        component
            .first()
            .map(|card| (card_strength(*card, rules), card.encoded()))
            .unwrap_or_default()
    });

    let required_group_count = grouped.iter().map(Vec::len).sum::<usize>().min(card_count);
    let mut selected = Vec::with_capacity(card_count);
    for requirement in &lead.multiplicities {
        let remaining_slots = required_group_count.saturating_sub(selected.len());
        if remaining_slots == 0 {
            break;
        }
        let mut best_index = None;
        let mut best_match = 0;
        for (index, component) in grouped.iter().enumerate() {
            let matched = component.len().min(*requirement).min(remaining_slots);
            if matched > best_match {
                best_match = matched;
                best_index = Some(index);
            }
        }
        let Some(best_index) = best_index else {
            break;
        };
        selected.extend(grouped[best_index].drain(..best_match));
    }

    for component in &mut grouped {
        let remaining_slots = required_group_count.saturating_sub(selected.len());
        if remaining_slots == 0 {
            break;
        }
        selected.extend(component.drain(..component.len().min(remaining_slots)));
    }

    let mut outside = hand
        .iter()
        .copied()
        .filter(|card| card_group(*card, rules) != lead.group)
        .collect::<Vec<_>>();
    outside.sort_by_key(|card| (card_strength(*card, rules), card.encoded()));
    selected.extend(outside.into_iter().take(card_count - selected.len()));

    (selected.len() == card_count && follow_is_legal(hand, &selected, lead, rules))
        .then_some(selected)
}

pub fn can_compete_with_lead(cards: &[Card], lead: &Combo, rules: UpgradeComboRules) -> bool {
    let Some(candidate) = classify(cards, rules) else {
        return false;
    };
    if lead_card_count(&candidate) != lead_card_count(lead) {
        return false;
    }
    match lead.kind {
        ComboKind::Single => candidate.kind == ComboKind::Single,
        ComboKind::Pair => candidate.kind == ComboKind::Pair,
        ComboKind::Triple => candidate.kind == ComboKind::Triple,
        ComboKind::Throw { .. } => {
            component_follow_score(cards, &lead.multiplicities) == lead.multiplicities
        }
    }
}

/// Return the strength of the lead's largest repeated component when this play
/// carries the same structure. Unrelated high singles or shorter components
/// never decide a long-throw cover.
pub fn combo_win_value(cards: &[Card], lead: &Combo, rules: UpgradeComboRules) -> Option<i32> {
    if !can_compete_with_lead(cards, lead, rules) {
        return None;
    }
    let required = lead.multiplicities.first().copied().unwrap_or(1);
    identity_groups(cards)
        .into_values()
        .filter(|component| component.len() >= required)
        .filter_map(|component| component.first().map(|card| card_strength(*card, rules)))
        .max()
}

fn component_follow_score(cards: &[Card], requirements: &[usize]) -> Vec<usize> {
    let mut available = identity_groups(cards)
        .into_values()
        .map(|cards| cards.len())
        .collect::<Vec<_>>();
    let mut score = Vec::with_capacity(requirements.len());
    for requirement in requirements {
        let Some((index, matched)) = available
            .iter()
            .enumerate()
            .map(|(index, count)| (index, (*count).min(*requirement)))
            .max_by_key(|(_, matched)| *matched)
        else {
            score.push(0);
            continue;
        };
        score.push(matched);
        available[index] -= matched;
    }
    score
}

pub const fn lead_card_count(combo: &Combo) -> usize {
    match combo.kind {
        ComboKind::Single => 1,
        ComboKind::Pair => 2,
        ComboKind::Triple => 3,
        ComboKind::Throw { cards, .. } => cards,
    }
}

#[cfg(test)]
#[path = "combo/tests.rs"]
mod tests;

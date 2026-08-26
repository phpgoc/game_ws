//! 升级的牌型分类、跟牌和甩牌分解。
//!
//! 升级的难点是保持领出牌的组别、张数以及同身份组件；本模块把这些约束
//! 集中在纯函数中，服务端请求和 AI 都使用同一套判定。

use std::collections::HashMap;

use upgrade_common::{
    Card, Rank, Suit, card_is_trump, largest_identity_group_size, trump_order_position,
};

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
    Repeated {
        cards: usize,
    },
    /// 甩牌只记录张数和最长的相同牌面组件；连续对子不会被合并。
    Throw {
        cards: usize,
        max_multiplicity: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    /// 组合类别、主牌组别和每个同身份组件的张数。
    pub kind: ComboKind,
    pub group: Option<Suit>,
    /// Same-identity component sizes, largest first. They carry no
    /// consecutive-run meaning in Upgrade.
    pub multiplicities: Vec<usize>,
}

pub fn card_group(card: Card, rules: UpgradeComboRules) -> Option<Suit> {
    (!card_is_trump(card, rules.target_rank, rules.trump_suit))
        .then(|| card.suit())
        .flatten()
}

/// Comparison value inside an upgrade trick. The selected trump suit wins each
/// main/vice tie; a non-level `2` is a trump only when it belongs to that suit.
pub fn card_strength(card: Card, rules: UpgradeComboRules) -> i32 {
    trump_order_position(card, rules.target_rank, rules.trump_suit)
        .map(|position| 1_000 + position)
        .unwrap_or(card.rank() as i32)
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
    // 先检查所有牌是否属于同一组，再按同身份数量识别对子/重复牌；其余
    // 组合保留为甩牌，不能错误降级成普通散牌。
    let group = same_group(cards, rules)?;
    let counts = identity_groups(cards);
    let max_multiplicity = counts.values().map(Vec::len).max()?;
    let mut multiplicities = counts.values().map(Vec::len).collect::<Vec<_>>();
    multiplicities.sort_unstable_by(|left, right| right.cmp(left));
    let kind = match cards.len() {
        1 => ComboKind::Single,
        2 if max_multiplicity == 2 && counts.len() == 1 => ComboKind::Pair,
        3 if max_multiplicity == 3 && counts.len() == 1 => ComboKind::Triple,
        cards if max_multiplicity == cards && counts.len() == 1 => ComboKind::Repeated { cards },
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
            component
                .first()
                .map(|card| card_strength(*card, rules))
                .unwrap_or_default(),
            component.len(),
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
    // 跟牌先满足领出组别的数量要求；只有手里没有足够同组牌时，才可用其他
    // 牌补齐张数，且最终组合仍必须能被 classify 识别。
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
    // 托管需要确定的合法跟牌：优先保留更高结构，无法完整跟出时再补齐最小
    // 牌组，最后由跟牌校验兜底。
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
        ComboKind::Repeated { cards } => candidate.kind == ComboKind::Repeated { cards },
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

const MAX_COMPONENT_CAPACITY: usize = 6;
const COMPONENT_MATCH_STATE_BUDGET: usize = 8_192;

fn component_follow_score(cards: &[Card], requirements: &[usize]) -> Vec<usize> {
    let mut histogram = [0_u8; MAX_COMPONENT_CAPACITY + 1];
    for count in identity_groups(cards).values().map(Vec::len) {
        if count > MAX_COMPONENT_CAPACITY {
            return component_follow_score_greedy(
                &identity_groups(cards)
                    .values()
                    .map(Vec::len)
                    .collect::<Vec<_>>(),
                requirements,
            );
        }
        histogram[count] = histogram[count].saturating_add(1);
    }

    let mut memo = HashMap::new();
    let mut states = 0;
    component_follow_score_bounded(histogram, requirements, 0, &mut memo, &mut states)
        .into_iter()
        .map(usize::from)
        .collect()
}

/// Each lead component is supplied by one identity group, while a group may
/// be split between later components. The current score is lexicographically
/// fixed by the largest remaining group; only equal-score group choices need
/// to be explored. This keeps the search bounded by the small six-deck copy
/// limit instead of enumerating card subsets.
fn component_follow_score_bounded(
    histogram: [u8; MAX_COMPONENT_CAPACITY + 1],
    requirements: &[usize],
    requirement_index: usize,
    memo: &mut HashMap<(usize, [u8; MAX_COMPONENT_CAPACITY + 1]), Vec<u8>>,
    states: &mut usize,
) -> Vec<u8> {
    if requirement_index == requirements.len() {
        return Vec::new();
    }

    let key = (requirement_index, histogram);
    if let Some(score) = memo.get(&key) {
        return score.clone();
    }
    if *states >= COMPONENT_MATCH_STATE_BUDGET {
        return component_follow_score_greedy_histogram(histogram, requirements, requirement_index);
    }
    *states += 1;

    let largest = (1..=MAX_COMPONENT_CAPACITY)
        .rev()
        .find(|capacity| histogram[*capacity] > 0)
        .unwrap_or_default();
    let matched = largest.min(requirements[requirement_index]);
    if matched == 0 {
        let score = vec![0; requirements.len() - requirement_index];
        memo.insert(key, score.clone());
        return score;
    }

    let mut best = None;
    for capacity in matched..=MAX_COMPONENT_CAPACITY {
        if histogram[capacity] == 0 {
            continue;
        }
        let mut next = histogram;
        next[capacity] -= 1;
        if capacity > matched {
            next[capacity - matched] = next[capacity - matched].saturating_add(1);
        }
        let suffix =
            component_follow_score_bounded(next, requirements, requirement_index + 1, memo, states);
        let mut candidate = Vec::with_capacity(suffix.len() + 1);
        candidate.push(matched as u8);
        candidate.extend(suffix);
        if best
            .as_ref()
            .is_none_or(|current: &Vec<u8>| candidate > *current)
        {
            best = Some(candidate);
        }
    }

    let score = best.unwrap_or_else(|| {
        component_follow_score_greedy_histogram(histogram, requirements, requirement_index)
    });
    memo.insert(key, score.clone());
    score
}

fn component_follow_score_greedy_histogram(
    mut histogram: [u8; MAX_COMPONENT_CAPACITY + 1],
    requirements: &[usize],
    requirement_index: usize,
) -> Vec<u8> {
    let mut score = Vec::with_capacity(requirements.len() - requirement_index);
    for requirement in &requirements[requirement_index..] {
        let largest = (1..=MAX_COMPONENT_CAPACITY)
            .rev()
            .find(|capacity| histogram[*capacity] > 0)
            .unwrap_or_default();
        let matched = largest.min(*requirement);
        score.push(matched as u8);
        if matched == 0 {
            continue;
        }
        let capacity = (matched..=MAX_COMPONENT_CAPACITY)
            .find(|capacity| histogram[*capacity] > 0)
            .unwrap_or(largest);
        histogram[capacity] -= 1;
        if capacity > matched {
            histogram[capacity - matched] = histogram[capacity - matched].saturating_add(1);
        }
    }
    score
}

fn component_follow_score_greedy(available: &[usize], requirements: &[usize]) -> Vec<usize> {
    let mut available = available.to_vec();
    available.sort_unstable();
    requirements
        .iter()
        .map(|requirement| {
            let matched = available
                .iter()
                .copied()
                .filter(|count| *count > 0)
                .max()
                .unwrap_or_default()
                .min(*requirement);
            let index = available
                .iter()
                .position(|count| *count >= matched && *count > 0);
            if let Some(index) = index {
                available[index] -= matched;
            }
            matched
        })
        .collect()
}

pub const fn lead_card_count(combo: &Combo) -> usize {
    match combo.kind {
        ComboKind::Single => 1,
        ComboKind::Pair => 2,
        ComboKind::Triple => 3,
        ComboKind::Repeated { cards } => cards,
        ComboKind::Throw { cards, .. } => cards,
    }
}

#[cfg(test)]
#[path = "combo/tests.rs"]
mod tests;

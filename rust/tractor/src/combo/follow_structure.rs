use std::collections::HashMap;

use super::{
    Combo, TractorRules, card_in_group, multiplicity_units, take_card, titanic_follow_priority,
    tractor_card_value,
};

fn multiplicity_run_components(
    cards: &[i32],
    lead_suit: Option<i32>,
    copies: usize,
    rules: &TractorRules,
) -> Vec<Vec<i32>> {
    let mut by_position: HashMap<i32, Vec<Vec<i32>>> = HashMap::new();
    for (position, _, cards) in multiplicity_units(cards, lead_suit, copies, rules) {
        by_position.entry(position).or_default().push(cards);
    }

    let mut components = Vec::new();
    loop {
        let mut positions = by_position
            .iter()
            .filter(|(_, units)| !units.is_empty())
            .map(|(position, _)| *position)
            .collect::<Vec<_>>();
        positions.sort_unstable();
        let mut best = Vec::new();
        let mut current = Vec::new();
        for position in positions {
            if current
                .last()
                .is_some_and(|previous| position == *previous + 1)
            {
                current.push(position);
            } else {
                if current.len() > best.len() {
                    best = current;
                }
                current = vec![position];
            }
        }
        if current.len() > best.len() {
            best = current;
        }
        if best.len() < 2 {
            break;
        }

        let mut component = Vec::with_capacity(best.len() * copies);
        for position in best {
            let unit = by_position
                .get_mut(&position)
                .and_then(|units| (!units.is_empty()).then(|| units.remove(0)))
                .expect("run position retains a multiplicity unit");
            component.extend(unit);
        }
        components.push(component);
    }
    components
}

fn best_run_unit_choices(run_lengths: &[usize], cap: usize) -> Vec<usize> {
    let mut states = vec![None; cap + 1];
    states[0] = Some(Vec::new());
    for run_length in run_lengths {
        let mut next = vec![None; cap + 1];
        for (used, path) in states.into_iter().enumerate() {
            let Some(path) = path else {
                continue;
            };
            for choice in std::iter::once(0).chain(2..=(*run_length).min(cap - used)) {
                let total = used + choice;
                if next[total].is_none() {
                    let mut choices = path.clone();
                    choices.push(choice);
                    next[total] = Some(choices);
                }
            }
        }
        states = next;
    }
    states
        .into_iter()
        .enumerate()
        .rev()
        .find_map(|(_, choices)| choices)
        .unwrap_or_else(|| vec![0; run_lengths.len()])
}

pub(super) fn maximum_run_units(
    cards: &[i32],
    lead_suit: Option<i32>,
    copies: usize,
    cap: usize,
    rules: &TractorRules,
) -> usize {
    let lengths = multiplicity_run_components(cards, lead_suit, copies, rules)
        .iter()
        .map(|component| component.len() / copies)
        .collect::<Vec<_>>();
    best_run_unit_choices(&lengths, cap).into_iter().sum()
}

pub(super) fn required_run_cards(
    cards: &[i32],
    lead_suit: Option<i32>,
    copies: usize,
    cap: usize,
    rules: &TractorRules,
) -> Vec<i32> {
    let components = multiplicity_run_components(cards, lead_suit, copies, rules);
    let lengths = components
        .iter()
        .map(|component| component.len() / copies)
        .collect::<Vec<_>>();
    let choices = best_run_unit_choices(&lengths, cap);
    components
        .into_iter()
        .zip(choices)
        .flat_map(|(component, units)| component.into_iter().take(units * copies))
        .collect()
}

#[derive(Debug, Default)]
pub(super) struct RequiredThrowStructure {
    pub(super) cards: Vec<i32>,
    pub(super) titanic_units: usize,
    pub(super) tractor_units: usize,
    pub(super) triple_units: usize,
}

fn take_lowest_structure_units(
    chosen: &mut Vec<i32>,
    remaining: &mut Vec<i32>,
    lead_suit: Option<i32>,
    copies: usize,
    count: usize,
    rules: &TractorRules,
) -> usize {
    let units = multiplicity_units(remaining, lead_suit, copies, rules)
        .into_iter()
        .take(count)
        .map(|(_, _, cards)| cards)
        .collect::<Vec<_>>();
    let selected = units.len();
    for unit in units {
        for card in unit {
            take_card(remaining, card);
            chosen.push(card);
        }
    }
    selected
}

fn take_titanic_fallback_structure(
    chosen: &mut Vec<i32>,
    remaining: &mut Vec<i32>,
    lead_suit: Option<i32>,
    triple_units: usize,
    rules: &TractorRules,
) {
    match titanic_follow_priority(remaining, lead_suit, rules) {
        6 => {
            let cards = required_run_cards(remaining, lead_suit, 2, triple_units, rules);
            for card in cards {
                take_card(remaining, card);
                chosen.push(card);
            }
        }
        5 => {
            take_lowest_structure_units(chosen, remaining, lead_suit, 3, triple_units, rules);
        }
        4 => {
            take_lowest_structure_units(chosen, remaining, lead_suit, 3, 1, rules);
            take_lowest_structure_units(chosen, remaining, lead_suit, 2, 1, rules);
        }
        3 => {
            take_lowest_structure_units(chosen, remaining, lead_suit, 3, 1, rules);
        }
        2 => {
            take_lowest_structure_units(chosen, remaining, lead_suit, 2, 2, rules);
        }
        1 => {
            take_lowest_structure_units(chosen, remaining, lead_suit, 2, 1, rules);
        }
        _ => {}
    }
}

pub(super) fn required_throw_structure(
    cards: &[i32],
    lead: &Combo,
    rules: &TractorRules,
) -> RequiredThrowStructure {
    let lead_suit = lead.suit;
    let mut remaining = cards
        .iter()
        .copied()
        .filter(|card| card_in_group(*card, lead_suit, rules))
        .collect::<Vec<_>>();
    remaining.sort_by_key(|card| tractor_card_value(*card, rules, lead_suit));
    let mut structure = RequiredThrowStructure::default();

    if lead.titanic_triple_count > 0 {
        let cards = required_run_cards(&remaining, lead_suit, 3, lead.titanic_triple_count, rules);
        structure.titanic_units = cards.len() / 3;
        for card in cards {
            take_card(&mut remaining, card);
            structure.cards.push(card);
        }
        if structure.titanic_units == 0 {
            take_titanic_fallback_structure(
                &mut structure.cards,
                &mut remaining,
                lead_suit,
                lead.titanic_triple_count,
                rules,
            );
        }
    }

    if lead.tractor_pair_count > 0 {
        let cards = required_run_cards(&remaining, lead_suit, 2, lead.tractor_pair_count, rules);
        structure.tractor_units = cards.len() / 2;
        for card in cards {
            take_card(&mut remaining, card);
            structure.cards.push(card);
        }
    }

    let standalone_triples = lead.triple_count.saturating_sub(lead.titanic_triple_count);
    structure.triple_units = take_lowest_structure_units(
        &mut structure.cards,
        &mut remaining,
        lead_suit,
        3,
        standalone_triples,
        rules,
    );
    structure
}

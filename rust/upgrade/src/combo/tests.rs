use upgrade_common::{Card, Rank, Suit};

use super::*;

fn cards(values: &[i32]) -> Vec<Card> {
    values
        .iter()
        .copied()
        .map(Card::try_from)
        .collect::<Result<_, _>>()
        .unwrap()
}

fn rules() -> UpgradeComboRules {
    UpgradeComboRules {
        target_rank: Rank::Two,
        trump_suit: Some(Suit::Heart),
    }
}

#[test]
fn consecutive_pairs_are_a_throw_not_a_tractor() {
    let pair_run = cards(&[2, 102, 3, 103, 4, 104]);
    assert_eq!(
        classify(&pair_run, rules()).map(|combo| combo.kind),
        Some(ComboKind::Throw {
            cards: 6,
            max_multiplicity: 2,
        })
    );
    assert_eq!(
        throw_components(&pair_run, rules())
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![2, 2, 2]
    );
}

#[test]
fn longest_component_controls_successful_bottom_multiplier() {
    let throw = cards(&[2, 102, 202, 12, 112, 13]);
    assert_eq!(bottom_multiplier(&throw), 3);
    assert_eq!(
        throw_components(&throw, rules())
            .unwrap()
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn higher_triple_tops_triple_component_and_forces_it_back() {
    let attempted = cards(&[2, 102, 202]);
    let opponent = cards(&[3, 103, 203]);

    assert_eq!(
        classify(&attempted, rules()).map(|combo| combo.kind),
        Some(ComboKind::Triple)
    );

    let attempted_throw = cards(&[2, 102, 202, 12, 112, 13]);
    assert_eq!(
        failed_throw_component(&attempted_throw, &opponent, rules())
            .unwrap()
            .iter()
            .map(|card| card.rank())
            .collect::<Vec<_>>(),
        vec![Rank::Three, Rank::Three, Rank::Three]
    );
}

#[test]
fn higher_pair_forces_a_three_pair_throw_back_to_its_lowest_pair() {
    let attempted = cards(&[5, 105, 6, 106, 7, 107]);
    let opponent = cards(&[8, 108]);
    let fallback = failed_throw_component(&attempted, &opponent, rules()).unwrap();

    assert_eq!(
        fallback.iter().map(|card| card.rank()).collect::<Vec<_>>(),
        vec![Rank::Six, Rank::Six,]
    );
}

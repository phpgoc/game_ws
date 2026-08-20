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
        vec![3, 2, 1]
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
fn four_higher_copies_can_supply_the_three_needed_to_top_a_triple() {
    let attempted_throw = cards(&[2, 102, 202, 12, 112, 13]);
    let opponent = cards(&[3, 103, 203, 303]);

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
fn four_to_six_identical_cards_are_atomic_repeated_shapes() {
    for (identical, count) in [
        (vec![2, 102, 202, 302], 4),
        (vec![2, 102, 202, 302, 402], 5),
        (vec![2, 102, 202, 302, 402, 502], 6),
    ] {
        let repeated = cards(&identical);
        assert_eq!(
            classify(&repeated, rules()).map(|combo| combo.kind),
            Some(ComboKind::Repeated { cards: count })
        );
        assert!(throw_components(&repeated, rules()).is_none());
    }

    let four = cards(&[2, 102, 202, 302]);
    let higher_four = cards(&[3, 103, 203, 303]);
    assert!(failed_throw_component(&four, &higher_four, rules()).is_none());
}

#[test]
fn level_cards_and_jokers_stay_above_ordinary_trump_cards() {
    let rules = UpgradeComboRules {
        target_rank: Rank::Three,
        trump_suit: Some(Suit::Heart),
    };
    let ordered = cards(&[26, 1, 14, 2, 15, 53, 54]);
    let strengths = ordered
        .iter()
        .map(|card| card_strength(*card, rules))
        .collect::<Vec<_>>();
    assert!(strengths.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(
        ordered
            .iter()
            .all(|card| card_group(*card, rules).is_none())
    );
}

#[test]
fn permanent_two_lead_requires_upgrade_players_to_follow_trump() {
    let rules = UpgradeComboRules {
        target_rank: Rank::Three,
        trump_suit: Some(Suit::Heart),
    };
    let hand = cards(&[27, 127, 3, 103]);
    let lead = classify(&cards(&[1, 101]), rules).unwrap();

    assert!(follow_is_legal(&hand, &cards(&[27, 127]), &lead, rules));
    assert!(!follow_is_legal(&hand, &cards(&[3, 103]), &lead, rules));
}

#[test]
fn follow_uses_every_available_lead_group_card_for_each_lead_size() {
    let rules = UpgradeComboRules {
        target_rank: Rank::Ace,
        trump_suit: Some(Suit::Heart),
    };

    for lead_len in 1..=8 {
        let lead_values = (0..lead_len)
            .map(|offset| 2 + offset as i32)
            .collect::<Vec<_>>();
        let lead_cards = cards(&lead_values);
        let lead = classify(&lead_cards, rules).expect("same-suit lead");
        for held_group_count in 0..=lead_len + 2 {
            let group_cards = (0..lead_len + 2)
                .map(|offset| 102 + offset as i32)
                .collect::<Vec<_>>();
            let outside_cards = (0..lead_len + 1)
                .map(|offset| 15 + offset as i32)
                .collect::<Vec<_>>();
            let mut hand_values = group_cards[..held_group_count].to_vec();
            hand_values.extend_from_slice(&outside_cards);
            let hand = cards(&hand_values);

            let required_group_count = held_group_count.min(lead_len);
            let forced = forced_follow(&hand, &lead, rules)
                .expect("a hand with enough total cards always has a forced follow");
            assert_eq!(forced.len(), lead_len);
            assert_eq!(
                forced
                    .iter()
                    .filter(|card| card_group(**card, rules) == lead.group)
                    .count(),
                required_group_count,
                "lead_len={lead_len}, held_group_count={held_group_count}, forced={forced:?}",
            );
            assert!(follow_is_legal(&hand, &forced, &lead, rules));

            if required_group_count == 0 {
                continue;
            }
            let mut under_values = group_cards[..required_group_count - 1].to_vec();
            under_values.extend_from_slice(
                &outside_cards[..lead_len.saturating_sub(required_group_count - 1)],
            );
            let under_follow = cards(&under_values);
            assert_eq!(under_follow.len(), lead_len);
            assert!(
                !follow_is_legal(&hand, &under_follow, &lead, rules),
                "must not omit one of {required_group_count} available group cards for a {lead_len}-card lead",
            );
        }
    }
}

#[test]
fn every_two_is_permanent_trump_and_the_trump_suit_two_is_stronger() {
    let all_twos = cards(&[1, 14, 27, 40]);

    for target_rank in [Rank::Two, Rank::Three, Rank::Five, Rank::Ace] {
        let rules = UpgradeComboRules {
            target_rank,
            trump_suit: Some(Suit::Heart),
        };
        assert!(
            all_twos
                .iter()
                .all(|card| card_group(*card, rules).is_none())
        );
        assert!(card_strength(all_twos[1], rules) > card_strength(all_twos[0], rules));
    }
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

#[test]
fn pair_follow_cannot_be_split_when_a_pair_is_available() {
    let hand = cards(&[3, 103, 4, 5]);
    let lead_cards = cards(&[2, 102]);
    let lead = classify(&lead_cards, rules()).unwrap();

    assert!(follow_is_legal(&hand, &cards(&[3, 103]), &lead, rules()));
    assert!(!follow_is_legal(&hand, &cards(&[4, 5]), &lead, rules()));
}

#[test]
fn forced_follow_keeps_an_available_pair_together() {
    let hand = cards(&[3, 4, 103]);
    let lead = classify(&cards(&[2, 102]), rules()).unwrap();

    let selected = forced_follow(&hand, &lead, rules()).unwrap();

    assert_eq!(
        selected
            .iter()
            .map(|card| card.encoded())
            .collect::<Vec<_>>(),
        vec![3, 103]
    );
    assert!(follow_is_legal(&hand, &selected, &lead, rules()));
}

#[test]
fn triple_follow_preserves_the_largest_available_component() {
    let hand = cards(&[3, 103, 4, 5]);
    let lead_cards = cards(&[2, 102, 202]);
    let lead = classify(&lead_cards, rules()).unwrap();

    assert!(follow_is_legal(&hand, &cards(&[3, 103, 4]), &lead, rules()));
    assert!(!follow_is_legal(&hand, &cards(&[3, 4, 5]), &lead, rules()));
}

#[test]
fn throw_follow_keeps_each_non_consecutive_component_when_possible() {
    let lead_cards = cards(&[2, 102, 202, 12, 112, 13]);
    let lead = classify(&lead_cards, rules()).unwrap();
    let hand = cards(&[3, 103, 4, 104, 5, 6, 7]);

    assert!(follow_is_legal(
        &hand,
        &cards(&[3, 103, 4, 104, 5, 6]),
        &lead,
        rules()
    ));
    assert!(!follow_is_legal(
        &hand,
        &cards(&[3, 103, 4, 5, 6, 7]),
        &lead,
        rules()
    ));
}

#[test]
fn throw_can_compete_only_when_every_lead_component_is_covered() {
    let lead_cards = cards(&[3, 103, 5, 105, 6, 106]);
    let lead = classify(&lead_cards, rules()).unwrap();
    let one_pair_and_singles = cards(&[26, 126, 25, 24, 23, 22]);
    let three_pairs = cards(&[26, 126, 25, 125, 24, 124]);

    assert!(!can_compete_with_lead(
        &one_pair_and_singles,
        &lead,
        rules()
    ));
    assert!(can_compete_with_lead(&three_pairs, &lead, rules()));
}

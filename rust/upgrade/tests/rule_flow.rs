use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{UpgradePhase, UpgradeRank};
use upgrade::state::{UpgradeGameState, UpgradeRules};
use upgrade_common::{Card, Rank, Suit};
use ws_common::CommonGameState;

fn state_with_hands(bottom_cards: Vec<i32>, hands: HashMap<usize, Vec<i32>>) -> UpgradeGameState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common
            .lock()
            .unwrap()
            .add_player(position, position as u64 + 1, &format!("p{position}"));
    }
    let mut state = UpgradeGameState::from_common(common);
    state.phase = UpgradePhase::Play;
    state.rules = UpgradeRules {
        deck_count: upgrade::UpgradeDeckCount::new(3).expect("three upgrade decks"),
        target_rank: Rank::Three,
        final_target_rank: Rank::Ace,
        removed_rank_count: 0,
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: bottom_cards.len(),
        trump_suit: Some(Suit::Heart),
    };
    state.hands = hands;
    state.bottom_cards = bottom_cards;
    state.current_position = 0;
    state
}

#[test]
fn three_deck_long_throw_uses_only_the_longest_component_for_bottom_score() {
    let lead = vec![13, 113, 213, 11, 111, 10];
    let mut state = state_with_hands(
        vec![109],
        HashMap::from([
            (0, lead.clone()),
            (1, vec![2, 3, 5, 6, 7, 8]),
            (2, vec![102, 103, 105, 106, 107, 108]),
            (3, vec![202, 203, 205, 206, 207, 208]),
        ]),
    );

    let played = state
        .play_cards(0, lead.clone())
        .expect("long throw should be accepted");
    assert_eq!(played.played_cards, lead);

    for (position, cards) in [
        (1, vec![2, 3, 5, 6, 7, 8]),
        (2, vec![102, 103, 105, 106, 107, 108]),
        (3, vec![202, 203, 205, 206, 207, 208]),
    ] {
        state
            .play_cards(position, cards)
            .expect("legal long-throw follow should be accepted");
    }

    assert_eq!(state.phase, UpgradePhase::Settlement);
    assert_eq!(state.last_trick_winner, Some(0));
    assert_eq!(state.bottom_multiplier, 3);
    // The last trick has no points; the 10-point bottom is multiplied by the
    // longest three-card component, not by the six cards in the throw.
    assert_eq!(state.collected_scores.get(&0), Some(&30));
    let settlement = state.settlement_event();
    assert_eq!(settlement.score, 0);
    assert_eq!(settlement.level_change, 3);
    assert_eq!(settlement.winner_positions, vec![0, 2]);
    assert_eq!(settlement.target_rank, UpgradeRank::THREE);
    assert_eq!(settlement.next_target_rank, Some(UpgradeRank::SIX));
}

#[test]
fn upgrade_does_not_classify_consecutive_pairs_as_a_tractor() {
    let cards = [2, 102, 3, 103, 4, 104]
        .into_iter()
        .map(|card| Card::try_from(card).expect("valid card"))
        .collect::<Vec<_>>();
    let combo = upgrade::combo::classify(
        &cards,
        upgrade::combo::UpgradeComboRules {
            target_rank: Rank::Two,
            trump_suit: Some(Suit::Heart),
        },
    )
    .expect("same-group throw");
    assert!(matches!(
        combo.kind,
        upgrade::combo::ComboKind::Throw {
            cards: 6,
            max_multiplicity: 2
        }
    ));
}

#[test]
fn upgrade_follow_preserves_the_longest_components_without_tractor_continuity() {
    let lead = vec![13, 113, 12, 112, 11, 111];
    let mut state = state_with_hands(
        vec![],
        HashMap::from([
            (0, lead.clone()),
            (1, vec![10, 110, 9, 109, 8, 7]),
            (2, vec![6, 106, 5, 105, 4, 3]),
            (3, vec![18, 118, 19, 119, 20, 21]),
        ]),
    );

    state
        .play_cards(0, lead.clone())
        .expect("upgrade throw lead should be accepted");

    let hand_before_illegal_follow = state.hands.get(&1).cloned().unwrap();
    let error = state
        .play_cards(1, vec![10, 110, 9, 8, 7, 6])
        .expect_err("upgrade follow must preserve every available lead component");
    assert_eq!(error, "illegal follow");
    assert_eq!(state.hands.get(&1), Some(&hand_before_illegal_follow));

    let legal = state
        .play_cards(1, vec![10, 110, 9, 109, 8, 7])
        .expect("non-consecutive pairs with a single component should follow");
    assert_eq!(legal.played_cards, vec![10, 110, 9, 109, 8, 7]);
}

#[test]
fn six_deck_bottom_score_keeps_a_four_copy_component_as_the_maximum() {
    let mut state = state_with_hands(
        vec![9],
        HashMap::from([
            (0, vec![13, 113, 213, 313, 12, 112, 11]),
            (1, vec![10, 110, 210, 310, 8, 108, 7]),
            (2, vec![9, 109, 209, 309, 6, 106, 5]),
            (3, vec![8, 108, 208, 308, 4, 104, 3]),
        ]),
    );
    state.rules.deck_count = upgrade::UpgradeDeckCount::new(6).unwrap();

    let lead = vec![13, 113, 213, 313, 12, 112, 11];
    state
        .play_cards(0, lead.clone())
        .expect("six-deck long throw should be accepted");
    for (position, cards) in [
        (1, vec![10, 110, 210, 310, 8, 108, 7]),
        (2, vec![9, 109, 209, 309, 6, 106, 5]),
        (3, vec![8, 108, 208, 308, 4, 104, 3]),
    ] {
        state
            .play_cards(position, cards)
            .expect("six-deck component follow should be accepted");
    }

    assert_eq!(state.phase, UpgradePhase::Settlement);
    assert_eq!(state.last_trick_winner, Some(0));
    assert_eq!(state.bottom_multiplier, 4);
}

#[test]
fn long_throw_winner_compares_the_largest_repeated_component() {
    let lead = vec![3, 103, 203, 12, 112, 13];
    let mut state = state_with_hands(
        vec![],
        HashMap::from([
            (0, lead.clone()),
            (1, vec![16, 116, 216, 26, 126, 25]),
            (2, vec![17, 117, 217, 18, 118, 19]),
            (3, vec![29, 129, 229, 38, 138, 39]),
        ]),
    );

    state
        .play_cards(0, lead)
        .expect("triple, pair and singleton lead");
    for (position, cards) in [
        (1, vec![16, 116, 216, 26, 126, 25]),
        (2, vec![17, 117, 217, 18, 118, 19]),
        (3, vec![29, 129, 229, 38, 138, 39]),
    ] {
        state
            .play_cards(position, cards)
            .expect("same-structure long throw follow");
    }

    // Seat 2 owns the higher trump triple. Seat 1's unrelated ace pair must
    // not protect its lower triple from being covered.
    assert_eq!(state.last_trick_winner, Some(2));
}

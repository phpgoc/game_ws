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

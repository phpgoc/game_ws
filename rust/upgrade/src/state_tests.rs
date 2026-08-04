use std::sync::{Arc, Mutex};

use share_type_public::{UpgradePhase, UpgradeSuit};
use ws_common::CommonGameState;

use super::*;

fn rules(deck_count: u8) -> UpgradeRules {
    UpgradeRules {
        deck_count: UpgradeDeckCount::new(deck_count).unwrap(),
        target_rank: Rank::Three,
        final_target_rank: Rank::Ace,
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 8,
        trump_suit: None,
    }
}

#[test]
fn deal_has_four_even_hands_and_private_bottom() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(4)).unwrap();

    assert_eq!(state.phase, UpgradePhase::Bury);
    assert_eq!(state.bottom_cards.len(), 8);
    assert_eq!(state.hand_count(), 52);
    assert_eq!(state.hands[&0].len(), 60);
    assert!((1..4).all(|position| state.hands[&position].len() == 52));
    assert_eq!(state.dealt_count, 216);
}

#[test]
fn bury_then_select_trump_enters_play() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = UpgradeGameState::from_common(common);
    state.deal_new_round(rules(3)).unwrap();
    let bottom = state.bottom_cards.clone();
    state.bury_bottom(0, bottom).unwrap();
    assert_eq!(state.phase, UpgradePhase::Bury);
    state.select_trump(0, UpgradeSuit::HEART).unwrap();
    assert_eq!(state.phase, UpgradePhase::Play);
    assert_eq!(state.rules.trump_suit, Some(Suit::Heart));
}

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{UpgradePhase, UpgradeSuit, WsUpgradePlayedCards};
use upgrade_common::{Rank, Suit};
use ws_common::CommonGameState;

use super::{best_trump_suit, choose_bury, decide, declaration_cards};
use crate::{
    UpgradeDeckCount,
    state::{UpgradeGameState, UpgradeRules},
};

fn state() -> UpgradeGameState {
    let mut state = UpgradeGameState::from_common(Arc::new(Mutex::new(CommonGameState::new())));
    state.rules = UpgradeRules {
        deck_count: UpgradeDeckCount::new(3).expect("valid deck count"),
        target_rank: Rank::Three,
        final_target_rank: Rank::Ace,
        removed_rank_count: 0,
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: 2,
        trump_suit: Some(Suit::Heart),
    };
    state
}

#[test]
fn fallback_keeps_the_original_deterministic_actions() {
    let mut state = state();
    state.phase = UpgradePhase::Bury;
    state.hands = HashMap::from([(0, vec![4, 5, 6])]);
    assert!(declaration_cards(&state, 0, 0).is_none());
    assert_eq!(best_trump_suit(&state, 0), UpgradeSuit::SPADE);
    assert_eq!(choose_bury(&state), Some(vec![4, 5]));

    state.phase = UpgradePhase::Play;
    state.current_position = 0;
    assert_eq!(decide(&state, 0), Some(vec![4]));

    state.current_trick.push(WsUpgradePlayedCards {
        position: 3,
        name: "p3".into(),
        cards: vec![4],
    });
    assert!(decide(&state, 0).is_some());
}

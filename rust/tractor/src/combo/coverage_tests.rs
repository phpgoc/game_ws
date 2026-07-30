use share_type_public::{TractorRank, WsTractorPlayedCards};

use super::{
    capped_choose, classify, combinations, forced_follow, hand_contains, pair_position, play_suit,
    throw_components, trick_winner,
};
use crate::game_state::TractorRules;

fn rules() -> TractorRules {
    TractorRules {
        blood_enabled: true,
        blood_score_per_unit: 40,
        blood_start_score: 80,
        bottom_card_count: 8,
        deck_count: 2,
        final_target_rank: TractorRank::A,
        removed_rank_count: 0,
        target_rank: TractorRank::TWO,
        trump_suit: None,
    }
}

fn played(position: i32, cards: Vec<i32>) -> WsTractorPlayedCards {
    WsTractorPlayedCards {
        position,
        name: String::new(),
        cards,
    }
}

#[test]
fn combination_and_follow_boundaries_handle_empty_or_insufficient_inputs() {
    let rules = rules();
    assert_eq!(capped_choose(2, 3, 10), 11);
    assert_eq!(capped_choose(5, 2, 4), 5);
    assert!(combinations(&[1], 2).is_empty());
    assert!(classify(&[], &rules).is_none());

    let pair_lead = classify(&[2, 102], &rules).expect("pair lead");
    assert!(forced_follow(&[3], &pair_lead, &rules).is_none());
    assert!(!hand_contains(&[2], &[2, 2]));
}

#[test]
fn combo_utilities_handle_trump_groups_throws_and_invalid_positions() {
    let rules = rules();
    assert_eq!(pair_position(54, &rules), 102);
    assert_eq!(pair_position(53, &rules), 101);
    assert_eq!(pair_position(1, &rules), 100);
    assert_eq!(play_suit(&[2], &rules), Some(0));
    assert_eq!(play_suit(&[1], &rules), None);
    assert_eq!(play_suit(&[], &rules), None);

    assert!(throw_components(&[2], &rules).is_none());
    let components = throw_components(&[2, 102, 3, 103, 4], &rules)
        .expect("tractor and single throw components");
    assert!(components.contains(&vec![2, 102, 3, 103]));
    assert!(components.contains(&vec![4]));

    assert_eq!(
        trick_winner(&[played(0, vec![2]), played(-1, vec![3])], &rules),
        Some(0)
    );
}

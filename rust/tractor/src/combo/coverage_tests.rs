use share_type_public::{TractorRank, WsTractorPlayedCards};

use super::{
    Combo, ComboKind, capped_choose, classify, combinations, enumerate_follows, enumerate_leads,
    follow_is_legal, forced_follow, hand_contains, pair_position, play_suit, throw_components,
    trick_winner,
};
use crate::game_state::TractorRules;

fn rules() -> TractorRules {
    TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
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

#[test]
fn lead_and_follow_enumeration_keep_throw_candidates_bounded_and_legal() {
    let rules = rules();
    let short_hand = vec![3, 103, 5];
    assert!(enumerate_leads(&short_hand, &rules).contains(&vec![3, 103, 5]));

    let long_hand = vec![3, 103, 5, 105, 7, 107, 9, 109, 11];
    assert!(enumerate_leads(&long_hand, &rules).contains(&vec![3, 103, 5, 105, 7, 107, 9, 109]));

    let large_follow_hand: Vec<i32> = (3..=12).flat_map(|card| [card, card + 100]).collect();
    let large_lead = Combo {
        kind: ComboKind::Throw {
            cards: 10,
            pairs: 0,
        },
        suit: Some(0),
    };
    assert!(!enumerate_follows(&large_follow_hand, &large_lead, &rules).is_empty());

    let pair_lead = classify(&[3, 103], &rules).expect("plain pair lead");
    assert!(!follow_is_legal(
        &[4, 104, 5, 105],
        &[4, 5],
        &pair_lead,
        &rules
    ));
}

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{TractorPhase, TractorRank};
use tractor::game_state::{TractorGameState, TractorRules};
use ws_common::CommonGameState;

fn rules(deck_count: usize) -> TractorRules {
    TractorRules {
        attacking_win_score: 80,
        score_per_level: 40,
        shutout_bonus_levels: 1,
        bottom_card_count: if deck_count == 3 { 10 } else { 8 },
        deck_count,
        final_target_rank: TractorRank::A,
        target_rank: TractorRank::TWO,
        trump_suit: None,
    }
}

fn state_with_hands(
    rules: TractorRules,
    bottom_cards: Vec<i32>,
    hands: HashMap<usize, Vec<i32>>,
) -> TractorGameState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common
            .lock()
            .unwrap()
            .add_player(position, position as u64 + 1, &format!("p{position}"));
    }
    let mut state = TractorGameState::from_common(common);
    state.phase = TractorPhase::Play;
    state.rules = rules;
    state.bottom_cards = bottom_cards;
    state.hands = hands;
    state.current_position = 0;
    state
}

#[test]
fn three_deck_titanic_can_lead_the_final_trick_and_score_the_bottom() {
    let titanic = vec![3, 103, 203, 4, 104, 204];
    let state_hands = HashMap::from([
        (0, titanic.clone()),
        (1, vec![5, 6, 7, 8, 9, 10]),
        (2, vec![15, 16, 17, 18, 19, 20]),
        (3, vec![28, 29, 30, 31, 32, 33]),
    ]);
    let mut state = state_with_hands(rules(3), vec![4, 104], state_hands);

    let lead = state
        .play_cards(0, "p0".to_owned(), titanic.clone())
        .expect("Titanic lead should be accepted");
    assert_eq!(lead.cards, titanic);
    assert_eq!(state.current_position, 1);

    for (position, cards) in [
        (1, vec![5, 6, 7, 8, 9, 10]),
        (2, vec![15, 16, 17, 18, 19, 20]),
        (3, vec![28, 29, 30, 31, 32, 33]),
    ] {
        state
            .play_cards(position, format!("p{position}"), cards)
            .expect("legal Titanic follow should be accepted");
    }

    assert_eq!(state.phase, TractorPhase::Settlement);
    assert_eq!(state.bottom_multiplier, 18);
    // The Titanic trick itself carries 35 points; the ten-point bottom is
    // multiplied by 18, so the winner collects 35 + 180 = 215.
    assert_eq!(state.collected_scores.get(&0), Some(&215));
}

#[test]
fn pair_follow_cannot_be_split_when_the_follower_has_the_pair() {
    let state_hands = HashMap::from([
        (0, vec![5, 105]),
        (1, vec![6, 106, 20]),
        (2, vec![7, 107]),
        (3, vec![8, 108]),
    ]);
    let mut state = state_with_hands(rules(2), Vec::new(), state_hands);

    state
        .play_cards(0, "p0".to_owned(), vec![5, 105])
        .expect("pair lead should be accepted");
    let hand_before_illegal_follow = state.hands.get(&1).cloned().unwrap();

    let error = state
        .play_cards(1, "p1".to_owned(), vec![6, 20])
        .expect_err("a pair follower must not split its available pair");
    assert_eq!(error, "illegal follow");
    assert_eq!(state.hands.get(&1), Some(&hand_before_illegal_follow));
    assert_eq!(state.current_position, 1);

    let legal = state
        .play_cards(1, "p1".to_owned(), vec![6, 106])
        .expect("the intact pair should be accepted");
    assert_eq!(legal.cards, vec![6, 106]);
    assert_eq!(state.current_trick.len(), 2);
}

#[test]
fn successful_throw_uses_the_largest_component_for_bottom_score() {
    let state_hands = HashMap::from([
        (0, vec![2, 102, 202, 12, 112, 13]),
        (1, vec![3, 6, 7, 8, 10, 11]),
        (2, vec![103, 106, 107, 108, 110, 111]),
        (3, vec![203, 206, 207, 208, 210, 211]),
    ]);
    let mut state = state_with_hands(rules(3), vec![9], state_hands);

    let attempted = vec![2, 102, 202, 12, 112, 13];
    let lead = state
        .play_cards(0, "p0".to_owned(), attempted.clone())
        .expect("strong throw should be accepted");
    assert_eq!(lead.cards, attempted);

    for (position, cards) in [
        (1, vec![3, 6, 7, 8, 10, 11]),
        (2, vec![103, 106, 107, 108, 110, 111]),
        (3, vec![203, 206, 207, 208, 210, 211]),
    ] {
        state
            .play_cards(position, format!("p{position}"), cards)
            .expect("same-group throw follow should be accepted");
    }

    assert_eq!(state.phase, TractorPhase::Settlement);
    assert_eq!(state.last_trick_winner, Some(0));
    assert_eq!(state.bottom_multiplier, 6);
    // The lead contains a ten-point king pair and the bottom contains a
    // ten-point card: 20 trick points + 10 × the strongest triple multiplier.
    assert_eq!(state.collected_scores.get(&0), Some(&80));
}

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{TractorPhase, TractorRank, TractorSuit, WsTractorPlayedCards};
use tractor::{
    combo,
    game_state::{TractorGameState, TractorRules},
};
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
    assert_eq!(state.bottom_multiplier, 64);
    // The Titanic trick itself carries 35 points; the ten-point bottom is
    // multiplied by 64, so the winner collects 35 + 640 = 675.
    assert_eq!(state.collected_scores.get(&0), Some(&675));
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
    assert_eq!(state.bottom_multiplier, 8);
    // The lead contains a ten-point king pair and the bottom contains a
    // ten-point card: 20 trick points + 10 × the strongest triple multiplier.
    assert_eq!(state.collected_scores.get(&0), Some(&100));
}

#[test]
fn three_of_a_kind_is_its_own_shape_and_must_be_followed_before_a_pair() {
    let three_deck_rules = rules(3);
    assert_eq!(
        combo::classify(&[2, 102, 202], &three_deck_rules).map(|combo| combo.kind),
        Some(combo::ComboKind::Triple)
    );

    let state_hands = HashMap::from([
        (0, vec![2, 102, 202]),
        (1, vec![3, 103, 203, 4]),
        (2, vec![15, 16, 17]),
        (3, vec![28, 29, 30]),
    ]);
    let mut state = state_with_hands(three_deck_rules.clone(), Vec::new(), state_hands);
    state
        .play_cards(0, "p0".to_owned(), vec![2, 102, 202])
        .expect("three of a kind lead");
    assert_eq!(
        state
            .play_cards(1, "p1".to_owned(), vec![3, 103, 4])
            .expect_err("a held triple cannot be split into a pair and singleton"),
        "illegal follow"
    );
    state
        .play_cards(1, "p1".to_owned(), vec![3, 103, 203])
        .expect("higher three of a kind follow");

    let lead = combo::classify(&[2, 102, 202], &three_deck_rules).expect("triple lead");
    for hand in [vec![4, 104, 5], vec![4, 5, 6]] {
        let forced = combo::forced_follow(&hand, &lead, &three_deck_rules)
            .expect("fallback response to a triple");
        assert!(combo::follow_is_legal(
            &hand,
            &forced,
            &lead,
            &three_deck_rules
        ));
    }
}

#[test]
fn higher_three_of_a_kind_breaks_a_long_throw_to_that_component() {
    let attempted = vec![2, 102, 202, 12, 112, 13];
    let state_hands = HashMap::from([
        (0, attempted.clone()),
        (1, vec![3, 103, 203, 5, 6, 7]),
        (2, vec![15, 16, 17, 18, 19, 20]),
        (3, vec![28, 29, 30, 31, 32, 33]),
    ]);
    let mut state = state_with_hands(rules(3), Vec::new(), state_hands);

    let played = state
        .play_cards(0, "p0".to_owned(), attempted.clone())
        .expect("the referee reduces a challenged throw");
    assert_eq!(played.cards, vec![2, 102, 202]);
    assert_eq!(state.failed_throws.len(), 1);
    assert_eq!(state.failed_throws[0].attempted_cards, attempted);
    assert_eq!(state.failed_throws[0].played_cards, played.cards);
}

#[test]
fn void_opponent_trumps_do_not_make_a_plain_suit_throw_fail_on_lead() {
    let state_hands = HashMap::from([
        (0, vec![13, 113, 11, 111]),
        // This seat is void in spades and can ruff only after the throw has
        // been accepted. Its trump pair must not challenge the lead itself.
        (1, vec![1, 101, 15, 16]),
        (2, vec![28, 29, 30, 31]),
        (3, vec![41, 42, 43, 44]),
    ]);
    let mut state = state_with_hands(rules(2), Vec::new(), state_hands);
    let attempted = vec![13, 113, 11, 111];

    let played = state
        .play_cards(0, "p0".to_owned(), attempted.clone())
        .expect("a void opponent's trumps must not reject a plain-suit throw");

    assert_eq!(played.cards, attempted);
    assert!(state.failed_throws.is_empty());
    assert_eq!(state.hands.get(&0), Some(&Vec::new()));
}

#[test]
fn standard_bottom_multiplier_uses_winning_component_card_count_up_to_sixty_four() {
    let two_deck = rules(2);
    assert_eq!(combo::bottom_multiplier(&[2], &two_deck), 2);
    assert_eq!(combo::bottom_multiplier(&[2, 102], &two_deck), 4);
    assert_eq!(combo::bottom_multiplier(&[2, 102, 3, 103], &two_deck), 16);
    assert_eq!(
        combo::bottom_multiplier(&[2, 102, 3, 103, 4, 104], &two_deck),
        64
    );

    let three_deck = rules(3);
    assert_eq!(combo::bottom_multiplier(&[2, 102, 202], &three_deck), 8);
    assert_eq!(
        combo::bottom_multiplier(&[2, 102, 202, 3, 103, 203], &three_deck),
        64
    );
    assert_eq!(
        combo::bottom_multiplier(&[2, 102, 202, 12, 112, 13], &three_deck),
        8
    );
}

#[test]
fn follower_with_a_tractor_cannot_replace_it_with_non_consecutive_pairs() {
    let state_hands = HashMap::from([
        (0, vec![2, 102, 3, 103]),
        (1, vec![4, 104, 5, 105, 7, 107, 9, 109]),
        (2, vec![15, 115, 16, 116]),
        (3, vec![28, 128, 29, 129]),
    ]);
    let mut state = state_with_hands(rules(2), Vec::new(), state_hands);

    state
        .play_cards(0, "p0".to_owned(), vec![2, 102, 3, 103])
        .expect("tractor lead");
    assert_eq!(
        state
            .play_cards(1, "p1".to_owned(), vec![7, 107, 9, 109])
            .expect_err("a held tractor must be followed intact"),
        "illegal follow"
    );
    state
        .play_cards(1, "p1".to_owned(), vec![4, 104, 5, 105])
        .expect("consecutive pair follow");
}

#[test]
fn follower_with_a_titanic_cannot_replace_it_with_non_consecutive_triples() {
    let state_hands = HashMap::from([
        (0, vec![2, 102, 202, 3, 103, 203]),
        (1, vec![4, 104, 204, 5, 105, 205, 7, 107, 207, 9, 109, 209]),
        (2, vec![15, 115, 215, 16, 116, 216]),
        (3, vec![28, 128, 228, 29, 129, 229]),
    ]);
    let mut state = state_with_hands(rules(3), Vec::new(), state_hands);

    state
        .play_cards(0, "p0".to_owned(), vec![2, 102, 202, 3, 103, 203])
        .expect("Titanic lead");
    assert_eq!(
        state
            .play_cards(1, "p1".to_owned(), vec![7, 107, 207, 9, 109, 209])
            .expect_err("a held Titanic must be followed intact"),
        "illegal follow"
    );
    state
        .play_cards(1, "p1".to_owned(), vec![4, 104, 204, 5, 105, 205])
        .expect("consecutive triple follow");
}

#[test]
fn titanic_follow_uses_the_official_structure_priority_order() {
    let rules = rules(3);
    let lead = combo::classify(&[2, 102, 202, 3, 103, 203], &rules).expect("six-card Titanic lead");
    let cases = [
        // A tractor plus any two cards outranks two non-consecutive triples.
        (
            vec![4, 104, 5, 105, 7, 107, 207, 9, 109, 209],
            Some(vec![7, 107, 207, 9, 109, 209]),
        ),
        // Two triples outrank one triple, a pair and a singleton.
        (
            vec![4, 104, 204, 7, 107, 207, 9, 10],
            Some(vec![4, 104, 204, 7, 107, 9]),
        ),
        // One triple plus a separate pair outranks a triple plus singles.
        (
            vec![4, 104, 204, 7, 107, 9, 10],
            Some(vec![4, 104, 204, 7, 9, 10]),
        ),
        (vec![4, 104, 204, 7, 9, 10], None),
        (vec![4, 104, 7, 107, 9, 10], None),
        (vec![4, 104, 7, 9, 10, 11], None),
        (vec![4, 6, 8, 10, 12, 13], None),
    ];

    for (hand, lower_priority_play) in cases {
        if let Some(cards) = lower_priority_play {
            assert!(
                !combo::follow_is_legal(&hand, &cards, &lead, &rules),
                "lower-priority Titanic response was accepted: {cards:?}"
            );
        }
        let forced = combo::forced_follow(&hand, &lead, &rules)
            .expect("every six-card hand can follow a Titanic lead");
        assert!(
            combo::follow_is_legal(&hand, &forced, &lead, &rules),
            "automatic follow violated the Titanic priority: {forced:?}"
        );
    }
}

#[test]
fn ruffing_a_throw_uses_the_leads_required_structure_instead_of_the_highest_card() {
    let mut two_deck_rules = rules(2);
    two_deck_rules.trump_suit = Some(TractorSuit::HEART);
    let played = |position, cards| WsTractorPlayedCards {
        position,
        name: format!("p{position}"),
        cards,
    };

    // The lead contains one pair. Both ruffs contain a pair, so the higher
    // trump pair wins even though its unrelated singleton is lower.
    let pair_throw = [
        played(0, vec![4, 104, 5]),
        played(1, vec![17, 117, 20]),
        played(2, vec![18, 118, 16]),
    ];
    assert_eq!(combo::trick_winner(&pair_throw, &two_deck_rules), Some(2));

    // When the lead contains only singles, a pair in the ruff is broken into
    // singles. It may ruff, but a later play whose highest card is lower does
    // not cover it merely because that play also happens to contain a pair.
    let single_throw = [
        played(0, vec![4, 5, 6]),
        played(1, vec![17, 117, 20]),
        played(2, vec![18, 118, 16]),
    ];
    assert_eq!(combo::trick_winner(&single_throw, &two_deck_rules), Some(1));

    // The lead itself must be ranked by that same required structure. A high
    // unrelated singleton cannot protect its lower pair from a higher pair.
    let plain_pair_throw = [played(0, vec![3, 103, 13]), played(1, vec![4, 104, 2])];
    assert_eq!(
        combo::trick_winner(&plain_pair_throw, &two_deck_rules),
        Some(1)
    );

    let trump_pair_throw = [played(0, vec![16, 116, 26]), played(1, vec![17, 117, 15])];
    assert_eq!(
        combo::trick_winner(&trump_pair_throw, &two_deck_rules),
        Some(1)
    );

    // In a three-deck throw, an independent triple outranks a pair. Compare
    // the triples even when the lead happens to contain a much higher pair.
    let rules = rules(3);
    let triple_throw = [
        played(0, vec![2, 102, 202, 12, 112, 13]),
        played(1, vec![3, 103, 203, 11, 111, 4]),
    ];
    assert_eq!(combo::trick_winner(&triple_throw, &rules), Some(1));
}

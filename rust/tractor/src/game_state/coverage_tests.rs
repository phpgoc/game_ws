use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{TractorPhase, TractorRank, TractorSuit, WsTractorPlayedCards};
use ws_common::{CommonGameState, GameState};

use super::{
    TractorFailedThrow, TractorGameState, standard_bottom_card_count, tractor_suit_from_index,
};

fn state() -> TractorGameState {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    for position in 0..4 {
        common
            .lock()
            .unwrap()
            .add_player(position, position as u64 + 1, &format!("p{position}"));
    }
    let mut state = TractorGameState::from_common(common);
    state.phase = TractorPhase::Play;
    state.rules.target_rank = TractorRank::TWO;
    state.rules.final_target_rank = TractorRank::A;
    state
}

#[test]
fn utility_boundaries_cover_standard_bottom_and_suit_mapping() {
    assert_eq!(standard_bottom_card_count(2), 8);
    assert_eq!(standard_bottom_card_count(3), 6);
    assert_eq!(standard_bottom_card_count(99), 8);
    assert_eq!(tractor_suit_from_index(0), Some(TractorSuit::SPADE));
    assert_eq!(tractor_suit_from_index(1), Some(TractorSuit::HEART));
    assert_eq!(tractor_suit_from_index(2), Some(TractorSuit::CLUB));
    assert_eq!(tractor_suit_from_index(3), Some(TractorSuit::DIAMOND));
    assert_eq!(tractor_suit_from_index(9), None);
}

#[test]
fn every_two_is_permanent_trump_and_the_trump_suit_two_is_stronger() {
    let mut state = state();
    state.rules.trump_suit = Some(TractorSuit::HEART);

    for target_rank in [TractorRank::THREE, TractorRank::FIVE, TractorRank::A] {
        state.rules.target_rank = target_rank;
        for card in [1, 14, 27, 40] {
            assert!(super::is_trump_card(card, &state.rules));
        }
        assert!(
            super::tractor_card_position(14, &state.rules)
                > super::tractor_card_position(1, &state.rules)
        );
    }
}

#[test]
fn bury_and_declaration_reject_wrong_phase_cards_and_jokers() {
    let mut state = state();
    state.dealer_position = 0;
    state.hands.insert(0, vec![2, 15, 53]);
    assert!(state.bury_bottom(0, vec![2]).is_err());
    state.phase = TractorPhase::Bury;
    state.rules.bottom_card_count = 2;
    assert!(state.bury_bottom(0, vec![2]).is_err());
    assert!(state.bury_bottom(1, vec![2, 15]).is_err());
    assert!(state.bury_bottom(0, vec![2, 99]).is_err());

    state.phase = TractorPhase::Deal;
    state.round_index = 0;
    state.rules.target_rank = TractorRank::TWO;
    assert!(state.declare_trump(0, Vec::new()).is_err());
    assert!(state.declare_trump(0, vec![99]).is_err());
    assert!(state.declare_trump(0, vec![53]).is_err());
    state.hands.insert(0, vec![2, 15]);
    assert!(state.declare_trump(0, vec![2, 15]).is_err());
    assert_eq!(state.auto_declaration_cards(0), None);
    assert_eq!(state.dealer_bottom_cards(), None);
}

#[test]
fn phase_guards_reject_bury_deal_and_non_dealer_trump_actions() {
    let mut state = state();
    assert_eq!(state.choose_auto_bury(), None);
    assert_eq!(state.choose_timeout_bury(), None);
    assert!(state.deal_next_card().is_none());

    state.phase = TractorPhase::Bury;
    state.round_index = 1;
    state.dealer_position = 0;
    assert!(state.select_dealer_trump(1, TractorSuit::SPADE).is_err());
}

#[test]
fn first_round_fallback_declaration_always_selects_a_suited_level_card() {
    let mut state = state();
    state.phase = TractorPhase::Deal;
    state.round_index = 0;
    state.rules.target_rank = TractorRank::THREE;
    state.rules.trump_suit = None;
    state.deal_queue = std::collections::VecDeque::from([(0, 1)]);
    state.total_deal_count = 1;
    state.hands = HashMap::from([
        (0, Vec::new()),
        (1, vec![2]),
        (2, Vec::new()),
        (3, Vec::new()),
    ]);

    let (_, _, finished, declaration) = state.deal_next_card().expect("final deal card");

    assert!(finished);
    let declaration = declaration.expect("fallback declaration");
    assert_eq!(declaration.position, 1);
    assert_eq!(declaration.cards, vec![2]);
    assert_eq!(declaration.target_rank, TractorRank::THREE);
    assert_eq!(state.rules.trump_suit, Some(TractorSuit::SPADE));
    assert_eq!(state.dealer_position, 1);
    assert_eq!(state.phase, TractorPhase::Bury);
}

#[test]
fn settlement_advances_to_the_first_winning_farmer_and_starts_a_round() {
    let mut state = state();
    assert!(state.advance_after_settlement().is_err());
    state.phase = TractorPhase::Settlement;
    state.dealer_position = 0;
    state.rules.target_rank = TractorRank::THREE;
    state.collected_scores = HashMap::from([(1, 80)]);
    state.rules.attacking_win_score = 80;
    assert!(state.advance_after_settlement().expect("next round"));
    assert_eq!(state.dealer_position, 1);
    assert_eq!(state.rules.target_rank, TractorRank::FOUR);
    assert_eq!(state.phase, TractorPhase::Deal);
}

#[test]
fn play_rejects_empty_invalid_and_missing_cards_and_reports_last_failed_throw() {
    let mut state = state();
    state.current_position = 0;
    state.hands.insert(0, vec![1]);
    assert!(state.play_cards(0, "p0".to_owned(), Vec::new()).is_err());
    assert!(state.play_cards(1, "p1".to_owned(), vec![1]).is_err());
    assert!(state.play_cards(0, "p0".to_owned(), vec![1, 2]).is_err());
    assert!(state.play_cards(0, "p0".to_owned(), vec![2]).is_err());

    state.failed_throws.push(TractorFailedThrow {
        position: 0,
        attempted_cards: vec![13, 113, 11, 111],
        played_cards: vec![11, 111],
        play_sequence: 0,
    });
    state.play_count = 1;
    let event = state
        .last_failed_throw_event(0)
        .expect("failed throw event");
    assert_eq!(event.attempted_cards, vec![13, 113, 11, 111]);
    assert!(state.last_failed_throw_event(1).is_none());
}

#[test]
fn auto_play_handles_void_follow_and_away_forced_points() {
    let mut state = state();
    state.current_position = 0;
    state.rules.trump_suit = Some(TractorSuit::SPADE);
    state.hands.insert(0, vec![4]);
    assert_eq!(state.choose_away_play(0), Some(vec![4]));

    state.current_position = 1;
    state.current_trick = vec![WsTractorPlayedCards {
        position: 0,
        name: "p0".to_owned(),
        cards: vec![2],
    }];
    state.hands.insert(1, vec![15]);
    assert_eq!(state.choose_auto_play(1), Some(vec![15]));
}

#[test]
fn automatic_plays_fall_back_when_an_incomplete_hand_cannot_follow() {
    let mut state = state();
    state.current_trick = vec![WsTractorPlayedCards {
        position: 0,
        name: "p0".to_owned(),
        cards: vec![3, 103],
    }];
    state.hands.insert(1, vec![4]);

    assert!(state.choose_away_play(1).is_none());
    assert!(state.choose_auto_play(1).is_none());
}

#[test]
fn game_state_accepts_seats_only_before_play_begins() {
    let mut state = state();
    state.phase = TractorPhase::Start;
    assert!(GameState::can_accept_players(&state));
    state.phase = TractorPhase::Play;
    assert!(!GameState::can_accept_players(&state));
    assert!(Arc::ptr_eq(
        &state.base,
        &GameState::shared_common_state(&state)
    ));
}

#[test]
fn state_helpers_cover_missing_seats_empty_hands_and_partner_turn_order() {
    let mut state = state();
    state.current_position = 0;
    state.hands.insert(0, Vec::new());
    assert_eq!(state.choose_away_play(0), None);
    assert_eq!(state.choose_auto_play(0), None);
    assert_eq!(state.preferred_dealer_trump_suit(), TractorSuit::SPADE);

    state.current_trick = vec![WsTractorPlayedCards {
        position: 0,
        name: "p0".to_owned(),
        cards: vec![1],
    }];
    let lead = state.lead_combo().expect("single lead combo");
    assert!(state.legal_follows(99, &lead).is_empty());
    assert!(state.partner_still_to_play(0));

    state.current_trick.extend([
        WsTractorPlayedCards {
            position: 1,
            name: "p1".to_owned(),
            cards: vec![2],
        },
        WsTractorPlayedCards {
            position: 2,
            name: "p2".to_owned(),
            cards: vec![3],
        },
    ]);
    assert!(!state.partner_still_to_play(0));

    let mut incomplete =
        TractorGameState::from_common(Arc::new(Mutex::new(CommonGameState::new())));
    assert!(incomplete.deal_new_round(state.rules.clone()).is_err());
}

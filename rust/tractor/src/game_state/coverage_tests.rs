use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use share_type_public::{TractorPhase, TractorRank, TractorSuit, WsTractorPlayedCards};
use ws_common::{CommonGameState, GameState};

use super::{
    TractorFailedThrow, TractorGameState, adjusted_bottom_card_count, removed_tractor_ranks,
    tractor_suit_from_index,
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
fn utility_boundaries_cover_bottom_fallback_and_suit_mapping() {
    assert_eq!(adjusted_bottom_card_count(10, 0, 4, 2), None);
    assert_eq!(adjusted_bottom_card_count(10, 4, 10, 10), None);
    assert_eq!(adjusted_bottom_card_count(13, 4, 8, 2), Some(9));
    assert_eq!(removed_tractor_ranks(0), Vec::<TractorRank>::new());
    assert_eq!(removed_tractor_ranks(usize::MAX).len(), 9);
    assert_eq!(tractor_suit_from_index(0), Some(TractorSuit::SPADE));
    assert_eq!(tractor_suit_from_index(1), Some(TractorSuit::HEART));
    assert_eq!(tractor_suit_from_index(2), Some(TractorSuit::CLUB));
    assert_eq!(tractor_suit_from_index(3), Some(TractorSuit::DIAMOND));
    assert_eq!(tractor_suit_from_index(9), None);
}

#[test]
fn bury_and_declaration_reject_wrong_phase_cards_and_jokers() {
    let mut state = state();
    state.dealer_position = 0;
    state.hands.insert(0, vec![2, 15, 53]);
    assert!(state.bury_bottom(0, vec![2]).is_err());
    state.phase = TractorPhase::Bury;
    state.rules.bottom_card_count = 2;
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
fn settlement_advances_to_the_first_winning_farmer_and_starts_a_round() {
    let mut state = state();
    assert!(state.advance_after_settlement().is_err());
    state.phase = TractorPhase::Settlement;
    state.dealer_position = 0;
    state.collected_scores = HashMap::from([(1, 80)]);
    state.rules.blood_start_score = 80;
    assert!(state.advance_after_settlement().expect("next round"));
    assert_eq!(state.dealer_position, 1);
    assert_eq!(state.rules.target_rank, TractorRank::THREE);
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

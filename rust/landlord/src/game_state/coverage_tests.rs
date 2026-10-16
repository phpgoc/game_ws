use std::sync::{Arc, Mutex};

use ws_common::{CommonGameState, GameState};

use super::{LandlordGameState, LandlordLoopState};

#[test]
fn game_state_wrapper_rejects_new_players_and_preserves_the_shared_common_state() {
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    common.lock().unwrap().mark_disconnected(0);
    let loop_state = Arc::new(Mutex::new(LandlordLoopState::new(Arc::clone(&common))));
    let game_state = LandlordGameState::from_loop_state(Arc::clone(&loop_state));

    assert!(!game_state.can_accept_players());
    assert!(Arc::ptr_eq(&game_state.shared_common_state(), &common));
    assert!(loop_state.lock().unwrap().has_disconnected_players());
}

#[test]
fn settlement_without_a_landlord_keeps_scores_unchanged() {
    let common = Arc::new(Mutex::new(CommonGameState::default()));
    let mut state = LandlordLoopState::new(common);
    state.score = 3;

    let summary = state.apply_settlement_scores(true);

    assert_eq!(summary.round_score, 3);
    assert!(state.player_scores.is_empty());
}

fn three_player_state() -> LandlordLoopState {
    let mut common = CommonGameState::default();
    // 插入顺序不能影响生产的按座位排序发牌。
    for position in [2, 0, 1] {
        common.add_player(position, position as u64 + 1, "player");
    }
    LandlordLoopState::new(Arc::new(Mutex::new(common)))
}

#[test]
fn seeded_production_deal_preserves_the_original_shuffle_and_card_ledger() {
    let expected = [
        1, 16, 53, 13, 8, 22, 44, 6, 38, 12, 33, 2, 36, 5, 31, 46, 49, 47, 14, 7, 15, 34, 50, 19,
        39, 3, 10, 30, 41, 54, 24, 23, 9, 32, 35, 26, 25, 48, 40, 29, 28, 37, 51, 17, 21, 27, 52,
        42, 20, 45, 4, 43, 18, 11,
    ];
    assert_eq!(LandlordLoopState::shuffle_with_seed(42), expected);
    for seed in [0, 1, 42, 956001, u64::MAX] {
        let mut state = three_player_state();
        state.generate_card_with_seed(seed);
        let deck = LandlordLoopState::shuffle_with_seed(seed);
        for seat in 0..3 {
            let mut hand = deck[seat * 17..(seat + 1) * 17].to_vec();
            hand.sort_unstable();
            assert_eq!(state.hands[&seat], hand);
        }
        assert_eq!(state.hidden_cards, deck[51..]);
        let mut ledger = state.hidden_cards.clone();
        ledger.extend(state.hands.values().flatten());
        ledger.sort_unstable();
        assert_eq!(ledger, (1..=54).collect::<Vec<_>>());
        let previous = state.hands.clone();
        state.generate_card_with_seed(seed);
        assert_eq!(previous, state.hands);
    }
}

#[test]
fn repeated_redeals_rotate_the_caller_and_preserve_cumulative_scores() {
    let mut state = three_player_state();
    state.player_scores = [(0, 12), (1, -6), (2, -6)].into();
    for round in 1..=7 {
        state.call_history = vec![(0, 0), (1, 0), (2, 0)];
        state.ai_bomb_signal_used = true;
        state.redeal();
        assert_eq!(state.call_position, round % 3);
        assert_eq!(state.current_position, round % 3);
        assert_eq!(state.phase, share_type_public::LandlordPhase::Start);
        assert!(state.call_history.is_empty() && state.play_history.is_empty());
        assert!(!state.ai_bomb_signal_used);
        assert_eq!(state.player_scores, [(0, 12), (1, -6), (2, -6)].into());
        state.generate_card_with_seed(956000 + round as u64);
        state.next_phase();
        assert_eq!(state.current_position, round % 3);
        assert!(state.hands.values().all(|hand| hand.len() == 17));
    }
}

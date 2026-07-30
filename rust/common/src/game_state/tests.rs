use std::sync::{Arc, Mutex};

use super::{CommonGameState, GameState, SharedGameState};

#[test]
fn shared_state_trait_delegates_all_common_roster_operations() {
    let common = Arc::new(Mutex::new(CommonGameState::new()));
    let mut state = SharedGameState::from_common(Arc::clone(&common));

    assert!(state.can_accept_players());
    assert!(state.can_join_players());
    assert!(!state.position_reserved_for_join(0));
    assert!(!state.action_received());
    assert!(!state.has_disconnected_players());
    assert!(!state.is_paused());
    assert!(!state.stop_requested());

    state.add_player(0, 10, "alice");
    state.add_player(1, 11, "bob");
    state.set_avatar(0, "alice-avatar");
    state.set_avatar(1, "bob-avatar");
    state.set_avatar(2, "");
    assert_eq!(state.player_name(0), "alice");
    assert_eq!(state.player_avatar(1), "bob-avatar");
    assert_eq!(state.player_name(2), "");
    assert_eq!(state.player_avatar(2), "");
    assert_eq!(state.players().len(), 2);

    state.set_action_received(true);
    state.set_turn_countdown(30);
    assert!(state.action_received());
    assert_eq!(state.turn_countdown(), 30);
    state.pause();
    assert!(state.is_paused());
    state.resume();
    assert!(!state.is_paused());

    state.mark_away(0);
    state.mark_disconnected(1);
    state.mark_ai_position(0);
    state.mark_ai_takeover_position(0);
    state.set_member_position(0, true);
    assert!(state.is_away(0));
    assert!(state.is_disconnected(1));
    assert!(state.has_disconnected_players());
    assert!(state.is_ai_position(0));
    assert!(state.is_ai_takeover_position(0));
    assert!(state.is_member_position(0));

    state.clear_ai_takeover_position(0);
    assert!(!state.is_ai_takeover_position(0));
    state.mark_ai_takeover_position(0);
    state.clear_away_position(0);
    assert!(!state.is_away(0));
    assert!(!state.is_ai_takeover_position(0));
    state.mark_away(0);
    state.mark_ai_takeover_position(0);
    state.clear_away();
    assert!(!state.is_away(0));
    assert!(!state.is_ai_takeover_position(0));

    state.clear_disconnected_position(1);
    assert!(!state.is_disconnected(1));
    assert!(!state.has_disconnected_players());
    state.set_member_position(0, false);
    assert!(!state.is_member_position(0));

    state.mark_away(0);
    state.mark_disconnected(0);
    state.mark_ai_takeover_position(0);
    state.set_member_position(0, true);
    state.swap_player(0, 1);
    assert_eq!(state.player_name(0), "bob");
    assert_eq!(state.player_name(1), "alice");
    assert!(state.is_away(1));
    assert!(state.is_disconnected(1));
    assert!(state.is_ai_position(1));
    assert!(state.is_ai_takeover_position(1));
    assert!(state.is_member_position(1));

    state.remove_player(1);
    assert_eq!(state.player_name(1), "");
    assert!(!state.is_away(1));
    assert!(!state.is_disconnected(1));
    assert!(!state.is_ai_position(1));
    assert!(!state.is_ai_takeover_position(1));
    assert!(!state.is_member_position(1));

    state.pause();
    state.request_stop();
    assert!(state.stop_requested());
    assert!(!state.is_paused());
    assert!(Arc::ptr_eq(&state.shared_common_state(), &common));
}

#[test]
fn common_state_collection_operations_return_set_mutation_results() {
    let mut state = CommonGameState::new();

    assert!(state.mark_away(4));
    assert!(!state.mark_away(4));
    assert!(state.mark_disconnected(4));
    assert!(!state.mark_disconnected(4));
    assert!(state.mark_ai_position(4));
    assert!(!state.mark_ai_position(4));
    assert!(state.mark_ai_takeover_position(4));
    assert!(!state.mark_ai_takeover_position(4));
    assert!(state.set_member_position(4, true));
    assert!(!state.set_member_position(4, true));
    assert!(state.clear_ai_takeover_position(4));
    assert!(!state.clear_ai_takeover_position(4));
    assert!(state.set_member_position(4, false));
    assert!(!state.set_member_position(4, false));

    state.swap_player(8, 9);
    assert!(!state.is_away(8));
    assert!(!state.is_away(9));
}

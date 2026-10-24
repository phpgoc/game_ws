use super::DominoesGameHandler;

#[path = "tests/lobby.rs"]
mod lobby;

#[test]
fn game_handler_module_has_external_tests() {
    use share_type_public::GameId;
    use ws_common::GameHandler;

    assert_eq!(
        super::DominoesGameHandler::default().game_id(),
        GameId::DOMINOES
    );
}

#[test]
fn human_actions_are_blocked_while_room_is_paused_or_stopping() {
    use std::sync::{Arc, Mutex};

    use share_type_public::{DominoesNoPlayableTiles, DominoesRule};
    use ws_common::CommonGameState;

    let mut common = CommonGameState::new();
    common.add_player(0, 1, "P0");
    common.add_player(1, 2, "P1");
    common.add_player(2, 3, "P2");
    let base = Arc::new(Mutex::new(common));
    let mut state = crate::core::DominoesRoundState::new_with_seed(
        base,
        DominoesRule::Simple,
        DominoesNoPlayableTiles::PassWithoutDraw,
        35,
        1,
    )
    .expect("state");
    state.start_new_game().expect("start");

    assert!(DominoesGameHandler::human_action_allowed(&state, 0));

    state.base.lock().expect("common").paused = true;
    assert!(!DominoesGameHandler::human_action_allowed(&state, 0));

    state.base.lock().expect("common").paused = false;
    state.base.lock().expect("common").stop_requested = true;
    assert!(!DominoesGameHandler::human_action_allowed(&state, 0));
}

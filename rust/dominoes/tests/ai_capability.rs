use dominoes::game::DominoesGameHandler;
use ws_common::GameHandler;

#[test]
fn dominoes_server_advertises_ai_players() {
    assert!(DominoesGameHandler::default().supports_ai_players());
}

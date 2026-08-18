use tractor::game::TractorGameHandler;
use ws_common::GameHandler;

#[test]
fn tractor_server_advertises_ai_players() {
    assert!(TractorGameHandler::default().supports_ai_players());
}

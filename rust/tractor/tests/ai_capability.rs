use tractor::game::TractorGameHandler;
use ws_common::GameHandler;

#[test]
fn ai_players_follow_the_ai_feature() {
    assert_eq!(
        TractorGameHandler::default().supports_ai_players(),
        cfg!(feature = "ai")
    );
}

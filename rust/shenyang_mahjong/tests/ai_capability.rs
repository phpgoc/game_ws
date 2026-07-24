use shenyang_mahjong::game::ShenyangMahjongGameHandler;
use ws_common::GameHandler;

#[test]
fn ai_players_follow_the_ai_feature() {
    assert_eq!(
        ShenyangMahjongGameHandler::default().supports_ai_players(),
        cfg!(feature = "ai")
    );
}

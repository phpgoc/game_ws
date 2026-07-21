use shenyang_mahjong::game::ShenyangMahjongGameHandler;
use ws_common::GameHandler;

#[test]
fn ai_players_are_available_only_in_official_builds() {
    assert_eq!(
        ShenyangMahjongGameHandler::default().supports_ai_players(),
        cfg!(feature = "official")
    );
}

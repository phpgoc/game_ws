use landlord::game::LandlordGameHandler;
use ws_common::GameHandler;

#[test]
fn ai_players_follow_the_official_feature() {
    assert_eq!(
        LandlordGameHandler::default().supports_ai_players(),
        cfg!(feature = "official")
    );
}

use dominoes::game::DominoesGameHandler;
use ws_common::GameHandler;

#[test]
fn public_dominoes_server_does_not_advertise_ai() {
    #[cfg(not(feature = "official"))]
    assert!(!DominoesGameHandler::default().supports_ai_players());
}

#[cfg(feature = "official")]
#[test]
fn official_dominoes_server_advertises_ai() {
    assert!(DominoesGameHandler::default().supports_ai_players());
}

#[test]
fn game_handler_module_has_external_tests() {
    use share_type_public::GameId;
    use ws_common::GameHandler;

    assert_eq!(
        super::DominoesGameHandler::default().game_id(),
        GameId::DOMINOES
    );
}

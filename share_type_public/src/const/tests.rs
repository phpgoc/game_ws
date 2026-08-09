use super::GameId;

#[test]
fn upgrade_has_a_stable_protocol_id() {
    assert_eq!(i32::from(GameId::UPGRADE), 10);
    assert_eq!(GameId::try_from(10), Ok(GameId::UPGRADE));
}

#[test]
fn dominoes_has_a_stable_protocol_id() {
    assert_eq!(i32::from(GameId::DOMINOES), 11);
    assert_eq!(GameId::try_from(11), Ok(GameId::DOMINOES));
}

#[test]
fn unknown_game_ids_are_rejected() {
    assert_eq!(GameId::try_from(12), Err(()));
}

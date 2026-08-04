use super::*;

#[test]
fn upgrade_accepts_only_three_to_six_decks() {
    for count in 3..=6 {
        assert_eq!(UpgradeDeckCount::new(count).unwrap().get(), count);
    }
    assert_eq!(UpgradeDeckCount::new(2), Err(DeckCountError(2)));
    assert_eq!(UpgradeDeckCount::new(7), Err(DeckCountError(7)));
}

#[test]
fn upgrade_server_has_an_independent_service_name() {
    assert_eq!(server::UPGRADE_SERVICE_NAME, "upgrade");
}

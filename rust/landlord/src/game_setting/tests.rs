use super::build_landlord_settings;

#[test]
fn landlord_settings_keep_the_fixed_three_player_contract() {
    let (settings, params) = build_landlord_settings();
    assert_eq!(settings.min_players, 3);
    assert_eq!(settings.max_players, 3);
    assert!(settings.values.is_empty());
    assert!(params.is_empty());
}

use super::build_landlord_settings;
use share_type_public::GameParam;

#[test]
fn landlord_settings_keep_the_fixed_three_player_contract() {
    let (settings, params) = build_landlord_settings();
    assert_eq!(settings.min_players, 3);
    assert_eq!(settings.max_players, 3);
    assert_eq!(settings.values["settlement_time"], 15);
    let GameParam::Range(settlement_time) = &params["settlement_time"] else {
        panic!("settlement time must be a range");
    };
    assert_eq!(
        (
            settlement_time.default,
            settlement_time.min,
            settlement_time.max
        ),
        (15, 1, 30)
    );
}

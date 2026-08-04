use share_type_public::GameParam;

use super::*;

#[test]
fn upgrade_settings_expose_only_three_to_six_decks_and_play_time() {
    let (settings, params) = build_upgrade_settings();

    assert_eq!((settings.min_players, settings.max_players), (4, 4));
    assert_eq!(settings.values.len(), 2);
    assert_eq!(settings.values[KEY_DECK_COUNT], 0);
    assert_eq!(settings.values[KEY_PLAY_TIME], 30);
    assert!(!params.keys().any(|key| key.contains("blood")));
    assert!(!params.keys().any(|key| key.contains("tribute")));

    let GameParam::Enum(decks) = &params[KEY_DECK_COUNT] else {
        panic!("deck count must be an enum");
    };
    assert_eq!(decks.options, ["3", "4", "5", "6"]);

    let GameParam::Range(play_time) = &params[KEY_PLAY_TIME] else {
        panic!("play time must be a range");
    };
    assert_eq!(
        (play_time.default, play_time.min, play_time.max),
        (30, 20, 40)
    );
}

#[test]
fn deck_setting_index_is_bounded_to_upgrade_range() {
    assert_eq!(deck_count_from_setting(-1).get(), 3);
    assert_eq!(deck_count_from_setting(0).get(), 3);
    assert_eq!(deck_count_from_setting(3).get(), 6);
    assert_eq!(deck_count_from_setting(99).get(), 6);
}

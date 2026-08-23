use share_type_public::GameParam;

use super::*;

#[test]
fn upgrade_settings_expose_decks_timing_and_score_progression() {
    let (settings, params) = build_upgrade_settings();

    assert_eq!((settings.min_players, settings.max_players), (4, 4));
    assert_eq!(settings.values.len(), 7);
    assert_eq!(settings.values[KEY_DECK_COUNT], 0);
    assert_eq!(settings.values[KEY_PLAY_TIME], 30);
    assert_eq!(settings.values[KEY_REMOVED_RANK_COUNT], 0);
    assert_eq!(settings.values[KEY_ATTACKING_WIN_SCORE], 80);
    assert_eq!(settings.values[KEY_SCORE_PER_LEVEL], 40);
    assert_eq!(settings.values[KEY_SHUTOUT_BONUS_LEVELS], 1);
    assert_eq!(settings.values[KEY_SETTLEMENT_TIME], 3);
    assert!(!settings.values.contains_key(KEY_FIRST_DEAL_TIME));
    assert!(!settings.values.contains_key(KEY_DEAL_TIME));
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

    let GameParam::Range(removed_ranks) = &params[KEY_REMOVED_RANK_COUNT] else {
        panic!("removed rank count must be a range");
    };
    assert_eq!(
        (removed_ranks.default, removed_ranks.min, removed_ranks.max),
        (0, 0, 6)
    );

    let GameParam::Range(attacking_win_score) = &params[KEY_ATTACKING_WIN_SCORE] else {
        panic!("attacking win score must be a range");
    };
    assert_eq!(
        (
            attacking_win_score.default,
            attacking_win_score.min,
            attacking_win_score.max
        ),
        (80, 20, 400)
    );
    let GameParam::Range(score_per_level) = &params[KEY_SCORE_PER_LEVEL] else {
        panic!("score per level must be a range");
    };
    assert_eq!(
        (
            score_per_level.default,
            score_per_level.min,
            score_per_level.max
        ),
        (40, 5, 200)
    );

    let GameParam::Range(settlement_time) = &params[KEY_SETTLEMENT_TIME] else {
        panic!("settlement time must be a range");
    };
    assert_eq!(
        (
            settlement_time.default,
            settlement_time.min,
            settlement_time.max
        ),
        (3, 1, 30)
    );
}

#[test]
fn deck_setting_index_is_bounded_to_upgrade_range() {
    assert_eq!(deck_count_from_setting(-1).get(), 3);
    assert_eq!(deck_count_from_setting(0).get(), 3);
    assert_eq!(deck_count_from_setting(3).get(), 6);
    assert_eq!(deck_count_from_setting(99).get(), 6);
}

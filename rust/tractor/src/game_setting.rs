use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

pub const KEY_AI_ACTION_TIME: &str = "ai_action_time";

pub const KEY_AWAY_TIME: &str = "away_time";
pub const KEY_ATTACKING_WIN_SCORE: &str = "attacking_win_score";
pub const KEY_DEAL_TIME: &str = "deal_time";
pub const KEY_DECK_COUNT: &str = "deck_count";
pub const KEY_FIRST_DEAL_TIME: &str = "first_deal_time";
pub const KEY_PLAY_TIME: &str = "play_time";
pub const KEY_SCORE_PER_LEVEL: &str = "score_per_level";
pub const KEY_SHUTOUT_BONUS_LEVELS: &str = "shutout_bonus_levels";
pub const KEY_SETTLEMENT_TIME: &str = "settlement_time";
pub const KEY_TARGET_RANK: &str = "target_rank";

pub fn build_tractor_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
        (
            KEY_DECK_COUNT.into(),
            GameParam::Enum(GameParamEnum {
                default: 0,
                options: vec!["2".into(), "3".into()],
            }),
        ),
        (
            KEY_ATTACKING_WIN_SCORE.into(),
            GameParam::Range(GameParamRange {
                default: 80,
                min: 20,
                max: 400,
            }),
        ),
        (
            KEY_SCORE_PER_LEVEL.into(),
            GameParam::Range(GameParamRange {
                default: 40,
                min: 5,
                max: 200,
            }),
        ),
        (
            KEY_SHUTOUT_BONUS_LEVELS.into(),
            GameParam::Range(GameParamRange {
                default: 1,
                min: 0,
                max: 3,
            }),
        ),
        (
            KEY_TARGET_RANK.into(),
            GameParam::Enum(GameParamEnum {
                default: 11,
                options: vec![
                    "3".into(),
                    "4".into(),
                    "5".into(),
                    "6".into(),
                    "7".into(),
                    "8".into(),
                    "9".into(),
                    "10".into(),
                    "J".into(),
                    "Q".into(),
                    "K".into(),
                    "A".into(),
                ],
            }),
        ),
        (
            KEY_PLAY_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 30,
                min: 20,
                max: 40,
            }),
        ),
        (
            KEY_SETTLEMENT_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 1,
                max: 30,
            }),
        ),
    ]
    .into_iter()
    .collect();

    let mut settings = GameSettings::new(4, 4);
    for (key, param) in &params {
        match param {
            GameParam::Range(range) => {
                settings.values.insert(key.clone(), range.default);
            }
            GameParam::Enum(item) => {
                settings.values.insert(key.clone(), item.default as i32);
            }
        }
    }

    (settings, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_settings_start_at_three_and_keep_timing_internal() {
        let (settings, params) = build_tractor_settings();
        assert!(!settings.values.contains_key(KEY_FIRST_DEAL_TIME));
        assert!(!settings.values.contains_key(KEY_DEAL_TIME));
        assert!(!settings.values.contains_key(KEY_AI_ACTION_TIME));
        assert!(!settings.values.contains_key(KEY_AWAY_TIME));
        assert_eq!(settings.values[KEY_SETTLEMENT_TIME], 5);
        let GameParam::Range(settlement_time) = &params[KEY_SETTLEMENT_TIME] else {
            panic!("settlement time must be a range");
        };
        assert_eq!(
            (
                settlement_time.default,
                settlement_time.min,
                settlement_time.max
            ),
            (5, 1, 30)
        );
        assert_eq!(settings.values[KEY_PLAY_TIME], 30);
        assert!(!settings.values.contains_key("removed_rank_count"));
        assert_eq!(settings.values[KEY_ATTACKING_WIN_SCORE], 80);
        assert_eq!(settings.values[KEY_SCORE_PER_LEVEL], 40);
        assert_eq!(settings.values[KEY_SHUTOUT_BONUS_LEVELS], 1);
        assert!(!settings.values.contains_key("bottom_card_count"));
        assert!(!params.contains_key("bottom_card_count"));
        let GameParam::Enum(deck_count) = &params[KEY_DECK_COUNT] else {
            panic!("deck count must be an enum");
        };
        assert_eq!(deck_count.default, 0);
        assert_eq!(deck_count.options, ["2", "3"]);
        let GameParam::Enum(target_rank) = &params[KEY_TARGET_RANK] else {
            panic!("target rank must be an enum");
        };
        assert_eq!(target_rank.default, 11);
        assert_eq!(target_rank.options.first().map(String::as_str), Some("3"));
        assert!(!target_rank.options.iter().any(|rank| rank == "2"));
        let GameParam::Range(play_time) = &params[KEY_PLAY_TIME] else {
            panic!("play time must be a range");
        };
        assert_eq!(
            (play_time.default, play_time.min, play_time.max),
            (30, 20, 40)
        );
    }
}

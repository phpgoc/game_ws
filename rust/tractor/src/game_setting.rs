use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

pub const KEY_AWAY_TIME: &str = "away_time";
pub const KEY_AI_ACTION_TIME: &str = "ai_action_time";
pub const KEY_BLOOD_ENABLED: &str = "blood_enabled";
pub const KEY_BLOOD_SCORE_PER_UNIT: &str = "blood_score_per_unit";
pub const KEY_BLOOD_START_SCORE: &str = "blood_start_score";
pub const KEY_BOTTOM_CARD_COUNT: &str = "bottom_card_count";
pub const KEY_DECK_COUNT: &str = "deck_count";
pub const KEY_DEAL_TIME: &str = "deal_time";
pub const KEY_FIRST_DEAL_TIME: &str = "first_deal_time";
pub const KEY_PLAY_TIME: &str = "play_time";
pub const KEY_REMOVED_RANK_COUNT: &str = "removed_rank_count";
pub const KEY_SETTLEMENT_TIME: &str = "settlement_time";
pub const KEY_TARGET_RANK: &str = "target_rank";

pub fn build_tractor_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
        (
            KEY_DECK_COUNT.into(),
            GameParam::Range(GameParamRange {
                default: 2,
                min: 2,
                max: 4,
            }),
        ),
        (
            // Total duration of the first round's incremental deal. It is
            // intentionally slow so players have time to declare/counter trump.
            KEY_FIRST_DEAL_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 15_000,
                min: 1_000,
                max: 60_000,
            }),
        ),
        (
            // Later rounds already have an established dealer and use a faster deal.
            KEY_DEAL_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 3_000,
                min: 500,
                max: 30_000,
            }),
        ),
        (
            KEY_BLOOD_ENABLED.into(),
            GameParam::Enum(GameParamEnum {
                default: 1,
                options: vec!["off".into(), "on".into()],
            }),
        ),
        (
            KEY_BLOOD_START_SCORE.into(),
            GameParam::Range(GameParamRange {
                default: 80,
                min: 5,
                max: 400,
            }),
        ),
        (
            KEY_BLOOD_SCORE_PER_UNIT.into(),
            GameParam::Range(GameParamRange {
                default: 40,
                min: 5,
                max: 200,
            }),
        ),
        (
            KEY_TARGET_RANK.into(),
            GameParam::Enum(GameParamEnum {
                default: 12,
                options: vec![
                    "2".into(),
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
            KEY_REMOVED_RANK_COUNT.into(),
            GameParam::Range(GameParamRange {
                default: 0,
                min: 0,
                max: 9,
            }),
        ),
        (
            KEY_BOTTOM_CARD_COUNT.into(),
            GameParam::Range(GameParamRange {
                default: 8,
                min: 8,
                max: 32,
            }),
        ),
        (
            KEY_AI_ACTION_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 1_000,
                min: 20,
                max: 3_000,
            }),
        ),
        (
            KEY_AWAY_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 2,
                max: 20,
            }),
        ),
        (
            KEY_PLAY_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 30,
                min: 5,
                max: 120,
            }),
        ),
        (
            KEY_SETTLEMENT_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 2,
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
    fn first_deal_is_slower_and_compact_deck_is_a_count() {
        let (settings, params) = build_tractor_settings();
        assert!(settings.values[KEY_FIRST_DEAL_TIME] > settings.values[KEY_DEAL_TIME]);
        assert_eq!(settings.values[KEY_REMOVED_RANK_COUNT], 0);
        assert_eq!(settings.values[KEY_AI_ACTION_TIME], 1_000);
        let GameParam::Range(removed) = &params[KEY_REMOVED_RANK_COUNT] else {
            panic!("removed rank count must be a range");
        };
        assert_eq!((removed.min, removed.max), (0, 9));
    }
}

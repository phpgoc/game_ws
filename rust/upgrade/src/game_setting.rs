use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

pub const KEY_AWAY_TIME: &str = "away_time";
pub const KEY_BLOOD_ENABLED: &str = "blood_enabled";
pub const KEY_BLOOD_SCORE_PER_UNIT: &str = "blood_score_per_unit";
pub const KEY_BLOOD_START_SCORE: &str = "blood_start_score";
pub const KEY_BOTTOM_CARD_COUNT: &str = "bottom_card_count";
pub const KEY_DECK_COUNT: &str = "deck_count";
pub const KEY_PLAY_TIME: &str = "play_time";
pub const KEY_SETTLEMENT_TIME: &str = "settlement_time";
pub const KEY_START_TIME: &str = "start_time";
pub const KEY_TARGET_RANK: &str = "target_rank";

pub fn build_upgrade_settings() -> (GameSettings, HashMap<String, GameParam>) {
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
                default: 3,
                options: vec!["J".into(), "Q".into(), "K".into(), "A".into()],
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
            KEY_START_TIME.into(),
            GameParam::Range(GameParamRange {
                default: 1,
                min: 0,
                max: 5,
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

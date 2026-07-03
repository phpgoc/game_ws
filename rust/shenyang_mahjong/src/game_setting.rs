use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

pub fn build_shenyang_mahjong_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
        (
            "animation_time".into(),
            GameParam::Range(GameParamRange {
                default: 200,
                min: 50,
                max: 2000,
            }),
        ),
        (
            "away_time".into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 2,
                max: 10,
            }),
        ),
        (
            "play_time".into(),
            GameParam::Range(GameParamRange {
                default: 20,
                min: 5,
                max: 50,
            }),
        ),
        (
            "claim_time".into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 3,
                max: 15,
            }),
        ),
        (
            "settlement_time".into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 2,
                max: 20,
            }),
        ),
        (
            "multi_hu_mode".into(),
            GameParam::Enum(GameParamEnum {
                default: 1,
                options: vec!["nearest".into(), "multi".into()],
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

use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange};
use ws_common::GameSettings;

pub fn build_holdem_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
        (
            "initial_chips".into(),
            GameParam::Range(GameParamRange {
                default: 1000,
                min: 200,
                max: 10000,
            }),
        ),
        (
            "small_blind".into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 1,
                max: 500,
            }),
        ),
        (
            "big_blind".into(),
            GameParam::Range(GameParamRange {
                default: 10,
                min: 2,
                max: 1000,
            }),
        ),
        (
            "play_time".into(),
            GameParam::Range(GameParamRange {
                default: 20,
                min: 5,
                max: 120,
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
    ]
    .into_iter()
    .collect();

    let mut settings = GameSettings::new(2, 8);
    for (key, param) in &params {
        if let GameParam::Range(range) = param {
            settings.values.insert(key.clone(), range.default);
        }
    }
    (settings, params)
}

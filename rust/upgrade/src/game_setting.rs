use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

use crate::UpgradeDeckCount;

pub const KEY_DECK_COUNT: &str = "deck_count";
pub const KEY_PLAY_TIME: &str = "play_time";

pub fn build_upgrade_settings() -> ws_common::SettingsBuilderResult {
    let params: HashMap<String, GameParam> = [
        (
            KEY_DECK_COUNT.into(),
            GameParam::Enum(GameParamEnum {
                default: 0,
                options: vec!["3".into(), "4".into(), "5".into(), "6".into()],
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
    ]
    .into_iter()
    .collect();

    let mut settings = GameSettings::new(4, 4);
    for (key, param) in &params {
        let default = match param {
            GameParam::Range(range) => range.default,
            GameParam::Enum(item) => item.default as i32,
        };
        settings.values.insert(key.clone(), default);
    }

    (settings, params)
}

pub fn deck_count_from_setting(index: i32) -> UpgradeDeckCount {
    let count = index.clamp(0, 3) as u8 + 3;
    UpgradeDeckCount::new(count).expect("clamped upgrade deck count is valid")
}

#[cfg(test)]
#[path = "game_setting/tests.rs"]
mod tests;

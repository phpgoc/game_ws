use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

use crate::UpgradeDeckCount;

pub const KEY_DECK_COUNT: &str = "deck_count";
pub const KEY_DEAL_TIME: &str = "deal_time";
pub const KEY_FIRST_DEAL_TIME: &str = "first_deal_time";
pub const KEY_PLAY_TIME: &str = "play_time";
pub const KEY_REMOVED_RANK_COUNT: &str = "removed_rank_count";
pub const KEY_ATTACKING_WIN_SCORE: &str = "attacking_win_score";
pub const KEY_SCORE_PER_LEVEL: &str = "score_per_level";
pub const KEY_SHUTOUT_BONUS_LEVELS: &str = "shutout_bonus_levels";

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
        (
            KEY_REMOVED_RANK_COUNT.into(),
            GameParam::Range(GameParamRange {
                default: 0,
                min: 0,
                max: 9,
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

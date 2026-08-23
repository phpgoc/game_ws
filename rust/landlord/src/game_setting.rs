use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange};
use ws_common::GameSettings;

/// 构建斗地主的 `GameSettings` + 参数描述。
/// 所有可配参数存储为 HashMap<String, i32>，param_descriptions 作为元数据。
pub fn build_landlord_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
        (
            "settlement_time".into(),
            GameParam::Range(GameParamRange {
                default: 15,
                min: 1,
                max: 30,
            }),
        ),
    ]
    .into_iter()
    .collect();

    let mut settings = GameSettings::new(3, 3);
    for (key, param) in &params {
        if let GameParam::Range(r) = param {
            settings.values.insert(key.clone(), r.default);
        }
    }

    (settings, params)
}

#[cfg(test)]
#[path = "game_setting/tests.rs"]
mod tests;

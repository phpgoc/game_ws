use std::collections::HashMap;

use share_type_public::GameParam;
use ws_common::GameSettings;

/// 构建斗地主的 `GameSettings` + 参数描述。
/// 所有可配参数存储为 HashMap<String, i32>，param_descriptions 作为元数据。
pub fn build_landlord_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [].into_iter().collect();

    let mut settings = GameSettings::new(3, 3);
    for (key, param) in &params {
        if let GameParam::Range(r) = param {
            settings.values.insert(key.clone(), r.default);
        }
    }

    (settings, params)
}

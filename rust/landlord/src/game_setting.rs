use std::collections::HashMap;

use share_type_public::GameParam;
use ws_common::GameSettings;

/// 构建斗地主的 `GameSettings` + 参数描述。
/// 所有可配参数存储为 HashMap<String, i32>，param_descriptions 作为元数据。
pub fn build_landlord_settings() -> (GameSettings, HashMap<String, GameParam>) {
    // Room timing is intentionally fixed; there are no user-facing settings.
    (GameSettings::new(3, 3), HashMap::new())
}

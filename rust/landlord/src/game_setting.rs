use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange};
use ws_common::GameSettings;

/// 构建斗地主的 `GameSettings` + 参数描述。
/// 所有可配参数存储为 HashMap<String, i32>，param_descriptions 作为元数据。
pub fn build_landlord_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
        (
            //出牌动画时间 毫秒
            "animation_time".into(),
            GameParam::Range(GameParamRange {
                default: 200,
                min: 50,
                max: 2000,
            }),
        ),
        (
            //离开状态等待时间 秒
            "away_time".into(),
            GameParam::Range(GameParamRange {
                default: 5,
                min: 2,
                max: 5,
            }),
        ),
        (
            //出牌和叫地主等待时间 秒
            "play_time".into(),
            GameParam::Range(GameParamRange {
                default: 30,
                min: 20,
                max: 50,
            }),
        ),
        (
            //发牌动画时间 毫秒
            "deal_time".into(),
            GameParam::Range(GameParamRange {
                default: 3000,
                min: 500,
                max: 4000,
            }),
        ),
        (
            // 开始阶段等待时间 秒
            "start_time".into(),
            GameParam::Range(GameParamRange {
                default: 1,
                min: 0,
                max: 5,
            }),
        ),
        (
            // 结算阶段等待时间 秒
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

    let mut settings = GameSettings::new(3, 3);
    for (key, param) in &params {
        if let GameParam::Range(r) = param {
            settings.values.insert(key.clone(), r.default);
        }
    }

    (settings, params)
}

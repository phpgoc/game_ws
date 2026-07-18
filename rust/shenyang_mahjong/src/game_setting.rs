use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

pub fn build_shenyang_mahjong_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params: HashMap<String, GameParam> = [
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
            "max_fan".into(),
            GameParam::Range(GameParamRange {
                default: 50,
                min: 1,
                max: 500,
            }),
        ),
        (
            "allow_first_chi".into(),
            GameParam::Enum(GameParamEnum {
                default: 1,
                options: vec!["disabled".into(), "enabled".into()],
            }),
        ),
        (
            "ting_fan".into(),
            GameParam::Enum(GameParamEnum {
                default: 0,
                options: vec!["disabled".into(), "enabled".into()],
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
    use super::build_shenyang_mahjong_settings;

    #[test]
    fn settings_do_not_expose_dead_start_or_animation_waits() {
        let (settings, descriptions) = build_shenyang_mahjong_settings();

        assert!(!settings.values.contains_key("start_time"));
        assert!(!descriptions.contains_key("start_time"));
        assert!(!settings.values.contains_key("animation_time"));
        assert!(!descriptions.contains_key("animation_time"));
        assert!(!settings.values.contains_key("away_time"));
        assert!(!descriptions.contains_key("away_time"));
        assert!(settings.values.contains_key("play_time"));
        assert!(settings.values.contains_key("claim_time"));
        assert_eq!(settings.values.get("max_fan"), Some(&50));
        assert!(descriptions.contains_key("max_fan"));
        assert!(!settings.values.contains_key("multi_hu_mode"));
        assert!(!descriptions.contains_key("multi_hu_mode"));
        assert!(!settings.values.contains_key("win_rule"));
        assert!(!descriptions.contains_key("win_rule"));
        assert_eq!(settings.values.get("allow_first_chi"), Some(&1));
        assert!(descriptions.contains_key("allow_first_chi"));
        assert_eq!(settings.values.get("ting_fan"), Some(&0));
        assert!(descriptions.contains_key("ting_fan"));
        assert!(!settings.values.contains_key("allow_chi"));
        assert!(!settings.values.contains_key("chi_opens_door"));
    }
}

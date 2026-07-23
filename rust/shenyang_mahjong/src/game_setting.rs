use std::collections::HashMap;

use share_type_public::{GameParam, GameParamRange, settings::GameParamEnum};
use ws_common::GameSettings;

pub(crate) const DEFAULT_PAYMENT_SCORE_CAP: i32 = 50;
pub(crate) const MAX_PAYMENT_SCORE_CAP: i32 = 200;
pub(crate) const MIN_PAYMENT_SCORE_CAP: i32 = 20;

pub(crate) fn payment_score_cap_from_configs(configs: &HashMap<String, i32>) -> i32 {
    configs
        .get("max_fan")
        .copied()
        .filter(|score_cap| (MIN_PAYMENT_SCORE_CAP..=MAX_PAYMENT_SCORE_CAP).contains(score_cap))
        .unwrap_or(DEFAULT_PAYMENT_SCORE_CAP)
}

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
                default: DEFAULT_PAYMENT_SCORE_CAP,
                min: MIN_PAYMENT_SCORE_CAP,
                max: MAX_PAYMENT_SCORE_CAP,
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
    use std::collections::HashMap;

    use super::{build_shenyang_mahjong_settings, payment_score_cap_from_configs};
    use share_type_public::GameParam;

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
        assert!(matches!(
            descriptions.get("max_fan"),
            Some(GameParam::Range(range))
                if range.default == 50 && range.min == 20 && range.max == 200
        ));
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

    #[test]
    fn payment_score_cap_defaults_invalid_or_missing_configs_to_fifty() {
        assert_eq!(payment_score_cap_from_configs(&HashMap::new()), 50);
        for invalid in [i32::MIN, -1, 0, 19, 201, i32::MAX] {
            let configs = HashMap::from([("max_fan".to_owned(), invalid)]);
            assert_eq!(payment_score_cap_from_configs(&configs), 50);
        }
        for valid in [20, 50, 200] {
            let configs = HashMap::from([("max_fan".to_owned(), valid)]);
            assert_eq!(payment_score_cap_from_configs(&configs), valid);
        }
    }
}

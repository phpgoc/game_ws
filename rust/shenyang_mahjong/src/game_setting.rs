use std::collections::HashMap;

use share_type_public::GameParam;
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
    // Rules and timing use the built-in defaults; no room settings are exposed.
    (GameSettings::new(4, 4), HashMap::new())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{build_shenyang_mahjong_settings, payment_score_cap_from_configs};

    #[test]
    fn settings_do_not_expose_dead_start_or_animation_waits() {
        let (settings, descriptions) = build_shenyang_mahjong_settings();

        assert!(!settings.values.contains_key("start_time"));
        assert!(!descriptions.contains_key("start_time"));
        assert!(!settings.values.contains_key("animation_time"));
        assert!(!descriptions.contains_key("animation_time"));
        assert!(!settings.values.contains_key("away_time"));
        assert!(!descriptions.contains_key("away_time"));
        assert!(settings.values.is_empty());
        assert!(descriptions.is_empty());
        assert!(!settings.values.contains_key("multi_hu_mode"));
        assert!(!descriptions.contains_key("multi_hu_mode"));
        assert!(!settings.values.contains_key("win_rule"));
        assert!(!descriptions.contains_key("win_rule"));
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

use std::collections::HashMap;

use share_type_public::settings::GameParamEnum;
use share_type_public::{DominoesNoPlayableTiles, DominoesRule, GameParam};
use ws_common::GameSettings;

pub const KEY_RULE: &str = "rule";
pub const KEY_NO_PLAYABLE_TILES: &str = "no_playable_tiles";
pub const KEY_TARGET_SCORE: &str = "target_score";

pub fn build_dominoes_settings() -> (GameSettings, HashMap<String, GameParam>) {
    let params = HashMap::from([
        (
            KEY_RULE.to_owned(),
            GameParam::Enum(GameParamEnum {
                default: DominoesRule::Simple as usize,
                options: vec!["simple".to_owned(), "five_up".to_owned()],
            }),
        ),
        (
            KEY_NO_PLAYABLE_TILES.to_owned(),
            GameParam::Enum(GameParamEnum {
                default: DominoesNoPlayableTiles::KeepDrawing as usize,
                options: vec![
                    "keep_drawing".to_owned(),
                    "draw_one".to_owned(),
                    "pass_without_draw".to_owned(),
                ],
            }),
        ),
        (
            KEY_TARGET_SCORE.to_owned(),
            GameParam::Enum(GameParamEnum {
                default: 0,
                options: vec!["35".to_owned(), "61".to_owned()],
            }),
        ),
    ]);
    let values = HashMap::from([
        (KEY_RULE.to_owned(), DominoesRule::Simple as i32),
        (
            KEY_NO_PLAYABLE_TILES.to_owned(),
            DominoesNoPlayableTiles::KeepDrawing as i32,
        ),
        (KEY_TARGET_SCORE.to_owned(), 0),
    ]);
    let mut settings = GameSettings::new(3, 4);
    settings.values = values;
    (settings, params)
}

pub fn rule_from_config(value: i32) -> DominoesRule {
    if value == DominoesRule::FiveUp as i32 {
        DominoesRule::FiveUp
    } else {
        DominoesRule::Simple
    }
}

pub fn no_playable_from_config(value: i32) -> DominoesNoPlayableTiles {
    match value {
        1 => DominoesNoPlayableTiles::DrawOne,
        2 => DominoesNoPlayableTiles::PassWithoutDraw,
        _ => DominoesNoPlayableTiles::KeepDrawing,
    }
}

pub fn target_from_config(value: i32) -> i32 {
    if value == 1 { 61 } else { 35 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_match_switch_options_and_room_capacity() {
        let (settings, params) = build_dominoes_settings();
        assert_eq!((settings.min_players, settings.max_players), (3, 4));
        assert_eq!(settings.values[KEY_RULE], 0);
        assert_eq!(settings.values[KEY_NO_PLAYABLE_TILES], 0);
        let GameParam::Enum(no_playable) = &params[KEY_NO_PLAYABLE_TILES] else {
            panic!("no-playable setting must be an enum");
        };
        assert_eq!(no_playable.default, 0);
        assert_eq!(
            no_playable.options,
            ["keep_drawing", "draw_one", "pass_without_draw"]
        );
    }
}

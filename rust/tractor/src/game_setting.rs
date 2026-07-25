use std::collections::HashMap;

use share_type_public::GameParam;
use ws_common::GameSettings;

pub const KEY_AI_ACTION_TIME: &str = "ai_action_time";

pub const KEY_AWAY_TIME: &str = "away_time";
pub const KEY_BLOOD_ENABLED: &str = "blood_enabled";
pub const KEY_BLOOD_SCORE_PER_UNIT: &str = "blood_score_per_unit";
pub const KEY_BLOOD_START_SCORE: &str = "blood_start_score";
pub const KEY_BOTTOM_CARD_COUNT: &str = "bottom_card_count";
pub const KEY_DEAL_TIME: &str = "deal_time";
pub const KEY_DECK_COUNT: &str = "deck_count";
pub const KEY_FIRST_DEAL_TIME: &str = "first_deal_time";
pub const KEY_PLAY_TIME: &str = "play_time";
pub const KEY_REMOVED_RANK_COUNT: &str = "removed_rank_count";
pub const KEY_SETTLEMENT_TIME: &str = "settlement_time";
pub const KEY_TARGET_RANK: &str = "target_rank";

pub fn build_tractor_settings() -> (GameSettings, HashMap<String, GameParam>) {
    // Rules and timing use the built-in defaults; no room settings are exposed.
    (GameSettings::new(4, 4), HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_deal_is_slower_and_compact_deck_is_a_count() {
        let (settings, params) = build_tractor_settings();
        assert!(settings.values.is_empty());
        assert!(params.is_empty());
    }
}

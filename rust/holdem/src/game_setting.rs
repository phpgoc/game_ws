use std::collections::HashMap;

use share_type_public::GameParam;
use ws_common::GameSettings;

pub fn build_holdem_settings() -> (GameSettings, HashMap<String, GameParam>) {
    // Stakes, chips, and timing use the built-in defaults; no room settings are exposed.
    (GameSettings::new(2, 8), HashMap::new())
}

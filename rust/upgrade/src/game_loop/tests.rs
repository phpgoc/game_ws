use std::{collections::HashMap, time::Duration};

use super::*;

#[test]
fn first_deal_defaults_to_fifteen_seconds_and_never_below_three_times() {
    let defaults = HashMap::new();
    assert_eq!(
        deal_step_delay(&defaults, 0, 100),
        Duration::from_millis(150)
    );
    assert_eq!(
        deal_step_delay(&defaults, 1, 100),
        Duration::from_millis(30)
    );

    let configured = HashMap::from([
        (KEY_FIRST_DEAL_TIME.to_owned(), 2_000),
        (KEY_DEAL_TIME.to_owned(), 2_000),
    ]);
    assert_eq!(
        deal_step_delay(&configured, 0, 100),
        Duration::from_millis(60)
    );
    assert_eq!(
        deal_step_delay(&configured, 1, 100),
        Duration::from_millis(20)
    );
}

#[test]
fn bottom_operation_uses_three_times_the_configured_play_window() {
    assert_eq!(bury_window_time(&HashMap::new()), 90);
    assert_eq!(
        bury_window_time(&HashMap::from([(KEY_PLAY_TIME.to_owned(), 40)])),
        120
    );
}

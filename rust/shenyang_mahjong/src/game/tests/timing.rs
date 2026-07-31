use std::collections::HashMap;

use super::{current_claim_time, current_play_time, settlement_time};

#[cfg(not(feature = "e2e-fixture"))]
#[test]
fn normal_build_uses_hidden_server_timing_defaults() {
    let configs = HashMap::new();

    assert_eq!(current_claim_time(&configs), 5);
    assert_eq!(current_play_time(&configs), 20);
    assert_eq!(settlement_time(&configs), 5);
}

#[cfg(feature = "e2e-fixture")]
#[test]
fn e2e_fixture_compresses_internal_loop_waits_without_room_settings() {
    let configs = HashMap::from([
        ("claim_time".to_owned(), 99),
        ("play_time".to_owned(), 99),
        ("settlement_time".to_owned(), 99),
    ]);

    assert_eq!(current_claim_time(&configs), 1);
    assert_eq!(current_play_time(&configs), 1);
    assert_eq!(settlement_time(&configs), 1);
}

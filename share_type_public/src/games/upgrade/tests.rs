use super::*;

#[test]
fn upgrade_protocol_keeps_game_specific_route_space() {
    assert_eq!(UpgradeRoutes::DECLARE_TRUMP as i32, 5001);
    assert_eq!(UpgradeWsCode::BOTTOM_CARDS as i32, 5002);
    assert_eq!(UpgradePhase::Settlement as i8, 4);
}

#[test]
fn settlement_has_level_change_without_blood_fields() {
    let event = WsUpgradeSettlementEvent {
        winner_positions: vec![0, 2],
        score: 120,
        level_change: 2,
        target_rank: UpgradeRank::FIVE,
        match_finished: false,
        next_target_rank: Some(UpgradeRank::SEVEN),
        player_scores: Default::default(),
    };
    let encoded = serde_json::to_value(event).unwrap();
    assert_eq!(encoded["level_change"], 2);
    assert!(encoded.get("blood_units").is_none());
}

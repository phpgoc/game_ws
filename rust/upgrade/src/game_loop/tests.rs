use std::{collections::HashMap, time::Duration};

use upgrade_common::Card;
use ws_common::{CommonGameState, RoomService, SessionSenders};

use crate::state::UpgradeGameState;

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

#[tokio::test]
async fn game_loop_stops_during_a_long_deal_delay() {
    let common = Arc::new(std::sync::Mutex::new(CommonGameState::new()));
    let mut game = UpgradeGameState::from_common(common);
    game.phase = UpgradePhase::Deal;
    game.deal_queue
        .push_back((0, Card::try_from(2).unwrap().encoded()));
    game.total_deal_count = 1;
    let state = Arc::new(std::sync::Mutex::new(game));
    let room = Arc::new(Mutex::new(RoomService::default()));
    let senders: SessionSenders = Arc::new(Mutex::new(HashMap::new()));

    start_upgrade_game_loop(
        "upgrade-stop-during-deal".to_owned(),
        Arc::clone(&state),
        room,
        senders,
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.lock().unwrap().phase == UpgradePhase::Deal {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("upgrade loop should deal before sleeping");
    state.lock().unwrap().base.lock().unwrap().request_stop();

    tokio::time::timeout(Duration::from_millis(500), async {
        while Arc::strong_count(&state) > 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("upgrade loop must release its state during the deal delay");
}

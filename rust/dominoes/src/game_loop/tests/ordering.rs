use std::collections::HashMap;

use share_type_public::{GameId, Routes, WsRequest};
use ws_common::{GameSettings, session_sender_channel};

use super::*;

#[tokio::test]
async fn background_action_keeps_room_locked_until_its_messages_are_queued() {
    let room_key = "ordered-dominoes";
    let mut rooms = RoomService::default();
    for session in 1..=3 {
        rooms.connect(session);
        rooms
            .handle_common_request(
                session,
                &WsRequest {
                    route: Routes::JOIN as i32,
                    data: serde_json::json!({
                        "name": format!("P{session}"),
                        "password": room_key,
                        "game_id": GameId::DOMINOES as i32,
                    }),
                },
                GameId::DOMINOES,
                || (GameSettings::new(3, 4), HashMap::new()),
            )
            .expect("join");
    }
    let common = rooms.room_common_state(room_key).expect("common state");
    let mut round = DominoesRoundState::new_with_seed(
        common,
        DominoesRule::Simple,
        DominoesNoPlayableTiles::PassWithoutDraw,
        35,
        11,
    )
    .unwrap();
    round.start_new_game().unwrap();
    round.remaining_seconds = 0;
    let state = Arc::new(Mutex::new(round));
    let states = Arc::new(Mutex::new(HashMap::from([(
        room_key.to_owned(),
        Arc::clone(&state),
    )])));
    let rooms = Arc::new(AsyncMutex::new(rooms));
    let (sender, mut receiver, _) = session_sender_channel(128);
    let senders = Arc::new(AsyncMutex::new(HashMap::from([(1, sender)])));
    // 卡住发送队列，稳定重现“已改牌局、尚未发出结果”的窗口。
    let sending = senders.lock().await;
    start_game_loop(
        room_key.to_owned(),
        Arc::clone(&state),
        Arc::clone(&rooms),
        Arc::clone(&senders),
        states,
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.lock().unwrap().placements.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("automatic opening");

    assert!(
        tokio::time::timeout(Duration::from_millis(40), rooms.lock())
            .await
            .is_err(),
        "重连、暂停或下一手不能抢在后台动作的消息之前读取或改变房间"
    );
    drop(sending);
    let first = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    let event: serde_json::Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
    assert_eq!(
        event["code"],
        share_type_public::DominoesWsCode::PLAY_TILE as i32
    );
    drop(
        tokio::time::timeout(Duration::from_secs(1), rooms.lock())
            .await
            .expect("room unlocks after queueing"),
    );
    state.lock().unwrap().base.lock().unwrap().stop_requested = true;
}

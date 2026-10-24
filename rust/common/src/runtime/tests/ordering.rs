use futures_util::{SinkExt, StreamExt};
use tokio::sync::oneshot;
use tokio_tungstenite::connect_async;

use super::*;

type Context = (super::super::SessionSenders, Arc<Mutex<RoomService>>);

struct OrderingHandler {
    context: Option<oneshot::Sender<Context>>,
    handled: mpsc::UnboundedSender<()>,
}

impl GameHandler for OrderingHandler {
    fn build_game_state(&self) -> Box<dyn crate::GameState> {
        Box::new(SharedGameState::new())
    }

    fn build_room_settings(&self) -> SettingsBuilderResult {
        (GameSettings::new(1, 4), HashMap::new())
    }

    fn game_id(&self) -> GameId {
        GameId::DOMINOES
    }

    fn handle_game_request(
        &mut self,
        room: &mut RoomService,
        session: SessionId,
        request: ClientRequest,
    ) -> Dispatch {
        self.handled.send(()).unwrap();
        room.error_response(session, request.route, WsResponseCode::NOT_IN_RANGE)
    }

    fn set_context(
        &mut self,
        senders: super::super::SessionSenders,
        room: Arc<Mutex<RoomService>>,
    ) {
        self.context
            .take()
            .unwrap()
            .send((senders, room))
            .ok()
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_keeps_room_locked_until_its_dispatch_is_queued() {
    let (context_tx, context_rx) = oneshot::channel();
    let (handled_tx, mut handled_rx) = mpsc::unbounded_channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let (stop, signal) = runtime_stop_channel();
    let runtime = tokio::spawn(super::super::run_room_runtime_until_stopped_with_ready(
        RuntimeConfig {
            service_name: "ordered-room-test",
            listen_addr: "127.0.0.1:0".to_owned(),
            idle_timeout: std::time::Duration::from_secs(10),
            heartbeat_interval: std::time::Duration::from_secs(10),
        },
        OrderingHandler {
            context: Some(context_tx),
            handled: handled_tx,
        },
        signal,
        ready_tx,
    ));
    let stats = ready_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    let (senders, room) = context_rx.await.unwrap();
    let (mut client, _) = connect_async(format!("ws://{}", stats.listen_addr()))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while stats.client_count().await != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("client registered");
    let sending = senders.lock().await;
    client
        .send(Message::Text(
            serde_json::json!({"route": 5901, "data": {}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), handled_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(40), room.lock())
            .await
            .is_err(),
        "后续请求与后台动作必须等前一条请求的消息入队后才能取得房间"
    );
    drop(sending);
    let response = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let frame = client.next().await.unwrap().unwrap();
            if let Message::Text(text) = frame {
                break serde_json::from_str::<Value>(&text).unwrap();
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(response["route"], 5901);
    drop(
        tokio::time::timeout(std::time::Duration::from_secs(1), room.lock())
            .await
            .expect("room unlocked"),
    );
    stop.stop();
    runtime.await.unwrap().unwrap();
}

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use p2p::{config::IceServiceConfig, runtime::run_p2p_listener};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test]
async fn websocket_clients_join_signal_and_leave() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("local address");
    let ice = IceServiceConfig::new(
        vec!["stun:stun.example.test:3478".into()],
        vec!["turn:turn.example.test:3478?transport=udp".into()],
        "integration-secret".into(),
        600,
    )
    .expect("ICE config");
    let server = tokio::spawn(run_p2p_listener(
        listener,
        ice,
        Duration::from_secs(5),
        Duration::from_secs(1),
    ));

    let url = format!("ws://{address}");
    let (mut first, _) = connect_async(&url).await.expect("first connection");
    let (mut second, _) = connect_async(&url).await.expect("second connection");

    send_json(
        &mut first,
        json!({
            "route": 5001,
            "data": { "game": "quoridor", "room": "integration", "name": "first" }
        }),
    )
    .await;
    let joined = read_until(&mut first, |value| value["route"] == 5001).await;
    assert_eq!(joined["code"], 201);
    let ice_event = read_until(&mut first, |value| value["code"] == 5001).await;
    assert_eq!(ice_event["data"]["self_position"], 0);
    assert_eq!(
        ice_event["data"]["ice_servers"][1]["urls"][0],
        "turn:turn.example.test:3478?transport=udp"
    );

    send_json(
        &mut second,
        json!({
            "route": 5001,
            "data": { "game": "quoridor", "room": "integration", "name": "second" }
        }),
    )
    .await;
    let second_joined = read_until(&mut second, |value| value["route"] == 5001).await;
    assert_eq!(second_joined["data"]["self_position"], 1);
    let first_peer = read_until(&mut first, |value| value["code"] == 5002).await;
    assert_eq!(first_peer["data"]["peer_name"], "second");
    assert_eq!(first_peer["data"]["initiator"], true);

    send_json(
        &mut first,
        json!({
            "route": 5002,
            "data": {
                "target_position": 1,
                "kind": 0,
                "sdp": "v=0\r\n"
            }
        }),
    )
    .await;
    let offer = read_until(&mut second, |value| value["code"] == 5003).await;
    assert_eq!(offer["data"]["from_position"], 0);
    assert_eq!(offer["data"]["sdp"], "v=0\r\n");

    second.close(None).await.expect("close second");
    let left = read_until(&mut first, |value| value["code"] == 5004).await;
    assert_eq!(left["data"]["peer_position"], 1);

    first.close(None).await.expect("close first");
    server.abort();
}

async fn send_json(client: &mut Client, value: Value) {
    client
        .send(Message::Text(value.to_string().into()))
        .await
        .expect("send request");
}

async fn read_until(client: &mut Client, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(frame) = client.next().await {
            let frame = frame.expect("websocket frame");
            if let Message::Text(text) = frame {
                let value: Value = serde_json::from_str(&text).expect("JSON response");
                if predicate(&value) {
                    return value;
                }
            }
        }
        panic!("websocket closed before matching response");
    })
    .await
    .expect("matching response timeout")
}

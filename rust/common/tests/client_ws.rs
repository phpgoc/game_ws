use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use ws_common::{WsClientEvent, connect_ws_client};

#[tokio::test]
async fn client_forwards_text_binary_and_remote_close_events() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = listener.local_addr().expect("read test address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept client");
        let mut websocket = accept_async(stream).await.expect("accept websocket");
        assert!(matches!(
            websocket.next().await.expect("client frame").expect("valid frame"),
            Message::Text(text) if text == "hello"
        ));
        websocket
            .send(Message::Binary(vec![1, 2, 3].into()))
            .await
            .expect("send binary");
        websocket
            .send(Message::Text("reply".into()))
            .await
            .expect("send reply");
        websocket
            .send(Message::Close(None))
            .await
            .expect("send close");
    });

    let (client, mut events) = connect_ws_client(&format!("ws://{address}"))
        .await
        .expect("connect common client");
    client.send_text("hello".to_owned()).expect("queue text");
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("binary event timeout"),
        Some(WsClientEvent::Error(message)) if message == "unexpected binary frame"
    ));
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("text event timeout"),
        Some(WsClientEvent::Message(message)) if message == "reply"
    ));
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("close event timeout"),
        Some(WsClientEvent::Closed { code: None, reason }) if reason.is_empty()
    ));
    server.await.expect("server task joins");
}

#[tokio::test]
async fn client_rejects_invalid_websocket_addresses() {
    assert!(connect_ws_client("not a websocket url").await.is_err());
}

#[tokio::test]
async fn client_reports_remote_close_details_and_handle_drop() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = listener.local_addr().expect("read test address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept client");
        let mut websocket = accept_async(stream).await.expect("accept websocket");
        websocket
            .send(Message::Close(Some(
                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Away,
                    reason: "server restart".into(),
                },
            )))
            .await
            .expect("send remote close");
    });

    let (client, mut events) = connect_ws_client(&format!("ws://{address}"))
        .await
        .expect("connect common client");
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("remote close event timeout"),
        Some(WsClientEvent::Closed { code: Some(1001), reason }) if reason == "server restart"
    ));
    assert!(client.send_text("after close".to_owned()).is_err());
    assert!(client.close(None, "after close".to_owned()).is_err());
    drop(client);
    server.await.expect("server task joins");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind drop server");
    let address = listener.local_addr().expect("read drop address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept drop client");
        let mut websocket = accept_async(stream).await.expect("accept drop websocket");
        websocket.next().await
    });
    let (client, mut events) = connect_ws_client(&format!("ws://{address}"))
        .await
        .expect("connect droppable client");
    drop(client);
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("drop event timeout"),
        Some(WsClientEvent::Closed { code: None, reason }) if reason == "client dropped"
    ));
    server.await.expect("drop server task joins");
}

#[tokio::test]
async fn client_ignores_control_frames_and_reports_an_abrupt_peer_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = listener.local_addr().expect("read test address");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept client");
        let mut websocket = accept_async(stream).await.expect("accept websocket");
        websocket
            .send(Message::Ping(vec![1].into()))
            .await
            .expect("send ping");
        websocket
            .send(Message::Pong(vec![2].into()))
            .await
            .expect("send pong");
        let mut stream = websocket.into_inner();
        tokio::io::AsyncWriteExt::shutdown(&mut stream)
            .await
            .expect("close tcp stream without a websocket close frame");
    });

    let (_client, mut events) = connect_ws_client(&format!("ws://{address}"))
        .await
        .expect("connect common client");
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("disconnect error timeout"),
        Some(WsClientEvent::Error(_))
    ));
    assert!(matches!(
        timeout(Duration::from_secs(1), events.recv())
            .await
            .expect("disconnect close timeout"),
        Some(WsClientEvent::Closed { code: None, reason }) if reason == "connection error"
    ));
    server.await.expect("server task joins");
}

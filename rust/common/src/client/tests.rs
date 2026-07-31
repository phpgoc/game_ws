use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::{WsClientEvent, connect_ws_client};

#[tokio::test]
async fn close_sends_requested_close_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        websocket.next().await.unwrap().unwrap()
    });

    let (client, mut events) = connect_ws_client(&format!("ws://{address}")).await.unwrap();
    client.close(Some(4001), "leaving".to_string()).unwrap();

    match server.await.unwrap() {
        Message::Close(Some(frame)) => {
            assert_eq!(u16::from(frame.code), 4001);
            assert_eq!(frame.reason, "leaving");
        }
        frame => panic!("expected close frame, got {frame:?}"),
    }
    assert!(matches!(
        events.recv().await,
        Some(WsClientEvent::Closed {
            code: Some(4001),
            reason
        }) if reason == "leaving"
    ));
}

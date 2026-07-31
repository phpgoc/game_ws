use futures_util::{SinkExt, StreamExt};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Message,
        protocol::{CloseFrame, frame::coding::CloseCode},
    },
};

enum WsClientCommand {
    SendText(String),
    Close { code: Option<u16>, reason: String },
}

#[derive(Debug, Clone)]
pub enum WsClientEvent {
    Message(String),
    Closed { code: Option<u16>, reason: String },
    Error(String),
}

pub struct WsClientHandle {
    commands: mpsc::UnboundedSender<WsClientCommand>,
    _task: JoinHandle<()>,
}

#[derive(Debug, thiserror::Error)]
pub enum WsClientSendError {
    #[error("websocket client is closed")]
    Closed,
}

fn close_message(code: Option<u16>, reason: &str) -> Message {
    Message::Close(code.map(|code| CloseFrame {
        code: CloseCode::from(code),
        reason: reason.to_string().into(),
    }))
}

/// 连接指定 WebSocket 地址，并返回发送控制句柄与异步事件接收器。
pub async fn connect_ws_client(
    url: &str,
) -> anyhow::Result<(WsClientHandle, mpsc::UnboundedReceiver<WsClientEvent>)> {
    let (ws, _) = connect_async(url).await?;
    let (mut sink, mut source) = ws.split();
    let (commands_tx, mut commands_rx) = mpsc::unbounded_channel::<WsClientCommand>();
    let (events_tx, events_rx) = mpsc::unbounded_channel::<WsClientEvent>();

    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                command = commands_rx.recv() => {
                    match command {
                        Some(WsClientCommand::SendText(message)) => {
                            if let Err(err) = sink.send(Message::Text(message.into())).await {
                                let _ = events_tx.send(WsClientEvent::Error(err.to_string()));
                                let _ = events_tx.send(WsClientEvent::Closed {
                                    code: None,
                                    reason: "send failed".to_string(),
                                });
                                break;
                            }
                        }
                        Some(WsClientCommand::Close { code, reason }) => {
                            if let Err(err) = sink.send(close_message(code, &reason)).await {
                                let _ = events_tx.send(WsClientEvent::Error(err.to_string()));
                            }
                            let _ = events_tx.send(WsClientEvent::Closed { code, reason });
                            break;
                        }
                        None => {
                            let _ = sink.close().await;
                            let _ = events_tx.send(WsClientEvent::Closed {
                                code: None,
                                reason: "client dropped".to_string(),
                            });
                            break;
                        }
                    }
                }
                frame = source.next() => {
                    match frame {
                        Some(Ok(Message::Text(text))) => {
                            let _ = events_tx.send(WsClientEvent::Message(text.to_string()));
                        }
                        Some(Ok(Message::Binary(_))) => {
                            let _ = events_tx.send(WsClientEvent::Error("unexpected binary frame".to_string()));
                        }
                        Some(Ok(Message::Close(frame))) => {
                            let (code, reason) = frame
                                .map(|frame| (Some(u16::from(frame.code)), frame.reason.to_string()))
                                .unwrap_or((None, String::new()));
                            let _ = events_tx.send(WsClientEvent::Closed { code, reason });
                            break;
                        }
                        Some(Ok(Message::Ping(_)| Message::Pong(_) | Message::Frame(_))) => {}
                        Some(Err(err)) => {
                            let _ = events_tx.send(WsClientEvent::Error(err.to_string()));
                            let _ = events_tx.send(WsClientEvent::Closed {
                                code: None,
                                reason: "connection error".to_string(),
                            });
                            break;
                        }
                        None => {
                            let _ = events_tx.send(WsClientEvent::Closed {
                                code: None,
                                reason: String::new(),
                            });
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok((
        WsClientHandle {
            commands: commands_tx,
            _task: task,
        },
        events_rx,
    ))
}

impl WsClientHandle {
    /// 请求客户端发送关闭帧，并在发送队列已关闭时返回错误。
    pub fn close(&self, code: Option<u16>, reason: String) -> Result<(), WsClientSendError> {
        self.commands
            .send(WsClientCommand::Close { code, reason })
            .map_err(|_| WsClientSendError::Closed)
    }

    /// 将一条文本消息放入客户端的发送队列。
    pub fn send_text(&self, message: String) -> Result<(), WsClientSendError> {
        self.commands
            .send(WsClientCommand::SendText(message))
            .map_err(|_| WsClientSendError::Closed)
    }
}

#[cfg(test)]
#[path = "client/tests.rs"]
mod tests;

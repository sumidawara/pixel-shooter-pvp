//! Tokio WebSocket listenerと接続単位の送受信。

use std::{collections::VecDeque, time::Duration};

use crossbeam_channel::Sender;
use futures_util::{SinkExt, StreamExt};
use pixel_shooter_protocol::ClientMessage;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use super::{ClientSenders, NetworkEvent, NetworkThreadSettings, snapshot::should_drop_packet};

/// TokioとWebSocketを動かす専用OSスレッドを開始する。
///
/// 非同期通信をBevyのSystem内で待つとゲーム更新が止まるため、
/// 通信は別スレッド、ゲーム計算はBevyのメインスレッドと役割を分ける。
pub(super) fn start_network_thread(
    events: Sender<NetworkEvent>,
    clients: ClientSenders,
    settings: NetworkThreadSettings,
) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
        runtime.block_on(async move {
            let listener = match TcpListener::bind(&settings.bind_address).await {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!(
                        "could not bind WebSocket server to {}: {error}",
                        settings.bind_address
                    );
                    std::process::exit(1);
                }
            };
            let mut next_client_id = 1_u64;
            loop {
                // 新しいTCP接続が来るまで非同期に待つ。
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let id = next_client_id;
                        next_client_id += 1;
                        let tx = events.clone();
                        let peers = clients.clone();
                        // クライアントごとに独立した非同期タスクを作る。
                        tokio::spawn(handle_connection(
                            id,
                            stream,
                            tx,
                            peers,
                            settings.simulated_latency,
                            settings.simulated_loss_percent,
                        ));
                    }
                    Err(error) => eprintln!("accept error: {error}"),
                }
            }
        });
    });
}

/// 1クライアント分のWebSocket送受信を担当する。
async fn handle_connection(
    id: u64,
    stream: TcpStream,
    events: Sender<NetworkEvent>,
    clients: ClientSenders,
    simulated_latency: Duration,
    simulated_loss_percent: u32,
) {
    // TCP接続をWebSocket接続へアップグレードする。
    let websocket = match accept_async(stream).await {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("websocket handshake error: {error}");
            return;
        }
    };
    // 送信側と受信側に分けることで、それぞれを同時に動かせる。
    let (mut socket_tx, mut socket_rx) = websocket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    clients.lock().expect("clients lock").insert(id, out_tx);
    let _ = events.send(NetworkEvent::Connected(id));

    // Bevy側からout_txへ投入されたメッセージを、実際のSocketへ書き出すタスク。
    let writer = tokio::spawn(async move {
        let mut delayed = VecDeque::new();
        loop {
            if let Some((deliver_at, _)) = delayed.front() {
                tokio::select! {
                    outbound = out_rx.recv() => {
                        let Some(outbound) = outbound else { break };
                        delayed.push_back((
                            tokio::time::Instant::now() + outbound.delay,
                            outbound.message,
                        ));
                    }
                    _ = tokio::time::sleep_until(*deliver_at) => {
                        let (_, message) = delayed.pop_front().expect("delayed message");
                        if socket_tx.send(message).await.is_err() {
                            break;
                        }
                    }
                }
            } else {
                let Some(outbound) = out_rx.recv().await else {
                    break;
                };
                if outbound.delay.is_zero() {
                    if socket_tx.send(outbound.message).await.is_err() {
                        break;
                    }
                } else {
                    delayed.push_back((
                        tokio::time::Instant::now() + outbound.delay,
                        outbound.message,
                    ));
                }
            }
        }
    });

    // Godotから届いたJSONをClientMessageへ変換し、Bevy側へ渡す。
    while let Some(result) = socket_rx.next().await {
        match result {
            Ok(Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(message) => {
                    // Joinは即時に処理し、入力だけを人工的な遅延・欠落の対象にする。
                    let input_sequence = match &message {
                        ClientMessage::Input { sequence, .. } => Some(u64::from(*sequence)),
                        _ => None,
                    };
                    if let Some(sequence) = input_sequence {
                        if should_drop_packet(sequence, simulated_loss_percent) {
                            continue;
                        }
                        if !simulated_latency.is_zero() {
                            let delayed_events = events.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(simulated_latency).await;
                                let _ = delayed_events.send(NetworkEvent::Message(id, message));
                            });
                            continue;
                        }
                    }
                    let _ = events.send(NetworkEvent::Message(id, message));
                }
                Err(error) => eprintln!("invalid message from {id}: {error}"),
            },
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // 受信ループを抜けたら切断扱いにし、送信用タスクも停止する。
    clients.lock().expect("clients lock").remove(&id);
    writer.abort();
    let _ = events.send(NetworkEvent::Disconnected(id));
}

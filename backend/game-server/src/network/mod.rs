//! GameServerのWebSocket通信をtransport・イベント反映・Snapshot配信へ分離する。

mod events;
mod snapshot;
mod transport;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::prelude::Resource;
use crossbeam_channel::{Receiver, unbounded};
use pixel_shooter_protocol::ClientMessage;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::{config::ServerSettings, control::SharedGameSnapshot};

pub(crate) use events::process_network;
pub(crate) use snapshot::broadcast_snapshot;

// 接続IDから、そのクライアントへメッセージを送るチャンネルを検索する表。
type ClientSenders = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<OutboundMessage>>>>;

pub(super) struct OutboundMessage {
    pub(super) message: Message,
    pub(super) delay: Duration,
}

#[derive(Resource)]
pub(crate) struct Network {
    pub(super) events: Receiver<NetworkEvent>,
    pub(super) clients: ClientSenders,
    pub(super) simulated_latency: Duration,
    pub(super) simulated_loss_percent: u32,
    pub(super) outbound_sequence: u64,
    pub(super) game_snapshot: SharedGameSnapshot,
}

pub(super) enum NetworkEvent {
    Connected(u64),
    Disconnected(u64),
    Message(u64, ClientMessage),
}

pub(super) struct NetworkThreadSettings {
    pub(super) bind_address: String,
    pub(super) simulated_latency: Duration,
    pub(super) simulated_loss_percent: u32,
}

/// WebSocket用スレッドを開始し、Bevyへ登録するNetwork Resourceを返す。
/// 待受を開始し、成功したら`Network`を返す。
///
/// 待受できたことを確かめてから返すので、呼び出し側は
/// 「listening」と表示してよい状態かどうかを判断できる。
pub(crate) fn start(
    settings: &ServerSettings,
    game_snapshot: SharedGameSnapshot,
) -> Result<Network, String> {
    let (event_tx, event_rx) = unbounded();
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let simulated_latency = Duration::from_millis(settings.network.simulated_latency_ms);
    let (bind_tx, bind_rx) = unbounded();
    transport::start_network_thread(
        event_tx,
        clients.clone(),
        NetworkThreadSettings {
            bind_address: settings.network.bind_address.clone(),
            simulated_latency,
            simulated_loss_percent: settings.network.simulated_loss_percent,
        },
        bind_tx,
    );
    // 待受の成否が返るまで待つ。返らない場合はスレッドが起動前に落ちている。
    match bind_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err("network thread stopped before binding".into()),
    }
    Ok(Network {
        events: event_rx,
        clients,
        simulated_latency,
        simulated_loss_percent: settings.network.simulated_loss_percent,
        outbound_sequence: 0,
        game_snapshot,
    })
}

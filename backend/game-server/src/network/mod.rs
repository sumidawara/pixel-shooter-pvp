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
pub(crate) fn start(settings: &ServerSettings, game_snapshot: SharedGameSnapshot) -> Network {
    let (event_tx, event_rx) = unbounded();
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let simulated_latency = Duration::from_millis(settings.network.simulated_latency_ms);
    transport::start_network_thread(
        event_tx,
        clients.clone(),
        NetworkThreadSettings {
            bind_address: settings.network.bind_address.clone(),
            simulated_latency,
            simulated_loss_percent: settings.network.simulated_loss_percent,
        },
    );
    Network {
        events: event_rx,
        clients,
        simulated_latency,
        simulated_loss_percent: settings.network.simulated_loss_percent,
        outbound_sequence: 0,
        game_snapshot,
    }
}

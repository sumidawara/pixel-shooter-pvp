//! AdminServerが共有するGameServerレジストリ。

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use pixel_shooter_admin_protocol::{
    AllocationResponse, GameServerRegistration, GameServerStatus, GameServerView,
};
use tokio::sync::{Mutex, RwLock};

pub(crate) const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const MAX_PLAYERS: usize = 4;
/// Join Ticket発行から接続までの猶予。Matchmakerが発行するTicketの有効期限
/// （`TICKET_LIFETIME_SECONDS`）と揃えている。
///
/// この時間を過ぎても接続してこない割当は席を返す。返さないと、GameServerに
/// 拒否された割当や離脱したプレイヤーのぶんだけ席が減り続ける。
pub(crate) const RESERVATION_TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) servers: Arc<RwLock<HashMap<String, ServerRecord>>>,
    pub(crate) allocation_lock: Arc<Mutex<()>>,
    pub(crate) client: reqwest::Client,
}

impl AppState {
    pub(crate) fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
            allocation_lock: Arc::new(Mutex::new(())),
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct ServerRecord {
    pub(crate) registration: GameServerRegistration,
    pub(crate) status: GameServerStatus,
    pub(crate) room_id: Option<String>,
    pub(crate) player_count: usize,
    /// GameServerが報告した、今すぐ参加を受け入れられるか。
    pub(crate) accepting_players: bool,
    /// Join Ticketを発行したが、まだ接続を確認できていない割当の発行時刻。
    pub(crate) reservations: Vec<Instant>,
    pub(crate) tick: u64,
    pub(crate) simulation_mode: pixel_shooter_admin_protocol::SimulationMode,
    pub(crate) last_seen: Instant,
}

impl ServerRecord {
    /// 期限切れの割当を捨てる。
    pub(crate) fn prune_reservations(&mut self, now: Instant) {
        self.reservations
            .retain(|issued| now.duration_since(*issued) < RESERVATION_TTL);
    }

    /// 接続済みと割当済みを合わせて、埋まっているとみなす席数。
    ///
    /// 割当と接続を突き合わせる手段がないため、多いほうを採る。実際に接続すれば
    /// `player_count` が追いつき、接続しなければ割当が期限切れで消える。
    pub(crate) fn occupied_seats(&self) -> usize {
        self.player_count.max(self.reservations.len())
    }
}

pub(crate) fn server_view(server: &ServerRecord) -> GameServerView {
    GameServerView {
        server_id: server.registration.server_id.clone(),
        public_url: server.registration.public_url.clone(),
        control_url: server.registration.control_url.clone(),
        status: server.status,
        room_id: server.room_id.clone(),
        player_count: server.player_count,
        accepting_players: server.accepting_players,
        reserved_players: server.occupied_seats(),
        tick: server.tick,
        simulation_mode: server.simulation_mode,
        healthy: server.last_seen.elapsed() <= HEALTH_TIMEOUT,
    }
}

pub(crate) fn allocation_response(server: &ServerRecord) -> AllocationResponse {
    AllocationResponse {
        server_id: server.registration.server_id.clone(),
        room_id: server.room_id.clone().expect("allocated room"),
        game_url: server.registration.public_url.clone(),
    }
}

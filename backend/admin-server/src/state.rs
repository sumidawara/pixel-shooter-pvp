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
    pub(crate) reserved_players: usize,
    pub(crate) tick: u64,
    pub(crate) simulation_mode: pixel_shooter_admin_protocol::SimulationMode,
    pub(crate) last_seen: Instant,
}

pub(crate) fn server_view(server: &ServerRecord) -> GameServerView {
    GameServerView {
        server_id: server.registration.server_id.clone(),
        public_url: server.registration.public_url.clone(),
        control_url: server.registration.control_url.clone(),
        status: server.status,
        room_id: server.room_id.clone(),
        player_count: server.player_count,
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

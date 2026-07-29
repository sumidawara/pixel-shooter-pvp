//! GameServer登録、heartbeat、ルーム割り当て。

use std::time::Instant;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pixel_shooter_admin_protocol::{
    AllocateRoomRequest, AllocationResponse, GameServerHeartbeat, GameServerRegistration,
    GameServerStatus, GameServerView,
};

use crate::{
    routes::error,
    state::{
        AppState, HEALTH_TIMEOUT, MAX_PLAYERS, ServerRecord, allocation_response, server_view,
    },
};

pub(crate) async fn register(
    State(state): State<AppState>,
    Json(registration): Json<GameServerRegistration>,
) -> StatusCode {
    let mut servers = state.servers.write().await;
    let previous = servers.get(&registration.server_id).cloned();
    servers.insert(
        registration.server_id.clone(),
        ServerRecord {
            registration,
            status: previous
                .as_ref()
                .map_or(GameServerStatus::Available, |server| server.status),
            room_id: previous.as_ref().and_then(|server| server.room_id.clone()),
            player_count: previous.as_ref().map_or(0, |server| server.player_count),
            reserved_players: previous
                .as_ref()
                .map_or(0, |server| server.reserved_players),
            tick: previous.as_ref().map_or(0, |server| server.tick),
            simulation_mode: previous.map_or(
                pixel_shooter_admin_protocol::SimulationMode::Realtime,
                |server| server.simulation_mode,
            ),
            last_seen: Instant::now(),
        },
    );
    StatusCode::NO_CONTENT
}

pub(crate) async fn heartbeat(
    State(state): State<AppState>,
    Json(heartbeat): Json<GameServerHeartbeat>,
) -> Response {
    let mut servers = state.servers.write().await;
    let Some(server) = servers.get_mut(&heartbeat.server_id) else {
        return error(StatusCode::NOT_FOUND, "game_server_not_registered");
    };
    server.status = heartbeat.status;
    server.room_id = heartbeat.room_id;
    server.player_count = heartbeat.player_count;
    server.reserved_players = if heartbeat.status == GameServerStatus::Available {
        0
    } else {
        server.reserved_players.max(heartbeat.player_count)
    };
    server.tick = heartbeat.tick;
    server.simulation_mode = heartbeat.simulation_mode;
    server.last_seen = Instant::now();
    StatusCode::NO_CONTENT.into_response()
}

pub(crate) async fn list_servers(State(state): State<AppState>) -> Json<Vec<GameServerView>> {
    let servers = state.servers.read().await;
    let mut views = servers.values().map(server_view).collect::<Vec<_>>();
    views.sort_by(|left, right| left.server_id.cmp(&right.server_id));
    Json(views)
}

pub(crate) async fn allocate(
    State(state): State<AppState>,
    Json(request): Json<AllocateRoomRequest>,
) -> Response {
    // 2つの同時マッチング要求が同じ空きサーバーを奪わないよう直列化する。
    let _allocation_guard = state.allocation_lock.lock().await;
    let now = Instant::now();

    // 先に、参加枠が残っている既存ルームへ合流させる。
    if let Some(response) = {
        let mut servers = state.servers.write().await;
        servers
            .values_mut()
            .filter(|server| now.duration_since(server.last_seen) <= HEALTH_TIMEOUT)
            .find(|server| {
                server.status == GameServerStatus::Allocated
                    && server.reserved_players < MAX_PLAYERS
                    && server.room_id.is_some()
            })
            .map(|server| {
                server.reserved_players += 1;
                allocation_response(server)
            })
    } {
        return Json(response).into_response();
    }

    let candidate = {
        let servers = state.servers.read().await;
        servers
            .values()
            .filter(|server| now.duration_since(server.last_seen) <= HEALTH_TIMEOUT)
            .find(|server| server.status == GameServerStatus::Available)
            .cloned()
    };
    let Some(candidate) = candidate else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "no_game_server_available");
    };
    let url = format!("{}/internal/allocate", candidate.registration.control_url);
    let response = match state.client.post(url).json(&request).send().await {
        Ok(response) if response.status().is_success() => response,
        _ => return error(StatusCode::BAD_GATEWAY, "game_server_allocation_failed"),
    };
    drop(response);

    let mut servers = state.servers.write().await;
    let Some(server) = servers.get_mut(&candidate.registration.server_id) else {
        return error(StatusCode::CONFLICT, "game_server_disappeared");
    };
    server.status = GameServerStatus::Allocated;
    server.room_id = Some(request.room_id.clone());
    server.reserved_players = 1;
    Json(AllocationResponse {
        server_id: server.registration.server_id.clone(),
        room_id: request.room_id,
        game_url: server.registration.public_url.clone(),
    })
    .into_response()
}

//! 固定GameServerプールの管理とデバッグ画面を提供するAdminServer。

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use pixel_shooter_admin_protocol::{
    AllocateRoomRequest, AllocationResponse, GameServerHeartbeat, GameServerRegistration,
    GameServerStatus, GameServerView, StepRequest,
};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::RwLock};

const INDEX_HTML: &str = include_str!("../../../tools/debug-web/dist/index.html");
const APP_JS: &[u8] = include_bytes!("../../../tools/debug-web/dist/assets/app.js");
const APP_CSS: &[u8] = include_bytes!("../../../tools/debug-web/dist/assets/app.css");
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PLAYERS: usize = 4;

#[derive(Clone)]
struct AppState {
    servers: Arc<RwLock<HashMap<String, ServerRecord>>>,
    allocation_lock: Arc<tokio::sync::Mutex<()>>,
    client: reqwest::Client,
}

#[derive(Clone)]
struct ServerRecord {
    registration: GameServerRegistration,
    status: GameServerStatus,
    room_id: Option<String>,
    player_count: usize,
    reserved_players: usize,
    tick: u64,
    simulation_mode: pixel_shooter_admin_protocol::SimulationMode,
    last_seen: Instant,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    unix_time: u64,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Deserialize)]
struct ServerQuery {
    server_id: Option<String>,
}

#[tokio::main]
async fn main() {
    let bind_address =
        std::env::var("PIXEL_SHOOTER_ADMIN_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8081".into());
    let state = AppState {
        servers: Arc::new(RwLock::new(HashMap::new())),
        allocation_lock: Arc::new(tokio::sync::Mutex::new(())),
        client: reqwest::Client::new(),
    };
    let app = Router::new()
        .route("/", get(|| async { Redirect::temporary("/debug/") }))
        .route("/debug", get(|| async { Redirect::temporary("/debug/") }))
        .route("/debug/", get(index))
        .route("/debug/assets/app.js", get(app_js))
        .route("/debug/assets/app.css", get(app_css))
        .route("/debug/api/health", get(health))
        .route("/debug/api/state", get(debug_snapshot))
        .route("/api/servers", get(list_servers))
        .route("/api/servers/{server_id}/state", get(server_state))
        .route("/api/servers/{server_id}/pause", post(pause))
        .route("/api/servers/{server_id}/step", post(step))
        .route("/api/servers/{server_id}/resume", post(resume))
        .route("/internal/health", get(health))
        .route("/internal/game-servers/register", post(register))
        .route("/internal/game-servers/heartbeat", post(heartbeat))
        .route("/internal/allocate", post(allocate))
        .with_state(state);

    let listener = TcpListener::bind(&bind_address)
        .await
        .expect("bind AdminServer");
    println!("AdminServer listening on http://{bind_address}/debug/");
    axum::serve(listener, app).await.expect("serve AdminServer");
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        unix_time: unix_time(),
    })
}

async fn register(
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

async fn heartbeat(
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

async fn list_servers(State(state): State<AppState>) -> Json<Vec<GameServerView>> {
    let servers = state.servers.read().await;
    let mut views = servers.values().map(server_view).collect::<Vec<_>>();
    views.sort_by(|left, right| left.server_id.cmp(&right.server_id));
    Json(views)
}

async fn allocate(
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

async fn debug_snapshot(
    State(state): State<AppState>,
    Query(query): Query<ServerQuery>,
) -> Response {
    let server = match select_server(&state, query.server_id.as_deref()).await {
        Ok(server) => server,
        Err(response) => return response,
    };
    proxy_get(&state, format!("{}/internal/snapshot", server.control_url)).await
}

async fn pause(State(state): State<AppState>, Path(server_id): Path<String>) -> Response {
    proxy_control(&state, &server_id, "pause", None).await
}

async fn server_state(State(state): State<AppState>, Path(server_id): Path<String>) -> Response {
    let server = match select_server(&state, Some(&server_id)).await {
        Ok(server) => server,
        Err(response) => return response,
    };
    proxy_get(&state, format!("{}/internal/state", server.control_url)).await
}

async fn step(
    State(state): State<AppState>,
    Path(server_id): Path<String>,
    Json(request): Json<StepRequest>,
) -> Response {
    proxy_control(&state, &server_id, "step", Some(request)).await
}

async fn resume(State(state): State<AppState>, Path(server_id): Path<String>) -> Response {
    proxy_control(&state, &server_id, "resume", None).await
}

async fn proxy_control(
    state: &AppState,
    server_id: &str,
    action: &str,
    step: Option<StepRequest>,
) -> Response {
    let server = match select_server(state, Some(server_id)).await {
        Ok(server) => server,
        Err(response) => return response,
    };
    let url = format!("{}/internal/debug/{action}", server.control_url);
    let request = state.client.post(url);
    let response = match step {
        Some(step) => request.json(&step).send().await,
        None => request.send().await,
    };
    proxy_response(response).await
}

async fn select_server(
    state: &AppState,
    requested_id: Option<&str>,
) -> Result<GameServerView, Response> {
    let servers = state.servers.read().await;
    let record = requested_id.and_then(|id| servers.get(id)).or_else(|| {
        servers
            .values()
            .min_by_key(|server| &server.registration.server_id)
    });
    record
        .map(server_view)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "game_server_not_found"))
}

async fn proxy_get(state: &AppState, url: String) -> Response {
    proxy_response(state.client.get(url).send().await).await
}

async fn proxy_response(result: Result<reqwest::Response, reqwest::Error>) -> Response {
    let response = match result {
        Ok(response) => response,
        Err(_) => return error(StatusCode::BAD_GATEWAY, "game_server_unreachable"),
    };
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .unwrap_or_else(|| "application/json".parse().expect("content type"));
    let body = response.bytes().await.unwrap_or_default();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .expect("proxy response")
}

fn server_view(server: &ServerRecord) -> GameServerView {
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

fn allocation_response(server: &ServerRecord) -> AllocationResponse {
    AllocationResponse {
        server_id: server.registration.server_id.clone(),
        room_id: server.room_id.clone().expect("allocated room"),
        game_url: server.registration.public_url.clone(),
    }
}

async fn index() -> Response {
    asset("text/html; charset=utf-8", INDEX_HTML.as_bytes())
}

async fn app_js() -> Response {
    asset("text/javascript; charset=utf-8", APP_JS)
}

async fn app_css() -> Response {
    asset("text/css; charset=utf-8", APP_CSS)
}

fn asset(content_type: &'static str, body: &'static [u8]) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .expect("asset response")
}

fn error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

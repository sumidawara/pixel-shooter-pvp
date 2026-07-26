//! AdminServerからだけ利用するGameServer内部Control API。

use std::{
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};
use pixel_shooter_admin_protocol::{
    AllocateRoomRequest, ControlState, GameServerHeartbeat, GameServerRegistration,
    GameServerStatus, SimulationMode, StepRequest,
};
use pixel_shooter_game_core::{MatchState, Player};
use serde::Serialize;
use tokio::{net::TcpListener, sync::oneshot};

use crate::config::ServerSettings;

pub(crate) type SharedGameSnapshot = Arc<RwLock<Option<String>>>;
type CommandResult = Result<ControlState, String>;

enum ControlCommand {
    Allocate(AllocateRoomRequest, oneshot::Sender<CommandResult>),
    Pause(oneshot::Sender<CommandResult>),
    Step(StepRequest, oneshot::Sender<CommandResult>),
    Resume(oneshot::Sender<CommandResult>),
}

#[derive(Resource)]
pub(crate) struct ControlPlane {
    commands: Receiver<ControlCommand>,
    shared_state: Arc<RwLock<ControlState>>,
    snapshot: SharedGameSnapshot,
}

impl ControlPlane {
    pub(crate) fn snapshot(&self) -> SharedGameSnapshot {
        self.snapshot.clone()
    }
}

#[derive(Resource, Debug)]
pub(crate) struct SimulationControl {
    pub(crate) mode: SimulationMode,
    pub(crate) pending_steps: u64,
}

impl Default for SimulationControl {
    fn default() -> Self {
        Self {
            mode: SimulationMode::Realtime,
            pending_steps: 0,
        }
    }
}

#[derive(Resource, Debug)]
pub(crate) struct AllocationState {
    pub(crate) status: GameServerStatus,
    pub(crate) room_id: Option<String>,
    had_players: bool,
}

impl Default for AllocationState {
    fn default() -> Self {
        Self {
            status: GameServerStatus::Available,
            room_id: None,
            had_players: false,
        }
    }
}

#[derive(Clone)]
struct HttpState {
    commands: Sender<ControlCommand>,
    shared_state: Arc<RwLock<ControlState>>,
    snapshot: SharedGameSnapshot,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

pub(crate) fn start(settings: &ServerSettings) -> ControlPlane {
    let (command_tx, command_rx) = unbounded();
    let shared_state = Arc::new(RwLock::new(ControlState {
        server_id: settings.control.server_id.clone(),
        status: GameServerStatus::Available,
        room_id: None,
        player_count: 0,
        tick: 0,
        simulation_mode: SimulationMode::Realtime,
        pending_steps: 0,
    }));
    let snapshot = Arc::new(RwLock::new(None));
    start_http_thread(
        settings.control.bind_address.clone(),
        settings.control.admin_url.clone(),
        GameServerRegistration {
            server_id: settings.control.server_id.clone(),
            public_url: settings.control.public_url.clone(),
            control_url: settings.control.control_url.clone(),
        },
        HttpState {
            commands: command_tx,
            shared_state: shared_state.clone(),
            snapshot: snapshot.clone(),
        },
    );
    ControlPlane {
        commands: command_rx,
        shared_state,
        snapshot,
    }
}

fn start_http_thread(
    bind_address: String,
    admin_url: String,
    registration: GameServerRegistration,
    state: HttpState,
) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("control Tokio runtime");
        runtime.block_on(async move {
            if !admin_url.is_empty() {
                tokio::spawn(report_to_admin(
                    admin_url,
                    registration,
                    state.shared_state.clone(),
                ));
            }
            let listener = match TcpListener::bind(&bind_address).await {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!("could not bind control server to {bind_address}: {error}");
                    return;
                }
            };
            let app = Router::new()
                .route("/internal/health", get(health))
                .route("/internal/state", get(current_state))
                .route("/internal/snapshot", get(current_snapshot))
                .route("/internal/allocate", post(allocate))
                .route("/internal/debug/pause", post(pause))
                .route("/internal/debug/step", post(step))
                .route("/internal/debug/resume", post(resume))
                .with_state(state);
            println!("GameServer control API listening on http://{bind_address}");
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("control server stopped: {error}");
            }
        });
    });
}

async fn report_to_admin(
    admin_url: String,
    registration: GameServerRegistration,
    shared_state: Arc<RwLock<ControlState>>,
) {
    let client = reqwest::Client::new();
    let register_url = format!("{admin_url}/internal/game-servers/register");
    let heartbeat_url = format!("{admin_url}/internal/game-servers/heartbeat");
    let mut registered = false;
    loop {
        if !registered {
            registered = client
                .post(&register_url)
                .json(&registration)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success());
        } else {
            let state = shared_state.read().expect("control state lock").clone();
            let heartbeat = GameServerHeartbeat {
                server_id: state.server_id,
                status: state.status,
                room_id: state.room_id,
                player_count: state.player_count,
                tick: state.tick,
                simulation_mode: state.simulation_mode,
            };
            let delivered = client
                .post(&heartbeat_url)
                .json(&heartbeat)
                .send()
                .await
                .is_ok_and(|response| response.status().is_success());
            if !delivered {
                registered = false;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "unix_time": unix_time(),
    }))
}

async fn current_state(State(state): State<HttpState>) -> Json<ControlState> {
    Json(
        state
            .shared_state
            .read()
            .expect("control state lock")
            .clone(),
    )
}

async fn current_snapshot(State(state): State<HttpState>) -> Response {
    let snapshot = state.snapshot.read().ok().and_then(|value| value.clone());
    match snapshot {
        Some(json) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(json))
            .expect("valid snapshot response"),
        None => error_response(StatusCode::SERVICE_UNAVAILABLE, "snapshot_not_ready"),
    }
}

async fn allocate(
    State(state): State<HttpState>,
    Json(request): Json<AllocateRoomRequest>,
) -> Response {
    request_command(&state, |reply| ControlCommand::Allocate(request, reply)).await
}

async fn pause(State(state): State<HttpState>) -> Response {
    request_command(&state, ControlCommand::Pause).await
}

async fn step(State(state): State<HttpState>, Json(request): Json<StepRequest>) -> Response {
    request_command(&state, |reply| ControlCommand::Step(request, reply)).await
}

async fn resume(State(state): State<HttpState>) -> Response {
    request_command(&state, ControlCommand::Resume).await
}

async fn request_command(
    state: &HttpState,
    command: impl FnOnce(oneshot::Sender<CommandResult>) -> ControlCommand,
) -> Response {
    let (reply_tx, reply_rx) = oneshot::channel();
    if state.commands.send(command(reply_tx)).is_err() {
        return error_response(StatusCode::SERVICE_UNAVAILABLE, "game_loop_unavailable");
    }
    match reply_rx.await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => error_response(StatusCode::CONFLICT, &error),
        Err(_) => error_response(StatusCode::SERVICE_UNAVAILABLE, "game_loop_stopped"),
    }
}

fn error_response(status: StatusCode, error: &str) -> Response {
    (
        status,
        Json(ErrorBody {
            error: error.into(),
        }),
    )
        .into_response()
}

pub(crate) fn process_commands(
    control: Res<ControlPlane>,
    mut simulation: ResMut<SimulationControl>,
    mut allocation: ResMut<AllocationState>,
    state: Res<MatchState>,
    players: Query<&Player>,
) {
    while let Ok(command) = control.commands.try_recv() {
        let result = match command {
            ControlCommand::Allocate(request, reply) => {
                let result = if allocation.status == GameServerStatus::Available
                    && players.is_empty()
                    && state.phase == pixel_shooter_protocol::MatchPhase::Waiting
                {
                    allocation.status = GameServerStatus::Allocated;
                    allocation.room_id = Some(request.room_id);
                    allocation.had_players = false;
                    simulation.mode = SimulationMode::Realtime;
                    simulation.pending_steps = 0;
                    Ok(build_state(
                        &control,
                        &simulation,
                        &allocation,
                        &state,
                        players.iter().len(),
                    ))
                } else {
                    Err("game_server_not_available".into())
                };
                let _ = reply.send(result.clone());
                result
            }
            ControlCommand::Pause(reply) => {
                simulation.mode = SimulationMode::Paused;
                simulation.pending_steps = 0;
                let result = Ok(build_state(
                    &control,
                    &simulation,
                    &allocation,
                    &state,
                    players.iter().len(),
                ));
                let _ = reply.send(result.clone());
                result
            }
            ControlCommand::Step(request, reply) => {
                let result = if simulation.mode == SimulationMode::Paused {
                    simulation.pending_steps = simulation
                        .pending_steps
                        .saturating_add(request.ticks.clamp(1, 1_000));
                    Ok(build_state(
                        &control,
                        &simulation,
                        &allocation,
                        &state,
                        players.iter().len(),
                    ))
                } else {
                    Err("pause_before_stepping".into())
                };
                let _ = reply.send(result.clone());
                result
            }
            ControlCommand::Resume(reply) => {
                simulation.mode = SimulationMode::Realtime;
                simulation.pending_steps = 0;
                let result = Ok(build_state(
                    &control,
                    &simulation,
                    &allocation,
                    &state,
                    players.iter().len(),
                ));
                let _ = reply.send(result.clone());
                result
            }
        };
        if let Ok(state) = result {
            *control.shared_state.write().expect("control state lock") = state;
        }
    }
}

pub(crate) fn publish_state(
    control: Res<ControlPlane>,
    simulation: Res<SimulationControl>,
    mut allocation: ResMut<AllocationState>,
    state: Res<MatchState>,
    players: Query<&Player>,
) {
    let player_count = players.iter().len();
    if allocation.status == GameServerStatus::Allocated {
        allocation.had_players |= player_count > 0;
        if allocation.had_players
            && player_count == 0
            && state.phase == pixel_shooter_protocol::MatchPhase::Waiting
        {
            allocation.status = GameServerStatus::Available;
            allocation.room_id = None;
            allocation.had_players = false;
        }
    }
    *control.shared_state.write().expect("control state lock") =
        build_state(&control, &simulation, &allocation, &state, player_count);
}

fn build_state(
    control: &ControlPlane,
    simulation: &SimulationControl,
    allocation: &AllocationState,
    state: &MatchState,
    player_count: usize,
) -> ControlState {
    let server_id = control
        .shared_state
        .read()
        .expect("control state lock")
        .server_id
        .clone();
    ControlState {
        server_id,
        status: allocation.status,
        room_id: allocation.room_id.clone(),
        player_count,
        tick: state.tick,
        simulation_mode: simulation.mode,
        pending_steps: simulation.pending_steps,
    }
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

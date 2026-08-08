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
    AllocateRoomRequest, AppliedInputFrame, ControlState, GameServerHeartbeat,
    GameServerRegistration, GameServerStatus, InputScenario, InputScenarioProgress, SimulationMode,
    StepRequest,
};
use pixel_shooter_game_core::{MatchState, Player};
use pixel_shooter_protocol::MatchPhase;
use serde::Serialize;
use tokio::sync::oneshot;

use crate::{bind::listen_with_search, config::ServerSettings};

pub(crate) type SharedGameSnapshot = Arc<RwLock<Option<String>>>;
type CommandResult = Result<ControlState, String>;

enum ControlCommand {
    Allocate(AllocateRoomRequest, oneshot::Sender<CommandResult>),
    Pause(oneshot::Sender<CommandResult>),
    Step(StepRequest, oneshot::Sender<CommandResult>),
    Resume(oneshot::Sender<CommandResult>),
    LoadScenario(InputScenario, oneshot::Sender<CommandResult>),
    ClearScenario(oneshot::Sender<CommandResult>),
}

#[derive(Resource)]
pub(crate) struct ControlPlane {
    /// 実際に開けた制御APIのアドレス。開けなかった場合は`None`。
    pub(crate) bind_address: Option<String>,
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

#[derive(Resource, Debug, Default)]
pub(crate) struct DebugInputScenario {
    name: String,
    frames: Vec<pixel_shooter_admin_protocol::InputFrame>,
    next_frame: usize,
    last_applied: Option<AppliedInputFrame>,
}

impl DebugInputScenario {
    pub(crate) fn take_next(&mut self) -> Option<pixel_shooter_admin_protocol::InputFrame> {
        let frame = self.frames.get(self.next_frame)?.clone();
        self.last_applied = Some(AppliedInputFrame {
            index: self.next_frame,
            frame: frame.clone(),
        });
        self.next_frame += 1;
        Some(frame)
    }

    fn load(&mut self, scenario: InputScenario) {
        self.name = scenario.name;
        self.frames = scenario.frames;
        self.next_frame = 0;
        self.last_applied = None;
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn progress(&self) -> Option<InputScenarioProgress> {
        (!self.frames.is_empty()).then(|| InputScenarioProgress {
            name: self.name.clone(),
            total_frames: self.frames.len(),
            next_frame: self.next_frame,
            last_applied: self.last_applied.clone(),
        })
    }
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

/// 制御APIを開始する。実際に開けたアドレスは`ControlPlane::bind_address`に入る。
///
/// 制御APIが開けなくても対戦そのものは成立するため、失敗しても起動は続ける。
/// ただし黙って落ちると管理機能だけが死んでいることに気付けないので、
/// 結果が返るまで待って理由を表示する。
pub(crate) fn start(settings: &ServerSettings) -> ControlPlane {
    let (command_tx, command_rx) = unbounded();
    let (bind_tx, bind_rx) = unbounded();
    let shared_state = Arc::new(RwLock::new(ControlState {
        server_id: settings.control.server_id.clone(),
        status: GameServerStatus::Available,
        room_id: None,
        player_count: 0,
        tick: 0,
        accepting_players: false,
        simulation_mode: SimulationMode::Realtime,
        pending_steps: 0,
        input_scenario: None,
    }));
    let snapshot = Arc::new(RwLock::new(None));
    start_http_thread(
        settings.control.bind_address.clone(),
        settings.control.port_search_range,
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
        bind_tx,
    );
    let bind_address = match bind_rx.recv() {
        Ok(Ok(address)) => Some(address),
        Ok(Err(error)) => {
            eprintln!("{error}");
            eprintln!("control API is unavailable; the match itself still runs");
            None
        }
        Err(_) => {
            eprintln!("control thread stopped before binding");
            None
        }
    };
    ControlPlane {
        bind_address,
        commands: command_rx,
        shared_state,
        snapshot,
    }
}

fn start_http_thread(
    bind_address: String,
    port_search_range: u32,
    admin_url: String,
    registration: GameServerRegistration,
    state: HttpState,
    bind_result: Sender<Result<String, String>>,
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
            let (listener, bind_address) =
                match listen_with_search(&bind_address, port_search_range).await {
                    Ok(opened) => opened,
                    Err(error) => {
                        let _ = bind_result.send(Err(format!("control server: {error}")));
                        return;
                    }
                };
            let _ = bind_result.send(Ok(bind_address.clone()));
            let app = Router::new()
                .route("/internal/health", get(health))
                .route("/internal/state", get(current_state))
                .route("/internal/snapshot", get(current_snapshot))
                .route("/internal/allocate", post(allocate))
                .route("/internal/debug/pause", post(pause))
                .route("/internal/debug/step", post(step))
                .route("/internal/debug/resume", post(resume))
                .route("/internal/debug/scenario", post(load_scenario))
                .route("/internal/debug/scenario/clear", post(clear_scenario))
                .with_state(state);
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
                accepting_players: state.accepting_players,
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

async fn load_scenario(
    State(state): State<HttpState>,
    Json(scenario): Json<InputScenario>,
) -> Response {
    request_command(&state, |reply| {
        ControlCommand::LoadScenario(scenario, reply)
    })
    .await
}

async fn clear_scenario(State(state): State<HttpState>) -> Response {
    request_command(&state, ControlCommand::ClearScenario).await
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
    mut scenario: ResMut<DebugInputScenario>,
    mut allocation: ResMut<AllocationState>,
    state: Res<MatchState>,
    players: Query<&Player>,
) {
    while let Ok(command) = control.commands.try_recv() {
        let result = match command {
            ControlCommand::Allocate(request, reply) => {
                let result = if allocation.status == GameServerStatus::Available
                    && players.is_empty()
                    && state.phase == MatchPhase::Waiting
                {
                    allocation.status = GameServerStatus::Allocated;
                    allocation.room_id = Some(request.room_id);
                    allocation.had_players = false;
                    simulation.mode = SimulationMode::Realtime;
                    simulation.pending_steps = 0;
                    Ok(build_state(
                        &control,
                        &simulation,
                        &scenario,
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
                    &scenario,
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
                        &scenario,
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
                    &scenario,
                    &allocation,
                    &state,
                    players.iter().len(),
                ));
                let _ = reply.send(result.clone());
                result
            }
            ControlCommand::LoadScenario(input_scenario, reply) => {
                let result = validate_scenario(&input_scenario).map(|()| {
                    simulation.mode = SimulationMode::Paused;
                    simulation.pending_steps = 0;
                    scenario.load(input_scenario);
                    build_state(
                        &control,
                        &simulation,
                        &scenario,
                        &allocation,
                        &state,
                        players.iter().len(),
                    )
                });
                let _ = reply.send(result.clone());
                result
            }
            ControlCommand::ClearScenario(reply) => {
                scenario.clear();
                let result = Ok(build_state(
                    &control,
                    &simulation,
                    &scenario,
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
    scenario: Res<DebugInputScenario>,
    mut allocation: ResMut<AllocationState>,
    state: Res<MatchState>,
    players: Query<&Player>,
) {
    let player_count = players.iter().len();
    if allocation.status == GameServerStatus::Allocated {
        allocation.had_players |= player_count > 0;
        if allocation.had_players && player_count == 0 && state.phase == MatchPhase::Waiting {
            allocation.status = GameServerStatus::Available;
            allocation.room_id = None;
            allocation.had_players = false;
        }
    }
    *control.shared_state.write().expect("control state lock") = build_state(
        &control,
        &simulation,
        &scenario,
        &allocation,
        &state,
        player_count,
    );
}

fn build_state(
    control: &ControlPlane,
    simulation: &SimulationControl,
    scenario: &DebugInputScenario,
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
        // network::process_network の Join 受理条件と同じ判定を、そのまま外へ公開する。
        // ここがずれると、AdminServerが参加できないルームへ案内してしまう。
        accepting_players: state.phase == MatchPhase::Waiting
            && player_count < pixel_shooter_game_core::MAX_PLAYERS,
        tick: state.tick,
        simulation_mode: simulation.mode,
        pending_steps: simulation.pending_steps,
        input_scenario: scenario.progress(),
    }
}

fn validate_scenario(scenario: &InputScenario) -> Result<(), String> {
    if scenario.schema_version != 1 {
        return Err("unsupported_input_scenario_version".into());
    }
    if scenario.name.trim().is_empty() {
        return Err("input_scenario_name_required".into());
    }
    if scenario.frames.is_empty() || scenario.frames.len() > 100_000 {
        return Err("input_scenario_frames_must_be_between_1_and_100000".into());
    }
    for frame in &scenario.frames {
        if frame.inputs.len() > pixel_shooter_game_core::MAX_PLAYERS {
            return Err("too_many_player_inputs_in_frame".into());
        }
        let mut player_ids = std::collections::HashSet::new();
        for command in &frame.inputs {
            if command.player_id == 0 || !player_ids.insert(command.player_id) {
                return Err("invalid_or_duplicate_player_id_in_frame".into());
            }
            let input = command.input;
            if ![input.move_x, input.move_y, input.aim_x, input.aim_y]
                .into_iter()
                .all(f32::is_finite)
            {
                return Err("input_values_must_be_finite".into());
            }
        }
    }
    Ok(())
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pixel_shooter_admin_protocol::{InputFrame, PlayerInput, PlayerInputCommand};

    use super::*;

    fn scenario_with_frames(frame_count: usize) -> InputScenario {
        InputScenario {
            schema_version: 1,
            name: "policy-episode".into(),
            frames: (0..frame_count)
                .map(|index| InputFrame {
                    note: Some(format!("observation {index}")),
                    inputs: vec![PlayerInputCommand {
                        player_id: 2,
                        input: PlayerInput {
                            move_x: index as f32,
                            ..default()
                        },
                        reason: Some("test decision".into()),
                        metadata: BTreeMap::new(),
                    }],
                })
                .collect(),
        }
    }

    #[test]
    fn scenario_advances_exactly_one_frame_at_a_time() {
        let mut queue = DebugInputScenario::default();
        queue.load(scenario_with_frames(2));

        assert_eq!(queue.progress().expect("progress").next_frame, 0);
        assert_eq!(
            queue.take_next().expect("first frame").inputs[0]
                .input
                .move_x,
            0.0
        );
        let progress = queue.progress().expect("progress after first frame");
        assert_eq!(progress.next_frame, 1);
        assert_eq!(progress.last_applied.expect("applied frame").index, 0);
        assert_eq!(
            queue.take_next().expect("second frame").inputs[0]
                .input
                .move_x,
            1.0
        );
        assert!(queue.take_next().is_none());
    }

    #[test]
    fn invalid_scenario_versions_and_duplicate_players_are_rejected() {
        let mut unsupported = scenario_with_frames(1);
        unsupported.schema_version = 2;
        assert_eq!(
            validate_scenario(&unsupported),
            Err("unsupported_input_scenario_version".into())
        );

        let mut duplicate = scenario_with_frames(1);
        let command = duplicate.frames[0].inputs[0].clone();
        duplicate.frames[0].inputs.push(command);
        assert_eq!(
            validate_scenario(&duplicate),
            Err("invalid_or_duplicate_player_id_in_frame".into())
        );
    }
}

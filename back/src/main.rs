//! Pixel Shooter PvP の権威サーバー。
//!
//! このプログラムでは、Godotクライアントは「キーやマウスの入力」だけを送り、
//! プレイヤーの位置・弾・HP・得点などの正しい状態はすべてこのサーバーが決める。
//!
//! Bevy初心者向けの用語:
//! - Entity: ゲーム内の物を識別する番号。プレイヤーや弾に自動で割り当てられる。
//! - Component: Entityに付けるデータ。このファイルでは `Player` と `Bullet`。
//! - Resource: ゲーム世界に1個だけ存在する共有データ。`Network` と `MatchState`。
//! - System: 毎tick実行される普通のRust関数。引数から必要なデータをBevyが渡す。
//! - Query: 指定したComponentを持つEntityを検索・反復するための仕組み。
//! - Commands: Entityの作成・削除を予約する仕組み。

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::{app::ScheduleRunnerPlugin, prelude::*, time::Fixed};
use crossbeam_channel::{Receiver, Sender, unbounded};
use futures_util::{SinkExt, StreamExt};
use pixel_shooter_protocol::{
    ARENA_HEIGHT, ARENA_WIDTH, BULLET_RADIUS, BulletSnapshot, ClientMessage, MatchPhase,
    PLAYER_RADIUS, PlayerSnapshot, ServerMessage, Snapshot, Vec2 as NetVec2,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

const TICK_RATE: f64 = 60.0;
const ROUND_SECONDS: f32 = 60.0;
const COUNTDOWN_SECONDS: f32 = 3.0;
const OVERTIME_SECONDS: f32 = 30.0;
const ROUND_INTERVAL_SECONDS: f32 = 3.0;
const MATCH_FINISHED_SECONDS: f32 = 6.0;
const RECONNECT_GRACE_SECONDS: f32 = 15.0;
const ROUNDS_TO_WIN: u32 = 3;
const MOVE_SPEED: f32 = 150.0;
const BULLET_SPEED: f32 = 340.0;
const SHOT_INTERVAL: f32 = 0.24;
const RECOIL_DISTANCE: f32 = 5.0;
const MAX_AMMO: u32 = 6;
const RELOAD_SECONDS: f32 = 1.0;
const HIT_INVULNERABLE_SECONDS: f32 = 0.18;
const RESPAWN_INVULNERABLE_SECONDS: f32 = 1.0;
const DASH_SPEED: f32 = 520.0;
const DASH_DURATION: f32 = 0.13;
const DASH_COOLDOWN: f32 = 1.1;
const RESPAWN_SECONDS: f32 = 2.0;
const MAX_HP: i32 = 5;
const MAX_PLAYERS: usize = 2;

// 接続IDから、そのクライアントへメッセージを送るチャンネルを検索する表。
// WebSocketは別スレッドで動くため、Arcで複数スレッドから共有し、
// Mutexで同時アクセスからHashMapを保護する。
type ClientSenders = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<OutboundMessage>>>>;

/// 通信試験用の遅延を伴う送信メッセージ。
struct OutboundMessage {
    message: Message,
    delay: Duration,
}

/// 通信スレッドとBevyのゲーム世界をつなぐResource。
///
/// `#[derive(Resource)]` を付けると、Bevyの `App` に1個だけ登録できる。
#[derive(Resource)]
struct Network {
    /// 通信スレッドから届いたイベントをBevy側で受信する。
    events: Receiver<NetworkEvent>,
    /// Bevy側から接続中のクライアントへ送信するときに使う。
    clients: ClientSenders,
    /// スナップショットへ人工的に加える片道遅延。
    simulated_latency: Duration,
    /// 0〜100で指定する人工的なパケット欠落率。
    simulated_loss_percent: u32,
    /// 欠落判定を再現可能にするための連番。
    outbound_sequence: u64,
}

/// 通信スレッドで起きた出来事をBevyのメインスレッドへ渡すための値。
enum NetworkEvent {
    Connected(u64),
    Disconnected(u64),
    Message(u64, ClientMessage),
}

/// 試合全体で1つだけ存在する状態。
///
/// プレイヤーごとのデータではないのでComponentではなくResourceにしている。
#[derive(Resource, Default)]
struct MatchState {
    /// サーバーが何回固定更新を実行したか。
    tick: u64,
    phase: MatchPhase,
    /// 現在のフェーズの残り時間。Runningではラウンド残り時間になる。
    phase_time_left: f32,
    /// 切断による一時停止から戻るフェーズ。
    resume_phase: Option<MatchPhase>,
    round_number: u32,
    round_winner_id: Option<u64>,
    match_winner_id: Option<u64>,
    next_bullet_id: u64,
    next_player_id: u64,
    reconnect_grace_seconds: f32,
}

/// プレイヤーEntityに付けるComponent。
///
/// Bevyでは継承を使った「Playerクラス」を作る代わりに、
/// Entityへ必要なComponentを付けてゲームオブジェクトを表現する。
#[derive(Component)]
struct Player {
    /// 試合中変わらないプレイヤーID。WebSocketの接続IDとは別。
    id: u64,
    /// 現在このプレイヤーを操作しているWebSocket接続。
    connection_id: Option<u64>,
    reconnect_token: String,
    reconnect_grace_left: f32,
    slot: usize,
    name: String,
    position: Vec2,
    aim: Vec2,
    movement: Vec2,
    shooting: bool,
    hp: i32,
    score: u32,
    round_wins: u32,
    alive: bool,
    respawn_left: f32,
    shot_cooldown: f32,
    ammo: u32,
    reload_left: f32,
    reload_requested: bool,
    invulnerable_left: f32,
    dash_cooldown_left: f32,
    dash_time_left: f32,
    dash_direction: Vec2,
    dash_requested: bool,
    last_input_sequence: u32,
}

/// 発射された弾Entityに付けるComponent。
#[derive(Component)]
struct Bullet {
    id: u64,
    owner_id: u64,
    position: Vec2,
    velocity: Vec2,
    life_left: f32,
}

fn main() {
    // WebSocketスレッドからBevyへイベントを渡すクロススレッド用チャンネル。
    let (event_tx, event_rx) = unbounded();
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let bind_address =
        std::env::var("PIXEL_SHOOTER_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:9001".into());
    let simulated_latency_ms = env_u64("PIXEL_SHOOTER_LATENCY_MS", 0);
    let simulated_loss_percent = env_u64("PIXEL_SHOOTER_PACKET_LOSS_PERCENT", 0).min(100) as u32;
    let reconnect_grace_seconds = env_f32(
        "PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS",
        RECONNECT_GRACE_SECONDS,
    )
    .max(0.1);
    let simulated_latency = Duration::from_millis(simulated_latency_ms);
    start_network_thread(
        event_tx,
        clients.clone(),
        bind_address.clone(),
        simulated_latency,
        simulated_loss_percent,
    );

    println!("Pixel Shooter server listening on ws://{bind_address}");
    if simulated_latency_ms > 0 || simulated_loss_percent > 0 {
        println!(
            "Network simulation: {simulated_latency_ms} ms latency, \
             {simulated_loss_percent}% snapshot loss"
        );
    }

    // AppはBevyアプリケーション全体を組み立てる入口。
    App::new()
        .add_plugins(
            // サーバーでは画面、音声、ウィンドウが不要なのでMinimalPluginsを使う。
            // ScheduleRunnerPluginにより、ウィンドウのイベントループなしで60Hz動作する。
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / TICK_RATE,
            ))),
        )
        // FixedUpdateが1秒間に60回進むよう、固定時間を設定する。
        .insert_resource(Time::<Fixed>::from_hz(TICK_RATE))
        // 作成済みの値をResourceとしてゲーム世界へ登録する。
        .insert_resource(Network {
            events: event_rx,
            clients,
            simulated_latency,
            simulated_loss_percent,
            outbound_sequence: 0,
        })
        .insert_resource(MatchState {
            reconnect_grace_seconds,
            ..default()
        })
        .add_systems(
            FixedUpdate,
            (
                process_network,
                update_match,
                move_players,
                fire_bullets,
                move_and_hit_bullets,
                update_respawns,
                broadcast_snapshot,
            )
                // chain()により上から順番に実行する。
                // 例えば入力を反映してから移動し、その後にスナップショットを送る。
                .chain(),
        )
        // サーバーを終了するまでBevyの更新ループを実行する。
        .run();
}

/// TokioとWebSocketを動かす専用OSスレッドを開始する。
///
/// 非同期通信をBevyのSystem内で待つとゲーム更新が止まるため、
/// 通信は別スレッド、ゲーム計算はBevyのメインスレッドと役割を分ける。
fn start_network_thread(
    events: Sender<NetworkEvent>,
    clients: ClientSenders,
    bind_address: String,
    simulated_latency: Duration,
    simulated_loss_percent: u32,
) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio runtime");
        runtime.block_on(async move {
            let listener = TcpListener::bind(&bind_address)
                .await
                .expect("bind websocket server");
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
                            simulated_latency,
                            simulated_loss_percent,
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
                        ClientMessage::Join { .. } => None,
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

/// 通信イベントをゲーム世界へ反映するSystem。
///
/// System引数の意味:
/// - `Commands`: Entityの作成・削除を予約する
/// - `Res<Network>`: 読み取り専用でNetwork Resourceを借りる
/// - `Query<(Entity, &mut Player)>`: Playerを持つ全EntityとPlayerデータを変更可能で取得する
fn process_network(
    mut commands: Commands,
    network: Res<Network>,
    mut state: ResMut<MatchState>,
    mut players: Query<(Entity, &mut Player)>,
) {
    // 同じtickに2人がJoinしても、Commandsで予約中のslotと重ならないよう保持する。
    let mut occupied_slots: Vec<usize> = players.iter().map(|(_, player)| player.slot).collect();

    // try_recvは待たずに受信する。通信がなくてもゲームループを止めないため。
    while let Ok(event) = network.events.try_recv() {
        match event {
            NetworkEvent::Connected(id) => println!("client {id} connected"),
            NetworkEvent::Disconnected(connection_id) => {
                for (_, mut player) in &mut players {
                    if player.connection_id == Some(connection_id) {
                        player.connection_id = None;
                        player.reconnect_grace_left = state.reconnect_grace_seconds;
                        player.movement = Vec2::ZERO;
                        player.shooting = false;
                        player.reload_requested = false;
                        player.dash_requested = false;
                        println!(
                            "player {} disconnected; waiting {} seconds",
                            player.id, state.reconnect_grace_seconds
                        );
                    }
                }
            }
            NetworkEvent::Message(
                connection_id,
                ClientMessage::Join {
                    name,
                    reconnect_token,
                },
            ) => {
                // 同じ接続からJoinが再送されてもPlayerを重複作成しない。
                if players
                    .iter()
                    .any(|(_, player)| player.connection_id == Some(connection_id))
                {
                    continue;
                }

                // 有効なトークンなら、以前のPlayer Entityへ新しい接続を結び直す。
                let mut reconnected = None;
                if let Some(token) = reconnect_token.filter(|token| !token.is_empty()) {
                    for (_, mut player) in &mut players {
                        if player.reconnect_token == token && player.connection_id.is_none() {
                            player.connection_id = Some(connection_id);
                            player.reconnect_grace_left = 0.0;
                            player.last_input_sequence = 0;
                            reconnected = Some((player.id, player.reconnect_token.clone()));
                            break;
                        }
                    }
                }
                if let Some((player_id, token)) = reconnected {
                    send_to(
                        &network,
                        connection_id,
                        &ServerMessage::Welcome {
                            player_id,
                            reconnect_token: token,
                            reconnected: true,
                        },
                    );
                    println!("player {player_id} reconnected");
                    continue;
                }

                if occupied_slots.len() >= MAX_PLAYERS {
                    send_to(
                        &network,
                        connection_id,
                        &ServerMessage::Rejected {
                            reason: "The arena already has two players. Reconnect with your token."
                                .into(),
                        },
                    );
                    continue;
                }

                let slot = (0..MAX_PLAYERS)
                    .find(|slot| !occupied_slots.contains(slot))
                    .expect("available player slot");
                occupied_slots.push(slot);
                state.next_player_id += 1;
                let player_id = state.next_player_id;
                let token = generate_reconnect_token();
                let position = spawn_position(slot);
                // spawnすると新しいEntityが作られ、Player Componentが付く。
                commands.spawn(Player {
                    id: player_id,
                    connection_id: Some(connection_id),
                    reconnect_token: token.clone(),
                    reconnect_grace_left: 0.0,
                    slot,
                    name: sanitize_name(&name, player_id),
                    position,
                    aim: if slot == 0 { Vec2::X } else { Vec2::NEG_X },
                    movement: Vec2::ZERO,
                    shooting: false,
                    hp: MAX_HP,
                    score: 0,
                    round_wins: 0,
                    alive: true,
                    respawn_left: 0.0,
                    shot_cooldown: 0.0,
                    ammo: MAX_AMMO,
                    reload_left: 0.0,
                    reload_requested: false,
                    invulnerable_left: RESPAWN_INVULNERABLE_SECONDS,
                    dash_cooldown_left: 0.0,
                    dash_time_left: 0.0,
                    dash_direction: Vec2::ZERO,
                    dash_requested: false,
                    last_input_sequence: 0,
                });
                send_to(
                    &network,
                    connection_id,
                    &ServerMessage::Welcome {
                        player_id,
                        reconnect_token: token,
                        reconnected: false,
                    },
                );
                println!("player {player_id} joined in slot {slot}");
            }
            NetworkEvent::Message(
                connection_id,
                ClientMessage::Input {
                    sequence,
                    move_x,
                    move_y,
                    aim_x,
                    aim_y,
                    shooting,
                    reload_pressed,
                    dash_pressed,
                },
            ) => {
                for (_, mut player) in &mut players {
                    // 古いsequenceの入力を後から適用すると巻き戻るため破棄する。
                    if player.connection_id != Some(connection_id)
                        || sequence <= player.last_input_sequence
                    {
                        continue;
                    }
                    player.last_input_sequence = sequence;
                    // 斜め移動だけ速くならないよう、入力ベクトルの長さを最大1にする。
                    player.movement = Vec2::new(move_x, move_y).clamp_length_max(1.0);
                    let aim = Vec2::new(aim_x, aim_y);
                    if aim.length_squared() > 0.001 {
                        player.aim = aim.normalize();
                    }
                    player.shooting = shooting;
                    // 押した瞬間だけtrueになる操作は、Systemで消費するまでORで保持する。
                    player.reload_requested |= reload_pressed;
                    player.dash_requested |= dash_pressed;
                }
            }
        }
    }
}

/// 待機から3ラウンド先取までの状態遷移を管理するSystem。
///
/// `ResMut<MatchState>` はResourceを変更可能で借りる指定。
/// `Query<Entity, With<Bullet>>` はBulletを持つEntity番号だけを取得するフィルター。
fn update_match(
    time: Res<Time<Fixed>>,
    mut state: ResMut<MatchState>,
    mut players: Query<(Entity, &mut Player)>,
    bullets: Query<Entity, With<Bullet>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();

    // 切断中のプレイヤーには猶予時間を与え、Entityと試合状態を保持する。
    let mut expired = Vec::new();
    for (entity, mut player) in &mut players {
        if player.connection_id.is_none() {
            player.reconnect_grace_left = (player.reconnect_grace_left - dt).max(0.0);
            if player.reconnect_grace_left <= 0.0 {
                expired.push((entity, player.id));
            }
        }
    }

    if !expired.is_empty() {
        let connected_winner = players
            .iter()
            .find(|(_, player)| player.connection_id.is_some())
            .map(|(_, player)| player.id);
        let match_was_active = state.phase == MatchPhase::Paused
            || matches!(
                state.phase,
                MatchPhase::Countdown
                    | MatchPhase::Running
                    | MatchPhase::Overtime
                    | MatchPhase::RoundEnd
            );
        for (entity, player_id) in expired {
            commands.entity(entity).despawn();
            println!("player {player_id} reconnect grace expired");
        }
        if match_was_active {
            state.phase = MatchPhase::MatchFinished;
            state.phase_time_left = MATCH_FINISHED_SECONDS;
            state.match_winner_id = connected_winner;
            state.round_winner_id = None;
            state.resume_phase = None;
            despawn_all_bullets(&mut commands, &bullets);
            println!("match ended by forfeit; winner: {connected_winner:?}");
        }
        state.tick += 1;
        return;
    }

    let connected_count = players
        .iter()
        .filter(|(_, player)| player.connection_id.is_some())
        .count();
    let has_disconnected_player = players
        .iter()
        .any(|(_, player)| player.connection_id.is_none());

    // 対戦中の切断はタイマーとゲーム計算を止める。復帰後は同じフェーズへ戻る。
    if has_disconnected_player
        && !matches!(
            state.phase,
            MatchPhase::Waiting | MatchPhase::MatchFinished | MatchPhase::Paused
        )
    {
        state.resume_phase = Some(state.phase);
        state.phase = MatchPhase::Paused;
        for (_, mut player) in &mut players {
            player.movement = Vec2::ZERO;
            player.shooting = false;
        }
    } else if !has_disconnected_player && state.phase == MatchPhase::Paused {
        state.phase = state.resume_phase.take().unwrap_or(MatchPhase::Countdown);
        println!("match resumed after reconnect");
    }

    match state.phase {
        MatchPhase::Waiting => {
            if connected_count == MAX_PLAYERS {
                start_new_match(&mut state, &mut players);
                despawn_all_bullets(&mut commands, &bullets);
            }
        }
        MatchPhase::Countdown => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.phase = MatchPhase::Running;
                state.phase_time_left = ROUND_SECONDS;
                println!("round {} started", state.round_number);
            }
        }
        MatchPhase::Running => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                let mut standings: Vec<(u64, u32)> = players
                    .iter()
                    .map(|(_, player)| (player.id, player.score))
                    .collect();
                standings.sort_by_key(|(_, score)| *score);
                if standings.len() == 2 && standings[0].1 == standings[1].1 {
                    state.phase = MatchPhase::Overtime;
                    state.phase_time_left = OVERTIME_SECONDS;
                    println!("round {} entered overtime", state.round_number);
                } else if let Some((winner_id, _)) = standings.last() {
                    award_round(&mut state, &mut players, *winner_id);
                    despawn_all_bullets(&mut commands, &bullets);
                }
            }
        }
        MatchPhase::Overtime => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                let mut standings: Vec<(u64, i32)> = players
                    .iter()
                    .map(|(_, player)| (player.id, player.hp))
                    .collect();
                standings.sort_by_key(|(_, hp)| *hp);
                if standings.len() == 2 && standings[0].1 == standings[1].1 {
                    // HPまで同じなら10秒ずつサドンデスを継続する。
                    state.phase_time_left = 10.0;
                } else if let Some((winner_id, _)) = standings.last() {
                    award_round(&mut state, &mut players, *winner_id);
                    despawn_all_bullets(&mut commands, &bullets);
                }
            }
        }
        MatchPhase::RoundEnd => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.round_number += 1;
                prepare_round(&mut players);
                state.round_winner_id = None;
                state.phase = MatchPhase::Countdown;
                state.phase_time_left = COUNTDOWN_SECONDS;
                despawn_all_bullets(&mut commands, &bullets);
            }
        }
        MatchPhase::MatchFinished => {
            state.phase_time_left = (state.phase_time_left - dt).max(0.0);
            if state.phase_time_left <= 0.0 {
                state.match_winner_id = None;
                state.round_winner_id = None;
                state.round_number = 0;
                for (_, mut player) in &mut players {
                    player.round_wins = 0;
                    player.score = 0;
                    reset_player(&mut player);
                }
                if connected_count == MAX_PLAYERS {
                    start_new_match(&mut state, &mut players);
                } else {
                    state.phase = MatchPhase::Waiting;
                    state.phase_time_left = 0.0;
                }
            }
        }
        MatchPhase::Paused => {}
    }

    state.tick += 1;
}

fn start_new_match(state: &mut MatchState, players: &mut Query<(Entity, &mut Player)>) {
    state.round_number = 1;
    state.round_winner_id = None;
    state.match_winner_id = None;
    state.resume_phase = None;
    state.phase = MatchPhase::Countdown;
    state.phase_time_left = COUNTDOWN_SECONDS;
    for (_, mut player) in players.iter_mut() {
        player.round_wins = 0;
        player.score = 0;
        reset_player(&mut player);
    }
    println!("new best-of-five match started");
}

fn prepare_round(players: &mut Query<(Entity, &mut Player)>) {
    for (_, mut player) in players.iter_mut() {
        player.score = 0;
        reset_player(&mut player);
    }
}

fn award_round(state: &mut MatchState, players: &mut Query<(Entity, &mut Player)>, winner_id: u64) {
    let mut match_won = false;
    for (_, mut player) in players.iter_mut() {
        if player.id == winner_id {
            player.round_wins += 1;
            match_won = player.round_wins >= ROUNDS_TO_WIN;
            break;
        }
    }
    set_round_result(state, winner_id, match_won);
}

fn set_round_result(state: &mut MatchState, winner_id: u64, match_won: bool) {
    state.round_winner_id = Some(winner_id);
    if match_won {
        state.phase = MatchPhase::MatchFinished;
        state.phase_time_left = MATCH_FINISHED_SECONDS;
        state.match_winner_id = Some(winner_id);
        println!("player {winner_id} won the match");
    } else {
        state.phase = MatchPhase::RoundEnd;
        state.phase_time_left = ROUND_INTERVAL_SECONDS;
        println!("player {winner_id} won round {}", state.round_number);
    }
}

fn despawn_all_bullets(commands: &mut Commands, bullets: &Query<Entity, With<Bullet>>) {
    for entity in bullets {
        commands.entity(entity).despawn();
    }
}

fn is_playing_phase(phase: MatchPhase) -> bool {
    matches!(phase, MatchPhase::Running | MatchPhase::Overtime)
}

/// クライアントから受け取った移動入力でプレイヤーを動かすSystem。
fn move_players(time: Res<Time<Fixed>>, state: Res<MatchState>, mut players: Query<&mut Player>) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = time.delta_secs();
    for mut player in &mut players {
        if !player.alive {
            player.reload_requested = false;
            player.dash_requested = false;
            continue;
        }
        player.shot_cooldown = (player.shot_cooldown - dt).max(0.0);
        player.invulnerable_left = (player.invulnerable_left - dt).max(0.0);
        player.dash_cooldown_left = (player.dash_cooldown_left - dt).max(0.0);

        // Rキーが押され、まだ弾が残っていない場合だけ手動リロードを始める。
        if player.reload_requested && player.reload_left <= 0.0 && player.ammo < MAX_AMMO {
            player.reload_left = RELOAD_SECONDS;
        }
        player.reload_requested = false;

        if player.reload_left > 0.0 {
            player.reload_left = (player.reload_left - dt).max(0.0);
            if player.reload_left <= 0.0 {
                player.ammo = MAX_AMMO;
            }
        }

        // Spaceが押された瞬間に、現在の移動入力方向へダッシュを開始する。
        if player.dash_requested
            && player.dash_cooldown_left <= 0.0
            && player.movement.length_squared() > 0.001
        {
            player.dash_direction = player.movement.normalize();
            player.dash_time_left = DASH_DURATION;
            player.dash_cooldown_left = DASH_COOLDOWN;
        }
        player.dash_requested = false;

        // ダッシュ中は通常入力ではなく、開始時に保存した方向へ高速移動する。
        let (direction, speed) = if player.dash_time_left > 0.0 {
            player.dash_time_left = (player.dash_time_left - dt).max(0.0);
            (player.dash_direction, DASH_SPEED)
        } else {
            (player.movement, MOVE_SPEED)
        };

        // 速度(px/秒) × 経過秒で、このtickに進む距離を求める。
        let delta = direction * speed * dt;

        // X軸とY軸を別々に判定する。
        // まとめて移動すると、片方の軸が壁に当たっただけで両方向とも止まってしまう。
        move_with_collision(&mut player.position, delta);
    }
}

/// 射撃入力とクールダウンを確認し、Bullet Entityを生成するSystem。
fn fire_bullets(
    mut commands: Commands,
    mut state: ResMut<MatchState>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    for mut player in &mut players {
        if !player.alive
            || !player.shooting
            || player.shot_cooldown > 0.0
            || player.reload_left > 0.0
            || player.dash_time_left > 0.0
        {
            continue;
        }
        if player.ammo == 0 {
            player.reload_left = RELOAD_SECONDS;
            continue;
        }

        player.shot_cooldown = SHOT_INTERVAL;
        player.ammo -= 1;
        state.next_bullet_id += 1;
        let aim = player.aim;
        // プレイヤー中心に弾を置くと自分と重なるため、照準方向へ少し前に出す。
        commands.spawn(Bullet {
            id: state.next_bullet_id,
            owner_id: player.id,
            position: player.position + aim * (PLAYER_RADIUS + 6.0),
            velocity: aim * BULLET_SPEED,
            life_left: 2.0,
        });

        // 射撃方向と反対へ少し押し戻す。サーバーで計算するので全員に同じ結果になる。
        move_with_collision(&mut player.position, -aim * RECOIL_DISTANCE);

        // 最後の1発を撃った直後から自動リロードを開始する。
        if player.ammo == 0 {
            player.reload_left = RELOAD_SECONDS;
        }
    }
}

/// 弾の移動、壁との衝突、プレイヤーへのダメージを処理するSystem。
fn move_and_hit_bullets(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut state: ResMut<MatchState>,
    mut bullets: Query<(Entity, &mut Bullet)>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = time.delta_secs();
    for (entity, mut bullet) in &mut bullets {
        // Rustの借用規則上、positionを変更しながらvelocityを読む式を分けている。
        let velocity = bullet.velocity;
        bullet.position += velocity * dt;
        bullet.life_left -= dt;
        if bullet.life_left <= 0.0
            || !bullet_in_bounds(bullet.position)
            || obstacle_at(bullet.position, 0.0)
        {
            // 寿命切れ、画面外、障害物への衝突のどれかなら弾を削除する。
            commands.entity(entity).despawn();
            continue;
        }

        let mut hit = false;
        let mut killed = false;
        let owner_id = bullet.owner_id;
        for mut player in &mut players {
            if !player.alive || player.id == owner_id || player.invulnerable_left > 0.0 {
                continue;
            }
            // 円同士の当たり判定。sqrtを避けるため距離も半径も二乗して比較する。
            let hit_distance = PLAYER_RADIUS + BULLET_RADIUS;
            if player.position.distance_squared(bullet.position) <= hit_distance * hit_distance {
                player.hp -= 1;
                player.invulnerable_left = HIT_INVULNERABLE_SECONDS;
                hit = true;
                if player.hp <= 0 {
                    player.alive = false;
                    player.respawn_left = RESPAWN_SECONDS;
                    player.shooting = false;
                    killed = true;
                }
                break;
            }
        }
        if hit {
            // 1つの弾は1回だけダメージを与える。
            commands.entity(entity).despawn();
            if killed {
                // 撃破した弾のowner_idと一致するプレイヤーへ1点追加する。
                let overtime_kill = state.phase == MatchPhase::Overtime;
                let mut match_won = false;
                for mut player in &mut players {
                    if player.id == owner_id {
                        player.score += 1;
                        if overtime_kill {
                            player.round_wins += 1;
                            match_won = player.round_wins >= ROUNDS_TO_WIN;
                        }
                        break;
                    }
                }
                if overtime_kill {
                    set_round_result(&mut state, owner_id, match_won);
                }
            }
        }
    }
}

/// 死亡したプレイヤーの復活カウントを進めるSystem。
fn update_respawns(
    time: Res<Time<Fixed>>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
    bullets: Query<&Bullet>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = time.delta_secs();
    // 下のループではPlayerを変更可能で借りるため、先に全員の位置だけコピーしておく。
    let positions: Vec<(u64, Vec2)> = players.iter().map(|p| (p.id, p.position)).collect();
    let bullet_positions: Vec<Vec2> = bullets.iter().map(|bullet| bullet.position).collect();
    for mut player in &mut players {
        if player.alive {
            continue;
        }
        player.respawn_left = (player.respawn_left - dt).max(0.0);
        if player.respawn_left <= 0.0 {
            // 複数候補から相手と弾に最も近づきにくい地点を選ぶ。
            player.position =
                choose_respawn_position(player.id, &positions, &bullet_positions, state.tick);
            player.hp = MAX_HP;
            player.alive = true;
            player.shot_cooldown = 0.3;
            player.ammo = MAX_AMMO;
            player.reload_left = 0.0;
            player.invulnerable_left = RESPAWN_INVULNERABLE_SECONDS;
            player.dash_time_left = 0.0;
        }
    }
}

/// 現在のゲーム状態を全Godotクライアントへ送るSystem。
fn broadcast_snapshot(
    state: Res<MatchState>,
    mut network: ResMut<Network>,
    players: Query<&Player>,
    bullets: Query<&Bullet>,
) {
    // サーバー更新は60Hzだが、3tickに1回だけ送ることで通信は20Hzになる。
    if !state.tick.is_multiple_of(3) {
        return;
    }
    let reconnect_grace_left = players
        .iter()
        .filter(|player| player.connection_id.is_none())
        .map(|player| player.reconnect_grace_left)
        .reduce(f32::min)
        .unwrap_or(0.0);

    // Bevy内部のComponentを、通信専用のSnapshot型へ詰め替える。
    // 内部データを直接シリアライズしないことで、通信仕様とゲーム実装を分離できる。
    let snapshot = ServerMessage::Snapshot(Snapshot {
        tick: state.tick,
        phase: state.phase,
        time_left: state.phase_time_left,
        round_number: state.round_number,
        rounds_to_win: ROUNDS_TO_WIN,
        round_winner_id: state.round_winner_id,
        winner_id: state.match_winner_id,
        reconnect_grace_left,
        players: players
            .iter()
            .map(|player| PlayerSnapshot {
                id: player.id,
                name: player.name.clone(),
                position: net_vec(player.position),
                aim: net_vec(player.aim),
                hp: player.hp.max(0),
                max_hp: MAX_HP,
                score: player.score,
                round_wins: player.round_wins,
                connected: player.connection_id.is_some(),
                reconnect_grace_left: player.reconnect_grace_left,
                alive: player.alive,
                respawn_left: player.respawn_left,
                invulnerable_left: player.invulnerable_left,
                ammo: player.ammo,
                max_ammo: MAX_AMMO,
                reloading: player.reload_left > 0.0,
                reload_left: player.reload_left,
                dash_cooldown_left: player.dash_cooldown_left,
                dashing: player.dash_time_left > 0.0,
                dash_time_left: player.dash_time_left,
                last_input_sequence: player.last_input_sequence,
            })
            .collect(),
        bullets: if is_playing_phase(state.phase) {
            bullets
                .iter()
                .map(|bullet| BulletSnapshot {
                    id: bullet.id,
                    owner_id: bullet.owner_id,
                    position: net_vec(bullet.position),
                    velocity: net_vec(bullet.velocity),
                })
                .collect()
        } else {
            Vec::new()
        },
    });
    broadcast(&mut network, &snapshot);
}

/// 新しい試合の開始時にプレイヤー状態を初期化する。
fn reset_player(player: &mut Player) {
    player.position = spawn_position(player.slot);
    player.hp = MAX_HP;
    player.alive = true;
    player.respawn_left = 0.0;
    player.shot_cooldown = 0.0;
    player.ammo = MAX_AMMO;
    player.reload_left = 0.0;
    player.reload_requested = false;
    player.invulnerable_left = RESPAWN_INVULNERABLE_SECONDS;
    player.dash_cooldown_left = 0.0;
    player.dash_time_left = 0.0;
    player.dash_requested = false;
}

/// 参加順に応じた左右の初期位置を返す。
fn spawn_position(index: usize) -> Vec2 {
    if index == 0 {
        Vec2::new(80.0, ARENA_HEIGHT * 0.5)
    } else {
        Vec2::new(ARENA_WIDTH - 80.0, ARENA_HEIGHT * 0.5)
    }
}

/// 相手との距離を主に、近くの弾と小さな揺らぎも考慮して復活地点を選ぶ。
fn choose_respawn_position(
    player_id: u64,
    player_positions: &[(u64, Vec2)],
    bullet_positions: &[Vec2],
    tick: u64,
) -> Vec2 {
    let candidates = [
        Vec2::new(64.0, 60.0),
        Vec2::new(64.0, ARENA_HEIGHT - 60.0),
        Vec2::new(ARENA_WIDTH - 64.0, 60.0),
        Vec2::new(ARENA_WIDTH - 64.0, ARENA_HEIGHT - 60.0),
        Vec2::new(92.0, ARENA_HEIGHT * 0.5),
        Vec2::new(ARENA_WIDTH - 92.0, ARENA_HEIGHT * 0.5),
    ];

    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| valid_position(**candidate))
        .max_by(|(left_index, left), (right_index, right)| {
            let left_score = respawn_safety_score(
                player_id,
                **left,
                *left_index,
                player_positions,
                bullet_positions,
                tick,
            );
            let right_score = respawn_safety_score(
                player_id,
                **right,
                *right_index,
                player_positions,
                bullet_positions,
                tick,
            );
            left_score.total_cmp(&right_score)
        })
        .map(|(_, position)| *position)
        .unwrap_or_else(|| spawn_position((player_id as usize) % MAX_PLAYERS))
}

fn respawn_safety_score(
    player_id: u64,
    candidate: Vec2,
    candidate_index: usize,
    player_positions: &[(u64, Vec2)],
    bullet_positions: &[Vec2],
    tick: u64,
) -> f32 {
    let opponent_distance = player_positions
        .iter()
        .filter(|(id, _)| *id != player_id)
        .map(|(_, position)| candidate.distance(*position))
        .fold(ARENA_WIDTH, f32::min);
    let bullet_distance = bullet_positions
        .iter()
        .map(|position| candidate.distance(*position))
        .fold(ARENA_WIDTH, f32::min);

    // 毎回完全に同じ地点にならないよう、距離評価を壊さない範囲で決定的な揺らぎを加える。
    let variation = ((tick + player_id * 31 + candidate_index as u64 * 17) % 11) as f32;
    opponent_distance + bullet_distance * 0.25 + variation
}

/// X・Y軸を分けて、衝突しない分だけ位置を更新する。
fn move_with_collision(position: &mut Vec2, delta: Vec2) {
    let mut next = *position;
    next.x += delta.x;
    if valid_position(next) {
        position.x = next.x;
    }
    next = *position;
    next.y += delta.y;
    if valid_position(next) {
        position.y = next.y;
    }
}

/// プレイヤーが移動できる位置かを、外周と障害物から判定する。
fn valid_position(position: Vec2) -> bool {
    position.x >= PLAYER_RADIUS
        && position.x <= ARENA_WIDTH - PLAYER_RADIUS
        && position.y >= PLAYER_RADIUS
        && position.y <= ARENA_HEIGHT - PLAYER_RADIUS
        && !obstacle_at(position, PLAYER_RADIUS)
}

/// 弾の中心がアリーナ内に残っているかを判定する。
fn bullet_in_bounds(position: Vec2) -> bool {
    position.x >= 0.0
        && position.x <= ARENA_WIDTH
        && position.y >= 0.0
        && position.y <= ARENA_HEIGHT
}

/// 点が中央の長方形障害物内にあるかを判定する。
///
/// プレイヤーでは `margin = PLAYER_RADIUS` として長方形を外側へ広げ、
/// 円の中心が壁へめり込まないようにする。弾ではmarginを0にする。
fn obstacle_at(position: Vec2, margin: f32) -> bool {
    let obstacles = [
        (Vec2::new(250.0, 85.0), Vec2::new(140.0, 28.0)),
        (Vec2::new(250.0, 247.0), Vec2::new(140.0, 28.0)),
    ];
    obstacles.iter().any(|(origin, size)| {
        position.x >= origin.x - margin
            && position.x <= origin.x + size.x + margin
            && position.y >= origin.y - margin
            && position.y <= origin.y + size.y + margin
    })
}

/// 空の名前を補い、長すぎる名前は16文字までに制限する。
fn sanitize_name(name: &str, id: u64) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        format!("Player {id}")
    } else {
        trimmed.chars().take(16).collect()
    }
}

/// 再接続時にPlayer Entityを安全に特定するためのランダムトークンを作る。
fn generate_reconnect_token() -> String {
    format!("{:032x}", rand::random::<u128>())
}

/// Bevyで使うVec2を、protocol crateの通信用Vec2へ変換する。
fn net_vec(value: Vec2) -> NetVec2 {
    NetVec2 {
        x: value.x,
        y: value.y,
    }
}

/// 環境変数をu64として読み、未設定・不正値ならdefaultを返す。
fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// 連番から決定的に欠落を判定する。
///
/// 乱数を使わないため、同じ設定なら試験結果を再現しやすい。
fn should_drop_packet(sequence: u64, loss_percent: u32) -> bool {
    loss_percent > 0 && sequence.wrapping_mul(37) % 100 < u64::from(loss_percent)
}

/// 指定した接続IDのクライアント1台だけへJSONを送る。
fn send_to(network: &Network, id: u64, message: &ServerMessage) {
    let Ok(text) = serde_json::to_string(message) else {
        return;
    };
    if let Some(sender) = network.clients.lock().expect("clients lock").get(&id) {
        let _ = sender.send(OutboundMessage {
            message: Message::Text(text.into()),
            // welcome/rejectedは試験対象にせず、即時に送る。
            delay: Duration::ZERO,
        });
    }
}

/// 接続中の全クライアントへ同じJSONメッセージを送る。
fn broadcast(network: &mut Network, message: &ServerMessage) {
    let Ok(text) = serde_json::to_string(message) else {
        return;
    };
    let message = Message::Text(text.into());
    for sender in network.clients.lock().expect("clients lock").values() {
        network.outbound_sequence += 1;
        if should_drop_packet(network.outbound_sequence, network.simulated_loss_percent) {
            continue;
        }
        let _ = sender.send(OutboundMessage {
            message: message.clone(),
            delay: network.simulated_latency,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_percent_never_drops() {
        assert!(!(0..500).any(|sequence| should_drop_packet(sequence, 0)));
    }

    #[test]
    fn simulated_loss_is_close_to_requested_percentage() {
        let dropped = (1..=1000)
            .filter(|sequence| should_drop_packet(*sequence, 25))
            .count();
        assert_eq!(dropped, 250);
    }

    #[test]
    fn collision_keeps_player_outside_obstacle() {
        let mut position = Vec2::new(235.0, 99.0);
        move_with_collision(&mut position, Vec2::new(10.0, 0.0));
        assert_eq!(position, Vec2::new(235.0, 99.0));
    }

    #[test]
    fn respawn_prefers_the_side_away_from_opponent() {
        let position = choose_respawn_position(1, &[(2, Vec2::new(60.0, 180.0))], &[], 100);
        assert!(position.x > ARENA_WIDTH * 0.5);
    }

    #[test]
    fn round_result_moves_to_interval_or_match_result() {
        let mut state = MatchState::default();
        set_round_result(&mut state, 7, false);
        assert_eq!(state.phase, MatchPhase::RoundEnd);
        assert_eq!(state.round_winner_id, Some(7));

        set_round_result(&mut state, 7, true);
        assert_eq!(state.phase, MatchPhase::MatchFinished);
        assert_eq!(state.match_winner_id, Some(7));
    }
}

//! GameServerのWebSocket送受信とPlayer Entityへの反映。

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};
use futures_util::{SinkExt, StreamExt};
use pixel_shooter_admin_protocol::decode_join_ticket;
use pixel_shooter_game_core::{ArenaMap, Bullet, MAX_PLAYERS, MatchState, Player, ScoreItem};
use pixel_shooter_protocol::{
    BulletSnapshot, ClientMessage, ItemSnapshot, MapSnapshot, MatchPhase, PlayerSnapshot,
    RoomSnapshot, ServerMessage, Snapshot, Vec2 as NetVec2,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{
    config::ServerSettings,
    control::{AllocationState, SharedGameSnapshot},
};

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
pub(crate) struct Network {
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
    /// AdminServerが取得する最新の読み取り専用Snapshot。
    game_snapshot: SharedGameSnapshot,
}

/// 通信スレッドで起きた出来事をBevyのメインスレッドへ渡すための値。
enum NetworkEvent {
    Connected(u64),
    Disconnected(u64),
    Message(u64, ClientMessage),
}

struct NetworkThreadSettings {
    bind_address: String,
    simulated_latency: Duration,
    simulated_loss_percent: u32,
}

/// WebSocket用スレッドを開始し、Bevyへ登録するNetwork Resourceを返す。
pub(crate) fn start(settings: &ServerSettings, game_snapshot: SharedGameSnapshot) -> Network {
    let (event_tx, event_rx) = unbounded();
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let simulated_latency = Duration::from_millis(settings.network.simulated_latency_ms);
    start_network_thread(
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

/// TokioとWebSocketを動かす専用OSスレッドを開始する。
///
/// 非同期通信をBevyのSystem内で待つとゲーム更新が止まるため、
/// 通信は別スレッド、ゲーム計算はBevyのメインスレッドと役割を分ける。
fn start_network_thread(
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

/// 通信イベントをゲーム世界へ反映するSystem。
///
/// System引数の意味:
/// - `Commands`: Entityの作成・削除を予約する
/// - `Res<Network>`: 読み取り専用でNetwork Resourceを借りる
/// - `Query<(Entity, &mut Player)>`: Playerを持つ全EntityとPlayerデータを変更可能で取得する
pub(crate) fn process_network(
    mut commands: Commands,
    network: Res<Network>,
    settings: Res<ServerSettings>,
    map: Res<ArenaMap>,
    allocation: Res<AllocationState>,
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
                // ロビーでは席をすぐ空ける。ここで再接続猶予を使うと、
                // 退出後も最大15秒間プレイヤー一覧に残ってしまう。
                if matches!(state.phase, MatchPhase::Waiting | MatchPhase::MatchFinished) {
                    let departed = players
                        .iter()
                        .find(|(_, player)| player.connection_id == Some(connection_id))
                        .map(|(entity, player)| (entity, player.id, player.slot));
                    if let Some((entity, player_id, slot)) = departed {
                        commands.entity(entity).despawn();
                        occupied_slots.retain(|occupied| *occupied != slot);
                        if state.host_player_id == Some(player_id) {
                            state.host_player_id = players
                                .iter()
                                .find(|(_, player)| {
                                    player.id != player_id
                                        && !player.is_cpu
                                        && player.connection_id.is_some()
                                })
                                .map(|(_, player)| player.id);
                        }
                        println!("player {player_id} left the lobby");
                    }
                } else {
                    // 試合中の一時的な通信断だけは、従来どおり同じPlayerへ戻れる。
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
            }
            NetworkEvent::Message(
                connection_id,
                ClientMessage::Join {
                    mut name,
                    reconnect_token,
                    join_ticket,
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

                if settings.control.require_join_ticket {
                    let Some(allocated_room_id) = allocation.room_id.as_deref() else {
                        reject_join(
                            &network,
                            connection_id,
                            "This game server has no allocated room.",
                        );
                        continue;
                    };
                    let Some(ticket) = join_ticket.as_deref() else {
                        reject_join(&network, connection_id, "A join ticket is required.");
                        continue;
                    };
                    let claims = match decode_join_ticket(
                        settings.control.join_secret.as_bytes(),
                        ticket,
                        unix_time(),
                    ) {
                        Ok(claims) => claims,
                        Err(error) => {
                            reject_join(
                                &network,
                                connection_id,
                                &format!("Invalid join ticket: {error}."),
                            );
                            continue;
                        }
                    };
                    if claims.room_id != allocated_room_id {
                        reject_join(
                            &network,
                            connection_id,
                            "The join ticket belongs to another room.",
                        );
                        continue;
                    }
                    name = claims.player_name;
                }

                if occupied_slots.len() >= MAX_PLAYERS {
                    send_to(
                        &network,
                        connection_id,
                        &ServerMessage::Rejected {
                            reason: "The room already has four players. Reconnect with your token."
                                .into(),
                        },
                    );
                    continue;
                }
                if state.phase != MatchPhase::Waiting {
                    send_to(
                        &network,
                        connection_id,
                        &ServerMessage::Rejected {
                            reason: "The match has already started.".into(),
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
                // spawnすると新しいEntityが作られ、Player Componentが付く。
                commands.spawn(new_player(
                    player_id,
                    Some(connection_id),
                    false,
                    token.clone(),
                    slot,
                    sanitize_name(&name, player_id),
                    &settings,
                    &map,
                ));
                if state.host_player_id.is_none() {
                    state.host_player_id = Some(player_id);
                }
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
            NetworkEvent::Message(connection_id, ClientMessage::AddCpu) => {
                if !is_host_connection(connection_id, &state, &players)
                    || state.phase != MatchPhase::Waiting
                    || occupied_slots.len() >= MAX_PLAYERS
                {
                    continue;
                }
                let slot = (0..MAX_PLAYERS)
                    .find(|slot| !occupied_slots.contains(slot))
                    .expect("available CPU slot");
                occupied_slots.push(slot);
                state.next_player_id += 1;
                let player_id = state.next_player_id;
                commands.spawn(new_player(
                    player_id,
                    None,
                    true,
                    String::new(),
                    slot,
                    format!("CPU-{}", slot + 1),
                    &settings,
                    &map,
                ));
                println!("CPU player {player_id} added in slot {slot}");
            }
            NetworkEvent::Message(connection_id, ClientMessage::RemoveCpu { player_id }) => {
                if !is_host_connection(connection_id, &state, &players)
                    || state.phase != MatchPhase::Waiting
                {
                    continue;
                }
                if let Some((entity, slot)) = players
                    .iter()
                    .find(|(_, player)| player.id == player_id && player.is_cpu)
                    .map(|(entity, player)| (entity, player.slot))
                {
                    commands.entity(entity).despawn();
                    occupied_slots.retain(|occupied| *occupied != slot);
                    println!("CPU player {player_id} removed");
                }
            }
            NetworkEvent::Message(
                connection_id,
                ClientMessage::UpdateRoomSettings {
                    settings: room_settings,
                },
            ) => {
                if is_host_connection(connection_id, &state, &players)
                    && state.phase == MatchPhase::Waiting
                {
                    state.room_settings = settings.game.sanitize_room_settings(room_settings);
                }
            }
            NetworkEvent::Message(connection_id, ClientMessage::StartMatch) => {
                if !is_host_connection(connection_id, &state, &players) {
                    println!("start request from client {connection_id} rejected: not the host");
                    continue;
                }
                if state.phase != MatchPhase::Waiting {
                    println!(
                        "start request from client {connection_id} rejected: phase is {:?}",
                        state.phase
                    );
                    continue;
                }
                let mut active_player_count = players
                    .iter()
                    .filter(|(_, player)| player.is_cpu || player.connection_id.is_some())
                    .count();
                // ホスト1人だけでも開始できるよう、対戦相手のCPUを自動で補う。
                if active_player_count == 1 {
                    let slot = (0..MAX_PLAYERS)
                        .find(|slot| !occupied_slots.contains(slot))
                        .expect("automatic CPU slot");
                    occupied_slots.push(slot);
                    state.next_player_id += 1;
                    let player_id = state.next_player_id;
                    commands.spawn(new_player(
                        player_id,
                        None,
                        true,
                        String::new(),
                        slot,
                        format!("CPU-{}", slot + 1),
                        &settings,
                        &map,
                    ));
                    active_player_count += 1;
                    println!("CPU player {player_id} automatically added for match start");
                }
                state.start_requested = true;
                println!(
                    "start request accepted from host; {} active human/CPU player(s)",
                    active_player_count
                );
            }
        }
    }
}

fn is_host_connection(
    connection_id: u64,
    state: &MatchState,
    players: &Query<(Entity, &mut Player)>,
) -> bool {
    players.iter().any(|(_, player)| {
        Some(player.id) == state.host_player_id
            && player.connection_id == Some(connection_id)
            && !player.is_cpu
    })
}

#[allow(clippy::too_many_arguments)]
fn new_player(
    id: u64,
    connection_id: Option<u64>,
    is_cpu: bool,
    reconnect_token: String,
    slot: usize,
    name: String,
    settings: &ServerSettings,
    map: &ArenaMap,
) -> Player {
    Player {
        id,
        connection_id,
        is_cpu,
        reconnect_token,
        reconnect_grace_left: 0.0,
        slot,
        name,
        position: map.spawn_position(slot),
        aim: if slot.is_multiple_of(2) {
            Vec2::X
        } else {
            Vec2::NEG_X
        },
        movement: Vec2::ZERO,
        shooting: false,
        hp: settings.game.gameplay.max_hp,
        score: 0,
        alive: true,
        respawn_left: 0.0,
        shot_cooldown: 0.0,
        ammo: settings.game.gameplay.max_ammo,
        reload_left: 0.0,
        reload_requested: false,
        invulnerable_left: settings.game.gameplay.respawn_invulnerable_seconds,
        dash_cooldown_left: 0.0,
        dash_time_left: 0.0,
        dash_direction: Vec2::ZERO,
        dash_requested: false,
        last_input_sequence: 0,
    }
}

/// 現在のゲーム状態を全Godotクライアントへ送るSystem。
pub(crate) fn broadcast_snapshot(
    state: Res<MatchState>,
    settings: Res<ServerSettings>,
    map: Res<ArenaMap>,
    mut network: ResMut<Network>,
    players: Query<&Player>,
    bullets: Query<&Bullet>,
    items: Query<&ScoreItem>,
) {
    // サーバー更新は60Hzだが、設定したtick間隔で送信頻度を抑える。
    if !state
        .tick
        .is_multiple_of(settings.network.snapshot_every_ticks)
    {
        return;
    }
    let reconnect_grace_left = players
        .iter()
        .filter(|player| !player.is_cpu && player.connection_id.is_none())
        .map(|player| player.reconnect_grace_left)
        .reduce(f32::min)
        .unwrap_or(0.0);

    // Bevy内部のComponentを、通信専用のSnapshot型へ詰め替える。
    // 内部データを直接シリアライズしないことで、通信仕様とゲーム実装を分離できる。
    let snapshot = ServerMessage::Snapshot(Box::new(Snapshot {
        tick: state.tick,
        map: MapSnapshot {
            id: map.id().to_string(),
            revision: map.revision().to_string(),
            width: map.width(),
            height: map.height(),
            tile_size: map.tile_size(),
        },
        phase: state.phase,
        time_left: state.phase_time_left,
        winner_id: state.match_winner_id,
        reconnect_grace_left,
        move_speed: settings.game.gameplay.move_speed,
        dash_speed: settings.game.gameplay.dash_speed,
        dash_duration: settings.game.gameplay.dash_duration,
        dash_cooldown: settings.game.gameplay.dash_cooldown,
        players: players
            .iter()
            .map(|player| PlayerSnapshot {
                id: player.id,
                name: player.name.clone(),
                position: net_vec(player.position),
                aim: net_vec(player.aim),
                hp: player.hp.max(0),
                max_hp: settings.game.gameplay.max_hp,
                score: player.score,
                is_cpu: player.is_cpu,
                connected: player.is_cpu || player.connection_id.is_some(),
                reconnect_grace_left: player.reconnect_grace_left,
                alive: player.alive,
                respawn_left: player.respawn_left,
                invulnerable_left: player.invulnerable_left,
                ammo: player.ammo,
                max_ammo: settings.game.gameplay.max_ammo,
                reloading: player.reload_left > 0.0,
                reload_left: player.reload_left,
                dash_cooldown_left: player.dash_cooldown_left,
                dashing: player.dash_time_left > 0.0,
                dash_time_left: player.dash_time_left,
                last_input_sequence: player.last_input_sequence,
            })
            .collect(),
        bullets: if state.phase == MatchPhase::Running {
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
        items: if state.phase == MatchPhase::Running {
            items
                .iter()
                .map(|item| ItemSnapshot {
                    id: item.id,
                    position: net_vec(item.position),
                    points: state.room_settings.item_points,
                })
                .collect()
        } else {
            Vec::new()
        },
        room: RoomSnapshot {
            host_player_id: state.host_player_id,
            can_start: state.phase == MatchPhase::Waiting
                && state.host_player_id.is_some_and(|host_player_id| {
                    players.iter().any(|player| {
                        !player.is_cpu
                            && player.id == host_player_id
                            && player.connection_id.is_some()
                    })
                }),
            max_players: MAX_PLAYERS,
            settings: state.room_settings,
        },
    }));
    broadcast(&mut network, &snapshot);
}

/// 接続中の全クライアントへ同じJSONメッセージを送る。
fn broadcast(network: &mut Network, message: &ServerMessage) {
    let Ok(text) = serde_json::to_string(message) else {
        return;
    };
    if let Ok(mut snapshot) = network.game_snapshot.write() {
        *snapshot = Some(text.clone());
    }
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

fn reject_join(network: &Network, connection_id: u64, reason: &str) {
    send_to(
        network,
        connection_id,
        &ServerMessage::Rejected {
            reason: reason.into(),
        },
    );
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

/// 再接続時にPlayer Entityを安全に特定するためのランダムトークンを作る。
fn generate_reconnect_token() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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

/// Bevyで使うVec2を、protocol crateの通信用Vec2へ変換する。
fn net_vec(value: Vec2) -> NetVec2 {
    NetVec2 {
        x: value.x,
        y: value.y,
    }
}

/// 連番から決定的に欠落を判定する。
///
/// 乱数を使わないため、同じ設定なら試験結果を再現しやすい。
fn should_drop_packet(sequence: u64, loss_percent: u32) -> bool {
    loss_percent > 0 && sequence.wrapping_mul(37) % 100 < u64::from(loss_percent)
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
}

//! WebSocketの送受信と、通信イベントからPlayer Entityへの反映。

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};
use futures_util::{SinkExt, StreamExt};
use pixel_shooter_protocol::{
    BulletSnapshot, ClientMessage, MatchPhase, PlayerSnapshot, ServerMessage, Snapshot,
    Vec2 as NetVec2,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{
    arena::spawn_position,
    config::ServerSettings,
    model::{Bullet, MAX_PLAYERS, MatchState, Player},
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
}

/// 通信スレッドで起きた出来事をBevyのメインスレッドへ渡すための値。
enum NetworkEvent {
    Connected(u64),
    Disconnected(u64),
    Message(u64, ClientMessage),
}

/// WebSocket用スレッドを開始し、Bevyへ登録するNetwork Resourceを返す。
pub(crate) fn start(settings: &ServerSettings) -> Network {
    let (event_tx, event_rx) = unbounded();
    let clients = Arc::new(Mutex::new(HashMap::new()));
    let simulated_latency = Duration::from_millis(settings.network.simulated_latency_ms);
    start_network_thread(
        event_tx,
        clients.clone(),
        settings.network.bind_address.clone(),
        simulated_latency,
        settings.network.simulated_loss_percent,
    );
    Network {
        events: event_rx,
        clients,
        simulated_latency,
        simulated_loss_percent: settings.network.simulated_loss_percent,
        outbound_sequence: 0,
    }
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
pub(crate) fn process_network(
    mut commands: Commands,
    network: Res<Network>,
    settings: Res<ServerSettings>,
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
                    hp: settings.gameplay.max_hp,
                    score: 0,
                    round_wins: 0,
                    alive: true,
                    respawn_left: 0.0,
                    shot_cooldown: 0.0,
                    ammo: settings.gameplay.max_ammo,
                    reload_left: 0.0,
                    reload_requested: false,
                    invulnerable_left: settings.gameplay.respawn_invulnerable_seconds,
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

/// 現在のゲーム状態を全Godotクライアントへ送るSystem。
pub(crate) fn broadcast_snapshot(
    state: Res<MatchState>,
    settings: Res<ServerSettings>,
    mut network: ResMut<Network>,
    players: Query<&Player>,
    bullets: Query<&Bullet>,
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
        rounds_to_win: settings.match_rules.rounds_to_win,
        round_winner_id: state.round_winner_id,
        winner_id: state.match_winner_id,
        reconnect_grace_left,
        move_speed: settings.gameplay.move_speed,
        dash_speed: settings.gameplay.dash_speed,
        dash_duration: settings.gameplay.dash_duration,
        dash_cooldown: settings.gameplay.dash_cooldown,
        players: players
            .iter()
            .map(|player| PlayerSnapshot {
                id: player.id,
                name: player.name.clone(),
                position: net_vec(player.position),
                aim: net_vec(player.aim),
                hp: player.hp.max(0),
                max_hp: settings.gameplay.max_hp,
                score: player.score,
                round_wins: player.round_wins,
                connected: player.connection_id.is_some(),
                reconnect_grace_left: player.reconnect_grace_left,
                alive: player.alive,
                respawn_left: player.respawn_left,
                invulnerable_left: player.invulnerable_left,
                ammo: player.ammo,
                max_ammo: settings.gameplay.max_ammo,
                reloading: player.reload_left > 0.0,
                reload_left: player.reload_left,
                dash_cooldown_left: player.dash_cooldown_left,
                dashing: player.dash_time_left > 0.0,
                dash_time_left: player.dash_time_left,
                last_input_sequence: player.last_input_sequence,
            })
            .collect(),
        bullets: if matches!(state.phase, MatchPhase::Running | MatchPhase::Overtime) {
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

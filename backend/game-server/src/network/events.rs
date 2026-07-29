//! WebSocketイベントとClientMessageをBevyのゲーム世界へ反映する。

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use pixel_shooter_admin_protocol::decode_join_ticket;
use pixel_shooter_game_core::{ArenaMap, MAX_PLAYERS, MatchState, Player};
use pixel_shooter_protocol::{ClientMessage, MatchPhase, ServerMessage};

use crate::{config::ServerSettings, control::AllocationState};

use super::{
    Network, NetworkEvent,
    snapshot::{reject_join, send_map_definition, send_to},
};

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
                    send_map_definition(&network, connection_id, &map);
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
                send_map_definition(&network, connection_id, &map);
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

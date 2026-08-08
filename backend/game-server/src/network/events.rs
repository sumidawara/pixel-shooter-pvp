//! WebSocketイベントとClientMessageをBevyのゲーム世界へ反映する。

use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use pixel_shooter_admin_protocol::decode_join_ticket;
use pixel_shooter_game_core::{
    ArenaMap, MAX_PLAYERS, MatchState, Player, RANDOM_MAP_ID, apply_network_player_input,
};
use pixel_shooter_protocol::{ClientMessage, MatchPhase, ServerMessage};

use crate::{config::ServerSettings, control::AllocationState, maps::MapCatalog};

use super::{
    Network, NetworkEvent,
    snapshot::{
        reject_join, reject_join_retryable, send_map_catalog, send_map_definition, send_to,
    },
};

/// 通信イベントをゲーム世界へ反映するSystem。
///
/// System引数の意味:
/// - `Commands`: Entityの作成・削除を予約する
/// - `Res<Network>`: 読み取り専用でNetwork Resourceを借りる
/// - `Query<(Entity, &mut Player)>`: Playerを持つ全EntityとPlayerデータを変更可能で取得する
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_network(
    mut commands: Commands,
    network: Res<Network>,
    settings: Res<ServerSettings>,
    mut map: ResMut<ArenaMap>,
    map_catalog: Res<MapCatalog>,
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
                            player.use_item_requested = false;
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
                    send_map_catalog(&network, connection_id, map_catalog.summaries());
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

                // 満室と試合開始済みは「このルームがだめ」なだけなので、
                // クライアントが別のルームを取り直せるよう再試行可能で返す。
                if occupied_slots.len() >= MAX_PLAYERS {
                    reject_join_retryable(
                        &network,
                        connection_id,
                        "The room already has four players. Reconnect with your token.",
                    );
                    continue;
                }
                if state.phase != MatchPhase::Waiting {
                    reject_join_retryable(
                        &network,
                        connection_id,
                        "The match has already started.",
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
                    PlayerKind::Human,
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
                send_map_catalog(&network, connection_id, map_catalog.summaries());
                send_map_definition(&network, connection_id, &map);
                println!("player {player_id} joined in slot {slot}");
            }
            NetworkEvent::Message(connection_id, ClientMessage::Input { sequence, input }) => {
                for (_, mut player) in &mut players {
                    // 古いsequenceの入力を後から適用すると巻き戻るため破棄する。
                    if player.connection_id != Some(connection_id)
                        || sequence <= player.last_input_sequence
                    {
                        continue;
                    }
                    player.last_input_sequence = sequence;
                    apply_network_player_input(&mut player, input);
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
                    PlayerKind::Cpu,
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
            NetworkEvent::Message(connection_id, ClientMessage::Leave) => {
                for (_, mut player) in &mut players {
                    if player.connection_id != Some(connection_id) {
                        continue;
                    }
                    // 明示的な退出には再接続猶予を与えない。次のGameTickで
                    // 最後の人間ならCPUを含むルーム全体が空へ戻る。
                    player.connection_id = None;
                    player.reconnect_grace_left = 0.0;
                    player.movement = Vec2::ZERO;
                    player.shooting = false;
                    player.reload_requested = false;
                    player.dash_requested = false;
                    player.use_item_requested = false;
                    println!("player {} intentionally left", player.id);
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
                    let requested_map_id = room_settings.map_id.clone();
                    let mut next_settings = settings.game.sanitize_room_settings(room_settings);
                    let selected_map = if requested_map_id == RANDOM_MAP_ID {
                        // 既に生成済みなら作り直さない。ここは得点や試合時間を
                        // 変えるたびにも届くため、毎回作り直すと数値をいじった
                        // だけで全員の画面のマップが差し替わる。
                        if map.id() == RANDOM_MAP_ID {
                            map.clone()
                        } else {
                            ArenaMap::generate(rand::random())
                        }
                    } else {
                        map_catalog
                            .get(&requested_map_id)
                            .cloned()
                            .unwrap_or_else(|| map.clone())
                    };
                    next_settings.map_id = selected_map.id().into();

                    // 自動生成はIDが同じままなので、版まで見ないと差し替えを取りこぼす。
                    if selected_map.id() != map.id() || selected_map.revision() != map.revision() {
                        *map = selected_map;
                        for (_, mut player) in &mut players {
                            player.position = map.spawn_position(player.slot);
                            player.movement = Vec2::ZERO;
                            player.shooting = false;
                            player.dash_requested = false;
                            player.use_item_requested = false;
                            if let Some(player_connection_id) = player.connection_id {
                                send_map_definition(&network, player_connection_id, &map);
                            }
                        }
                        println!("host selected map {} ({})", map.name(), map.id());
                    }
                    state.room_settings = next_settings;
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
                // 練習場では、空いているスロットをすべて的で埋める。
                // 撃つ相手が要るのは対戦と同じだが、ここで要るのは撃ち返さない相手。
                //
                // 的をプレイヤーとして置いているのは、Ghost（他人の持ち物を奪う）と
                // ラロキンポッポス（得点1位を狙う）が、プレイヤーしか対象にできないため。
                // 別種のEntityにすると、この2つだけ練習場で試せなくなる。
                if state.room_settings.sandbox {
                    while occupied_slots.len() < MAX_PLAYERS {
                        let slot = (0..MAX_PLAYERS)
                            .find(|slot| !occupied_slots.contains(slot))
                            .expect("free sandbox slot");
                        occupied_slots.push(slot);
                        state.next_player_id += 1;
                        let player_id = state.next_player_id;
                        commands.spawn(new_player(
                            player_id,
                            None,
                            PlayerKind::Dummy,
                            String::new(),
                            slot,
                            format!("DUMMY-{}", slot + 1),
                            &settings,
                            &map,
                        ));
                        active_player_count += 1;
                        println!("sandbox dummy {player_id} added in slot {slot}");
                    }
                } else if active_player_count == 1 {
                    // ホスト1人だけでも開始できるよう、対戦相手のCPUを自動で補う。
                    let slot = (0..MAX_PLAYERS)
                        .find(|slot| !occupied_slots.contains(slot))
                        .expect("automatic CPU slot");
                    occupied_slots.push(slot);
                    state.next_player_id += 1;
                    let player_id = state.next_player_id;
                    commands.spawn(new_player(
                        player_id,
                        None,
                        PlayerKind::Cpu,
                        String::new(),
                        slot,
                        format!("CPU-{}", slot + 1),
                        &settings,
                        &map,
                    ));
                    active_player_count += 1;
                    println!("CPU player {player_id} automatically added for match start");
                }
                // 「毎回作る」を選んでいる場合は、試合ごとに地形を作り直す。
                // 同じ地形を繰り返すなら手書きのマップを選べばよく、
                // 自動生成を選ぶ理由は毎回違う場所で遊べることにある。
                if state.room_settings.map_id == RANDOM_MAP_ID {
                    *map = ArenaMap::generate(rand::random());
                    for (_, mut player) in &mut players {
                        // 差し替え前の位置が新しい地形の壁の中ということがある。
                        player.position = map.spawn_position(player.slot);
                        player.movement = Vec2::ZERO;
                        player.shooting = false;
                        if let Some(player_connection_id) = player.connection_id {
                            send_map_definition(&network, player_connection_id, &map);
                        }
                    }
                    println!("generated a random arena (revision {})", map.revision());
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

/// 生成するプレイヤーの種別。
///
/// `is_cpu` と `is_dummy` の bool を2つ並べて渡すと、呼び出し側で
/// `true, false` と `true, true` を取り違えても型では気付けない。
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayerKind {
    Human,
    Cpu,
    /// 動かず撃ち返さない的。練習場でのみ置く。
    Dummy,
}

#[allow(clippy::too_many_arguments)]
fn new_player(
    id: u64,
    connection_id: Option<u64>,
    kind: PlayerKind,
    reconnect_token: String,
    slot: usize,
    name: String,
    settings: &ServerSettings,
    map: &ArenaMap,
) -> Player {
    Player {
        id,
        connection_id,
        is_cpu: kind != PlayerKind::Human,
        is_dummy: kind == PlayerKind::Dummy,
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
        use_item_requested: false,
        held_item: None,
        berserk_left: 0.0,
        shield_hp: 0,
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

//! ゲーム状態のSnapshot化とクライアントへの送信。

use std::time::Duration;

use bevy::prelude::*;
use pixel_shooter_game_core::{
    ArenaMap, Bullet, LarokinPoppos, MAX_PLAYERS, MatchState, Player, ScoreItem,
};
use pixel_shooter_protocol::{
    BulletSnapshot, HeldItemSnapshot, ItemSnapshot, LarokinPopposSnapshot, MapSummary, MatchPhase,
    PlayerSnapshot, RoomSnapshot, ServerMessage, Snapshot, Vec2 as NetVec2,
};
use tokio_tungstenite::tungstenite::Message;

use crate::config::ServerSettings;

use super::{Network, OutboundMessage};

/// 現在のゲーム状態を全Godotクライアントへ送るSystem。
pub(crate) fn broadcast_snapshot(
    state: Res<MatchState>,
    settings: Res<ServerSettings>,
    mut network: ResMut<Network>,
    players: Query<&Player>,
    bullets: Query<&Bullet>,
    items: Query<&ScoreItem>,
    larokin_poppos: Query<&LarokinPoppos>,
) {
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
                held_item: player.held_item.map(|item| HeldItemSnapshot {
                    kind: item.kind,
                    charges: item.charges,
                }),
                berserk_left: player.berserk_left,
                shield_hp: player.shield_hp,
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
                    damage: bullet.damage,
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
                    kind: item.kind,
                })
                .collect()
        } else {
            Vec::new()
        },
        larokin_poppos: if state.phase == MatchPhase::Running {
            larokin_poppos
                .iter()
                .map(|attacker| LarokinPopposSnapshot {
                    id: attacker.id,
                    owner_id: attacker.owner_id,
                    position: net_vec(attacker.position),
                    velocity: net_vec(attacker.velocity),
                    telegraph_left: attacker.telegraph_left,
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
            settings: state.room_settings.clone(),
        },
    }));
    let Ok(text) = serde_json::to_string(&snapshot) else {
        return;
    };
    if let Ok(mut debug_snapshot) = network.game_snapshot.write() {
        *debug_snapshot = Some(text.clone());
    }
    // Adminの1tick観察用Snapshotは毎tick保存するが、ゲームクライアントへの
    // 配信頻度は従来どおり設定値で抑える。
    if state
        .tick
        .is_multiple_of(settings.network.snapshot_every_ticks)
    {
        broadcast(&mut network, text);
    }
}

/// 接続中の全クライアントへ同じJSONメッセージを送る。
fn broadcast(network: &mut Network, text: String) {
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

/// 別のルームでも解決しない理由でJoinを拒否する。
pub(super) fn reject_join(network: &Network, connection_id: u64, reason: &str) {
    send_to(
        network,
        connection_id,
        &ServerMessage::Rejected {
            reason: reason.into(),
            retryable: false,
        },
    );
}

/// このルームの都合でJoinを拒否する。クライアントは別のルームを取り直せる。
pub(super) fn reject_join_retryable(network: &Network, connection_id: u64, reason: &str) {
    send_to(
        network,
        connection_id,
        &ServerMessage::Rejected {
            reason: reason.into(),
            retryable: true,
        },
    );
}

/// 指定した接続IDのクライアント1台だけへJSONを送る。
pub(super) fn send_to(network: &Network, id: u64, message: &ServerMessage) {
    let Ok(text) = serde_json::to_string(message) else {
        return;
    };
    if let Some(sender) = network.clients.lock().expect("clients lock").get(&id) {
        let _ = sender.send(OutboundMessage {
            message: Message::Text(text.into()),
            // 接続時メッセージは試験対象にせず、即時に送る。
            delay: Duration::ZERO,
        });
    }
}

pub(super) fn send_map_definition(network: &Network, connection_id: u64, map: &ArenaMap) {
    send_to(
        network,
        connection_id,
        &ServerMessage::MapDefinition {
            map: map.definition(),
        },
    );
}

pub(super) fn send_map_catalog(network: &Network, connection_id: u64, maps: Vec<MapSummary>) {
    send_to(network, connection_id, &ServerMessage::MapCatalog { maps });
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
pub(super) fn should_drop_packet(sequence: u64, loss_percent: u32) -> bool {
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

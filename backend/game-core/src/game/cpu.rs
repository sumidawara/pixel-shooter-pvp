//! CPUプレイヤーの入力生成。

use bevy::prelude::*;

use crate::model::{MatchState, Player, ScoreItem};

use super::is_playing_phase;

/// CPUプレイヤーの入力をサーバー内で作る簡易AI。
pub(crate) fn update_cpu_players(
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
    items: Query<&ScoreItem>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }

    let targets: Vec<(u64, Vec2, bool)> = players
        .iter()
        .map(|player| (player.id, player.position, player.alive))
        .collect();
    let item_positions: Vec<Vec2> = items.iter().map(|item| item.position).collect();

    for mut cpu in &mut players {
        if !cpu.is_cpu || !cpu.alive {
            continue;
        }
        let Some((_, enemy_position, _)) = targets
            .iter()
            .filter(|(id, _, alive)| *id != cpu.id && *alive)
            .min_by(|left, right| {
                cpu.position
                    .distance_squared(left.1)
                    .total_cmp(&cpu.position.distance_squared(right.1))
            })
        else {
            continue;
        };
        let movement_target = item_positions
            .iter()
            .min_by(|left, right| {
                cpu.position
                    .distance_squared(**left)
                    .total_cmp(&cpu.position.distance_squared(**right))
            })
            .copied()
            .unwrap_or(*enemy_position);
        cpu.movement = (movement_target - cpu.position).normalize_or_zero();
        cpu.aim = (*enemy_position - cpu.position).normalize_or_zero();
        cpu.shooting = cpu.aim != Vec2::ZERO;
        if state.tick.is_multiple_of(180) {
            cpu.dash_requested = true;
        }
    }
}

//! 死亡プレイヤーの復活と試合開始時の状態初期化。

use bevy::prelude::*;

use crate::{
    arena::ArenaMap,
    model::{Bullet, MatchState, Player},
    schedule::GameClock,
    settings::{GameSettings, GameplaySettings},
};

use super::is_playing_phase;

/// 死亡したプレイヤーの復活カウントを進めるSystem。
pub(crate) fn update_respawns(
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
    bullets: Query<&Bullet>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
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
                map.choose_respawn_position(player.id, &positions, &bullet_positions, state.tick);
            player.hp = settings.gameplay.max_hp;
            player.alive = true;
            player.shot_cooldown = 0.3;
            player.ammo = settings.gameplay.max_ammo;
            player.reload_left = 0.0;
            player.invulnerable_left = settings.gameplay.respawn_invulnerable_seconds;
            player.dash_time_left = 0.0;
        }
    }
}

/// 新しい試合の開始時にプレイヤー状態を初期化する。
pub(super) fn reset_player(player: &mut Player, gameplay: &GameplaySettings, map: &ArenaMap) {
    player.position = map.spawn_position(player.slot);
    player.hp = gameplay.max_hp;
    player.alive = true;
    player.respawn_left = 0.0;
    player.shot_cooldown = 0.0;
    player.ammo = gameplay.max_ammo;
    player.reload_left = 0.0;
    player.reload_requested = false;
    player.invulnerable_left = gameplay.respawn_invulnerable_seconds;
    player.dash_cooldown_left = 0.0;
    player.dash_time_left = 0.0;
    player.dash_requested = false;
}

//! プレイヤーの移動、リロード、ダッシュ。

use bevy::prelude::*;

use crate::{
    arena::ArenaMap,
    model::{MatchState, Player},
    schedule::GameClock,
    settings::GameSettings,
};

use super::is_playing_phase;

/// クライアントから受け取った移動入力でプレイヤーを動かすSystem。
pub(crate) fn move_players(
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    state: Res<MatchState>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
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
        if player.reload_requested
            && player.reload_left <= 0.0
            && player.ammo < settings.gameplay.max_ammo
        {
            player.reload_left = settings.gameplay.reload_seconds;
        }
        player.reload_requested = false;

        if player.reload_left > 0.0 {
            player.reload_left = (player.reload_left - dt).max(0.0);
            if player.reload_left <= 0.0 {
                player.ammo = settings.gameplay.max_ammo;
            }
        }

        // Spaceが押された瞬間に、現在の移動入力方向へダッシュを開始する。
        if player.dash_requested
            && player.dash_cooldown_left <= 0.0
            && player.movement.length_squared() > 0.001
        {
            player.dash_direction = player.movement.normalize();
            player.dash_time_left = settings.gameplay.dash_duration;
            player.dash_cooldown_left = settings.gameplay.dash_cooldown;
        }
        player.dash_requested = false;

        // ダッシュ中は通常入力ではなく、開始時に保存した方向へ高速移動する。
        let (direction, speed) = if player.dash_time_left > 0.0 {
            player.dash_time_left = (player.dash_time_left - dt).max(0.0);
            (player.dash_direction, settings.gameplay.dash_speed)
        } else {
            (player.movement, settings.gameplay.move_speed)
        };

        // 速度(px/秒) × 経過秒で、このtickに進む距離を求める。
        let delta = direction * speed * dt;

        // X軸とY軸を別々に判定する。
        // まとめて移動すると、片方の軸が壁に当たっただけで両方向とも止まってしまう。
        map.move_with_collision(&mut player.position, delta);
    }
}

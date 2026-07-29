//! 射撃、弾丸移動、命中、撃破スコア。

use bevy::prelude::*;
use pixel_shooter_protocol::{BULLET_RADIUS, PLAYER_RADIUS};

use crate::{
    arena::ArenaMap,
    model::{Bullet, MatchState, Player},
    schedule::GameClock,
    settings::GameSettings,
};

use super::{
    is_playing_phase,
    score::{add_points, subtract_points},
};

/// 射撃入力とクールダウンを確認し、Bullet Entityを生成するSystem。
pub(crate) fn fire_bullets(
    mut commands: Commands,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
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
            player.reload_left = settings.gameplay.reload_seconds;
            continue;
        }

        player.shot_cooldown = settings.gameplay.shot_interval;
        player.ammo -= 1;
        state.next_bullet_id += 1;
        let aim = player.aim;
        // プレイヤー中心に弾を置くと自分と重なるため、照準方向へ少し前に出す。
        commands.spawn(Bullet {
            id: state.next_bullet_id,
            owner_id: player.id,
            position: player.position + aim * (PLAYER_RADIUS + 6.0),
            velocity: aim * settings.gameplay.bullet_speed,
            life_left: 2.0,
        });

        // 射撃方向と反対へ少し押し戻す。サーバーで計算するので全員に同じ結果になる。
        map.move_with_collision(
            &mut player.position,
            -aim * settings.gameplay.recoil_distance,
        );

        // 最後の1発を撃った直後から自動リロードを開始する。
        if player.ammo == 0 {
            player.reload_left = settings.gameplay.reload_seconds;
        }
    }
}

/// 弾の移動、壁との衝突、プレイヤーへのダメージを処理するSystem。
pub(crate) fn move_and_hit_bullets(
    mut commands: Commands,
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    state: Res<MatchState>,
    mut bullets: Query<(Entity, &mut Bullet)>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
    for (entity, mut bullet) in &mut bullets {
        // Rustの借用規則上、positionを変更しながらvelocityを読む式を分けている。
        let velocity = bullet.velocity;
        bullet.position += velocity * dt;
        bullet.life_left -= dt;
        if bullet.life_left <= 0.0
            || !map.bullet_in_bounds(bullet.position)
            || map.obstacle_at(bullet.position, 0.0)
        {
            // 寿命切れ、画面外、障害物への衝突のどれかなら弾を削除する。
            commands.entity(entity).despawn();
            continue;
        }

        let mut hit = false;
        let mut killed_player_id = None;
        let owner_id = bullet.owner_id;
        for mut player in &mut players {
            if !player.alive || player.id == owner_id || player.invulnerable_left > 0.0 {
                continue;
            }
            // 円同士の当たり判定。sqrtを避けるため距離も半径も二乗して比較する。
            let hit_distance = PLAYER_RADIUS + BULLET_RADIUS;
            if player.position.distance_squared(bullet.position) <= hit_distance * hit_distance {
                player.hp -= 1;
                player.invulnerable_left = settings.gameplay.hit_invulnerable_seconds;
                hit = true;
                if player.hp <= 0 {
                    player.alive = false;
                    player.respawn_left = settings.gameplay.respawn_seconds;
                    player.shooting = false;
                    killed_player_id = Some(player.id);
                }
                break;
            }
        }
        if hit {
            // 1つの弾は1回だけダメージを与える。
            commands.entity(entity).despawn();
            if let Some(victim_id) = killed_player_id {
                // 撃破者へ加点し、死亡したプレイヤーからペナルティを引く。
                // 得点は負数も取り得るためi32で保持し、極端な設定でも飽和演算する。
                for mut player in &mut players {
                    if player.id == owner_id {
                        player.score = add_points(player.score, state.room_settings.kill_points);
                    } else if player.id == victim_id {
                        player.score =
                            subtract_points(player.score, state.room_settings.death_penalty);
                    }
                }
            }
        }
    }
}

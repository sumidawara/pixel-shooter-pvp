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
        let berserk = player.berserk_left > 0.0;
        commands.spawn(Bullet {
            id: state.next_bullet_id,
            owner_id: player.id,
            position: player.position + aim * (PLAYER_RADIUS + 6.0),
            velocity: aim * settings.gameplay.bullet_speed * if berserk { 1.3 } else { 1.0 },
            life_left: if berserk { 2.6 } else { 2.0 },
            damage: if berserk { 2 } else { 1 },
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
                if player.shield_hp > 0 {
                    player.shield_hp = (player.shield_hp - bullet.damage).max(0);
                } else {
                    player.hp -= bullet.damage;
                }
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

#[cfg(test)]
mod tests {
    use bevy::prelude::{App, Entity, Vec2};
    use pixel_shooter_protocol::MatchPhase;

    use crate::{
        arena::ArenaMap,
        game::test_support::{test_app, test_player},
        model::{Bullet, Player},
        schedule::advance_one_tick,
    };

    fn spawn_player(app: &mut App, id: u64, slot: usize) -> Entity {
        let position = app.world().resource::<ArenaMap>().spawn_position(slot);
        let mut player = test_player(id, Some(100 + id));
        player.slot = slot;
        player.position = position;
        app.world_mut().spawn(player).id()
    }

    fn spawn_bullet(app: &mut App, owner_id: u64, position: Vec2, damage: i32) {
        app.world_mut().spawn(Bullet {
            id: 1,
            owner_id,
            position,
            velocity: Vec2::ZERO,
            life_left: 1.0,
            damage,
        });
    }

    fn bullet_count(app: &mut App) -> usize {
        let world = app.world_mut();
        let mut bullets = world.query::<&Bullet>();
        bullets.iter(world).count()
    }

    #[test]
    fn normal_bullet_hit_damages_player_and_despawns() {
        let mut app = test_app(MatchPhase::Running, 60.0);
        spawn_player(&mut app, 1, 0);
        let target_entity = spawn_player(&mut app, 2, 1);
        let target_position = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player")
            .position;
        spawn_bullet(&mut app, 1, target_position, 1);

        advance_one_tick(app.world_mut());

        let target = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player");
        assert_eq!(target.hp, 4);
        assert!(target.alive);
        assert_eq!(bullet_count(&mut app), 0);
    }

    #[test]
    fn lethal_bullet_awards_kill_and_death_scores_once() {
        let mut app = test_app(MatchPhase::Running, 60.0);
        let owner_entity = spawn_player(&mut app, 1, 0);
        let target_entity = spawn_player(&mut app, 2, 1);
        app.world_mut()
            .get_mut::<Player>(target_entity)
            .expect("target player")
            .hp = 1;
        let target_position = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player")
            .position;
        spawn_bullet(&mut app, 1, target_position, 1);

        advance_one_tick(app.world_mut());
        advance_one_tick(app.world_mut());

        let owner = app
            .world()
            .get::<Player>(owner_entity)
            .expect("owner player");
        let target = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player");
        assert_eq!(owner.score, 100);
        assert_eq!(target.score, -25);
        assert!(!target.alive);
        assert_eq!(bullet_count(&mut app), 0);
    }

    #[test]
    fn shield_absorbs_damage_and_invulnerability_blocks_followup_hit() {
        let mut app = test_app(MatchPhase::Running, 60.0);
        spawn_player(&mut app, 1, 0);
        let target_entity = spawn_player(&mut app, 2, 1);
        app.world_mut()
            .get_mut::<Player>(target_entity)
            .expect("target player")
            .shield_hp = 2;
        let target_position = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player")
            .position;
        spawn_bullet(&mut app, 1, target_position, 1);

        advance_one_tick(app.world_mut());

        let target = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player");
        assert_eq!(target.hp, 5);
        assert_eq!(target.shield_hp, 1);
        assert!(target.invulnerable_left > 0.0);

        spawn_bullet(&mut app, 1, target_position, 1);
        advance_one_tick(app.world_mut());

        let target = app
            .world()
            .get::<Player>(target_entity)
            .expect("target player");
        assert_eq!(target.hp, 5);
        assert_eq!(target.shield_hp, 1);
        assert_eq!(bullet_count(&mut app), 1);
    }
}

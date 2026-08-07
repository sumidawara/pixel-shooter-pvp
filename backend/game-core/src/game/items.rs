//! フィールドアイテム、所持スロット、使用効果、ラロキンポッポス。

use bevy::prelude::*;
use pixel_shooter_protocol::{ITEM_RADIUS, ItemKind, PLAYER_RADIUS};

use crate::{
    arena::ArenaMap,
    model::{HeldItem, LarokinPoppos, MatchState, Player, ScoreItem},
    schedule::GameClock,
    settings::GameSettings,
};

use super::{
    is_playing_phase,
    score::{add_points, subtract_points},
};

const BERSERK_SECONDS: f32 = 3.0;
const LAROKIN_COUNT: usize = 10;
const LAROKIN_SPEED: f32 = 230.0;
const LAROKIN_TELEGRAPH_SECONDS: f32 = 0.7;
const LAROKIN_RADIUS: f32 = 8.0;

/// 出現、取得、スロット使用を1tick内で決定的に処理する。
pub(crate) fn update_items(
    mut commands: Commands,
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    map: Res<ArenaMap>,
    mut state: ResMut<MatchState>,
    mut players: Query<&mut Player>,
    items: Query<(Entity, &ScoreItem)>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
    for mut player in &mut players {
        player.berserk_left = (player.berserk_left - dt).max(0.0);
    }

    state.item_spawn_left = (state.item_spawn_left - dt).max(0.0);
    if state.item_spawn_left <= 0.0 {
        if items.iter().len() < state.room_settings.max_items as usize {
            let player_positions: Vec<_> = players
                .iter()
                .filter(|player| player.alive)
                .map(|player| player.position)
                .collect();
            let item_positions: Vec<_> = items.iter().map(|(_, item)| item.position).collect();
            if let Some((id, position)) = choose_score_item_spawn(
                &map,
                state.next_item_id,
                &player_positions,
                &item_positions,
            ) {
                state.next_item_id = id;
                commands.spawn(ScoreItem {
                    id,
                    position,
                    kind: item_kind_for_id(id),
                });
            }
        }
        state.item_spawn_left = state.room_settings.item_spawn_interval;
    }

    let pickup_distance = PLAYER_RADIUS + ITEM_RADIUS;
    for (entity, item) in &items {
        for mut player in &mut players {
            if !player.alive
                || player.position.distance_squared(item.position)
                    > pickup_distance * pickup_distance
            {
                continue;
            }
            let picked_up = if item.kind == ItemKind::EnergyCell {
                player.score = add_points(player.score, state.room_settings.item_points);
                true
            } else if player.held_item.is_none() {
                player.held_item = Some(HeldItem {
                    kind: item.kind,
                    charges: if item.kind == ItemKind::Dash { 5 } else { 1 },
                });
                true
            } else {
                false
            };
            if picked_up {
                commands.entity(entity).despawn();
                break;
            }
        }
    }

    // 対象選択は変更前の状態から行い、Queryの多重mutable borrowを避ける。
    let player_info: Vec<_> = players
        .iter()
        .map(|p| (p.id, p.position, p.score, p.alive, p.held_item.is_some()))
        .collect();
    let mut larokin_uses = Vec::new();
    let mut ghost_uses = Vec::new();
    for mut player in &mut players {
        if !player.use_item_requested || !player.alive {
            player.use_item_requested = false;
            continue;
        }
        player.use_item_requested = false;
        let Some(mut held) = player.held_item else {
            continue;
        };
        match held.kind {
            ItemKind::EnergyCell => {}
            ItemKind::Dash => {
                if player.movement.length_squared() <= 0.001 {
                    continue;
                }
                player.dash_direction = player.movement.normalize();
                player.dash_time_left = settings.gameplay.dash_duration;
                held.charges = held.charges.saturating_sub(1);
                player.held_item = (held.charges > 0).then_some(held);
            }
            ItemKind::Berserk => {
                player.berserk_left = BERSERK_SECONDS;
                player.held_item = None;
            }
            ItemKind::Shield => {
                player.shield_hp = 2;
                player.held_item = None;
            }
            ItemKind::LarokinPoppos => {
                let target = player_info
                    .iter()
                    .filter(|(id, _, _, alive, _)| *id != player.id && *alive)
                    .max_by_key(|(id, _, score, _, _)| (*score, std::cmp::Reverse(*id)))
                    .map(|(id, position, _, _, _)| (*id, *position));
                if let Some(target) = target {
                    larokin_uses.push((player.id, target));
                    player.held_item = None;
                }
            }
            ItemKind::Ghost => {
                let target = player_info
                    .iter()
                    .filter(|(id, _, _, alive, has_item)| *id != player.id && *alive && *has_item)
                    .min_by(|left, right| {
                        player
                            .position
                            .distance_squared(left.1)
                            .total_cmp(&player.position.distance_squared(right.1))
                    })
                    .map(|(id, _, _, _, _)| *id);
                if let Some(target_id) = target {
                    ghost_uses.push((player.id, target_id));
                    player.held_item = None;
                }
            }
        }
    }

    for (user_id, target_id) in ghost_uses {
        let stolen = players
            .iter_mut()
            .find(|player| player.id == target_id)
            .and_then(|mut target| target.held_item.take());
        if let Some(stolen) = stolen {
            if let Some(mut user) = players.iter_mut().find(|player| player.id == user_id) {
                user.held_item = Some(stolen);
            }
        }
    }
    for (owner_id, (_, target_position)) in larokin_uses {
        spawn_larokin_wave(&mut commands, &map, &mut state, owner_id, target_position);
    }
}

pub(crate) fn update_larokin_poppos(
    mut commands: Commands,
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    state: Res<MatchState>,
    mut attackers: Query<(Entity, &mut LarokinPoppos)>,
    mut players: Query<&mut Player>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }
    let dt = clock.delta_seconds();
    for (entity, mut attacker) in &mut attackers {
        attacker.life_left -= dt;
        if attacker.telegraph_left > 0.0 {
            attacker.telegraph_left = (attacker.telegraph_left - dt).max(0.0);
            continue;
        }
        let velocity = attacker.velocity;
        attacker.position += velocity * dt;
        let mut victim_id = None;
        for mut player in &mut players {
            if !player.alive || player.id == attacker.owner_id || player.invulnerable_left > 0.0 {
                continue;
            }
            let distance = PLAYER_RADIUS + LAROKIN_RADIUS;
            if player.position.distance_squared(attacker.position) <= distance * distance {
                if player.shield_hp > 0 {
                    player.shield_hp -= 1;
                } else {
                    player.hp -= 1;
                    player.invulnerable_left = settings.gameplay.hit_invulnerable_seconds;
                }
                if player.hp <= 0 {
                    player.alive = false;
                    player.respawn_left = settings.gameplay.respawn_seconds;
                    player.shooting = false;
                    victim_id = Some(player.id);
                }
                commands.entity(entity).despawn();
                break;
            }
        }
        if let Some(victim_id) = victim_id {
            award_kill(&mut players, attacker.owner_id, victim_id, &state);
        } else if attacker.life_left <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn award_kill(players: &mut Query<&mut Player>, owner_id: u64, victim_id: u64, state: &MatchState) {
    for mut player in players.iter_mut() {
        if player.id == owner_id {
            player.score = add_points(player.score, state.room_settings.kill_points);
        } else if player.id == victim_id {
            player.score = subtract_points(player.score, state.room_settings.death_penalty);
        }
    }
}

fn spawn_larokin_wave(
    commands: &mut Commands,
    map: &ArenaMap,
    state: &mut MatchState,
    owner_id: u64,
    target: Vec2,
) {
    let margin = map.tile_size() + LAROKIN_RADIUS;
    for index in 0..LAROKIN_COUNT {
        let lane = (index / 4) as f32 - 1.0;
        let position = match index % 4 {
            0 => Vec2::new(
                margin,
                (target.y + lane * 28.0).clamp(margin, map.pixel_height() - margin),
            ),
            1 => Vec2::new(
                map.pixel_width() - margin,
                (target.y + lane * 28.0).clamp(margin, map.pixel_height() - margin),
            ),
            2 => Vec2::new(
                (target.x + lane * 28.0).clamp(margin, map.pixel_width() - margin),
                margin,
            ),
            _ => Vec2::new(
                (target.x + lane * 28.0).clamp(margin, map.pixel_width() - margin),
                map.pixel_height() - margin,
            ),
        };
        state.next_larokin_id += 1;
        commands.spawn(LarokinPoppos {
            id: state.next_larokin_id,
            owner_id,
            position,
            velocity: (target - position).normalize_or_zero() * LAROKIN_SPEED,
            telegraph_left: LAROKIN_TELEGRAPH_SECONDS,
            life_left: 4.0,
        });
    }
}

fn item_kind_for_id(id: u64) -> ItemKind {
    const ROTATION: [ItemKind; 7] = [
        ItemKind::EnergyCell,
        ItemKind::EnergyCell,
        ItemKind::Dash,
        ItemKind::Shield,
        ItemKind::Berserk,
        ItemKind::LarokinPoppos,
        ItemKind::Ghost,
    ];
    ROTATION[id.saturating_sub(1) as usize % ROTATION.len()]
}

pub(super) fn choose_score_item_spawn(
    map: &ArenaMap,
    current_id: u64,
    player_positions: &[Vec2],
    item_positions: &[Vec2],
) -> Option<(u64, Vec2)> {
    (1..=map.item_spawn_count()).find_map(|offset| {
        let id = current_id.saturating_add(offset as u64);
        let position = map.item_spawn_position(id.saturating_sub(1) as usize);
        let away_from_players = player_positions
            .iter()
            .all(|other| other.distance_squared(position) > 48.0 * 48.0);
        let away_from_items = item_positions
            .iter()
            .all(|other| other.distance_squared(position) > ITEM_RADIUS * ITEM_RADIUS * 4.0);
        (away_from_players && away_from_items).then_some((id, position))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixel_shooter_protocol::MatchPhase;

    use crate::{
        game::test_support::{test_app, test_player},
        model::{HeldItem, LarokinPoppos, Player},
        schedule::advance_one_tick,
    };

    #[test]
    fn item_rotation_keeps_energy_cells_common() {
        assert_eq!(item_kind_for_id(1), ItemKind::EnergyCell);
        assert_eq!(item_kind_for_id(2), ItemKind::EnergyCell);
        assert_eq!(item_kind_for_id(3), ItemKind::Dash);
        assert_eq!(item_kind_for_id(7), ItemKind::Ghost);
    }

    #[test]
    fn ghost_steals_from_nearest_living_item_holder() {
        let mut app = test_app(MatchPhase::Running, 60.0);

        let mut user = test_player(1, Some(101));
        user.position = Vec2::new(100.0, 100.0);
        user.use_item_requested = true;
        user.held_item = Some(HeldItem {
            kind: ItemKind::Ghost,
            charges: 1,
        });
        let user_entity = app.world_mut().spawn(user).id();

        let mut near_target = test_player(2, Some(102));
        near_target.position = Vec2::new(130.0, 100.0);
        near_target.held_item = Some(HeldItem {
            kind: ItemKind::Shield,
            charges: 1,
        });
        let near_entity = app.world_mut().spawn(near_target).id();

        let mut far_target = test_player(3, Some(103));
        far_target.position = Vec2::new(300.0, 100.0);
        far_target.held_item = Some(HeldItem {
            kind: ItemKind::Berserk,
            charges: 1,
        });
        let far_entity = app.world_mut().spawn(far_target).id();

        advance_one_tick(app.world_mut());

        let user = app.world().get::<Player>(user_entity).expect("item user");
        let near_target = app
            .world()
            .get::<Player>(near_entity)
            .expect("near target");
        let far_target = app.world().get::<Player>(far_entity).expect("far target");
        assert_eq!(
            user.held_item,
            Some(HeldItem {
                kind: ItemKind::Shield,
                charges: 1,
            })
        );
        assert_eq!(near_target.held_item, None);
        assert_eq!(
            far_target.held_item,
            Some(HeldItem {
                kind: ItemKind::Berserk,
                charges: 1,
            })
        );
    }

    #[test]
    fn larokin_targets_lowest_id_score_leader_and_spawns_ten_attackers() {
        let mut app = test_app(MatchPhase::Running, 60.0);

        let mut user = test_player(1, Some(101));
        user.position = Vec2::new(320.0, 180.0);
        user.use_item_requested = true;
        user.held_item = Some(HeldItem {
            kind: ItemKind::LarokinPoppos,
            charges: 1,
        });
        let user_entity = app.world_mut().spawn(user).id();

        let mut lower_score = test_player(2, Some(102));
        lower_score.position = Vec2::new(320.0, 80.0);
        lower_score.score = 80;
        app.world_mut().spawn(lower_score);

        let leader_position = Vec2::new(500.0, 180.0);
        let mut lower_id_leader = test_player(3, Some(103));
        lower_id_leader.position = leader_position;
        lower_id_leader.score = 100;
        app.world_mut().spawn(lower_id_leader);

        let mut higher_id_leader = test_player(4, Some(104));
        higher_id_leader.position = Vec2::new(100.0, 180.0);
        higher_id_leader.score = 100;
        app.world_mut().spawn(higher_id_leader);

        advance_one_tick(app.world_mut());

        let attackers: Vec<(u64, Vec2, Vec2)> = {
            let world = app.world_mut();
            let mut query = world.query::<&LarokinPoppos>();
            query
                .iter(world)
                .map(|attacker| (attacker.owner_id, attacker.position, attacker.velocity))
                .collect()
        };
        assert_eq!(attackers.len(), 10);
        for (owner_id, position, velocity) in attackers {
            assert_eq!(owner_id, 1);
            let expected_direction = (leader_position - position).normalize_or_zero();
            assert!(velocity.normalize_or_zero().distance(expected_direction) < 0.0001);
        }
        assert_eq!(
            app.world()
                .get::<Player>(user_entity)
                .expect("item user")
                .held_item,
            None
        );
        assert_eq!(app.world().resource::<MatchState>().next_larokin_id, 10);
    }
}

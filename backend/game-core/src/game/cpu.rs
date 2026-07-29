//! CPUプレイヤーの入力生成。

use std::collections::HashMap;

use bevy::prelude::*;

use crate::{
    arena::{ArenaMap, GridPosition},
    model::{MatchState, Player, ScoreItem},
};

use super::is_playing_phase;

const ROUTE_REFRESH_TICKS: u64 = 15;
const WAYPOINT_REACHED_DISTANCE: f32 = 6.0;

#[derive(Resource, Default)]
pub(crate) struct CpuNavigation {
    routes: HashMap<u64, CpuRoute>,
}

#[derive(Default)]
struct CpuRoute {
    target_cell: Option<GridPosition>,
    planned_tick: u64,
    waypoints: Vec<Vec2>,
    waypoint_index: usize,
}

/// CPUプレイヤーの入力をサーバー内で作り、壁があれば経路に沿って迂回する。
pub(crate) fn update_cpu_players(
    state: Res<MatchState>,
    map: Res<ArenaMap>,
    mut navigation: ResMut<CpuNavigation>,
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
            cpu.movement = Vec2::ZERO;
            cpu.aim = Vec2::ZERO;
            cpu.shooting = false;
            navigation.routes.remove(&cpu.id);
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
        let waypoint = next_movement_waypoint(
            &map,
            &mut navigation,
            cpu.id,
            cpu.position,
            movement_target,
            state.tick,
        );
        cpu.movement = waypoint
            .map(|position| (position - cpu.position).normalize_or_zero())
            .unwrap_or(Vec2::ZERO);
        cpu.aim = (*enemy_position - cpu.position).normalize_or_zero();
        cpu.shooting = cpu.aim != Vec2::ZERO;
        let dash_target = cpu.position + cpu.movement * map.tile_size() * 2.0;
        if state.tick.is_multiple_of(180)
            && cpu.movement != Vec2::ZERO
            && map.has_clear_player_path(cpu.position, dash_target)
        {
            cpu.dash_requested = true;
        }
    }

    navigation.routes.retain(|id, _| {
        targets
            .iter()
            .any(|(target_id, _, alive)| target_id == id && *alive)
    });
}

fn next_movement_waypoint(
    map: &ArenaMap,
    navigation: &mut CpuNavigation,
    cpu_id: u64,
    position: Vec2,
    target: Vec2,
    tick: u64,
) -> Option<Vec2> {
    if map.has_clear_player_path(position, target) {
        navigation.routes.remove(&cpu_id);
        return Some(target);
    }

    let target_cell = map.grid_position(target)?;
    let route = navigation.routes.entry(cpu_id).or_default();
    let route_expired = tick.saturating_sub(route.planned_tick) >= ROUTE_REFRESH_TICKS;
    if route.target_cell != Some(target_cell)
        || route.waypoint_index >= route.waypoints.len()
        || route_expired
    {
        route.target_cell = Some(target_cell);
        route.planned_tick = tick;
        let Some(waypoints) = map.find_player_path(position, target) else {
            route.waypoints.clear();
            route.waypoint_index = 0;
            return None;
        };
        route.waypoints = waypoints;
        route.waypoint_index = 0;
    }

    while route
        .waypoints
        .get(route.waypoint_index)
        .is_some_and(|waypoint| position.distance(*waypoint) <= WAYPOINT_REACHED_DISTANCE)
    {
        route.waypoint_index += 1;
    }

    while route.waypoint_index + 1 < route.waypoints.len()
        && map.has_clear_player_path(position, route.waypoints[route.waypoint_index + 1])
    {
        route.waypoint_index += 1;
    }

    route.waypoints.get(route.waypoint_index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_waypoint_turns_toward_gap_instead_of_wall() {
        let map = ArenaMap::from_json(
            r########"{
                "schema_version": 1,
                "id": "cpu_path_test",
                "revision": "1",
                "name": "CPU Path Test",
                "width": 7,
                "height": 7,
                "tile_size": 32,
                "tiles": [
                    "#######",
                    "#..#..#",
                    "#..#..#",
                    "#.....#",
                    "#..#..#",
                    "#..#..#",
                    "#######"
                ],
                "spawn_points": [[1, 1], [5, 1], [1, 5], [5, 5]],
                "item_spawn_points": [[2, 3]]
            }"########,
        )
        .expect("CPU path test map");
        let start = map.tile_center(GridPosition { x: 1, y: 1 });
        let target = map.tile_center(GridPosition { x: 5, y: 1 });
        let mut navigation = CpuNavigation::default();

        let waypoint =
            next_movement_waypoint(&map, &mut navigation, 1, start, target, 1).expect("waypoint");

        assert!(
            waypoint.y > start.y,
            "CPU should turn down toward the wall opening"
        );
        assert!(map.has_clear_player_path(start, waypoint));

        let mut position = start;
        for tick in 2..=240 {
            let waypoint = next_movement_waypoint(&map, &mut navigation, 1, position, target, tick)
                .expect("waypoint while following route");
            let movement = (waypoint - position).normalize_or_zero();
            map.move_with_collision(&mut position, movement * 2.5);
        }
        assert!(
            position.distance(target) < 8.0,
            "CPU should reach the target after going around the wall; position={position:?}"
        );
    }
}

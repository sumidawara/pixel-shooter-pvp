//! CPUプレイヤーの入力生成。

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use pixel_shooter_protocol::ItemKind;

use crate::{
    arena::{ArenaMap, GridPosition},
    model::{MatchState, Player, ScoreItem},
    schedule::GameClock,
    settings::GameSettings,
};

use super::is_playing_phase;

const ROUTE_REFRESH_TICKS: u64 = 15;
const WAYPOINT_REACHED_DISTANCE: f32 = 6.0;
const RETREAT_START_TILES: f32 = 1.75;
const RETREAT_END_TILES: f32 = 3.0;
const RETREAT_TARGET_TILES: f32 = 2.5;

#[derive(Resource, Default)]
pub(crate) struct CpuNavigation {
    routes: HashMap<u64, CpuRoute>,
    retreating: HashSet<u64>,
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
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
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
    let field_items: Vec<(Vec2, ItemKind)> = items
        .iter()
        .map(|item| (item.position, item.kind))
        .collect();
    // このtickで進む距離。目標を通り過ぎないよう入力を弱めるのに使う。
    let step = settings.gameplay.move_speed * clock.delta_seconds();

    for mut cpu in &mut players {
        // ダミーはCPUではあるが、狙いも移動も持たない。ここで除くのは
        // 経路探索の対象からも外すため（動かないので経路は要らない）。
        if !cpu.is_cpu || cpu.is_dummy || !cpu.alive {
            continue;
        }
        let Some((enemy_id, enemy_position, _)) = targets
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
            navigation.retreating.remove(&cpu.id);
            continue;
        };
        let enemy_distance = cpu.position.distance(*enemy_position);
        let retreating =
            update_retreat_state(&mut navigation, cpu.id, enemy_distance, map.tile_size());
        let movement_target = if retreating {
            choose_retreat_target(&map, cpu.id, cpu.position, *enemy_id, *enemy_position)
                .unwrap_or(cpu.position)
        } else {
            nearest_reachable_item(&field_items, cpu.position, cpu.held_item.is_some())
                .unwrap_or(*enemy_position)
        };
        let waypoint = next_movement_waypoint(
            &map,
            &mut navigation,
            cpu.id,
            cpu.position,
            movement_target,
            state.tick,
        );
        cpu.movement = waypoint
            .map(|position| approach_input(cpu.position, position, step))
            .unwrap_or(Vec2::ZERO);
        cpu.aim = separation_direction(cpu.id, cpu.position, *enemy_id, *enemy_position);
        cpu.shooting = cpu.aim != Vec2::ZERO;
        if cpu.held_item.is_some() {
            let should_use = match cpu.held_item.map(|item| item.kind) {
                Some(pixel_shooter_protocol::ItemKind::Shield) => cpu.hp <= 3,
                Some(pixel_shooter_protocol::ItemKind::Dash) => state.tick.is_multiple_of(120),
                Some(pixel_shooter_protocol::ItemKind::Ghost) => targets.len() > 1,
                Some(_) => state.tick.is_multiple_of(90),
                None => false,
            };
            cpu.use_item_requested |= should_use;
        }
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
    navigation.retreating.retain(|id| {
        targets
            .iter()
            .any(|(target_id, _, alive)| target_id == id && *alive)
    });
}

/// 今の状態で実際に拾えるアイテムのうち、いちばん近いものを選ぶ。
///
/// スロットが埋まっていると、得点アイテム以外は拾えない（`game::items`参照）。
/// 拾えないアイテムを目標にすると、CPUはその上へ着いたまま永久に離れられない。
/// アイテムは取得されないので次のtickでも最寄りのままであり、状況が変わらない。
fn nearest_reachable_item(
    field_items: &[(Vec2, ItemKind)],
    position: Vec2,
    holds_item: bool,
) -> Option<Vec2> {
    field_items
        .iter()
        .filter(|(_, kind)| !holds_item || *kind == ItemKind::EnergyCell)
        .min_by(|left, right| {
            position
                .distance_squared(left.0)
                .total_cmp(&position.distance_squared(right.0))
        })
        .map(|(item_position, _)| *item_position)
}

/// 目標へ向かう移動入力を作る。
///
/// 残り距離が1tickの移動量`step`より短いときは入力を弱め、通り過ぎないようにする。
/// 常に長さ1を返すと、目標へ着いた後も毎tick行き過ぎては戻るため、
/// その場で震え続けているように見える。
fn approach_input(position: Vec2, target: Vec2, step: f32) -> Vec2 {
    let to_target = target - position;
    let distance = to_target.length();
    if distance <= f32::EPSILON {
        return Vec2::ZERO;
    }
    // stepは通常移動の最大距離。バーサクやダッシュで実際の速度が変わっても、
    // ここを基準にしておけば行き過ぎる側にはずれない。
    let scale = if step > 0.0 {
        (distance / step).min(1.0)
    } else {
        1.0
    };
    to_target / distance * scale
}

fn update_retreat_state(
    navigation: &mut CpuNavigation,
    cpu_id: u64,
    enemy_distance: f32,
    tile_size: f32,
) -> bool {
    if enemy_distance < tile_size * RETREAT_START_TILES {
        navigation.retreating.insert(cpu_id);
    } else if enemy_distance >= tile_size * RETREAT_END_TILES {
        navigation.retreating.remove(&cpu_id);
    }
    navigation.retreating.contains(&cpu_id)
}

/// 敵から離れつつ、壁にぶつからず到達できる退避先を選ぶ。
fn choose_retreat_target(
    map: &ArenaMap,
    cpu_id: u64,
    position: Vec2,
    enemy_id: u64,
    enemy_position: Vec2,
) -> Option<Vec2> {
    let away = -separation_direction(cpu_id, position, enemy_id, enemy_position);
    let side = if cpu_id < enemy_id { 1.0 } else { -1.0 };
    let perpendicular = Vec2::new(-away.y, away.x) * side;
    let directions = [
        away,
        (away + perpendicular).normalize_or_zero(),
        (away - perpendicular).normalize_or_zero(),
        perpendicular,
        -perpendicular,
        -away,
    ];
    let distance = map.tile_size() * RETREAT_TARGET_TILES;
    let candidates = directions
        .into_iter()
        .map(|direction| position + direction * distance)
        .filter(|candidate| map.valid_player_position(*candidate));

    candidates
        .clone()
        .filter(|candidate| map.has_clear_player_path(position, *candidate))
        .max_by(|left, right| {
            left.distance_squared(enemy_position)
                .total_cmp(&right.distance_squared(enemy_position))
        })
        .or_else(|| {
            candidates.max_by(|left, right| {
                left.distance_squared(enemy_position)
                    .total_cmp(&right.distance_squared(enemy_position))
            })
        })
}

/// 完全に同じ座標へ重なった場合も、IDを使って互いに逆方向を返す。
fn separation_direction(cpu_id: u64, position: Vec2, enemy_id: u64, enemy_position: Vec2) -> Vec2 {
    let direction = enemy_position - position;
    if direction.length_squared() > 0.001 {
        direction.normalize()
    } else if cpu_id < enemy_id {
        Vec2::X
    } else {
        Vec2::NEG_X
    }
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
    use pixel_shooter_protocol::{ITEM_RADIUS, MatchPhase, PLAYER_RADIUS};

    use super::*;
    use crate::{
        game::test_support::{test_app, test_player},
        model::HeldItem,
        schedule::advance_one_tick,
    };

    /// CPU1体と遠くの的1体を置き、指定したアイテムを1つ落とした試合を作る。
    ///
    /// 的は死なせない。倒してしまうとCPUの目標が消え、検証したい状況から外れる。
    fn cpu_with_item_on_field(
        held: Option<ItemKind>,
        item_kind: ItemKind,
        item_cell: GridPosition,
    ) -> (App, Entity, Vec2) {
        let mut app = test_app(MatchPhase::Running, 60.0);
        let map = app.world().resource::<ArenaMap>().clone();

        let mut cpu = test_player(1, None);
        cpu.is_cpu = true;
        cpu.position = map.tile_center(GridPosition { x: 3, y: 2 });
        cpu.held_item = held.map(|kind| HeldItem { kind, charges: 1 });
        let cpu_entity = app.world_mut().spawn(cpu).id();

        let mut enemy = test_player(2, Some(102));
        enemy.position = map.tile_center(GridPosition { x: 17, y: 8 });
        enemy.hp = i32::MAX;
        app.world_mut().spawn(enemy);

        let item_position = map.tile_center(item_cell);
        app.world_mut().spawn(ScoreItem {
            id: 1,
            position: item_position,
            kind: item_kind,
        });
        (app, cpu_entity, item_position)
    }

    fn cpu_position(app: &App, entity: Entity) -> Vec2 {
        app.world().get::<Player>(entity).expect("cpu").position
    }

    #[test]
    fn approach_input_stops_exactly_on_the_target_instead_of_overshooting() {
        let step = 2.5;
        let target = Vec2::new(100.0, 0.0);

        // 遠いあいだは全速力で向かう。
        let far = approach_input(Vec2::ZERO, target, step);
        assert!((far.length() - 1.0).abs() < 0.0001);

        // 残り距離が1tickの移動量より短ければ、ちょうど届くぶんだけ入力する。
        let close = approach_input(Vec2::new(99.0, 0.0), target, step);
        assert!((close.length() * step - 1.0).abs() < 0.0001);

        // 到達済みなら動かない。
        assert_eq!(approach_input(target, target, step), Vec2::ZERO);
    }

    #[test]
    fn cpu_does_not_hover_on_an_item_it_cannot_pick_up() {
        // スロットが埋まっていると得点アイテム以外は拾えない。
        // 以前はそれでも最寄りのアイテムを目標にし続け、拾えないまま
        // その上で毎tick行き過ぎては戻る（震える）状態から抜けられなかった。
        let (mut app, cpu_entity, item_position) = cpu_with_item_on_field(
            Some(ItemKind::Shield),
            ItemKind::Berserk,
            GridPosition { x: 5, y: 2 },
        );

        for _ in 0..240 {
            advance_one_tick(app.world_mut());
        }

        let position = cpu_position(&app, cpu_entity);
        assert!(
            position.distance(item_position) > PLAYER_RADIUS + ITEM_RADIUS,
            "拾えないアイテムの上に居座っている: position={position:?}"
        );
    }

    #[test]
    fn cpu_still_collects_items_it_can_pick_up_while_holding_one() {
        // 拾えないアイテムを避ける修正で、拾えるアイテムまで無視しては意味がない。
        // 得点アイテムはスロットを使わないので、所持中でも取りに行く。
        let (mut app, _, _) = cpu_with_item_on_field(
            Some(ItemKind::Shield),
            ItemKind::EnergyCell,
            GridPosition { x: 5, y: 2 },
        );

        for _ in 0..240 {
            advance_one_tick(app.world_mut());
        }

        let remaining = {
            let world = app.world_mut();
            let mut items = world.query::<&ScoreItem>();
            items.iter(world).count()
        };
        assert_eq!(remaining, 0, "所持中でも得点アイテムは取りに行く");
    }

    #[test]
    fn following_a_fixed_target_settles_instead_of_trembling() {
        // 震えの正体は、目標へ着いた後も長さ1の入力を出し続けて毎tick行き過ぎること。
        // 移動そのものを繰り返し、収束したあと動かなくなることを確かめる。
        let map = ArenaMap::default();
        let target = map.tile_center(GridPosition { x: 8, y: 2 });
        let step = 2.5;

        // 距離が1tickの移動量で割り切れると、たまたま目標の真上へ着地して
        // 行き過ぎが出ない。割り切れない距離も必ず通す。
        for offset in [0.0_f32, 0.7, 1.3, 2.1] {
            let start = map.tile_center(GridPosition { x: 3, y: 2 }) + Vec2::new(offset, 0.0);
            let mut position = start;
            for _ in 0..200 {
                let input = approach_input(position, target, step);
                map.move_with_collision(&mut position, input * step);
            }
            assert!(
                position.distance(target) < 0.01,
                "目標へ収束していない: offset={offset} position={position:?}"
            );

            // 収束後にどれだけ動き続けるか。震えていれば1tickぶんの距離だけ往復する。
            let settled = position;
            let mut largest_drift: f32 = 0.0;
            for _ in 0..30 {
                let input = approach_input(position, target, step);
                map.move_with_collision(&mut position, input * step);
                largest_drift = largest_drift.max(position.distance(settled));
            }
            assert!(
                largest_drift < 0.01,
                "目標に着いた後も動き続けている（震えている）: \
                 offset={offset} 最大{largest_drift}px"
            );
        }
    }

    #[test]
    fn overlapping_cpus_choose_opposite_retreat_targets() {
        let map = ArenaMap::default();
        let position = map.tile_center(GridPosition { x: 10, y: 3 });

        let first = choose_retreat_target(&map, 1, position, 2, position)
            .expect("first CPU retreat target");
        let second = choose_retreat_target(&map, 2, position, 1, position)
            .expect("second CPU retreat target");

        assert!((first - position).dot(second - position) < 0.0);
        assert!(first.distance(second) >= map.tile_size() * 4.0);
        assert!(map.has_clear_player_path(position, first));
        assert!(map.has_clear_player_path(position, second));
    }

    #[test]
    fn cpu_keeps_retreating_until_safe_distance_is_restored() {
        let mut navigation = CpuNavigation::default();
        let tile_size = 32.0;

        assert!(update_retreat_state(
            &mut navigation,
            1,
            tile_size,
            tile_size
        ));
        assert!(update_retreat_state(
            &mut navigation,
            1,
            tile_size * 2.0,
            tile_size
        ));
        assert!(!update_retreat_state(
            &mut navigation,
            1,
            tile_size * RETREAT_END_TILES,
            tile_size
        ));
    }

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

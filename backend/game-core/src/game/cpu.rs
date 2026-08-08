//! CPUプレイヤーの入力生成。
//!
//! 1tick分を「見る → 覚える → 狙う → 動く」の順で決める。強さの数値は
//! [`CpuSkill`]が持ち、ここにはその数値でどう振る舞うかだけを書く。
//!
//! 弱いCPUを作るのに、移動速度や与ダメージは触らない。それはルールを変える話で、
//! 下手な相手ではなく別のユニットになってしまう。触るのは、目の届く範囲・
//! 反応の速さ・狙いの正確さという、プレイヤー側の能力に対応するものだけ。
//!
//! 乱数は使わない。GameCoreは同じ入力から必ず同じ結果になる必要がある
//! （移動予測のゴールデンベクタ試験がその上に立っている）。狙いのぶれは
//! tickとプレイヤーIDから決まる滑らかな波で作る。

use std::collections::HashMap;

use bevy::prelude::*;
use pixel_shooter_protocol::{BULLET_RADIUS, ItemKind, PLAYER_RADIUS};

use crate::{
    arena::{ArenaMap, GridPosition},
    cpu_skill::CpuSkill,
    model::{MatchState, Player, ScoreItem},
    schedule::GameClock,
    settings::GameSettings,
};

use super::{combat::BERSERK_BULLET_SPEED_MULTIPLIER, is_playing_phase};

const ROUTE_REFRESH_TICKS: u64 = 15;
const WAYPOINT_REACHED_DISTANCE: f32 = 6.0;
/// 見失ってから、最後に見た場所へ向かうのをやめるまで。
///
/// 見えなくなった瞬間に忘れると、遮蔽の裏に入られるたびに探索へ戻り、
/// 追い詰められている感じが出ない。
const MEMORY_TICKS: u64 = 90;
/// 退避をやめる距離は、始める距離よりこれだけ遠い。
///
/// 同じ距離で切り替えると、境目で前進と後退を繰り返して震える。
const RETREAT_RELEASE_TILES: f32 = 1.25;
/// 退避先を探す距離（タイル）。
const RETREAT_TARGET_TILES: f32 = 2.5;
/// 探索先を選び直すまでの上限。壁際で詰まったときに抜けるための保険。
const WANDER_TIMEOUT_TICKS: u64 = 300;
/// 探索先へ着いたとみなす距離。
const WANDER_REACHED_DISTANCE: f32 = 20.0;
/// 回り込む向きを入れ替える間隔。
///
/// ずっと同じ向きだと、ただ円を描くだけの読みやすい動きになる。
const STRAFE_FLIP_TICKS: u64 = 90;
/// 相手へ寄るときに、1回で目指す距離（タイル）。
const ENGAGE_STEP_TILES: f32 = 2.0;

/// CPUごとの、いま何を見て何を覚えているか。
#[derive(Resource, Default)]
pub(crate) struct CpuMinds {
    minds: HashMap<u64, CpuMind>,
}

#[derive(Default)]
struct CpuMind {
    route: CpuRoute,
    retreating: bool,
    /// 追っている相手。見失ってもしばらく覚えている。
    target_id: Option<u64>,
    /// 相手が居ると思っている場所。反応が遅いほど実際から遅れる。
    believed_position: Vec2,
    believed_velocity: Vec2,
    /// 最後に実際に見えた位置。速度を出すために持つ。
    last_seen_position: Vec2,
    /// 連続で見えているtick数。反応時間に届くまで撃たない。
    seen_ticks: u64,
    /// 見失ってからのtick数。
    lost_ticks: u64,
    /// いまの狙いの向き。角速度の上限を超えて動かさない。
    aim: Vec2,
    wander_target: Option<Vec2>,
    wander_index: u64,
    wander_since_tick: u64,
}

#[derive(Default)]
struct CpuRoute {
    target_cell: Option<GridPosition>,
    planned_tick: u64,
    waypoints: Vec<Vec2>,
    waypoint_index: usize,
}

/// 周囲を読むときの、他プレイヤーの見え方。
struct Sighting {
    id: u64,
    position: Vec2,
    alive: bool,
}

/// CPUプレイヤーの入力をサーバー内で作る。
pub(crate) fn update_cpu_players(
    state: Res<MatchState>,
    map: Res<ArenaMap>,
    clock: Res<GameClock>,
    settings: Res<GameSettings>,
    mut minds: ResMut<CpuMinds>,
    mut players: Query<&mut Player>,
    items: Query<&ScoreItem>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }

    let dt = clock.delta_seconds();
    // このtickで進む距離。目標を通り過ぎないよう入力を弱めるのに使う。
    let step = settings.gameplay.move_speed * dt;

    // 周囲は変更前の状態から読む。Queryを二重に可変で借りられないため。
    let others: Vec<Sighting> = players
        .iter()
        .map(|player| Sighting {
            id: player.id,
            position: player.position,
            alive: player.alive,
        })
        .collect();
    let field_items: Vec<(Vec2, ItemKind)> = items
        .iter()
        .map(|item| (item.position, item.kind))
        .collect();

    for mut cpu in &mut players {
        // ダミーはCPUではあるが、狙いも移動も持たない。
        if !cpu.is_cpu || cpu.is_dummy {
            continue;
        }
        if !cpu.alive {
            minds.minds.remove(&cpu.id);
            continue;
        }

        let skill = settings.cpu.skill(cpu.cpu_level);
        let mind = minds.minds.entry(cpu.id).or_default();
        if mind.aim == Vec2::ZERO {
            // 初期の向きから始める。0のままだと最初の1tickだけ制限なく振り向けてしまい、
            // 「振り向きが遅い」という弱さが、出会い頭にだけ効かなくなる。
            mind.aim = cpu.aim.normalize_or_zero();
        }

        // --- 見る・覚える ---
        let seen = nearest_visible_enemy(&others, &map, cpu.id, cpu.position, skill.sight_radius);
        remember(mind, seen, skill, dt);

        // --- 狙う ---
        let aim_point = mind.target_id.map(|_| {
            if skill.leads_target {
                lead_point(
                    cpu.position,
                    mind.believed_position,
                    mind.believed_velocity,
                    bullet_speed(&settings, &cpu),
                )
            } else {
                mind.believed_position
            }
        });

        // --- 動く ---
        let tile = map.tile_size();
        let enemy_distance = aim_point
            .map(|point| cpu.position.distance(point))
            .unwrap_or(f32::INFINITY);
        mind.retreating = update_retreat_state(mind.retreating, skill, enemy_distance, tile);

        let movement_target = if mind.retreating {
            let enemy_id = mind.target_id.unwrap_or(cpu.id);
            choose_retreat_target(&map, cpu.id, cpu.position, enemy_id, mind.believed_position)
                .unwrap_or(cpu.position)
        } else if let Some(item) = visible_item(
            &field_items,
            &map,
            cpu.position,
            cpu.held_item.is_some(),
            skill,
        ) {
            item
        } else if mind.target_id.is_some() {
            engage_target(
                &map,
                cpu.id,
                cpu.position,
                mind.believed_position,
                skill.strafe_bias,
                state.tick,
            )
        } else {
            wander_target(mind, &map, cpu.id, cpu.position, state.tick)
        };

        let waypoint = next_movement_waypoint(
            &map,
            &mut mind.route,
            cpu.position,
            movement_target,
            state.tick,
        );
        cpu.movement = waypoint
            .map(|position| approach_input(cpu.position, position, step))
            .unwrap_or(Vec2::ZERO);

        // 狙う相手が居ないあいだは、進む向きを見ておく。
        // その場で明後日を向いていると、何もしていないのではなく壊れて見える。
        let desired = match aim_point {
            Some(point) => (point - cpu.position).normalize_or_zero(),
            None => cpu.movement.normalize_or_zero(),
        };
        let desired = if desired == Vec2::ZERO {
            mind.aim
        } else {
            rotate(
                desired,
                aim_drift(cpu.id, state.tick, skill.aim_drift_degrees),
            )
        };
        mind.aim = turn_toward(mind.aim, desired, skill.aim_turn_degrees.to_radians() * dt);
        cpu.aim = mind.aim;

        // --- 撃つ ---
        // 反応が終わっていて、自分では狙えたと思える向きに入ったときだけ撃つ。
        // ぶれた向きを基準にするのは、下手な相手ほど「当たると思って外す」ため。
        let reacted = mind.seen_ticks >= skill.reaction_ticks.max(1);
        let allowed = hit_tolerance(enemy_distance) + skill.fire_cone_degrees.to_radians();
        let on_target = mind.aim != Vec2::ZERO
            && desired != Vec2::ZERO
            && angle_between(mind.aim, desired) <= allowed;
        cpu.shooting = aim_point.is_some() && mind.seen_ticks > 0 && reacted && on_target;

        if cpu.held_item.is_some() {
            let should_use = match cpu.held_item.map(|item| item.kind) {
                Some(ItemKind::Shield) => cpu.hp <= 3,
                Some(ItemKind::Dash) => state.tick.is_multiple_of(120),
                Some(ItemKind::Ghost) => others.len() > 1,
                Some(_) => state.tick.is_multiple_of(90),
                None => false,
            };
            cpu.use_item_requested |= should_use;
        }
        let dash_target = cpu.position + cpu.movement * tile * 2.0;
        if state.tick.is_multiple_of(180)
            && cpu.movement != Vec2::ZERO
            && map.has_clear_player_path(cpu.position, dash_target)
        {
            cpu.dash_requested = true;
        }
    }

    minds
        .minds
        .retain(|id, _| others.iter().any(|other| other.id == *id && other.alive));
}

/// 視界の中で、壁越しでない、いちばん近い相手。
///
/// 壁の向こうを見せないのは段階によらない。位置を透視して撃ってくるCPUは、
/// 何をされたのか分からないまま倒されるので、強い弱い以前に理不尽になる。
fn nearest_visible_enemy(
    others: &[Sighting],
    map: &ArenaMap,
    cpu_id: u64,
    position: Vec2,
    sight_radius: f32,
) -> Option<(u64, Vec2)> {
    others
        .iter()
        .filter(|other| other.id != cpu_id && other.alive)
        .filter(|other| position.distance(other.position) <= sight_radius)
        .filter(|other| map.has_line_of_sight(position, other.position))
        .min_by(|left, right| {
            position
                .distance_squared(left.position)
                .total_cmp(&position.distance_squared(right.position))
        })
        .map(|other| (other.id, other.position))
}

/// 見えたものを、反応の遅さぶん遅れて頭の中へ反映する。
fn remember(mind: &mut CpuMind, seen: Option<(u64, Vec2)>, skill: CpuSkill, dt: f32) {
    let Some((id, position)) = seen else {
        mind.seen_ticks = 0;
        mind.lost_ticks += 1;
        if mind.lost_ticks > MEMORY_TICKS {
            mind.target_id = None;
        }
        return;
    };

    let following_same_target = mind.target_id == Some(id) && mind.seen_ticks > 0;
    if following_same_target {
        let follow = reaction_follow(skill.reaction_ticks);
        let observed_velocity = if dt > 0.0 {
            (position - mind.last_seen_position) / dt
        } else {
            Vec2::ZERO
        };
        mind.believed_position = mind.believed_position.lerp(position, follow);
        mind.believed_velocity = mind.believed_velocity.lerp(observed_velocity, follow);
    } else {
        // 見つけた瞬間は、そこに居るとだけ分かる。動きはまだ読めていない。
        mind.believed_position = position;
        mind.believed_velocity = Vec2::ZERO;
        mind.seen_ticks = 0;
    }
    mind.last_seen_position = position;
    mind.target_id = Some(id);
    mind.seen_ticks += 1;
    mind.lost_ticks = 0;
}

/// 反応の遅さを、1tickあたりどれだけ実際へ近づくかへ変える。
///
/// `reaction_ticks`が時定数になる。0なら遅れなし。
fn reaction_follow(reaction_ticks: u64) -> f32 {
    if reaction_ticks == 0 {
        return 1.0;
    }
    1.0 - (-1.0 / reaction_ticks as f32).exp()
}

/// 弾が届くころに相手が居る場所。
///
/// 距離から到達時間を出し、その時間だけ進んだ先を狙う。移動先が遠のくと
/// 時間も伸びるので、数回繰り返して落ち着かせる。
fn lead_point(shooter: Vec2, target: Vec2, velocity: Vec2, bullet_speed: f32) -> Vec2 {
    if bullet_speed <= 0.0 {
        return target;
    }
    let mut travel_time = shooter.distance(target) / bullet_speed;
    for _ in 0..3 {
        travel_time = shooter.distance(target + velocity * travel_time) / bullet_speed;
    }
    target + velocity * travel_time
}

/// 自分の弾が実際に出る速さ。バーサク中は速くなる。
fn bullet_speed(settings: &GameSettings, player: &Player) -> f32 {
    let multiplier = if player.berserk_left > 0.0 {
        BERSERK_BULLET_SPEED_MULTIPLIER
    } else {
        1.0
    };
    settings.gameplay.bullet_speed * multiplier
}

/// 狙いのぶれ(ラジアン)。tickとIDから決まる、ゆっくり漂う波。
///
/// 毎tickの乱数にしないのは、平均すると中心に戻ってしまい、結局当たるため。
/// 数百ms単位で偏っていることが、外れる理由になる。
fn aim_drift(cpu_id: u64, tick: u64, amplitude_degrees: f32) -> f32 {
    if amplitude_degrees <= 0.0 {
        return 0.0;
    }
    // 黄金角。IDが近くても位相が重ならない。
    let phase = cpu_id as f32 * 2.399_963;
    let t = tick as f32;
    let wobble = (t * 0.031 + phase).sin() * 0.65 + (t * 0.011_7 + phase * 1.7).sin() * 0.35;
    wobble * amplitude_degrees.to_radians()
}

fn rotate(direction: Vec2, radians: f32) -> Vec2 {
    Vec2::from_angle(direction.to_angle() + radians)
}

/// `current`を`desired`へ、1tickで`max_step`ラジアンまで近づける。
fn turn_toward(current: Vec2, desired: Vec2, max_step: f32) -> Vec2 {
    if desired == Vec2::ZERO {
        return current;
    }
    if current == Vec2::ZERO {
        return desired;
    }
    let mut delta = desired.to_angle() - current.to_angle();
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    Vec2::from_angle(current.to_angle() + delta.clamp(-max_step, max_step))
}

/// この距離なら、これだけ向きがずれても当たる、という角度。
///
/// 固定の角度で「狙えた」とすると、近距離では厳しすぎ（密着しているのに
/// 撃てない）、遠距離では緩すぎる（当たらないのに撃つ）。当たり判定の幅を
/// 距離で割り、実際に当たる範囲を出す。技量の`fire_cone_degrees`は、
/// そこからさらにどれだけ雑に撃つかを足す。
fn hit_tolerance(distance: f32) -> f32 {
    let hit_window = PLAYER_RADIUS + BULLET_RADIUS;
    (hit_window / distance.max(hit_window)).atan()
}

fn angle_between(left: Vec2, right: Vec2) -> f32 {
    left.normalize_or_zero()
        .dot(right.normalize_or_zero())
        .clamp(-1.0, 1.0)
        .acos()
}

fn update_retreat_state(
    retreating: bool,
    skill: CpuSkill,
    enemy_distance: f32,
    tile_size: f32,
) -> bool {
    if skill.retreat_start_tiles <= 0.0 {
        return false;
    }
    let start = tile_size * skill.retreat_start_tiles;
    let release = tile_size * (skill.retreat_start_tiles + RETREAT_RELEASE_TILES);
    if enemy_distance < start {
        true
    } else if enemy_distance >= release {
        false
    } else {
        retreating
    }
}

/// 視界の中にあって、いま実際に拾えるアイテムのうち最寄り。
///
/// スロットが埋まっていると得点アイテム以外は拾えない（`game::items`参照）。
/// 拾えないものを目標にすると、その上に着いたまま離れられなくなる。
fn visible_item(
    field_items: &[(Vec2, ItemKind)],
    map: &ArenaMap,
    position: Vec2,
    holds_item: bool,
    skill: CpuSkill,
) -> Option<Vec2> {
    if !skill.seeks_items {
        return None;
    }
    field_items
        .iter()
        .filter(|(_, kind)| !holds_item || *kind == ItemKind::EnergyCell)
        .filter(|(item_position, _)| position.distance(*item_position) <= skill.sight_radius)
        .filter(|(item_position, _)| map.has_line_of_sight(position, *item_position))
        .min_by(|left, right| {
            position
                .distance_squared(left.0)
                .total_cmp(&position.distance_squared(right.0))
        })
        .map(|(item_position, _)| *item_position)
}

/// 相手が見えているときの移動先。まっすぐ寄らず、横へ回り込む割合を混ぜる。
///
/// まっすぐ近づくと、相手から見た自分の角速度がほぼ0になり、相手は狙いを
/// 動かさずに当てられる。横へ動くほど相手の旋回速度を要求するので、
/// 撃たれにくくなると同時に、相手の`aim_turn_degrees`の低さを突ける。
///
/// 壁へ突っ込む向きになったら、回り込みを弱めて正面寄りへ倒す。
fn engage_target(
    map: &ArenaMap,
    cpu_id: u64,
    position: Vec2,
    enemy: Vec2,
    strafe_bias: f32,
    tick: u64,
) -> Vec2 {
    let toward = (enemy - position).normalize_or_zero();
    if toward == Vec2::ZERO {
        return enemy;
    }
    let flip = if (tick / STRAFE_FLIP_TICKS)
        .wrapping_add(cpu_id)
        .is_multiple_of(2)
    {
        1.0
    } else {
        -1.0
    };
    let side = Vec2::new(-toward.y, toward.x) * flip;
    let distance = map.tile_size() * ENGAGE_STEP_TILES;
    for bias in [strafe_bias, strafe_bias * 0.5, 0.0] {
        let direction = toward.lerp(side, bias).normalize_or_zero();
        let candidate = position + direction * distance;
        if map.valid_player_position(candidate) && map.has_clear_player_path(position, candidate) {
            return candidate;
        }
    }
    enemy
}

/// 何も見えないときに向かう場所。マップが用意した地点を巡る。
///
/// 完全な乱数で向きを決めると、壁際で足踏みして「探している」ように見えない。
/// アイテムの出る場所は通路がつながっている所なので、そこを巡らせる。
fn wander_target(
    mind: &mut CpuMind,
    map: &ArenaMap,
    cpu_id: u64,
    position: Vec2,
    tick: u64,
) -> Vec2 {
    let expired = mind.wander_target.is_none_or(|target| {
        position.distance(target) <= WANDER_REACHED_DISTANCE
            || tick.saturating_sub(mind.wander_since_tick) >= WANDER_TIMEOUT_TICKS
    });
    if expired {
        mind.wander_index += 1;
        mind.wander_target = Some(wander_spot(map, cpu_id, mind.wander_index));
        mind.wander_since_tick = tick;
    }
    mind.wander_target.unwrap_or(position)
}

fn wander_spot(map: &ArenaMap, cpu_id: u64, index: u64) -> Vec2 {
    let count = map.item_spawn_count().max(1) as u64;
    // 巡る順をCPUごとにずらす。同じ順だと全員が同じ場所へ固まる。
    let mut mixed = cpu_id
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(index.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^= mixed >> 31;
    map.item_spawn_position((mixed % count) as usize)
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
    let scale = if step > 0.0 {
        (distance / step).min(1.0)
    } else {
        1.0
    };
    to_target / distance * scale
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
    route: &mut CpuRoute,
    position: Vec2,
    target: Vec2,
    tick: u64,
) -> Option<Vec2> {
    if map.has_clear_player_path(position, target) {
        route.waypoints.clear();
        route.waypoint_index = 0;
        route.target_cell = None;
        return Some(target);
    }

    let target_cell = map.grid_position(target)?;
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
    use pixel_shooter_protocol::MatchPhase;

    use super::*;
    use crate::{
        cpu_skill::CpuLevel,
        game::test_support::{test_app, test_player},
        schedule::advance_one_tick,
    };

    fn classic() -> ArenaMap {
        ArenaMap::default()
    }

    fn at(map: &ArenaMap, x: usize, y: usize) -> Vec2 {
        map.tile_center(GridPosition { x, y })
    }

    fn sighting(id: u64, position: Vec2) -> Sighting {
        Sighting {
            id,
            position,
            alive: true,
        }
    }

    /// CPU1体と相手1体を置いた試合。相手は倒れない。
    fn duel_app(level: CpuLevel, cpu_at: Vec2, enemy_at: Vec2) -> (App, Entity, Entity) {
        let mut app = test_app(MatchPhase::Running, 60.0);
        let mut cpu = test_player(1, None);
        cpu.is_cpu = true;
        cpu.cpu_level = level;
        cpu.position = cpu_at;
        let cpu_entity = app.world_mut().spawn(cpu).id();

        let mut enemy = test_player(2, Some(102));
        enemy.position = enemy_at;
        enemy.hp = i32::MAX;
        let enemy_entity = app.world_mut().spawn(enemy).id();
        (app, cpu_entity, enemy_entity)
    }

    /// 視界の外にいる相手は、居ないものとして扱うこと。
    ///
    /// マップ全体を見渡して寄ってくると、遮蔽の意味が無くなる。
    #[test]
    fn enemies_beyond_the_sight_radius_are_not_seen() {
        let map = classic();
        let cpu_at = at(&map, 1, 2);
        let near = sighting(2, cpu_at + Vec2::new(80.0, 0.0));
        let far = sighting(3, cpu_at + Vec2::new(240.0, 0.0));
        let others = vec![sighting(1, cpu_at), near, far];

        let radius = CpuSkill::preset(CpuLevel::One).sight_radius;
        assert!(radius > 80.0 && radius < 240.0, "検査の前提が崩れている");

        let seen = nearest_visible_enemy(&others, &map, 1, cpu_at, radius);
        assert_eq!(seen.map(|(id, _)| id), Some(2));

        // 近い方が消えたら、遠い方は視界の外なので誰も見えない。
        let others = vec![
            sighting(1, cpu_at),
            sighting(3, cpu_at + Vec2::new(240.0, 0.0)),
        ];
        assert_eq!(
            nearest_visible_enemy(&others, &map, 1, cpu_at, radius),
            None
        );
    }

    /// 壁の向こうは、どの段階でも見えないこと。
    ///
    /// 位置を透視して撃ってくるCPUは、何をされたのか分からないまま倒されるので、
    /// 強い弱い以前に理不尽になる。
    #[test]
    fn walls_block_sight_at_every_level() {
        let map = classic();
        // 中央の障害物をはさんだ位置と、同じ距離で遮るもののない位置。
        let blocked_from = at(&map, 7, 3);
        let blocked_to = at(&map, 7, 7);
        let open_from = at(&map, 2, 3);
        let open_to = at(&map, 2, 7);
        assert!(
            (blocked_from.distance(blocked_to) - open_from.distance(open_to)).abs() < 0.01,
            "検査の前提が崩れている: 距離が違う"
        );

        for number in 1..=4u8 {
            let radius = CpuSkill::preset(CpuLevel::from_number(number)).sight_radius;
            if radius < blocked_from.distance(blocked_to) {
                continue;
            }
            assert_eq!(
                nearest_visible_enemy(&[sighting(2, blocked_to)], &map, 1, blocked_from, radius),
                None,
                "段階{number}が壁越しに見えている"
            );
            assert!(
                nearest_visible_enemy(&[sighting(2, open_to)], &map, 1, open_from, radius)
                    .is_some(),
                "段階{number}が遮るものがなくても見えていない"
            );
        }
    }

    /// 見えてから撃ち始めるまでに、段階ごとの間があること。
    #[test]
    fn slow_levels_take_longer_to_open_fire() {
        let map = classic();
        // 初期の向き(+X)の先に置く。振り向きの時間ではなく反応時間を見る。
        let cpu_at = at(&map, 2, 2);
        let enemy_at = at(&map, 6, 2);

        let first_shot = |level: CpuLevel| -> Option<u64> {
            let (mut app, cpu_entity, _) = duel_app(level, cpu_at, enemy_at);
            for tick in 1..=120u64 {
                advance_one_tick(app.world_mut());
                if app.world().get::<Player>(cpu_entity).expect("cpu").shooting {
                    return Some(tick);
                }
            }
            None
        };

        let slow = first_shot(CpuLevel::One).expect("段階1が撃たない");
        let fast = first_shot(CpuLevel::Four).expect("段階4が撃たない");
        let reaction = CpuSkill::preset(CpuLevel::One).reaction_ticks;
        assert!(
            slow >= reaction,
            "段階1が反応時間({reaction}tick)より早く撃っている: {slow}"
        );
        assert!(fast < slow, "段階4が段階1より遅い: {fast} / {slow}");
    }

    /// 狙いが1tickで動ける角度に上限があること。
    #[test]
    fn aim_cannot_snap_faster_than_the_turn_rate() {
        let step = 5.0_f32.to_radians();
        let turned = turn_toward(Vec2::X, Vec2::NEG_X, step);
        assert!(
            (angle_between(Vec2::X, turned) - step).abs() < 0.001,
            "上限を超えて振り向いている"
        );
        // 目標が上限より近ければ、ちょうどそこで止まる。
        let close = Vec2::from_angle(2.0_f32.to_radians());
        assert!(angle_between(turn_toward(Vec2::X, close, step), close) < 0.001);
        // 向きが未定のあいだは、そのまま目標を向く。
        assert_eq!(turn_toward(Vec2::ZERO, Vec2::Y, step), Vec2::Y);
    }

    /// 狙いのぶれが、ゆっくり漂って偏り続けること。
    ///
    /// 毎tickの乱数だと平均すると中心に戻り、結局当たってしまう。
    #[test]
    fn aim_drift_wanders_slowly_instead_of_jittering() {
        let amplitude = 10.0;
        let samples: Vec<f32> = (0..60).map(|tick| aim_drift(1, tick, amplitude)).collect();

        for pair in samples.windows(2) {
            let change = (pair[1] - pair[0]).abs().to_degrees();
            assert!(change < 1.0, "1tickで{change}度も飛んでいる");
        }
        // 1発撃つ間隔(0.24秒≒15tick)のあいだは、ほとんど動かないこと。
        // ここで大きく動くと、連射の中で誤差が平均化されて結局当たってしまう。
        for window in samples.windows(16) {
            let change = (window[15] - window[0]).abs();
            assert!(
                change < amplitude.to_radians(),
                "1発の間隔で{}度動いている。連射のあいだに誤差が均される",
                change.to_degrees()
            );
        }
        // 振れ幅は指定の範囲に収まる。
        assert!(
            samples
                .iter()
                .all(|value| value.abs() <= amplitude.to_radians() + 0.001)
        );
        // 設定が0なら完全に正確。
        assert_eq!(aim_drift(1, 37, 0.0), 0.0);
        // CPUごとに位相が違う。全員が同じ方向へ外すと、外れ方が不自然になる。
        assert_ne!(aim_drift(1, 37, amplitude), aim_drift(2, 37, amplitude));
    }

    /// 偏差撃ちが、相手の進む先を狙うこと。
    #[test]
    fn leading_aims_where_the_target_is_going() {
        let shooter = Vec2::ZERO;
        let target = Vec2::new(200.0, 0.0);
        let velocity = Vec2::new(0.0, 150.0);
        let bullet_speed = 340.0;

        let point = lead_point(shooter, target, velocity, bullet_speed);
        assert!(point.y > 0.0, "進む先を狙っていない: {point:?}");

        // 狙った点まで弾が飛ぶ時間と、相手がそこへ着く時間が一致すること。
        let travel = shooter.distance(point) / bullet_speed;
        let arrival = (point.y - target.y) / velocity.y;
        assert!(
            (travel - arrival).abs() < 0.01,
            "弾と相手が同じ時刻に着かない: 弾{travel}s / 相手{arrival}s"
        );

        // 止まっている相手には、そのままの位置。
        assert_eq!(
            lead_point(shooter, target, Vec2::ZERO, bullet_speed),
            target
        );
    }

    /// 誰も見えないとき、その場に立ち尽くさず探しに動くこと。
    #[test]
    fn a_cpu_that_sees_nothing_goes_looking() {
        let map = classic();
        // 相手を視界の外へ置く。段階1は視界が狭い。
        let (mut app, cpu_entity, _) = duel_app(CpuLevel::One, at(&map, 1, 1), at(&map, 18, 9));
        let start = app.world().get::<Player>(cpu_entity).expect("cpu").position;

        for _ in 0..120 {
            advance_one_tick(app.world_mut());
        }

        let moved = app
            .world()
            .get::<Player>(cpu_entity)
            .expect("cpu")
            .position
            .distance(start);
        assert!(moved > map.tile_size(), "探索せずその場に居る: {moved}px");
    }

    /// 探索先がCPUごとに違うこと。全員同じだと固まって動く。
    #[test]
    fn cpus_do_not_all_wander_to_the_same_place() {
        let map = classic();
        let spots: Vec<Vec2> = (1..=4).map(|id| wander_spot(&map, id, 1)).collect();
        assert!(
            spots.iter().any(|spot| *spot != spots[0]),
            "全員が同じ場所へ向かう"
        );
        // 同じ引数なら必ず同じ場所。決定性を保つ。
        assert_eq!(wander_spot(&map, 3, 5), wander_spot(&map, 3, 5));
    }

    /// 退避しない設定なら、近づかれても下がらないこと。
    #[test]
    fn a_level_that_never_retreats_holds_its_ground() {
        let mut skill = CpuSkill::preset(CpuLevel::One);
        assert_eq!(skill.retreat_start_tiles, 0.0, "段階1は退避しない設定");
        assert!(!update_retreat_state(true, skill, 1.0, 32.0));

        // 退避する設定では、始める距離と戻る距離が違う（境目で震えない）。
        skill.retreat_start_tiles = 2.0;
        assert!(update_retreat_state(false, skill, 32.0 * 1.9, 32.0));
        assert!(update_retreat_state(true, skill, 32.0 * 2.5, 32.0));
        assert!(!update_retreat_state(
            true,
            skill,
            32.0 * (2.0 + RETREAT_RELEASE_TILES),
            32.0
        ));
    }

    /// 反応の遅さが、狙う位置の遅れになること。
    #[test]
    fn slower_reaction_lags_further_behind_a_moving_target() {
        assert_eq!(reaction_follow(0), 1.0, "遅れ無しの設定で追従しきらない");
        let quick = reaction_follow(8);
        let slow = reaction_follow(30);
        assert!(slow < quick, "反応が遅い方が速く追いついている");
        assert!(quick < 1.0);
    }

    /// 拾えないアイテムを目標にしないこと。
    ///
    /// スロットが埋まっていると得点アイテム以外は拾えない。目標にすると、
    /// その上に着いたまま離れられなくなる。
    #[test]
    fn items_that_cannot_be_picked_up_are_not_chased() {
        let map = classic();
        let position = at(&map, 2, 2);
        let skill = CpuSkill::preset(CpuLevel::Three);
        let items = vec![(at(&map, 4, 2), ItemKind::Berserk)];

        assert!(visible_item(&items, &map, position, true, skill).is_none());
        // 得点アイテムはスロットを使わないので、所持中でも取りに行く。
        let cells = vec![(at(&map, 4, 2), ItemKind::EnergyCell)];
        assert!(visible_item(&cells, &map, position, true, skill).is_some());
        // 視界の外にあるものは目標にしない。
        let far = vec![(at(&map, 16, 2), ItemKind::EnergyCell)];
        assert!(visible_item(&far, &map, position, false, skill).is_none());
        // 取りに行かない設定では、見えていても寄らない。
        let never = CpuSkill::preset(CpuLevel::One);
        assert!(visible_item(&cells, &map, position, false, never).is_none());
    }

    /// 目標へ着いた後に震えないこと。
    ///
    /// 常に長さ1の入力を出すと、毎tick行き過ぎては戻るため、その場で
    /// 震え続けているように見える。
    #[test]
    fn following_a_fixed_target_settles_instead_of_trembling() {
        let map = classic();
        let target = at(&map, 8, 2);
        let step = 2.5;

        for offset in [0.0_f32, 0.7, 1.3, 2.1] {
            let mut position = at(&map, 3, 2) + Vec2::new(offset, 0.0);
            for _ in 0..200 {
                let input = approach_input(position, target, step);
                map.move_with_collision(&mut position, input * step);
            }
            assert!(position.distance(target) < 0.01, "目標へ収束していない");

            let settled = position;
            let mut largest_drift: f32 = 0.0;
            for _ in 0..30 {
                let input = approach_input(position, target, step);
                map.move_with_collision(&mut position, input * step);
                largest_drift = largest_drift.max(position.distance(settled));
            }
            assert!(
                largest_drift < 0.01,
                "着いた後も動き続けている（震えている）"
            );
        }
    }

    /// 重なったCPUどうしが、互いに逆へ退くこと。
    #[test]
    fn overlapping_cpus_choose_opposite_retreat_targets() {
        let map = classic();
        let position = at(&map, 10, 3);

        let first = choose_retreat_target(&map, 1, position, 2, position).expect("first");
        let second = choose_retreat_target(&map, 2, position, 1, position).expect("second");

        assert!((first - position).dot(second - position) < 0.0);
        assert!(map.has_clear_player_path(position, first));
        assert!(map.has_clear_player_path(position, second));
    }

    /// 壁があるとき、開いている方へ回り込むこと。
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
        let start = at(&map, 1, 1);
        let target = at(&map, 5, 1);
        let mut route = CpuRoute::default();

        let waypoint =
            next_movement_waypoint(&map, &mut route, start, target, 1).expect("waypoint");
        assert!(waypoint.y > start.y, "壁の切れ目へ向かっていない");
        assert!(map.has_clear_player_path(start, waypoint));

        let mut position = start;
        for tick in 2..=240 {
            let waypoint = next_movement_waypoint(&map, &mut route, position, target, tick)
                .expect("waypoint while following route");
            let movement = (waypoint - position).normalize_or_zero();
            map.move_with_collision(&mut position, movement * 2.5);
        }
        assert!(position.distance(target) < 8.0, "回り込んで着いていない");
    }

    /// 段階が上の方が実際に強いこと。
    ///
    /// 数値を並べただけでは「段階」にならない。生成した地形で実際に戦わせ、
    /// 上の段が勝ち越すことを確かめる。1つでも逆転していると、番号を上げたのに
    /// 弱くなる組み合わせがあることになる。
    ///
    /// 勝った試合数ではなく合計得点で見る。1試合は湧き位置の運や一度の連射で
    /// ひっくり返るので、勝敗の数だとぶれて判定にならない。
    #[test]
    fn a_higher_level_actually_beats_a_lower_one() {
        /// 1試合の長さ。出会って撃ち合うだけの余裕を取る。
        const TICKS: usize = 1800;
        const SEEDS: u64 = 8;

        /// 上位が余分に取っていなければならない合計得点。撃破3回ぶん。
        const MARGIN: i32 = 300;

        let duel = |low: CpuLevel, high: CpuLevel, seed: u64| -> (i32, i32) {
            let map = ArenaMap::generate(seed);
            let mut app = test_app(MatchPhase::Running, 60.0);
            app.insert_resource(map.clone());

            let mut weak = test_player(1, None);
            weak.is_cpu = true;
            weak.cpu_level = low;
            weak.position = map.spawn_position(0);
            let weak_entity = app.world_mut().spawn(weak).id();

            let mut strong = test_player(2, None);
            strong.is_cpu = true;
            strong.cpu_level = high;
            strong.position = map.spawn_position(1);
            let strong_entity = app.world_mut().spawn(strong).id();

            for _ in 0..TICKS {
                advance_one_tick(app.world_mut());
            }
            let score = |entity| app.world().get::<Player>(entity).expect("cpu").score;
            (score(weak_entity), score(strong_entity))
        };

        // 総当たりで見る。隣どうしだけだと、1と3のように離れた組で
        // 逆転していても気付けない。
        let levels = [
            CpuLevel::One,
            CpuLevel::Two,
            CpuLevel::Three,
            CpuLevel::Four,
        ];
        for (index, low) in levels.iter().enumerate() {
            for high in &levels[index + 1..] {
                let mut totals = (0, 0);
                for seed in 0..SEEDS {
                    let (weak, strong) = duel(*low, *high, seed);
                    totals.0 += weak;
                    totals.1 += strong;
                }
                assert!(
                    totals.1 - totals.0 >= MARGIN,
                    "{high:?} が {low:?} に勝ち越していない: 合計 {} vs {}",
                    totals.0,
                    totals.1
                );
            }
        }
    }
}

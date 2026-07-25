//! アリーナの形、衝突判定、スポーン地点の選択。

use bevy::prelude::Vec2;
use pixel_shooter_protocol::{ARENA_HEIGHT, ARENA_WIDTH, PLAYER_RADIUS};

use crate::model::MAX_PLAYERS;

/// 参加順に応じた左右の初期位置を返す。
pub(crate) fn spawn_position(index: usize) -> Vec2 {
    if index == 0 {
        Vec2::new(80.0, ARENA_HEIGHT * 0.5)
    } else {
        Vec2::new(ARENA_WIDTH - 80.0, ARENA_HEIGHT * 0.5)
    }
}

/// 相手との距離を主に、近くの弾と小さな揺らぎも考慮して復活地点を選ぶ。
pub(crate) fn choose_respawn_position(
    player_id: u64,
    player_positions: &[(u64, Vec2)],
    bullet_positions: &[Vec2],
    tick: u64,
) -> Vec2 {
    let candidates = [
        Vec2::new(64.0, 60.0),
        Vec2::new(64.0, ARENA_HEIGHT - 60.0),
        Vec2::new(ARENA_WIDTH - 64.0, 60.0),
        Vec2::new(ARENA_WIDTH - 64.0, ARENA_HEIGHT - 60.0),
        Vec2::new(92.0, ARENA_HEIGHT * 0.5),
        Vec2::new(ARENA_WIDTH - 92.0, ARENA_HEIGHT * 0.5),
    ];

    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| valid_player_position(**candidate))
        .max_by(|(left_index, left), (right_index, right)| {
            let left_score = respawn_safety_score(
                player_id,
                **left,
                *left_index,
                player_positions,
                bullet_positions,
                tick,
            );
            let right_score = respawn_safety_score(
                player_id,
                **right,
                *right_index,
                player_positions,
                bullet_positions,
                tick,
            );
            left_score.total_cmp(&right_score)
        })
        .map(|(_, position)| *position)
        .unwrap_or_else(|| spawn_position((player_id as usize) % MAX_PLAYERS))
}

fn respawn_safety_score(
    player_id: u64,
    candidate: Vec2,
    candidate_index: usize,
    player_positions: &[(u64, Vec2)],
    bullet_positions: &[Vec2],
    tick: u64,
) -> f32 {
    let opponent_distance = player_positions
        .iter()
        .filter(|(id, _)| *id != player_id)
        .map(|(_, position)| candidate.distance(*position))
        .fold(ARENA_WIDTH, f32::min);
    let bullet_distance = bullet_positions
        .iter()
        .map(|position| candidate.distance(*position))
        .fold(ARENA_WIDTH, f32::min);

    // 毎回完全に同じ地点にならないよう、距離評価を壊さない範囲で決定的な揺らぎを加える。
    let variation = ((tick + player_id * 31 + candidate_index as u64 * 17) % 11) as f32;
    opponent_distance + bullet_distance * 0.25 + variation
}

/// X・Y軸を分けて、衝突しない分だけ位置を更新する。
pub(crate) fn move_with_collision(position: &mut Vec2, delta: Vec2) {
    let mut next = *position;
    next.x += delta.x;
    if valid_player_position(next) {
        position.x = next.x;
    }
    next = *position;
    next.y += delta.y;
    if valid_player_position(next) {
        position.y = next.y;
    }
}

/// プレイヤーが移動できる位置かを、外周と障害物から判定する。
fn valid_player_position(position: Vec2) -> bool {
    position.x >= PLAYER_RADIUS
        && position.x <= ARENA_WIDTH - PLAYER_RADIUS
        && position.y >= PLAYER_RADIUS
        && position.y <= ARENA_HEIGHT - PLAYER_RADIUS
        && !obstacle_at(position, PLAYER_RADIUS)
}

/// 弾の中心がアリーナ内に残っているかを判定する。
pub(crate) fn bullet_in_bounds(position: Vec2) -> bool {
    position.x >= 0.0
        && position.x <= ARENA_WIDTH
        && position.y >= 0.0
        && position.y <= ARENA_HEIGHT
}

/// 点が中央の長方形障害物内にあるかを判定する。
///
/// プレイヤーでは `margin = PLAYER_RADIUS` として長方形を外側へ広げ、
/// 円の中心が壁へめり込まないようにする。弾ではmarginを0にする。
pub(crate) fn obstacle_at(position: Vec2, margin: f32) -> bool {
    let obstacles = [
        (Vec2::new(250.0, 85.0), Vec2::new(140.0, 28.0)),
        (Vec2::new(250.0, 247.0), Vec2::new(140.0, 28.0)),
    ];
    obstacles.iter().any(|(origin, size)| {
        position.x >= origin.x - margin
            && position.x <= origin.x + size.x + margin
            && position.y >= origin.y - margin
            && position.y <= origin.y + size.y + margin
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_keeps_player_outside_obstacle() {
        let mut position = Vec2::new(235.0, 99.0);
        move_with_collision(&mut position, Vec2::new(10.0, 0.0));
        assert_eq!(position, Vec2::new(235.0, 99.0));
    }

    #[test]
    fn respawn_prefers_the_side_away_from_opponent() {
        let position = choose_respawn_position(1, &[(2, Vec2::new(60.0, 180.0))], &[], 100);
        assert!(position.x > ARENA_WIDTH * 0.5);
    }
}

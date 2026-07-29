//! スコアアイテムの生成と取得。

use bevy::prelude::*;
use pixel_shooter_protocol::{ITEM_RADIUS, PLAYER_RADIUS};

use crate::{
    arena::ArenaMap,
    model::{MatchState, Player, ScoreItem},
    schedule::GameClock,
};

use super::{is_playing_phase, score::add_points};

/// 得点アイテムの生成と取得判定を処理するSystem。
pub(crate) fn update_score_items(
    mut commands: Commands,
    clock: Res<GameClock>,
    map: Res<ArenaMap>,
    mut state: ResMut<MatchState>,
    mut players: Query<&mut Player>,
    items: Query<(Entity, &ScoreItem)>,
) {
    if !is_playing_phase(state.phase) {
        return;
    }

    // 一定間隔で候補地点を巡回し、マップ上の個数が上限未満なら1個生成する。
    state.item_spawn_left = (state.item_spawn_left - clock.delta_seconds()).max(0.0);
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
                commands.spawn(ScoreItem { id, position });
            }
        }
        state.item_spawn_left = state.room_settings.item_spawn_interval;
    }

    // 1つのアイテムを同じtickに2人が取得しないよう、アイテム単位で判定してbreakする。
    let pickup_distance = PLAYER_RADIUS + ITEM_RADIUS;
    for (entity, item) in &items {
        for mut player in &mut players {
            if player.alive
                && player.position.distance_squared(item.position)
                    <= pickup_distance * pickup_distance
            {
                player.score = add_points(player.score, state.room_settings.item_points);
                commands.entity(entity).despawn();
                break;
            }
        }
    }
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

//! GameCoreのSystemを試合進行・移動・戦闘などの責務単位で構成する。

mod combat;
mod cpu;
mod items;
mod match_flow;
mod movement;
mod respawn;
mod score;

use pixel_shooter_protocol::MatchPhase;

pub(crate) use combat::{fire_bullets, move_and_hit_bullets};
pub(crate) use cpu::update_cpu_players;
pub(crate) use items::update_score_items;
pub(crate) use match_flow::update_match;
pub(crate) use movement::move_players;
pub(crate) use respawn::update_respawns;

pub(super) fn is_playing_phase(phase: MatchPhase) -> bool {
    phase == MatchPhase::Running
}

#[cfg(test)]
use crate::{arena::ArenaMap, model::MatchState};
#[cfg(test)]
use bevy::prelude::default;
#[cfg(test)]
use items::choose_score_item_spawn;
#[cfg(test)]
use match_flow::{reset_empty_room, unique_score_winner};
#[cfg(test)]
use pixel_shooter_protocol::ITEM_RADIUS;
#[cfg(test)]
use score::{add_points, subtract_points};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highest_unique_score_wins() {
        assert_eq!(
            unique_score_winner([(1, 75), (2, 120)].into_iter()),
            Some(2)
        );
        assert_eq!(unique_score_winner([(1, -25), (2, -25)].into_iter()), None);
    }

    #[test]
    fn score_item_positions_do_not_overlap_obstacles() {
        let map = ArenaMap::default();
        for index in 0..map.item_spawn_count() {
            assert!(!map.obstacle_at(map.item_spawn_position(index), ITEM_RADIUS));
        }
    }

    #[test]
    fn score_item_spawn_avoids_occupied_candidate() {
        let map = ArenaMap::default();
        let occupied = map.item_spawn_position(0);
        let (_, position) =
            choose_score_item_spawn(&map, 0, &[occupied], &[]).expect("another candidate");
        assert_ne!(position, occupied);
    }

    #[test]
    fn score_events_include_kill_death_and_item_values() {
        let rules = crate::settings::MatchRules::default();
        assert_eq!(add_points(0, rules.kill_points), 100);
        assert_eq!(subtract_points(0, rules.death_penalty), -25);
        assert_eq!(add_points(-25, rules.item_points), -5);
    }

    #[test]
    fn empty_room_reset_removes_host_and_pending_match_state() {
        let mut state = MatchState {
            phase: MatchPhase::MatchFinished,
            phase_time_left: 4.0,
            resume_phase: Some(MatchPhase::Running),
            match_winner_id: Some(2),
            item_spawn_left: 1.0,
            host_player_id: Some(1),
            start_requested: true,
            ..default()
        };

        reset_empty_room(&mut state);

        assert_eq!(state.phase, MatchPhase::Waiting);
        assert_eq!(state.phase_time_left, 0.0);
        assert_eq!(state.resume_phase, None);
        assert_eq!(state.match_winner_id, None);
        assert_eq!(state.item_spawn_left, 0.0);
        assert_eq!(state.host_player_id, None);
        assert!(!state.start_requested);
    }
}

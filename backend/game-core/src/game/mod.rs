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
pub(crate) use cpu::{CpuNavigation, update_cpu_players};
pub(crate) use items::{update_items, update_larokin_poppos};
pub(crate) use match_flow::update_match;
pub(crate) use movement::move_players;
pub(crate) use respawn::update_respawns;

pub(super) fn is_playing_phase(phase: MatchPhase) -> bool {
    phase == MatchPhase::Running
}

#[cfg(test)]
use crate::{
    arena::ArenaMap,
    model::{MatchState, Player},
    schedule::{GameCorePlugin, advance_one_tick},
    settings::GameSettings,
};
#[cfg(test)]
use bevy::prelude::{App, default};
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

    fn test_player(id: u64, is_cpu: bool, connection_id: Option<u64>) -> Player {
        Player {
            id,
            connection_id,
            is_cpu,
            reconnect_token: String::new(),
            reconnect_grace_left: 0.0,
            slot: id as usize - 1,
            name: format!("Player {id}"),
            position: bevy::prelude::Vec2::new(64.0 + id as f32 * 32.0, 64.0),
            aim: bevy::prelude::Vec2::X,
            movement: bevy::prelude::Vec2::ZERO,
            shooting: false,
            hp: 5,
            score: 0,
            alive: true,
            respawn_left: 0.0,
            shot_cooldown: 0.0,
            ammo: 6,
            reload_left: 0.0,
            reload_requested: false,
            invulnerable_left: 0.0,
            dash_cooldown_left: 0.0,
            dash_time_left: 0.0,
            dash_direction: bevy::prelude::Vec2::ZERO,
            dash_requested: false,
            use_item_requested: false,
            held_item: None,
            berserk_left: 0.0,
            shield_hp: 0,
            last_input_sequence: 0,
        }
    }

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

    #[test]
    fn explicit_last_human_leave_destroys_cpu_room_on_next_tick() {
        let settings = GameSettings::default();
        let mut app = App::new();
        app.insert_resource(settings.clone())
            .insert_resource(MatchState {
                phase: MatchPhase::Running,
                phase_time_left: 60.0,
                host_player_id: Some(1),
                reconnect_grace_seconds: settings.match_rules.reconnect_grace_seconds,
                room_settings: settings.room_settings(),
                ..default()
            })
            .add_plugins(GameCorePlugin::new(60.0));
        // 明示的なleaveを受けた人間はconnectionなし・猶予0としてGameTickへ渡る。
        app.world_mut().spawn(test_player(1, false, None));
        app.world_mut().spawn(test_player(2, true, None));

        advance_one_tick(app.world_mut());

        let player_count = {
            let world = app.world_mut();
            let mut players = world.query::<&Player>();
            players.iter(world).count()
        };
        assert_eq!(player_count, 0);
        let state = app.world().resource::<MatchState>();
        assert_eq!(state.phase, MatchPhase::Waiting);
        assert_eq!(state.host_player_id, None);
    }
}

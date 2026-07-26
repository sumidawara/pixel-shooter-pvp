//! 実時間ループから独立した、ゲーム世界を1tick進めるBevy Schedule。
//!
//! WebSocket、OSスレッド、待受ポートなどのサービス都合をここへ入れないことで、
//! 通常運転、1tickデバッグ、リプレイのどれからでも同じゲーム計算を呼び出せる。

use bevy::{ecs::schedule::ScheduleLabel, prelude::*};

use crate::game;

/// 実時間とは無関係な、ゲーム内の1tickの長さ。
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct GameClock {
    delta_seconds: f32,
}

impl GameClock {
    fn from_hz(tick_rate: f64) -> Self {
        Self {
            delta_seconds: (1.0 / tick_rate) as f32,
        }
    }

    pub(crate) fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }
}

/// 1回実行するとゲーム世界がちょうど1tick進むSchedule。
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GameTick;

/// ゲーム計算だけをAppへ登録するPlugin。
pub(crate) struct GameCorePlugin {
    tick_rate: f64,
}

impl GameCorePlugin {
    pub(crate) fn new(tick_rate: f64) -> Self {
        Self { tick_rate }
    }
}

impl Plugin for GameCorePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GameClock::from_hz(self.tick_rate))
            .init_schedule(GameTick)
            .add_systems(
                GameTick,
                (
                    game::update_match,
                    game::update_cpu_players,
                    game::move_players,
                    game::fire_bullets,
                    game::move_and_hit_bullets,
                    game::update_score_items,
                    game::update_respawns,
                )
                    // 入力反映後の状態を、移動→射撃→判定の順に確定する。
                    .chain(),
            );
    }
}

/// 実時間ランナーや将来のデバッグ操作からGameTickを1回だけ呼び出す。
pub(crate) fn advance_one_tick(world: &mut World) {
    world.run_schedule(GameTick);
}

#[cfg(test)]
mod tests {
    use pixel_shooter_protocol::MatchPhase;

    use super::*;
    use crate::{config::ServerSettings, model::MatchState};

    #[test]
    fn game_tick_can_advance_without_realtime_runner() {
        let settings = ServerSettings::default();
        let reconnect_grace_seconds = settings.match_rules.reconnect_grace_seconds;
        let room_settings = settings.room_settings();
        let mut app = App::new();
        app.insert_resource(settings)
            .insert_resource(MatchState {
                reconnect_grace_seconds,
                room_settings,
                ..default()
            })
            .add_plugins(GameCorePlugin::new(60.0));

        assert_eq!(app.world().resource::<MatchState>().tick, 0);
        assert_eq!(
            app.world().resource::<GameClock>().delta_seconds(),
            1.0 / 60.0
        );
        advance_one_tick(app.world_mut());
        assert_eq!(app.world().resource::<MatchState>().tick, 1);
        advance_one_tick(app.world_mut());
        assert_eq!(app.world().resource::<MatchState>().tick, 2);
    }

    #[test]
    fn manual_ticks_advance_game_time_without_waiting_for_wall_clock() {
        let settings = ServerSettings::default();
        let reconnect_grace_seconds = settings.match_rules.reconnect_grace_seconds;
        let room_settings = settings.room_settings();
        let mut app = App::new();
        app.insert_resource(settings)
            .insert_resource(MatchState {
                phase: MatchPhase::Countdown,
                phase_time_left: 3.0,
                reconnect_grace_seconds,
                room_settings,
                ..default()
            })
            .add_plugins(GameCorePlugin::new(60.0));

        for _ in 0..60 {
            advance_one_tick(app.world_mut());
        }

        let state = app.world().resource::<MatchState>();
        assert_eq!(state.tick, 60);
        assert!((state.phase_time_left - 2.0).abs() < 0.001);
    }
}

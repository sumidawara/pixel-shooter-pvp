use bevy::prelude::{App, Vec2};
use pixel_shooter_protocol::MatchPhase;

use crate::{
    model::{MatchState, Player},
    schedule::GameCorePlugin,
    settings::GameSettings,
};

pub(super) fn test_app(phase: MatchPhase, tick_rate: f64) -> App {
    let settings = GameSettings::default();
    let reconnect_grace_seconds = settings.match_rules.reconnect_grace_seconds;
    let room_settings = settings.room_settings();
    let mut app = App::new();
    app.insert_resource(settings)
        .insert_resource(MatchState {
            phase,
            phase_time_left: 60.0,
            reconnect_grace_seconds,
            room_settings,
            item_spawn_left: 60.0,
            ..Default::default()
        })
        .add_plugins(GameCorePlugin::new(tick_rate));
    app
}

pub(super) fn test_player(id: u64, connection_id: Option<u64>) -> Player {
    Player {
        id,
        connection_id,
        is_cpu: false,
        reconnect_token: String::new(),
        reconnect_grace_left: 0.0,
        slot: id as usize - 1,
        name: format!("Player {id}"),
        position: Vec2::new(64.0 + id as f32 * 32.0, 64.0),
        aim: Vec2::X,
        movement: Vec2::ZERO,
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
        dash_direction: Vec2::ZERO,
        dash_requested: false,
        use_item_requested: false,
        held_item: None,
        berserk_left: 0.0,
        shield_hp: 0,
        last_input_sequence: 0,
    }
}

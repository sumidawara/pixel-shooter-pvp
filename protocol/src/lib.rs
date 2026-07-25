use serde::{Deserialize, Serialize};

pub const ARENA_WIDTH: f32 = 640.0;
pub const ARENA_HEIGHT: f32 = 360.0;
pub const PLAYER_RADIUS: f32 = 12.0;
pub const BULLET_RADIUS: f32 = 4.0;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Join {
        name: String,
        #[serde(default)]
        reconnect_token: Option<String>,
    },
    Input {
        sequence: u32,
        move_x: f32,
        move_y: f32,
        aim_x: f32,
        aim_y: f32,
        shooting: bool,
        reload_pressed: bool,
        dash_pressed: bool,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        player_id: u64,
        reconnect_token: String,
        reconnected: bool,
    },
    Rejected {
        reason: String,
    },
    Snapshot(Snapshot),
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub tick: u64,
    pub phase: MatchPhase,
    pub time_left: f32,
    pub round_number: u32,
    pub rounds_to_win: u32,
    pub round_winner_id: Option<u64>,
    pub winner_id: Option<u64>,
    pub reconnect_grace_left: f32,
    pub players: Vec<PlayerSnapshot>,
    pub bullets: Vec<BulletSnapshot>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhase {
    #[default]
    Waiting,
    Countdown,
    Running,
    Overtime,
    RoundEnd,
    Paused,
    MatchFinished,
}

#[derive(Debug, Serialize)]
pub struct PlayerSnapshot {
    pub id: u64,
    pub name: String,
    pub position: Vec2,
    pub aim: Vec2,
    pub hp: i32,
    pub max_hp: i32,
    pub score: u32,
    pub round_wins: u32,
    pub connected: bool,
    pub reconnect_grace_left: f32,
    pub alive: bool,
    pub respawn_left: f32,
    pub invulnerable_left: f32,
    pub ammo: u32,
    pub max_ammo: u32,
    pub reloading: bool,
    pub reload_left: f32,
    pub dash_cooldown_left: f32,
    pub dashing: bool,
    pub dash_time_left: f32,
    pub last_input_sequence: u32,
}

#[derive(Debug, Serialize)]
pub struct BulletSnapshot {
    pub id: u64,
    pub owner_id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
}

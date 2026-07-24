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
    },
    Input {
        sequence: u32,
        move_x: f32,
        move_y: f32,
        aim_x: f32,
        aim_y: f32,
        shooting: bool,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome { player_id: u64 },
    Rejected { reason: String },
    Snapshot(Snapshot),
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub tick: u64,
    pub phase: MatchPhase,
    pub time_left: f32,
    pub winner_id: Option<u64>,
    pub players: Vec<PlayerSnapshot>,
    pub bullets: Vec<BulletSnapshot>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhase {
    Waiting,
    Running,
    Finished,
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
    pub alive: bool,
    pub respawn_left: f32,
    pub last_input_sequence: u32,
}

#[derive(Debug, Serialize)]
pub struct BulletSnapshot {
    pub id: u64,
    pub owner_id: u64,
    pub position: Vec2,
}

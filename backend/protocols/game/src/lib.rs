use serde::{Deserialize, Serialize};

pub const PLAYER_RADIUS: f32 = 12.0;
pub const BULLET_RADIUS: f32 = 4.0;
pub const ITEM_RADIUS: f32 = 10.0;

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
        #[serde(default)]
        join_ticket: Option<String>,
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
    AddCpu,
    RemoveCpu {
        player_id: u64,
    },
    UpdateRoomSettings {
        settings: RoomSettings,
    },
    StartMatch,
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
    Snapshot(Box<Snapshot>),
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub tick: u64,
    pub map: MapSnapshot,
    pub phase: MatchPhase,
    pub time_left: f32,
    pub winner_id: Option<u64>,
    pub reconnect_grace_left: f32,
    pub move_speed: f32,
    pub dash_speed: f32,
    pub dash_duration: f32,
    pub dash_cooldown: f32,
    pub players: Vec<PlayerSnapshot>,
    pub bullets: Vec<BulletSnapshot>,
    pub items: Vec<ItemSnapshot>,
    pub room: RoomSnapshot,
}

#[derive(Debug, Serialize)]
pub struct MapSnapshot {
    pub id: String,
    pub revision: String,
    pub width: usize,
    pub height: usize,
    pub tile_size: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RoomSettings {
    pub match_seconds: f32,
    pub kill_points: i32,
    pub death_penalty: i32,
    pub item_points: i32,
    pub item_spawn_interval: f32,
    pub max_items: u32,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            match_seconds: 120.0,
            kill_points: 100,
            death_penalty: 25,
            item_points: 20,
            item_spawn_interval: 5.0,
            max_items: 3,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RoomSnapshot {
    pub host_player_id: Option<u64>,
    pub can_start: bool,
    pub max_players: usize,
    pub settings: RoomSettings,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPhase {
    #[default]
    Waiting,
    Countdown,
    Running,
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
    pub score: i32,
    pub is_cpu: bool,
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

#[derive(Debug, Serialize)]
pub struct ItemSnapshot {
    pub id: u64,
    pub position: Vec2,
    pub points: i32,
}

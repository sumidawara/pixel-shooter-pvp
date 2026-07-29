use serde::{Deserialize, Serialize};

pub const PLAYER_RADIUS: f32 = 12.0;
pub const BULLET_RADIUS: f32 = 4.0;
pub const ITEM_RADIUS: f32 = 10.0;

/// クライアント、訓練環境、デバッグ入力注入で共通利用する1tick分の操作。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerInput {
    pub move_x: f32,
    pub move_y: f32,
    pub aim_x: f32,
    pub aim_y: f32,
    pub shooting: bool,
    pub reload_pressed: bool,
    pub dash_pressed: bool,
}

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
    MapDefinition {
        map: MapDefinition,
    },
    MapCatalog {
        maps: Vec<MapSummary>,
    },
    Snapshot(Box<Snapshot>),
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub tick: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDefinition {
    pub schema_version: u32,
    pub id: String,
    pub revision: String,
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub tile_size: u32,
    pub tiles: Vec<String>,
    pub spawn_points: Vec<[usize; 2]>,
    pub item_spawn_points: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSettings {
    #[serde(default = "default_map_id")]
    pub map_id: String,
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
            map_id: default_map_id(),
            match_seconds: 120.0,
            kill_points: 100,
            death_penalty: 25,
            item_points: 20,
            item_spawn_interval: 5.0,
            max_items: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapSummary {
    pub id: String,
    pub name: String,
}

fn default_map_id() -> String {
    "classic_arena".into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_definition_has_a_dedicated_message_type() {
        let message = ServerMessage::MapDefinition {
            map: MapDefinition {
                schema_version: 1,
                id: "test".into(),
                revision: "1".into(),
                name: "Test".into(),
                width: 2,
                height: 1,
                tile_size: 32,
                tiles: vec![".#".into()],
                spawn_points: vec![[0, 0]],
                item_spawn_points: vec![[0, 0]],
            },
        };
        let value = serde_json::to_value(message).expect("serialize map message");
        assert_eq!(value["type"], "map_definition");
        assert_eq!(value["map"]["tiles"][0], ".#");
    }

    #[test]
    fn map_catalog_has_menu_labels_and_ids() {
        let message = ServerMessage::MapCatalog {
            maps: vec![MapSummary {
                id: "crossroads".into(),
                name: "Crossroads".into(),
            }],
        };
        let value = serde_json::to_value(message).expect("serialize map catalog");

        assert_eq!(value["type"], "map_catalog");
        assert_eq!(value["maps"][0]["id"], "crossroads");
        assert_eq!(value["maps"][0]["name"], "Crossroads");
    }
}

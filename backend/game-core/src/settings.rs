//! GameCoreだけが参照する試合ルールと操作パラメーター。

use bevy::prelude::Resource;
use pixel_shooter_protocol::RoomSettings;
use serde::Deserialize;

#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    #[serde(rename = "match")]
    pub match_rules: MatchRules,
    pub gameplay: GameplaySettings,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct MatchRules {
    pub match_seconds: f32,
    pub countdown_seconds: f32,
    pub match_finished_seconds: f32,
    pub reconnect_grace_seconds: f32,
    pub kill_points: i32,
    pub death_penalty: i32,
    pub item_points: i32,
    pub item_spawn_interval: f32,
    pub max_items: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct GameplaySettings {
    pub move_speed: f32,
    pub bullet_speed: f32,
    pub shot_interval: f32,
    pub recoil_distance: f32,
    pub max_ammo: u32,
    pub reload_seconds: f32,
    pub max_hp: i32,
    pub hit_invulnerable_seconds: f32,
    pub respawn_invulnerable_seconds: f32,
    pub respawn_seconds: f32,
    pub dash_speed: f32,
    pub dash_duration: f32,
    pub dash_cooldown: f32,
}

impl GameSettings {
    pub fn room_settings(&self) -> RoomSettings {
        RoomSettings {
            map_id: "classic_arena".into(),
            match_seconds: self.match_rules.match_seconds,
            kill_points: self.match_rules.kill_points,
            death_penalty: self.match_rules.death_penalty,
            item_points: self.match_rules.item_points,
            item_spawn_interval: self.match_rules.item_spawn_interval,
            max_items: self.match_rules.max_items as u32,
            sandbox: false,
        }
    }

    pub fn sanitize_room_settings(&self, mut room: RoomSettings) -> RoomSettings {
        room.match_seconds = room.match_seconds.clamp(30.0, 900.0);
        room.kill_points = room.kill_points.clamp(0, 10_000);
        room.death_penalty = room.death_penalty.clamp(0, 10_000);
        room.item_points = room.item_points.clamp(0, 10_000);
        room.item_spawn_interval = room.item_spawn_interval.clamp(0.5, 60.0);
        room.max_items = room.max_items.clamp(1, 16);
        room
    }

    pub fn sanitize(&mut self) {
        self.match_rules.match_seconds = self.match_rules.match_seconds.max(1.0);
        self.match_rules.countdown_seconds = self.match_rules.countdown_seconds.max(0.0);
        self.match_rules.match_finished_seconds = self.match_rules.match_finished_seconds.max(0.1);
        self.match_rules.reconnect_grace_seconds =
            self.match_rules.reconnect_grace_seconds.max(0.1);
        self.match_rules.kill_points = self.match_rules.kill_points.max(0);
        self.match_rules.death_penalty = self.match_rules.death_penalty.max(0);
        self.match_rules.item_points = self.match_rules.item_points.max(0);
        self.match_rules.item_spawn_interval = self.match_rules.item_spawn_interval.max(0.1);
        self.match_rules.max_items = self.match_rules.max_items.clamp(1, 16);
        self.gameplay.move_speed = self.gameplay.move_speed.max(1.0);
        self.gameplay.bullet_speed = self.gameplay.bullet_speed.max(1.0);
        self.gameplay.shot_interval = self.gameplay.shot_interval.max(0.01);
        self.gameplay.recoil_distance = self.gameplay.recoil_distance.max(0.0);
        self.gameplay.max_ammo = self.gameplay.max_ammo.max(1);
        self.gameplay.reload_seconds = self.gameplay.reload_seconds.max(0.01);
        self.gameplay.max_hp = self.gameplay.max_hp.max(1);
        self.gameplay.hit_invulnerable_seconds = self.gameplay.hit_invulnerable_seconds.max(0.0);
        self.gameplay.respawn_invulnerable_seconds =
            self.gameplay.respawn_invulnerable_seconds.max(0.0);
        self.gameplay.respawn_seconds = self.gameplay.respawn_seconds.max(0.1);
        self.gameplay.dash_speed = self.gameplay.dash_speed.max(1.0);
        self.gameplay.dash_duration = self.gameplay.dash_duration.max(0.01);
        self.gameplay.dash_cooldown = self.gameplay.dash_cooldown.max(0.01);
    }
}

impl Default for MatchRules {
    fn default() -> Self {
        Self {
            match_seconds: 120.0,
            countdown_seconds: 3.0,
            match_finished_seconds: 6.0,
            reconnect_grace_seconds: 15.0,
            kill_points: 100,
            death_penalty: 25,
            item_points: 20,
            item_spawn_interval: 5.0,
            max_items: 3,
        }
    }
}

impl Default for GameplaySettings {
    fn default() -> Self {
        Self {
            move_speed: 150.0,
            bullet_speed: 340.0,
            shot_interval: 0.24,
            recoil_distance: 5.0,
            max_ammo: 6,
            reload_seconds: 1.5,
            max_hp: 5,
            hit_invulnerable_seconds: 0.18,
            respawn_invulnerable_seconds: 1.0,
            respawn_seconds: 2.0,
            dash_speed: 520.0,
            dash_duration: 0.13,
            dash_cooldown: 1.1,
        }
    }
}

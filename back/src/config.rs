//! `server.json` と環境変数からサーバー設定を読み込む。

use std::fs;

use bevy::prelude::Resource;
use serde::Deserialize;

/// Bevy全体から参照するサーバー設定Resource。
///
/// `#[serde(default)]` により、設定ファイルに新しい項目が増えても、
/// 書かれていない項目には安全な初期値が入る。
#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ServerSettings {
    pub(crate) network: NetworkSettings,
    #[serde(rename = "match")]
    pub(crate) match_rules: MatchRules,
    pub(crate) gameplay: GameplaySettings,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct NetworkSettings {
    pub(crate) bind_address: String,
    pub(crate) tick_rate: f64,
    pub(crate) snapshot_every_ticks: u64,
    pub(crate) simulated_latency_ms: u64,
    pub(crate) simulated_loss_percent: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct MatchRules {
    pub(crate) match_seconds: f32,
    pub(crate) countdown_seconds: f32,
    pub(crate) match_finished_seconds: f32,
    pub(crate) reconnect_grace_seconds: f32,
    pub(crate) kill_points: i32,
    pub(crate) death_penalty: i32,
    pub(crate) item_points: i32,
    pub(crate) item_spawn_interval: f32,
    pub(crate) max_items: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct GameplaySettings {
    pub(crate) move_speed: f32,
    pub(crate) bullet_speed: f32,
    pub(crate) shot_interval: f32,
    pub(crate) recoil_distance: f32,
    pub(crate) max_ammo: u32,
    pub(crate) reload_seconds: f32,
    pub(crate) max_hp: i32,
    pub(crate) hit_invulnerable_seconds: f32,
    pub(crate) respawn_invulnerable_seconds: f32,
    pub(crate) respawn_seconds: f32,
    pub(crate) dash_speed: f32,
    pub(crate) dash_duration: f32,
    pub(crate) dash_cooldown: f32,
}

impl ServerSettings {
    /// 設定ファイルを読み、運用向け環境変数を上書きして安全な値へ補正する。
    pub(crate) fn load() -> Self {
        let path =
            std::env::var("PIXEL_SHOOTER_CONFIG").unwrap_or_else(|_| "server.json".to_string());
        let mut settings = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(settings) => {
                    println!("Loaded server settings from {path}");
                    settings
                }
                Err(error) => {
                    eprintln!("Could not parse {path}: {error}; using defaults");
                    Self::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("{path} was not found; using built-in defaults");
                Self::default()
            }
            Err(error) => {
                eprintln!("Could not read {path}: {error}; using defaults");
                Self::default()
            }
        };
        settings.apply_environment_overrides();
        settings.sanitize();
        settings
    }

    /// 設定ファイルを書き換えにくいコンテナ環境向けの上書きを適用する。
    fn apply_environment_overrides(&mut self) {
        self.network.bind_address = std::env::var("PIXEL_SHOOTER_BIND_ADDR")
            .unwrap_or_else(|_| self.network.bind_address.clone());
        self.network.simulated_latency_ms = env_u64(
            "PIXEL_SHOOTER_LATENCY_MS",
            self.network.simulated_latency_ms,
        );
        self.network.simulated_loss_percent = env_u64(
            "PIXEL_SHOOTER_PACKET_LOSS_PERCENT",
            u64::from(self.network.simulated_loss_percent),
        )
        .min(100) as u32;
        self.match_rules.reconnect_grace_seconds = env_f32(
            "PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS",
            self.match_rules.reconnect_grace_seconds,
        )
        .max(0.1);
    }

    /// ゼロ除算や処理過多を起こさない範囲へ設定値を補正する。
    fn sanitize(&mut self) {
        self.network.tick_rate = self.network.tick_rate.clamp(10.0, 240.0);
        self.network.snapshot_every_ticks = self.network.snapshot_every_ticks.max(1);
        self.network.simulated_loss_percent = self.network.simulated_loss_percent.min(100);
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

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9001".into(),
            tick_rate: 60.0,
            snapshot_every_ticks: 3,
            simulated_latency_ms: 0,
            simulated_loss_percent: 0,
        }
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
            reload_seconds: 1.0,
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

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f32(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_server_settings_are_clamped() {
        let mut settings = ServerSettings::default();
        settings.network.tick_rate = 0.0;
        settings.network.snapshot_every_ticks = 0;
        settings.network.simulated_loss_percent = 999;
        settings.gameplay.max_hp = 0;
        settings.gameplay.max_ammo = 0;
        settings.match_rules.kill_points = -1;
        settings.match_rules.max_items = 0;

        settings.sanitize();

        assert_eq!(settings.network.tick_rate, 10.0);
        assert_eq!(settings.network.snapshot_every_ticks, 1);
        assert_eq!(settings.network.simulated_loss_percent, 100);
        assert_eq!(settings.gameplay.max_hp, 1);
        assert_eq!(settings.gameplay.max_ammo, 1);
        assert_eq!(settings.match_rules.kill_points, 0);
        assert_eq!(settings.match_rules.max_items, 1);
    }
}

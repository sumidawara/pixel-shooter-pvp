//! GameServerの`server.json`と環境変数を読み込む。

use std::fs;

use bevy::prelude::Resource;
use pixel_shooter_game_core::GameSettings;
use serde::Deserialize;

/// Bevy全体から参照するサーバー設定Resource。
///
/// `#[serde(default)]` により、設定ファイルに新しい項目が増えても、
/// 書かれていない項目には安全な初期値が入る。
#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ServerSettings {
    pub(crate) network: NetworkSettings,
    pub(crate) debug: DebugSettings,
    #[serde(flatten)]
    pub(crate) game: GameSettings,
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
pub(crate) struct DebugSettings {
    pub(crate) enabled: bool,
    pub(crate) bind_address: String,
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
        self.debug.enabled = env_bool("PIXEL_SHOOTER_DEBUG_ENABLED", self.debug.enabled);
        self.debug.bind_address = std::env::var("PIXEL_SHOOTER_DEBUG_BIND_ADDR")
            .unwrap_or_else(|_| self.debug.bind_address.clone());
        self.game.match_rules.reconnect_grace_seconds = env_f32(
            "PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS",
            self.game.match_rules.reconnect_grace_seconds,
        )
        .max(0.1);
    }

    /// ゼロ除算や処理過多を起こさない範囲へ設定値を補正する。
    fn sanitize(&mut self) {
        self.network.tick_rate = self.network.tick_rate.clamp(10.0, 240.0);
        self.network.snapshot_every_ticks = self.network.snapshot_every_ticks.max(1);
        self.network.simulated_loss_percent = self.network.simulated_loss_percent.min(100);
        self.game.sanitize();
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

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            bind_address: "127.0.0.1:9101".into(),
        }
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
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
        settings.game.gameplay.max_hp = 0;
        settings.game.gameplay.max_ammo = 0;
        settings.game.match_rules.kill_points = -1;
        settings.game.match_rules.max_items = 0;

        settings.sanitize();

        assert_eq!(settings.network.tick_rate, 10.0);
        assert_eq!(settings.network.snapshot_every_ticks, 1);
        assert_eq!(settings.network.simulated_loss_percent, 100);
        assert_eq!(settings.game.gameplay.max_hp, 1);
        assert_eq!(settings.game.gameplay.max_ammo, 1);
        assert_eq!(settings.game.match_rules.kill_points, 0);
        assert_eq!(settings.game.match_rules.max_items, 1);
    }
}

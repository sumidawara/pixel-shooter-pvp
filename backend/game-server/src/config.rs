//! GameServerの`server.json`と環境変数を読み込む。

use std::{fs, path::PathBuf};

use bevy::prelude::Resource;
use pixel_shooter_game_core::GameSettings;
use serde::Deserialize;

/// 空きポート探索の上限。設定の桁間違いで起動が固まるのを防ぐ。
const MAX_PORT_SEARCH_RANGE: u32 = 1000;

/// Bevy全体から参照するサーバー設定Resource。
///
/// `#[serde(default)]` により、設定ファイルに新しい項目が増えても、
/// 書かれていない項目には安全な初期値が入る。
#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct ServerSettings {
    pub(crate) network: NetworkSettings,
    pub(crate) control: ControlSettings,
    #[serde(flatten)]
    pub(crate) game: GameSettings,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct NetworkSettings {
    pub(crate) bind_address: String,
    /// `bind_address`のポートが埋まっていたとき、いくつ先まで空きを探すか。
    ///
    /// `0`ならポート固定。指定した番号が使えなければ起動しない。
    /// 公開先を他へ知らせている場合（Compose越しのポート公開など）は、
    /// 勝手に番号が変わると接続先が食い違うので`0`にする。
    pub(crate) port_search_range: u32,
    pub(crate) tick_rate: f64,
    pub(crate) snapshot_every_ticks: u64,
    pub(crate) simulated_latency_ms: u64,
    pub(crate) simulated_loss_percent: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub(crate) struct ControlSettings {
    pub(crate) bind_address: String,
    /// 制御APIの空きポート探索範囲。意味は`network.port_search_range`と同じ。
    pub(crate) port_search_range: u32,
    pub(crate) control_url: String,
    pub(crate) server_id: String,
    pub(crate) public_url: String,
    pub(crate) admin_url: String,
    pub(crate) require_join_ticket: bool,
    pub(crate) join_secret: String,
}

impl ServerSettings {
    /// 設定ファイルを読み、運用向け環境変数を上書きして安全な値へ補正する。
    /// server.json を探す場所を、優先順に並べる。
    ///
    /// カレントディレクトリしか見ていなかったため、Godotの CREATE ROOM や
    /// 配布版のように別の場所から起動されると設定が読まれず、
    /// 「server.json を編集したのに反映されない」という状態になっていた。
    fn config_candidates() -> Vec<PathBuf> {
        // 明示的に指定された場合は、それだけを使う。黙って別の設定を読むと混乱する。
        if let Ok(path) = std::env::var("PIXEL_SHOOTER_CONFIG") {
            return vec![PathBuf::from(path)];
        }
        let mut candidates = vec![PathBuf::from("server.json")];
        if let Ok(executable) = std::env::current_exe()
            && let Some(directory) = executable.parent()
        {
            // 配布物では実行ファイルの隣に置かれる。
            candidates.push(directory.join("server.json"));
            // macOSのアプリバンドルでは Contents/MacOS/ の隣の Resources/ に入る。
            candidates.push(directory.join("../Resources/server.json"));
        }
        candidates
    }

    pub(crate) fn load() -> Self {
        let candidates = Self::config_candidates();
        let path = candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .cloned()
            .unwrap_or_else(|| candidates[0].clone());
        let path = path.display().to_string();
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
                // どこを探したかを出す。読まれていないことに気付けるように。
                eprintln!(
                    "server settings were not found; using built-in defaults (looked in: {})",
                    Self::config_candidates()
                        .iter()
                        .map(|candidate| candidate.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let _ = error;
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
        // 上限で丸めてから u32 へ落とす。先に `as u32` すると、
        // 巨大な値が 0（＝ポート固定）へ化けて意味が反転する。
        self.network.port_search_range = env_u64(
            "PIXEL_SHOOTER_PORT_SEARCH_RANGE",
            u64::from(self.network.port_search_range),
        )
        .min(u64::from(MAX_PORT_SEARCH_RANGE)) as u32;
        self.network.simulated_latency_ms = env_u64(
            "PIXEL_SHOOTER_LATENCY_MS",
            self.network.simulated_latency_ms,
        );
        self.network.simulated_loss_percent = env_u64(
            "PIXEL_SHOOTER_PACKET_LOSS_PERCENT",
            u64::from(self.network.simulated_loss_percent),
        )
        .min(100) as u32;
        self.control.bind_address = std::env::var("PIXEL_SHOOTER_CONTROL_BIND_ADDR")
            .or_else(|_| std::env::var("PIXEL_SHOOTER_DEBUG_BIND_ADDR"))
            .unwrap_or_else(|_| self.control.bind_address.clone());
        self.control.port_search_range = env_u64(
            "PIXEL_SHOOTER_CONTROL_PORT_SEARCH_RANGE",
            u64::from(self.control.port_search_range),
        )
        .min(u64::from(MAX_PORT_SEARCH_RANGE)) as u32;
        self.control.control_url = std::env::var("PIXEL_SHOOTER_CONTROL_URL")
            .unwrap_or_else(|_| self.control.control_url.clone());
        self.control.server_id = std::env::var("PIXEL_SHOOTER_SERVER_ID")
            .unwrap_or_else(|_| self.control.server_id.clone());
        self.control.public_url = std::env::var("PIXEL_SHOOTER_PUBLIC_URL")
            .unwrap_or_else(|_| self.control.public_url.clone());
        self.control.admin_url = std::env::var("PIXEL_SHOOTER_ADMIN_URL")
            .unwrap_or_else(|_| self.control.admin_url.clone());
        self.control.require_join_ticket = env_bool(
            "PIXEL_SHOOTER_REQUIRE_JOIN_TICKET",
            self.control.require_join_ticket,
        );
        self.control.join_secret = std::env::var("PIXEL_SHOOTER_JOIN_SECRET")
            .unwrap_or_else(|_| self.control.join_secret.clone());
        self.game.match_rules.reconnect_grace_seconds = env_f32(
            "PIXEL_SHOOTER_RECONNECT_GRACE_SECONDS",
            self.game.match_rules.reconnect_grace_seconds,
        )
        .max(0.1);
    }

    /// ゼロ除算や処理過多を起こさない範囲へ設定値を補正する。
    fn sanitize(&mut self) {
        // 桁を間違えて書かれても、起動時に何万個もポートを試さないようにする。
        self.network.port_search_range = self.network.port_search_range.min(MAX_PORT_SEARCH_RANGE);
        self.control.port_search_range = self.control.port_search_range.min(MAX_PORT_SEARCH_RANGE);
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
            // 既定は探索する。ポート番号は遊ぶ人にとってどうでもよく、
            // 何かに埋まっているだけで部屋を作れないのは行き止まりになる。
            port_search_range: 20,
            tick_rate: 60.0,
            snapshot_every_ticks: 3,
            simulated_latency_ms: 0,
            simulated_loss_percent: 0,
        }
    }
}

impl Default for ControlSettings {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9101".into(),
            port_search_range: 20,
            control_url: "http://127.0.0.1:9101".into(),
            server_id: "local-game-1".into(),
            public_url: "ws://127.0.0.1:9001".into(),
            admin_url: String::new(),
            require_join_ticket: false,
            join_secret: "development-only-secret".into(),
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

    /// 探索範囲は上限で丸める。桁を間違えて書かれても起動が固まらないように。
    #[test]
    fn a_huge_search_range_is_clamped() {
        let mut settings = ServerSettings::default();
        settings.network.port_search_range = 60_000;
        settings.control.port_search_range = 60_000;

        settings.sanitize();

        assert_eq!(settings.network.port_search_range, MAX_PORT_SEARCH_RANGE);
        assert_eq!(settings.control.port_search_range, MAX_PORT_SEARCH_RANGE);
    }

    /// 0は「探索しない」という指定。丸めの都合で1以上へ持ち上げてはいけない。
    /// 公開先へ知らせた番号から勝手にずれると、誰も接続できなくなる。
    #[test]
    fn a_fixed_port_stays_fixed() {
        let mut settings = ServerSettings::default();
        settings.network.port_search_range = 0;
        settings.control.port_search_range = 0;

        settings.sanitize();

        assert_eq!(settings.network.port_search_range, 0);
        assert_eq!(settings.control.port_search_range, 0);
    }

    /// 配る`server.json`にポート設定が載っていること。
    ///
    /// 既定値はRust側にもあるが、書かれていない設定は存在に気付かれない。
    /// ポートを固定したい人が最初に開くのはこのファイルになる。
    #[test]
    fn the_shipped_config_documents_the_port_settings() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../server.json");
        let text = std::fs::read_to_string(path).expect("server.json");
        let settings: ServerSettings = serde_json::from_str(&text).expect("server.json parses");

        assert!(
            text.contains("\"port_search_range\""),
            "server.json に port_search_range が書かれていない"
        );
        assert_eq!(settings.network.port_search_range, 20);
        assert_eq!(settings.control.port_search_range, 20);
    }

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

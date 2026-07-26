//! Pixel Shooter PvP の権威GameServer。
//!
//! このプログラムでは、Godotクライアントは「キーやマウスの入力」だけを送り、
//! プレイヤーの位置・弾・HP・得点などの正しい状態はすべてサーバーが決める。
//!
//! Bevy初心者向けの用語:
//! - Entity: ゲーム内の物を識別する番号。プレイヤーや弾に自動で割り当てられる。
//! - Component: Entityに付けるデータ。本作では `Player` と `Bullet`。
//! - Resource: ゲーム世界に1個だけ存在する共有データ。
//! - System: 毎tick実行される普通のRust関数。必要なデータはBevyが引数へ渡す。
//! - Query: 指定したComponentを持つEntityを検索・反復する仕組み。
//! - Commands: Entityの作成・削除を予約する仕組み。
//!
//! モジュール構成:
//! - `config`: server.jsonと環境変数
//! - `network`: WebSocket通信
//! - `server_runtime`: 通信と実時間ランナーをGameTickへ接続

mod config;
mod control;
mod network;
mod server_runtime;

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, prelude::*, time::Fixed};
use pixel_shooter_game_core::{ArenaMap, GameCorePlugin, MatchState};

use crate::{config::ServerSettings, server_runtime::ServerRuntimePlugin};

fn main() {
    let mut settings = ServerSettings::load();
    if let Some(bind_address) = command_line_value("--bind") {
        settings.network.bind_address = bind_address;
    }
    if let Some(debug_bind_address) = command_line_value("--debug-bind") {
        settings.control.bind_address = debug_bind_address;
    }
    let tick_rate = settings.network.tick_rate;
    let arena_map = match std::env::var("PIXEL_SHOOTER_MAP") {
        Ok(path) => ArenaMap::load(&path)
            .unwrap_or_else(|error| panic!("Could not load PIXEL_SHOOTER_MAP {path}: {error}")),
        Err(_) => ArenaMap::default(),
    };
    let game_settings = settings.game.clone();
    let reconnect_grace_seconds = game_settings.match_rules.reconnect_grace_seconds;
    let room_settings = game_settings.room_settings();
    let control_plane = control::start(&settings);
    let network = network::start(&settings, control_plane.snapshot());

    println!(
        "Pixel Shooter server listening on ws://{}",
        settings.network.bind_address
    );
    println!(
        "Rules: {} second score match, kill +{}, death -{}, item +{}, {} HP, {} ammo",
        game_settings.match_rules.match_seconds,
        game_settings.match_rules.kill_points,
        game_settings.match_rules.death_penalty,
        game_settings.match_rules.item_points,
        game_settings.gameplay.max_hp,
        game_settings.gameplay.max_ammo
    );
    println!(
        "Map: {} ({}, {}x{} tiles at {} px)",
        arena_map.name(),
        arena_map.id(),
        arena_map.width(),
        arena_map.height(),
        arena_map.tile_size()
    );
    if settings.network.simulated_latency_ms > 0 || settings.network.simulated_loss_percent > 0 {
        println!(
            "Network simulation: {} ms latency, {}% snapshot loss",
            settings.network.simulated_latency_ms, settings.network.simulated_loss_percent
        );
    }

    // AppはBevyアプリケーション全体を組み立てる入口。
    App::new()
        .add_plugins(
            // サーバーでは画面、音声、ウィンドウが不要なのでMinimalPluginsを使う。
            // ScheduleRunnerPluginにより、ウィンドウのイベントループなしで動作する。
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / tick_rate,
            ))),
        )
        // FixedUpdateが設定したtick rateで進むよう、固定時間を登録する。
        .insert_resource(Time::<Fixed>::from_hz(tick_rate))
        .insert_resource(arena_map)
        .insert_resource(game_settings)
        .insert_resource(settings)
        .insert_resource(network)
        .insert_resource(control_plane)
        .insert_resource(control::SimulationControl::default())
        .insert_resource(control::AllocationState::default())
        .insert_resource(MatchState {
            reconnect_grace_seconds,
            room_settings,
            ..default()
        })
        // GameCoreは「1tick分の計算」、ServerRuntimeは「いつ呼ぶか」を担当する。
        .add_plugins((GameCorePlugin::new(tick_rate), ServerRuntimePlugin))
        // サーバーを終了するまでBevyの更新ループを実行する。
        .run();
}

fn command_line_value(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

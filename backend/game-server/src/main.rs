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
//! - `bind`: 待受ポートの決定（空きの探索）
//! - `config`: server.jsonと環境変数
//! - `network`: WebSocket通信
//! - `server_runtime`: 通信と実時間ランナーをGameTickへ接続

mod bind;
mod config;
mod control;
mod logging;
mod maps;
mod network;
mod server_runtime;

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, prelude::*, time::Fixed};
use pixel_shooter_game_core::{ArenaMap, GameCorePlugin, MatchState};

use crate::{config::ServerSettings, maps::MapCatalog, server_runtime::ServerRuntimePlugin};

fn main() {
    // 設定の読み込みより先に出力を切り替える。どの設定を読んだかもログへ残したい。
    if let Some(log_path) = command_line_value("--log-file")
        .or_else(|| std::env::var("PIXEL_SHOOTER_LOG_FILE").ok())
        .filter(|path| !path.is_empty())
    {
        match logging::redirect_to_file(std::path::Path::new(&log_path)) {
            Ok(()) => println!("--- Pixel Shooter server starting (log: {log_path}) ---"),
            // 切り替えに失敗しても起動は続ける。ログが残らないだけで遊べる。
            Err(error) => eprintln!("could not write to {log_path}: {error}"),
        }
    }

    let mut settings = ServerSettings::load();
    if let Some(bind_address) = command_line_value("--bind") {
        settings.network.bind_address = bind_address;
    }
    if let Some(debug_bind_address) = command_line_value("--debug-bind") {
        settings.control.bind_address = debug_bind_address;
    }
    let tick_rate = settings.network.tick_rate;
    let map_catalog = MapCatalog::load_from_environment();
    let arena_map: ArenaMap = map_catalog.default_map().clone();
    let game_settings = settings.game.clone();
    let reconnect_grace_seconds = game_settings.match_rules.reconnect_grace_seconds;
    let mut room_settings = game_settings.room_settings();
    room_settings.map_id = arena_map.id().into();
    let control_plane = control::start(&settings);
    // 待受できたことを確かめてから「listening」と表示する。
    let network = match network::start(&settings, control_plane.snapshot()) {
        Ok(network) => network,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    // 空きを探した結果、希望した番号と違うことがある。
    // 以降はどこも「実際に開けた方」を見るよう、設定を実態へ合わせる。
    let requested_address = std::mem::replace(
        &mut settings.network.bind_address,
        network.bind_address.clone(),
    );
    if requested_address != settings.network.bind_address {
        println!(
            "{requested_address} was busy; opened {} instead",
            settings.network.bind_address
        );
        // 公開先を他所へ知らせている場合、番号がずれると誰も繋がらなくなる。
        // 黙って動かすと原因の分からない接続失敗になるため、ここで言う。
        if let Some((_, requested_port)) = requested_address.rsplit_once(':')
            && settings
                .control
                .public_url
                .ends_with(&format!(":{requested_port}"))
        {
            eprintln!(
                "warning: public_url ({}) still points at the old port; \
                 set port_search_range to 0 when the port is published elsewhere",
                settings.control.public_url
            );
        }
    }
    if let Some(control_address) = &control_plane.bind_address {
        println!("GameServer control API listening on http://{control_address}");
    }

    // 実際の接続先をファイルへ残す。CREATE ROOM で起動した側は、
    // 番号を探し直さずにこれを読んで接続先を知る。
    if let Some(path) = command_line_value("--address-file").filter(|path| !path.is_empty())
        && let Err(error) = std::fs::write(&path, &settings.network.bind_address)
    {
        eprintln!("could not write {path}: {error}");
    }

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
        .insert_resource(map_catalog)
        .insert_resource(game_settings)
        .insert_resource(settings)
        .insert_resource(network)
        .insert_resource(control_plane)
        .insert_resource(control::SimulationControl::default())
        .insert_resource(control::DebugInputScenario::default())
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

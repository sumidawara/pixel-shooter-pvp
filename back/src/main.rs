//! Pixel Shooter PvP の権威サーバー。
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
//! - `arena`: マップ形状、衝突、スポーン地点
//! - `config`: server.jsonと環境変数
//! - `model`: ComponentとResource
//! - `network`: WebSocket通信
//! - `game`: 試合ルールとゲーム計算

mod arena;
mod config;
mod game;
mod model;
mod network;

use std::time::Duration;

use bevy::{app::ScheduleRunnerPlugin, prelude::*, time::Fixed};

use crate::{config::ServerSettings, model::MatchState};

fn main() {
    let settings = ServerSettings::load();
    let tick_rate = settings.network.tick_rate;
    let reconnect_grace_seconds = settings.match_rules.reconnect_grace_seconds;
    let network = network::start(&settings);

    println!(
        "Pixel Shooter server listening on ws://{}",
        settings.network.bind_address
    );
    println!(
        "Rules: {} second score match, kill +{}, death -{}, item +{}, {} HP, {} ammo",
        settings.match_rules.match_seconds,
        settings.match_rules.kill_points,
        settings.match_rules.death_penalty,
        settings.match_rules.item_points,
        settings.gameplay.max_hp,
        settings.gameplay.max_ammo
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
        .insert_resource(settings)
        .insert_resource(network)
        .insert_resource(MatchState {
            reconnect_grace_seconds,
            ..default()
        })
        .add_systems(
            FixedUpdate,
            (
                network::process_network,
                game::update_match,
                game::move_players,
                game::fire_bullets,
                game::move_and_hit_bullets,
                game::update_score_items,
                game::update_respawns,
                network::broadcast_snapshot,
            )
                // chain()により、入力→移動→射撃→判定→配信の順番を固定する。
                .chain(),
        )
        // サーバーを終了するまでBevyの更新ループを実行する。
        .run();
}

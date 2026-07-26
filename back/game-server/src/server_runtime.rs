//! 通信と実時間ランナーをGameCoreへ接続するGameServer固有の層。

use bevy::prelude::*;
use pixel_shooter_game_core::advance_one_tick;

use crate::network;

/// 実時間で動くサーバー処理をAppへ登録するPlugin。
pub(crate) struct ServerRuntimePlugin;

impl Plugin for ServerRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                // 通信はGameTickの外側に置く。将来ゲームを一時停止しても、
                // 接続、管理コマンド、Heartbeatを処理し続けられる。
                network::process_network,
                advance_one_tick,
                network::broadcast_snapshot,
            )
                .chain(),
        );
    }
}

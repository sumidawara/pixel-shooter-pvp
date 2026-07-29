//! 通信と実時間ランナーをGameCoreへ接続するGameServer固有の層。

use bevy::prelude::*;
use pixel_shooter_admin_protocol::SimulationMode;
use pixel_shooter_game_core::{PlayerInputOverrides, advance_one_tick};

use crate::{control, network};

/// 実時間で動くサーバー処理をAppへ登録するPlugin。
pub(crate) struct ServerRuntimePlugin;

impl Plugin for ServerRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            (
                // 通信はGameTickの外側に置く。将来ゲームを一時停止しても、
                // 接続、管理コマンド、Heartbeatを処理し続けられる。
                control::process_commands,
                network::process_network,
                drive_game_tick,
                network::broadcast_snapshot,
                control::publish_state,
            )
                .chain(),
        );
    }
}

fn drive_game_tick(world: &mut World) {
    let should_advance = {
        let mut simulation = world.resource_mut::<control::SimulationControl>();
        match simulation.mode {
            SimulationMode::Realtime => true,
            SimulationMode::Paused if simulation.pending_steps > 0 => {
                simulation.pending_steps -= 1;
                true
            }
            SimulationMode::Paused => false,
        }
    };
    if should_advance {
        let frame = world
            .resource_mut::<control::DebugInputScenario>()
            .take_next();
        let mut overrides = world.resource_mut::<PlayerInputOverrides>();
        if let Some(frame) = frame {
            overrides.replace(
                frame
                    .inputs
                    .into_iter()
                    .map(|command| (command.player_id, command.input)),
            );
        } else {
            overrides.clear();
        }
        advance_one_tick(world);
    }
}

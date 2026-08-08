//! 外部の訓練・デバッグ環境から1tickだけ上書きするプレイヤー入力。

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use pixel_shooter_protocol::PlayerInput;

use crate::model::Player;

#[derive(Clone, Copy)]
enum PendingActionPolicy {
    Preserve,
    Replace,
}

#[derive(Resource, Default)]
pub struct PlayerInputOverrides {
    inputs: HashMap<u64, PlayerInput>,
    released: HashSet<u64>,
}

impl PlayerInputOverrides {
    pub fn replace(&mut self, inputs: impl IntoIterator<Item = (u64, PlayerInput)>) {
        self.released = self.inputs.keys().copied().collect();
        self.inputs.clear();
        self.inputs.extend(inputs);
        self.released
            .retain(|player_id| !self.inputs.contains_key(player_id));
    }

    pub fn clear(&mut self) {
        self.released = self.inputs.keys().copied().collect();
        self.inputs.clear();
    }
}

/// WebSocketから受け取った入力をPlayerへ反映する。
///
/// PlayerInputを網羅的に分解する共通処理を通すため、操作フィールドを追加した際は
/// 通信入力とデバッグ注入の両方を同時に更新しない限りコンパイルが通らない。
pub fn apply_network_player_input(player: &mut Player, input: PlayerInput) {
    apply_player_input(player, input, PendingActionPolicy::Preserve);
}

fn apply_player_input(
    player: &mut Player,
    input: PlayerInput,
    pending_action_policy: PendingActionPolicy,
) {
    let PlayerInput {
        move_x,
        move_y,
        aim_x,
        aim_y,
        shooting,
        reload_pressed,
        dash_pressed,
        use_item_pressed,
    } = input;

    player.movement = Vec2::new(move_x, move_y).clamp_length_max(1.0);
    let aim = Vec2::new(aim_x, aim_y);
    if aim.length_squared() > 0.001 {
        player.aim = aim.normalize();
    }
    player.shooting = shooting;
    match pending_action_policy {
        PendingActionPolicy::Preserve => {
            // 押した瞬間だけtrueになる操作は、Systemで消費するまでORで保持する。
            player.reload_requested |= reload_pressed;
            player.dash_requested |= dash_pressed;
            player.use_item_requested |= use_item_pressed;
        }
        PendingActionPolicy::Replace => {
            // 注入入力はCPU入力を含むそのtickの操作を完全に上書きする。
            player.reload_requested = reload_pressed;
            player.dash_requested = dash_pressed;
            player.use_item_requested = use_item_pressed;
        }
    }
}

pub(crate) fn apply_player_input_overrides(
    overrides: Res<PlayerInputOverrides>,
    mut players: Query<&mut Player>,
) {
    for mut player in &mut players {
        if overrides.released.contains(&player.id) {
            player.movement = Vec2::ZERO;
            player.shooting = false;
            player.reload_requested = false;
            player.dash_requested = false;
            player.use_item_requested = false;
        }
        let Some(input) = overrides.inputs.get(&player.id) else {
            continue;
        };
        apply_player_input(&mut player, *input, PendingActionPolicy::Replace);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_player(id: u64) -> Player {
        Player {
            id,
            connection_id: None,
            is_cpu: true,
            is_dummy: false,
            reconnect_token: String::new(),
            reconnect_grace_left: 0.0,
            slot: 0,
            name: "CPU".into(),
            position: Vec2::ZERO,
            aim: Vec2::X,
            movement: Vec2::ZERO,
            shooting: false,
            hp: 5,
            score: 0,
            alive: true,
            respawn_left: 0.0,
            shot_cooldown: 0.0,
            ammo: 6,
            reload_left: 0.0,
            reload_requested: false,
            invulnerable_left: 0.0,
            dash_cooldown_left: 0.0,
            dash_time_left: 0.0,
            dash_direction: Vec2::ZERO,
            dash_requested: false,
            use_item_requested: false,
            held_item: None,
            berserk_left: 0.0,
            shield_hp: 0,
            last_input_sequence: 0,
        }
    }

    #[test]
    fn injected_action_is_clamped_applied_and_released() {
        let mut app = App::new();
        let mut overrides = PlayerInputOverrides::default();
        overrides.replace([(
            7,
            PlayerInput {
                move_x: 2.0,
                aim_y: 4.0,
                shooting: true,
                dash_pressed: true,
                use_item_pressed: true,
                ..default()
            },
        )]);
        app.insert_resource(overrides)
            .add_systems(Update, apply_player_input_overrides)
            .world_mut()
            .spawn(test_player(7));

        app.update();
        {
            let world = app.world_mut();
            let mut players = world.query::<&Player>();
            let player = players.single(world).unwrap();
            assert_eq!(player.movement, Vec2::X);
            assert_eq!(player.aim, Vec2::Y);
            assert!(player.shooting);
            assert!(player.dash_requested);
            assert!(player.use_item_requested);
        }

        app.world_mut()
            .resource_mut::<PlayerInputOverrides>()
            .clear();
        app.update();
        {
            let world = app.world_mut();
            let mut players = world.query::<&Player>();
            let player = players.single(world).unwrap();
            assert_eq!(player.movement, Vec2::ZERO);
            assert!(!player.shooting);
            assert!(!player.dash_requested);
            assert!(!player.use_item_requested);
        }
    }
}

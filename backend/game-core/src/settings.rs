//! GameCoreだけが参照する試合ルールと操作パラメーター。

use bevy::prelude::Resource;
use pixel_shooter_protocol::RoomSettings;
use serde::Deserialize;

use crate::cpu_skill::{CpuLevel, CpuSettings};

#[derive(Resource, Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    #[serde(rename = "match")]
    pub match_rules: MatchRules,
    pub gameplay: GameplaySettings,
    /// アイテムの効果。
    pub items: ItemSettings,
    /// 練習場（サンドボックス）の手触り。
    pub sandbox: SandboxSettings,
    /// CPUの強さ。段階ごとの数値を上書きできる。
    pub cpu: CpuSettings,
}

/// アイテムを使ったときに起きることの数値。
///
/// ここをRustの`const`ではなく設定に置いているのは、バランス調整でいちばん
/// よく触る類だから。組み立て直さずに試せる方がよい。
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct ItemSettings {
    /// バーサクの効果時間(秒)。
    pub berserk_seconds: f32,
    /// バーサク中に弾が速くなる倍率。
    pub berserk_bullet_speed_multiplier: f32,
    /// ラロキンポッポスが一度に出る数。
    pub larokin_count: usize,
    /// ラロキンポッポスの速さ(px/秒)。
    pub larokin_speed: f32,
    /// 突撃を始めるまでの溜め(秒)。避ける余地を作るための間。
    pub larokin_telegraph_seconds: f32,
    /// ラロキンポッポスの当たり判定の半径(px)。
    pub larokin_radius: f32,
    /// ラロキンポッポス1体あたりのダメージ。
    pub larokin_damage: i32,
    /// ゴーストが飛んで戻るまでの時間(秒)。
    ///
    /// 奪取そのものは使用したtickで確定しており、これは見せている時間だけを表す。
    pub ghost_thief_seconds: f32,
}

/// 練習場の手触り。
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct SandboxSettings {
    /// 取られたアイテムが戻ってくるまでの時間(秒)。
    pub item_restock_seconds: f32,
    /// 的が倒れてから起き上がるまでの時間(秒)。
    pub dummy_respawn_seconds: f32,
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
            cpu_level: CpuLevel::default().number(),
        }
    }

    pub fn sanitize_room_settings(&self, mut room: RoomSettings) -> RoomSettings {
        room.match_seconds = room.match_seconds.clamp(30.0, 900.0);
        room.kill_points = room.kill_points.clamp(0, 10_000);
        room.death_penalty = room.death_penalty.clamp(0, 10_000);
        room.item_points = room.item_points.clamp(0, 10_000);
        room.item_spawn_interval = room.item_spawn_interval.clamp(0.5, 60.0);
        room.max_items = room.max_items.clamp(1, 16);
        // 段階は1〜4。範囲外は近い方へ寄せて返す。
        room.cpu_level = CpuLevel::from_number(room.cpu_level).number();
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
        self.items.berserk_seconds = self.items.berserk_seconds.max(0.0);
        self.items.berserk_bullet_speed_multiplier =
            self.items.berserk_bullet_speed_multiplier.clamp(0.1, 10.0);
        // 0にすると使っても何も起きない。上限は1回の使用で場が埋まらない程度。
        self.items.larokin_count = self.items.larokin_count.clamp(1, 64);
        self.items.larokin_speed = self.items.larokin_speed.max(1.0);
        self.items.larokin_telegraph_seconds = self.items.larokin_telegraph_seconds.max(0.0);
        self.items.larokin_radius = self.items.larokin_radius.max(1.0);
        self.items.larokin_damage = self.items.larokin_damage.max(0);
        // 0にすると演出が1tickも見えないまま消える。
        self.items.ghost_thief_seconds = self.items.ghost_thief_seconds.max(0.05);
        self.sandbox.item_restock_seconds = self.sandbox.item_restock_seconds.max(0.05);
        self.sandbox.dummy_respawn_seconds = self.sandbox.dummy_respawn_seconds.max(0.05);
        self.cpu.sanitize();
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

impl Default for ItemSettings {
    fn default() -> Self {
        Self {
            berserk_seconds: 3.0,
            berserk_bullet_speed_multiplier: 1.3,
            larokin_count: 10,
            larokin_speed: 230.0,
            larokin_telegraph_seconds: 0.7,
            larokin_radius: 8.0,
            larokin_damage: 1,
            ghost_thief_seconds: 0.9,
        }
    }
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            item_restock_seconds: 1.0,
            dummy_respawn_seconds: 1.0,
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

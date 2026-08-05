//! GameCoreのBevy Worldに保存するComponentとResource。

use bevy::prelude::*;
use pixel_shooter_protocol::{ItemKind, MatchPhase, RoomSettings};

pub const MAX_PLAYERS: usize = 4;

/// 試合全体で1つだけ存在する状態。
///
/// プレイヤーごとのデータではないのでComponentではなくResourceにしている。
#[derive(Resource, Default)]
pub struct MatchState {
    /// サーバーが何回固定更新を実行したか。
    pub tick: u64,
    pub phase: MatchPhase,
    /// 現在のフェーズの残り時間。Runningでは試合残り時間になる。
    pub phase_time_left: f32,
    /// 切断による一時停止から戻るフェーズ。
    pub resume_phase: Option<MatchPhase>,
    pub match_winner_id: Option<u64>,
    pub next_bullet_id: u64,
    pub next_item_id: u64,
    pub next_larokin_id: u64,
    pub item_spawn_left: f32,
    pub next_player_id: u64,
    pub reconnect_grace_seconds: f32,
    pub host_player_id: Option<u64>,
    pub start_requested: bool,
    pub room_settings: RoomSettings,
}

/// プレイヤーEntityに付けるComponent。
///
/// Bevyでは継承を使った「Playerクラス」を作る代わりに、
/// Entityへ必要なComponentを付けてゲームオブジェクトを表現する。
#[derive(Component)]
pub struct Player {
    /// 試合中変わらないプレイヤーID。WebSocketの接続IDとは別。
    pub id: u64,
    /// 現在このプレイヤーを操作しているWebSocket接続。
    pub connection_id: Option<u64>,
    /// trueなら通信接続を持たず、サーバーのAI Systemが操作する。
    pub is_cpu: bool,
    pub reconnect_token: String,
    pub reconnect_grace_left: f32,
    pub slot: usize,
    pub name: String,
    pub position: Vec2,
    pub aim: Vec2,
    pub movement: Vec2,
    pub shooting: bool,
    pub hp: i32,
    pub score: i32,
    pub alive: bool,
    pub respawn_left: f32,
    pub shot_cooldown: f32,
    pub ammo: u32,
    pub reload_left: f32,
    pub reload_requested: bool,
    pub invulnerable_left: f32,
    pub dash_cooldown_left: f32,
    pub dash_time_left: f32,
    pub dash_direction: Vec2,
    pub dash_requested: bool,
    pub use_item_requested: bool,
    pub held_item: Option<HeldItem>,
    pub berserk_left: f32,
    pub shield_hp: i32,
    pub last_input_sequence: u32,
}

/// 発射された弾Entityに付けるComponent。
#[derive(Component)]
pub struct Bullet {
    pub id: u64,
    pub owner_id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub life_left: f32,
    pub damage: i32,
}

/// アリーナに出現し、触れたプレイヤーへ得点を与えるアイテム。
#[derive(Component)]
pub struct ScoreItem {
    pub id: u64,
    pub position: Vec2,
    pub kind: ItemKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldItem {
    pub kind: ItemKind,
    pub charges: u32,
}

/// ラロキンポッポス使用時にアリーナ端から突撃する攻撃体。
#[derive(Component)]
pub struct LarokinPoppos {
    pub id: u64,
    pub owner_id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub telegraph_left: f32,
    pub life_left: f32,
}

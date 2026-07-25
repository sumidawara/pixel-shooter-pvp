//! Bevyのゲーム世界に保存するComponentとResource。

use bevy::prelude::*;
use pixel_shooter_protocol::{MatchPhase, RoomSettings};

pub(crate) const MAX_PLAYERS: usize = 4;

/// 試合全体で1つだけ存在する状態。
///
/// プレイヤーごとのデータではないのでComponentではなくResourceにしている。
#[derive(Resource, Default)]
pub(crate) struct MatchState {
    /// サーバーが何回固定更新を実行したか。
    pub(crate) tick: u64,
    pub(crate) phase: MatchPhase,
    /// 現在のフェーズの残り時間。Runningでは試合残り時間になる。
    pub(crate) phase_time_left: f32,
    /// 切断による一時停止から戻るフェーズ。
    pub(crate) resume_phase: Option<MatchPhase>,
    pub(crate) match_winner_id: Option<u64>,
    pub(crate) next_bullet_id: u64,
    pub(crate) next_item_id: u64,
    pub(crate) item_spawn_left: f32,
    pub(crate) next_player_id: u64,
    pub(crate) reconnect_grace_seconds: f32,
    pub(crate) host_player_id: Option<u64>,
    pub(crate) start_requested: bool,
    pub(crate) room_settings: RoomSettings,
}

/// プレイヤーEntityに付けるComponent。
///
/// Bevyでは継承を使った「Playerクラス」を作る代わりに、
/// Entityへ必要なComponentを付けてゲームオブジェクトを表現する。
#[derive(Component)]
pub(crate) struct Player {
    /// 試合中変わらないプレイヤーID。WebSocketの接続IDとは別。
    pub(crate) id: u64,
    /// 現在このプレイヤーを操作しているWebSocket接続。
    pub(crate) connection_id: Option<u64>,
    /// trueなら通信接続を持たず、サーバーのAI Systemが操作する。
    pub(crate) is_cpu: bool,
    pub(crate) reconnect_token: String,
    pub(crate) reconnect_grace_left: f32,
    pub(crate) slot: usize,
    pub(crate) name: String,
    pub(crate) position: Vec2,
    pub(crate) aim: Vec2,
    pub(crate) movement: Vec2,
    pub(crate) shooting: bool,
    pub(crate) hp: i32,
    pub(crate) score: i32,
    pub(crate) alive: bool,
    pub(crate) respawn_left: f32,
    pub(crate) shot_cooldown: f32,
    pub(crate) ammo: u32,
    pub(crate) reload_left: f32,
    pub(crate) reload_requested: bool,
    pub(crate) invulnerable_left: f32,
    pub(crate) dash_cooldown_left: f32,
    pub(crate) dash_time_left: f32,
    pub(crate) dash_direction: Vec2,
    pub(crate) dash_requested: bool,
    pub(crate) last_input_sequence: u32,
}

/// 発射された弾Entityに付けるComponent。
#[derive(Component)]
pub(crate) struct Bullet {
    pub(crate) id: u64,
    pub(crate) owner_id: u64,
    pub(crate) position: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) life_left: f32,
}

/// アリーナに出現し、触れたプレイヤーへ得点を与えるアイテム。
#[derive(Component)]
pub(crate) struct ScoreItem {
    pub(crate) id: u64,
    pub(crate) position: Vec2,
}

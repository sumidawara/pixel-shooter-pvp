//! Pixel Shooterの通信や実時間ランナーに依存しないゲームシミュレーション。

mod arena;
mod game;
mod input;
mod model;
mod schedule;
mod settings;

pub use arena::{ArenaMap, GridPosition, MapLoadError, RANDOM_MAP_ID, TileKind};
pub use input::{PlayerInputOverrides, apply_network_player_input};
pub use model::{
    Bullet, GhostThief, HeldItem, LarokinPoppos, MAX_PLAYERS, MatchState, Player, ScoreItem,
};
pub use schedule::{GameClock, GameCorePlugin, GameTick, advance_one_tick};
pub use settings::{GameSettings, GameplaySettings, MatchRules};

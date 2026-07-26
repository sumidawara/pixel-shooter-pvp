//! Pixel Shooterの通信や実時間ランナーに依存しないゲームシミュレーション。

mod arena;
mod game;
mod model;
mod schedule;
mod settings;

pub use arena::{ArenaMap, GridPosition, MapLoadError, TileKind};
pub use model::{Bullet, MAX_PLAYERS, MatchState, Player, ScoreItem};
pub use schedule::{GameClock, GameCorePlugin, GameTick, advance_one_tick};
pub use settings::{GameSettings, GameplaySettings, MatchRules};

//! Pure simulation. No `std::io`, no `std::time`, no `ratatui` — time arrives only as a `Tick`.

pub mod entities;
pub mod rng;
pub mod state;
pub mod tuning;
pub mod world;

pub mod ai;
pub mod combat;

pub use entities::{AiState, Enemy, EnemyId, Facing, Hero, Pos, Swing};
pub use rng::Rng;
pub use state::{state_hash, update, Action, GameEvent, GameState, Progress, Tick};
pub use world::{
    Chest, Door, EnemyKind, EnemySpawn, LockKind, Npc, Puzzle, PuzzleKind, Reward, Room, RoomIdx,
    RoomKind, Route, Spawn, StartPoint, Tile, TileGrid, World,
};

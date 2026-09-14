//! Pure simulation. No `std::io`, no `std::time`, no `ratatui` — time arrives only as a `Tick`.

pub mod entities;
pub mod rng;
pub mod state;
pub mod tuning;
pub mod world;

pub mod ai;
pub mod combat;
pub mod puzzles;

pub use entities::{AiState, DialogueState, Enemy, EnemyId, Facing, Hero, ObjectRef, Pos, Swing};
pub use puzzles::PlateState;
pub use rng::Rng;
pub use state::{state_hash, update, Action, GameEvent, GameState, Progress, Tick};
pub use world::{
    Chest, DialogueNode, Door, EnemyKind, EnemySpawn, LockKind, Npc, ObjectKind, Plate, Puzzle,
    PuzzleKind, Reward, Room, RoomIdx, RoomKind, Route, Spawn, StartPoint, Tile, TileGrid, Torch,
    World,
};

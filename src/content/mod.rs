//! RON content schema, `include_str!` loader and the §7 validator (spec §7, §8).
//!
//! Pure aside from `loader::EMBEDDED`'s compile-time `include_str!`: no runtime `std::io` here.
//! `main.rs`/`config.rs` own the one exception, the `--debug-content PATH` file read.

pub mod error;
pub mod loader;
pub mod schema;
pub mod validate;

pub use error::{report, ContentError, IdKind};
pub use loader::{load, parse, EMBEDDED};
pub use schema::{
    Chest, Door, EnemyKind, EnemySpawn, LockKind, Npc, Puzzle, PuzzleKind, Reward, Room, RoomIdx,
    RoomKind, Route, Spawn, StartPoint, Tile, TileGrid, World,
};
pub use validate::validate;

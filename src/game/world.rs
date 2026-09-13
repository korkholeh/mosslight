//! Re-exports the content module's tile/room/world types so `crate::game::{Tile, Room, ...}`
//! keep resolving. The world itself is authored data (`assets/world.ron`), not code — see
//! `src/content/`.

pub use crate::content::{
    Chest, Door, EnemyKind, EnemySpawn, LockKind, Npc, Puzzle, PuzzleKind, Reward, Room, RoomIdx,
    RoomKind, Route, Spawn, StartPoint, Tile, TileGrid, World,
};

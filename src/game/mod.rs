//! Pure simulation. No `std::io`, no `std::time`, no `ratatui` — time arrives only as a `Tick`.

pub mod entities;
pub mod rng;
pub mod state;
pub mod tuning;
pub mod world;

pub use entities::{Facing, Hero, Pos};
pub use rng::Rng;
pub use state::{update, Action, GameEvent, GameState, Tick};
pub use world::{debug_room, Room, Tile};

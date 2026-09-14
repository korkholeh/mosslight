//! Mosslight — a single-player, top-down terminal adventure.
//!
//! `game` and `render` are pure: no terminal, no clock, no I/O. `main.rs` is the only place that
//! owns a terminal.

pub mod app;
pub mod config;
pub mod content;
pub mod game;
pub mod input;
pub mod render;
pub mod save;
pub mod terminal;

//! Hero and shared position/facing types.

use super::state::Tick;
use super::tuning::HERO_STEP_TICKS;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Pos {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Facing {
    North,
    East,
    South,
    West,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Hero {
    pub pos: Pos,
    pub facing: Facing,
    pub step_ready_at: Tick,
    pub health_halves: u8,
    pub max_health_halves: u8,
    pub keys: u8,
}

impl Hero {
    pub fn at_spawn(pos: Pos) -> Self {
        Hero {
            pos,
            facing: Facing::South,
            step_ready_at: 0,
            health_halves: 6,
            max_health_halves: 6,
            keys: 0,
        }
    }

    pub fn can_step(&self, tick: Tick) -> bool {
        tick >= self.step_ready_at
    }

    pub fn mark_stepped(&mut self, tick: Tick) {
        self.step_ready_at = tick + HERO_STEP_TICKS;
    }
}

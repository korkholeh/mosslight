//! Every combat/movement constant, expressed in ticks (spec §6).
//!
//! `pub` so these are provably used across phases without `#[allow(dead_code)]`.

/// Simulation frequency. Fixed regardless of `--fps` (spec §9).
pub const TICK_HZ: u64 = 30;

/// Hero step interval: 4 ticks at 30 Hz = 133 ms. The spec's 120 ms is not a multiple of the
/// 33.3 ms tick; rounded once here rather than at each use site (see DECISIONS.md).
pub const HERO_STEP_TICKS: u64 = 4;

/// Sword cooldown: 9 ticks = 300 ms.
pub const SWORD_COOLDOWN_TICKS: u64 = 9;

/// Sword active phase: 3 ticks = 100 ms.
pub const SWORD_ACTIVE_TICKS: u64 = 3;

/// Invulnerability window after taking damage: 24 ticks = 800 ms.
pub const INVULN_TICKS: u64 = 24;

/// Minimum telegraph time before a dangerous attack: 18 ticks = 600 ms.
pub const TELEGRAPH_MIN_TICKS: u64 = 18;

/// Maximum catch-up simulation steps per loop iteration after a stall (spec §9).
pub const MAX_CATCHUP_STEPS: u32 = 5;

/// Maximum input events read per loop iteration.
pub const INPUT_EVENTS_PER_ITER: usize = 32;

/// Livelock guard on the number of raw terminal events drained in one iteration. The real
/// overflow policy (coalescing, the control-action retain limit) runs on the mapped actions via
/// `apply_overflow_policy`; this cap only stops a pathological event flood from blocking the loop
/// forever, so it is set far above anything a real burst produces.
pub const INPUT_EVENT_HARD_CAP: usize = 4096;

/// Room width in tiles.
pub const ROOM_W: usize = 24;

/// Room height in tiles.
pub const ROOM_H: usize = 16;

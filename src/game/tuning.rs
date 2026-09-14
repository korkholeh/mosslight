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

/// Sword damage dealt to an enemy per hit.
pub const SWORD_DAMAGE_HP: u8 = 1;

/// Contact damage dealt to the hero, in half-hearts.
pub const CONTACT_DAMAGE_HALVES: u8 = 1;

/// Knockback distance on hero contact damage, in tiles.
pub const KNOCKBACK_TILES: u8 = 1;

/// Slime hit points.
pub const SLIME_HP: u8 = 2;
/// Slime movement interval: 12 ticks = 400 ms.
pub const SLIME_STEP_TICKS: u64 = 12;
/// Manhattan radius at which an idle slime notices the hero.
pub const SLIME_AGGRO_RADIUS: i32 = 5;
/// Idle phase length: 60 ticks = 2000 ms.
pub const SLIME_IDLE_TICKS: u64 = 60;
/// Chase phase length: 90 ticks = 3000 ms.
pub const SLIME_CHASE_TICKS: u64 = 90;

/// Bat hit points.
pub const BAT_HP: u8 = 1;
/// Bat movement interval: 6 ticks = 200 ms, faster than the slime.
pub const BAT_STEP_TICKS: u64 = 6;
/// Steps taken per dart.
pub const BAT_DART_STEPS: u8 = 4;
/// Rest phase length: 30 ticks = 1000 ms.
pub const BAT_REST_TICKS: u64 = 30;
/// Manhattan radius at which a dart aims at the hero instead of a random direction.
pub const BAT_AGGRO_RADIUS: i32 = 6;

/// Guardian hit points.
pub const GUARDIAN_HP: u8 = 4;
/// Manhattan sight distance along the facing axis that triggers a telegraph.
pub const GUARDIAN_SIGHT: i32 = 8;
/// Unconditional patrol-to-telegraph timeout: 150 ticks = 5000 ms — guarantees the cycle never
/// stalls even in an empty maze with the hero out of sight.
pub const GUARDIAN_PATROL_TICKS: u64 = 150;
/// Patrol movement interval: 10 ticks = 333 ms.
pub const GUARDIAN_PATROL_STEP_TICKS: u64 = 10;
/// Telegraph hold length. Fixed to the §6 floor so it cannot drift out of sync.
pub const GUARDIAN_TELEGRAPH_TICKS: u64 = TELEGRAPH_MIN_TICKS;
/// Dash distance, in tiles.
pub const GUARDIAN_DASH_TILES: u8 = 5;
/// Dash movement interval: 3 ticks = 100 ms, much faster than patrol.
pub const GUARDIAN_DASH_STEP_TICKS: u64 = 3;
/// Recovery hold length (the only window in which the guardian takes damage): 30 ticks = 1000 ms.
pub const GUARDIAN_RECOVER_TICKS: u64 = 30;

/// Upper bound on ticks between two consecutive `AiState` changes for any enemy kind, given the
/// constants above (guardian's worst case: patrol timeout + telegraph + dash + recover). Used by
/// `tests/ai.rs` to assert liveness.
pub const MAX_STALL_TICKS: u64 = 300;

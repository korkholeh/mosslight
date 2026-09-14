//! An itemised estimate of a first-time player's playthrough length (spec §1's 30-45 minute
//! target), derived from `tuning`'s simulation constants and the authored `World` rather than a
//! single fudge factor on the headless route's tick count. Pure: no `std::io`, no `std::time`, no
//! `ratatui`. Never called by `update` — this cannot affect simulation, only measure it.
//!
//! The headless route in `tests/common/route.rs` is a *perfect* run: it knows every door, fights
//! nothing optional, never backtracks, never dies. A first-time player's run is not that number
//! times a single constant chosen to land in the band — that would be circular. Instead every term
//! below is derived from something the world or `tuning` actually contains, so a balance change
//! (enemy count, boss HP, step interval) moves the estimate.

use super::state::Tick;
use super::tuning::*;
use super::world::{EnemyKind, World};

/// A first-time player revisits roughly this many times the tiles the optimal route touches,
/// backtracking to rooms the perfect run skips.
const REVISIT_FACTOR: Tick = 3;

/// Sub-optimal in-room pathing versus the BFS-optimal route: 5/2 as a ticks numerator/denominator
/// pair, so the run-length model has no float in it.
const PATHING_FACTOR_NUM: Tick = 5;
const PATHING_FACTOR_DEN: Tick = 2;

/// Reading the HUD, hesitating at a junction, admiring a new room — time that is not pathing and
/// not dialogue, but is not zero either.
const PAUSE_FACTOR: Tick = 3;

/// Ticks a first-time player spends reading one dialogue node before pressing Confirm: 8 s.
const DIALOGUE_READ_TICKS: Tick = 240;

/// Ticks a first-time player spends working out one puzzle's mechanic before solving it: 60 s.
const PUZZLE_THINK_TICKS: Tick = 1800;

/// Ticks spent closing the distance to a regular enemy before the first swing: 3 s.
const ENGAGE_APPROACH_TICKS: Tick = 90;

/// Extra ticks per required hit accounting for a first-time player's whiffed swings: one full
/// `SWORD_COOLDOWN_TICKS`-equivalent miss, on average, per hit landed.
const ENGAGE_MISS_TICKS: Tick = 18;

/// How many times a first-time player is assumed to die to the boss and retry.
const BOSS_DEATHS_ASSUMED: Tick = 2;

/// Ticks to walk from the post-death checkpoint back to the boss arena: 10 s.
const BOSS_RETURN_TICKS: Tick = 300;

/// Title screen, New Game confirmation, the victory screen: 20 s total.
const MENU_TICKS: Tick = 600;

/// The §1 target band, in ticks, so the test and the docs cannot disagree about it.
pub const TARGET_MIN_TICKS: Tick = 30 * 60 * TICK_HZ;
pub const TARGET_MAX_TICKS: Tick = 45 * 60 * TICK_HZ;

/// Per-component breakdown of an estimated first playthrough, in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunEstimate {
    pub travel_ticks: Tick,
    pub dialogue_ticks: Tick,
    pub combat_ticks: Tick,
    pub puzzle_ticks: Tick,
    pub boss_retry_ticks: Tick,
    pub menu_ticks: Tick,
    pub total_ticks: Tick,
}

impl RunEstimate {
    pub fn total_seconds(&self) -> u64 {
        self.total_ticks / TICK_HZ
    }

    pub fn total_minutes(&self) -> u64 {
        self.total_seconds() / 60
    }
}

/// Ticks a first-time player spends on one enemy of `kind`: closing the distance, the hits its HP
/// needs at `SWORD_COOLDOWN_TICKS`, the misses, and — for the guardian — waiting out
/// telegraph + dash + recovery for each hit, since recovery is its only vulnerable window.
/// `EnemyKind::Boss` is never a regular spawn (see `boss_fight_ticks`); it returns `0` rather than
/// panicking, so this stays a safe function to call with any `EnemyKind`.
pub fn engage_ticks(kind: EnemyKind) -> Tick {
    match kind {
        EnemyKind::Slime => {
            ENGAGE_APPROACH_TICKS + SLIME_HP as Tick * (SWORD_COOLDOWN_TICKS + ENGAGE_MISS_TICKS)
        }
        EnemyKind::Bat => {
            ENGAGE_APPROACH_TICKS + BAT_HP as Tick * (SWORD_COOLDOWN_TICKS + ENGAGE_MISS_TICKS)
        }
        EnemyKind::Guardian => {
            let per_hit = GUARDIAN_TELEGRAPH_TICKS
                + GUARDIAN_DASH_TILES as Tick * GUARDIAN_DASH_STEP_TICKS
                + GUARDIAN_RECOVER_TICKS;
            ENGAGE_APPROACH_TICKS + GUARDIAN_HP as Tick * per_hit
        }
        EnemyKind::Boss => 0,
    }
}

/// Ticks one full boss fight takes at the tuned `BOSS_*` constants: for each of `BOSS_HP` hits,
/// the phase's stalk + windup ticks plus the vulnerable window itself (assumed spent landing the
/// hit, the same way `engage_ticks`'s guardian case counts the whole recovery window), phase one
/// for the first `BOSS_HP - BOSS_PHASE_TWO_HP` hits and phase two for the rest. This is why
/// `BOSS_HP` and the phase-two vulnerable window are real levers on the estimate (see DECISIONS.md,
/// phase 7 T4).
pub fn boss_fight_ticks() -> Tick {
    let phase_one_hits = (BOSS_HP - BOSS_PHASE_TWO_HP) as Tick;
    let phase_two_hits = BOSS_PHASE_TWO_HP as Tick;
    let phase_one =
        phase_one_hits * (BOSS_P1_STALK_TICKS + BOSS_P1_WINDUP_TICKS + BOSS_P1_VULNERABLE_TICKS);
    let phase_two =
        phase_two_hits * (BOSS_P2_STALK_TICKS + BOSS_P2_WINDUP_TICKS + BOSS_P2_VULNERABLE_TICKS);
    phase_one + phase_two
}

fn dialogue_node_count(world: &World) -> Tick {
    world
        .rooms
        .iter()
        .flat_map(|r| r.npcs.iter())
        .map(|npc| npc.dialogue.len() as Tick)
        .sum()
}

fn puzzle_count(world: &World) -> Tick {
    world.rooms.iter().map(|r| r.puzzles.len() as Tick).sum()
}

fn optional_combat_ticks(world: &World) -> Tick {
    world
        .rooms
        .iter()
        .flat_map(|r| r.enemies.iter())
        .filter(|spawn| spawn.kind != EnemyKind::Boss)
        .map(|spawn| engage_ticks(spawn.kind))
        .sum()
}

/// The whole estimate. `optimal_ticks` is what the headless route actually took; everything else
/// is counted out of `world`.
pub fn estimate_first_playthrough(world: &World, optimal_ticks: Tick) -> RunEstimate {
    let travel_ticks =
        optimal_ticks * REVISIT_FACTOR * PATHING_FACTOR_NUM * PAUSE_FACTOR / PATHING_FACTOR_DEN;
    let dialogue_ticks = dialogue_node_count(world) * DIALOGUE_READ_TICKS;
    let combat_ticks = optional_combat_ticks(world);
    let puzzle_ticks = puzzle_count(world) * PUZZLE_THINK_TICKS;
    let boss_retry_ticks = BOSS_DEATHS_ASSUMED * (boss_fight_ticks() + BOSS_RETURN_TICKS);
    let menu_ticks = MENU_TICKS;

    let total_ticks =
        travel_ticks + dialogue_ticks + combat_ticks + puzzle_ticks + boss_retry_ticks + menu_ticks;

    RunEstimate {
        travel_ticks,
        dialogue_ticks,
        combat_ticks,
        puzzle_ticks,
        boss_retry_ticks,
        menu_ticks,
        total_ticks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        crate::content::load().expect("embedded world validates")
    }

    #[test]
    fn boss_fight_ticks_matches_the_tuned_constants() {
        let phase_one_hits = (BOSS_HP - BOSS_PHASE_TWO_HP) as Tick;
        let phase_two_hits = BOSS_PHASE_TWO_HP as Tick;
        let expected = phase_one_hits
            * (BOSS_P1_STALK_TICKS + BOSS_P1_WINDUP_TICKS + BOSS_P1_VULNERABLE_TICKS)
            + phase_two_hits
                * (BOSS_P2_STALK_TICKS + BOSS_P2_WINDUP_TICKS + BOSS_P2_VULNERABLE_TICKS);
        assert_eq!(boss_fight_ticks(), expected);
    }

    #[test]
    fn engage_ticks_is_strictly_ordered_bat_then_slime_then_guardian() {
        let bat = engage_ticks(EnemyKind::Bat);
        let slime = engage_ticks(EnemyKind::Slime);
        let guardian = engage_ticks(EnemyKind::Guardian);
        assert!(
            bat < slime,
            "bat {bat} should engage faster than slime {slime}"
        );
        assert!(
            slime < guardian,
            "slime {slime} should engage faster than guardian {guardian}"
        );
    }

    #[test]
    fn engage_ticks_of_boss_is_zero_not_a_panic() {
        assert_eq!(engage_ticks(EnemyKind::Boss), 0);
    }

    #[test]
    fn every_component_is_nonzero_for_the_embedded_world() {
        let world = world();
        let estimate = estimate_first_playthrough(&world, 1955);
        assert!(estimate.travel_ticks > 0);
        assert!(
            estimate.dialogue_ticks > 0,
            "the embedded world authors dialogue"
        );
        assert!(
            estimate.combat_ticks > 0,
            "the embedded world authors regular enemies"
        );
        assert!(
            estimate.puzzle_ticks > 0,
            "the embedded world authors puzzles"
        );
        assert!(estimate.boss_retry_ticks > 0);
        assert!(estimate.menu_ticks > 0);
    }

    #[test]
    fn total_ticks_is_the_sum_of_its_parts() {
        let world = world();
        let estimate = estimate_first_playthrough(&world, 1955);
        assert_eq!(
            estimate.total_ticks,
            estimate.travel_ticks
                + estimate.dialogue_ticks
                + estimate.combat_ticks
                + estimate.puzzle_ticks
                + estimate.boss_retry_ticks
                + estimate.menu_ticks
        );
    }
}

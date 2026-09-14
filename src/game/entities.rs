//! Hero, enemy and shared position/facing types.

use super::state::Tick;
use super::tuning::HERO_STEP_TICKS;
use super::world::EnemyKind;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Pos {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    North,
    East,
    South,
    West,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hero {
    pub pos: Pos,
    pub facing: Facing,
    pub step_ready_at: Tick,
    pub health_halves: u8,
    pub max_health_halves: u8,
    pub keys: u8,
    pub attack: Option<Swing>,
    pub attack_ready_at: Tick,
    pub invuln_until: Tick,
    /// Latches `true` the first tick `update` observes zero health, so `GameEvent::HeroDied` is
    /// emitted exactly once (round-2 review, nit) even though `health_halves` itself stays `0`
    /// (nothing heals it back up this phase) across every subsequent tick a caller still ticks —
    /// a headless test, or a future replay tool.
    pub died: bool,
    pub has_sword: bool,
    pub has_lantern: bool,
    /// Written in phase 5, hashed and rendered from here so the inventory screen does not change
    /// shape later.
    pub has_ember: bool,
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
            attack: None,
            attack_ready_at: 0,
            invuln_until: 0,
            died: false,
            has_sword: false,
            has_lantern: false,
            has_ember: false,
        }
    }

    pub fn can_step(&self, tick: Tick) -> bool {
        tick >= self.step_ready_at
    }

    pub fn mark_stepped(&mut self, tick: Tick) {
        self.step_ready_at = tick + HERO_STEP_TICKS;
    }

    pub fn can_attack(&self, tick: Tick) -> bool {
        tick >= self.attack_ready_at
    }

    pub fn is_invulnerable(&self, tick: Tick) -> bool {
        tick < self.invuln_until
    }
}

/// A stable index into `GameState::enemies`. Dead enemies stay in the vec (as `alive: false`) so
/// this index never dangles within a room's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnemyId(pub u16);

/// A room-local authored object: `(room, index into that room's vec)`. Dense and `Ord`, so
/// `BTreeSet` iteration stays deterministic for hashing. Not `Serialize` for the same reason
/// `Progress` is not (see `state.rs`): `RoomIdx` is a dense index assigned by file order, and
/// phase 6 defines the on-disk shape (ids, not indices) rather than this phase fixing the wrong
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectRef {
    pub room: super::world::RoomIdx,
    pub index: u16,
}

/// The simulation's cursor into an open dialogue. `GameState` owns it directly so `sets_flag`
/// writes stay inside the pure `update()` pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogueState {
    pub npc: ObjectRef,
    pub node: u16,
}

/// The boss's per-phase telegraphed attack shape (spec §6: a distinct pattern per phase). Phase 1
/// always slams, phase 2 always sweeps — see `combat::apply_boss_strike`'s `strike_tiles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BossPattern {
    Slam,
    Sweep,
}

/// One explicit state machine per kind (ADR 0004): flat variants rather than four nested enums,
/// so a `match` over the whole machine fits on one screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiState {
    SlimeIdle {
        until: Tick,
    },
    /// Idle's out-of-aggro-range exit: unlike `SlimeIdle` (which redraws a random direction every
    /// step), `SlimeWander` commits to one drawn `facing` for the whole phase and only redraws it
    /// when blocked, so the idle/wander cycle is a real behavioural difference, not just a
    /// distinct discriminant on identical movement (round-2 review, minor).
    SlimeWander {
        until: Tick,
        facing: Facing,
    },
    SlimeChase {
        until: Tick,
    },
    BatDart {
        steps_left: u8,
        facing: Facing,
    },
    BatRest {
        until: Tick,
    },
    GuardianPatrol {
        waypoint: u8,
        until: Tick,
    },
    GuardianTelegraph {
        until: Tick,
        facing: Facing,
    },
    GuardianDash {
        steps_left: u8,
        facing: Facing,
    },
    GuardianRecover {
        until: Tick,
    },
    BossStalk {
        phase: u8,
        until: Tick,
    },
    BossWindup {
        phase: u8,
        until: Tick,
        pattern: BossPattern,
    },
    /// Exactly one tick long: `ai::step_boss` always advances it to `BossVulnerable` the same
    /// tick, so `combat::apply_boss_strike` (which runs between `ai::step` and
    /// `apply_contact_damage` in the pipeline) sees it exactly once per strike.
    BossStrike {
        phase: u8,
        pattern: BossPattern,
    },
    BossVulnerable {
        phase: u8,
        until: Tick,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enemy {
    pub id: EnemyId,
    pub kind: EnemyKind,
    pub pos: Pos,
    pub facing: Facing,
    pub hp: u8,
    pub ai: AiState,
    /// Empty unless authored (`EnemySpawn::patrol`).
    pub patrol: Vec<Pos>,
    pub move_ready_at: Tick,
    /// Dead enemies stay in the vec so `EnemyId` stays a stable index.
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swing {
    pub started_at: Tick,
    pub facing: Facing,
    pub at: Option<Pos>,
    /// Enemies already damaged by this swing — each target takes damage at most once per swing.
    pub hit: Vec<EnemyId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_hero_can_attack_at_tick_zero_and_is_not_invulnerable() {
        let hero = Hero::at_spawn(Pos { x: 1, y: 1 });
        assert!(hero.can_attack(0));
        assert!(!hero.is_invulnerable(0));
        assert!(hero.attack.is_none());
    }
}

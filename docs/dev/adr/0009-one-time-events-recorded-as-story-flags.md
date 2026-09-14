# 0009. One-time world events (including boss defeat) are recorded as story flags, not dedicated fields

- **Status:** accepted
- **Date:** 2026-09-14

## Context

`.autodev/ARCHITECTURE.md`'s data model lists `Progress { visited, opened_chests, unlocked_doors,
solved_puzzles, flags, boss_defeated: bool }` — a dedicated field for the one boss fight the game
has. Phase 5 also needed a way to (a) stop a defeated boss from respawning when its room is
re-entered, (b) let `LockKind::Flag` doors and `Npc.condition` react to the boss being dead, and
(c) grant the boss's `drops` (the ember) exactly once.

`Progress::flags` (a `BTreeSet<FlagId>`, or equivalent) already exists for dialogue-driven story
flags (ADR 0005's id vocabulary), is already hashed by `state_hash`, and is already the mechanism
`LockKind::Flag` and `Npc.condition` read.

## Decision

`EnemySpawn` gains an authored `defeat_flag: Option<FlagId>`, meaningful only on the one
`EnemyKind::Boss` spawn (validator-enforced: `BossFieldOnRegularEnemy`). On defeat,
`combat::apply_boss_strike`'s companion death handling sets that flag into `Progress::flags` —
exactly the same mechanism a dialogue node uses via `sets_flag` — instead of adding a
`Progress::boss_defeated: bool`. `GameState::enter_room` checks the flag and skips spawning the
boss into the room's live `Vec<Enemy>` when it is already set, rather than spawning a `Dead`/idle
`AiState` for it (see ADR 0004's phase-5 amendment).

This is a deliberate deviation from `.autodev/ARCHITECTURE.md`'s listed `Progress` shape, logged in
`.autodev/DECISIONS.md` ("PLAN phase 05") at design time.

## Alternatives considered

- **`Progress::boss_defeated: bool` as specified** — rejected: a second mechanism recording the same
  kind of fact (a flag) needs its own line in `state_hash`, its own field in the eventual save
  schema (phase 6), and its own case in the validator's `check_npc_conditions`/`LockKind::Flag`
  wiring, all to say something `flags` already says.
- **Inferring "boss defeated" from `hero.has_ember`** — rejected: it conflates the reward with the
  event. A future room, door, or dialogue line that should react to the boss being dead but not to
  the player still carrying the ember (e.g. after the ember is later spent or lost) would have
  nothing to check.

## Consequences

- Respawn suppression, post-victory dialogue, and any future flag-locked door or NPC condition tied
  to the boss fight are all the same mechanism, already tested by `tests/dungeon.rs` (respawn) and
  `tests/boss.rs` (the grant itself); no boss-specific branch exists anywhere outside `ai.rs`/
  `combat.rs`'s own state machine and `enter_room`'s spawn filter.
- Phase 6's save format has one flag set to serialize for every one-time world event, boss defeat
  included, rather than a growing list of dedicated booleans as later content adds more one-time
  events (a second boss, a story milestone, and so on).
- A reader of `.autodev/ARCHITECTURE.md`'s original `Progress` sketch will not find `boss_defeated`
  in the code; this ADR is the record of why, and `docs/dev/content.md` documents the
  `defeat_flag`/`drops` authoring surface this decision produces.

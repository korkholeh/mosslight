# 0004. Model the world with plain structs, enums and entity vectors instead of an ECS

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§8 states that for this scope a single thread with structs, enums and entity collections is sufficient, and
that a game engine or a full ECS should not be used without a justified need. §6 asks for readable state
machines for the three enemy types and the boss. §10 requires that a well-defined subset of state be
persisted while transient combat state is deliberately discarded. §13 requires per-rule unit tests over
movement, hitboxes, cooldowns, invulnerability, key consumption, puzzles, death, and both boss phases.

The scale: 15 rooms, ≤ 12 enemies live at once, four enemy kinds, one hero, a handful of blocks, torches and
plates. This is roughly 20 mutable entities at peak.

## Decision

- `GameState` is a plain struct owning `Hero`, `Vec<Enemy>`, and the per-room mutable objects (blocks,
  torches, plates, hazards) plus `Progress`.
- Each enemy kind gets its own explicit state-machine enum, stored in `Enemy::ai`:
  `Slime: Idle | Chase`, `Bat: Dart | Rest`, `Guardian: Patrol | Telegraph | Dash | Recover`,
  `Boss: PhaseOne{..} | PhaseTwo{..} | Stunned{..} | Dead`. Transitions are a `match` over `(state, tick)`.
- `World` (authored content) is immutable and shared by reference; `GameState` holds only what changes.
- `Progress` — the durable subset named in §10 — is one struct, and `SaveFile` is its serialized form plus a
  version. The save/restore boundary is therefore a field list, not a query over a component store.
- Enemy navigation is local movement plus a breadth-first search bounded to the current 24×16 room, run only
  when an enemy needs a new target. No global pathfinding, no navigation mesh.
- Iteration order over `Vec<Enemy>` is stable and is the tie-break rule when two entities contest a tile,
  which is how §6's "simultaneous attempts resolved deterministically" is satisfied.

## Alternatives considered

- **`bevy_ecs` or `hecs`** — rejected: archetype storage solves a cache-locality problem that does not exist
  at 20 entities, and it would make the §10 save boundary a query rather than a struct, and the §13
  determinism guarantee dependent on the library's iteration order rather than ours.
- **Trait objects (`Box<dyn Enemy>`) with per-kind implementations** — rejected: it scatters the four state
  machines across four files and hides the transition table that §6 asks to be readable. A `match` over an
  enum keeps every transition of one enemy visible on one screen.
- **A single generic `Entity` with optional fields** — rejected: it produces states like "a bat with patrol
  waypoints" that the type system should have made unrepresentable.
- **Storing the current room's mutable objects inside `World`** — rejected: it would blur immutable content
  and mutable progress, which is the distinction the save format depends on.

## Amendment (phase 3, 2026-09-14)

`Enemy::ai` is implemented as one flat `AiState` enum (`SlimeIdle`, `SlimeWander`, `SlimeChase`,
`BatDart`, `BatRest`, `GuardianPatrol`, `GuardianTelegraph`, `GuardianDash`, `GuardianRecover`)
rather than the per-kind nested enums sketched above (`Slime: Idle|Chase`, `Bat: Dart|Rest`, ...).
The core decision this ADR makes — plain structs and enums, no ECS, `Vec<Enemy>` iteration order as
the tie-break — is unchanged; only the shape of `AiState` differs from the sketch. A flat enum keeps
every transition of every kind in one `match` in `game/ai.rs`, which is the "readable state
machines" property §6 asks for and this ADR's own rejection of trait objects already argues for —
nesting would add an indirection at every match site for no benefit at this scale. See
`.autodev/DECISIONS.md`, "PLAN phase 03", for the logged alternatives. `Boss` is not yet a variant
of anything — it is out of scope until phase 5, and adding it is expected to follow the same flat
naming convention (`BossPhaseOne`, `BossStunned`, ...).

## Amendment (phase 5, 2026-09-14)

The boss's four `AiState` variants landed as `BossStalk { phase, until }`, `BossWindup { phase,
until, pattern }`, `BossStrike { phase, pattern }`, `BossVulnerable { phase, until }` — a
`phase: 1 | 2` field on each variant plus `BossPattern::{Slam, Sweep}` on the two that need it,
rather than the `BossPhaseOne{..} | BossPhaseTwo{..} | BossStunned{..} | Dead` sketch in the
previous amendment. Naming the states after what the boss is *doing*
(stalk/telegraph/strike/vulnerable) instead of *which phase* keeps the one cycle that both phases
share — `ai::step_boss` — a single `match` with a `phase` field threaded through, instead of
duplicating the cycle into a phase-1 and a phase-2 branch of the enum itself; `Dead` was never
needed because `enter_room` simply does not spawn a boss whose `defeat_flag` is already set into
`GameState`'s live `Vec<Enemy>`, rather than spawning it in a fourth idle `AiState` (see ADR 0009).
The core decision — a flat enum, no ECS — is unchanged; only this naming detail differs from the
earlier prediction.

## Consequences

- Any simulation test is "build a `GameState`, call `update` with actions, assert fields" — no world
  bootstrap, no component registration.
- Adding a fifth enemy kind means a new enum variant and a new match arm; the compiler lists every place
  that must handle it. This is the property that makes an unattended agent pipeline safe to extend.
- The design does not scale to hundreds of entities. That is accepted: §2 fixes the content, and procedural
  generation is explicitly out of scope.
- Room-local BFS assumes rooms stay 24×16 and enemies never navigate between rooms. Both are spec-fixed (§4,
  §6); a future roaming enemy would need this revisited.

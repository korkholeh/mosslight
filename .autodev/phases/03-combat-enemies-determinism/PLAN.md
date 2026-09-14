# Phase 3 — Sword combat, three enemy kinds, death and determinism

Roadmap phase 3/7 · spec §6 (movement/collision, combat, tuning table, enemies, telegraphs), §7 (pacing:
teach movement, then attack, then threats), §8 (pure simulation, no clock), §13 (per-rule tests for hitboxes,
cooldowns, invulnerability, death; determinism by state hash) · ADR 0002, ADR 0004 · RISKS #2, #9, #14.

## Context

**What exists.** Phases 1–2 shipped the loop, the terminal lifecycle, the input policy, the content pipeline
and room transitions.

- `src/game/state.rs` — `GameState { tick, rng, hero, world: Rc<World>, room: RoomIdx, progress }`, a
  hand-written `PartialEq` that skips `world`, and `update(&mut GameState, &[Action], tick) -> Vec<GameEvent>`
  handling movement, facing-on-blocked-move, tile collision and door transitions. `Action` already carries
  `Attack`, which `update` accepts and ignores. `GameEvent` is `HeroMoved | MoveBlocked | Message |
  RoomEntered`.
- `src/game/entities.rs` — `Pos { x: u8, y: u8 }`, `Facing`, `Hero { pos, facing, step_ready_at,
  health_halves: 6, max_health_halves: 6, keys }` with `can_step`/`mark_stepped`. No enemy type exists.
- `src/game/tuning.rs` — `TICK_HZ = 30`, `HERO_STEP_TICKS = 4`, `SWORD_COOLDOWN_TICKS = 9`,
  `SWORD_ACTIVE_TICKS = 3`, `INVULN_TICKS = 24`, `TELEGRAPH_MIN_TICKS = 18`, `ROOM_W = 24`, `ROOM_H = 16`
  — every §6 constant is already declared and, apart from `HERO_STEP_TICKS`, unused.
- `src/game/rng.rs` — SplitMix64 `Rng { state }` with `next_u64`/`below(n)`; currently nothing in `update`
  consumes it.
- `src/content/schema.rs` — `EnemySpawn { kind: EnemyKind, at: Pos, patrol: Option<Vec<Pos>> }` and
  `Room::enemies: Vec<EnemySpawn>` already parse; `EnemyKind` is `Slime | Bandit | Wisp`.
  `assets/world.ron` authors **no** enemies (nine overworld rooms, geometry and doors only).
- `src/content/validate.rs` — `collect_errors` runs five structural checks plus the `(room, component,
  unlocked, items)` reachability BFS; there is no check over `Room::enemies` yet.
- `src/app.rs` — `Mode { MainMenu, Playing, Paused, ConfirmQuit, Help, TooSmall }`, `App { mode, prev_mode,
  state, menu, message, size, dirty, quit, tick_counter, seed, world }`, `App::tick(actions)` calling
  `update` and setting `dirty` only when the returned event list is non-empty. No `GameOver`, no checkpoint.
- `src/render/` — `tiles::Kind { Hero, Wall, Floor, Water, Bush, Door, Stairs, Pit }` with one glyph table and
  a colour-only `Theme`; `scene::draw_scene` paints the tile grid then the hero.

**What this phase changes.** The room stops being empty scenery and becomes a fight. `GameState` gains a
stable `Vec<Enemy>` rebuilt from the room's authored spawns on every room entry; `update` grows a fixed
per-tick pipeline (hero → sword → AI → contact → death); `combat.rs` and `ai.rs` appear as the two new pure
modules ARCHITECTURE.md already reserves; `state_hash()` makes determinism a machine-checked property rather
than a claim; `App` grows `Mode::GameOver` with a retry that restores an in-memory room-entry checkpoint; and
`assets/world.ron` gets its first enemy spawns, placed to encode §7's pacing ramp structurally (RISKS #2).

**Key files.**

```
new      src/game/combat.rs        swing lifecycle, hit resolution, damage, invulnerability, knockback
new      src/game/ai.rs            room-local BFS + the three per-kind state machines
new      tests/combat.rs  tests/ai.rs  tests/determinism.rs
new      tests/fixtures/broken_enemy_spawn.ron
changed  src/game/entities.rs      Enemy, EnemyId, AiState, Swing; Hero attack/invuln fields
changed  src/game/state.rs         enemies vec, tick pipeline, new GameEvents, state_hash()
changed  src/game/tuning.rs        the per-kind and damage/knockback constants
changed  src/game/mod.rs           re-exports
changed  src/content/schema.rs     EnemyKind: Bandit/Wisp -> Bat/Guardian
changed  src/content/validate.rs   check_enemy_spawns
changed  src/content/error.rs      EnemySpawnNotWalkable, EnemyPatrolInvalid
changed  src/app.rs                Mode::GameOver, checkpoint, retry
changed  src/render/{tiles,theme,scene,hud}.rs  enemy/sword/telegraph kinds and draw order
changed  assets/world.ron          enemy spawns in six of the nine overworld rooms
changed  tests/{mode_machine,render,content}.rs  new mode and new glyph coverage
```

## Design

### Content: the enemy kinds are renamed to the spec's

`EnemyKind` becomes `Slime | Bat | Guardian`. `Bandit`/`Wisp` were a phase-2 placeholder and match neither
spec §6 ("Слиз / Кажан / Вартовий") nor ARCHITECTURE.md's `EnemySpawn { kind: Slime|Bat|Guardian|Boss }`.
Nothing constructs the variants today (only the three re-export lines and one empty `enemies: Vec::new()`
in a validator test), so this is a rename, not a migration. `Boss` is *not* added: it belongs to phase 5 and
adding it now would mean an `ai.rs` match arm with nothing behind it.

### Entities

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnemyId(pub u16);          // index into GameState::enemies; spawn order, stable

pub enum AiState {                    // one explicit machine per kind (ADR 0004)
    SlimeIdle    { until: Tick },
    SlimeChase   { until: Tick },
    BatDart      { steps_left: u8, facing: Facing },
    BatRest      { until: Tick },
    GuardianPatrol    { waypoint: u8, until: Tick },
    GuardianTelegraph { until: Tick, facing: Facing },
    GuardianDash      { steps_left: u8, facing: Facing },
    GuardianRecover   { until: Tick },
}

pub struct Enemy {
    pub id: EnemyId,
    pub kind: EnemyKind,
    pub pos: Pos,
    pub facing: Facing,
    pub hp: u8,
    pub ai: AiState,
    pub patrol: Vec<Pos>,             // empty unless authored
    pub move_ready_at: Tick,
    pub alive: bool,                  // dead enemies stay in the Vec so EnemyId stays an index
}

pub struct Swing { pub started_at: Tick, pub facing: Facing, pub at: Pos, pub hit: Vec<EnemyId> }
```

`Hero` gains `attack: Option<Swing>`, `attack_ready_at: Tick`, `invuln_until: Tick`. The vestigial
`serde::Serialize` derives on `Hero`, `Facing` and `Rng` are dropped — nothing serializes them, and phase 6
defines the on-disk shape from scratch (the same reasoning phase 2 applied to `GameState`/`Progress`).
`Pos` keeps `Serialize`/`Deserialize`: the content schema needs them.

`AiState` names are flat (`SlimeIdle`, not `Slime(SlimeState)`) so a `match` over the whole machine fits on
one screen, which is what §6's "readable state machines" asks for and what ADR 0004 chose the enum for.

### `GameState` and the per-tick pipeline

`GameState` gains `pub enemies: Vec<Enemy>`; `PartialEq` gains the field. `GameState::new` and every room
transition call `spawn_enemies(&mut self)`, which rebuilds `enemies` from `self.room().enemies` in authored
order — so enemies are room-local, respawn on re-entry, and `EnemyId(i)` is always `enemies[i]`. Nothing
about a fight is durable, which is what §10 already says ("transient combat state is deliberately
discarded") and what phase 6's "on load, enemies respawn" will inherit for free.

`update` becomes a fixed sequence per tick, and the order is part of the contract because it is what makes
contested tiles deterministic:

1. **Hero actions** — movement (unchanged) and `Action::Attack` starting a swing. A door transition here
   ends the tick: the enemy vector is rebuilt for the new room and steps 2–5 are skipped, so the hero never
   takes contact damage from a room they have already left.
2. **Sword resolution** — while `tick < swing.started_at + SWORD_ACTIVE_TICKS`, every live enemy standing on
   `swing.at` whose `EnemyId` is not yet in `swing.hit` takes damage and is added to `swing.hit`. Evaluating
   every tick of the active window (rather than once at the start) is what makes the window mean something —
   an enemy that steps into the struck tile mid-swing is hit — and the per-swing `hit` list is what makes
   "each target damaged at most once per swing" true by construction.
3. **Enemy AI** — `ai::step(state, tick)` iterates `enemies` by index in order. Each enemy sees the
   *current* occupancy (hero tile plus every enemy's position, including those already moved this tick), so
   two enemies contesting a tile resolve by `Vec` iteration order, as ADR 0004 requires. An enemy never
   enters the hero's tile, a hazard tile, or a non-walkable tile.
4. **Contact damage** — after all movement, the first live enemy (in `Vec` order) orthogonally adjacent to
   or on the hero's tile deals `CONTACT_DAMAGE_HALVES` if `tick >= hero.invuln_until`; the hero is knocked
   back one tile directly away from that enemy and `invuln_until = tick + INVULN_TICKS`. Iterating in `Vec`
   order and stopping at the first contact keeps the knockback direction deterministic when the hero is
   pinned between two enemies.
5. **Death** — `health_halves == 0` emits `HeroDied` once and freezes the hero for the rest of the tick.
6. **Swing expiry** — the swing is cleared once the active window has passed; `attack_ready_at` was already
   set to `started_at + SWORD_COOLDOWN_TICKS` in step 1, so `Attack` inside the cooldown is dropped silently.

### Combat rules (`src/game/combat.rs`)

- **Hitbox**: exactly one tile, `step_target(hero.pos, hero.facing)`. Off-grid means the swing lands on
  nothing; it still costs the cooldown.
- **Damage**: `SWORD_DAMAGE_HP = 1` against an enemy, `CONTACT_DAMAGE_HALVES = 1` against the hero — §6's
  "typical damage = half a heart". Health is always counted in halves (`health_halves`), never in hearts.
- **Guardian armour**: a Guardian takes sword damage **only** while in `GuardianRecover`; otherwise the hit
  is deflected (`GameEvent::AttackDeflected`) and costs the cooldown. This is how §6's "vulnerability window
  after the attack" is made real for the guardian and is the property the acceptance criterion pins. A
  guardian is never on the critical path, so armour cannot soft-lock a route.
- **Knockback**: `knockback(room, occupied, from, facing, tiles)` walks up to `tiles` steps one tile at a
  time and stops *before* the first tile that is out of bounds, non-walkable, a hazard, occupied, or a
  `Tile::Door`. Excluding doors is deliberate: a knockback that lands on a door tile would teleport the hero
  into the next room mid-hit, which is a surprising, un-cancellable transition. The entity therefore always
  ends adjacent to the obstacle and never inside or beyond it.
- No projectiles this phase: enemies damage only by contact (§6 permits "contact with an enemy or its
  attack"; the dash is the guardian's attack and it damages on contact while dashing).

### AI (`src/game/ai.rs`)

**Navigation.** `path_step(room, occupied, from, to) -> Option<Facing>`: a breadth-first search over the
current room only, bounded to `ROOM_W * ROOM_H = 384` cells in a fixed-size array, expanding neighbours in
the fixed order N, E, S, W. It returns the first step of the shortest path, or `None`. The fallback when
`None` (walled off, or the hero unreachable) is `local_step`: try the axis with the larger delta first, then
the other, then keep still. Both are total and allocation-light; neither consults the RNG, so navigation is
deterministic independent of seed (RISKS #14).

**The three machines.** Each one alternates by construction, so none of them can settle into a single state
even against a motionless hero — which is exactly the "never stops changing AI state" property the
acceptance criterion asserts, and the structural answer to "an enemy wedged in a corner makes a room look
broken" (RISKS #14):

| Kind | States | Transition rule |
|---|---|---|
| Slime | `SlimeIdle{until}` → `SlimeChase{until}` → `SlimeIdle{..}` | Idle wanders one tile every `SLIME_STEP_TICKS` in an RNG-drawn direction; at `until` it enters Chase if the hero is within `SLIME_AGGRO_RADIUS` (Manhattan), otherwise a fresh Idle. Chase BFS-steps toward the hero every `SLIME_STEP_TICKS` for `SLIME_CHASE_TICKS`, then returns to Idle. |
| Bat | `BatDart{steps_left, facing}` → `BatRest{until}` → `BatDart{..}` | Dart moves every `BAT_STEP_TICKS` (faster than the slime) for `BAT_DART_STEPS` steps; the direction is the BFS step toward the hero when in range, else an RNG-drawn direction. A blocked dart step ends the dart early. Rest holds for `BAT_REST_TICKS`. |
| Guardian | `GuardianPatrol{waypoint, until}` → `GuardianTelegraph{until}` → `GuardianDash{steps_left}` → `GuardianRecover{until}` → Patrol | Patrol walks the authored waypoint ring (or paces its facing axis when no waypoints are authored) and enters Telegraph when the hero is on its facing axis within `GUARDIAN_SIGHT`, **or** unconditionally after `GUARDIAN_PATROL_TICKS` — the timer is what guarantees the cycle never stalls in an empty maze. Telegraph holds `GUARDIAN_TELEGRAPH_TICKS` (= `TELEGRAPH_MIN_TICKS` = 18 ticks = 600 ms) without moving. Dash moves `GUARDIAN_DASH_TILES` tiles at `GUARDIAN_DASH_STEP_TICKS`, stopping early at an obstacle. Recover holds `GUARDIAN_RECOVER_TICKS` and is the only state in which the guardian takes damage. |

RNG use is confined to slime wander direction, bat dart direction out of range, and patrol tie-breaks — and
it is what makes the different-seed determinism test meaningful rather than a constant-hash tautology.

### `state_hash()` (`src/game/state.rs`)

A hand-rolled FNV-1a 64 over a canonical byte encoding, not `std::collections::hash_map::DefaultHasher`
(documented as unstable across Rust releases) and not a new dependency (RISKS #16). `StateHasher` writes,
in a fixed order: `tick`, `rng` state, hero (pos, facing, `health_halves`, `max_health_halves`, `keys`,
`step_ready_at`, `attack_ready_at`, `invuln_until`, swing presence/`started_at`/`facing`/`at`/sorted `hit`),
`room.0`, every `Enemy` in `Vec` order (id, kind discriminant, pos, facing, hp, alive, `move_ready_at`,
AI discriminant + payload), then `progress.visited` in `BTreeSet` order. `world` contributes only
`world.version`: it is immutable input, shared by `Rc`, and hashing 15 tile grids every tick would make the
determinism test measure the content loader instead of the simulation.

### Events

`GameEvent` gains `HeroDamaged { remaining_halves }`, `HeroDied`, `EnemyDamaged { id, remaining_hp }`,
`EnemyKilled { id, kind, at }`, `AttackSwung { at: Option<Pos> }`, `AttackDeflected { id }`,
`EnemyMoved { id, from, to }` and `EnemyAiChanged { id }`. The first four are on ARCHITECTURE.md's list; the
last four are additions this phase needs and are justified rather than silent: `App::tick` sets `dirty` only
when `update` returns events, so an enemy that moves or begins telegraphing while the hero stands still must
announce itself or the frame is never redrawn (CLAUDE.md: "do not draw when nothing changed" cuts both ways).

### Death, `GameOver` and the checkpoint

`Mode::GameOver` joins the mode machine. `App` gains `checkpoint: GameState`, cloned at New Game and
refreshed whenever `update` reports `RoomEntered` — so the checkpoint is always the state as of the last
room entry, the hero's health included. `App::tick` switching to `GameOver` on `HeroDied`; in `GameOver`,
`Confirm` (retry) restores `self.state = self.checkpoint.clone()` **and rewinds `self.tick_counter` to
`self.state.tick`**, `Cancel` returns to the main menu, `Quit` routes to `ConfirmQuit` as everywhere else.
The rewind matters: every timer in the simulation is an absolute `Tick`, so restoring a state from tick 900
while the counter sat at tick 5000 would fire every cooldown, invulnerability window and AI timer at once.
Phase 6 repoints the restore at the save file and keeps this rewind rule. The simulation does not run in
`GameOver` (`App::simulating()` is still `mode == Playing`), so the death frame stays on screen.

### Content: enemy spawns and the pacing ramp

`assets/world.ron` gets its first enemies, placed so §7's ramp ("the first safe stretch teaches movement and
interaction; the next teaches attack; only then harder threats") is a property of the content rather than an
intention (RISKS #2). Using the authored 3×3 `map_index` grid:

| Room | grid | Enemies |
|---|---|---|
| `room.lighthouse` (start) | (1,2) | **none** — the movement/interaction room |
| `room.crossroads` | (1,1) | **none** — phase 4 puts the sword chest here, on the critical path |
| `room.west_grove` | (0,1) | 1 Slime |
| `room.east_marsh` | (2,1) | 1 Slime |
| `room.south_shore` | (2,2) | 1 Slime |
| `room.fallen_pines` | (0,2) | 1 Slime, 1 Bat |
| `room.stone_circle` | (1,0) | 2 Bats |
| `room.old_mill` | (2,0) | 1 Bat, 1 Slime |
| `room.north_ridge` | (0,0) | 1 Guardian (with a four-waypoint patrol) |

The two rooms nearest the start stay empty and the guardian sits in the far corner, two rooms off the
critical path. Phase 4 authors the sword into `room.crossroads` and inherits this table; a test in
`tests/ai.rs` pins "start room and crossroads have zero spawns" so a later content edit cannot quietly break
the ramp.

The validator gains `check_enemy_spawns`: each spawn position and each patrol waypoint must be in bounds,
walkable, not a hazard and not a `Tile::Door`, reported as `ContentError::EnemySpawnNotWalkable` /
`EnemyPatrolInvalid`. `tests/fixtures/broken_enemy_spawn.ron` (a spawn inside a wall) proves the check
rejects rather than passes.

### Render

`tiles::Kind` gains `Slime`, `Bat`, `Guardian`, `Sword`, `Telegraph` with glyphs `o`, `^`, `&`, `/`, `!`.
Glyphs are theme-independent as always (RISKS #10), and `theme.rs` gains a colour per kind for
gameboy/ansi while `mono` stays white. `draw_scene`'s order becomes: tiles → telegraph markers → enemies →
sword → hero, so the hero is never hidden by an enemy and the sword is never hidden by a tile.

- **Attack animation**: while `hero.attack` is `Some` and the window is active, `Kind::Sword` is painted on
  `swing.at`. It is a state projection, not a timer in the renderer — ~100 ms of a glyph, no decorative
  animation (§4).
- **Danger cue**: while a guardian is in `GuardianTelegraph`, `Kind::Telegraph` is painted on every tile of
  the lane it is about to dash through. The cue is a glyph, so it survives monochrome.
- `hud.rs` keeps showing `HP n/m`; the `GameOver` overlay is a new `overlays::draw_game_over` box reading
  "You fell. Enter: retry from the last room · Esc: main menu".

### Error handling

Nothing here is fallible in the `Result` sense: `game/` stays total. Out-of-bounds arithmetic uses
`checked_add`/`checked_sub` exactly as `step_target` already does; an unresolvable BFS returns `None` and
falls through to local movement; an enemy spawn that somehow points outside the grid is a content error
caught by the validator before any `GameState` is built. No `unwrap` is added on the content or save path.

### Architecture conformance

This phase adds the two modules ARCHITECTURE.md already reserves (`game/combat.rs`, `game/ai.rs`) with the
entity shapes ADR 0004 specifies, and keeps `update(&mut GameState, &[Action], tick) -> Vec<GameEvent>`
unchanged. `game/` still imports neither `std::io`, nor `std::time`, nor `ratatui`. Deviations, all logged
in DECISIONS.md: the `EnemyKind` rename (schema was wrong, not the architecture); `AiState` as one flat enum
rather than four nested ones; `Enemy` carrying `patrol`/`move_ready_at`/`alive` instead of an opaque
`timers`; four extra `GameEvent` variants; and `pending_transition` staying unimplemented (transitions
resolve within the tick, as phase 2 built them).

## Tasks

- [x] **T1** — `src/game/tuning.rs`: add `SWORD_DAMAGE_HP`, `CONTACT_DAMAGE_HALVES`, `KNOCKBACK_TILES`,
  `SLIME_{HP,STEP_TICKS,AGGRO_RADIUS,IDLE_TICKS,CHASE_TICKS}`, `BAT_{HP,STEP_TICKS,DART_STEPS,REST_TICKS,
  AGGRO_RADIUS}`, `GUARDIAN_{HP,SIGHT,PATROL_TICKS,PATROL_STEP_TICKS,TELEGRAPH_TICKS,DASH_TILES,
  DASH_STEP_TICKS,RECOVER_TICKS}`, each with the millisecond equivalent in its doc comment.
  `GUARDIAN_TELEGRAPH_TICKS` is defined as `TELEGRAPH_MIN_TICKS` so the §6 floor cannot drift.
  `src/content/schema.rs`: rename `EnemyKind::{Bandit, Wisp}` to `{Bat, Guardian}`.
- [x] **T2** — `src/game/entities.rs`: add `EnemyId`, `AiState`, `Enemy`, `Swing`; add `attack`,
  `attack_ready_at`, `invuln_until` to `Hero` plus `can_attack(tick)`/`is_invulnerable(tick)`; drop the
  vestigial `serde::Serialize` derives from `Hero`, `Facing` and `Rng`. Unit test: a fresh hero can attack at
  tick 0 and is not invulnerable.
- [x] **T3** — `src/game/state.rs`: add `enemies: Vec<Enemy>` and `spawn_enemies()`, called from
  `GameState::new` and on every door transition; extend `PartialEq`; add the eight new `GameEvent` variants;
  restructure `update` into the six-step pipeline with steps 2–5 as calls into the (still empty) `combat`
  and `ai` modules. Existing `tests/movement.rs` and `tests/transitions.rs` must still pass unchanged.
- [x] **T4** — `src/game/state.rs`: `StateHasher` (FNV-1a 64) and `state_hash(&GameState) -> u64` over the
  field order fixed in Design. Unit tests: two states built the same way hash equal; mutating each hashed
  field in turn changes the hash (a table-driven test, so a field added later without being hashed is caught).
- [x] **T5** — `src/game/combat.rs`: `start_swing`, `resolve_swing`, `damage_enemy`, `damage_hero`,
  `knockback`. Implements the hitbox, the active window, the per-swing `hit` list, the cooldown, half-heart
  damage, the invulnerability window, guardian armour and obstacle-stopping knockback. Wire into `update`
  steps 2, 4 and 5.
- [x] **T6** — `src/game/ai.rs`: `path_step` (bounded BFS, fixed N/E/S/W expansion order) and `local_step`
  fallback, plus the `occupancy` helper. Unit tests: BFS finds the one-tile-wide corridor route around a
  wall; `path_step` returns `None` when walled off and `local_step` still produces a legal move or none.
- [x] **T7** — `src/game/ai.rs`: `step(state, tick) -> Vec<GameEvent>` with the three per-kind machines from
  the Design table, iterating `enemies` by index and honouring current occupancy. Emits `EnemyMoved` and
  `EnemyAiChanged`.
- [x] **T8** — `src/app.rs`: `Mode::GameOver`, `App::checkpoint`, checkpoint refresh on `RoomEntered`,
  transition to `GameOver` on `HeroDied`, retry restoring the checkpoint and rewinding `tick_counter`,
  `Cancel` back to `MainMenu`. `src/input.rs` needs no change (`Confirm`/`Cancel` already map in every mode
  except `Playing`; add `Mode::GameOver` to the `Enter`/`E` arm's confirm list).
- [x] **T9** — `src/render/`: new `Kind` variants and glyphs in `tiles.rs`, colours in `theme.rs` (mono
  unchanged), draw order and the sword/telegraph projections in `scene.rs`, `overlays::draw_game_over` and
  its `Mode::GameOver` arm. Extend the in-module glyph/theme tables so `ALL_KINDS` covers the new kinds.
- [x] **T10** — `src/content/error.rs` + `validate.rs`: `EnemySpawnNotWalkable` and `EnemyPatrolInvalid`
  variants with `Display` text, `check_enemy_spawns` added to `collect_errors`, plus
  `tests/fixtures/broken_enemy_spawn.ron` and a case in `tests/content.rs` asserting that specific variant.
- [x] **T11** — `assets/world.ron`: author the enemy spawns of the pacing table (including the guardian's
  four waypoints). `tests/content.rs` keeps asserting the real world validates.
- [x] **T12** — `tests/combat.rs`: facing hitbox (4 facings hit, the two side tiles and the behind tile
  miss); two enemies on the struck tile path each damaged at most once per swing; a second `Attack` inside
  `SWORD_COOLDOWN_TICKS` ignored and the next one outside it landing; contact damage costs one half-heart,
  sets invulnerability, and a second contact inside the window costs nothing; knockback into a wall leaves
  the entity adjacent to it; knockback never lands on a door tile; an enemy at 0 hp emits `EnemyKilled`
  and stops acting.
- [x] **T13** — `tests/ai.rs`: a maze-room fixture world; each kind run 1000 ticks asserting the AI state
  changes at least once every `MAX_STALL_TICKS` and the position stays inside the room and off non-walkable
  tiles; the guardian telegraph precedes the first dash step by ≥ `TELEGRAPH_MIN_TICKS` and the guardian
  takes damage only in `GuardianRecover`; two enemies contesting one tile resolve by `Vec` order; the
  authored start room and `room.crossroads` contain zero enemy spawns.
- [x] **T14** — `tests/determinism.rs`: a 2000-action pseudo-random sequence (drawn from a test-local
  `Rng`, one action per tick) replayed twice from the same seed with `state_hash()` compared at every tick;
  the same sequence under two different seeds asserted to produce a differing hash at some tick; and a
  fps-style variant confirming the hash sequence is unaffected by how the actions are batched.
- [x] **T15** — update `tests/mode_machine.rs` (GameOver entry, retry, Esc to menu, simulation frozen in
  GameOver) and `tests/render.rs` (enemy, sword and telegraph glyphs land on their expected cells at 60×24;
  the same scene renders identical characters under all three themes).
- [x] **T16** — run the full gate, fix fallout (clippy on the new matches is the likely source), and update
  `CHANGELOG.md` plus `docs/dev/testing.md` with the three new test files.

## Verification

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

| # | Acceptance criterion | Test that proves it |
|---|---|---|
| 1 | Sword hits exactly the faced tile for each of the four facings, misses beside and behind | `tests/combat.rs::sword_hits_only_the_faced_tile` (4 facings × hit) and `::sword_misses_side_and_rear_tiles` |
| 2 | Two enemies on the struck tile path each take damage at most once per swing | `tests/combat.rs::each_target_is_damaged_at_most_once_per_swing` |
| 3 | A second attack inside the cooldown is ignored; the first one outside it lands | `tests/combat.rs::attack_inside_cooldown_is_ignored_and_the_next_one_lands` |
| 4 | Contact costs one half-heart, grants the tuned invulnerability, second contact inside it is free | `tests/combat.rs::contact_damage_costs_one_half_heart_and_grants_invulnerability` |
| 5 | Knockback into a wall leaves the entity adjacent, never inside or beyond | `tests/combat.rs::knockback_stops_adjacent_to_an_obstacle` (+ `::knockback_never_lands_on_a_door_tile`) |
| 6 | Each kind runs 1000 ticks in a maze, never stops changing AI state, never leaves the room | `tests/ai.rs::each_kind_keeps_changing_state_for_1000_ticks` (parametrised over the three kinds) and `::enemies_stay_inside_the_room_bounds` |
| 7 | Guardian telegraphs ≥ 600 ms before the dash and is vulnerable during Recover | `tests/ai.rs::guardian_telegraphs_at_least_the_minimum_warning` and `::guardian_takes_damage_only_while_recovering` |
| 8 | Zero health emits `HeroDied`, opens `GameOver`, retry restores the room-entry checkpoint | `tests/mode_machine.rs::death_opens_game_over_and_retry_restores_the_checkpoint` |
| 9 | Same seed + same 2000-action sequence twice ⇒ identical `state_hash()` at every tick | `tests/determinism.rs::same_seed_same_actions_hash_identically_every_tick` |
| 10 | Two different seeds over the same sequence ⇒ the hashes differ | `tests/determinism.rs::different_seeds_diverge` |
| 11 | The full phase gate passes | the four commands above |

Supporting coverage, mapped to `.autodev/guides/case-taxonomy.md`: *boundaries* — a swing aimed off-grid,
knockback at the room edge, hero at 1 half-heart; *state* — a room with zero enemies, a room with two
enemies contesting one tile, all enemies dead; *idempotency* — `Attack` pressed twice in one tick batch,
re-entering a room respawning its enemies; *lifecycle* — death → retry → death again; *platform sanity* —
the new glyphs asserted at 60×24 and in mono. Deferred as `deferred_not_authored`: boss states (phase 5),
save-backed restore (phase 6), enemy drops and `EnemyKilled` rewards (no reward mechanic exists yet).

## Risks

- **#14 Enemy AI gets stuck** — the risk this phase owns most directly. Mitigations are structural, not
  hopeful: every machine alternates on a timer (the guardian even telegraphs unconditionally after
  `GUARDIAN_PATROL_TICKS`), the BFS is bounded to 384 cells with a fixed expansion order, `local_step` is
  the total fallback when no path exists, and `tests/ai.rs` runs each kind 1000 ticks in a maze asserting
  both liveness (state keeps changing) and containment (never outside the room, never on a wall).
- **#2 We build the wrong game (pacing)** — the enemy placement table above encodes §7's ramp as content,
  the start room and the future sword room hold zero enemies, and a test pins that so phase 4 cannot undo it.
- **#9 Boss unbeatable or trivial** — not the boss yet, but the guardian is its rehearsal: telegraph →
  attack → vulnerability window, every constant in `tuning.rs`, the vulnerability window asserted by a test.
  Whatever phase 5 learns here is a one-file balance change.
- **#1 Content unfinishable** — new authored objects mean new ways to author them wrongly, so
  `check_enemy_spawns` extends the validator the same tick the spawns appear, with a broken fixture proving
  it rejects.
- **#8 / ADR 0002 determinism** — `state_hash()` plus the two-seed test turn "the simulation is
  deterministic" into a suite property. The AI deliberately draws from `state.rng` so the different-seed
  test cannot pass on a constant.
- **#16 Extra dependencies** — the hash is hand-rolled FNV-1a; no crate is added.
- **#10 Monochrome legibility** — five new glyphs, all distinct, all theme-independent, asserted by the
  render test that compares characters across themes.

## Out of scope

- The boss, its two phases, `Boss` as an `EnemyKind`, and `BossPhaseChanged`/`BossDefeated` — phase 5.
- Puzzles, `game/puzzles.rs`, blocks, plates and torches — phase 5.
- Chests, rewards, NPCs, dialogue, the sword and lantern as *acquired items* — phase 4. The hero swings
  from the start this phase; gating the swing on owning the sword is phase 4's job.
- The dungeon rooms and locked doors — phase 5.
- `src/save.rs`, autosave, and repointing the death checkpoint at the save file — phase 6.
- Themes beyond the colours the new kinds need, Unicode glyphs, `NO_COLOR` presentation work — phase 6.
- Balance of the constants added here against the 30–45 minute target, and the byte/CPU measurement —
  phase 7.

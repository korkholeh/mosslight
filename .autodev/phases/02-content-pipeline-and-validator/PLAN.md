# Phase 2 — Content pipeline, validator and room transitions

Roadmap phase 2/7 · spec §7 (world, progress, validator list), §8 (serde + RON, embedded content),
§13 (content validity, room transitions), §15 (no dead ends) · ADR 0005.

## Context

**What exists.** Phase 1 shipped a runnable skeleton: `Cargo.toml` (ratatui 0.30.2, clap, serde, ron,
serde_json, signal-hook — `ron` is already declared and unused), `src/{main,lib,config,terminal,input,app}.rs`,
`src/game/{mod,state,world,entities,tuning,rng}.rs`, `src/render/{mod,theme,tiles,hud,scene,overlays}.rs`
and nine test files. The world today is a single hard-coded room: `game::world::debug_room()` returns
`Room { tiles: [[Tile; 24]; 16], spawn: Pos }` with `Tile = Floor | Wall | Water | Bush`, and
`GameState { tick, rng, hero, room: Room }` owns that one room by value. `game::update` moves the hero one
tile per `HERO_STEP_TICKS`, refuses non-`Floor` targets and emits `HeroMoved | MoveBlocked | Message`.
`src/content/`, `assets/world.ron` and `src/save.rs` do not exist. `terminal::preflight` already defines the
pre-raw-mode refusal shape `StartupError { message, code }`, and `--debug-panic` shows the house style for a
hidden test-harness flag (`#[arg(long, hide = true)]`).

**What this phase changes.** The world stops being code and becomes data: `assets/world.ron` embedded with
`include_str!`, parsed into a `World` of nine overworld rooms in a 3×3 grid, and machine-checked by a
validator implementing the full §7 list — including a reachability search that makes "no dead ends through
keys or locks" a property the suite enforces *before* any real content is authored (RISKS #1). `GameState`
then points into that world instead of owning one room, and stepping onto a door tile moves the hero to the
target room's spawn, grows the visited set and emits `RoomEntered`.

**Key files.**

```
new      src/content/{mod,schema,loader,validate,error}.rs
new      assets/world.ron
new      tests/content.rs  tests/transitions.rs  tests/content_startup.rs
new      tests/fixtures/*.ron           (1 valid base + 8 single-defect + 1 three-defect)
new      docs/dev/content.md
changed  src/lib.rs                     (+ `pub mod content;`)
changed  src/game/world.rs              (re-export the content tile/room types; debug_room deleted)
changed  src/game/state.rs              (Rc<World>, RoomIdx, Progress, transition handling, RoomEntered)
changed  src/game/mod.rs                (re-exports)
changed  src/app.rs                     (App::new / New Game build state from the loaded world)
changed  src/render/{tiles,scene,hud}.rs (new tile kinds; scene reads state.room())
changed  src/config.rs                  (hidden `--debug-content PATH`)
changed  src/main.rs                    (content preflight before the terminal is touched)
changed  tests/movement.rs, tests/render.rs, tests/mode_machine.rs, tests/loop_timing.rs (as needed)
changed  CHANGELOG.md
```

## Design

### Module layout

`src/content/` is the only owner of the authored world. It is pure (no `std::io` except the `--debug-content`
read, which lives in `main.rs`/`config.rs`, not in `content`), and it is the layer `game` and `render` read
through.

```rust
// content/mod.rs
pub mod error; pub mod loader; pub mod schema; pub mod validate;
pub use error::{ContentError, IdKind};
pub use schema::{Door, LockKind, Pos, Reward, Room, RoomKind, Spawn, Tile, World};
pub use loader::{load, parse};          // load() = parse(EMBEDDED)
pub use validate::validate;
```

### Data model (`content/schema.rs`)

Authored (RON) and runtime (decoded) types are the same structs, except that tiles are authored as character
rows and decoded into a fixed grid.

```rust
pub struct World {
    pub version: u32,
    pub start: StartPoint,                  // room id + spawn id
    pub route: Route,                       // { ember_required: bool, home: RoomId }
    pub rooms: Vec<Room>,
}

pub struct Room {
    pub id: String,                         // "room.lighthouse"
    pub name: String,
    pub kind: RoomKind,                     // Overworld | Dungeon
    pub map_index: Option<(u8, u8)>,        // 3x3 overworld slot
    pub rows: Vec<String>,                  // authored: 16 strings of 24 chars
    #[serde(skip)] pub tiles: TileGrid,     // decoded by the loader
    #[serde(default)] pub doors: Vec<Door>,
    #[serde(default)] pub spawns: Vec<Spawn>,
    #[serde(default)] pub chests: Vec<Chest>,
    #[serde(default)] pub npcs: Vec<Npc>,
    #[serde(default)] pub enemies: Vec<EnemySpawn>,
    #[serde(default)] pub puzzles: Vec<Puzzle>,
}

pub struct Door  { pub id: String, pub at: Pos, pub to_room: String, pub to_spawn: String,
                   pub lock: Option<LockKind>, #[serde(default = "yes")] pub two_way: bool }
pub struct Spawn { pub id: String, pub at: Pos }
pub struct Chest { pub id: String, pub at: Pos, pub contains: Reward }
pub struct Npc   { pub id: String, pub at: Pos, pub dialogue: Vec<String>, pub condition: Option<String> }
pub struct EnemySpawn { pub kind: EnemyKind, pub at: Pos, pub patrol: Option<Vec<Pos>> }
pub struct Puzzle { pub id: String, pub kind: PuzzleKind, pub reward: Option<Reward> }

pub enum Tile { Floor, Wall, Water, Bush, Door, Stairs, Pit, Hidden }
pub enum LockKind { SmallKey, Lantern, Flag(String) }
pub enum Reward { Sword, Lantern, SmallKey, HeartContainer, Ember, Message(String) }
```

`Chest`, `Npc`, `EnemySpawn`, `Puzzle`, `LockKind::{Lantern,Flag}` and the `Reward` variants exist in the
schema this phase (the roadmap deliverable names them, the validator's lock and reachability rules need
them, and the fixtures exercise them) but `assets/world.ron` authors **none** of them yet: geometry, doors
and spawns only. Every collection is `#[serde(default)]`, so later phases add content without touching the
schema.

**Tile characters** (`content/schema.rs`, the single table):

| char | tile | walkable | hazard | glyph on screen |
|---|---|---|---|---|
| `.` | Floor | yes | no | `.` |
| `#` | Wall | no | no | `#` |
| `~` | Water | no | yes | `~` |
| `"` | Bush | no | no | `"` |
| `+` | Door | yes | no | `+` |
| `>` | Stairs | yes | no | `>` |
| `v` | Pit | no | yes | `v` |
| `?` | Hidden | no | no | `#` (deliberately indistinguishable until phase 4 reveals it) |

Any other character is `ContentError::IllegalTile`. `Tile::Hidden` is inert this phase: not walkable, no
reveal mechanic (phase 4).

### Ids and interning

Ids are stable namespaced strings exactly as ADR 0005 fixes them (`room.lighthouse`,
`door.crossroads.north`, `spawn.crossroads.north`). The loader interns room ids into dense
`RoomIdx(u16)` and door/spawn ids into per-room indices, storing the maps in `World`:

```rust
impl World {
    pub fn room_idx(&self, id: &str) -> Option<RoomIdx>;
    pub fn room(&self, idx: RoomIdx) -> &Room;
    pub fn spawn_pos(&self, idx: RoomIdx, spawn_id: &str) -> Option<Pos>;
    pub fn door_at(&self, idx: RoomIdx, at: Pos) -> Option<&Door>;
}
```

Runtime code (and, in phase 6, the save file) keeps using strings only where they cross a durability
boundary; inside `GameState` the current room is a `RoomIdx`.

### Loading and error collection

```rust
pub const EMBEDDED: &str = include_str!("../../assets/world.ron");

pub fn parse(src: &str) -> Result<World, Vec<ContentError>>;   // RON parse -> decode rows -> intern -> validate
pub fn load() -> Result<World, Vec<ContentError>>;             // parse(EMBEDDED)
pub fn validate(world: &World) -> Result<(), Vec<ContentError>>;
```

`parse` runs three stages and **collects** rather than short-circuits:

1. **RON parse** — a syntax error is fatal and returns a single `ContentError::Parse`; there is nothing to
   continue with.
2. **Decode** — per room: row count must be 16, every row exactly 24 chars, every char in the table.
   Errors are collected per room; a room that fails to decode is dropped from the `World` handed to stage 3.
3. **Validate** — all structural checks plus reachability, collecting every error.

If stage 2 produced any error, stage 3 still runs its structural checks but **skips the reachability
search** (a world with a missing room would otherwise emit a cascade of route noise). The errors returned
are the union, in a stable order: parse, then decode (by room, then position), then validation (by check,
then id). Stable ordering is what lets tests assert on the list rather than on a set.

### `ContentError` (`content/error.rs`)

One variant per §7 rule, each carrying the ids needed to fix the file, with a `Display` that prints one
readable line (`room.west_grove: spawn 'spawn.west_grove.east' at (3,7) is not walkable (Wall)`):

```rust
pub enum ContentError {
    Parse { message: String },
    RoomDimensions { room: String, expected: (usize, usize), found: (usize, usize) },
    IllegalTile { room: String, at: Pos, ch: char },
    DuplicateId { kind: IdKind, id: String },                  // IdKind = Room|Door|Spawn|Chest|Npc|Puzzle
    UnknownRoom { referenced_by: String, room: String },
    UnknownSpawn { referenced_by: String, room: String, spawn: String },
    PositionOutOfBounds { room: String, what: String, at: Pos },
    SpawnNotWalkable { room: String, spawn: String, at: Pos, tile: Tile },
    SpawnOnDoorTile { room: String, spawn: String, at: Pos },
    DoorTileMismatch { room: String, at: Pos },                 // Door tile with no door, or door off a Door tile
    NonReciprocalDoor { door: String, to_room: String },
    LockNeverUnlockable { door: String, lock: LockKind },
    RoomUnreachable { room: String },
    EmberUnreachable,
    HomeUnreachableWithEmber { home: String },
    EmberMissing,                                               // only when route.ember_required
    DuplicateMapIndex { at: (u8, u8) },
}
```

### The §7 checks, one by one

| §7 rule | Implementation | Error |
|---|---|---|
| map dimensions | 16 rows × 24 chars per room, from `tuning::{ROOM_W, ROOM_H}` | `RoomDimensions` |
| legal tiles | every char in the table | `IllegalTile` |
| unique ids | rooms globally; doors, spawns, chests, npcs, puzzles globally (ids are namespaced, so global uniqueness is the authoring rule); overworld `map_index` unique | `DuplicateId`, `DuplicateMapIndex` |
| transition targets exist | `to_room` resolves, `to_spawn` exists in that room; `start` resolves | `UnknownRoom`, `UnknownSpawn` |
| spawn points correct | in bounds, walkable, non-hazard, not on a door tile | `PositionOutOfBounds`, `SpawnNotWalkable`, `SpawnOnDoorTile` |
| door geometry | every `Door` tile has exactly one door entry at that position and every door sits on a `Door` tile | `DoorTileMismatch`, `PositionOutOfBounds` |
| two-way reciprocity | for a two-way door `A` in `R` targeting spawn `s` in `S`: exactly one two-way door `B` in `S` with `B.to_room == R`, `A`'s target spawn `s` is orthogonally adjacent to `B.at`, and `B`'s target spawn is orthogonally adjacent to `A.at` | `NonReciprocalDoor` |
| keys reachable before locks | the reachability search below | `LockNeverUnlockable` |
| traversable main route | the reachability search below | `RoomUnreachable`, `EmberUnreachable`, `HomeUnreachableWithEmber`, `EmberMissing` |

### Reachability search

A breadth-first search over `(unlocked-door bitmask, item bitmask)` states, memoised on that pair — the
`(room, item-set)` search ADR 0005 specifies, with the door set made explicit because small keys are
fungible and consumable.

```
state    = (unlocked: u64 door bits, items: u32 flag bits)
derive   = flood the room graph from start through unlocked/unlocked-able doors
           -> reachable rooms, reachable chests -> rewards -> keys_found, items'
frontier = every locked door touching a reachable room and not yet unlocked
step     = for each frontier door whose lock is satisfiable now
             (SmallKey: keys_found - keys_spent > 0; Lantern/Flag: the bit is in items')
           push (unlocked | door, items')
```

Branching over *which* door to open is what makes the search correct rather than greedy; memoisation on
`(unlocked, items)` keeps it bounded (15 rooms, a handful of locked doors — well under a millisecond). From
the union of all reachable states the validator concludes:

- a room in no reachable state → `RoomUnreachable` (already meaningful this phase: it catches an orphaned
  room in the 3×3 grid);
- a door in `lock` that is unlocked in no reachable state → `LockNeverUnlockable` (this is "key behind the
  lock it opens");
- if some chest grants `Reward::Ember`: it must be reachable (`EmberUnreachable`), and from its state
  `route.home` must still be reachable (`HomeUnreachableWithEmber`);
- if `route.ember_required` and no chest grants the ember → `EmberMissing`.

`assets/world.ron` sets `route: (ember_required: false, home: "room.lighthouse")` this phase — there is no
dungeon and no ember yet. Phase 5 flips the flag to `true`, which is the one-line switch that turns the
ember rules on for the finished world. The ember machinery is proven *now* by fixtures that do author a
chest with the ember.

### `assets/world.ron` — the 3×3 overworld

Nine rooms, geometry + doors + spawns only. `map_index` is `(col, row)`, `(0,0)` top-left:

```
(0,0) room.north_ridge    (1,0) room.stone_circle   (2,0) room.old_mill
(0,1) room.west_grove     (1,1) room.crossroads     (2,1) room.east_marsh
(0,2) room.fallen_pines   (1,2) room.lighthouse     (2,2) room.south_shore
```

Start: `room.lighthouse` / `spawn.lighthouse.start`. All 12 internal edges carry a two-way door pair (24
doors), so the grid has no dead ends and the "walk every door" test covers every edge. Naming is
mechanical: the door on the north edge of `room.crossroads` is `door.crossroads.north`, and the spawn an
arriving hero lands on is `spawn.crossroads.north`, placed one tile inside the room from the door. Each
room is a wall ring with a doorway at the middle of each shared edge plus a little interior geometry (water,
bushes, wall stubs) so the rooms are visually distinct and collision has something to hit.

### Runtime: `GameState` points into the world

```rust
pub struct GameState {
    pub tick: Tick,
    pub rng: Rng,
    pub hero: Hero,
    #[serde(skip)] pub world: Rc<World>,   // immutable, shared
    pub room: RoomIdx,
    pub progress: Progress,                // { visited: BTreeSet<RoomIdx> } this phase
}

impl GameState {
    pub fn new(seed: u64, world: Rc<World>) -> Self;   // hero at world.start, start room visited
    pub fn room(&self) -> &Room;
}
```

`Rc<World>` keeps the pinned contract `update(&mut GameState, &[Action], tick) -> Vec<GameEvent>` exactly as
`CLAUDE.md` and `ARCHITECTURE.md` state it, and keeps `render::draw(frame, app, theme)` unchanged — the
alternative (`update(&mut GameState, &World, ...)`) would thread a second parameter through `app`, `render`
and every existing test for no gain. `Rc` is `std`, single-threaded by design (ADR 0001), and cloning it is
a refcount bump. `World` is `#[serde(skip)]` so it never enters a save or a state hash.

`Progress` is introduced now with only `visited`; phase 4–6 add `opened_chests`, `unlocked_doors`,
`solved_puzzles`, `flags`. `BTreeSet` (not `HashSet`) so iteration order is deterministic for hashing and
saving.

### Room transitions in `game::update`

`Tile::Door` is walkable, so the existing movement rule already lets the hero step onto it. After a
successful step the update asks `world.door_at(room, hero.pos)`; if there is a door:

1. resolve `to_room` / `to_spawn` (both validator-guaranteed, so this is an infallible lookup with a
   defensive "stay put" fallback rather than an `unwrap` — the simulation is total);
2. set `state.room`, place the hero at the spawn position, keep the current facing;
3. insert the room into `progress.visited`;
4. emit `GameEvent::RoomEntered { room: RoomIdx, first_visit: bool }` after the `HeroMoved` event.

Locked doors are inert this phase (no lock is authored, and no key exists); the lock branch lands in phase 5
along with keys. The hero's step cooldown is charged once for the step, so a transition costs one step, not
two. The start room is marked visited at construction and emits no `RoomEntered` — the event means "a
transition happened", which is what the autosave trigger in phase 6 needs.

### Startup validation

`main` validates before anything touches the terminal, ahead of the existing TTY/`TERM` preflight so a bad
world is reported even in a non-TTY environment (which is what makes the check testable from a spawned
process):

```
main
 └─ Config::from_args
 └─ content preflight  -> Err(Vec<ContentError>) => eprintln one line per error, exit 2   <-- new
 └─ terminal::preflight (TTY/TERM)
 └─ run(): TerminalGuard::enter ...
```

The hidden flag `--debug-content <PATH>` (mirroring `--debug-panic`: `#[arg(long, hide = true)]`, absent
from `--help` and from the documented §12 surface) makes `main` read that file instead of the embedded
string. That is the "test harness entry point" the acceptance criterion asks for: a test spawns
`env!("CARGO_BIN_EXE_mosslight") --debug-content tests/fixtures/broken_ember_unreachable.ron`, asserts a
non-zero exit, asserts stderr lists the errors, and asserts the failure is the *content* message rather than
the TTY message — which proves the content check ran and exited before raw mode could be enabled. Reading
the file is `main`'s job; `content::parse(&str)` stays pure.

### Render

`render/tiles.rs` gains `Kind::{Door, Stairs, Pit}` with the glyphs above; `Hidden` maps to `Kind::Wall`.
`render/scene.rs` reads `state.room()` instead of `state.room`. The HUD gains nothing this phase. Glyphs
stay theme-independent (CLAUDE.md).

### Architecture deviations (also appended to DECISIONS.md)

1. **Tiles are authored as 16 character rows per room**, not as `tiles: [[Tile; 24]; 16]` literal enum
   lists. A 24×16 enum grid is ~384 tokens per room and unreadable as a diff; a char grid *is* the map.
   §7's "legal tiles" check becomes "every char is in the table", which is exactly the check.
2. **`Tile::Door` carries no id**; the door id comes from the room's `doors` list matched by position, and
   the validator enforces the 1:1 correspondence. Carrying an id inside a char cell is impossible, and the
   position match is a stronger invariant than a payload.
3. **`GameState` holds `Rc<World>`** rather than taking `&World` as an `update` parameter, to preserve the
   documented `update` signature and avoid a lifetime through `App`/`render`.
4. **The ember/home rules are gated on `route.ember_required`**, `false` this phase, `true` from phase 5.
   The rules are proven by fixtures now instead of being written later.
5. **The validator does not enforce the §2 content table** (9 overworld + 6 dungeon rooms, 3 NPCs, …). It
   must accept the small fixture worlds; the content-table assertion is a phase-5 test over the real world,
   as the roadmap already schedules.

## Tasks

- [x] **T1** — `src/content/schema.rs`: the serde types above, the tile character table
      (`Tile::from_char`, `Tile::is_walkable`, `Tile::is_hazard`), `RoomIdx`, `Pos` reused from
      `game::entities`. `src/content/mod.rs` + `pub mod content;` in `src/lib.rs`. Unit tests in-module:
      every char maps to a tile and back, walkable/hazard sets are the table above.
- [x] **T2** — `src/content/error.rs`: `ContentError`, `IdKind`, `Display` one line per error, plus
      `pub fn report(errors: &[ContentError]) -> String`. Unit test: `Display` of each variant mentions the
      offending id.
- [x] **T3** — `src/content/loader.rs`: `parse(src)` stages 1–2 (RON parse, row decode, interning,
      `World` lookup helpers), error collection and the stable ordering rule. Unit tests: a 2-room inline
      source parses; a bad row length and an illegal char are both reported from one source.
- [x] **T4** — `assets/world.ron`: the nine rooms, 24 doors, 24+1 spawns, `route` and `start` blocks;
      `loader::EMBEDDED` + `load()`. Test in `tests/content.rs`: `content::load()` is `Ok` and yields 9
      rooms with 9 distinct `map_index` values.
- [x] **T5** — `src/content/validate.rs` part A: ids, map indices, door/spawn targets, bounds, spawn
      walkability/hazard/door-tile, door↔tile correspondence, reciprocity. Unit tests in-module for the
      reciprocity rule (adjacent-spawn pairing accepted, off-by-one rejected).
- [x] **T6** — `src/content/validate.rs` part B: the `(unlocked, items)` BFS; `RoomUnreachable`,
      `LockNeverUnlockable`, `EmberUnreachable`, `HomeUnreachableWithEmber`, `EmberMissing`; the
      "skip reachability when decode failed" rule. In-module test: a two-room world with a `SmallKey` chest
      before a locked door validates, and the same world with the chest behind the door does not.
- [x] **T7** — `tests/fixtures/`: `base.ron` (a valid minimal 2-room world used as the copy source) plus
      `broken_missing_door_target.ron`, `broken_non_reciprocal.ron`, `broken_spawn_in_wall.ron`,
      `broken_duplicate_id.ron`, `broken_dimensions.ron`, `broken_illegal_tile.ron`,
      `broken_key_behind_lock.ron`, `broken_ember_unreachable.ron`, `broken_three_defects.ron`
      (duplicate id + missing door target + spawn in a wall).
- [x] **T8** — `tests/content.rs`: the real world validates; each of the eight fixtures is rejected **with
      its own named variant** (`matches!` on the variant, asserting the id it names); `base.ron` validates
      (so the fixtures differ from valid content by exactly the defect); `broken_three_defects.ron` returns
      three distinct variants in one call.
- [x] **T9** — `src/game/state.rs` + `world.rs` + `mod.rs`: `Rc<World>`, `RoomIdx`, `Progress { visited }`,
      `GameState::new(seed, Rc<World>)`, `room()` accessor; `debug_room()` and the duplicate `Tile`/`Room`
      definitions deleted, `game::world` re-exporting the content types so `crate::game::Tile` keeps
      resolving. Update `src/app.rs` (`App::new`, New Game) and `src/render/{scene,hud,tiles}.rs`. Fix the
      phase-1 tests that used `debug_room` (`tests/movement.rs`, `tests/render.rs` and any other) to build
      from the real world.
- [x] **T10** — Room transitions in `game::update`: door lookup after a step, spawn placement, `visited`
      insertion, `GameEvent::RoomEntered { room, first_visit }`. In-module test: stepping onto a door tile
      changes `state.room` and leaves the hero on a walkable tile.
- [x] **T11** — `tests/transitions.rs`: walk through every door in the grid asserting the resulting room id,
      that the arrival tile is walkable and not a door tile, and that the reciprocal door returns the hero to
      a tile adjacent to the door they left by; the visited set grows by exactly one per newly entered room
      and not at all on re-entry; `RoomEntered` is emitted exactly once per transition (and not on the start
      room); a blocked move into a wall next to a door emits no `RoomEntered`.
- [x] **T12** — `src/config.rs` hidden `--debug-content PATH`; `src/main.rs` content preflight before
      `terminal::preflight`, printing `report(&errors)` to stderr and exiting 2; `tests/content_startup.rs`
      spawning the binary on a broken fixture and on a valid one.
- [x] **T13** — `docs/dev/content.md` (the RON schema, the tile table, the id convention, how to add a room,
      what the validator checks and how to read its output); `CHANGELOG.md` entry.
- [x] **T14** — Run the full gate; fix fmt/clippy findings without weakening any check.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
# combined gate (phase exit condition):
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked
```

Unchanged from phase 1 — no new runner, no new test surface (no PTY harness, per CLAUDE.md).

| Acceptance criterion | Proved by |
|---|---|
| `content::validate` over the real `assets/world.ron` returns `Ok`, in CI | `tests/content.rs::real_world_validates` (plus `real_world_has_nine_rooms_in_a_3x3_grid`) |
| Each of the eight broken fixtures is rejected with the specific expected variant | `tests/content.rs`: `missing_door_target_is_rejected`, `non_reciprocal_door_is_rejected`, `spawn_in_wall_is_rejected`, `duplicate_id_is_rejected`, `wrong_dimensions_is_rejected`, `illegal_tile_is_rejected`, `key_behind_its_own_lock_is_rejected`, `ember_unreachable_is_rejected` — each `matches!` on its variant and asserts the named id; `base_fixture_validates` pins that the fixtures differ only by the defect |
| All errors returned from a three-defect fixture | `tests/content.rs::three_defects_are_all_reported` (asserts three distinct variants in one call) |
| Hero walks every door: resulting room id, walkable spawn tile, reciprocal door leads back | `tests/transitions.rs::every_door_leads_to_its_target_room`, `::arrival_tile_is_walkable_and_not_a_door`, `::reciprocal_door_returns_to_the_origin_door` |
| Visited set grows exactly as rooms are entered; `RoomEntered` once per transition | `tests/transitions.rs::visited_set_grows_once_per_new_room`, `::room_entered_is_emitted_once_per_transition`, `::re_entering_a_visited_room_does_not_grow_the_set` |
| Corrupted embedded world: errors reported, non-zero exit, no raw mode | `tests/content_startup.rs::broken_world_exits_two_with_a_readable_list` (spawns the binary with `--debug-content`, asserts exit code 2, asserts stderr contains the content report and not the TTY message, so the abort happened before `terminal::preflight`) |
| The full phase gate passes | T14; CI matrix (ubuntu-latest + macos-latest) from phase 1 |

Case selection follows `case-taxonomy.md`: happy path (real world validates; every door walked), input and
boundaries (row count, row length, illegal char, out-of-bounds positions), errors (eight named defects plus
multi-error collection — each asserting the message, not just "an error"), state (empty → growing visited
set), idempotency (re-entry does not grow the set; re-entry still emits the event), lifecycle (startup abort
path). Deferred, with reasons: locked-door traversal and key consumption (`deferred_not_authored` — no keys
exist until phase 5, but the validator rule is covered by a fixture), puzzle and NPC schema round-trips
(`deferred_not_authored` — phases 4–5 author them), save round-trip of `visited` (phase 6).

## Risks

| Risk | Touched how | What this plan does |
|---|---|---|
| **#1 Content is authored but unfinishable** (High/High) | This phase *is* the mitigation | The validator lands before any content-heavy phase; the `(unlocked, items)` BFS branches over key choices rather than being greedy; eight fixtures prove each rule actually fires, so a validator that returns `Ok` for everything cannot make the suite green. Startup validation means a bad edit in phase 4/5 fails loudly instead of shipping. |
| **#3 Time runs out mid-content** (High/Medium) | Phase 2 is on the critical path | Geometry-only content: nine rooms of tiles, doors and spawns, no NPCs/items/enemies, so the phase cannot expand into phase-4 content work. The world stays completable (fully connected grid) at the end of this phase. |
| **#16 Extra dependencies creep in** (Low/Low) | A parser is the classic place to add one | Nothing new: `ron` and `serde` are already declared, `Rc` and `BTreeSet` are `std`. No `petgraph`, no `bitflags`. |
| **#10 Monochrome/ASCII legibility** (Medium/Medium) | New tile kinds | Each new tile gets its own ASCII glyph in the single `tiles.rs` table; `Hidden` maps to the wall glyph on purpose. Themes still change colour only. |
| **#13 Clippy debt** (Low/Medium) | New module | T14 runs the gate; error enums get `Display` rather than `#[allow]`, and the door-lookup fallback is an explicit match, not an `unwrap` (CLAUDE.md forbids `unwrap` on the content path). |
| **#7 Save data loss** (Medium/High) | Indirect | Ids are stable namespaced strings and `World` is `#[serde(skip)]` in `GameState`, so the save format (phase 6) is built on ids, not on indices that content reordering would invalidate. |

## Out of scope

- NPCs, dialogue, chests, items, the sword, the lantern, secrets, the map screen (**phase 4**) — schema
  types only, no authored instances.
- Enemy spawns and any AI or combat (**phase 3**).
- The six dungeon rooms, locked doors actually consuming keys, puzzles, the boss, the ember and the ending
  (**phase 5**) — the validator's lock/ember rules exist now but are exercised by fixtures.
- `Tile::Hidden` reveal mechanics and the lantern-lit passage (**phase 4**).
- `src/save.rs`, persisting `Progress` to disk, autosave on room transition (**phase 6**).
- Unicode glyphs for the new tiles (**phase 6**; the ASCII table is authoritative).
- The §2 content-table assertion (9 + 6 rooms, 3 NPCs, 3 enemy kinds, 1 boss) and the headless playthrough
  (**phase 5**).

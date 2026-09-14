# Phase 4 — Overworld: NPCs, chests, sword, lantern, secrets, map and inventory

Goal: fill the nine-room overworld with the content and the pacing §7 asks for, so the player learns
movement, then interaction, then attack, and leaves the overworld holding the sword and the lantern.

## Context

**What exists after phase 3**

| Area | State |
|---|---|
| `src/content/schema.rs` | `World/Room/Door/Spawn/Chest/Npc/EnemySpawn/Puzzle/Route/StartPoint`. `Npc.dialogue` is `Vec<String>`, `Chest.contains: Reward`, `Puzzle` has only `id/kind/reward`. `Tile::Hidden` exists but is inert: not walkable, rendered as a wall. |
| `src/content/validate.rs` | Structural checks (ids, transitions, spawns, door geometry, reciprocity, enemy spawns) plus a `(unlocked, items)` BFS over `Loc = (RoomIdx, component)`, with per-room tile components from `compute_components`. Knows about `Reward::{SmallKey, Lantern, Ember}` and `LockKind::{SmallKey, Lantern, Flag}` — `Flag` locks are always reported as `LockNeverUnlockable`. |
| `src/game/state.rs` | `Progress { visited }` only. `update()` handles movement, facing-on-blocked, enemy-occupancy blocking, door transitions (ignoring `Door.lock`), and `Attack`. `UseLantern / Interact / ToggleMap / ToggleInventory` are accepted and ignored. `state_hash()` is FNV-1a over an explicit field list with a table-driven "every hashed field matters" test. |
| `src/game/entities.rs` | `Hero { pos, facing, step_ready_at, health_halves, max_health_halves, keys, attack, attack_ready_at, invuln_until, died }` — no equipment flags. |
| `src/app.rs` | `Mode { MainMenu, Playing, Paused, ConfirmQuit, Help, TooSmall, GameOver }`; `simulating() == (mode == Playing)`; `apply()` already drops pending actions and suppresses the rest of the batch when an overlay close re-enters `Playing`. |
| `src/render/` | `tiles::Kind` covers tiles, the three enemies, sword and telegraph; `overlays.rs` has main menu, pause, confirm-quit, help, game-over, too-small. |
| `assets/world.ron` | Nine overworld rooms, geometry + doors + phase-3 enemy spawns. No chests, NPCs, torches, plates, locks or dungeon. |

**What this phase changes**

1. Content grows a fifth object family: chests, NPCs, torches and plates become *authored objects that occupy a
   tile*, plus dialogue nodes, room hints and secret marks. One stub dungeon room is authored so the
   lantern-locked entrance has a real target.
2. `Progress` grows `opened_chests`, `lit_torches`, `solved_puzzles`, `flags`; `Hero` grows `has_sword`,
   `has_lantern`, `has_ember`.
3. `update()` grows interaction (`Interact`, `UseLantern`), lock-aware door transitions, torch reveals, the
   `StepPlates` puzzle and a dialogue modal that suspends the per-tick pipeline.
4. `App` grows `Dialogue`, `Map` and `Inventory` modes, all of which pause the simulation.
5. The validator grows object-placement checks, an NPC-condition check, reveal-aware reachability and a
   main-route computation used to prove no secret gates progress.

**Key files:** `src/content/{schema,validate,error}.rs`, `src/game/{state,entities,puzzles,ai,combat,world}.rs`,
`src/app.rs`, `src/input.rs`, `src/render/{tiles,scene,hud,overlays}.rs`, `assets/world.ron`,
`tests/{overworld,dialogue,content,render,mode_machine,movement}.rs`, `tests/common/mod.rs`.

## Design

### Objects occupy tiles; interaction is on the faced tile

The single rule this phase adds to the simulation:

> An authored object (chest, NPC, torch) sits on a `Floor` tile, is **solid** (blocks the hero and every
> enemy), and is acted on by facing it and pressing `Interact` (E/Enter) or `UseLantern` (K). A plate is the
> one authored object that is **not** solid — it is stepped on.

This mirrors the sword hitbox (`the single tile the hero faces`) and §6's "solid objects block movement", and it
removes the need for any proximity/radius rule. It is also what makes the acceptance criterion "the lantern
lights a torch on the faced tile only" a direct assertion.

### Content schema (`src/content/schema.rs`)

```rust
pub struct DialogueNode {
    pub text: String,
    /// Set on the tick this node becomes the shown node.
    #[serde(default)] pub sets_flag: Option<String>,
}

pub struct Npc {
    pub id: String,
    pub at: Pos,
    pub dialogue: Vec<DialogueNode>,   // was Vec<String>
    /// The NPC only talks once this flag is set; `None` means always.
    #[serde(default)] pub condition: Option<String>,
}

pub struct Chest {
    pub id: String,
    pub at: Pos,
    pub contains: Reward,
    /// A secret reward: off the main route, never route-critical (validator-enforced).
    #[serde(default)] pub secret: bool,
}

pub struct Torch {
    pub id: String,
    pub at: Pos,
    /// `Tile::Hidden` positions in this room that become walkable once this torch is lit.
    #[serde(default)] pub reveals: Vec<Pos>,
}

pub struct Plate { pub id: String, pub at: Pos }

pub struct Puzzle {
    pub id: String,
    pub kind: PuzzleKind,          // StepPlates | PushBlock | Switches
    /// Plate ids that must all be pressed (StepPlates).
    #[serde(default)] pub plates: Vec<String>,
    /// `Tile::Hidden` positions revealed when the puzzle is solved.
    #[serde(default)] pub reveals: Vec<Pos>,
    #[serde(default)] pub reward: Option<Reward>,
}

pub struct Room {
    /* … existing …, plus */
    #[serde(default)] pub torches: Vec<Torch>,
    #[serde(default)] pub plates: Vec<Plate>,
    /// Shown in the message row on entry. The §7 teaching prompt of the first room.
    #[serde(default)] pub hint: Option<String>,
}

pub struct Route {
    pub ember_required: bool,
    pub home: String,
    /// The deepest room the main route must reach. Phase 4: the dungeon vestibule.
    pub goal: String,
}
```

`PuzzleKind` gains `StepPlates`. `Reward` is unchanged.

### Runtime identity: `ObjectRef`

`Progress` must name chests, torches and puzzles without depending on `Vec` order across a save (phase 6 writes
strings). Phase 3's precedent (`visited: BTreeSet<RoomIdx>`, deliberately not `Serialize`) is extended:

```rust
/// A room-local authored object: `(room, index into that room's vec)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectRef { pub room: RoomIdx, pub index: u16 }
```

Dense, `Ord` (so `BTreeSet` iteration stays deterministic for hashing), and resolvable back to the authored
string id through `World` when phase 6 defines the on-disk shape. Not `Serialize`, for the same reason
`Progress` is not.

```rust
pub struct Progress {
    pub visited: BTreeSet<RoomIdx>,
    pub opened_chests: BTreeSet<ObjectRef>,
    pub lit_torches: BTreeSet<ObjectRef>,
    pub solved_puzzles: BTreeSet<ObjectRef>,
    /// Story flags, by authored id — the save vocabulary, few and short.
    pub flags: BTreeSet<String>,
}
```

`Hero` gains `has_sword: bool`, `has_lantern: bool`, `has_ember: bool` (the last is written in phase 5 but
hashed and rendered from here, so the inventory screen does not change shape later).

### Reveal-aware walkability

`Tile::Hidden` stops being inert. A hidden tile is walkable iff some lit torch or solved puzzle in that room
lists it in `reveals`:

```rust
impl GameState {
    pub fn is_revealed(&self, room: RoomIdx, at: Pos) -> bool;   // lit torches ∪ solved puzzles of that room
    pub fn object_at(&self, room: RoomIdx, at: Pos) -> Option<ObjectKind>; // Chest | Npc | Torch (solid), Plate (not)
    /// The one walkability predicate the simulation uses from now on.
    pub fn walkable(&self, room: RoomIdx, at: Pos) -> bool;
}
```

`walkable` = in bounds ∧ (`Tile::is_walkable()` ∨ (`Tile::Hidden` ∧ `is_revealed`)) ∧ no solid object.
Every existing call site of `Tile::is_walkable` in the simulation moves to it: hero movement in
`state::update`, `ai::try_move` / the room-local BFS, and `combat::knockback`. `Tile::is_walkable` itself is
unchanged and stays the content-level predicate the validator's geometry checks use.

Render mirrors it: a revealed `Hidden` draws as `Floor`, an unrevealed one still draws as `Wall`.

### Interaction (`src/game/state.rs`)

`Interact` and `UseLantern` resolve `step_target(hero.pos, hero.facing)` and act on what is there:

| Action | Faced tile | Effect |
|---|---|---|
| `Interact` | unopened `Chest` | insert into `opened_chests`, grant `contains`, emit `ChestOpened` + `ItemPicked` + `Message` |
| `Interact` | opened `Chest` | `Message("The chest is empty.")`, nothing granted |
| `Interact` | `Npc` whose `condition` is satisfied | `state.dialogue = Some(DialogueState { npc, node: 0 })`, apply node 0's `sets_flag`, emit `DialogueStarted` (+ `FlagSet`) |
| `Interact` | `Npc` whose `condition` is unsatisfied | `Message` with a short "nothing to say yet" line |
| `Interact` | anything else | no event |
| `UseLantern` | `Torch`, hero has lantern | insert into `lit_torches`, emit `TorchLit`; for each `reveals` position emit `PassageRevealed` + a `Message` |
| `UseLantern` | `Torch`, no lantern | `Message("You have no lantern.")` |
| `UseLantern` | anything else | `Message("Nothing to light here.")` |

`Attack` requires the sword: without `hero.has_sword` it emits `Message("You have no weapon.")` and starts no
swing (and costs no cooldown). This is what makes §7's "teach movement, then interaction, then attack" ramp
structural rather than a content wish.

Rewards: `Sword`/`Lantern`/`Ember` set the hero flag; `SmallKey` increments `hero.keys`; `HeartContainer` raises
`max_health_halves` by 2 (capped at 10 = 5 hearts, §2) and heals by 2; `Message(s)` only emits `Message`.

### Door locks

`resolve_door` becomes lock-aware, in `update`'s movement branch, before the transition:

- `None` → transition, as today.
- `Some(Lantern)` → transition iff `hero.has_lantern`; otherwise the step is refused, `MoveBlocked` and
  `DoorBlocked { lock }` + a `Message` are emitted (the hero still turns to face it).
- `Some(Flag(f))` → transition iff `progress.flags.contains(f)`; same refusal otherwise.
- `Some(SmallKey)` → refused with `DoorBlocked`. **Key consumption is phase 5**; no `SmallKey` door is authored
  before then, so this is not a reachable half-behaviour, and `Progress::unlocked_doors` is deliberately not
  added yet.

### Dialogue: a modal inside the simulation, a mode in the app

`GameState` owns the cursor so the flag writes stay in the pure layer:

```rust
pub struct DialogueState { pub npc: ObjectRef, pub node: u16 }
// GameState { …, pub dialogue: Option<DialogueState> }
```

`update()` gets one early branch: **while `state.dialogue.is_some()`, only `Confirm`/`Cancel` are processed and
the six-step per-tick pipeline does not run at all.** `Confirm` advances to the next node (applying its
`sets_flag`, emitting `DialogueAdvanced` + `FlagSet`); past the last node, and on `Cancel`, the dialogue closes
(`DialogueEnded`). Nothing moves, no timer fires, no damage lands while a dialogue is open — which is exactly
the "simulation paused" requirement, proved by `state.tick` and `state_hash()` staying put.

`App` mirrors it: `GameEvent::DialogueStarted` → `set_mode(Mode::Dialogue)`; `DialogueEnded` → back to
`Mode::Playing`. In `Mode::Dialogue`, `apply()` routes `Confirm`/`Cancel` straight into
`update(&mut self.state, &[action], self.tick_counter)` — the **current** tick, not an incremented one — so
dialogue responds to the keypress in the same iteration and `tick_counter` provably does not advance. The
existing `mode_before != Playing && mode == Playing` rule in `apply()` then fires `drop_pending` and suppresses
the rest of the batch, so the movement key queued behind the closing Enter never steps the hero.

`App::tick()` keeps returning early for every non-`Playing` mode, so `Map`, `Inventory`, `Paused`, `Help`,
`Dialogue` and `ConfirmQuit` all freeze the simulation by the same one-line rule.

### The overworld puzzle: `StepPlates`

`src/game/puzzles.rs` (the module CLAUDE.md's layout already reserves) is created with the one kind §7 step 4
needs:

```rust
/// Transient: which plates of the current room are currently pressed. Cleared on room exit, so an
/// unsolved puzzle resets on re-entry (§6); a solved puzzle lives in `Progress::solved_puzzles`.
pub struct PlateState { pub pressed: BTreeSet<u16> }  // indices into Room::plates

pub fn on_hero_moved(state: &mut GameState, tick: Tick) -> Vec<GameEvent>;
```

Called from `update` right after a successful step: if the hero's new tile holds a plate, mark it pressed
(`PlatePressed` event); if every plate named by an unsolved `StepPlates` puzzle of that room is pressed, insert
it into `solved_puzzles`, apply `reveals`, grant `reward` if any, and emit `PuzzleSolved`. Stepping off a plate
does not release it (pressure holds), so no ordering and no soft-lock is possible — the "wrong state" the case
taxonomy asks about does not exist for this kind. `PushBlock`/`Switches` stay unimplemented here and are phase
5's job.

### Validator (`src/content/validate.rs`)

New checks, each with a fixture:

1. `check_object_placement` — every chest / npc / torch / plate `at` is in bounds, on `Tile::Floor`, not on a
   door tile, not on a spawn tile, not on an enemy-spawn or patrol tile, and at most one object per tile.
   Errors: `ObjectNotOnFloor`, `ObjectTileConflict`, plus `PositionOutOfBounds` and `DuplicateId` reuse
   (`IdKind` gains `Torch`, `Plate`).
2. `check_npc_conditions` — every `Npc.condition` flag and every `LockKind::Flag` flag is set by some
   `DialogueNode.sets_flag` somewhere in the world. Error: `FlagNeverSet { flag }`. A `Flag` lock is therefore
   no longer unconditionally `LockNeverUnlockable`: `traversable` now treats `Flag(f)` as passable once `f` is
   in the search's reachable-flag set, which is grown by the same fixpoint as items.
3. `check_puzzles` — every `Puzzle.plates` id exists in that room; `StepPlates` has at least one plate;
   `reveals` positions are `Tile::Hidden` in that room. Errors: `UnknownPlate`, `RevealNotHidden`.
4. `check_torches` — every `Torch.reveals` position is `Tile::Hidden` in that room (`RevealNotHidden`).
5. `check_secrets` — a chest with `secret: true` may not contain `Sword`, `Lantern` or `Ember`
   (`SecretOnMainRoute` is reserved for the placement rule), and its room may not be in `main_route_rooms`.
   Error: `SecretOnMainRoute { chest }`, `SecretRouteCritical { chest }`.

**Reachability becomes reveal-aware.** `ReachabilitySearch` keeps its `(unlocked, items)` outer BFS, but:

- `Loc` changes from `(RoomIdx, component)` to `(RoomIdx, Pos)` and `flood` becomes a plain orthogonal
  tile BFS across rooms (5 760 nodes worst case for 15 rooms). `compute_components` is deleted. This removes the
  component-renumbering hazard that a reveal-dependent component map would otherwise introduce, and it makes
  "object X is reachable" expressible directly as *"some orthogonal neighbour of X's tile is in the reached
  set"*, which is the right question now that objects are solid.
- `settle`'s fixpoint grows two more monotone sets alongside `items`: **lit torches** (a reachable torch is
  lit once `items.lantern` is true; its `reveals` become walkable on the next iteration) and **flags** (a
  reachable NPC whose `condition` is satisfied contributes all its `sets_flag` values). Both only ever grow, so
  the loop terminates for the same reason the item fixpoint does.
- `walkable_for_search(room, at, lit)` = `Tile::is_walkable()` ∨ (`Hidden` ∧ revealed by a lit torch or by a
  solvable puzzle in that room) — a `StepPlates` puzzle counts as solvable once every one of its plates is in
  the reached tile set.

**Main route.** `pub fn main_route_rooms(world: &World) -> BTreeSet<RoomIdx>` — the union, over every
route-critical target (the rooms holding `Reward::Sword`, `Reward::Lantern`, `Reward::Ember` in a non-secret
chest, plus `route.goal` and `route.home`), of every room lying on **any** shortest path from `start.room` to
that target in the unlocked room graph. Union-of-all-shortest-paths, so there is no arbitrary tie-break and the
result is a deterministic set. Exposed publicly because `tests/overworld.rs` asserts the secrets against it.

### Content (`assets/world.ron`)

The critical path is made structural by **turning the start room into a cul-de-sac**: `door.lighthouse.east`
and `door.lighthouse.west` (and their reciprocals `door.south_shore.west`, `door.fallen_pines.east`, the four
matching `+` tiles and the four now-unused spawns) are removed. From `room.lighthouse` the only exit is north to
`room.crossroads`, which holds the sword and no enemies. The 3×3 ring keeps every room connected.

| Room | Grid | Content |
|---|---|---|
| `room.lighthouse` | (1,2) start | `hint` (movement + interaction prompt); `npc.keeper` → `flag.told_about_sanctuary`; zero enemies; one exit (north) |
| `room.crossroads` | (1,1) | `chest.forest_sword` (Sword); zero enemies |
| `room.west_grove` | (0,1) | first slime |
| `room.stone_circle` | (1,0) | `npc.forester` (hints at the mill plates); bats |
| `room.old_mill` | (2,0) | `puzzle.mill_plates` (`StepPlates`, 3 plates) revealing the alcove holding `chest.mill_lantern` (Lantern); bat + slime |
| `room.east_marsh` | (2,1) | `npc.warden`, `condition: flag.told_about_sanctuary`; `door.dungeon_entrance` with `lock: Lantern` → `room.sanctuary_gate`; slime |
| `room.fallen_pines` | (0,2) | secret 1: `torch.pines` → hidden alcove → `chest.pines_heart` (HeartContainer); slime + bat |
| `room.south_shore` | (2,2) | secret 2: `torch.shore` → `chest.shore_key` (SmallKey); slime |
| `room.north_ridge` | (0,0) | secret 3: `torch.ridge` → `chest.ridge_lore` (Message); guardian |
| `room.sanctuary_gate` | — | new, `kind: Dungeon`, `map_index: None`. The dungeon vestibule: a stairs tile and the reciprocal door back to the marsh. Phase 5 expands the dungeon from here and counts it as one of the six. |

`route.goal = "room.sanctuary_gate"`, `route.home = "room.lighthouse"`, `ember_required: false` (phase 5 flips
both). Phase 3's enemy placement table is otherwise preserved.

### Screens (`src/app.rs`, `src/input.rs`, `src/render/overlays.rs`)

`Mode` gains `Dialogue`, `Map`, `Inventory`. `ToggleMap` / `ToggleInventory` open from `Playing` and close from
their own mode (as does `Cancel`); both go through `set_mode`, so `prev_mode` and the `TooSmall` recovery rule
keep working unchanged. `input::map_key` gains the two new modes to the `Enter/E → Confirm` arm (dialogue advance
and menu confirm, never `Interact`).

- **Map** — the 3×3 overworld grid, one cell per `map_index`. Unvisited rooms render as blank. Visited rooms
  render, in priority order, `@` (current room), `C` (holds an unopened chest), `>` (holds the dungeon
  entrance), `N` (holds an NPC), `.` (plain). Dungeon rooms have no `map_index` and never appear. A legend line
  and the room name are printed below the grid.
- **Inventory** — sword / lantern / ember as `yes|no`, key count, hearts as `n/m`, and the story flags known.
- **Help** — extended with map, inventory and dialogue lines.
- **Dialogue** — a bordered window with the NPC's name, the current node's text wrapped to the box, and an
  explicit `[E] continue` / `[E] close` prompt (§4: "explicit advance step"; ARCHITECTURE's accessibility rule:
  advance on a keypress, never on a timer).

### Render (`src/render/{tiles,scene,hud}.rs`)

`tiles::Kind` gains `Npc`, `Chest`, `ChestOpen`, `Torch`, `TorchLit`, `Plate`, `PlatePressed`. Glyphs, ASCII and
identical in every theme (CLAUDE.md): `N`, `C`, `c`, `t`, `T`, `_`, `=`. Every added object type is
distinguishable by glyph alone, so RISKS #10 stays structurally satisfied.

`draw_scene` paint order becomes tiles (revealed `Hidden` as floor) → plates → objects → telegraph → enemies →
sword → hero. HUD's `Item: none` becomes the real equipment (`Item: lantern` / `sword` / `none`).

### Error handling

No new panics and no new `unwrap` on the content or save paths. Every new authored reference (flag ids, plate
ids, reveal positions) is checked by the validator and therefore cannot be `expect`ed away at runtime: the
simulation resolves them with `Option` and treats a miss as "nothing there", staying total exactly as
`resolve_door` already does for an unresolved door target.

### Architecture conformance

- `src/game/` stays pure: no `std::io`, no `std::time`, no `ratatui`. Dialogue is simulation state precisely so
  that flag writes stay inside `update()`.
- `update(&mut GameState, &[Action], Tick) -> Vec<GameEvent>` keeps its pinned signature.
- `src/render/` only reads state.
- ARCHITECTURE lists `Room { …, plates, torches, blocks, secrets }` and `Npc.dialogue: Vec<DialogueNode>`; this
  phase authors `plates`, `torches` and dialogue nodes as designed. `blocks` is phase 5 (`PushBlock`);
  `secrets` is realised as `Chest.secret` rather than a separate vector — a deviation, logged in DECISIONS.md.
- `Progress` matches ARCHITECTURE's list minus `unlocked_doors` and `boss_defeated`, both phase 5/6.
- Deviations logged in DECISIONS.md: `ObjectRef` instead of interned global ids; `Loc` becoming a tile rather
  than a component; the `StepPlates` puzzle kind; the dialogue-suspends-`update` rule; the sword gate on
  `Attack`; the `room.sanctuary_gate` stub; the removal of the two lighthouse side doors; `GameEvent` variants
  beyond ARCHITECTURE's list.

## Tasks

- [x] **T1: Content schema for the new object families.** `src/content/schema.rs`: `DialogueNode`, `Torch`,
  `Plate`, `Chest.secret`, `Room.{torches, plates, hint}`, `Puzzle.{plates, reveals}`, `Route.goal`,
  `Npc.dialogue: Vec<DialogueNode>`, `PuzzleKind::StepPlates`, `IdKind::{Torch, Plate}`. Re-export through
  `src/game/world.rs` and `src/game/mod.rs`. Unit tests in `schema.rs` for the new accessors
  (`Room::object_at`, `Room::plate_at`).
- [x] **T2: `ObjectRef`, `Progress` and `Hero` equipment.** `src/game/entities.rs` (`ObjectRef`, `has_sword`,
  `has_lantern`, `has_ember`), `src/game/state.rs` (`Progress` fields, `DialogueState`,
  `GameState::{is_revealed, object_at, walkable}`), `state_hash()` extended over every new field. Extend the
  table-driven `mutating_any_hashed_field_changes_the_hash` test with one row per new field.
- [x] **T3: Reveal-aware walkability and solid objects.** Route hero movement (`state::update`), `ai::try_move`
  and the room-local BFS (`ai.rs`), and `combat::knockback` through `GameState::walkable`. Tests in
  `tests/movement.rs`: the hero cannot step onto a chest/NPC/torch tile but can step onto a plate tile; an
  enemy cannot step onto a solid object tile; knockback stops before one.
- [x] **T4: Chests and rewards.** `Interact` on the faced tile in `state.rs`; `ChestOpened`, `ItemPicked`,
  `Message`; reward application including the `HeartContainer` cap; `opened_chests` persistence.
  `tests/overworld.rs`: open-once / reopen-yields-nothing, each `Reward` variant's effect, persistence across
  leaving and re-entering the room.
- [x] **T5: Dialogue in the simulation.** `DialogueState`, the early branch in `update()`, `sets_flag`
  application, `DialogueStarted / DialogueAdvanced / DialogueEnded / FlagSet` events, `Npc.condition` gating.
  `tests/dialogue.rs`: node order, flag set on the sanctuary node, `Cancel` closes early, an unsatisfied
  condition yields a message and no dialogue, and `state_hash()` is unchanged across a whole dialogue except
  for the flag.
- [x] **T6: Dialogue, Map and Inventory modes.** `src/app.rs` (`Mode` variants, `apply_dialogue` calling
  `update` at the current tick, `apply_map` / `apply_inventory`, hint `Message` on `RoomEntered`),
  `src/input.rs` (new modes in the `Confirm` arm). `tests/mode_machine.rs`: opening the map/inventory/dialogue
  freezes `tick_counter` and `state.tick`; closing any of them drops a queued movement action so the hero does
  not step.
- [x] **T7: Lantern and door locks.** `UseLantern` handling, `lit_torches`, `TorchLit` / `PassageRevealed`,
  lock-aware `resolve_door` for `Lantern` / `Flag` / `SmallKey`, `DoorBlocked`. `tests/overworld.rs`: the
  faced-tile-only rule, the passage opening only after the marked torch is lit, the dungeon entrance blocked
  without the lantern and open with it.
- [x] **T8: `src/game/puzzles.rs` — `StepPlates`.** `PlateState`, `on_hero_moved`, `PlatePressed`,
  `PuzzleSolved`, reveal application, reset-on-room-exit for the transient pressed set, persistence via
  `solved_puzzles`. `tests/overworld.rs`: solving in any plate order, leaving mid-puzzle resets the pressed set,
  a solved puzzle stays solved across re-entry.
- [x] **T9: Validator extensions.** `src/content/error.rs` (new variants), `src/content/validate.rs`
  (`check_object_placement`, `check_npc_conditions`, `check_puzzles`, `check_torches`, `check_secrets`, the
  tile-level `Loc` refactor, the lit-torch/flag fixpoint, `main_route_rooms`). New fixtures under
  `tests/fixtures/`: object on a wall, two objects on one tile, unknown plate id, reveal position that is not
  `Hidden`, flag never set, secret on the main route, secret holding the lantern. `tests/content.rs` asserts the
  specific error variant for each.
- [x] **T10: Render the new objects and screens.** `src/render/tiles.rs` (new `Kind`s + glyphs),
  `src/render/scene.rs` (revealed `Hidden` as floor, object paint order), `src/render/hud.rs` (real item),
  `src/render/overlays.rs` (`draw_dialogue`, `draw_map`, `draw_inventory`, extended help). `tests/render.rs`:
  object glyph placement at 60×24, the map overlay showing only visited rooms with the current one marked, the
  dialogue and inventory overlays, and the existing "identical characters under every theme" test extended to a
  scene containing the new objects.
- [x] **T11: Author `assets/world.ron`.** Remove the two lighthouse side doors, their reciprocals, `+` tiles and
  spawns; add the start-room hint; the three NPCs and their dialogue; the sword, lantern and three secret
  chests; three torches with their hidden alcoves; the mill plates and puzzle; the lantern-locked dungeon
  entrance and the `room.sanctuary_gate` vestibule; `route.goal`. `content::validate` must return `Ok`
  (`tests/content.rs`).
- [x] **T12: `tests/common/mod.rs` — the headless driver.** A helper that runs `App::apply` + `App::tick` at
  30 Hz, plus `walk_to(app, pos)` (room-local BFS over `GameState::walkable`, one movement action per step) and
  `face(app, dir)`. Phase 5's `tests/playthrough.rs` reuses it unchanged.
- [x] **T13: `tests/overworld.rs` — pacing and milestones.** The start-to-lantern run using only ordinary
  `Action`s asserting the milestone order; zero enemies in the start room and none reachable before the sword
  chest; exactly three secret chests, all reachable, none in `main_route_rooms`.
- [x] **T14: Adapt phase-3 tests to the sword gate and update the docs.** Give the hero `has_sword = true` in
  the `tests/combat.rs` / `tests/ai.rs` / `tests/determinism.rs` fixture helpers (no assertion is weakened —
  only the precondition the new gate introduces is established). Update `docs/user/controls.md` (map,
  inventory, dialogue advance, lantern), `docs/dev/loop-and-modes.md` (the three new modes and the
  dialogue-suspends-`update` rule), `docs/dev/content.md` (the new schema) and `CHANGELOG.md`.
- [x] **T15: Full gate.** `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings &&
  cargo test --locked`, then `cargo build --release --locked`.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

| # | Acceptance criterion | Test that proves it |
|---|---|---|
| 1 | Start→lantern using only ordinary Actions, milestone order asserted (including a first slime encounter) | `tests/overworld.rs::main_route_milestones_happen_in_order` (T13) |
| 2 | Start room has zero enemy spawns; no enemy spawn reachable before the sword chest | `tests/overworld.rs::start_room_and_crossroads_have_zero_enemies_and_no_enemy_is_reachable_before_the_sword` (T13) — a structural BFS from the start room, not a two-room spot check (round-2 review, major) |
| 3 | Lantern lights a torch on the faced tile only; a hidden passage opens only after the marked torch is lit | `tests/overworld.rs::lantern_lights_only_the_faced_torch_and_opens_its_passage` (T7) |
| 4 | Dungeon entrance impassable without the lantern, open with it | `tests/overworld.rs::dungeon_entrance_needs_the_lantern` (T7) |
| 5 | Each chest opens exactly once; reopening yields nothing | `tests/overworld.rs::chest_opens_once_and_reopening_yields_nothing` (T4) |
| 6 | Map renders only visited rooms, marks the current one, and pauses the simulation | `tests/render.rs::{map_overlay_hides_unvisited_rooms_and_marks_the_current_one, map_overlay_reveals_a_visited_non_current_room_with_its_unopened_chest}` (T10) + `tests/mode_machine.rs::opening_the_map_freezes_the_tick` (T6) |
| 7 | Closing a dialogue or a menu drops queued movement actions | `tests/mode_machine.rs::{closing_an_overlay_drops_a_queued_move_from_the_same_batch, closing_a_dialogue_drops_a_queued_move_from_the_same_batch}` and `tests/dialogue.rs::closing_a_dialogue_drops_a_queued_movement_action_from_the_same_batch` (T5/T6) |
| 8 | Three secret rewards exist, are reachable, none on the main-route BFS path | `tests/overworld.rs::{three_secrets_exist_and_are_off_the_main_route, every_secret_chest_is_reachable_once_its_torch_is_lit}` (T13) |
| 9 | `content::validate` still passes over the expanded world | `tests/content.rs::real_world_validates` (existing, re-run in T11) |
| 10 | The full phase gate passes | the four commands above (T15) |

Round-2 review also required, beyond this table: `state_hash()`'s table-driven guard extended with
one row per new hashed field (`src/game/state.rs::mutating_any_hashed_field_changes_the_hash`); the
map excluding secret chests (`tests/render.rs::map_overlay_does_not_mark_a_room_whose_only_chest_is_secret`);
every glyph proven distinct, not just non-null (`tiles.rs::every_glyph_is_distinct`); the mono theme's
white/black-family claim actually checked (`theme.rs::mono_theme_only_uses_white_or_black_family_colors`);
a scene per new object kind/state variant checked under every theme
(`tests/render.rs::object_kind_glyphs_render_identically_under_every_theme`); `route.goal` validated
like `route.home`, with a fixture; a chest pickup reporting itself in the message row, not just
`ItemPicked`.

Additional cases from the taxonomy, beyond the acceptance list: `UseLantern` without the lantern and facing a
non-torch tile; `Interact` facing empty floor; dialogue cancelled at the first node; the `HeartContainer` cap at
5 hearts; the plate puzzle solved in every plate order and reset by leaving the room mid-solve; an NPC whose
condition is unsatisfied; `state_hash()` covering each new `Progress` field; every new validator error variant
against its own fixture.

## Risks

| Row | How this phase touches it | What the plan does |
|---|---|---|
| **#1 Content authored but unfinishable** | The world roughly triples in authored objects and gains its first locks, hidden passages and puzzle. | Every new authored reference gets a validator check *in this phase* (phase 3's rule): object placement, plate ids, reveal positions, flag ids, secret placement. Reachability becomes reveal- and flag-aware, so a passage that no reachable torch opens is a build error, not a discovery. Seven new broken fixtures, each asserting its own error variant. |
| **#2 We build the wrong game (pacing)** | This is the phase where the §7 ramp either lands or does not. | The ramp is content structure: the start room is a cul-de-sac with one exit, a hint and no enemies; the sword is in the only room that exit leads to; `Attack` without the sword does nothing. Criterion 2's test fails if any enemy is authored into a room reachable before the sword chest, so the ramp cannot be regressed silently. |
| **#3 Time runs out mid-content** | The overworld is fully authored here; the dungeon is not. | `room.sanctuary_gate` is authored as a real, reachable, validating room so the build stays completable-to-the-gate at the end of this phase, and phase 5 starts from a working door rather than from a stub. |
| **#6 Input feels wrong over SSH** | Two more overlay modes and a dialogue that takes `Confirm`. | Dialogue and the two new screens reuse the existing `apply()` close rule (`drop_pending` + suppress the rest of the batch) rather than adding a second path; criterion 7 tests it for both a menu and a dialogue. No key-release, no chord, no extended protocol is introduced. |
| **#10 Monochrome unreadable** | Seven new object kinds on screen. | Each gets a distinct ASCII glyph in the single `tiles::glyph` table; themes still change colour only, and the existing "identical characters under every theme" test is extended to a scene holding the new objects. |
| **#11 Layout breaks at 60×24** | Three new overlays. | All three use the existing `centered_box` helper and are sized to fit inside 60×24; `tests/render.rs` renders each at 60×24. |
| **#14 Enemy AI gets stuck** | Solid objects and revealed passages change what enemies can walk on. | `ai::try_move` and the room-local BFS move to the same `GameState::walkable` predicate as the hero, so an object can never become a tile the pathfinder believes in; phase 3's 1000-tick liveness test keeps running against the re-authored rooms. |

## Out of scope

- The six dungeon rooms beyond the `room.sanctuary_gate` vestibule, small-key doors and key **consumption**,
  `Progress::unlocked_doors` — phase 5.
- `PuzzleKind::PushBlock` / `Switches`, pushable blocks, the torch-sequence puzzle — phase 5.
- The boss, the ember, `GameWon` and the victory screen — phase 5.
- `tests/playthrough.rs` (start-to-victory) — phase 5; this phase only builds the driver it will use.
- The save file, autosave triggers, repointing the death checkpoint at the save — phase 6.
- Themes, palettes, `--unicode`, `NO_COLOR` presentation semantics — phase 6.
- Balance of the new rewards against the 30–45 minute target, and the measurement harness — phase 7.

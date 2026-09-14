# Phase 5 — Dungeon, keys, puzzles, two-phase boss and the ending

Goal: finish the game. Six dungeon rooms behind the lantern-locked gate, two small keys and two locked
doors, the two authored puzzle kinds, a two-phase telegraphed boss that drops the ember, and the return
to the lighthouse — all proved by a headless start-to-victory run that only presses keys.

## Context

**What exists after phase 4**

| Area | State |
|---|---|
| `assets/world.ron` | 10 rooms: the nine-room 3×3 overworld plus the `room.sanctuary_gate` dungeon stub. Sword (`room.crossroads`), lantern (`room.old_mill`, behind the `StepPlates` puzzle), 3 NPCs, 3 secret chests (lore, heart, small key), the `Lantern`-locked `door.east_marsh.dungeon_entrance`. `route: (ember_required: false, home: "room.lighthouse", goal: "room.sanctuary_gate")`. |
| `src/content/schema.rs` | `World/Room/Door/Spawn/Chest/Npc/DialogueNode/Torch/Plate/Puzzle/EnemySpawn/Route`. `PuzzleKind = StepPlates \| PushBlock \| Switches` (only `StepPlates` implemented and used). `ObjectKind = Chest \| Npc \| Torch` — solid; `Plate` is the non-solid one. `LockKind::SmallKey` is authorable but never authored. |
| `src/content/validate.rs` | All §7 structural checks plus a branching `(unlocked-bitmask, items)` BFS at **tile** granularity, with an inner fixpoint over reachable chests / lantern-lit torches / NPC flags / `StepPlates` solves. `SmallKey` doors already get one bit each. `main_route_rooms` = union of all shortest paths to the Sword/Lantern/Ember chest rooms plus `route.goal`/`route.home`; `check_secrets` rides on it. |
| `src/game/state.rs` | `Progress { visited, opened_chests, lit_torches, solved_puzzles, flags }`. `update()` six-step pipeline; `resolve_door` returns `DoorCheck::{None, Blocked, Open}` and hard-codes `SmallKey => false` ("phase 5"). `apply_reward` handles every `Reward` including `Ember`. `state_hash` is FNV-1a over an explicit field list, guarded by a table-driven "mutating any hashed field changes the hash" test. |
| `src/game/entities.rs` | `Hero { …, keys, has_sword, has_lantern, has_ember }`. `AiState` is one flat enum with the Slime/Bat/Guardian variants. `ObjectRef { room, index }`. |
| `src/game/ai.rs` | `step()` dispatches per `EnemyKind`; `step_guardian` is the existing Patrol → Telegraph → Dash → Recover machine, i.e. the shape the boss copies. |
| `src/game/combat.rs` | `resolve_swing` (guardian deflects outside `GuardianRecover`), `apply_contact_damage` (any live enemy adjacent to or on the hero), `knockback`. |
| `src/game/puzzles.rs` | `PlateState { pressed }` and `on_hero_moved` — `StepPlates` only. |
| `src/app.rs` | `Mode { MainMenu, Playing, Paused, ConfirmQuit, Help, TooSmall, GameOver, Dialogue, Map, Inventory }`. `tick()` folds `GameEvent`s into mode/message/checkpoint. |
| `src/render/` | `tiles::Kind` has 20 kinds with a "no two kinds share a glyph" test; `scene.rs` draws plates, torches, chests, NPCs, the guardian telegraph lane and the sword. |
| `tests/common/mod.rs` | The headless driver `new_game/step/idle/face/walk_to` plus `Runner` (raw `GameEvent`s, `cross_door`, `walk_to`) — written in phase 4 explicitly for this phase to reuse unchanged. |

**What this phase changes**

1. **Content grows the last two object families**: pushable `Block`s and the lighthouse `Beacon`; `Puzzle`
   grows `torches` (ordered) alongside `plates`; `EnemySpawn` grows `drops`/`defeat_flag` for the boss.
2. **`Progress` grows `unlocked_doors`**, and `resolve_door` finally consumes a `SmallKey`.
3. **`game/puzzles.rs` becomes the puzzle module it is named for**: `PuzzleState` carries the transient
   plate/block/torch-sequence state, `BlockOnPlates` and `TorchSequence` join `StepPlates`.
4. **The boss** is a fourth `EnemyKind` with its own state machine in `ai.rs` and its own damage path in
   `combat.rs`; it never deals contact damage.
5. **The ending**: a `Beacon` in `room.lighthouse`, `GameEvent::GameWon`, `Mode::Victory`.
6. **The validator** learns blocks (a real one-block push search), torch sequences, the boss drop and the
   beacon, so the finished 15-room world is machine-proved completable.
7. **`assets/world.ron`** gains five dungeon rooms and the beacon; `route` becomes
   `(ember_required: true, home: "room.lighthouse", goal: "room.boss_arena")`.

**Key files:** `src/content/{schema,validate,error}.rs`, `src/game/{puzzles,state,entities,tuning,ai,combat}.rs`,
`src/app.rs`, `src/input.rs`, `src/render/{tiles,scene,overlays,mod}.rs`, `assets/world.ron`,
`tests/{dungeon,puzzles,boss,playthrough,content,render,mode_machine,determinism}.rs`, `tests/fixtures/`.

## Design

### Naming: the authored puzzle kinds become the architecture's names

`PuzzleKind::{PushBlock, Switches}` are renamed to `{BlockOnPlates, TorchSequence}`, matching
`.autodev/ARCHITECTURE.md` and the phase deliverables. Neither variant appears in `assets/world.ron` today,
so the rename costs nothing and stops the schema and the design docs from disagreeing about the same two
puzzles. `StepPlates` keeps its name (authored in `room.old_mill`, and it is a genuinely third, simpler kind).

### Content schema additions (`src/content/schema.rs`)

```rust
pub struct Block { pub id: String, pub at: Pos }
pub struct Beacon { pub id: String, pub at: Pos }

pub enum ObjectKind { Chest(u16), Npc(u16), Torch(u16), Beacon(u16) }  // all solid

pub struct Puzzle {
    pub id: String,
    pub kind: PuzzleKind,
    #[serde(default)] pub plates: Vec<String>,   // StepPlates + BlockOnPlates
    #[serde(default)] pub blocks: Vec<String>,   // BlockOnPlates: exactly one (validator-enforced)
    #[serde(default)] pub torches: Vec<String>,  // TorchSequence: the required order
    #[serde(default)] pub reveals: Vec<Pos>,
    #[serde(default)] pub reward: Option<Reward>,
}

pub struct EnemySpawn {
    pub kind: EnemyKind,                          // + EnemyKind::Boss
    pub at: Pos,
    #[serde(default)] pub patrol: Option<Vec<Pos>>,
    #[serde(default)] pub drops: Option<Reward>,       // Boss only
    #[serde(default)] pub defeat_flag: Option<String>, // Boss only
}

pub struct Room { /* … */ #[serde(default)] pub blocks: Vec<Block>, #[serde(default)] pub beacons: Vec<Beacon> }
```

A `Block` is **not** in `ObjectKind`: its position is mutable simulation state, not authored placement, so it
lives in `PuzzleState.blocks` at runtime and `Room::blocks` only supplies the reset positions. A `Beacon` is
an ordinary solid object, interacted with exactly like a chest — faced tile plus `Interact`.

`EnemyKind::Boss` is a variant of the same enum (as `.autodev/ARCHITECTURE.md` specifies), not a separate
authored family: it reuses `Room::enemies`, `spawn_enemies`, `EnemyId`, the swing hit-list, `revealed_set`
and every existing occupancy rule. "Three *regular* kinds" is then a content assertion (`tests/dungeon.rs`),
not a type-level one.

### Transient puzzle state (`src/game/puzzles.rs`)

`PlateState` becomes:

```rust
pub struct PuzzleState {
    pub pressed: BTreeSet<u16>,   // plate indices held down
    pub blocks: Vec<Pos>,         // live position of Room::blocks[i]
    pub sequence: Vec<u16>,       // torch indices lit so far, in order
}
impl PuzzleState { pub fn for_room(room: &Room) -> Self }   // blocks seeded from the authored positions
```

`GameState::plates: PlateState` becomes `GameState::puzzle: PuzzleState`, and `GameState::spawn_enemies` is
renamed `GameState::enter_room` (it already reset plates and rebuilt enemies; it now also reseeds blocks and
clears the sequence, and the old name no longer describes it). This is the whole reset rule:

> Everything a puzzle changes inside a room is transient and reseeded from the authored content on every
> room entry. Only `Progress::solved_puzzles` survives, and with it the puzzle's `reveals` and `reward`.

So "leaving and re-entering resets an unsolved puzzle" and "a solved puzzle stays solved" are the same one
line of code, and **the reset is also the anti-soft-lock guarantee**: any wrong state a player can reach is
undone by walking out of the room and back in.

### BlockOnPlates

Hero movement in `update()` gains one branch, before the ordinary blocked case:

> If the faced tile holds a block and the hero's step cooldown has elapsed, try to push. The push target is
> `block.pos + facing`. It is legal only if that tile is in bounds, is plain `Tile::Floor` **or** a `Plate`'s
> floor tile, holds no solid object, no other block and no live enemy. Door, stairs, water, pit, bush, wall
> and unrevealed `Hidden` tiles are all illegal push targets. On a legal push the block moves one tile, the
> hero steps into the vacated tile (paying the normal step cooldown), and `BlockPushed` is emitted. On an
> illegal push nothing moves and `MoveBlocked` is emitted, exactly as for any other solid.

Excluding `Tile::Door`/`Tile::Stairs` as push targets is what stops a block ever sealing a room's exit; the
room reset covers everything else. The predicate is a single shared function,
`puzzles::block_push_target(room, state_blocks, occupied, block_pos, facing) -> Option<Pos>`, called by both
the simulation and the validator, so the two can never disagree about what a player can do.

After a push (and after any ordinary hero step, as today) `puzzles::on_hero_moved` re-evaluates the room's
puzzles. A `BlockOnPlates` puzzle is solved when every plate it names holds a block. Plates keep their
existing "pressure holds" semantics for `StepPlates`; for `BlockOnPlates` the check is positional (block on
plate) so pushing a block *off* a plate before the puzzle completes un-presses it — visible feedback, and
still not a soft-lock because the block can be pushed back or the room re-entered.

Events: `BlockPushed { index, from, to }`, plus the existing `PlatePressed` / `PuzzleSolved` /
`PassageRevealed`.

### TorchSequence

A torch named by a `TorchSequence` puzzle is a **sequence torch**: lighting it is transient
(`PuzzleState.sequence`), not a `Progress::lit_torches` entry, and the validator forbids it from carrying
`reveals` of its own (the puzzle owns the reveal). Every other torch keeps today's permanent behaviour.

`handle_use_lantern` on a sequence torch of an unsolved puzzle:

- it is the next id in `puzzle.torches` → push its index, emit `TorchLit`; if the sequence is now complete,
  solve the puzzle (reveals + reward + `PuzzleSolved`);
- it is any other torch of that puzzle → clear the sequence, emit `PuzzleReset { puzzle }` and a message
  ("The flames gutter out.").

Wrong order therefore costs nothing but a retry — the puzzle has no unrecoverable configuration at all, which
is what the "every wrong state" acceptance test asserts directly. Once solved, all of the puzzle's torches
render lit.

### Keys and locked doors

`Progress` grows `unlocked_doors: BTreeSet<ObjectRef>` (`ObjectRef { room, index into Room::doors }`).
`DoorCheck` grows one variant so the read-only `resolve_door` stays read-only:

```rust
enum DoorCheck { None, Blocked(LockKind), NeedsKey(ObjectRef, Transition), Open(Transition) }
```

- already in `unlocked_doors` → `Open`;
- `hero.keys > 0` → `NeedsKey`; `update()` does `hero.keys -= 1` (guarded by the `> 0` test, so the `u8`
  can never wrap), inserts the `ObjectRef`, emits `DoorUnlocked { door }`, then performs the transition;
- otherwise → `Blocked(SmallKey)` with today's message.

On unlocking, the **reciprocal** door (the door in the target room whose `to_room` is the room just left —
the pairing `check_reciprocity` already validates) is inserted into `unlocked_doors` too, so returning
through the same doorway is free from either side.

### The boss

Constants in `game/tuning.rs`, ticks only:

| Constant | Value | Meaning |
|---|---|---|
| `BOSS_HP` | 8 | eight sword hits total |
| `BOSS_PHASE_TWO_HP` | 4 | phase 1 → 2 threshold (`hp <= 4`) |
| `BOSS_P1_STALK_TICKS` / `BOSS_P2_STALK_TICKS` | 60 / 45 | stalk before committing to an attack |
| `BOSS_P1_STEP_TICKS` / `BOSS_P2_STEP_TICKS` | 12 / 8 | stalk movement interval |
| `BOSS_P1_WINDUP_TICKS` / `BOSS_P2_WINDUP_TICKS` | 24 / 18 | telegraph hold; both ≥ `TELEGRAPH_MIN_TICKS` (18 = §6's 600 ms floor) |
| `BOSS_P1_VULNERABLE_TICKS` / `BOSS_P2_VULNERABLE_TICKS` | 45 / 30 | the only window the boss takes damage |
| `BOSS_STRIKE_DAMAGE_HALVES` | 1 | §6's "typical damage: half a heart" |

State machine (flat `AiState` variants, same style as the guardian):

```
BossStalk { phase, until } --(until, or hero on the pattern)--> BossWindup { phase, until, pattern }
BossWindup --(until)--> BossStrike { phase, pattern }        // exactly one tick
BossStrike --> BossVulnerable { phase, until } --(until)--> BossStalk { phase, … }
```

`pattern` is `BossPattern::{Slam, Sweep}` — **phase 1 always slams, phase 2 always sweeps**, which is the
"distinct attack pattern per phase" requirement, asserted on the tile sets rather than on the variant name:

- `Slam`: the boss tile and its four orthogonal neighbours. Punishes standing next to the boss; you step one
  tile out and back.
- `Sweep`: the full row and the full column through the boss, each stopped at the first wall. Punishes
  standing on either axis; you step diagonally off both.

The phase change is checked at the top of `step_boss`: `phase == 1 && hp <= BOSS_PHASE_TWO_HP` enters
`BossStalk { phase: 2, … }` and emits `BossPhaseChanged { id, phase: 2 }` — interrupting a windup, so the
transition can never strand a telegraph without a strike.

**The boss deals no contact damage.** `combat::apply_contact_damage` skips `EnemyKind::Boss`. This is not a
softening: the sword hits the tile the hero faces, so hitting the boss *requires* standing adjacent to it,
and a contact-damaging boss would make the scripted no-damage fight impossible by construction and turn the
fight into the damage race §6 forbids. All boss damage comes from telegraphed strikes.

Strike resolution lives in `combat.rs` (which already owns every hero-damage path), as
`combat::apply_boss_strike(state, tick)`, inserted into the `update` pipeline between `ai::step` and
`apply_contact_damage`:

```
resolve_swing -> ai::step -> apply_boss_strike -> apply_contact_damage -> death -> expire_swing
```

It finds any live boss whose `ai` is `BossStrike` this tick, computes the tile set from
`(pattern, boss.pos, room)`, and if the hero stands on one and is not invulnerable, applies
`BOSS_STRIKE_DAMAGE_HALVES`, knockback and `INVULN_TICKS` — the same three effects contact damage applies.

`combat::resolve_swing` gains one clause: a boss outside `BossVulnerable` deflects (`AttackDeflected`),
exactly as a guardian outside `GuardianRecover` does. At zero HP the boss emits `BossDefeated { id }`, sets
its authored `defeat_flag` (`FlagSet`), and grants its authored `drops` through the existing `apply_reward`
(so the ember pickup emits the same `ItemPicked`/`Message` pair as every other reward).

`enter_room` skips a boss spawn whose `defeat_flag` is already set, so a defeated boss does not respawn while
ordinary enemies still do (spec §10).

Events: `BossPhaseChanged { id, phase }`, `BossTelegraph { id, tiles }`, `BossStruck { tiles }`,
`BossDefeated { id }`. `BossTelegraph`/`BossStruck` carry the tiles so the telegraph-lead-time test measures
the real thing (tick of the telegraph for a tile set vs. tick of the strike on that same tile set) rather
than trusting a constant.

### The ending

`Interact` facing a `Beacon`:

- without the ember → `Message("The brazier is cold; the ember is not yours yet.")`;
- with the ember → set `flag.lighthouse_relit`, emit `GameEvent::GameWon`.

`App` gains `Mode::Victory` (already named in `.autodev/ARCHITECTURE.md`'s mode list): `GameWon` switches to
it, it does not simulate, `Confirm`/`Cancel` return to the main menu, `Quit` routes through `ConfirmQuit`.
`render::overlays::draw_victory` is a centred box like `draw_game_over`. `input::map_key` gets its
`Mode::Victory` arm (the `match` is exhaustive, so this is compiler-enforced).

### Render

New `tiles::Kind`s with distinct ASCII glyphs (the "no two kinds share a glyph" test enforces it):
`Boss` = `W`, `BossVulnerable` = `w`, `Block` = `O` (spec §4's movable-block symbol), `Beacon` = `*`.
`scene.rs` additions: blocks from `state.puzzle.blocks`; beacons; `Kind::Telegraph` over every tile of a
`BossWindup` pattern (the same `!` the guardian lane already uses); a torch counts as lit if it is in
`progress.lit_torches`, in `puzzle.sequence`, or belongs to a solved `TorchSequence`; a plate counts as
pressed if it is in `puzzle.pressed` **or** a block stands on it.

### Validator (`src/content/validate.rs`, `error.rs`)

Structural, all collected rather than short-circuited, as today:

- ids: `Block` and `Beacon` join `check_unique_ids` (`IdKind::{Block, Beacon}`).
- placement: blocks and beacons join `check_object_placement` — on plain `Tile::Floor`, one object per tile.
- `check_puzzles`, per kind: `BlockOnPlates` names exactly one block and at least one plate, all resident in
  the same room; `TorchSequence` names ≥ 2 torches of the same room, no duplicates, and none of them carries
  its own `reveals`; a torch or block may be claimed by at most one puzzle.
- `check_boss` (new): at most one `EnemyKind::Boss` spawn world-wide; `drops`/`defeat_flag` only on a boss
  spawn; a boss that drops the ember must carry a `defeat_flag`. The boss's `defeat_flag` counts as a
  settable flag for `check_npc_conditions`' `FlagNeverSet`.
- `check_beacon` (new): exactly one beacon world-wide, and it must live in `route.home`.

Reachability — the `settle` fixpoint gains three producers:

- **TorchSequence**: solved once the lantern is held and every one of its torches has an orthogonal
  neighbour in `reached`. Order is irrelevant to reachability precisely because a wrong order resets and
  costs nothing.
- **BlockOnPlates**: an exact one-block push search. BFS over `(block_pos, hero_pos)` seeded from every
  reached hero tile of the room, where an edge is either a hero step (blocked by the block) or a push
  through the shared `puzzles::block_push_target`. Solved iff some reachable `block_pos` is one of the
  puzzle's plates. Bounded by `ROOM_W * ROOM_H` squared per puzzle (≈ 147k states, milliseconds), and exact
  because the schema forbids a second block in the same puzzle. `ContentError::BlockPuzzleUnsolvable`
  otherwise.
- **Boss drop**: a boss spawn with an orthogonal neighbour in `reached` grants its `drops` (the ember) and
  inserts its `defeat_flag`. The validator cannot simulate the fight, so this asserts what it can — that the
  arena is reachable and the reward and flag are wired — and `tests/boss.rs` plus `tests/playthrough.rs`
  assert the fight itself.

`ember_authored`, the `Items::ember` producer and `main_route_rooms`' route-critical target set all learn
about a boss drop alongside a chest, so `EmberUnreachable` / `HomeUnreachableWithEmber` and the
secret-off-the-main-route rule keep working with the ember behind the boss.

### The dungeon (`assets/world.ron`)

Six rooms, a linear chain behind the existing lantern-locked gate — every room on the main route, which is
why none of them holds a secret (the three overworld secrets already satisfy §2's "≥ 3"):

| # | Room | Content | Exit |
|---|---|---|---|
| 1 | `room.sanctuary_gate` (exists) | safe landing, hint; no enemies | south → flooded hall |
| 2 | `room.flooded_hall` | water channels, 2 slimes + 1 bat, `chest.hall_key` (`SmallKey`, non-secret) | east → plate chamber, **`SmallKey`-locked** |
| 3 | `room.plate_chamber` | `puzzle.chamber_block` (`BlockOnPlates`: `block.chamber` + `plate.chamber`), 1 slime; solving reveals the `?` tiles in front of the east door | east → torch vault |
| 4 | `room.torch_vault` | `puzzle.vault_torches` (`TorchSequence`: moss → water → stone, order in the room `hint`), 2 bats; solving reveals the alcove holding `chest.vault_key` (`SmallKey`, non-secret) | south → warden walk |
| 5 | `room.warden_walk` | 2 guardians on patrol routes | east → boss arena, **`SmallKey`-locked** |
| 6 | `room.boss_arena` | the boss only (`drops: Some(Ember)`, `defeat_flag: Some("flag.boss_defeated")`) | west → warden walk |

Two keys, two locked doors, both keys strictly upstream of their locks. The overworld's secret `SmallKey` is
surplus, so the route never depends on a secret. `room.lighthouse` gains `beacon.lighthouse`; `route`
becomes `(ember_required: true, home: "room.lighthouse", goal: "room.boss_arena")`.

### Architecture conformance

Every module keeps its current role: `game/` stays pure (ticks only, no `std::io`/`std::time`/`ratatui`), the
boss is a state machine in `ai.rs` next to the other three, hero damage stays in `combat.rs`, rendering only
reads. No new module and no new dependency. Three named deviations from `.autodev/ARCHITECTURE.md`, each
appended to `DECISIONS.md`: `PuzzleKind` renames (toward the architecture's names), boss defeat represented
as a story flag instead of `Progress::boss_defeated: bool`, and the boss dealing no contact damage.

## Tasks

- [x] **T1 — Schema: blocks, beacons, boss spawns, puzzle fields.** `src/content/schema.rs`: add `Block`,
  `Beacon`, `Room::{blocks, beacons}` (+ `PartialEq`), `ObjectKind::Beacon`, `Room::{block_index,
  block_at, beacon_at}`, `EnemyKind::Boss`, `EnemySpawn::{drops, defeat_flag}`, `Puzzle::{blocks, torches}`;
  rename `PuzzleKind::{PushBlock, Switches}` → `{BlockOnPlates, TorchSequence}`. Re-export through
  `game/world.rs` and `content/mod.rs`. Tests: extend the `schema.rs` unit tests (`object_at` finds a
  beacon; a block is not an `ObjectKind`).
- [x] **T2 — `PuzzleState` and the shared push rule.** `src/game/puzzles.rs`: `PlateState` → `PuzzleState
  { pressed, blocks, sequence }` with `for_room`; `pub fn block_push_target(...) -> Option<Pos>` (the one
  legality predicate, shared with the validator). `src/game/state.rs`: field `plates` → `puzzle`,
  `spawn_enemies` → `enter_room` (also reseeds blocks/sequence); `walkable` and `ai::occupied_positions`
  treat a current-room block as solid. Update `render/scene.rs` and `tests/determinism.rs` call sites.
  Tests: `puzzles.rs` unit tests for `block_push_target` (floor ok; door/stairs/water/wall/object/block/enemy
  refused).
- [x] **T3 — BlockOnPlates in the simulation.** `src/game/state.rs` movement branch: push attempt before the
  blocked case, `GameEvent::BlockPushed`; `puzzles::on_hero_moved` solves `BlockOnPlates` when every named
  plate holds a block, and un-presses when one leaves. Tests: `tests/puzzles.rs` —
  `block_on_plates_can_be_solved`, `pushing_a_block_emits_visible_feedback`,
  `a_block_is_never_pushed_onto_a_door_or_stairs_tile`, `a_block_blocks_the_hero_and_enemies`.
- [x] **T4 — TorchSequence in the simulation.** `src/game/state.rs::handle_use_lantern` branches on sequence
  torches; `GameEvent::PuzzleReset`; solved sequences persist through `progress.solved_puzzles`. Tests:
  `tests/puzzles.rs` — `torch_sequence_solves_in_the_hinted_order`,
  `a_wrong_torch_resets_the_sequence_with_a_message`, `a_sequence_torch_never_enters_progress_lit_torches`.
- [x] **T5 — Puzzle reset and persistence.** Wire `enter_room` reseeding end to end. Tests: `tests/puzzles.rs`
  — `leaving_and_re_entering_resets_an_unsolved_block_puzzle`,
  `…_resets_an_unsolved_torch_sequence`, `a_solved_puzzle_and_its_reveals_survive_re_entry`,
  `every_wrong_torch_order_still_leaves_the_puzzle_solvable`,
  `every_reachable_wrong_block_position_still_leaves_the_room_solvable` (drive the block to each reachable
  wrong tile, then prove the room is solvable — by pushing back where possible, and via exit/re-entry
  otherwise).
- [x] **T6 — Keys and locked doors.** `Progress::unlocked_doors`; `DoorCheck::NeedsKey`; consumption,
  reciprocal unlock, `GameEvent::DoorUnlocked`; `state_hash` covers `unlocked_doors`, with its row in the
  table-driven hash test. Tests: `tests/dungeon.rs` — `unlocking_a_door_consumes_exactly_one_key`,
  `a_second_pass_through_the_same_door_costs_nothing`, `the_reciprocal_side_is_free_after_unlocking`,
  `a_locked_door_with_no_key_blocks_and_never_makes_the_count_negative`.
- [x] **T7 — Boss types and tuning.** `game/tuning.rs` constants above; `entities.rs` `BossPattern` and the
  four `AiState` boss variants; `state.rs` `initial_hp`/`initial_ai_state`/`enemy_kind_code`/`write_ai`;
  `enter_room` skips a boss whose `defeat_flag` is set. Tests: unit — `write_ai` covers every new variant
  (extend the existing hash-coverage test).
- [x] **T8 — Boss state machine.** `ai.rs::step_boss` (stalk → windup → strike → vulnerable, per-phase
  patterns, the HP-threshold phase change), `strike_tiles(pattern, pos, room)`, `BossTelegraph`/
  `BossPhaseChanged`. Tests: `tests/boss.rs` — `the_boss_enters_phase_two_at_the_tuned_threshold`,
  `each_phase_uses_a_distinct_attack_pattern`,
  `every_strike_is_preceded_by_a_telegraph_of_at_least_the_tuned_warning`, plus the `tests/ai.rs` liveness
  sweep extended to the boss.
- [x] **T9 — Boss damage, vulnerability and the ember.** `combat.rs`: `apply_boss_strike`, the
  `BossVulnerable`-only damage clause in `resolve_swing`, the `EnemyKind::Boss` skip in
  `apply_contact_damage`, `BossDefeated` + `defeat_flag` + `drops`; pipeline order in `update`. Tests:
  `tests/boss.rs` — `the_boss_only_takes_damage_during_its_vulnerability_window`,
  `the_boss_deals_no_contact_damage`, `a_scripted_fight_beats_the_boss_with_full_health_intact`,
  `defeating_the_boss_grants_the_ember_and_sets_the_flag`, `a_defeated_boss_does_not_respawn_on_re_entry`.
- [x] **T10 — The ending.** Beacon interaction in `handle_interact`; `GameEvent::GameWon`; `Mode::Victory` in
  `app.rs`/`input.rs`; `overlays::draw_victory` + the `draw_overlay_for_mode` arm. Tests:
  `tests/mode_machine.rs` — `game_won_enters_victory_mode_and_stops_simulating`,
  `victory_confirm_returns_to_the_main_menu`; `tests/dungeon.rs` —
  `the_beacon_without_the_ember_only_reports_a_cold_brazier`.
- [x] **T11 — Render.** `tiles::Kind::{Boss, BossVulnerable, Block, Beacon}` + glyphs; `scene.rs` draws
  blocks, beacons, the boss (vulnerable variant), the windup telegraph tiles, sequence-lit torches and
  block-pressed plates. Tests: `tests/render.rs` — glyph distinctness (existing test, extended table),
  `the_boss_arena_renders_boss_and_telegraph_glyphs_at_60x24`, `the_victory_overlay_renders_at_60x24`.
- [x] **T12 — Validator: structure.** `error.rs` new variants (`BlockPuzzleShape`, `TorchSequenceShape`,
  `PuzzleObjectClaimedTwice`, `BlockPuzzleUnsolvable`, `BossFieldOnRegularEnemy`, `MultipleBosses`,
  `BeaconMissing`, `BeaconNotAtHome`, `MultipleBeacons`) + `IdKind::{Block, Beacon}`; the id, placement,
  puzzle-shape, boss and beacon checks. Tests: `tests/content.rs` with new fixtures
  (`broken_block_puzzle_shape.ron`, `broken_torch_sequence_unknown.ron`, `broken_boss_field_on_slime.ron`,
  `broken_beacon_not_at_home.ron`).
- [x] **T13 — Validator: reachability.** `settle` gains the TorchSequence rule, the one-block push search and
  the boss-drop producer; `ember_authored`/`Items::ember`/`main_route_rooms` learn about boss drops. Tests:
  `tests/content.rs` — `a_block_puzzle_with_no_pushable_plate_is_rejected`
  (`broken_block_unsolvable.ron`), `an_ember_behind_an_unreachable_boss_is_rejected`, and the existing
  `ember_unreachable`/`key_behind_its_own_lock` fixtures still pass.
- [x] **T14 — Author the dungeon.** `assets/world.ron`: the five new rooms per the table above, the beacon,
  the two `SmallKey` locks and their key chests, the boss spawn, the new `route`. Tests: `tests/dungeon.rs`
  — `the_world_has_nine_overworld_and_six_dungeon_rooms`,
  `the_world_matches_the_section_2_content_table` (3 NPCs, all 3 regular enemy kinds spawned, exactly one
  boss, ≥ 3 secrets), and `tests/content.rs::real_world_validates` updated to 15 rooms.
- [x] **T15 — The headless playthrough.** `tests/playthrough.rs` on `tests/common::Runner`, driving only
  `Action`s: a reactive navigator (per-tick room BFS, swing when an enemy blocks the way) plus the scripted
  puzzle and boss routines. Asserts the milestone order — sanctuary learned → sword → lantern → dungeon
  entered → first key → block puzzle → torch sequence → second key → boss phase 2 → ember → `GameWon` — that
  no `Progress::flags` entry is ever written by the test itself, and that the run finishes inside a tick
  ceiling well under the §15 budget.
- [x] **T16 — Docs and gate.** `docs/user/controls.md` (block pushing, the beacon), `docs/dev/` notes on the
  puzzle/boss state machines and the reset rule, `CHANGELOG.md`. Run the full gate.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Full gate (the phase exit condition):

```sh
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked
```

| Acceptance criterion | Test that proves it |
|---|---|
| Playthrough New Game → `GameWon` using only Actions, asserting the milestone order | `tests/playthrough.rs::new_game_reaches_game_won_using_only_actions`, `…::main_route_milestones_happen_in_order` |
| Unlocking decrements the key count by exactly one | `tests/dungeon.rs::unlocking_a_door_consumes_exactly_one_key` |
| A second pass through the same door costs nothing | `tests/dungeon.rs::a_second_pass_through_the_same_door_costs_nothing`, `…::the_reciprocal_side_is_free_after_unlocking` |
| The key count never goes negative | `tests/dungeon.rs::a_locked_door_with_no_key_blocks_and_never_makes_the_count_negative` |
| Each puzzle kind can be solved | `tests/puzzles.rs::block_on_plates_can_be_solved`, `…::torch_sequence_solves_in_the_hinted_order` |
| Leaving and re-entering resets an unsolved puzzle to its authored state | `tests/puzzles.rs::leaving_and_re_entering_resets_an_unsolved_block_puzzle`, `…::leaving_and_re_entering_resets_an_unsolved_torch_sequence` |
| A solved puzzle stays solved | `tests/puzzles.rs::a_solved_puzzle_and_its_reveals_survive_re_entry` |
| Every wrong state a puzzle allows still leaves the room solvable | `tests/puzzles.rs::every_wrong_torch_order_still_leaves_the_puzzle_solvable`, `…::every_reachable_wrong_block_position_still_leaves_the_room_solvable`, `…::a_block_is_never_pushed_onto_a_door_or_stairs_tile` |
| Boss moves to phase 2 at the tuned threshold | `tests/boss.rs::the_boss_enters_phase_two_at_the_tuned_threshold` |
| Each phase has a distinct attack pattern | `tests/boss.rs::each_phase_uses_a_distinct_attack_pattern` |
| A telegraph precedes every dangerous action by ≥ the tuned warning | `tests/boss.rs::every_strike_is_preceded_by_a_telegraph_of_at_least_the_tuned_warning` |
| Scripted no-damage boss kill (the vulnerability window is real) | `tests/boss.rs::a_scripted_fight_beats_the_boss_with_full_health_intact`, `…::the_boss_only_takes_damage_during_its_vulnerability_window`, `…::the_boss_deals_no_contact_damage` |
| `content::validate` passes over the finished 15-room world, ember obtainable and lighthouse reachable after | `tests/content.rs::real_world_validates`, `tests/content_startup.rs` (startup preflight), plus the negative fixtures `…::an_ember_behind_an_unreachable_boss_is_rejected`, `…::a_block_puzzle_with_no_pushable_plate_is_rejected` |
| Exactly 9 overworld + 6 dungeon rooms, 3 NPCs, 3 regular enemy kinds, 1 boss | `tests/dungeon.rs::the_world_has_nine_overworld_and_six_dungeon_rooms`, `…::the_world_matches_the_section_2_content_table` |
| The full phase gate passes | the command block above |

## Risks

| Row | How this phase touches it | What the plan does |
|---|---|---|
| **#1 unfinishable content** | The phase authors the half of the world that has locks, puzzles and the ember. | The reachability BFS is extended so the two new puzzle kinds and the boss drop are *modelled*, not assumed: the block puzzle gets an exact one-block push search rather than an optimistic "the plate is reachable" rule, and the ember/home check runs through the boss. `tests/playthrough.rs` is the independent second proof. |
| **#2 wrong game / unpaced** | The dungeon is the second half of the pacing curve. | The dungeon chain is ordered vestibule (safe) → combat → block puzzle → torch puzzle → guardians → boss, and the playthrough asserts the milestone *order*, not just victory. |
| **#3 time runs out mid-content** | This is the last content phase. | Tasks are ordered so the world is authored (T14) only after the mechanics it uses exist and are tested; if time runs short, the chain is linear and any shortfall is a named room rather than a broken route. |
| **#9 boss unbeatable or trivial** | Directly. | Every boss value is a tick constant in `game/tuning.rs`; the fight is a fixed cycle with a 24/18-tick telegraph and a 45/30-tick vulnerability window (no frame-precise input); `tests/boss.rs` bounds it from both sides — the no-damage script proves it is beatable cleanly, the phase/telegraph tests prove it is not trivial. |
| **#10 monochrome unreadable** | Four new `Kind`s. | Glyphs only (`W`, `w`, `O`, `*`), added to the existing exhaustive distinctness test; the theme layer still cannot change a glyph. |
| **#14 AI gets stuck** | A stuck boss is an unwinnable game. | The boss cycle is timer-driven — every state has an unconditional `until`, so it advances even if the hero never approaches — and the `tests/ai.rs` liveness sweep is extended to the boss. |
| **#6 input feels wrong over SSH** | Block pushing is a new movement consequence. | A push is one step on one movement event, paying the normal step cooldown; no new key, no hold, no chord. |

## Out of scope

- **Save/load** (phase 6): `Progress::unlocked_doors` and the new flags are hashed but not serialized;
  `Progress` and `ObjectRef` stay non-`Serialize` so phase 6 can pick the ids-not-indices on-disk shape.
  Autosave triggers on puzzle solve and boss defeat are phase 6.
- **Full CLI, themes, Unicode glyphs, monochrome** (phase 6): this phase adds ASCII glyphs only.
- **Balance passes, byte-volume measurement, the verification report, the manual PTY/SSH checks** (phase 7).
- **Extra dungeon secrets, optional side rooms, additional dialogue** — §2 asks for ≥ 3 secrets and the
  overworld already has them; the dungeon chain stays fully on-route.

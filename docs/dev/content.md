# Content: the RON world, the tile table, and the validator

The entire world lives in one hand-authored file, `assets/world.ron`, embedded into the binary at
compile time (`include_str!`, see `src/content/loader.rs`). Nothing under `src/content/` performs
runtime file I/O except `main.rs`'s `--debug-content PATH` read; `content::parse` itself takes a
`&str` and is pure.

## Schema (`src/content/schema.rs`)

```
World { version, start: StartPoint, route: Route, rooms: Vec<Room> }
StartPoint { room: RoomId, spawn: SpawnId }
Route { ember_required: bool, home: RoomId, goal: RoomId }
Room {
    id, name, kind: Overworld | Dungeon, map_index: Option<(u8, u8)>,
    rows: Vec<String>,           // 16 rows of 24 characters — the tile grid, authored as text
    doors: Vec<Door>, spawns: Vec<Spawn>, chests: Vec<Chest>, npcs: Vec<Npc>,
    enemies: Vec<EnemySpawn>, puzzles: Vec<Puzzle>, torches: Vec<Torch>, plates: Vec<Plate>,
    hint: Option<String>,        // shown in the message row on entry
}
Door    { id, at: Pos, to_room: RoomId, to_spawn: SpawnId, lock: Option<LockKind>, two_way: bool }
Spawn   { id, at: Pos }
Chest   { id, at: Pos, contains: Reward, secret: bool }
Npc     { id, at: Pos, dialogue: Vec<DialogueNode>, condition: Option<FlagId> }
DialogueNode { text: String, sets_flag: Option<FlagId> }
Torch   { id, at: Pos, reveals: Vec<Pos> }   // Hidden positions this room that become Floor once lit
Plate   { id, at: Pos }                      // not solid — stepped on, not interacted with
Puzzle  { id, kind: StepPlates | PushBlock | Switches, plates: Vec<PlateId>, reveals: Vec<Pos>, reward: Option<Reward> }
```

`doors`/`spawns`/`chests`/`npcs`/`enemies`/`puzzles`/`torches`/`plates`/`hint` are all
`#[serde(default)]`, so a room that doesn't use one of them simply omits the field. `map_index` is
`Some((col, row))` for the nine overworld rooms in their 3x3 grid, `None` for dungeon rooms
(phase 5's six-room dungeon, and phase 4's own `room.sanctuary_gate` vestibule).

An authored chest, NPC or torch **occupies its tile**: it is solid (blocks the hero and every
enemy — `GameState::object_at`/`walkable`) and is acted on by facing it and pressing Interact or
Use Lantern, mirroring the sword's one-tile hitbox. A `Plate` is the one object that is not solid;
stepping onto it (not facing it) is what presses it (`game::puzzles::on_hero_moved`). `PuzzleKind`
only implements `StepPlates` this phase — `PushBlock`/`Switches` are phase 5.

Every content struct carries `#[serde(deny_unknown_fields)]`, so a typo'd field name (`lockk:` for
`lock:`) is a parse error rather than a silently-ignored key — the field would otherwise just take
its `#[serde(default)]` and the authored value would vanish with no error reported.

## Tile characters

A room's 16 `rows` strings are the single source of truth for its geometry — a character grid
*is* the map, not a `[[Tile; 24]; 16]` literal enum list (see DECISIONS.md, "plan/02"). Every row
must be exactly `game::tuning::ROOM_W` (24) characters; every room must have exactly
`game::tuning::ROOM_H` (16) rows.

| char | tile | walkable | hazard | glyph |
|---|---|---|---|---|
| `.` | `Floor` | yes | no | `.` |
| `#` | `Wall` | no | no | `#` |
| `~` | `Water` | no | yes | `~` |
| `"` | `Bush` | no | no | `"` |
| `+` | `Door` | yes | no | `+` |
| `>` | `Stairs` | yes | no | `>` |
| `v` | `Pit` | no | yes | `v` |
| `?` | `Hidden` | no (until revealed) | no | `#` unrevealed, `.` once a lit torch or a solved puzzle in the same room lists it in `reveals` (`GameState::is_revealed`/`walkable`) |

Any other character is `ContentError::IllegalTile`. Because `"` is both the bush glyph and RON's
string delimiter, a bush tile inside a `rows` string must be written as `\"`.

## Object glyphs (`src/render/tiles.rs`)

| kind | glyph | solid |
|---|---|---|
| `Npc` | `N` | yes |
| `Chest` / `ChestOpen` | `C` / `c` | yes |
| `Torch` / `TorchLit` | `t` / `T` | yes |
| `Plate` / `PlatePressed` | `_` / `=` | no |

Paint order in `render/scene.rs`: tiles (a revealed `Hidden` draws as `Floor`) → plates → chests →
NPCs → torches → the guardian danger cue → enemies → the sword → the hero, so the hero is never
hidden and an object glyph is never hidden by the tile underneath it.

## Ids

Ids are stable, namespaced strings (ADR 0005): `room.lighthouse`, `door.crossroads.north`,
`spawn.crossroads.north`, `chest.forest_sword`. They are part of the save-format compatibility
surface (phase 6) — renaming or removing one is a breaking content change, adding one is not. The
loader interns room ids into a dense `RoomIdx` for runtime use; door and spawn ids stay strings,
resolved by `World::spawn_pos`/`World::door_at`. Runtime code names an authored chest/torch/puzzle
by `ObjectRef { room: RoomIdx, index: u16 }` (an index into that room's own vec) rather than by its
string id — dense and `Ord`, so `Progress`'s `BTreeSet<ObjectRef>` fields (`opened_chests`,
`lit_torches`, `solved_puzzles`) iterate deterministically for hashing; phase 6 defines how a save
resolves one back to its authored string id.

The convention this phase's `assets/world.ron` follows: a door on the `north` edge of
`room.<x>` is `door.<x>.north`, and the spawn an arriving hero lands on (one tile inside the room,
adjacent to that door) is `spawn.<x>.north`.

## Adding a room

1. Pick an id (`room.<name>`) and, for an overworld room, an unused `map_index`.
2. Author 16 rows of 24 characters: a wall ring is not required by the validator, but every door
   tile needs a matching `Door` entry (see below) and every non-door edge needs to actually block
   movement if you don't want the hero walking off the authored area.
3. For every neighbor the room should connect to, add a `Door` at the shared edge and a `Spawn` one
   tile inside the room, adjacent to that door — this is what makes the two-way reciprocity check
   pass (see below).
4. Run `cargo test --locked content::` (unit tests) and `cargo test --locked --test content` (the
   real-world and fixture tests) — a new room is exercised by both without further wiring, since
   `content::load()` re-parses `assets/world.ron` fresh in each test.

## What the validator checks (`src/content/validate.rs`)

`content::validate(&world)` (called from tests, from CI, and from `main`'s startup preflight)
collects every violation of the spec §7 list rather than stopping at the first:

- **Structure**: room/door/spawn/chest/npc/puzzle/torch/plate ids are globally unique; overworld `map_index`
  values are unique; every door's `to_room`/`to_spawn` resolve, and so does `start`/`route.home`;
  every spawn is in bounds, walkable, and not itself a door tile; every `Door` tile has exactly one
  matching door entry, and every door entry sits on a `Door` tile.
- **Reciprocity**: for a two-way door `A` targeting spawn `s`, there must be a two-way door `B` in
  the target room back to `A`'s room, with `s` orthogonally adjacent to `B`'s position and `B`'s
  own target spawn orthogonally adjacent to `A`'s position — i.e. walking through either door lands
  you next to the door you'd take back.
- **Reachability**: a breadth-first search over `(unlocked-door, item)` states, branching over
  which frontier door to open next rather than greedily picking one (small keys are fungible and
  consumable, so a greedy walk can report a false dead end). The graph the search flood-fills is a
  plain orthogonal tile BFS across every room's grid, not a room graph: `Loc` is a `(RoomIdx, Pos)`
  tile, and a door only crosses into another room if the hero can actually walk to that door's own
  tile — a spawn walled off from the rest of its room is not "reachable" just because the room
  itself is (`DoorUnreachableInRoom`). From the union of every reachable state: every room must be
  reachable (`RoomUnreachable`); every lock must be
  satisfiable in some reachable state (`LockNeverUnlockable` — this is "the key is behind the very
  lock it opens"); if any chest grants the ember, it must be reachable (`EmberUnreachable`) and
  `route.home` must still be reachable from that state (`HomeUnreachableWithEmber`); if
  `route.ember_required` and no chest grants the ember, `EmberMissing`. The search's unlocked-door
  state is a `u64` bitmask, one bit per `SmallKey` door, so a world authoring more than 64 of them
  is rejected outright (`TooManySmallKeyDoors`) rather than overflowing the mask.

- **Enemy spawns** (`check_enemy_spawns`, phase 3): every `EnemySpawn.at` and every waypoint in its
  optional `patrol` must be in bounds, walkable, not a hazard tile and not a `Tile::Door`, else
  `ContentError::EnemySpawnNotWalkable` / `EnemyPatrolInvalid`. `tests/fixtures/broken_enemy_spawn.ron`
  (a spawn placed inside a wall) proves the check rejects. `EnemyKind` is `Slime | Bat | Guardian`
  (`Boss` is not a content-authorable kind — it belongs to the phase-5 boss arena).
- **Object placement** (`check_object_placement`, phase 4): every chest/npc/torch/plate `at` must be
  in bounds, sit on plain `Tile::Floor`, and not coincide with a spawn, an enemy spawn, or a patrol
  waypoint (`ObjectNotOnFloor`); at most one object may occupy a tile (`ObjectTileConflict`).
- **NPC conditions** (`check_npc_conditions`, phase 4): every `Npc.condition` flag and every
  `LockKind::Flag` flag must be set by some `DialogueNode.sets_flag` somewhere in the world, or it
  could never be satisfied (`FlagNeverSet`).
- **Puzzles and torches** (`check_puzzles`/`check_torches`, phase 4): every `Puzzle.plates` id must
  name a plate in the same room (`UnknownPlate`); every `Puzzle.reveals`/`Torch.reveals` position
  must be a `Tile::Hidden` tile in that room (`RevealNotHidden`).
- **Secrets** (`check_secrets`, phase 4): a secret chest (`Chest.secret`) may not hold a
  route-critical reward — `Sword`, `Lantern`, or `Ember` (`SecretRouteCritical`) — and its room may
  not lie on `main_route_rooms` (`SecretOnMainRoute`), the union of every room on any shortest path
  (plain room-adjacency graph, locks ignored) from `start.room` to each route-critical target (the
  rooms holding a non-secret `Sword`/`Lantern`/`Ember` chest, plus `route.goal` and `route.home`) —
  a secret must never gate progress.

Reachability (phase 4) is reveal-aware: `Loc` is now a `(RoomIdx, Pos)` tile, not a room/component
pair, so "is object X reachable" is directly "some orthogonal neighbour of X's tile is in the
reached set" — the right question now that chests/NPCs/torches are solid. Alongside the item
fixpoint (`Items { lantern, ember }`), `settle` now also grows a `RevealState`: a reachable torch is
lit once the lantern is held (its `reveals` become walkable next iteration); a reachable NPC whose
condition is satisfied contributes its dialogue's `sets_flag` values; a `StepPlates` puzzle counts
as solved once every one of its plates is in the reached tile set. A `LockKind::Flag` door is now
traversable once its flag is in the reached set's flags, instead of always being reported
`LockNeverUnlockable`.

## Reading validator output

Each `ContentError` prints as one line naming the offending id (`content::report`, joined
newline-separated):

```
room.west_grove: spawn 'spawn.west_grove.east' at (3, 7) is not walkable (Wall)
door.crossroads.north: references unknown spawn 'spawn.stone_circle.souht' in room 'room.stone_circle'
```

`main` runs this preflight before touching the terminal (so a broken world is reported even
without a TTY) and exits with code 2 on failure. `tests/content_startup.rs` exercises this against
the real binary via the hidden `--debug-content PATH` flag; `tests/content.rs` exercises the
validator itself against `assets/world.ron` and the fixtures in `tests/fixtures/`.

## Fixtures (`tests/fixtures/`)

`base.ron` is a valid 3-room world (a `SmallKey` lock with its key reachable first, and an ember
chest reachable with `route.home` still reachable afterwards) that every `broken_*.ron` fixture is
derived from by exactly one defect, so `tests/content.rs` can assert the validator rejects each
specific rule rather than merely "some fixture fails". `broken_three_defects.ron` combines three
independent defects to prove errors are collected, not short-circuited.
`broken_walled_in_spawn.ron` walls a spawn into its own one-tile pocket to exercise
`DoorUnreachableInRoom` (and the `RoomUnreachable` it cascades into); `broken_unknown_field.ron`
misspells a field name to exercise `deny_unknown_fields`.

Phase 4 adds one fixture per new check: `broken_object_on_wall.ron`, `broken_object_tile_conflict.ron`,
`broken_unknown_plate.ron`, `broken_reveal_not_hidden.ron`, `broken_flag_never_set.ron`,
`broken_secret_on_main_route.ron`, `broken_secret_route_critical.ron` — each `base.ron` plus exactly
the one defect its name says, asserted in `tests/content.rs` against the specific `ContentError`
variant it exists to trigger.

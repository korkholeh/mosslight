# Content: the RON world, the tile table, and the validator

The entire world lives in one hand-authored file, `assets/world.ron`, embedded into the binary at
compile time (`include_str!`, see `src/content/loader.rs`). Nothing under `src/content/` performs
runtime file I/O except `main.rs`'s `--debug-content PATH` read; `content::parse` itself takes a
`&str` and is pure.

## Schema (`src/content/schema.rs`)

```
World { version, start: StartPoint, route: Route, rooms: Vec<Room> }
StartPoint { room: RoomId, spawn: SpawnId }
Route { ember_required: bool, home: RoomId }
Room {
    id, name, kind: Overworld | Dungeon, map_index: Option<(u8, u8)>,
    rows: Vec<String>,           // 16 rows of 24 characters — the tile grid, authored as text
    doors: Vec<Door>, spawns: Vec<Spawn>, chests: Vec<Chest>, npcs: Vec<Npc>,
    enemies: Vec<EnemySpawn>, puzzles: Vec<Puzzle>,
}
Door  { id, at: Pos, to_room: RoomId, to_spawn: SpawnId, lock: Option<LockKind>, two_way: bool }
Spawn { id, at: Pos }
```

`doors`/`spawns`/`chests`/`npcs`/`enemies`/`puzzles` are all `#[serde(default)]`, so a room that
doesn't use one of them (every room this phase has no chests, npcs, enemies or puzzles yet) simply
omits the field. `map_index` is `Some((col, row))` for the nine overworld rooms in their 3x3 grid,
`None` for dungeon rooms (phase 5).

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
| `?` | `Hidden` | no | no | `#` (deliberately indistinguishable until phase 4's reveal mechanic) |

Any other character is `ContentError::IllegalTile`. Because `"` is both the bush glyph and RON's
string delimiter, a bush tile inside a `rows` string must be written as `\"`.

## Ids

Ids are stable, namespaced strings (ADR 0005): `room.lighthouse`, `door.crossroads.north`,
`spawn.crossroads.north`, `chest.forest_sword`. They are part of the save-format compatibility
surface (phase 6) — renaming or removing one is a breaking content change, adding one is not. The
loader interns room ids into a dense `RoomIdx` for runtime use; door and spawn ids stay strings,
resolved by `World::spawn_pos`/`World::door_at`.

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

- **Structure**: room/door/spawn/chest/npc/puzzle ids are globally unique; overworld `map_index`
  values are unique; every door's `to_room`/`to_spawn` resolve, and so does `start`/`route.home`;
  every spawn is in bounds, walkable, and not itself a door tile; every `Door` tile has exactly one
  matching door entry, and every door entry sits on a `Door` tile.
- **Reciprocity**: for a two-way door `A` targeting spawn `s`, there must be a two-way door `B` in
  the target room back to `A`'s room, with `s` orthogonally adjacent to `B`'s position and `B`'s
  own target spawn orthogonally adjacent to `A`'s position — i.e. walking through either door lands
  you next to the door you'd take back.
- **Reachability**: a breadth-first search over `(unlocked-door, item)` states, branching over
  which frontier door to open next rather than greedily picking one (small keys are fungible and
  consumable, so a greedy walk can report a false dead end). The graph the search flood-fills is
  not just rooms: each room's walkable tiles are first partitioned into 4-directionally connected
  components, and a door only crosses into another room if the hero can actually walk to that
  door's tile from the component they landed in — a spawn walled off from the rest of its room is
  not "reachable" just because the room itself is (`DoorUnreachableInRoom`). From the union of
  every reachable state: every room must be reachable (`RoomUnreachable`); every lock must be
  satisfiable in some reachable state (`LockNeverUnlockable` — this is "the key is behind the very
  lock it opens"); if any chest grants the ember, it must be reachable (`EmberUnreachable`) and
  `route.home` must still be reachable from that state (`HomeUnreachableWithEmber`); if
  `route.ember_required` and no chest grants the ember, `EmberMissing`. The search's unlocked-door
  state is a `u64` bitmask, one bit per `SmallKey` door, so a world authoring more than 64 of them
  is rejected outright (`TooManySmallKeyDoors`) rather than overflowing the mask.

`LockKind::Flag` has no authored way to become true yet (a later phase adds the mechanism), so a
`Flag`-locked door is always reported `LockNeverUnlockable` — correct for now, since nothing can
ever open one.

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

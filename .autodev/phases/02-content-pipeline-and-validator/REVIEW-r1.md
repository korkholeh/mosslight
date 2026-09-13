# Review — phase 2 round 1

**Verdict:** changes_requested

Phase 2 delivers a genuinely well-built content pipeline: a clean four-module `src/content/`, a char-grid RON schema that reads as a map, 17 typed error variants with real `Display` output, a correct branching `(unlocked, items)` BFS, nine correctly-wired overworld rooms with 24 doors, and a fixture suite where every broken world differs from `base.ron` by exactly one field. I ran the full gate locally: fmt, clippy `-D warnings` and `cargo test --locked` (113 tests) all pass. No dependency changes; the phase-1 tests were adapted, not weakened. Every ARCHITECTURE deviation is logged in DECISIONS.md, and every PLAN task marked `[x]` has code behind it.

All seven acceptance criteria have a test behind them. Two are proven more narrowly than they read, and both gaps are in the validator itself — the artifact this phase exists to produce and that phases 4–5 will depend on.

Blocking on two `major` findings, both reproduced against the built binary:
1. Reachability runs over the *door graph* only, never over walkable tiles inside a room. I walled in an arrival spawn in `base.ron` — hero permanently trapped, ember unreachable — and the validator accepted it. Spec §7 asks for a *traversable* route; RISKS #1 names this exact class.
2. No `deny_unknown_fields` anywhere in the schema. Renaming `lock:` to `lockk:` silently deleted a SmallKey lock with zero errors reported. A validator built to catch authoring typos is blind to the most common one.

Both are contained fixes in `src/content/`, each worth one new fixture. The remaining findings are smaller: `content::validate` is exported but never called and its doc comment describes the opposite behaviour; `App::new` re-parses and `expect`s the content path inside the terminal guard (CLAUDE.md forbids unwrap there); the every-door test asserts the engine against `door.to_room` rather than against grid geometry, so a mis-wired grid would pass; and `RoomEntered.first_visit` is written but never asserted.

## [MAJOR] Reachability is room-graph only — intra-room walkability never checked, so "traversable main route" is not actually proven
`src/content/validate.rs`

`ReachabilitySearch::flood` walks the *door graph*: from a room it follows every door entry regardless of whether the hero can physically walk from the tile they arrive on to that door's tile. Nothing in the validator ever does a tile-level BFS inside a room. Spec §7's last bullet is "наявність прохідного основного маршруту" (a *traversable* main route), and RISKS #1 names exactly this class ("a spawn inside a wall"). `check_spawns` only proves the spawn tile itself is walkable, not that it connects to anything.

Verified by construction: I took `tests/fixtures/base.ron` and walled in `spawn.b.fromA` at (1,7) (row 6 -> `##.....................#`, row 7 -> `+.#....................#`, row 8 -> `##.....................#`). The hero entering room.b is now permanently boxed in and can never reach `door.b.toC` or the ember in room.c. `mosslight --debug-content` accepted the file with zero content errors — only the TTY refusal was printed.

This is the phase's headline deliverable ("make 'the world is completable' a machine-checked property before any content is authored") and the safety net phases 4–5 will lean on precisely when interior geometry stops being an empty rectangle. Today's `assets/world.ron` is fine only because all nine rooms are an open box.

**Fix:** In `flood`, before following a room's doors, run a walkable-tile BFS from the tile(s) the hero can occupy in that room (the start spawn for the start room; the spawn each incoming door targets otherwise) and only treat a door as an exit if its `at` tile is tile-reachable from one of them. Add a `ContentError::DoorUnreachableInRoom { room, door, from_spawn }` variant plus a `broken_walled_in_spawn.ron` fixture asserting it.

## [MAJOR] No `deny_unknown_fields`: a misspelled `lock:` silently deletes a lock and the validator sees nothing
`src/content/schema.rs`

None of the content structs (`World`, `Room`, `Door`, `Spawn`, `Chest`, `Npc`, `EnemySpawn`, `Puzzle`, `StartPoint`, `Route`) carry `#[serde(deny_unknown_fields)]`, so serde's default of silently ignoring unknown keys applies. Combined with `#[serde(default)]` on `lock`, `patrol`, `condition` and `#[serde(default = "default_two_way")]` on `two_way`, a typo in an optional field name is invisible: the field takes its default and no error is produced.

Verified: I copied `base.ron` renaming `lock:` -> `lockk:` and `two_way:` -> `two_wayy:` on `door.b.toC`. `mosslight --debug-content` reported no content errors at all. The SmallKey lock had silently vanished — the gated area became free, and `LockNeverUnlockable` / the whole key-before-lock machinery had nothing left to check.

A validator whose entire purpose is catching authoring mistakes before content lands should not be blind to the most common authoring mistake there is. This gets worse in phases 4–5 when `chests`, `npcs`, `enemies` and `puzzles` are actually authored.

**Fix:** Add `#[serde(deny_unknown_fields)]` to every content struct in `src/content/schema.rs`. (`Room.tiles` is `#[serde(skip)]`, so an authored `tiles:` key will then also be rejected — which is correct.) Add a `broken_unknown_field.ron` fixture asserting `ContentError::Parse`.

## [MINOR] `content::validate` is dead public API and its doc comment is wrong
`src/content/validate.rs`

`validate` is re-exported from `content/mod.rs` but no caller exists anywhere: `loader::parse` goes straight to `collect_errors`, `main.rs` calls `parse`, and no test in `tests/` or `src/` invokes it (grep for `content::validate` finds only the `pub use` and a doc reference). The acceptance criterion names it by name ("content::validate over the real assets/world.ron returns Ok"); `tests/content.rs::real_world_validates` proves the equivalent through `load()`, so the property holds, but the named entry point itself is untested.

Its doc comment also states "Runs every check and fails on the first problem found", which is the opposite of what it does — it collects every error, and the phase's third acceptance criterion depends on that.

**Fix:** Have `real_world_validates` call `content::validate(&world)` after `load()`, and fix the doc comment to say it collects all errors.

## [MINOR] `App::new` re-parses and `expect`s the content path, inside the terminal guard; `--debug-content` validates a world gameplay then ignores
`src/app.rs`

`App::new` calls `content::load().expect("the content preflight in main() already validated the embedded world")`. Three problems:

1. CLAUDE.md: "Never `unwrap` on the save path or the content path; both return explicit outcome enums." This is an unwrap-family call on the content path.
2. `App::new` runs at `src/main.rs` after `TerminalGuard::enter`, so the panic fires with raw mode and the alternate screen already up (the panic hook recovers, but the whole point of the preflight was to fail before that).
3. It re-parses and re-validates `EMBEDDED` a second time, and it always loads `EMBEDDED` — so with `--debug-content <valid file>` the preflight validates one world and gameplay plays a different one. `docs/dev/development.md` says the flag "validates PATH instead of the embedded assets/world.ron", which is accurate, but the asymmetry means the flag can never be used to actually play a candidate world.

**Fix:** Parse once in `main::content_preflight`, return the `World`, and pass `Rc<World>` into `App::new(cfg, world)`. That removes the `expect`, the second parse, and the divergence in one change.

## [MINOR] `every_door_leads_to_its_target_room` is self-referential — a mis-wired 3×3 grid passes every test in the phase
`tests/transitions.rs`

The test resolves the expected room via `world.room_idx(&door.to_room)` and asserts `state.room` equals it. That proves the engine honours whatever the door declares, but says nothing about whether the declaration is right. If `door.crossroads.north` pointed at `room.old_mill` instead of `room.stone_circle`, this test, `reciprocal_door_returns_to_the_origin_door`, `real_world_has_nine_rooms_in_a_3x3_grid` and the validator would all still be green — nothing anywhere relates a door's edge to the `map_index` delta between the two rooms.

The phase goal is "wire room transitions against the real nine-room overworld grid"; the grid topology itself is currently unasserted. (I checked all 24 doors by hand and the wiring is in fact correct — this is about the test, not the data.)

**Fix:** In `every_door_leads_to_its_target_room`, derive the expected neighbour from geometry: from `door.at` compute the edge (N/S/E/W), apply that delta to the source room's `map_index`, and assert the arrival room's `map_index` matches. Doors named `.north`/`.south`/`.east`/`.west` make the id-vs-edge check free as well.

## [MINOR] `GameEvent::RoomEntered.first_visit` is produced but never asserted anywhere
`src/game/state.rs`

`update` computes `first_visit` from `progress.visited.insert(...)`, but grepping `src` and `tests` shows the field is only ever written, never read or asserted. `re_entering_a_visited_room_does_not_grow_the_set` checks the set length and `room_entered_is_emitted_once_per_transition_and_not_for_the_start_room` counts events, but neither pins the flag. DECISIONS.md records that this event is phase 6's autosave trigger and phase 4's map will branch on `first_visit`, so a wrong value is a silent regression today and an expensive one later.

**Fix:** In `visited_set_grows_once_per_new_room` assert the emitted event has `first_visit: true`; in `re_entering_a_visited_room_does_not_grow_the_set` assert the re-entry event has `first_visit: false`.

## [MINOR] `Progress.visited` serializes dense `RoomIdx`, contradicting the save-stability rationale recorded in DECISIONS.md
`src/game/state.rs`

`Progress` derives `Serialize` over `BTreeSet<RoomIdx>`, and `RoomIdx` is a dense index assigned by `loader::intern_room_ids` in file order. The phase-2 DECISIONS entry for RISKS #7 claims "the save format (phase 6) is built on ids, not on indices that content reordering would invalidate" — as written today, the only serialized progress field is exactly such an index. Reordering rooms in `assets/world.ron`, or inserting the six dungeon rooms in phase 5, renumbers every previously saved visited room. Saving is phase 6 work, but the serialization shape is being fixed now.

**Fix:** Either drop the `Serialize` derive from `Progress` until phase 6 defines the on-disk shape, or add `#[serde(serialize_with = ...)]` that writes the room's string id, so the recorded rationale and the code agree.

## [NIT] `1u64 << i` over `smallkey_doors` panics past 64 small-key doors
`src/content/validate.rs`

`ReachabilitySearch::bit_for` and the `LockNeverUnlockable` loop in `check_reachability_and_route` both do `1u64 << i` indexed by position in `smallkey_doors`, with no bound. A world with more than 64 SmallKey-locked doors panics on shift overflow (debug) rather than returning a `ContentError`. Unreachable with a 6-room dungeon, but it is an unguarded panic on the content path.

**Fix:** Guard with `if smallkey_doors.len() > 64 { return vec![ContentError::…] }` (or skip the search and emit a dedicated error) before building the bitmask.

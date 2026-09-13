# 0005. The world is authored in RON, embedded into the binary at compile time, and machine-validated

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§7 requires the world description in readable RON or JSON, embedded into the binary at build time, with
stable ids for rooms, doors, chests and story events. It then lists a content validator that must check map
dimensions and legal tiles, id uniqueness, that transition targets exist, that spawn points are correct, that
two-way transitions are reciprocal, that the keys needed for each lock are obtainable, and that a traversable
main route exists.

§3 requires the installed game to need no external asset files. §15 makes "no dead ends caused by keys or
puzzles" an acceptance criterion — a property about the content, not the code.

The content is the largest hand-authored artifact in the project: 15 rooms of 24×16 tiles, doors, chests,
dialogue, spawns, and puzzles. It will be written by an agent under time pressure, which makes an
authoring mistake the most likely way this project ships something unwinnable.

## Decision

- One file, `assets/world.ron`, holding the entire world. RON is chosen over JSON because it carries Rust
  enums natively (`Door(id: "door.cellar", lock: Some(SmallKey))`) and allows comments, which matters for a
  file a human has to read and correct.
- Embedded with `include_str!` and parsed once at startup into `World`. No file is read at runtime, so the
  installed binary is self-sufficient (§3).
- Ids are **stable strings** namespaced by kind: `room.lighthouse`, `door.dungeon_entrance`,
  `chest.forest_sword`, `flag.told_about_sanctuary`. They are the save file's vocabulary (ADR 0006), so
  renaming one is a save-compatibility break and needs a format bump.
- `content::validate(&World) -> Result<(), Vec<ContentError>>` implements exactly §7's list, collecting
  **all** errors rather than stopping at the first. It runs in three places: as a `cargo test` case, in CI,
  and at startup before the terminal is touched.
- The reachability check is a breadth-first search over `(room, acquired item set)` states from the start
  position, asserting that the ember is obtainable and that the lighthouse is reachable again afterwards.
  This is what turns §15's "no dead ends" from a playtesting hope into a machine-checked property.

## Alternatives considered

- **JSON for content** — rejected: no comments, and enums become tagged-object boilerplate that makes a
  hand-edited 15-room file substantially harder to read and to correct.
- **Loading `world.ron` from disk at runtime** — rejected by §3 and by §2 (no map editor): it would add an
  install step, a path-resolution problem, and a new untrusted input.
- **A `build.rs` that parses and validates at compile time** — rejected as the primary mechanism: it makes
  build errors harder to read and slows every build. The validator as a test plus a startup check gives the
  same guarantee with better diagnostics. (A build script may still be added later purely as a fail-fast.)
- **Splitting content across one file per room** — rejected for now: cross-room invariants (reciprocal
  doors, key/lock ordering) are easier to see and to diff in one file at this scale.
- **Numeric ids** — rejected: they make save files unreadable and make a content reordering silently corrupt
  a save.

## Consequences

- The binary is standalone and the content is one reviewable artifact.
- A content mistake fails a test with a full list of problems, not a confusing crash three rooms into play.
- Changing content requires a rebuild. That is acceptable — there is no map editor in v1 (§2) — but it means
  balance iteration is a compile cycle.
- The validator is itself code that can be wrong. Its reachability search is therefore tested against
  deliberately broken fixture worlds (a missing key, a one-way door pair, an out-of-bounds spawn), so the
  check that guards the acceptance criterion is itself guarded.

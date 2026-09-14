# Testing

## Running the suite

```sh
cargo test --locked
```

Runs everything: unit tests inside `src/**/*.rs` (`#[cfg(test)]` modules in `game/rng.rs`,
`game/entities.rs`, `game/state.rs`, `render/scene.rs`, `render/tiles.rs`,
`render/theme.rs`, `render/hud.rs`, `content/schema.rs`, `content/error.rs`, `content/loader.rs`,
`content/validate.rs`, `save.rs`) and the integration suites under `tests/`. There is no separate
e2e command — the headless start-to-victory playthrough (`tests/playthrough.rs`) and the
room-transition checks are ordinary `#[test]` functions under `tests/`, not a second harness.

No `tempfile` crate: `tests/common/mod.rs::ScratchDir` is ~20 lines over `std::env::temp_dir()` +
`std::fs::create_dir_all`/`remove_dir_all`, unique per call (a global counter plus the process id)
and removed on `Drop`. It exists because the save-file tests (`tests/save.rs`) are the one place
that needs a real directory on disk; everything else that needs a `SaveIo` uses
`tests/common/mod.rs::memory_io()` (an in-process `MemorySaveIo` handle — no filesystem at all).

Full gate, run before every commit and in CI:

```sh
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked
```

## What each integration file covers

| File | Covers |
|---|---|
| `tests/deps.rs` | `Cargo.lock` has exactly one `crossterm` entry; `Cargo.toml` never names it directly |
| `tests/config.rs` | CLI parsing, flag defaults and conflicts, colour precedence, save-dir resolution, and two spawned-binary checks (non-TTY stdin, `TERM=dumb`) asserting exit code, one stderr line, and empty stdout |
| `tests/movement.rs` | Four-directional movement, wall/water/bush/out-of-bounds collision, facing-on-blocked-move, step cooldown, determinism of `game::update` for a fixed seed and action sequence; `GameState::walkable`: the hero can step onto a plate but not a chest/NPC/torch, an enemy cannot step onto a solid object either, and knockback stops before one |
| `tests/input_policy.rs` | Key-to-action mapping per mode, `coalesce`, the overflow policy (a burst larger than the per-iteration cap keeps the newest movement, not the oldest), `drop_pending` |
| `tests/no_key_release.rs` | Scans `src/**/*.rs` for `KeyEventKind`/`Release`/keyboard-enhancement identifiers and fails if any appear — a structural check that no key-release path is ever added |
| `tests/mode_machine.rs` | Mode transitions (menu, pause, confirm-quit, help, resize into/out of `TooSmall`); `HeroDied` opening `Mode::GameOver`, the simulation not advancing while there, retry restoring the last *autosave* (health included, via a shared `MemorySaveIo` handle) and rewinding the tick counter, retry never writing a save, and `Cancel` returning to the main menu; opening the Map, Inventory or a dialogue freezes `tick_counter`/`state.tick`, and closing any overlay or a dialogue drops a movement action queued in the same input batch |
| `tests/overworld.rs` | A headless driver (`tests/common/mod.rs`) run against the real `assets/world.ron`: zero enemies in the start room and none reachable before the sword chest (a structural BFS, not a hardcoded room pair); each chest opens once and reopening yields nothing; every `Reward` variant's effect including the `HeartContainer` cap at 5 hearts; `Attack` without the sword; the lantern lighting only the faced torch and opening its passage; the dungeon entrance blocked without the lantern and open with it; the `StepPlates` puzzle solving in any press order, resetting when the room is left mid-solve, and staying solved after re-entry; an NPC with an unsatisfied condition; the full start-to-lantern milestone order (including the first slime encounter); exactly three secret chests, all reachable once their torch is lit, none in `main_route_rooms` |
| `tests/dialogue.rs` | Interacting with a talkative NPC opens `Mode::Dialogue` and applies node zero's `sets_flag`; nodes advance in order and the named node sets its flag; `Cancel` closes at the first node; an unsatisfied `Npc.condition` never starts a dialogue and a later-satisfied one does; nothing moves or takes damage while a dialogue is open; closing a dialogue drops a queued movement action from the same batch |
| `tests/render.rs` | `TestBackend` assertions: hero/wall/HUD/message/hint cell positions at 60x24 and 80x24, the too-small notice at 59x23, enemy/sword/telegraph glyph placement at 60x24, that the same scene renders identical characters under all three themes, and that every `tiles::Kind` has a distinct glyph and the mono theme returns only a white/black/gray family colour for each; object glyphs (chest/NPC/torch/plate, each state) rendered identically under every theme; the Map overlay hiding unvisited rooms, marking the current one, showing a visited room's unopened chest, and never marking a room whose only chest is secret; the Inventory overlay's equipment/keys/hearts; the Dialogue overlay's current node and continue prompt; the `ConfirmNewGame` overlay over a usable slot and the `SaveProblem` overlay's backup-restore/new-game options over a corrupt slot |
| `tests/render_modes.rs` | The presentation half of RISKS #10 at the rendered-buffer level (not just `Theme::color_for` in isolation): one scene (hero/wall/bush/chest/enemy) renders identical characters under all three themes; `gameboy` only ever paints the four authored indexed greens, `ansi` only the 16 named ANSI colours, `mono` only the white/black family, all read off the actual `Buffer` cells `scene::draw_scene` paints; `ColorMode::Never` writes `Color::Reset` everywhere regardless of theme; `NO_COLOR` disables colour end-to-end through `Config` into a rendered buffer, and an explicit `--color always` overrides it |
| `tests/save.rs` | The save file layer (`save::{load,store,stage,commit}`): round-trips every field of a populated `SaveFile`; empty/truncated/garbage saves load as `Corrupt` without modifying the file; a newer `format_version` is refused on load and store; a crash simulated between `stage` and the rename leaves `save.json` and its `.bak` both intact; a death state is refused; a missing file is `Missing` not `Corrupt`; a grep test that `src/save.rs` calls no `unwrap`/`expect`/`panic!`/`unreachable!` outside its own unit tests. `SaveFile::capture`/`restore`: round-trips real progress by discovering real chest/torch/puzzle/door refs in the embedded world; drops an unknown id while keeping the rest; falls back to the room's first spawn when the saved position is not walkable; zeroes the tick and every hero timer. The `App` layer (via `tests/common::memory_io`/`ScratchDir`): each of the four autosave triggers writes exactly once; a death state never overwrites the last usable save; manual save is refused during combat and accepted outside it; loading respawns enemies, grants the safe window, and restarts a defeated boss without it respawning; `New Game` over an existing run requires confirmation; a corrupt slot offers backup restore and never overwrites the file; a future-version slot is never overwritten by a new run's autosave |
| `tests/terminal_guard.rs` | Restoration order and idempotence via `RecordingOps`, restore-before-panic-message, a partial failure during `enter()` still disabling raw mode |
| `tests/loop_timing.rs` | `Pacer`/`advance_iteration` in isolation: byte-identical `GameState` across fps 10/20/30 for the same action schedule, catch-up capped at 5 steps with surplus discarded, draw cadence scaling with fps while tick count does not, a keypress in a `sim_steps == 0` iteration is not lost, held-key movement lands at the cooldown rate |
| `tests/content.rs` | `content::validate` against the real `assets/world.ron` (15 rooms: 9 overworld with distinct 3x3 `map_index` values, plus the 6-room dungeon) and against `tests/fixtures/`: `base.ron` validates, and each `broken_*.ron` fixture is rejected with its specific named `ContentError` variant (missing door target, non-reciprocal door, spawn in a wall, duplicate id, wrong dimensions, illegal tile, key behind its own lock, ember unreachable, a spawn walled into its own pocket, an unknown RON field, an object on a non-floor tile, two objects on one tile, an unknown plate id, a reveal position that is not `Hidden`, a flag never set by any dialogue node, a secret chest on the main route, a secret chest holding a route-critical reward, an unresolvable `route.goal`, a block puzzle naming no block, a torch sequence naming unresolved torches, boss-only fields on a regular enemy, a beacon outside `route.home`, a block puzzle with no pushable plate, the ember behind an unreachable boss); `broken_three_defects.ron` returns three distinct variants from one call |
| `tests/transitions.rs` | Walking through every door in the real world: the hero lands in the declared `to_room` on a walkable, non-door tile, and the reciprocal door leads back adjacent to the door taken; `progress.visited` grows by exactly one per newly entered room and not on re-entry; `GameEvent::RoomEntered` fires once per transition, never for the start room, and not at all when a move next to a door is blocked |
| `tests/content_startup.rs` | Spawns the real binary with the hidden `--debug-content PATH` flag: a broken fixture exits 2 with the content error list on stderr (and not the TTY-refusal message, proving the abort happens before raw mode); a valid fixture passes the content preflight |
| `tests/combat.rs` | The sword hitbox (exactly the faced tile, four facings, side/rear tiles miss), the per-swing `hit` list (two enemies on the struck tile each damaged at most once), the cooldown (an `Attack` inside it is dropped, the next one outside it lands), contact damage and its invulnerability window, `combat::knockback` stopping adjacent to a wall and never landing on a door tile, and a killed enemy staying dead (no further movement or `AiState` change) |
| `tests/ai.rs` | A hand-built maze `World` (no loader, no validator — built directly from `content` schema types): each of the four kinds (including the boss) run 1000 ticks asserting `AiState` never goes `MAX_STALL_TICKS` without changing and the enemy never leaves the room or steps onto a non-walkable tile; the guardian's telegraph precedes its first dash by at least `TELEGRAPH_MIN_TICKS` and it only takes sword damage in `GuardianRecover`; two enemies dashing into the same tile resolve by `Vec` order (ADR 0004); the real `assets/world.ron` authors zero enemies in `room.lighthouse` and `room.crossroads` (the pacing-safe first stretch, RISKS #2) |
| `tests/determinism.rs` | A 2000-action pseudo-random sequence (from a test-local `Rng`, independent of `state.rng`) replayed twice from the same seed produces an identical `state_hash()` at every tick; the same sequence under two different seeds diverges somewhere; the same sequence driven through `App`/`Pacer` at `--fps` 10/20/30 produces an identical hash trace regardless of batching |
| `tests/puzzles.rs` | `BlockOnPlates` and `TorchSequence` against the real dungeon rooms: solving each, `BlockPushed`/`PuzzleReset` visible feedback, `block_push_target` refusing a door/wall target, a block blocking movement, re-entry resetting an unsolved puzzle while a solved one (and its reveal) survives, and that every wrong torch order or block overshoot still leaves the room solvable |
| `tests/dungeon.rs` | Keys and locked doors: unlocking spends exactly one key, a second pass through the same door is free, a synthetic two-sided-lock fixture proves the reciprocal side is freed by the same unlock, a locked door with no key blocks without going negative; the beacon without the ember only reports a cold brazier; the finished world's room/content counts (9 overworld + 6 dungeon, 3 NPCs, all 3 regular enemy kinds, exactly one boss, ≥ 3 secrets) |
| `tests/boss.rs` | The two-phase boss: the phase-two threshold, each phase's distinct `strike_tiles` shape, a telegraph always precedes a strike by at least `TELEGRAPH_MIN_TICKS`, damage lands only during `BossVulnerable`, `HeroDamaged` never fires without a `BossStruck` alongside it (no contact damage), a reactive no-damage scripted kill, the ember/`defeat_flag` grant, and no respawn on re-entry |
| `tests/playthrough.rs` | The full headless run: `Runner::new` to `GameEvent::GameWon` using only ordinary `Action`s (dialogue, item pickups, the mill puzzle, the dungeon's block/torch puzzles, a reactive boss fight, the beacon) with the milestone order (sanctuary learned → sword → lantern → dungeon entered → first key → block puzzle → torch sequence → second key → boss phase two → ember → `GameWon`) verified against the actual event stream; a tick-ceiling check well under the spec §15 target playthrough length |

## Adding a case

- A new simulation rule goes in `tests/movement.rs` (or a new file, if it stops being about
  movement) and drives `game::update` directly — no terminal, no clock.
- A new mode transition or pause rule goes in `tests/mode_machine.rs` and drives `App::apply`
  directly.
- A new render assertion goes in `tests/render.rs` using `ratatui::backend::TestBackend`; assert on
  specific cell positions, not string search over the whole buffer, since layout is fixed and
  computed (`docs/dev/loop-and-modes.md`).
- Anything that needs a `crossterm::event::KeyEvent` fixture belongs in `tests/`, not a `src/`
  unit-test module — `tests/no_key_release.rs` greps `src/**/*.rs` for the literal token
  `KeyEventKind`, and constructing a `KeyEvent` requires naming it.
- A new content validation rule needs a fixture: copy `tests/fixtures/base.ron`, introduce exactly
  one defect, and add a `matches!` assertion on the specific `ContentError` variant in
  `tests/content.rs` — the point is proving the rule actually fires, not just that "some fixture
  fails" (see `docs/dev/content.md`). A rule that only reachability can decide (a lock, the ember)
  belongs in `src/content/validate.rs`'s own `#[cfg(test)]` module instead, where a two-room inline
  `World` is cheaper to build than a fixture file.
- A new interaction, dialogue or puzzle rule against the real world goes in `tests/overworld.rs`
  (or `tests/dialogue.rs`, for anything specifically about dialogue nodes/flags) and drives the
  shared headless helpers in `tests/common/mod.rs` (`new_game`, `step`, `walk_to`, `face`, or the
  `GameState`-level `Runner` when the test needs the raw `GameEvent`s a step produced, which `App`
  folds away). Phase 5's `tests/playthrough.rs` reuses `tests/common/mod.rs` unchanged.
- A new room-transition rule goes in `tests/transitions.rs` and walks the real `assets/world.ron`
  via `content::load()`, not a hand-built `World` — it is the test that keeps the shipped content
  and the transition code honest against each other.
- A new rule about the file format itself (`stage`/`commit`/`store`/`load`) goes in `tests/save.rs`
  using `tests/common::ScratchDir` for a real directory. A new rule about *when* `App` saves (a
  trigger, a refusal, `Continue`/retry/backup-restore behaviour) goes in the same file using
  `tests/common::memory_io()`'s inspectable `MemorySaveIo` handle instead — no disk I/O needed
  unless the rule is specifically about corruption or version skew in the actual bytes on disk.

## Known gaps

- `Mode::SaveProblem`'s "Save damaged" screen and the diagnostics line for an unresolved
  `RestoreError` don't surface `LoadOutcome::Corrupt`'s `path`/`detail` (the serde error and the file
  path are computed in `src/save.rs` and then dropped in `SlotState::from`, `src/app.rs`) — a
  maintainer debugging a real damaged save currently has to reproduce it rather than read the message.
  No test pins this because there is nothing to assert; it is a known, deferred gap from phase 6's
  round-2 review (`.autodev/phases/06-save-cli-presentation/REVIEW-r2.md`).
- After a deliberate New Game over a `Usable` slot (main menu or `SaveProblem`'s New Game), if the
  hero dies before the fresh run's first autosave, `Continue`/the game-over retry correctly refuse to
  resurrect the abandoned run, but they report it as "no save yet" even though the old `save.json` is
  still intact on disk, and a second New Game in that narrow window skips the overwrite confirmation.
  Deferred in the same round-2 review as narrow and self-resolving (one room transition closes it).
- `scripts/terminal-restore-check.sh` drives a real PTY by hand; it is not part of `cargo test` and
  is not run in CI. See `docs/dev/troubleshooting.md` for the sandbox limitation that keeps it from
  running cleanly in an unattended environment.
- Linux, a real interactive SSH session with a PTY, and the ~150 ms RTT playability check are not
  reachable from this development host; CI (`ubuntu-latest` in `.github/workflows/ci.yml`) is the
  Linux evidence for build and test, not for interactive play. See ADR 0008.
- `check_object_placement` (phase 4) closed the phase-2/3 gap above: every chest/npc/torch/plate
  `at` is now bounds-, floor- and tile-conflict-checked, with fixtures
  (`broken_object_on_wall.ron`, `broken_object_tile_conflict.ron`).
- Several `ContentError` variants (`UnknownSpawn`, `PositionOutOfBounds`, `SpawnOnDoorTile`,
  `DoorTileMismatch`, `DuplicateMapIndex`, `HomeUnreachableWithEmber`, `EmberMissing`,
  `TooManySmallKeyDoors`, `EnemyPatrolInvalid`) still have no fixture or test that makes the
  validator actually produce them; only `error.rs`'s `Display` test exercises their message text.
  `route.ember_required` is `true` and the real world grants the ember (via the boss) since phase 5,
  so `real_world_validates` now exercises the "ember reachable and home reachable afterward" path
  that `EmberMissing`/`HomeUnreachableWithEmber` guard against, even without a dedicated negative
  fixture for either.

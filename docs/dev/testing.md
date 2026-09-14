# Testing

## Running the suite

```sh
cargo test --locked
```

Runs everything: unit tests inside `src/**/*.rs` (`#[cfg(test)]` modules in `game/rng.rs`,
`game/entities.rs`, `game/world.rs`, `game/state.rs`, `render/scene.rs`, `render/tiles.rs`,
`render/theme.rs`, `render/hud.rs`, `content/schema.rs`, `content/error.rs`, `content/loader.rs`,
`content/validate.rs`) and the integration suites under `tests/`. There is no separate e2e command
— the headless playthrough planned for a later phase and the room-transition checks already landed
are ordinary `#[test]` functions under `tests/`, not a second harness.

Full gate, run before every commit and in CI:

```sh
cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked
```

## What each integration file covers

| File | Covers |
|---|---|
| `tests/deps.rs` | `Cargo.lock` has exactly one `crossterm` entry; `Cargo.toml` never names it directly |
| `tests/config.rs` | CLI parsing, flag defaults and conflicts, colour precedence, save-dir resolution, and two spawned-binary checks (non-TTY stdin, `TERM=dumb`) asserting exit code, one stderr line, and empty stdout |
| `tests/movement.rs` | Four-directional movement, wall/water/bush/out-of-bounds collision, facing-on-blocked-move, step cooldown, determinism of `game::update` for a fixed seed and action sequence |
| `tests/input_policy.rs` | Key-to-action mapping per mode, `coalesce`, the overflow policy (a burst larger than the per-iteration cap keeps the newest movement, not the oldest), `drop_pending` |
| `tests/no_key_release.rs` | Scans `src/**/*.rs` for `KeyEventKind`/`Release`/keyboard-enhancement identifiers and fails if any appear — a structural check that no key-release path is ever added |
| `tests/mode_machine.rs` | Mode transitions (menu, pause, confirm-quit, help, resize into/out of `TooSmall`); `HeroDied` opening `Mode::GameOver`, the simulation not advancing while there, retry restoring the last room-entry checkpoint (health included) and rewinding the tick counter, and `Cancel` returning to the main menu |
| `tests/render.rs` | `TestBackend` assertions: hero/wall/HUD/message/hint cell positions at 60x24 and 80x24, the too-small notice at 59x23, enemy/sword/telegraph glyph placement at 60x24, and that the same scene renders identical characters under all three themes |
| `tests/terminal_guard.rs` | Restoration order and idempotence via `RecordingOps`, restore-before-panic-message, a partial failure during `enter()` still disabling raw mode |
| `tests/loop_timing.rs` | `Pacer`/`advance_iteration` in isolation: byte-identical `GameState` across fps 10/20/30 for the same action schedule, catch-up capped at 5 steps with surplus discarded, draw cadence scaling with fps while tick count does not, a keypress in a `sim_steps == 0` iteration is not lost, held-key movement lands at the cooldown rate |
| `tests/content.rs` | `content::validate` against the real `assets/world.ron` (9 rooms, distinct 3x3 `map_index` values) and against `tests/fixtures/`: `base.ron` validates, and each `broken_*.ron` fixture is rejected with its specific named `ContentError` variant (missing door target, non-reciprocal door, spawn in a wall, duplicate id, wrong dimensions, illegal tile, key behind its own lock, ember unreachable, a spawn walled into its own pocket, an unknown RON field); `broken_three_defects.ron` returns three distinct variants from one call |
| `tests/transitions.rs` | Walking through every door in the real world: the hero lands in the declared `to_room` on a walkable, non-door tile, and the reciprocal door leads back adjacent to the door taken; `progress.visited` grows by exactly one per newly entered room and not on re-entry; `GameEvent::RoomEntered` fires once per transition, never for the start room, and not at all when a move next to a door is blocked |
| `tests/content_startup.rs` | Spawns the real binary with the hidden `--debug-content PATH` flag: a broken fixture exits 2 with the content error list on stderr (and not the TTY-refusal message, proving the abort happens before raw mode); a valid fixture passes the content preflight |
| `tests/combat.rs` | The sword hitbox (exactly the faced tile, four facings, side/rear tiles miss), the per-swing `hit` list (two enemies on the struck tile each damaged at most once), the cooldown (an `Attack` inside it is dropped, the next one outside it lands), contact damage and its invulnerability window, `combat::knockback` stopping adjacent to a wall and never landing on a door tile, and a killed enemy staying dead (no further movement or `AiState` change) |
| `tests/ai.rs` | A hand-built maze `World` (no loader, no validator — built directly from `content` schema types): each of the three kinds run 1000 ticks asserting `AiState` never goes `MAX_STALL_TICKS` without changing and the enemy never leaves the room or steps onto a non-walkable tile; the guardian's telegraph precedes its first dash by at least `TELEGRAPH_MIN_TICKS` and it only takes sword damage in `GuardianRecover`; two enemies dashing into the same tile resolve by `Vec` order (ADR 0004); the real `assets/world.ron` authors zero enemies in `room.lighthouse` and `room.crossroads` (the pacing-safe first stretch, RISKS #2) |
| `tests/determinism.rs` | A 2000-action pseudo-random sequence (from a test-local `Rng`, independent of `state.rng`) replayed twice from the same seed produces an identical `state_hash()` at every tick; the same sequence under two different seeds diverges somewhere; the same sequence driven through `App`/`Pacer` at `--fps` 10/20/30 produces an identical hash trace regardless of batching |

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
- A new room-transition rule goes in `tests/transitions.rs` and walks the real `assets/world.ron`
  via `content::load()`, not a hand-built `World` — it is the test that keeps the shipped content
  and the transition code honest against each other.

## Known gaps

- `scripts/terminal-restore-check.sh` drives a real PTY by hand; it is not part of `cargo test` and
  is not run in CI. See `docs/dev/troubleshooting.md` for the sandbox limitation that keeps it from
  running cleanly in an unattended environment.
- Linux, a real interactive SSH session with a PTY, and the ~150 ms RTT playability check are not
  reachable from this development host; CI (`ubuntu-latest` in `.github/workflows/ci.yml`) is the
  Linux evidence for build and test, not for interactive play. See ADR 0008.
- `content::validate` does not bounds- or walkability-check `Chest.at` or `Npc.at` (a chest or NPC
  authored at an out-of-bounds or walled-in position parses and validates clean today).
  `EnemySpawn.at` and its patrol waypoints gained `check_enemy_spawns` in phase 3; `Chest`/`Npc`
  should get the same treatment before phase 4 authors any (phase 2 round-2 review, minor finding
  1, narrowed by phase 3).
- Seven `ContentError` variants (`UnknownSpawn`, `PositionOutOfBounds`, `SpawnOnDoorTile`,
  `DoorTileMismatch`, `DuplicateMapIndex`, `HomeUnreachableWithEmber`, `EmberMissing`) have no
  fixture or test that makes the validator actually produce them; only `error.rs`'s `Display` test
  exercises their message text. `HomeUnreachableWithEmber` and `EmberMissing` matter most, since
  both stay dormant until phase 5 sets `route.ember_required: true` (phase 2 round-2 review, minor
  finding 2).

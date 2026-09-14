# Architecture summary

A pointer-heavy summary for a reader who already has the code open, not a copy of
`.autodev/ARCHITECTURE.md` (the original design-of-record, still the place to read the *why* behind
a decision in full). This file exists so a new contributor can orient in five minutes: where each
concern lives, what the pure/impure boundary actually is, and which ADR to read for a given corner.

## Module map

```
src/main.rs     argv -> Config -> TerminalGuard -> loop -> exit code (the only terminal owner)
src/lib.rs      module tree; the surface tests use
src/config.rs   clap parser, Config, TTY / TERM / NO_COLOR probing, cli_command() for docs_cli.rs
src/terminal.rs raw mode, alternate screen, RAII guard, panic hook, signal flags, size probe
src/input.rs    KeyEvent -> Action, per-mode maps, coalescing, the overflow policy, event draining
src/app.rs      screen/mode machine, Pacer, advance_iteration, draw_due, autosave triggers
src/render/     pure projection: theme.rs tiles.rs hud.rs scene.rs overlays.rs
src/game/       pure simulation: state.rs world.rs entities.rs combat.rs ai.rs puzzles.rs
                tuning.rs rng.rs balance.rs
src/content/    RON schema, include_str! loader, validator
src/save.rs     versioned JSON, path resolution, atomic write, backup rotation
assets/world.ron  the entire world, embedded at compile time
tests/          integration tests; tests/common/ holds the shared headless drivers
```

## The pure-core boundary

The one architectural rule everything else is built to protect: **`src/game/` is a pure function**

```
game::update(&mut GameState, &[Action], tick: Tick) -> Vec<GameEvent>
```

`game/` imports neither `std::io`, `std::time`, nor `ratatui`. Time arrives only as an integer
`Tick` (`game::tuning::TICK_HZ` = 30). This is what makes the determinism test
(`tests/determinism.rs`) and the headless playthrough (`tests/playthrough.rs`,
`tests/common/route.rs`) possible without a terminal or a clock, and it is why `game::balance`
(below) is a plain function over `World`/`tuning` and never a method `update` calls.

`src/render/` mirrors this on the output side: it only *reads* `GameState`/`App`, projecting them
into a `ratatui::Frame`. Rendering never mutates game state, and a `Theme` only changes colour —
every glyph is identical in every theme (`tests/render_modes.rs`), so monochrome play never loses
information a colour theme carries.

Everything with a side effect — the terminal, the clock, the filesystem, the RNG's seed source —
lives outside `game/`: `main.rs`, `terminal.rs`, `app.rs`, `save.rs`. `App` (in `app.rs`) is the
seam: it holds a `GameState`, translates raw `Action`s into calls to `game::update`, and turns the
resulting `GameEvent`s into UI-facing effects (messages, mode transitions, autosave triggers) that
`game/` itself knows nothing about.

## The loop and the draw gate

`main.rs::run` owns the only `Terminal`/`TerminalGuard` in the process. Each iteration:

1. `poll_events` blocks (with a pacer-computed deadline) for the first event, then
   `input::drain_ready` drains everything already buffered — a burst never survives to replay as
   stale input across iterations (spec §5).
2. Raw events map through `input::map_key` (per-`Mode`) into `Action`s, then
   `input::apply_overflow_policy` + `coalesce` cap the batch and collapse it to at most one
   movement action.
3. `app::advance_iteration` applies mode-transition actions immediately via `App::apply`, merges
   whatever lands on the simulation into a `pending` buffer that survives across iterations (a
   keypress arriving in a tick-less iteration is not lost — see ADR 0001's rationale and
   DECISIONS.md's phase-1 round-2 entries), and steps `game::update` through `Pacer::advance`'s
   `Due { sim_steps, draw }`, capped at `tuning::MAX_CATCHUP_STEPS`.
4. **The draw gate**, `app::draw_due(&mut App, Due) -> bool = due.draw && app.take_dirty()`, decides
   whether this iteration actually calls `Terminal::draw`. It consumes the dirty bit, so it must run
   exactly once per iteration. This one function is why a paused game or a static menu writes zero
   bytes after its first frame (`tests/metrics.rs`) — `main.rs`, `tests/loop_timing.rs`, and the
   byte-counting harness (`tests/common/metrics.rs`) all call the same function, so the
   zero-bytes-while-idle claim is about the real draw gate, not a copy of it.

`Pacer` is clock-free: it takes `elapsed_ns` and returns `Due`, which is what makes fps-independence
(`tests/loop_timing.rs::draw_cadence_scales_with_fps_while_simulation_tick_count_does_not`,
`tests/metrics.rs::output_scales_with_fps_not_with_simulation`) provable without a terminal. See
`docs/dev/loop-and-modes.md` for the full mode machine.

## The tick model

Simulation time is `game::state::Tick = u64`, incremented once per `game::update` call, at a fixed
30 Hz regardless of `--fps`. Every timed thing in the simulation — the hero's step cooldown, sword
cooldown/active window, invulnerability, every enemy AI timer, the boss's phase timers — is an
absolute tick deadline (`until: Tick`), not a countdown duration, so restoring a checkpoint or a
save rewinds `App::tick_counter` to match rather than leaving every timer's deadline stale (see
DECISIONS.md's phase-3 entry on retry/restore). `game::rng::Rng` is a hand-rolled SplitMix64 seeded
from `--seed`, not the `rand` crate — a dependency bump must never be able to change what a fixed
seed produces (ADR 0002).

## The run-length model (`src/game/balance.rs`)

Spec §1's 30-45 minute target playthrough is a tested property (`tests/balance.rs`), not an
intention. `tests/common/route.rs::play_to_victory` is a *perfect* headless run — every door known,
no optional fight, no backtracking, no death — and finishes in a small fraction of the target
(under 3 simulated minutes, asserted). A first-time player's run is not that number times one fudge
factor (that would be circular); instead `balance::estimate_first_playthrough` sums named terms,
each derived from `tuning` or from what `World` actually contains:

```
total = travel + dialogue + combat + puzzle + boss_retries + menu

travel   = optimal_ticks * REVISIT_FACTOR * PATHING_FACTOR(5/2) * PAUSE_FACTOR   (integer factors only)
dialogue = (authored DialogueNode count) * DIALOGUE_READ_TICKS
combat   = Σ engage_ticks(kind) over every authored non-boss EnemySpawn
puzzle   = (authored Puzzle count) * PUZZLE_THINK_TICKS
boss     = BOSS_DEATHS_ASSUMED * (boss_fight_ticks() + BOSS_RETURN_TICKS)
menu     = MENU_TICKS
```

`engage_ticks` and `boss_fight_ticks` are themselves derived from `tuning`'s real combat constants
(sword cooldown, enemy HP, the guardian's telegraph/dash/recover windows, the boss's per-phase
stalk/windup/vulnerable windows) — `boss_fight_ticks` counts the *full* vulnerable window per hit,
the same way `engage_ticks`'s guardian case counts the whole recovery window per hit, which is what
makes `BOSS_HP` and the phase-two vulnerable window real levers on the estimate rather than
decorative ones (see DECISIONS.md, phase-07/implement, for the T4 balance pass that used exactly
this lever). The estimator's own parameters (`REVISIT_FACTOR`, `PATHING_FACTOR`, `PAUSE_FACTOR`,
`DIALOGUE_READ_TICKS`, `PUZZLE_THINK_TICKS`, `BOSS_DEATHS_ASSUMED`, `BOSS_RETURN_TICKS`,
`MENU_TICKS`) live in `balance.rs`, not `tuning.rs` — they are assumptions about a human, never read
by the simulation, and `tuning.rs` stays the file a balance change to the *game* touches.

At the tuned content (phase 7's T4), the estimate lands at roughly 33 minutes — inside the band with
real margin from both edges. This is the single weakest claim in the whole build: it depends on the
estimator's parameters being roughly right, which nothing here proves against a real player. See
`HANDOFF.md`. `PAUSE_FACTOR` in particular carries more weight than the others: at `PAUSE_FACTOR = 2`
(PLAN.md's originally pinned value) the same content estimates under the 30-minute floor, so this one
parameter, not T4's content pass, is what keeps the band test green — see
`.autodev/DECISIONS.md`'s phase-07/review-r1 entry for why it was kept at 3 anyway.

## The content pipeline

`assets/world.ron` is hand-authored RON, embedded via `include_str!` at compile time (ADR 0005). It
decodes into `content::schema` types, then `content::validate` runs a battery of structural checks
(legal tiles, unique ids, door/spawn targets, two-way reciprocity, object placement) plus a
memoised BFS over `(unlocked-door bitmask, item bitmask)` states that proves every room and lock is
reachable and the ember/home route resolves — a validator this load-bearing is itself tested against
deliberately broken fixture worlds (`tests/content.rs`, `tests/fixtures/`). Validation runs in
tests, in CI, and at startup (`main.rs`'s `content_preflight`, before the terminal is touched), so
unfinishable content is refused rather than shipped.

## The save format

One slot, JSON, at a path resolved per-platform (`docs/user/cli.md`) or overridden by `--save-dir`.
`save::stage`/`commit` write to a temp file, `fsync`, then rename over the real one, keeping the
previous copy as `.bak` — a crash mid-write can never corrupt the slot in place (ADR 0006). Content
ids are stable, namespaced strings (`room.lighthouse`, `chest.forest_sword`), stored as strings in
the save and resolved to indices only in memory, so adding content is non-breaking and a human can
read a save file while debugging. `LoadOutcome` is an explicit `Ok | Missing | Corrupt |
FutureVersion` enum; nothing on the save path calls `unwrap` (checked structurally by
`tests/save.rs`).

## The terminal lifecycle

One `TerminalGuard`, entered once in `main.rs::run`, with an idempotent `Drop` and a panic hook that
shares the guard's own `restored` flag so whichever runs first — the panic hook or `Drop` — is the
only one that restores (ADR 0007; see DECISIONS.md's phase-1 round-1 entry for the double-restore
bug this closed). SIGTERM/SIGHUP are `AtomicBool` flags polled by the main loop
(`signal-hook`, the one dependency beyond the four ADR 0003 names) rather than handled inline, since
a signal handler doing file I/O is unsafe. `docs/dev/troubleshooting.md` has the operational
version of this section; `scripts/terminal-restore-check.sh` is the host-run confirmation.

## ADRs

| ADR | Decision |
|---|---|
| [0001](adr/0001-single-threaded-fixed-timestep-loop.md) | Single-threaded, fixed-timestep loop with poll-based input — no input thread, no async runtime |
| [0002](adr/0002-deterministic-integer-tick-simulation.md) | Integer `Tick` time, integer tile positions, a seeded in-crate SplitMix64 PRNG |
| [0003](adr/0003-ratatui-with-vendored-crossterm-reexport.md) | `ratatui` only, via its `ratatui::crossterm` re-export — never declare `crossterm` directly |
| [0004](adr/0004-plain-structs-instead-of-ecs.md) | Plain structs/enums/`Vec<Enemy>`, no ECS; `Vec` iteration order is the deterministic tie-break |
| [0005](adr/0005-world-content-in-embedded-ron.md) | World authored in RON, embedded via `include_str!`, machine-validated |
| [0006](adr/0006-versioned-json-save-with-atomic-write.md) | Single save slot, versioned JSON, atomic write with one retained backup |
| [0007](adr/0007-terminal-lifecycle-guard-panic-hook-signals.md) | One RAII terminal guard, a panic hook sharing its restore flag, flag-based signal handling |
| [0008](adr/0008-single-binary-packaging-and-ci-matrix.md) | One self-contained binary; `ubuntu-latest` + `macos-latest` CI as the portability check; other targets expected-but-unverified |
| [0009](adr/0009-one-time-events-recorded-as-story-flags.md) | One-time world events (including boss defeat) recorded as story flags, not dedicated fields |

## Where the build diverged from the design-of-record

`.autodev/ARCHITECTURE.md` was written before any code existed. Where it and the code disagree, **the
code is the truth** — this table is the reconciliation, so nobody reads the design doc and expects a
signature or a flag that never shipped. Each row links the decision that moved it.

| `.autodev/ARCHITECTURE.md` says | What was actually built | Why / where it is logged |
|---|---|---|
| `--log-file PATH` file sink, and a `--debug` overlay showing tick/fps/entity count/bytes per second ("Logging / observability") | **Neither exists.** The deferred diagnostic buffer flushed to stderr after the guard drops is the whole observability story, plus the hidden `--debug-panic` and `--debug-content PATH` flags. | No spec §13 check or §12 flag ever needed them. Phase 7 judged adding a new CLI surface in the final phase worse than documenting the gap. DECISIONS.md, PLAN phase 7. |
| `--unicode` "may be deferred as a whole" | **Withdrawn entirely.** `--unicode` is a usage error (exit 2), not an accepted no-op. ASCII is the one glyph table. | Every Unicode block that would beat ASCII is `East_Asian_Width=Ambiguous` and can shear the fixed tile grid. `docs/user/cli.md`, "The Unicode decision"; ADR 0003's consequences. |
| `content::load() -> Result<World, ContentError>` | `content::load() -> Result<World, Vec<ContentError>>` | The validator reports *every* defect in one pass rather than the first — that is what makes a broken world one edit-and-recheck cycle instead of N. `tests/content.rs::broken_three_defects` pins it. Phase 2. |
| `render::draw(frame, app, theme: &Theme)` | `render::draw(frame, app, theme: Theme)` — by value | `Theme` is a small `Copy` enum, not a palette struct; a reference bought nothing. Phase 1. |
| `Action` listed without `Help` | `Action::Help` exists (bound to `?`) | Help is reachable from MainMenu/Playing/Paused and needed its own semantic action. Phase 1. |
| `save` sketched as free functions | `App` owns a `Box<dyn SaveIo>` port, with `FileSaveIo` and `MemorySaveIo` | Keeps the filesystem out of every test that only cares about *when* `App` saves. Already amended in `.autodev/ARCHITECTURE.md`'s component table; DECISIONS.md, PLAN phase 6. |
| `--save-dir` "canonicalized and checked writable" | Created if missing and reported clearly if unusable; not canonicalized | Canonicalizing a not-yet-existing directory fails on both platforms; the writability failure surfaces at first write as a `StoreOutcome::Failed` diagnostic. Phase 6. |
| Enemy AI as per-kind nested enums | One flat `AiState` enum covering all four kinds including the boss | Keeps every transition in one `match` in `game/ai.rs`. ADR 0004's phase-3 and phase-5 amendments. |

Two assumed NFRs came out far under budget and are worth knowing before anyone optimises something
that does not need it: the release binary is **2.3 MB** against an assumed 12 MB ceiling, and peak RSS
across a measured play-plus-pause run was **3.5 MB** against an assumed 32 MB. The tuning constant
`BOSS_HP` is 12, not the 8 phase 5 shipped — phase 7's balance pass raised it to move the run-length
estimate. Numbers and method: `docs/dev/verification-report.md`.

## Where to look next

- `docs/dev/loop-and-modes.md` — the mode machine in full, every transition.
- `docs/dev/content.md` — how to add a room, an enemy kind, or a screen.
- `docs/dev/testing.md` — what each test file proves and how to add a case.
- `docs/dev/verification-report.md` — what was actually run, on what host, and what was not.
- `.autodev/ARCHITECTURE.md` — the original design-of-record with full rationale and alternatives
  considered; `.autodev/DECISIONS.md` — every deviation from it, phase by phase, with why.

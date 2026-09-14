# Phase 7 — Balance, measurement, documentation and verification report

Goal: make the 30–45 minute target a checkable property instead of an intention, measure the two things §13
asks to be measured (terminal output volume and CPU, with the environment named), finish the user- and
developer-facing documentation, and write a verification report that is honest about which environments were
actually exercised.

## Context

**What exists after phase 6**

| Area | State |
|---|---|
| `src/game/tuning.rs` | 40 constants, all in ticks: hero step 4, sword 9/3, invuln 24, telegraph ≥18, per-kind enemy constants, `BOSS_*` (HP 8, phase-two at 4, stalk 60/45, windup 24/18, vulnerable 45/30). No constant relates to *run length*; nothing derives a playthrough duration from them. |
| `tests/playthrough.rs` | Drives New Game → `GameWon` with ordinary `Action`s only, asserts milestone order, and asserts `state.tick` is under a 40-simulated-minute ceiling. **Measured on 2026-09-14 on the host Mac: the optimal route finishes in 1955 ticks = 65.2 s of simulated time.** The route walker is ~200 lines inline in that file and cannot be reused. |
| `assets/world.ron` | 15 rooms (9 overworld + 6 dungeon), 3 NPCs, 18 regular enemy spawns (8 slime, 7 bat, 3 guardian), 1 boss, sword + lantern, one `BlockOnPlates` and one `TorchSequence` puzzle, 3 secret chests (`chest.ridge_lore`, `chest.pines_heart`, `chest.shore_key`). **Exactly one `HeartContainer`** — `state.rs` caps `max_health_halves` at 10 (5 hearts, §2), but the authored content lets a player reach at most 4 hearts, so §2's "maximum 5" is unreachable. |
| `src/main.rs` | The only terminal owner. The draw gate is the inline expression `due.draw && app.take_dirty()`; `Terminal::new(CrosstermBackend::new(io::stdout()))`. Nothing counts bytes. |
| `src/app.rs` | `Pacer`, `Due { sim_steps, draw }`, `advance_iteration`, `App::take_dirty` — all in the library, all clock-free (time arrives as `elapsed_ns`). `tests/loop_timing.rs` already re-implements the draw gate inline twice. |
| `src/config.rs` | Private `struct Cli` (clap `Parser`): `--ascii --color --theme --fps --save-dir --seed`, plus hidden `--debug-panic` / `--debug-content`. No way for a test to reach clap's own flag list. |
| `docs/user/` | `cli.md` (84 lines: flag table, the Unicode withdrawal, colour precedence, save directories per OS, corrupt/incompatible-save behaviour, startup refusal, exit codes) and `controls.md` (68 lines: full binding table with the modes each key is live in). **No SSH/tmux page.** |
| `docs/dev/` | `development.md`, `testing.md`, `loop-and-modes.md`, `troubleshooting.md`, `content.md`, `adr/0001…0009`. **No `architecture.md`, no `verification-report.md`.** |
| `README.md` | Build, run, controls, an SSH section, terminal-recovery, development. Ends with a "Known limitations (phase 6)" section that names the balance/measurement pass as not yet done. |
| `scripts/` | `terminal-restore-check.sh` only: host-run, `expect`-driven, sets an explicit pty size, greps the transcript for scenario-specific content so a misconfigured pty fails instead of silently passing. This is the pattern every manual check in this phase follows. |
| CI | `.github/workflows/ci.yml`: `ubuntu-latest` + `macos-latest`, `rustup show` → `cargo fetch --locked` → fmt → clippy `-D warnings` → `cargo test --locked` → `cargo build --release --locked`. |
| Tests | 25 integration files, ~17.6 kloc total. `tests/common/mod.rs` is the shared headless driver (`cfg`, `new_game`, `step`, `idle`, `face`, `walk_to`, `Runner`). No test touches a real terminal; none counts output bytes; none reads a doc file. |

**What this phase changes**

1. **A run-length model exists and is tested.** New pure module `src/game/balance.rs` turns the measured
   optimal-route tick count plus the authored world's workload into an estimated first-playthrough duration,
   derived from `tuning`'s constants. `tests/balance.rs` asserts the estimate lands inside 30–45 minutes.
2. **`tuning.rs` and `assets/world.ron` get a balance pass** aimed at the middle of that band.
3. **Terminal output is measured for real**, by driving the actual `CrosstermBackend` write path into a
   byte-counting writer; the paused case is asserted to be exactly zero bytes after the first frame.
4. **CPU is measured on the host** by an `expect`-driven script, and the number is reported with the machine
   named — it is not, and cannot honestly be, a `cargo test` assertion.
5. **Docs close**: `docs/user/ssh.md` (new), a flag-diff test and a binding-coverage test that keep
   `docs/user/` from drifting, `docs/dev/architecture.md` (new), `docs/dev/verification-report.md` (new),
   README's limitations section rewritten, `HANDOFF.md` (new).
6. **Main-route stub freedom becomes a test** (`tests/no_stubs.rs`) rather than a review habit.

**Key files:** `src/game/balance.rs` (new), `src/game/mod.rs`, `src/game/tuning.rs`, `src/app.rs`,
`src/main.rs`, `src/config.rs`, `assets/world.ron`, `tests/common/{mod,route,metrics}.rs`,
`tests/{balance,metrics,docs_cli,docs_controls,no_stubs,playthrough,content}.rs`,
`scripts/{manual-checks.sh,measure-cpu.sh}` (new), `docs/user/ssh.md`, `docs/dev/architecture.md`,
`docs/dev/verification-report.md`, `README.md`, `HANDOFF.md`, `CHANGELOG.md`, `docs/dev/testing.md`.

## Design

### 1. The run-length model (`src/game/balance.rs`)

The headless run is a *perfect* route: it knows every door, never fights an optional enemy, never re-treads a
room, never stops to read, and never dies. It finishes in 1955 ticks (65 s). A first-time player's run is not
that number scaled by a single fudge factor picked to hit the target — that would be circular and would prove
nothing. Instead the estimate is an itemised sum whose every term is derived either from `tuning`'s constants
or from what the world actually contains, so deleting half the enemies, halving `BOSS_HP`, or doubling
`HERO_STEP_TICKS` all move it:

```
estimate = travel + dialogue + optional_combat + puzzle_thinking + boss_retries + menus

travel          = optimal_ticks * REVISIT_FACTOR * PATHING_FACTOR * PAUSE_FACTOR
dialogue        = authored_dialogue_nodes * DIALOGUE_READ_TICKS
optional_combat = Σ over authored regular spawns: engage_ticks(kind)
puzzle_thinking = authored_puzzles * PUZZLE_THINK_TICKS
boss_retries    = BOSS_DEATHS_ASSUMED * (boss_fight_ticks() + BOSS_RETURN_TICKS)
menus           = MENU_TICKS
```

Module surface (pure — no `std::io`, no `std::time`, no `ratatui`; `Tick` arithmetic in `u64`, factors as
integer numerator/denominator pairs so there is no float in the simulation crate's core path):

```rust
/// Per-component breakdown of an estimated first playthrough, in ticks.
pub struct RunEstimate {
    pub travel_ticks: Tick,
    pub dialogue_ticks: Tick,
    pub combat_ticks: Tick,
    pub puzzle_ticks: Tick,
    pub boss_retry_ticks: Tick,
    pub menu_ticks: Tick,
    pub total_ticks: Tick,
}

impl RunEstimate {
    pub fn total_seconds(&self) -> u64;   // total_ticks / TICK_HZ
    pub fn total_minutes(&self) -> u64;   // integer minutes, floor
}

/// Ticks a first-time player spends on one enemy of `kind`: closing the distance, the hits its HP
/// needs at `SWORD_COOLDOWN_TICKS`, the misses, and — for the guardian — waiting out
/// telegraph + dash + recovery for each hit, since recovery is its only vulnerable window.
pub fn engage_ticks(kind: EnemyKind) -> Tick;

/// Ticks one full boss fight takes at the tuned `BOSS_*` constants: for each of `BOSS_HP` hits, the
/// phase's stalk + windup ticks plus the hit itself inside the vulnerable window, phase one for the
/// first `BOSS_HP - BOSS_PHASE_TWO_HP` hits and phase two for the rest.
pub fn boss_fight_ticks() -> Tick;

/// The whole estimate. `optimal_ticks` is what the headless route actually took; everything else is
/// counted out of `world`.
pub fn estimate_first_playthrough(world: &World, optimal_ticks: Tick) -> RunEstimate;

/// The §1 target band, in ticks, so the test and the docs cannot disagree about it.
pub const TARGET_MIN_TICKS: Tick = 30 * 60 * TICK_HZ;
pub const TARGET_MAX_TICKS: Tick = 45 * 60 * TICK_HZ;
```

The estimator parameters (`REVISIT_FACTOR = 3`, `PATHING_FACTOR = 5/2`, `PAUSE_FACTOR = 2`,
`DIALOGUE_READ_TICKS`, `PUZZLE_THINK_TICKS`, `BOSS_DEATHS_ASSUMED = 2`, `BOSS_RETURN_TICKS`, `MENU_TICKS`)
live in `balance.rs`, not in `tuning.rs`: `tuning.rs` is documented as the file where *simulation* constants
live, and these are assumptions about a human, not values the simulation reads. Each carries a doc comment
naming the player behaviour it stands for, and `docs/dev/architecture.md` repeats the model so a later reader
can argue with the numbers instead of guessing at them. Fixing the parameters happens in T2, **before** T4
looks at what the current content scores — the balance pass then moves the *game*, never the parameters.

Rough arithmetic at today's content (for orientation, not a promise): travel 1955 × 15 = 29 325; dialogue and
puzzles a few thousand each; 18 regular enemies at ~10 s each ≈ 5 400; two boss retries at the tuned
constants ≈ 5 000. That lands near the bottom of the band, which is why T4 is a real task and not a rubber
stamp.

### 2. Sharing the route: `tests/common/route.rs`

`tests/balance.rs` needs the same start-to-victory route `tests/playthrough.rs` drives, and neither file may
be the other's dependency. The walker moves to `tests/common/route.rs`:

```rust
pub struct RouteOutcome { pub events: Vec<GameEvent>, pub ticks: Tick }
/// New Game → GameWon using only `Action`s. Panics with a located message if the route breaks.
pub fn play_to_victory(runner: &mut Runner) -> RouteOutcome;
```

`tests/playthrough.rs` keeps every assertion it has today (victory reached, milestone order, tick ceiling)
and loses only the walking code. This is a pure move — no assertion is weakened, and the phase gate proves
the route still works.

### 3. Measuring terminal output (`tests/common/metrics.rs`, `tests/metrics.rs`)

The only measurement worth reporting is the byte count the real backend actually writes, so the harness
drives `ratatui`'s `CrosstermBackend` — the same type `main.rs` uses — over a counting writer instead of
stdout:

```rust
#[derive(Default, Clone)]
pub struct ByteCounter(Rc<Cell<usize>>);           // io::Write: counts, discards
pub struct Harness { terminal: Terminal<CrosstermBackend<ByteCounter>>, app: App, pacer: Pacer, … }
impl Harness {
    pub fn new(size: (u16, u16), fps: u32, seed: u64) -> Self;
    /// One main-loop iteration with `elapsed_ns` of wall time and these actions. Mirrors
    /// `main.rs`'s body: `advance_iteration`, then draw iff `app::draw_due(&mut app, due)`.
    pub fn iterate(&mut self, elapsed_ns: u64, actions: &[Action]);
    pub fn bytes(&self) -> usize;
}
```

`Terminal` is built with `TerminalOptions { viewport: Viewport::Fixed(Rect::new(0, 0, 80, 24)) }` so
construction never calls `crossterm::terminal::size()` — CI has no TTY. If ratatui 0.30.2 still probes on
some path, the fallback is to call `Backend::draw`/`flush` directly with the same buffer diff; the byte count
is identical either way and no test may be relaxed to dodge this.

To keep the harness measuring the *real* gate rather than a copy of it, the inline
`due.draw && app.take_dirty()` in `main.rs` becomes one library function:

```rust
// src/app.rs
/// The draw gate: a frame is drawn only when the pacer says a frame is due **and** something
/// changed. Consumes the dirty bit, so it must be called exactly once per iteration.
pub fn draw_due(app: &mut App, due: Due) -> bool { due.draw && app.take_dirty() }
```

`main.rs` and `tests/loop_timing.rs` both switch to it. Without this, the zero-bytes-while-paused test could
pass against a gate the shipped binary does not use.

`tests/metrics.rs` cases:

| Case | What it drives | Assertion |
|---|---|---|
| `active_play_stays_inside_the_output_budget` | 60 s of simulated wall time at 20 fps, 80×24, hero moving continuously through a room with live enemies (a scripted action stream, not the victory route) | bytes < 200 KB (the ARCHITECTURE budget); prints `bytes/min` and `KB/min` for the report |
| `a_paused_game_with_no_input_writes_nothing_after_the_first_frame` | enters `Mode::Paused`, one iteration to flush the first frame, then 60 s of iterations with no actions | byte count **exactly unchanged** after the first frame |
| `a_static_main_menu_writes_nothing_after_the_first_frame` | same, at the main menu | byte count exactly unchanged |
| `output_scales_with_fps_not_with_simulation` | the same 60 s of play at `--fps` 10 and 30 | 30 fps writes more bytes than 10 fps, and both simulate the identical tick count (guards RISKS #8 from the output side) |

The printed numbers are the ones copied into `docs/dev/verification-report.md`, with the host named.

### 4. Measuring CPU (`scripts/measure-cpu.sh`)

CPU load cannot be asserted in `cargo test` without making the suite flaky on a shared CI runner, and §13
asks for a measurement with the environment named, not a gate. So it is a host-run script in the
`terminal-restore-check.sh` mould: `expect` spawns the release binary on a sized pty, sends a scripted stream
of key presses for 60 s of play and then 60 s of pause, while a background `ps -o %cpu=,rss= -p <pid>`
samples once a second. The script prints the samples, their mean and max, and `uname -a` + `sw_vers` +
`rustc -V` so the environment is in the transcript itself. Its output is pasted verbatim into the report. It
is not wired into `cargo test` and not into CI.

### 5. Manual check pass (`scripts/manual-checks.sh`)

One script, one case per §13 manual line that is reachable on this host, each following
`terminal-restore-check.sh`'s rule: set the pty size explicitly, drain output while waiting, and grep the
transcript for a string only that scenario could produce — so a broken case fails loudly instead of passing
on an empty transcript. Every case runs with `--save-dir "$(mktemp -d)"`.

| Case | Method | Evidence grepped for |
|---|---|---|
| Local run, 80×24 | pty at 80×24, New Game, walk, quit | the room's own glyphs and the HUD hearts |
| Local run, 60×24 | pty at 60×24 | the same, at the minimum size |
| Below minimum (59×24) | pty at 59×24 | the too-small screen's required-vs-current text |
| Live resize | start at 80×24, `stty` the pty to 59×24 mid-run, then back | too-small text appears, then **Paused** on recovery (never straight into play) |
| Monochrome | `--theme mono --color never` | the scene glyphs present **and** no SGR colour sequence in the transcript |
| Held key | 40 movement keys sent back to back in one burst | the hero's position moved by far fewer tiles than 40 — the §5 no-backlog rule, read off the transcript |
| Save / quit / continue | play, Esc → save, Q → confirm quit, relaunch same `--save-dir` | `Continue` label reflecting a present save, and the restored room's name |
| Terminal after normal exit | `stty -a` before/after | identical line-discipline flags |
| Terminal after a panic | `--debug-panic`, `stty -a` before/after | identical flags, panic message on stderr |
| tmux | `tmux new-session -d` + `send-keys`, `capture-pane` | scene glyphs inside the captured pane (skipped with a printed reason if `tmux` is absent — never silently) |

Not reachable here and therefore **not** faked: Linux, a real SSH session over a network, ~150 ms RTT, Intel
macOS, aarch64 Linux. The script prints them as `SKIPPED (no such environment on this host)` so the
transcript itself carries the honesty, not just the prose around it.

### 6. Documentation-drift tests

Two tests keep `docs/user/` honest, both reading the doc files at run time from `CARGO_MANIFEST_DIR`:

- `tests/docs_cli.rs` — `config::cli_command() -> clap::Command` is added (a one-line public accessor for the
  private `Cli`, since `Parser::command()` cannot be reached from outside). The test collects clap's
  non-hidden long flags, adds `--help`/`--version`, parses the `` | `--flag` | `` column of `docs/user/cli.md`'s
  table, and asserts set equality in **both** directions — an undocumented flag and a documented-but-removed
  flag both fail. It also asserts the two hidden flags are absent from that table, and that each documented
  value-enum flag's listed values (`auto|always|never`, `gameboy|ansi|mono`, `10|20|30`) match clap's
  `possible_values`.
- `tests/docs_controls.rs` — iterates a fixed alphabet of `KeyEvent`s (a–z, 0–9, the four arrows, Enter,
  Space, Esc, `?`, Ctrl+C) across every `Mode`, calls `input::map_key`, and asserts that every key which maps
  to `Some(Action)` in some mode is named in `docs/user/controls.md`, and that every key the doc names maps
  to something. This is the §5 table proved against the real key map rather than against a copied list.

`tests/no_stubs.rs` walks `src/` and `assets/` and fails on `todo!`, `unimplemented!`, `TODO`, `FIXME`, `XXX`,
`unreachable!("stub"`, or the word `placeholder` in `assets/world.ron`. It reads the directories rather than a
hardcoded file list, so a new file cannot escape it.

### 7. Content: making §2's five hearts reachable

`assets/world.ron` authors one `HeartContainer`, so 5 hearts (the §2 maximum, and the cap `state.rs` already
enforces) cannot be reached. One more `HeartContainer` is authored into a secret chest off the main route
(`chest.*_heart`, a new stable id — adding a chest id is a non-breaking content change per ADR 0006, and
`check_secrets` already forbids a secret holding a route-critical reward, which a heart is not). A new case
in `tests/content.rs` asserts the whole §2 table against the embedded world — rooms 9+6, 3 NPCs, 3 regular
enemy kinds, 1 boss, sword + lantern, both puzzle kinds, ≥3 secrets, ≥2 heart containers — so the table stops
being something a human has to re-count.

### 8. Error handling

Nothing in this phase is on a user-facing failure path. `balance.rs` is total: it counts what the world has
and returns a struct; there is no `Result` because there is no failure mode that is not a content-validation
failure already caught by `content::validate`. The doc tests read files with `fs::read_to_string` and are
allowed to panic — they are tests. The scripts use `set -euo pipefail` and exit non-zero if any case's
evidence grep fails, following `terminal-restore-check.sh`.

### 9. How this honours the architecture

`balance.rs` lives under `src/game/` and obeys its rule literally: pure, integer ticks, no `std::io`, no
`std::time`, no `ratatui`. It reads `World` and `tuning` and returns a plain struct; it is never called by
`update`, so it cannot affect simulation. `draw_due` moves an expression out of `main.rs` into `app.rs`,
which is where the loop's mode/pacing policy already lives — `main.rs` keeps sole ownership of the terminal.
The measurement harness lives in `tests/`, so no measurement code ships in the binary. Deviations from
`.autodev/ARCHITECTURE.md` (the new `game/balance.rs` module, estimator parameters outside `tuning.rs`,
`app::draw_due`, the `--debug` overlay named in ARCHITECTURE's observability paragraph staying unbuilt) are
appended to `.autodev/DECISIONS.md` under *PLAN phase 7*.

## Tasks

- [x] **T1: Extract the victory route into `tests/common/route.rs`.** Move `tests/playthrough.rs`'s walker
  (`cross`, `interact_facing`, `light`, `solve_block_puzzle`, `fight_boss`, the route body) into
  `pub fn play_to_victory(&mut Runner) -> RouteOutcome`; `tests/playthrough.rs` calls it and keeps every
  existing assertion. Files: `tests/common/mod.rs`, `tests/common/route.rs` (new), `tests/playthrough.rs`.
  Test: the existing playthrough cases must still pass unchanged.
- [x] **T2: Add `src/game/balance.rs`.** `RunEstimate`, `engage_ticks`, `boss_fight_ticks`,
  `estimate_first_playthrough`, `TARGET_{MIN,MAX}_TICKS`, and the documented estimator parameters. Wire into
  `src/game/mod.rs`. Files: `src/game/balance.rs` (new), `src/game/mod.rs`. Tests (in-file `#[cfg(test)]`,
  matching `state.rs`'s existing style): `boss_fight_ticks` equals the value recomputed from the `BOSS_*`
  constants; `engage_ticks` is strictly ordered bat < slime < guardian; every `RunEstimate` component is
  non-zero for the embedded world; `total_ticks` equals the sum of its parts.
- [x] **T3: Add `tests/balance.rs`.** Runs `play_to_victory`, feeds `runner.state.tick` into
  `estimate_first_playthrough(&world, ticks)`, asserts `TARGET_MIN_TICKS <= total <= TARGET_MAX_TICKS` with a
  failure message printing the full breakdown and the measured optimal tick count. Second case: the optimal
  route itself stays under 3 simulated minutes (a perfect run must stay a perfect run — catches a tuning
  change that inflates travel without anyone noticing). Third case: the estimate's combat term actually
  responds to content, asserted by comparing `engage_ticks` sums over the authored spawn list.
- [x] **T4: Balance pass.** Lands at 60088 ticks (~33 min), inside the band with real margin from both
  edges — travel dominates the estimate (~76%) so content-only levers (boss HP, enemy density) have limited
  further range without unreasonable in-game inflation; see DECISIONS.md phase-07/implement. With T3 red or near the band edge, tune toward ~37 minutes: enemy density and
  placement on the main route in `assets/world.ron`, and the `BOSS_*` / enemy constants in
  `src/game/tuning.rs` (boss HP and the phase-two vulnerable window are the two levers with the most effect
  per unit of risk). Keep every §6 floor: telegraph ≥ 18 ticks, invulnerability 24, sword 9/3. Re-run
  `tests/{boss,combat,ai,playthrough,dungeon}.rs` after each change — a balance change that makes the boss
  unbeatable shows up there first. Every constant moved gets a line in DECISIONS.md.
- [x] **T5: Author the second heart container + the §2 content table test.** `assets/world.ron` gains one
  `HeartContainer` secret chest; `tests/content.rs` gains `the_embedded_world_meets_the_spec_content_table`
  asserting every row of §2 against the loaded world.
- [x] **T6: Add `app::draw_due` and use it everywhere.** `src/app.rs` (function + doc comment), `src/main.rs`
  (replace the inline gate), `tests/loop_timing.rs` (replace both inline copies). No behaviour change; the
  existing loop-timing assertions are the test.
- [x] **T7: Add the byte-counting harness `tests/common/metrics.rs`.** `ByteCounter` (`io::Write`) and
  `Harness` over `Terminal<CrosstermBackend<ByteCounter>>` with a fixed viewport, iterating exactly as
  `main.rs` does. Files: `tests/common/{mod,metrics}.rs`.
- [x] **T8: Add `tests/metrics.rs`** with the four cases from Design §3, printing `bytes`, `bytes/min` and
  `KB/min` for both the active and the paused runs.
- [x] **T9: Add `config::cli_command()` and `tests/docs_cli.rs`** — the two-way flag diff plus the
  hidden-flags-absent and value-enum checks of Design §6.
- [x] **T10: Add `tests/docs_controls.rs`** — the key-map-vs-`controls.md` two-way coverage check.
- [x] **T11: Add `tests/no_stubs.rs`** — the `src/` + `assets/` walk of Design §6.
- [x] **T12: Write `docs/user/ssh.md`** — `ssh -t`, why a PTY is required, `--fps 10` on a slow link, what the
  output volume measurement actually was, running under `tmux`/`screen` (detach/reattach, `TERM` inside tmux,
  resize-on-reattach behaviour), the 256-colour caveat for the `gameboy` theme under tmux and the `--theme
  ansi` / `--theme mono` fallbacks, and an explicit "the ~150 ms RTT check was not run" line pointing at the
  verification report. Link it from `docs/user/cli.md`, `docs/user/controls.md` and README.
- [x] **T13: Update `README.md`** — replace "Known limitations (phase 6)" with the shipped state,
  point at `docs/user/ssh.md` and `docs/dev/verification-report.md`. The Build/Run blocks were
  re-verified during the review-r2 fix pass by executing exactly what they say on the host Mac
  (`cargo build --release --locked`, then the printed binary path with `--save-dir` on a scratch dir
  through `expect`): New Game reached, quit confirmed, exit 0, no orphan — see
  docs/dev/verification-report.md's manual-checks table.
- [x] **T14: Write `docs/dev/architecture.md`** — module map, the pure-core boundary, the loop and the draw
  gate, the tick model, the content pipeline, the save format, the terminal lifecycle, the run-length model
  of Design §1, and a table linking each ADR to the decision it records. Summary with pointers, not a copy of
  `.autodev/ARCHITECTURE.md`.
- [x] **T15: Write `scripts/measure-cpu.sh` and `scripts/manual-checks.sh`** — both run to
  completion in the review-r2 fix pass, in this same sandbox (no controlling TTY on the parent
  shell), exit 0, no process left behind. The round-1 claim that this sandbox cannot drive a pty
  session at all was wrong: the actual blockers were bugs in `manual-checks.sh` itself, not an
  environment limitation — see DECISIONS.md phase-07/review-r2 for the diagnosis and
  docs/dev/verification-report.md for the transcripts.
- [x] **T16: Write `docs/dev/verification-report.md`** — every §13 mandatory automatic check mapped to the
  test that covers it with a pass/fail status and the command that produced it; the four readiness commands
  with their real output; the byte and CPU measurements with the host named (`aarch64-apple-darwin`, macOS
  build, rustc 1.98.1, terminal emulator named); the manual list with each result **verbatim, including any
  failure**; and an explicit *Not verified* section for Linux (CI-only: build+test, no interactive run), a
  real PTY SSH session over a network, the ~150 ms RTT check, Intel macOS and aarch64 Linux.
- [x] **T17: Write `HANDOFF.md`** — state of the §2 content table row by row (with anything unmet named as
  remaining work), the unverified environments, the withdrawn `--unicode` flag, the estimator's assumptions
  as the weakest claim in the build, and what a next session should pick up first.
- [x] **T18: Update `CHANGELOG.md` and `docs/dev/testing.md`** — the new test files, what each proves, and how
  to run the two host scripts.
- [x] **T19: Full gate + release build.** fmt/clippy/test/build --release all pass on this host
  (aarch64-apple-darwin, rustc 1.98.1); 31 test binaries, 301 tests, 0 failures. CI status not
  observable from this session (no git remote configured here — see
  docs/dev/verification-report.md's CI status section). `cargo fmt --check`, `cargo clippy --all-targets --all-features --
  -D warnings`, `cargo test --locked`, `cargo build --release --locked`; record CI status for
  `ubuntu-latest` / `macos-latest` in the report from the actual run if one is observable, and say so plainly
  if it is not yet.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
# host-only, not part of any gate:
./scripts/terminal-restore-check.sh
./scripts/manual-checks.sh
./scripts/measure-cpu.sh
```

| # | Acceptance criterion | Proven by |
|---|---|---|
| 1 | Playthrough tick count maps to a 30–45 min run at the tuned intervals; a balance regression fails the suite | `tests/balance.rs::estimated_first_playthrough_lands_in_the_target_band` (+ `src/game/balance.rs` in-file tests tying `boss_fight_ticks`/`engage_ticks` to `tuning`) |
| 2 | Harness reports bytes/min playing and paused; a paused game with no input emits zero bytes after the first frame | `tests/metrics.rs::active_play_stays_inside_the_output_budget`, `::a_paused_game_with_no_input_writes_nothing_after_the_first_frame`, `::a_static_main_menu_writes_nothing_after_the_first_frame` |
| 3 | `docs/user/` documents every §5 binding and every §12 flag | `tests/docs_cli.rs` (two-way flag diff vs clap, value-enum values, hidden flags absent), `tests/docs_controls.rs` (two-way key-map diff vs `controls.md`) |
| 4 | README builds and runs the game from its own instructions on the host Mac | T13 executed on the host; transcript pasted into `docs/dev/verification-report.md` |
| 5 | Verification report lists every §13 check with status, and marks Linux, real PTY SSH, ~150 ms RTT, Intel macOS, aarch64 Linux unverified | `docs/dev/verification-report.md` (T16), cross-checked against the §13 list in `docs/spec.md` |
| 6 | Manual list executed on the host Mac, results recorded verbatim including failures | `scripts/manual-checks.sh` transcript in `docs/dev/verification-report.md` (T15/T16) |
| 7 | No stub, TODO or `unimplemented!` on the main route | `tests/no_stubs.rs` |
| 8 | fmt, clippy `-D warnings`, `test --locked`, `build --release --locked` pass; CI green on ubuntu + macos | T19, plus `.github/workflows/ci.yml` running the identical four commands on both runners |
| — | §2 content table met (five hearts reachable) | `tests/content.rs::the_embedded_world_meets_the_spec_content_table` |

## Risks

- **#2 (wrong game / unpaced 30–45 min)** — the reason this phase exists. The mitigation moves from "recorded
  as an open question" to a failing test, at the cost of depending on the estimator's parameters. That
  dependency is itself the phase's weakest claim and is named as such in the report and HANDOFF.
- **#9 (boss unbeatable or trivial)** — T4 touches the boss constants, which is exactly where this risk
  lives. Every boss change re-runs `tests/boss.rs` and `tests/playthrough.rs`, which beat the boss with
  ordinary actions and assert the vulnerability window is real; §6's telegraph floor is never crossed.
- **#12 (terminal output volume over SSH)** — measured for the first time here, against the real
  `CrosstermBackend` write path, with the paused case asserted at exactly zero bytes.
- **#15 (Unicode half-populated)** — already resolved by withdrawal in phase 6; the report and HANDOFF state
  it as a cut, not an omission.
- **#17 (overstated Linux/SSH/RTT claims)** — the report is written from ADR 0008's fixed honest-claims list,
  and `manual-checks.sh` prints `SKIPPED` lines for unreachable environments so the transcript cannot be read
  as a pass.
- **#3 (shortfall against §2)** — T5 closes the one real gap found (five hearts unreachable); anything left
  goes into HANDOFF.md by name.

## Out of scope

- Anything after phase 7 — there is none; the roadmap ends here.
- The `--debug` overlay and `--log-file` sink named in ARCHITECTURE's observability paragraph: never
  implemented in phases 1–6, not required by any §13 check or §12 flag, and adding a flag now would put an
  undocumented entry in front of the flag-diff test for no player benefit. Recorded in DECISIONS.md and
  HANDOFF.md as a deliberate non-deliverable.
- Reviving `--unicode` (withdrawn in phase 6, ADR-level decision).
- New game content beyond the second heart container and T4's enemy-placement tuning: no new rooms, NPCs,
  puzzles or secrets.
- A PTY test harness inside `cargo test` — CLAUDE.md forbids it; the pty work stays in host-run scripts.
- Packaging, release archives, `cargo install` automation, and any CI change beyond keeping the existing
  matrix green.

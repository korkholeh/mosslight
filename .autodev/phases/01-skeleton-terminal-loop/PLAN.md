# Phase 1 — Skeleton, terminal lifecycle and the playable room

Roadmap phase 1/7 · spec §3, §4, §5, §6 (movement only), §8, §9, §11, §12, §13, §14 step 1.

## Context

**What exists.** Nothing executable. The repository holds `docs/spec.md` (Ukrainian, source of truth),
`CLAUDE.md`, the eight ADRs under `docs/dev/adr/`, and the autodev documents
(`ARCHITECTURE.md`, `RISKS.md`, `ROADMAP.md`, `DECISIONS.md`, `PROFILE.md`). There is no `Cargo.toml`, no
`src/`, no `tests/`, no CI. Host toolchain verified: `rustc 1.98.1 (48a229cea 2026-09-01)`,
`cargo 1.98.1`, `stable-aarch64-apple-darwin`.

**What this phase changes.** It creates the whole package and makes it a runnable game: main menu → one
hard-coded 24×16 room → a hero that walks with arrows and WASD at a fixed 30 Hz → Esc pause → Q confirm
quit, with the terminal restored on every exit path and an input policy that cannot replay stale moves
over SSH.

**Key files created.**

```
Cargo.toml  Cargo.lock  rust-toolchain.toml  .gitignore
.github/workflows/ci.yml
src/main.rs  src/lib.rs  src/config.rs  src/terminal.rs  src/input.rs  src/app.rs
src/game/{mod,state,world,entities,tuning,rng}.rs
src/render/{mod,theme,tiles,hud,scene,overlays}.rs
tests/{deps,config,movement,input_policy,no_key_release,mode_machine,render,terminal_guard,loop_timing}.rs
scripts/terminal-restore-check.sh
README.md  docs/user/{controls,cli}.md  docs/dev/loop-and-modes.md
```

`src/game/{combat,ai,puzzles}.rs`, `src/content/`, `src/save.rs` and `assets/world.ron` are **not** created
here — they belong to phases 2, 3 and 6 and an empty module would be a stub on the main route (§14).

## Design

### Module layout and ownership

Exactly the layout in `ARCHITECTURE.md` / `PROFILE.md`. `src/main.rs` is the only file that owns a terminal;
everything else is reachable from `src/lib.rs` and testable without one.

### Data model (phase-1 subset)

```rust
// game::tuning — every §6 constant, in ticks, pub (pub items cannot trip dead_code)
pub const TICK_HZ: u64 = 30;
pub const HERO_STEP_TICKS: u64 = 4;        // 133 ms (§6's 120 ms on the tick grid, per DECISIONS)
pub const SWORD_COOLDOWN_TICKS: u64 = 9;   // 300 ms
pub const SWORD_ACTIVE_TICKS: u64 = 3;     // 100 ms
pub const INVULN_TICKS: u64 = 24;          // 800 ms
pub const TELEGRAPH_MIN_TICKS: u64 = 18;   // 600 ms
pub const MAX_CATCHUP_STEPS: u32 = 5;      // §9
pub const INPUT_EVENTS_PER_ITER: usize = 32;
pub const ROOM_W: usize = 24;
pub const ROOM_H: usize = 16;

// game::state
pub type Tick = u64;
pub struct GameState { pub tick: Tick, pub rng: Rng, pub hero: Hero, pub room: Room }
pub struct Hero { pub pos: Pos, pub facing: Facing, pub step_ready_at: Tick,
                  pub health_halves: u8, pub max_health_halves: u8, pub keys: u8 }
pub enum Facing { North, East, South, West }
pub struct Pos { pub x: u8, pub y: u8 }
// GameState derives Clone, PartialEq, Debug, Serialize — Serialize is what makes
// "byte-identical state" checkable before state_hash() arrives in phase 3.

// game::world (phase 1 only; phase 2 replaces this with content::World)
pub enum Tile { Floor, Wall, Water, Bush }   // Door/Stairs/Pit/Hidden arrive with the content schema
pub struct Room { pub tiles: [[Tile; ROOM_W]; ROOM_H], pub spawn: Pos }
pub fn debug_room() -> Room;   // wall ring, two interior wall stubs, a water pool, a bush patch

// game::rng — SplitMix64, seeded from Config::seed
pub struct Rng { state: u64 }
impl Rng { pub fn new(seed: u64) -> Self; pub fn next_u64(&mut self) -> u64; pub fn below(&mut self, n: u32) -> u32 }

// game — the only simulation entry point (ARCHITECTURE contract, unchanged)
pub fn update(state: &mut GameState, actions: &[Action], tick: Tick) -> Vec<GameEvent>;
pub enum Action { MoveNorth, MoveEast, MoveSouth, MoveWest, Attack, UseLantern, Interact,
                  Confirm, Cancel, ToggleMap, ToggleInventory, Help, Quit }
pub enum GameEvent { HeroMoved { from: Pos, to: Pos }, MoveBlocked { at: Pos, facing: Facing },
                     Message(String) }
```

`update` is total: it returns events, never `Result`, and touches no clock — `tick` is the only time input.
Phase-1 rules: an attempted move always sets `facing` (§5), a step is applied only when
`tick >= hero.step_ready_at`, and it is refused when the target tile is `Wall`/`Water`/`Bush` or outside
the 24×16 grid; a refused step still sets facing and emits `MoveBlocked`. `Attack`, `UseLantern`,
`Interact`, `ToggleMap`, `ToggleInventory` are accepted and ignored this phase (their screens are later
phases); they are not silently dropped in `input`, so their key bindings are already provable.

### Input policy (§5, RISKS #6)

```rust
pub fn map_key(mode: Mode, key: KeyEvent) -> Option<Action>;  // pure; press events only
pub fn coalesce(actions: &mut Vec<Action>);                   // keeps the LAST movement, order preserved
pub fn drop_pending(actions: &mut Vec<Action>);               // called when a menu/overlay closes
```

- `map_key` matches on `KeyCode` only. `KeyEventKind` is never named anywhere in `src/`, so there is no
  key-release path to remove later; `tests/no_key_release.rs` makes that a checked property.
- Ctrl+C arrives as a key event under raw mode and maps to `Action::Quit` (same confirmation as `Q`).
- Per iteration the loop reads at most `INPUT_EVENTS_PER_ITER` (32) events. **Overflow policy:** once the
  cap is hit, the remaining immediately-available events are drained and discarded, except non-movement
  actions (`Quit`, `Cancel`, `Confirm`) of which at most 8 are retained. This is what makes "no stale moves
  replay afterwards" structural rather than a tuning hope; a discarded-event count goes to the deferred
  diagnostics.
- `coalesce` then leaves at most one movement action per iteration, and `HERO_STEP_TICKS` bounds it again
  to at most one step per 4 ticks.
- Resize events are coalesced to the last size seen in the iteration.

### Mode machine and pause (`app.rs`)

```rust
pub enum Mode { MainMenu, Playing, Paused, ConfirmQuit, Help, TooSmall }
pub struct App { pub mode: Mode, prev_mode: Mode, pub state: GameState, pub menu: MenuCursor,
                 pub message: String, pub size: (u16, u16), dirty: bool, pub quit: Option<ExitReason> }
impl App {
    pub fn new(cfg: &Config) -> Self;                       // starts in MainMenu
    pub fn apply(&mut self, actions: &[Action]);            // mode transitions + menu selection
    pub fn tick(&mut self, actions: &[Action]) -> bool;     // runs game::update only in Mode::Playing
    pub fn on_resize(&mut self, w: u16, h: u16);
    pub fn simulating(&self) -> bool;                       // Mode::Playing only
    pub fn take_dirty(&mut self) -> bool;
}
```

- Menu: `Continue` (present but disabled in phase 1 — no save subsystem yet, hint "no save yet"),
  `New Game`, `Help`, `Quit`. `New Game` → `Playing` with the hero at `debug_room().spawn` facing South.
- `Esc` in `Playing` → `Paused`; `Esc` in `Paused` → `Playing` and `drop_pending` clears buffered actions.
- `Q` in `Playing`/`Paused`/`MainMenu` → `ConfirmQuit`; `Confirm` quits (exit 0), `Cancel` returns.
- `?` opens `Help` from any of `MainMenu`/`Playing`/`Paused`; `Esc`/`Cancel` returns to `prev_mode`.
- Below 60×24 (at startup or on resize) → `TooSmall`, which records `prev_mode` and stops simulation. On
  recovery: `Playing` → **`Paused`** (§9, so the hero cannot take an unseen hit); any other previous mode
  is restored as-is.
- `dirty` is set by every mode/menu/message change; the loop draws only when the fps interval elapsed
  **and** (`dirty` or a simulation step ran). No `Clear` of the full screen is ever issued — ratatui's
  buffer diff does the work (§9, RISKS #12).

### The loop and its testable core

The pacing arithmetic is a clock-free type so acceptance criterion 3 can be proven without a terminal:

```rust
// app.rs — no std::time, elapsed time arrives as integer nanoseconds
pub struct Pacer { tick_interval_ns: u64, frame_interval_ns: u64, sim_acc_ns: u64, frame_acc_ns: u64 }
pub struct Due { pub sim_steps: u32, pub draw: bool }
impl Pacer {
    pub fn new(fps: Fps) -> Self;                       // sim always 1/30 s; fps gates draw only
    pub fn advance(&mut self, elapsed_ns: u64) -> Due;  // caps at MAX_CATCHUP_STEPS, discards surplus
    pub fn next_deadline_ns(&self) -> u64;              // what main.rs passes to event::poll
}
```

`main.rs` converts `Instant` deltas to nanoseconds and does nothing else with time. The loop:

```
Config::from_args(argv, env)?          // may exit 2 before the terminal is touched
terminal::preflight(&probe)?           // TTY + TERM check, still before raw mode
let guard = TerminalGuard::enter()?;   // raw mode + alt screen + hide cursor + panic hook
let signals = terminal::SignalFlags::register()?;
loop {
    poll(pacer.next_deadline_ns())  -> read <= 32 events, drop the overflow, map, coalesce
    app.apply(&actions);
    let due = pacer.advance(elapsed_ns);
    for step in 0..due.sim_steps { app.tick(if step == 0 { &actions } else { &[] }); }
    if due.draw && (app.take_dirty() || due.sim_steps > 0) { terminal.draw(|f| render::draw(f, &app, &theme))?; }
    if signals.pending() || app.quit.is_some() { break; }
}
drop(guard);                           // restore, then
diagnostics.flush_to_stderr();         // print — never before restoration (§11)
```

Actions are applied to the first catch-up step only, so a stall cannot multiply one keypress into five
steps.

### Terminal lifecycle (`terminal.rs`, ADR 0007, RISKS #4)

```rust
pub trait TerminalOps {           // the seam that makes restoration testable without a PTY
    fn enable_raw(&mut self) -> io::Result<()>;
    fn disable_raw(&mut self) -> io::Result<()>;
    fn enter_alt(&mut self) -> io::Result<()>;
    fn leave_alt(&mut self) -> io::Result<()>;
    fn hide_cursor(&mut self) -> io::Result<()>;
    fn show_cursor(&mut self) -> io::Result<()>;
    fn reset_styles(&mut self) -> io::Result<()>;
}
pub struct TerminalGuard<O: TerminalOps> { ops: O, restored: bool }
impl<O: TerminalOps> TerminalGuard<O> {
    pub fn enter(ops: O) -> io::Result<Self>;   // enable_raw, enter_alt, hide_cursor, install hook
    pub fn restore(&mut self);                  // idempotent; Drop calls it
}
pub struct Probe { pub stdin_tty: bool, pub stdout_tty: bool, pub term: Option<String> }
pub fn preflight(p: &Probe) -> Result<(), StartupError>;
pub struct SignalFlags { term: Arc<AtomicBool>, hup: Arc<AtomicBool> }  // signal_hook::flag::register
pub fn size() -> io::Result<(u16, u16)>;
pub struct Diagnostics { lines: Vec<String> }   // buffered; flushed to stderr after the guard drops
```

The real implementation is `CrosstermOps` over `io::stdout()` using `ratatui::crossterm` only. Tests use
`RecordingOps`, which appends each call to a `Vec<&'static str>`, so "restores in the right order",
"idempotent", "never enabled raw mode" and "restore precedes the panic message" are all asserted on a
recorded sequence. The panic hook calls the same restore path, then the previous hook.

`--debug-panic` (hidden clap flag) panics deliberately once the guard is up, so §13's controlled-panic check
can actually be run instead of asserted.

### Config (`config.rs`, §12)

```rust
pub struct Config { pub glyphs: GlyphSet, pub color: ColorMode, pub theme: ThemeName,
                    pub fps: Fps, pub save_dir: PathBuf, pub seed: u64, pub debug_panic: bool }
pub enum GlyphSet { Ascii, Unicode }  pub enum ColorMode { Always, Never }
pub enum ThemeName { Gameboy, Ansi, Mono }  pub enum Fps { F10, F20, F30 }
pub fn resolve_color(flag: Option<ColorArg>, no_color: Option<&str>, stdout_tty: bool, term: Option<&str>) -> ColorMode;
```

Full §12 surface: `--ascii`, `--unicode` (mutually exclusive, default ASCII), `--color auto|always|never`,
`--theme gameboy|ansi|mono`, `--fps 10|20|30` (a value-enum, so `--fps 45` is a clap error, not a silent
clamp), `--save-dir PATH`, `--seed NUMBER`, `--help`, `--version`, plus hidden `--debug-panic`. Defaults:
ASCII, `gameboy`, 20 fps, per-OS save dir (§10 paths resolved with `std::env`), fixed default seed.
Precedence: an explicit `--color` always wins; otherwise a non-empty `NO_COLOR` forces `Never`; otherwise
`auto` = colour unless `TERM` is unset/`dumb`. `--unicode` is parsed and stored but renders the ASCII glyph
table this phase (Unicode is one atomic deliverable in phase 6). Themes are parsed; only `gameboy` and
`mono` palettes are wired in phase 1 and `ansi` maps to the gameboy palette until phase 6.

Errors: `StartupError { message: String, code: i32 }`; exit codes **0** success, **1** runtime/write error,
**2** usage or preflight refusal. The refusal is one line on stderr (§3) and happens before `TerminalGuard`
exists, so raw mode is provably untouched.

### Render (`render/`, §4, ADR layout budget)

Pure projection `pub fn draw(frame: &mut Frame, app: &App, theme: &Theme)`. Layout is fixed:

| Block row (relative) | Content |
|---|---|
| 0 | HUD: health hearts, active item, key count |
| 1..=18 | bordered scene box, 50×18 → interior 48×16 = 24 tiles × 2 cols |
| 19 | message line |
| 20 | hint line |

Block is 50×21, centred with floor division: `x0 = (w - 50) / 2`, `y0 = (h - 21) / 2`. At 60×24 → `(5, 1)`;
at 80×24 → `(15, 1)`. A tile `(tx, ty)` therefore lands at `col = x0 + 1 + 2*tx`, `row = y0 + 2 + ty`
(+1 for the scene border, +1 more for the HUD row). At 60×24 the hero spawning at tile (12, 8) is at
**col 30, row 11**, and the room's top-left wall tile is at **col 6, row 3**. Each tile writes its glyph in
the left column and a space in the right (padded to tile width, §4).

`tiles.rs` owns the single glyph table (`@ # . ~ " N s b g C + O _ t` per §4); `theme.rs` maps a
`TileKind`/`Role` to colours **only** — it cannot return a glyph, so a monochrome regression is
structurally impossible (RISKS #10). `overlays.rs` renders the main menu, pause, confirm-quit, help and the
too-small notice; the too-small notice prints required (`60x24`) and current size.

### Error handling summary

Fallible work lives at the edges and returns `Result` with a concrete error: `config` (`StartupError`),
`terminal` (`io::Error`). `game/` and `render/` are infallible. No `unwrap`/`expect` outside tests. A write
error from `terminal.draw` ends the loop, restores, and exits 1 (never retried — §11).

### Deviations from ARCHITECTURE.md

Three, all appended to `DECISIONS.md`:

1. **`Pacer` lives in `app.rs`** rather than in a new module or inside `main.rs` — keeping the loop's
   arithmetic in the lib is what makes the fps-independence criterion testable headlessly, and it adds no
   file outside the fixed layout.
2. **`Mode::Help`** is added to the phase-1 mode set (ARCHITECTURE's mode list omits it) because §5 binds
   `?` to Help and a documented key with no destination is a stub.
3. **`TerminalOps` trait seam** in `terminal.rs`: ADR 0007 says one type mutates terminal state; the trait
   keeps that true (one guard, one `Drop`) while allowing a recording fake in tests instead of a PTY
   harness, which the intake forbids.

Plus two phase-scoping choices: the room is hard-coded in `game/world.rs` until the phase-2 content
pipeline replaces it, and fps-independence is compared via `serde_json` bytes of `GameState` because
`state_hash()` is a phase-3 deliverable.

## Tasks

- [x] **T1 — Package skeleton.** `Cargo.toml` (lib + bin `mosslight`, deps exactly: ratatui 0.30.2, clap 4.6
      derive, serde 1 derive, ron 0.12, serde_json 1, signal-hook 0.4; **no crossterm**),
      `rust-toolchain.toml` (1.98.1 + rustfmt + clippy), `.gitignore` (`/target`), `src/lib.rs` module tree,
      `src/main.rs` stub returning `ExitCode::SUCCESS`. Run `cargo fetch --locked` (needs network once) and
      commit `Cargo.lock`. Verify `cargo build --locked`.
- [x] **T2 — Lock-file guard.** `tests/deps.rs`: exactly one `name = "crossterm"` entry in `Cargo.lock`, and
      `Cargo.toml` never names crossterm. (RISKS #5)
- [x] **T3 — CI.** `.github/workflows/ci.yml`: matrix `ubuntu-latest` + `macos-latest`, checkout, `rustup show`
      (honours `rust-toolchain.toml`), then `cargo fmt --check`, `cargo clippy --all-targets --all-features
      -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`.
- [x] **T4 — Config.** `src/config.rs`: clap derive parser for the full §12 surface + hidden `--debug-panic`,
      `Config`, `resolve_color`, save-dir resolution, default seed. `tests/config.rs`: defaults; every flag
      parsed; `--fps 45` rejected; `--ascii --unicode` rejected; colour precedence matrix (explicit flag vs
      `NO_COLOR` vs `TERM=dumb` vs auto).
- [x] **T5 — Startup refusal.** `terminal::preflight(&Probe)` + `StartupError` + wiring in `main.rs` (exit 2,
      one line to stderr). Tests: unit table over `Probe` combinations (no stdin TTY, no stdout TTY,
      `TERM=dumb`, `TERM` unset, healthy), plus an integration test running `env!("CARGO_BIN_EXE_mosslight")`
      with piped stdin and asserting exit code 2 and exactly one stderr line.
- [x] **T6 — Simulation core.** `src/game/{mod,tuning,rng,state,entities,world}.rs`: constants, SplitMix64,
      `GameState`/`Hero`/`Pos`/`Facing`, `debug_room()`, `update` with movement, step cooldown,
      facing-on-blocked-move and tile collision. `tests/movement.rs`: a step in each of the four directions;
      blocked by wall/water/bush/out-of-bounds changes facing but not position and emits `MoveBlocked`;
      cooldown allows exactly one step per `HERO_STEP_TICKS`; `update` with no actions is a no-op except the
      tick; the same seed + action sequence yields equal `GameState`.
- [x] **T7 — Input policy.** `src/input.rs`: `map_key`, `coalesce`, `drop_pending`, overflow drain.
      `tests/input_policy.rs`: arrows and WASD both map to the four moves; `J`/`Space`, `K`, `E`/`Enter`,
      `M`, `I`, `Esc`, `?`, `Q`, `Ctrl+C` map per §5; per-mode maps differ (menu `Confirm` vs play
      `Interact`); `coalesce` keeps the last movement and preserves other actions; **200 queued movement
      events in one iteration yield one action and leave nothing queued**; `drop_pending` clears buffered
      game actions on overlay close.
- [x] **T8 — No key-release, by test.** `tests/no_key_release.rs` walks `src/**/*.rs` and asserts no
      occurrence of `KeyEventKind`, `Release`, `KeyboardEnhancementFlags`, `PushKeyboardEnhancementFlags`.
- [x] **T9 — Mode machine and pacer.** `src/app.rs`: `Mode`, `App`, transitions, pause semantics, dirty flag,
      `Pacer`/`Due`. `tests/mode_machine.rs`: menu → New Game → `Playing` with the hero at spawn; `Esc`
      pauses and `app.simulating()` is false; `Esc` resumes and pending actions were dropped; `Q` →
      `ConfirmQuit`, cancel returns, confirm sets `quit`; resize to 59×24 → `TooSmall` and the tick count
      stops advancing; resize back to 80×24 → `Paused`, not `Playing`; `?` → `Help` → back to the previous
      mode.
- [x] **T10 — Glyphs and theme.** `src/render/{tiles,theme}.rs`: the §4 glyph table (single source), `Theme`
      returning colours only for gameboy/mono (ansi aliases gameboy this phase). Test inside the module:
      every theme returns the identical glyph for every tile kind.
- [x] **T11 — Layout and draw.** `src/render/{scene,hud,overlays,mod}.rs`, `render::draw`. `tests/render.rs`
      with `TestBackend`: at 60×24 the hero is at (col 30, row 11), the room's corner walls are at the four
      computed corners, the HUD row starts at (5, 1) with `HP`, the message row is row 20 and the hint row
      row 21 and contains `Esc` and `Q`; at 80×24 the same scene is centred at `x0 = 15`; at 59×23 the
      `TooSmall` notice appears and contains both `60x24` and the current size; the main-menu overlay lists
      Continue / New Game / Help / Quit.
- [x] **T12 — Terminal guard.** `src/terminal.rs`: `TerminalOps`, `CrosstermOps`, `TerminalGuard` with
      idempotent `restore`/`Drop`, panic hook, `SignalFlags` via `signal_hook::flag::register`, `size()`,
      `Diagnostics`. `tests/terminal_guard.rs` with `RecordingOps`: `enter` records enable-raw → enter-alt →
      hide-cursor; drop records show-cursor → reset-styles → leave-alt → disable-raw; an explicit `restore`
      followed by drop records the restore sequence **once**; a preflight refusal records nothing at all; the
      panic path records the full restore sequence **before** the diagnostic line is emitted.
- [x] **T13 — The loop.** `src/main.rs`: argv → `Config` → preflight → guard → signals → loop exactly as in
      Design, `--debug-panic` handling, deferred diagnostics flushed to stderr after the guard drops, exit
      codes 0/1/2, write error → clean fatal.
- [x] **T14 — Loop timing tests.** `tests/loop_timing.rs`: the same action schedule driven through
      `Pacer` + `App` at `--fps` 10, 20 and 30 produces **byte-identical** `serde_json::to_vec(&app.state)`;
      a 900 ms stall yields at most 5 simulation steps and the surplus is discarded (the next iteration is
      not accelerated); no draw is due while paused with nothing dirty over 100 iterations; draws per second
      never exceed the configured fps.
- [x] **T15 — Restoration check script.** `scripts/terminal-restore-check.sh` drives the release binary
      through a real PTY via `expect` (plain `script -q /dev/null` cannot itself send keystrokes) for the
      normal-exit (`Q`, Enter), `--debug-panic`, and (since review round 1) a live resize down to 100x20
      followed by `Q` with no confirm dialog, printing `stty -a`'s `lflags` line before/after each.
      **Actually run** on this host (Darwin arm64, this sandbox has no controlling TTY at all but
      `expect`/`script` still allocate a real pty pair) on 2026-09-13; the captured transcript is committed
      at `TERMINAL_RESTORE.txt` alongside this plan. All three paths exit as expected (0, 101, 0), `lflags`
      is byte-identical before/after every case, and for `--debug-panic` the restore escape sequences
      (`\x1b[?25h\x1b[0m\x1b[?1049l`) appear before the panic message in the transcript. Two gaps remain
      unexercised by this script and are stated explicitly in its own output rather than being silently
      skipped: a terminal already too small at *startup* (expect's default pty is 80x24, so this script
      cannot spawn one pre-sized below that; covered instead by
      `tests/render.rs::draw_never_panics_on_a_too_small_frame_even_without_a_prior_resize`), and a genuine
      post-disconnect write-error exit path (phase 1 has no fallible save subsystem to trigger it against;
      revisit once phase 6 adds one).
- [x] **T16 — Docs.** `README.md` (build, run, controls, SSH note, `reset` recovery), `docs/user/controls.md`
      (every binding per mode), `docs/user/cli.md` (the §12 surface, NO_COLOR precedence, save-dir),
      `docs/dev/loop-and-modes.md` (loop iteration, mode diagram, layout budget).
- [x] **T17 — Gate.** `cargo fmt`, then the full gate plus the release build; fix everything it reports
      without weakening a check.

## Verification

```sh
cargo fetch --locked
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
# manual, on a real terminal:
cargo run --release -- --save-dir /tmp/mosslight-scratch
bash scripts/terminal-restore-check.sh
```

| # | Acceptance criterion | Proven by |
|---|---|---|
| 1 | Full gate + release build succeed from a clean checkout | T17; CI (`.github/workflows/ci.yml`) on ubuntu-latest and macos-latest |
| 2 | Main menu → New Game → walk with arrows/WASD → Esc pauses → Q confirms and exits | `tests/mode_machine.rs` (transitions) + `tests/input_policy.rs` (bindings) + `tests/movement.rs` (walking); the interactive run is the manual check in T15's log |
| 3 | Same action sequence at fps 10/20/30 gives byte-identical `GameState` | `tests/loop_timing.rs::state_is_identical_at_every_fps` |
| 4 | `Cargo.lock` has exactly one crossterm entry | `tests/deps.rs::single_crossterm_in_lockfile` |
| 5 | Non-TTY stdin or `TERM=dumb` exits non-zero with one line and never enables raw mode | `tests/config.rs` / `tests/terminal_guard.rs::preflight_refusal_records_no_terminal_calls` + the spawned-binary test in T5 |
| 6 | Terminal restored after normal exit, error exit and `--debug-panic`; panic message printed after restoration | `tests/terminal_guard.rs` (recorded order, idempotence, restore-before-print) for the automated part; `scripts/terminal-restore-check.sh` + `stty -a` for the real-PTY part, reported honestly (§13) |
| 7 | 60×24 places hero, walls, HUD and hint row at fixed positions; 80×24 centres the scene | `tests/render.rs::scene_placement_60x24`, `::scene_centred_80x24` |
| 8 | Below 60×24 → `TooSmall` with required and current size; recovery resumes into `Paused` | `tests/render.rs::too_small_notice` + `tests/mode_machine.rs::resize_recovery_enters_paused` |
| 9 | 200 queued movement events → at most one step per tick, no stale replay | `tests/input_policy.rs::burst_of_200_moves_yields_one_action` + `tests/movement.rs::one_step_per_cooldown` |
| 10 | No key-release or extended keyboard protocol anywhere in `src/` | `tests/no_key_release.rs` |

Manual checks belonging to this phase (§13), to be run on the host Mac and recorded as run-or-not:
local run at 60×24 and 80×24, live resize, monochrome (`--theme mono`), held-key without move accumulation,
terminal state after exit and after a controlled panic. Linux, a real SSH PTY and the ~150 ms RTT check are
**not** reachable here and stay unverified (ADR 0008).

## Risks

| Row | Risk | What this plan does |
|---|---|---|
| #4 | Terminal left broken on some exit path | One `TerminalGuard`, idempotent `restore`, panic hook that restores before printing, signal flags polled in the loop, preflight before raw mode, hidden `--debug-panic`; the recorded-ops tests make the order checkable without a PTY (T12, T15) |
| #5 | ratatui/crossterm version skew | crossterm never declared; `ratatui::crossterm` only; `tests/deps.rs` fails the build if a second entry appears (T1, T2) |
| #6 | Input feels wrong over SSH | Press-only mapping with no `KeyEventKind` in the tree (enforced by T8), ≤32 events per iteration with the overflow discarded, movement coalesced, step cooldown, pending drop on overlay close, ≤5 catch-up steps with actions applied to the first step only (T7, T9, T13, T14) |
| #8 | Render rate leaks into simulation speed | Integer `Tick`, `Pacer` gating draws only, and the byte-identical-state test across fps values (T9, T14) |
| #11 | Layout breaks at 60×24 or on resize | Fixed 50×21 budget with computed offsets, `TooSmall` with pause, recovery into `Paused`, resize coalescing; asserted at 60×24, 80×24 and 59×23 (T9, T11) |
| #12 | Too much terminal output over SSH | Dirty flag + fps gate + no full-screen clear; the "no draw while paused" assertion in T14 keeps it honest |
| #13 | Clippy debt discovered late | The full gate is the phase exit condition (T17) and runs in CI from this phase onward (T3) |
| #16 | Dependency creep | The manifest is exactly the six crates in PROFILE.md; anything else needs a DECISIONS line |
| #17 | Overstated environment claims | T15 records what was actually executed; Linux, SSH PTY and RTT stay unverified |
| **new** | `cargo fetch` needs network once to resolve ratatui 0.30.2 (not in the local registry cache; the sandbox used for planning had no network) | T1 runs `cargo fetch --locked` first; if crates.io is unreachable the phase cannot produce a lock file and that is the one genuine blocker — report it rather than substituting versions |

## Out of scope

Phase 2: the RON content schema, `include_str!` loader, the validator and `assets/world.ron`; room
transitions and doors. Phase 3: combat, enemies, AI, death/retry, `state_hash()`. Phase 4: dialogue, NPCs,
chests, items, map and inventory screens, secrets. Phase 5: dungeon, keys, puzzles, boss, ending, the
headless playthrough. Phase 6: the save subsystem and `Continue`, palettes/themes beyond the phase-1 pair,
Unicode glyphs. Phase 7: balance, CPU and output-volume measurement, the verification report.
Also deferred: `--debug` overlay and `--log-file` (not part of §12), autosave triggers in `app.rs`, and any
PTY-based test harness (forbidden by the intake).

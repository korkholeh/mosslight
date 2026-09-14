# Decisions & assumptions (autonomous run)

Appended by agents whenever they choose between options without a human.


## ARCHITECT (2026-09-13)

- [architect/design] Depend on `ratatui = "0.30.2"` only and use the `ratatui::crossterm` re-export; `crossterm` is never declared in `Cargo.toml` — why: spec §8 names ratatui/crossterm version skew as a concrete hazard, and two version requirements in one manifest is the mechanism that causes it — alternatives: direct `crossterm = "0.29"` alongside ratatui, pinning both by hand, ratatui 0.29.
- [architect/design] Verified compatible pair recorded per the intake: ratatui 0.30.2 (2026-06-19) → ratatui-crossterm 0.1.x → crossterm 0.29.0; ratatui enables the crossterm backend by default and re-exports it. Checked against crates.io on 2026-09-13 — why: the intake enabled web access specifically to pin this pair — alternatives: choosing offline from memory.
- [architect/design] Simulation time is an integer `Tick` (u64) at 30 Hz, not a `Duration` — why: §13 requires identical simulation for an identical seed and action sequence, and float accumulation is not reproducible across platforms — alternatives: `Duration` as in the §8 sketch, float `dt`.
- [architect/design] Hero step interval is 4 ticks = 133 ms rather than the 120 ms in §6 — why: 120 ms is not a multiple of the 33.3 ms tick and §6 labels these as starting values to tune; rounding once in `tuning` beats rounding at each use site — alternatives: a 40 Hz or 50 Hz tick to hit 120 ms exactly (contradicts §9's fixed 30 Hz), per-use rounding.
- [architect/design] Randomness is a hand-written SplitMix64 in `game::rng` seeded from `--seed`, with a fixed default seed — why: `rand` gives no cross-version guarantee that a seed yields a given sequence, so a dependency bump could silently break the §13 determinism test — alternatives: `rand` + `StdRng`, `fastrand`, no randomness at all.
- [architect/design] Content in RON (`assets/world.ron`, embedded via `include_str!`), save file in JSON — why: RON carries Rust enums and comments for a hand-authored world, while JSON lets `serde_json::Value` probe `format_version` alone before parsing a body possibly written by a newer build — alternatives: RON for both, JSON for both, a binary format such as bincode/postcard.
- [architect/design] Content ids are stable namespaced strings (`room.lighthouse`, `chest.forest_sword`) stored as strings in the save, interned to indices only in memory — why: reordering or adding content must not corrupt an existing save, and a save a human can read is the only debugging surface this game has — alternatives: numeric ids, index-based references.
- [architect/design] `content::validate` runs in tests, in CI, **and** at startup, and collects all errors rather than failing on the first — why: §15 makes "no dead ends through keys or puzzles" an acceptance criterion, and unfinishable content is the most likely way this run ships a failing product — alternatives: validation only in tests, only in a `build.rs`.
- [architect/design] Reachability is checked by a BFS over (room, acquired-item-set) states proving the ember is obtainable and the lighthouse reachable afterwards; the validator is itself tested against deliberately broken fixture worlds — why: a validator that guards the acceptance criterion must itself be guarded — alternatives: trusting the headless playthrough alone.
- [architect/design] Single thread, `crossterm::event::poll` with a deadline; no input thread — why: poll already gives non-blocking input, and a second thread adds a synchronization boundary to the one subsystem that must be provably deterministic — alternatives: the conventional ratatui input-thread-plus-channel pattern, an async runtime (forbidden by §8).
- [architect/design] Input policy: ≤ 32 events read per iteration, movement coalesced to ≤ 1 step per tick, pending game actions dropped when a menu or dialogue closes — why: §5 requires that a stall or a batched input flush not replay dozens of stale moves — alternatives: an unbounded queue, a time-based input debounce.
- [architect/design] Plain structs plus per-kind `AiState` enums in `Vec<Enemy>`; no ECS — why: §8 says structs and enums suffice at this scope, and an ECS would make the §10 save boundary a query and the §13 determinism guarantee dependent on library iteration order — alternatives: `bevy_ecs`, `hecs`, `Box<dyn Enemy>` trait objects.
- [architect/design] Contested-tile resolution uses stable `Vec<Enemy>` iteration order as the tie-break — why: §6 requires simultaneous tile claims to resolve deterministically — alternatives: random tie-break, position-based ordering.
- [architect/design] Save durability: serialize → `save.json.tmp` → `sync_all` → copy current to `save.json.bak` → atomic rename; `LoadOutcome` enum with `Ok | Missing | Corrupt | FutureVersion`, no `unwrap` on the path — why: §10 requires exactly this and forbids panicking on or silently overwriting a corrupt or newer file — alternatives: in-place write, a checksum/HMAC, multiple rolling slots.
- [architect/design] No checksum on the save file — why: the atomic rename already prevents torn writes, tampering is not a threat in an offline single-player game, and a parse failure is a sufficient corruption signal — alternatives: CRC32, HMAC.
- [architect/design] Save paths resolved with ~20 lines of `std::env` rather than the `directories`/`dirs` crate — why: `ProjectDirs` yields `~/Library/Application Support/<qualifier>.<org>.<app>` on macOS, not the literal `~/Library/Application Support/mosslight` §10 requires — alternatives: `directories`, `dirs`, `etcetera`.
- [architect/design] `signal-hook 0.4` is the only dependency added beyond the four §8 names; SIGTERM/SIGHUP register as `AtomicBool` flags polled by the main loop — why: §11 requires these signals to reach the loop through a safe mechanism with no file I/O in the handler, and crossterm does not deliver them — alternatives: the `ctrlc` crate (SIGINT only, and raw mode already turns Ctrl+C into a key event), raw `libc::signal`, a `signal_hook::iterator::Signals` thread.
- [architect/design] One `TerminalGuard` with an idempotent `Drop`, plus a panic hook that restores before printing, plus a pre-raw-mode TTY/`TERM` check — why: §11 requires restoration on four exit paths and §15 makes it an acceptance criterion; one owner is the only version of this that is provable by inspection — alternatives: scattered enable/disable calls, `catch_unwind` around the loop.
- [architect/design] A hidden `--debug-panic` flag will exist so §13's "correct terminal after a controlled panic" check can actually be executed — why: §13 forbids claiming a scenario was verified when it was not run — alternatives: asserting the behaviour from code inspection.
- [architect/design] A write error to stdout is a clean fatal: leave the loop, attempt restoration, exit non-zero; never retried — why: §11 requires handling post-SSH-drop write errors, and a dropped connection will not recover — alternatives: retry with backoff, ignore write errors.
- [architect/design] Diagnostics are buffered in memory and flushed to stderr after the guard drops; `--log-file PATH` is off by default — why: §11 forbids log output landing on the game screen — alternatives: a log file always on, `tracing` with a terminal-aware layer.
- [architect/design] Recovering from a too-small terminal resumes into **Paused**, never directly into play — why: §9 requires the hero not take unexpected damage after a resize — alternatives: resuming directly, a countdown.
- [architect/design] Layout budget fixed at 50×21 inside the 60×24 minimum: 1 HUD row, a bordered 48×16 scene (50×18), 1 message row, 1 hint row — why: §4 sets the room at 24×16 tiles of 2 columns each and a 60×24 minimum, so the fit needed to be computed once rather than discovered during rendering — alternatives: an unbordered scene, a two-row HUD.
- [architect/design] The theme layer may change colour only; glyphs are identical in every theme — why: §4 forbids colour being the only distinction and requires monochrome to stay fully legible; making glyphs theme-independent turns that into a structural property instead of a review item — alternatives: per-theme glyph tables, colour-only differentiation for some objects.
- [architect/design] Assumed non-functional numbers where the spec is silent: ≤ 12 live enemies per room, < 200 ms startup, < 200 KB/min terminal output during play and ~0 while paused, < 5 % of one core, < 32 MB RSS, < 12 MB release binary, < 64 KB save, macOS 13+ / glibc 2.31+ floor — why: §13 asks for measured CPU and output volume, which needs a target to measure against — alternatives: leaving them unspecified until measurement.
- [architect/design] No release archives, Homebrew formula, `.deb`, or musl static build in v1; distribution is `cargo install --path .` or copying `target/release/mosslight` — why: §3's launch examples only require a copyable binary, and packaging is additive later — alternatives: per-triple release archives, musl static Linux builds, `cross`/QEMU CI for aarch64 Linux and Intel macOS.
- [architect/design] CI matrix is `ubuntu-latest` + `macos-latest` only; Intel macOS and aarch64 Linux are labelled expected-but-unverified — why: the code is integer-only with no architecture-specific constructs, and `cross`/QEMU would roughly double CI cost for no plausible divergence — alternatives: a four-target matrix with emulation.
- [architect/design] Save-format upgrades are a forward migration chain keyed on `format_version`; unknown ids in a save are dropped with a note and missing ids default to "not done", so adding content is non-breaking while renaming or deleting an id requires a version bump — why: §10 requires a format version and the upgrade path had to be stated before content authoring starts — alternatives: refusing any older version, best-effort lenient parsing with no version gate.
- [orchestrator/architect] lint_command set to `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings` by this session (was `(none)`)

## ROADMAP (2026-09-13)

- [roadmap/plan] Seven phases instead of spec §14's six; the content pipeline and validator are pulled out of §14 stage 3 into phase 2, before any world is authored — why: RISKS #1 (content authored but unfinishable) is the highest-cost risk and the validator is its only real mitigation, so it must exist before 15 rooms do — alternatives: following §14's six stages literally, folding the validator into the dungeon phase, validating only in a final QA phase.
- [roadmap/plan] `test_command` is `cargo test --locked`, not the combined fmt+clippy+test gate — why: the orchestrator tracks the lint command separately, and the field is defined as the unit/integration suite runner; the combined gate is recorded in ROADMAP.md and CLAUDE.md as the per-phase exit condition — alternatives: returning the full gate as the test command and leaving lint_command unused.
- [roadmap/plan] The headless start-to-victory playthrough lands in phase 5 (dungeon/boss/ending), not in the final polish phase — why: it is the proof that the game is completable, and RISKS #3 wants that proof to exist well before the run's time ceiling, not at the end — alternatives: writing it in the documentation/balance phase alongside the verification report.
- [roadmap/plan] Death and retry in phase 3 restore from an in-memory checkpoint taken on room entry; phase 6 repoints the restore at the save file — why: §14 puts the save system at stage 5 but §13 requires death-and-restore to be tested as soon as combat exists, and an in-memory checkpoint is the smallest thing that makes the phase-3 game a complete loop — alternatives: pulling the whole save subsystem forward into phase 3, deferring the death test until phase 6.
- [roadmap/plan] The full §12 flag surface is parsed in phase 1; only the flags' *presentation* semantics (themes, palettes, Unicode) are deferred to phase 6 — why: `Config` is immutable and read by every module, so its shape should not churn, and §3's pre-raw-mode TTY/`TERM=dumb` refusal is already phase-1 work — alternatives: a minimal phase-1 CLI grown flag by flag, all CLI work in phase 6.
- [roadmap/plan] Phase 2 authors the 9 overworld rooms as geometry and doors only — no NPCs, items or enemies — why: it gives the validator and the room-transition tests real content to run against while keeping the pacing-sensitive content decisions (RISKS #2) in phase 4 where they belong — alternatives: a throwaway hardcoded room in phase 1, authoring full overworld content in phase 2.
- [roadmap/assumption] `--fps` is assumed to accept only the three documented values {10, 20, 30} (§12 lists exactly these), rejecting others with a clap error — why: §12 writes the flag as an enumeration rather than a range, and a rejected value is more honest than a silently clamped one — alternatives: accepting any integer and clamping, accepting any integer unclamped.
- [roadmap/assumption] "At least 3 secrets" (§2) is read as 3 hidden rewards in the overworld reachable with the lantern, none of them on the main route — why: §7 requires the main route to have no dead ends, so a secret gating progress would contradict it — alternatives: dungeon secrets, secrets that shortcut the main route.
- [roadmap/assumption] The two required puzzle kinds (§6) are instantiated in the dungeon (phase 5); phase 4's overworld carries the one "simple puzzle" that yields the lantern per §7 step 4 — why: §7's main route names exactly one pre-lantern puzzle and §2 places the puzzle content in the dungeon — alternatives: spreading both puzzle kinds across the overworld, all puzzles in the dungeon including the lantern gate.
- [roadmap/assumption] Maximum health is reached through 2 heart containers (3 → 5 hearts, §2), placed one as a secret and one as a dungeon reward — why: §2 fixes the start and cap but not how the gap is closed — alternatives: a single +2 container, containers sold or given by NPCs (shops are out of scope).

## PHASE 1 — PLAN (2026-09-13)

- [phase-1/plan] The loop's pacing arithmetic lives in `app.rs` as a clock-free `Pacer` that takes elapsed nanoseconds and returns `Due { sim_steps, draw }` — why: the fps-independence acceptance criterion has to be provable without a terminal, and putting the arithmetic in the lib does that without adding a module outside ARCHITECTURE's fixed file list — alternatives: a new `src/loop_timing.rs`, keeping the arithmetic in `main.rs` (untestable), passing `Instant`/`Duration` into the lib.
- [phase-1/plan] `Mode::Help` is part of the phase-1 mode set even though ARCHITECTURE's mode list omits it — why: §5 binds `?` to Help, and shipping a documented key with no destination is a stub on a user-facing surface — alternatives: deferring Help to a later phase, dropping the `?` binding until then.
- [phase-1/plan] Terminal side effects go through a `TerminalOps` trait (`CrosstermOps` in production, `RecordingOps` in tests) while `TerminalGuard` stays the single owner with one idempotent `Drop` — why: ADR 0007's guarantee has to be asserted on a recorded call sequence, and the intake forbids a PTY harness — alternatives: asserting restoration by code inspection only, an `expectrl`/`portable-pty` harness, testing against the real terminal in CI.
- [phase-1/plan] The phase-1 room is hard-coded in `game/world.rs` as `debug_room()` (24×16, wall ring, water and bush patches) and is replaced by the phase-2 content pipeline — why: the content schema and validator are phase 2 by roadmap decision, and a playable room is a phase-1 deliverable — alternatives: pulling the RON pipeline forward into phase 1, shipping phase 1 without a walkable room.
- [phase-1/plan] fps-independence is asserted by comparing `serde_json::to_vec(&GameState)` bytes — why: `state_hash()` is a phase-3 deliverable and serde is already a fixed dependency, so this gives a literal byte comparison now — alternatives: deriving `PartialEq` only, pulling `state_hash()` forward into phase 1.
- [phase-1/plan] Input overflow policy: past the 32-event per-iteration cap the remaining immediately-available events are drained and discarded, keeping at most 8 non-movement actions (`Quit`, `Cancel`, `Confirm`) — why: leaving the surplus queued would let a 200-event burst replay as steps over the following iterations, which is exactly what §5 forbids; discarding everything could swallow a quit request — alternatives: leaving the surplus queued for later iterations, discarding the surplus wholesale, an unbounded queue with time-based debouncing.
- [phase-1/plan] Exit codes: 0 success, 1 runtime/write error, 2 usage or startup refusal (non-TTY, `TERM=dumb`, clap error) — why: §3 requires a non-zero exit with a short explanation and clap already uses 2 for usage errors — alternatives: 1 for everything, distinct codes per refusal reason.
- [phase-1/plan] `--debug` overlay and `--log-file PATH` (named in ARCHITECTURE but not in §12) are deferred out of phase 1 — why: phase 1 owns the §12 surface plus the hidden `--debug-panic`, and diagnostics already have a deferred stderr buffer — alternatives: implementing both now, dropping them from the design entirely.
- [phase-1/plan] `--unicode` is parsed and stored but renders the ASCII glyph table until phase 6, and `--theme ansi` aliases the gameboy palette in phase 1 — why: Unicode is one atomic deliverable (RISKS #15) and a half-populated tile set is worse than none; the flag surface must still not churn — alternatives: rejecting `--unicode` with an error in phase 1, authoring Unicode glyphs now.
- [phase-1/plan] Layout is fixed at a 50×21 block centred by floor division (`x0 = (w-50)/2`, `y0 = (h-21)/2`): HUD row, 50×18 bordered scene (interior 48×16), message row, hint row; a tile's glyph occupies the left column of its two and is padded with a space on the right — why: the render tests assert absolute cell positions, so the arithmetic must be stated once rather than discovered per call site — alternatives: top-aligned layout, centring with rounding up, glyph centred or right-aligned within the tile.
- [phase-1/plan] All §6 combat constants are defined in `game/tuning.rs` now as `pub` items even though only movement uses them this phase — why: §6 requires the values kept in one place, and `pub` items in the lib cannot trip `dead_code` under `clippy -D warnings` — alternatives: adding each constant in the phase that first uses it, `#[allow(dead_code)]`.

## PHASE 1 — IMPLEMENT (2026-09-13)

- [phase-1/implement] `App::apply(&mut self, actions: &[Action]) -> Vec<Action>` returns the sim-bound
  actions instead of the void signature sketched in PLAN.md, and `App` has no persistent `pending` buffer —
  mode transitions and menu navigation are resolved action-by-action inside `apply`, and any action is
  routed into the returned vector only while already `Playing` at that point in the batch. The moment an
  action closes an overlay back into `Playing`, everything collected so far is dropped and the rest of the
  batch is ignored — why: a separate `pending: Vec<Action>` plus a `drop_pending` call on resume left a gap
  where a movement key queued in the *same* input batch right after the closing key would still reach
  `game::update` on the very same tick, which is exactly the stale-replay behaviour §5 forbids; folding the
  drop into `apply` itself makes it structural instead of an easy-to-forget call site — alternatives: the
  literal `pending` buffer from PLAN.md with an explicit `drop_pending` call in `apply_paused`'s `Cancel`
  arm (misses the same-batch case), clearing `pending` only in `main.rs` after `on_resize`.
- [phase-1/implement] `TerminalGuard::enter` no longer installs the panic hook itself; `install_panic_hook(f)`
  is a separate function `main.rs` calls right after `enter`, taking a plain restore closure — why: the
  original sketch needed a process-global `Mutex<Option<Box<dyn Fn()>>>` slot so the hook could reach a
  guard living on `main`'s stack, which added a global-mutable-state seam with no test value; a bare closure
  lets `tests/terminal_guard.rs` install a `RecordingOps`-backed hook and assert the call order via
  `catch_unwind` without any global slot — alternatives: the `OnceLock<Mutex<Option<Box<dyn Fn()>>>>` slot,
  making `TerminalGuard` itself `!Send` and hook-aware.
- [phase-1/implement] Default `--seed` is the fixed literal `0x4D6F73736C696768` — why: PLAN.md specifies
  only "a fixed default seed" without a value; this one is arbitrary (an ASCII-derived constant) but fixed,
  which is all §13's determinism test requires — alternatives: `0`, `1`, a value derived from a build-time
  constant.
- [phase-1/implement] Most of PLAN.md's named test files (`tests/movement.rs`, `tests/input_policy.rs`,
  `tests/mode_machine.rs`, `tests/terminal_guard.rs`, `tests/config.rs`, `tests/loop_timing.rs`) hold their
  tests exclusively — nothing is duplicated back into the corresponding `src/*.rs` unit-test module — why:
  `tests/no_key_release.rs` scans all of `src/**/*.rs` for the literal token `KeyEventKind`, and constructing
  a `crossterm::event::KeyEvent` fixture (required field, even for a synthetic "key pressed" event) requires
  naming that type; keeping those fixtures in `tests/` is what lets the scan stay a blunt whole-file grep
  instead of gaining exceptions — alternatives: keeping unit tests in `src/` and carving an exception into
  the forbidden-token scan for test-only code, constructing `KeyEvent` via a helper that hides the literal
  token (still fails a textual scan unless the scan gets smarter).

## PHASE 1 — REVIEW FIXES, round 1 (2026-09-13)

- [phase-1/review-fix] `main.rs::run` now probes `terminal::size()` once before entering the loop and feeds it
  through `App::on_resize`, and `render::draw` additionally guards on `render::is_too_small(frame.area())`
  regardless of `app.mode` — why: crossterm emits no `Resize` event at startup, so `Mode::TooSmall` was
  unreachable until the first resize and a terminal already below 60×24 at launch panicked inside ratatui's
  buffer indexing (confirmed in a real pty: `index outside of buffer ... index is (25, 20)`, exit 101); the
  render-side guard is belt-and-braces so a stale `app.size`/frame-area mismatch can never reach that panic
  again — alternatives: only the startup probe (leaves the render path trusting `app.mode`), only the render
  guard (still misses the "resumes into the wrong mode" half of the criterion).
- [phase-1/review-fix] `Mode::TooSmall` now handles `Action::Quit` by setting `self.quit` directly instead of
  routing through `ConfirmQuit` — why: a confirm-quit dialog cannot render at a too-small size, and spec §11
  requires Ctrl+C (which maps to `Action::Quit`) to be a graceful-shutdown request in every mode; before this
  fix a player who shrank the terminal had no keyboard way out at all — alternatives: transitioning to
  `ConfirmQuit` and drawing a one-line hint in the too-small notice, requiring a resize back up before quitting
  is possible.
- [phase-1/review-fix] `Config::from_args` now classifies `clap::error::ErrorKind::DisplayHelp` /
  `DisplayVersion` / `DisplayHelpOnMissingArgumentOrSubcommand` as exit code 0 on stdout (new
  `config::OutputStream` enum on `StartupError`), keeping exit code 2 on stderr for genuine usage errors —
  why: clap itself exits 0 on stdout for `--help`/`--version`, and the previous blanket mapping to code 2 on
  stderr broke `mosslight --help | less` and contradicted `docs/user/cli.md`'s own (now corrected) claim —
  alternatives: leaving `StartupError` as a single message+code pair and special-casing the print call site
  by re-parsing `e.kind()` in `main.rs` (duplicates the classification logic across two files).
- [phase-1/review-fix] Raw terminal events are now drained to exhaustion every iteration via a new
  `input::{EventSource, drain_ready}` seam (`main.rs`'s `CrosstermEvents` wraps zero-timeout
  `crossterm::event::poll`/`read`; tests use an in-memory `VecDeque`), replacing the old `poll_events` that
  stopped at 256 raw events and left the remainder queued for later iterations — why: PLAN.md and this file
  both specify draining and discarding the surplus, and leaving events queued let a >256-event burst replay as
  further simulation steps over subsequent iterations, exactly what §5 forbids; the movement/control overflow
  policy (`apply_overflow_policy`) already runs correctly on however many raw events arrive, so draining fully
  is safe. `drain_ready`'s `hard_cap` (`game::tuning::INPUT_EVENT_HARD_CAP = 4096`) is a livelock guard only,
  not a second overflow policy; events discarded past it are counted into `Diagnostics` — alternatives: raising
  the old break threshold (still leaves an unbounded burst queued), an unbounded drain with no livelock guard
  at all.
- [phase-1/review-fix] `TerminalGuard` now holds `Arc<AtomicBool>` instead of a private `bool` for `restored`,
  exposes it via `restored_flag()`, and `install_panic_hook` takes that same flag so whichever of "the guard's
  `Drop`" or "the panic hook" runs first is the only one that emits the restore sequence — why: the previous
  panic hook used a fresh `CrosstermOps` independent of the live guard's own `restored` bool, so unwinding
  after a panic ran the full restore sequence a second time, after the panic message had already printed
  (confirmed in a real pty transcript) — alternatives: a process-global `OnceLock<AtomicBool>` instead of a
  guard-owned `Arc` (adds global mutable state for one call site), leaving the double restore as harmless
  (it is invisible in practice, but it is still stray output after the diagnostic, which is exactly the
  ordering §11 cares about).
- [phase-1/review-fix] Added `tests/loop_timing.rs::draw_cadence_scales_with_fps_while_simulation_tick_count_does_not`,
  which drives `Due.draw` (previously untouched by any test) across fps 10/20/30 over a fixed number of tick
  iterations and asserts draw counts rise with fps while the simulation tick count does not — why: the
  reviewed fps-independence test only proved `Pacer` ignores `frame_interval_ns` for its own tick math, never
  exercising `due.draw` at all. A literal "same wall-clock second chopped into N frames" version was
  considered and rejected: `frame_interval_ns` for 30fps (`1e9/30` truncated to 33,333,333ns) does not evenly
  divide the 100ms/500ms alignment marks that make 10fps/20fps exact, so action-injection points would land up
  to one tick later at 30fps than at 10/20fps, shifting `Hero.step_ready_at` (part of the serialized state) by
  a tick and breaking byte-identical comparison for reasons unrelated to any real bug — alternatives: the
  rejected literal-chopping design, leaving the gap unaddressed since it was rated minor.
- [phase-1/review-fix] `scripts/terminal-restore-check.sh` gained a third case: resize the pty to 100×20 after
  launch, then `Q` (which now quits immediately per the `TooSmall`/`Quit` fix above, with no confirm dialog to
  wait for). Its captured transcript is committed at
  `.autodev/phases/01-skeleton-terminal-loop/TERMINAL_RESTORE.txt` — why: T15 previously claimed specific
  results ("Actually run... exits 0 and 101... `lflags` byte-identical") with no transcript in the diff for a
  future reader to check, which CLAUDE.md explicitly forbids; a terminal already too small *at startup* (before
  any resize) is still not exercised here because expect's default pty is 80×24 and cannot be spawned smaller
  by this script, so that gap is stated in the script's own output rather than silently skipped — alternatives:
  attempting to force expect's initial pty size below 80×24 (no straightforward portable mechanism), leaving
  T15 as a code-inspection claim.
- [phase-1/review-fix] `tests/terminal_guard.rs::preflight_refusal_records_no_terminal_calls` was deleted rather
  than rewritten — why: its only assertion (`err.code == 2`) already exists in `preflight_table`, and the
  actual property it claimed to pin down ("no `TerminalOps` impl is ever constructed on this path in
  `main.rs`") was stated in a comment with nothing in the test observing `main.rs`; the real version of that
  property is now the stdout-emptiness assertions added to `tests/config.rs`'s spawned-binary tests (see
  below), which do observe the real binary — alternatives: keeping the comment-only test alongside the new
  spawned assertions (redundant and still not a check on anything).
- [phase-1/review-fix] `tests/config.rs::non_tty_stdin_exits_2_with_one_stderr_line` now also asserts
  `output.stdout.is_empty()`, and a new sibling `term_dumb_exits_2_with_one_stderr_line_and_no_stdout` spawns
  the binary with `TERM=dumb` — why: an empty stdout is a real, cheap proof that raw mode/the alternate screen
  were never entered (either would write an escape sequence such as `\x1b[?1049h`), and the `TERM=dumb` half of
  the startup-refusal criterion previously had no end-to-end test at all — alternatives: asserting the absence
  of the specific escape sequence by substring match instead of emptiness (weaker: raw mode alone, without
  entering the alt screen, would not contain that exact substring but would still be a violation).

## PHASE 1 — REVIEW FIXES, round 2 (2026-09-13)

- [phase-1/review-fix-r2] `main.rs::run` now keeps a `pending: Vec<Action>` across loop iterations instead of
  recomputing `app.apply(&actions)` as a local dropped whenever `due.sim_steps == 0`; the iteration body that
  merges/coalesces/consumes it is extracted into a new `app::advance_iteration` free function so the real
  loop's timing-dependent wiring — not just `App`/`Pacer` in isolation — is directly testable — why: round-2
  review found the hero effectively unable to walk: a keypress arriving mid-tick (the normal case, since the
  poll deadline is tick-based) landed in an iteration with `sim_steps == 0` and was thrown away outright,
  verified in a real pty (4 isolated key presses moved the hero 0 times). `tests/loop_timing.rs` gained
  `a_keypress_delivered_when_no_sim_step_is_due_is_not_lost` and
  `a_held_movement_key_advances_the_hero_at_the_step_cooldown_rate_across_iterations`, both driving
  `advance_iteration` directly — alternatives: buffering inside `App` itself (rejected in round 1 for a
  different reason — see PHASE 1 — IMPLEMENT — that still applies: `App::apply`'s per-action same-batch drop
  needs mode transitions resolved every iteration regardless of pacing, so the pacing-only buffer has to be a
  layer above `App`, not inside it), retrying the poll until a tick boundary is crossed (adds input latency and
  contradicts the "sleep between iterations" comment on `CrosstermEvents`).
- [phase-1/review-fix-r2] `App::tick` now returns/uses `!events.is_empty()` to gate `dirty` instead of setting
  it unconditionally whenever `Mode::Playing`, and `main.rs`'s draw condition dropped the separate `any_step`
  OR-bypass (`due.draw && app.take_dirty()` instead of `due.draw && (app.take_dirty() || any_step)`) — why:
  CLAUDE.md states "do not draw when nothing changed," and the unconditional `dirty = true` plus the
  `any_step` bypass together meant a frame was written every fps interval forever while `Playing`, confirmed in
  the pty transcript (~20 bytes/frame at idle, forever); `any_step` was redundant with the fixed `dirty` once
  `dirty` correctly tracks "did this tick change anything" — every tick that changes something already ORs
  that into `dirty`, across all catch-up steps in the iteration — alternatives: keeping `any_step` as a
  belt-and-braces OR (defeats the fix, since it becomes true on any ticked step regardless of change), gating
  on `GameEvent` variant instead of emptiness (finer-grained than this phase needs; §9's tick-vs-render split
  only cares whether *anything* changed).
- [phase-1/review-fix-r2] `input::apply_overflow_policy` now keeps the **tail** (last `INPUT_EVENTS_PER_ITER`
  actions) instead of the **head** (first), rescuing up to `MAX_RETAINED_CONTROL_ACTIONS` control actions from
  the discarded, older surplus ahead of that tail — why: keeping the head meant a burst larger than 32 events
  applied the 32nd-oldest keypress as the surviving movement after `coalesce`, discarding every newer key
  including the direction actually pressed last — the exact stale-replay behaviour §5 forbids, just moved one
  layer up from where round 1 fixed it. `tests/input_policy.rs` gained
  `burst_ending_in_a_direction_change_yields_the_newest_movement` (200 North + 1 South → South survives) and
  replaced the old `overflow_retains_up_to_eight_control_actions_beyond_the_cap` scenario (20 Quit ahead of a
  40-Move burst, since a Quit *after* the burst now survives via the tail alone and no longer exercises the
  rescue path) — alternatives: coalescing before capping (equivalent for movement freshness, but collapses
  repeated non-movement actions no differently and was more invasive to the existing head/surplus structure for
  no behavioural gain).
- [phase-1/review-fix-r2] `terminal::preflight` now checks `TERM` before the stdin/stdout TTY check — why: a
  spawned test process can never be given a TTY (no PTY harness — see round 1's `preflight_table` alternative),
  so `tests/config.rs::term_dumb_exits_2_with_one_stderr_line_and_no_stdout` could only ever exercise the same
  branch as the plain non-TTY test regardless of `TERM`, making it pass unconditionally even if the `dumb`
  branch were deleted; reordering plus asserting `stderr.contains("dumb")` makes it a real, distinguishing
  end-to-end check. The unit-level `preflight_table` already proves the `dumb` branch triggers `Err` on its
  own, independent of ordering — alternatives: dropping the spawned `dumb` case entirely and relying on the
  unit level only (loses end-to-end coverage of the message actually reaching stderr through `main.rs`'s
  wiring); this reorder does mean a terminal that is simultaneously non-TTY and `TERM=dumb` now reports the
  `TERM` message first in real usage instead of the TTY message — judged an acceptable, rare edge case, and
  arguably no less informative to the user.
- [phase-1/review-fix-r2] `TerminalGuard::enter` now constructs the guard immediately after `enable_raw`
  succeeds and runs `enter_alt`/`hide_cursor` through it, calling `guard.restore()` before propagating an `Err`
  — why: previously `ops.enable_raw()?; ops.enter_alt()?; ops.hide_cursor()?;` meant a failure in either of the
  last two calls returned `Err` before any `TerminalGuard` existed, so nothing ran `disable_raw` and the
  caller's shell was left in raw mode with no way to recover short of `reset` — exactly the failure RISKS #4 is
  about, on a path the existing `RecordingOps` (which never fails) could not reach. Added
  `tests/terminal_guard.rs::a_failure_partway_through_enter_still_disables_raw_mode` with a new
  `FailingEnterAltOps` fake that fails `enter_alt` and asserts the full restore sequence, including
  `disable_raw`, still ran — alternatives: wrapping the whole body in a closure and calling a free `restore`
  function on the raw `ops` before returning (equivalent in effect, more indirection for no benefit since
  `TerminalGuard` already owns exactly this restore sequence).
- [phase-1/review-fix-r2] `scripts/terminal-restore-check.sh` now sets an explicit pty size (`stty rows 24
  columns 80`) inside the spawned bash command before the binary starts, targets the resize case's `stty` at
  the spawned pty's slave device (`< $spawn_out(slave,name)`) instead of `expect`'s own stdin, raises Expect's
  2000-byte `match_max` (smaller than one rendered frame here), actively drains output between keystrokes
  instead of a bare Tcl `sleep` (which reads nothing), and asserts the captured transcript contains content
  that could only have been drawn by the scenario each case claims (`HP`, `Quit?`, `panicked`, `60x24`, and
  each case's exit code) — why: round-2 review reproduced the round-1 script running in a 0×0 pty in this
  sandbox (`expect` allocates an unsized pty when it has no controlling terminal of its own), so every case
  silently rendered nothing and the committed transcript's scenario labels ("normal exit", "resize to 100×20")
  described paths that never actually ran; asserting on drawn content turns a misconfigured pty into a failing
  check instead of a silently-passing one — alternatives: only fixing the pty size without a content assertion
  (would have hidden the exact same failure mode from a future reviewer/regression), driving `stty` against
  `expect`'s own controlling terminal (the round-1 bug for the resize case specifically).
- [phase-1/review-fix-r2] The committed `TERMINAL_RESTORE.txt` was replaced with an honest status note instead
  of a fresh clean transcript, and no claim of a successful end-to-end run is made — why: even after every fix
  above, repeated runs of the script in this sandbox hit a further, distinct timing race: a keystroke sent
  shortly after `spawn` can land while the spawned shell is still in canonical/echo mode, before mosslight has
  enabled raw mode, so it is consumed as ordinary shell input (visible in the transcript as the literal
  character echoed in cleartext ahead of any rendered output) instead of reaching the program, and the case
  then times out. A minimal `python3 pty.fork()` harness outside `expect` entirely reproduced the identical
  failure mode when it wrote a keystroke without first draining the child's output backlog, and — this is the
  part worth recording — exited correctly and immediately, every time across repeated runs, once output was
  actively drained before and while sending keys; that is independent evidence mosslight's own `Mode::TooSmall`
  Quit handling and the normal confirm-quit path are correct, and that the unresolved part is this sandbox's
  `expect`/pty scheduling, not application behaviour — consistent with the already-logged "no controlling TTY"
  limitation (ADR 0008, RISKS #17) — alternatives: committing a transcript from the `pty.fork()` harness instead
  (that harness does not exercise `TerminalGuard`/raw-mode/alt-screen the way the real binary under `expect`
  does, and the intake explicitly forbids building a project-owned PTY harness as a test dependency), continuing
  to retry `expect` until it happens to succeed and committing that transcript uncommented (would misrepresent
  a lucky run as a reliable one).


## PLAN phase 02 — Content pipeline, validator and room transitions (2026-09-14)

- [plan/02] Room tiles are authored in `assets/world.ron` as 16 strings of 24 characters with a single char→`Tile` table, not as the `tiles: [[Tile; 24]; 16]` enum literal ARCHITECTURE.md sketches — why: a char grid is readable and diffable as a map, and §7's "legal tiles" check becomes literally "every char is in the table" — alternatives: nested enum lists per ARCHITECTURE.md, run-length encoding, one file per room.
- [plan/02] `Tile::Door` carries no id; the door id comes from the room's `doors` list matched by position, and the validator enforces the 1:1 correspondence between `Door` tiles and door entries — why: a character cell cannot carry a payload, and the position match is a stronger invariant than an inline id — alternatives: `Tile::Door(DoorId)` per ARCHITECTURE.md (impossible with a char grid), doors implied by edge position only.
- [plan/02] `GameState` holds `Rc<World>` (`#[serde(skip)]`) plus a `RoomIdx`, instead of `update` taking `&World` as a second parameter — why: it preserves the `update(&mut GameState, &[Action], tick)` contract pinned in CLAUDE.md/ARCHITECTURE.md and keeps `render::draw(frame, app, theme)` and every phase-1 test signature unchanged — alternatives: `update(&mut GameState, &World, ...)`, a `GameState<'w>` borrowing the world (self-reference through `App`), `Arc` (no threads exist, ADR 0001).
- [plan/02] Key/lock reachability is a memoised BFS over `(unlocked-door bitmask, item bitmask)` states that branches over which frontier door to open — why: small keys are fungible and consumable, so a greedy "open the first unlockable door" walk can report a false dead end; at 15 rooms and a handful of locks the branching search is still sub-millisecond — alternatives: greedy fixpoint, treating each key as a distinct id, no lock check at all.
- [plan/02] The ember/home route rules are gated on an authored `route: (ember_required: bool, home: RoomId)` block, set to `false` in phase 2 and flipped to `true` in phase 5 — why: the geometry-only overworld has no ember yet, but the rules must exist and be tested now (RISKS #1); fixtures that do author an ember chest exercise them — alternatives: writing the ember rules in phase 5 (the risk this phase exists to remove), always requiring an ember (phase 2 content would never validate).
- [plan/02] The validator does not enforce the §2 content table (9 overworld + 6 dungeon rooms, 3 NPCs, 3 enemy kinds, 1 boss) — why: it must accept the small two-room fixture worlds the validator's own tests are built from; the content-table assertion is a phase-5 test over the real world, as the roadmap already schedules — alternatives: enforcing the table in the validator (fixtures would need 15 rooms each), a separate "release" validation mode.
- [plan/02] The corrupted-world startup check is exercised through a hidden `--debug-content PATH` flag (mirroring `--debug-panic`), and the content preflight runs in `main` *before* `terminal::preflight` — why: the acceptance criterion asks for a test harness entry point, and running the content check first means a spawned test process (which has no TTY) still gets the content error rather than the TTY refusal, which is what proves the abort happened before raw mode — alternatives: a separate test binary embedding a broken world, a `cfg(test)`-only hook (would not exercise the real `main`), an environment variable.
- [plan/02] The start room is marked visited at `GameState::new` and emits no `RoomEntered`; the event means "a transition happened" — why: it is the phase-6 autosave trigger ("after a safe room transition"), and emitting it for the start room would fire an autosave at New Game — alternatives: emitting `RoomEntered` for the start room, leaving the start room unvisited until the player leaves and returns.

## PHASE 2 — IMPLEMENT (2026-09-14)

- [phase-2/implement] `Room`'s bush glyph `"` collides with RON's own string delimiter, so every authored row containing a bush tile must escape it as `\"` inside the `rows` string — the world generator script does this automatically (`str.replace('"', '\\"')`) — why: `ron::from_str` parses `"#...""...#"` as the string terminating at the first inner `"`, which is a silent-looking parse error rather than a content bug; alternatives: picking a different bush character (rejected — the char table is fixed by PLAN.md/spec §7 and is part of the reviewed design), documenting it as a known authoring footgun only (still needed, added to `docs/dev/content.md`).
- [phase-2/implement] The reachability BFS's `Items` state tracks only `lantern`/`ember` as booleans; `LockKind::Flag` is hardcoded `never satisfiable` rather than modeled with its own bit — why: no `Reward` variant can set a story flag yet (that mechanism arrives with puzzles in phase 4/5), so a `Flag`-locked door is unconditionally `LockNeverUnlockable` today, which is the *correct* answer for content that doesn't exist yet, not a stub; `assets/world.ron` authors no locks at all this phase, so nothing exercises the false branch — alternatives: adding a placeholder bit that's never set (equivalent behavior, more state-space for no test benefit), deferring the `Flag` variant's `Display`/validator wiring entirely (rejected — PLAN.md's `ContentError` table and §7 checks list require the case to exist now).
- [phase-2/implement] `GameState` implements `PartialEq` by hand (comparing `tick`/`rng`/`hero`/`room`/`progress`, skipping `world`) instead of deriving it — why: deriving would require `World` (and every nested content type) to implement `PartialEq` purely to satisfy the derive on a field that's always the same `Rc` in every test that compares two states, which is dead weight; alternatives: deriving `PartialEq` on `World`/`Room`/etc. anyway (works, but is validator-book-keeping weight with no test that needs it), wrapping `world` in a newtype that's always `Eq` (more indirection for the same outcome).
- [phase-2/implement] `assets/world.ron`'s nine rooms share one fixed decoration layout (a 2x2 water patch at rows 3-4/cols 3-4, a 2x2 bush patch at rows 11-12/cols 18-19, one wall stub at (6,6)) rather than bespoke per-room geometry — why: phase 2's scope is geometry + doors only (no NPCs/items/enemies), and uniform decoration placed away from every door's mid-edge line (col 12 / row 8) is guaranteed not to block any spawn-to-door path regardless of which edges a given room's doors sit on; bespoke per-room art is a phase-4/5 content-authoring concern, not a pipeline concern — alternatives: no interior decoration at all (weaker "collision has something to hit" per PLAN.md), hand-tuned unique layouts per room (correct but pure content-authoring time this phase doesn't need to spend).
- [phase-2/implement] `tests/fixtures/base.ron` is a 3-room world (not 2) with one `SmallKey`-locked door and one `Ember` chest, so every `broken_*.ron` fixture can be produced by changing exactly one field of `base.ron` and still exercise the lock/ember rules named in the acceptance criteria — why: a 2-room base can't host both an ungated area (for the valid key-before-lock/ember-reachable case) and a locked area, so `broken_key_behind_lock.ron`/`broken_ember_unreachable.ron` would otherwise need their own bespoke base, breaking "fixtures differ from valid content by exactly the defect" — alternatives: a per-fixture bespoke world (rejected by T8's acceptance criterion), a 2-room base with the lock/ember rules only covered by the in-module `validate.rs` tests (weaker end-to-end coverage of the fixture pipeline itself).
- [phase-2/implement] Test fixtures that don't care about door reciprocity set `two_way: false` explicitly (`content::loader` tests, the `key_lock_world` helper in `validate.rs`) rather than authoring a fully reciprocal pair — why: `content::parse` always runs full validation (stage 3 is not optional), so a decode-stage-only test fixture would otherwise fail on an unrelated reciprocity error; `two_way: false` is the schema's own documented way to opt a door out of that check, not a workaround — alternatives: authoring full reciprocal pairs even for tests that don't need them (more fixture code, and it obscures which property each test is actually pinning).

## PHASE 2 — REVIEW FIXES, round 1 (2026-09-14)

- [phase-2/review-fix-r1] The intra-room reachability fix (review major #1) is a full `(room, tile-component)` state in the `(unlocked, items)` BFS, not the simpler "static per-room walkable-set from every door's `to_spawn`" the review's fix text sketched — why: that simpler version is unsound for any two-way door pair, because the reciprocal door's own `to_spawn` (e.g. `spawn.b.fromC`, the landing point of `door.c.fromB`) is only ever reachable *after* crossing the door being tested (`door.b.toC`) in the first place; unioning it in as a static "entry point" makes the check pass on exactly the walled-in-spawn fixture it exists to catch (found by writing `tests/fixtures/broken_walled_in_spawn.ron` from the review's own repro and watching the naive version accept it). The component-per-state BFS derives reachability from the actual search history instead, so it cannot be fooled by a door's own reciprocal spawn — alternatives: the review's static per-room heuristic (verified unsound above), gating only doors and not chests the same way (would leave a walled-in ember chest with the identical blind spot the review is about, since nothing distinguishes a door tile from a chest tile at the tile-connectivity level).
- [phase-2/review-fix-r1] `ContentError::DoorUnreachableInRoom` is emitted from inside `check_reachability_and_route` (after the full BFS), not as an independent structural check that runs unconditionally like `check_door_geometry`/`check_reciprocity` — why: whether a door's tile is in a *reached* component depends on which locks are open in some reachable state, which only the full search knows; a room is skipped for this check (falling through to plain `RoomUnreachable` instead) when no component of it is reached at all, to avoid firing one `DoorUnreachableInRoom` per door on top of the room-level error — alternatives: a separate pre-BFS static check (the unsound version described above), reporting `DoorUnreachableInRoom` for every door in a wholly-unreachable room too (redundant with `RoomUnreachable`, noisier output for the same defect).
- [phase-2/review-fix-r1] `Progress`/`GameState` had their `#[serde(Serialize)]` derives removed outright (review minor: dense-`RoomIdx` serialization contradicts the ids-not-indices save-format rationale) rather than given a `serialize_with` that maps `RoomIdx` back to a room id string — why: the only caller was `tests/loop_timing.rs::run_schedule`, comparing two runs' final states for equality, which `GameState`'s existing hand-written `PartialEq` already does directly; a `serialize_with` would need `World` access to resolve ids that a field-level serializer doesn't have, forcing a hand-written `Serialize` impl on `GameState` for a property (`Vec<u8>` equality) that `PartialEq` already provides for free. Phase 6 defines the real on-disk shape from scratch rather than inheriting this phase's incidental one — alternatives: `serialize_with` mapping through a thread-local/context `World` (indirection with no test that needs it), leaving the misleading derive in place (rejected — it's exactly what the review flagged).
- [phase-2/review-fix-r1] `App::new` now takes `world: Rc<World>` as a second parameter instead of loading content itself (review minor: the old `App::new` re-parsed `content::EMBEDDED` and `.expect()`-ed inside the terminal guard) — `main` parses once in `content_preflight`, which now returns `Result<World, i32>`, and threads that same `World` through `run`/`App::new`, so `--debug-content PATH` plays the exact file that was validated instead of always falling back to the embedded world during gameplay — alternatives: keeping `App::new(cfg)` as a convenience wrapper that still calls `content::load().expect(...)` internally for tests only (rejected — leaves a real, if now-test-only, unwrap-family call on the content path for no benefit, and two divergent constructors is worse than updating ~13 test call sites).

## PLAN phase 03 — Sword combat, three enemy kinds, death and determinism (2026-09-14)

- [plan/03] `EnemyKind::{Bandit, Wisp}` is renamed to `{Bat, Guardian}` — why: spec §6 names Слиз/Кажан/Вартовий and ARCHITECTURE.md already writes `EnemySpawn { kind: Slime|Bat|Guardian|Boss }`; the phase-2 placeholder names match neither, nothing constructs them yet, and `ai.rs` is about to be written against these names — alternatives: keeping the placeholder names and mapping them in `ai.rs` (two vocabularies for one concept), adding the spec names as extra variants (four kinds, two of them dead).
- [plan/03] `Boss` is *not* added to `EnemyKind` this phase — why: it would be an `ai.rs` match arm with nothing behind it, and phase 5 owns the boss; the compiler will list every site to update when it arrives, which is the property ADR 0004 chose the enum for — alternatives: adding it now with a `todo!()` arm (forbidden: a stub on the main route), adding it with a no-op arm (silently shippable wrong behaviour).
- [plan/03] `AiState` is one flat enum (`SlimeIdle | SlimeChase | BatDart | BatRest | GuardianPatrol | GuardianTelegraph | GuardianDash | GuardianRecover`) rather than four nested per-kind enums — why: §6 asks for readable state machines and ADR 0004's stated reason for an enum is "every transition of one enemy visible on one screen"; a flat enum makes the whole transition table one `match` and keeps `Enemy` free of a kind-indexed union — alternatives: `AiState::Slime(SlimeState)` per ARCHITECTURE.md's sketch (an extra indirection at every match site), one struct per kind behind a trait object (explicitly rejected by ADR 0004).
- [plan/03] Enemies are room-local: `GameState::enemies` is rebuilt from the room's authored spawns on every room entry, so leaving and re-entering respawns them and `EnemyId(i)` is always the index into the `Vec` — why: §10 already lists transient combat state as deliberately not persisted, and phase 6's "on load, enemies respawn" inherits it for free; index-as-id is what makes the contested-tile tie-break "stable `Vec` iteration order" (ADR 0004) mean something concrete — alternatives: world-wide persistent enemy instances (a save-format surface for state §10 says not to save), keeping dead enemies dead per room in `Progress` (ditto, plus it lets a player farm a room empty and trivialise the pacing).
- [plan/03] Contact damage fires when a live enemy is orthogonally adjacent to (or on) the hero's tile at the end of a tick's movement resolution, taking the *first* such enemy in `Vec` order as the knockback source — why: enemies never enter the hero's tile, so a same-tile-only rule would make contact damage unreachable; adjacency is the standard top-down contact hitbox, and "first in `Vec` order" keeps the knockback direction deterministic when the hero is pinned between two enemies — alternatives: damage only when an enemy's move is blocked *by* the hero (fiddly, and silent when the hero walks into a stationary enemy), a sub-tile hitbox (no sub-tile positions exist: §6 fixes integer tile coordinates).
- [plan/03] Knockback stops before out-of-bounds, non-walkable, hazard, occupied **and `Tile::Door`** tiles — why: a knockback landing on a door tile would teleport the hero into the next room mid-hit, an un-cancellable transition the player never asked for; every other stop condition is §6's "knockback does not pass through obstacles" — alternatives: allowing the door transition (surprising, and it would fire an autosave mid-combat in phase 6), treating doors as walls for the hero's own movement too (breaks phase 2's transitions).
- [plan/03] A Guardian takes sword damage only while in `GuardianRecover`; hits in any other state are deflected and still cost the cooldown — why: it is the readable, testable form of §6's "vulnerability window after the attack" and is the rehearsal for the boss's same requirement (RISKS #9); guardians are never on the critical path, so armour cannot soft-lock a route — alternatives: double damage during Recover and normal damage otherwise (the window stops being a window and the acceptance criterion degenerates into a damage-number check), no armour at all (nothing would test the vulnerability window before phase 5).
- [plan/03] Each enemy machine alternates on a timer even with a motionless, out-of-range hero: Slime Chase has a duration and falls back to Idle, Bat alternates Dart/Rest by construction, and Guardian leaves Patrol unconditionally after `GUARDIAN_PATROL_TICKS` — why: the acceptance criterion "never stops changing AI state over 1000 ticks" is otherwise unsatisfiable for an enemy that has nothing to react to, and a permanently-idle enemy is exactly the "room looks broken" failure of RISKS #14 — alternatives: relaxing the test to allow a stable state when the hero is out of range (removes the only automated guard against a wedged enemy), making every enemy permanently aggressive (no pacing ramp, and §6 describes patrol/rest behaviour explicitly).
- [plan/03] `state_hash()` is a hand-rolled FNV-1a 64 over an explicit field order, and hashes `world` only through `world.version` — why: `std::collections::hash_map::DefaultHasher` is documented as unstable across Rust releases (and `RandomState` is randomly seeded), a hash crate would breach the fixed dependency set (RISKS #16), and re-hashing 15 immutable tile grids every tick would make the determinism test measure the content loader instead of the simulation — alternatives: `DefaultHasher` (unstable), `derive(Hash)` on `GameState` (requires `Hash` on `World` and still hashes the content every tick), serializing to JSON and hashing the bytes (drags serde derives back onto `GameState`, which phase 2 removed on purpose).
- [plan/03] Retry from `GameOver` restores the checkpointed `GameState` *and* rewinds `App::tick_counter` to the checkpoint's `state.tick` — why: every timer in the simulation is an absolute `Tick` (`step_ready_at`, `attack_ready_at`, `invuln_until`, every AI `until`), so restoring a tick-900 state while the counter sits at tick 5000 would fire every cooldown and AI timer at once; phase 6 keeps this rule when it repoints the restore at the save file — alternatives: rebasing every stored deadline by the tick delta on restore (the same result, spread across every timer field and re-derived on each future field), storing timers as remaining durations instead of deadlines (a larger refactor of already-reviewed phase-1/2 code for no gameplay gain).
- [plan/03] `GameEvent` gains four variants beyond ARCHITECTURE.md's list — `AttackSwung`, `AttackDeflected`, `EnemyMoved`, `EnemyAiChanged` — why: `App::tick` sets the dirty flag only when `update` returns events, so an enemy that moves or starts telegraphing while the hero stands still must announce itself or the frame is never redrawn; the alternative of always redrawing while enemies exist breaks CLAUDE.md's "do not draw when nothing changed" — alternatives: a `bool` return from `update` alongside the events (changes the signature CLAUDE.md pins), a `visual_dirty` flag on `GameState` (render bookkeeping inside the pure simulation, and it would have to be excluded from the state hash).
- [plan/03] The overworld pacing table (start room and `room.crossroads` hold zero enemies; slimes in the mid ring; bats in the north ring; one guardian in the far corner `room.north_ridge`) is fixed now and pinned by a test — why: RISKS #2 says the pacing ramp of §7 must be content structure rather than intention, and phase 4 authors the sword into `room.crossroads`, so the two rooms on the first stretch of the critical path have to stay clear — alternatives: placing enemies in phase 4 alongside the items (leaves this phase's AI untested against real content), spreading enemies uniformly (no ramp).
- [plan/03] The validator gains `check_enemy_spawns` (spawn and every patrol waypoint in bounds, walkable, not a hazard, not a door tile) with a `broken_enemy_spawn.ron` fixture — why: the first authored enemies arrive this phase and RISKS #1's rule is that a new authored object gets its check in the same phase it appears, not later — alternatives: deferring the check to phase 4/5 (a spawn inside a wall would be an invisible enemy that never acts, discovered by hand or not at all), checking spawns at runtime in `spawn_enemies` (the simulation is total and must not report content errors).
- [phase-3/impl] `Swing.at` is `Option<Pos>`, not the `Pos` in the Design section's pseudocode — why: combat rules say an off-grid swing (`step_target` returns `None`) still costs the cooldown but lands on nothing, and `GameEvent::AttackSwung { at: Option<Pos> }` already carries that optionality; a non-optional `Swing.at` would need a sentinel position for the off-grid case — alternatives: a sentinel `Pos` (magic value), never constructing a `Swing` at all when off-grid (loses the cooldown-still-applies behaviour the design calls out).

## PHASE 3 — REVIEW FIXES, round 1 (2026-09-14)

- [phase-3/review-fix-r1] `combat::expire_swing` now returns `bool`, and `state::update` pushes a new `GameEvent::AttackEnded` (a fifth variant beyond the four already logged above) when it fires — review major: clearing `hero.attack` reported no event, so `App::tick`'s dirty flag stayed false on the tick the sword animation ended and the glyph never got a redraw to clear it — why a new event rather than reusing an existing one: nothing else in the six-step pipeline fires exactly "the swing just ended", and folding it into e.g. `AttackSwung` would make that variant mean two different things — alternatives: a `changed: bool` return threaded out of `combat::expire_swing` and `OR`-ed into a hypothetical `update` return tuple (changes the `update(&mut GameState, &[Action], tick) -> Vec<GameEvent>` signature CLAUDE.md/ARCHITECTURE.md pin), always marking the tick dirty while `hero.attack.is_some()` at tick start (redraws every active-window tick even when nothing else changed, weaker than the existing per-event dirty model).
- [phase-3/review-fix-r1] The door-transition branch in `state::update` now clears `state.hero.attack = None` immediately before `spawn_enemies()`, leaving `attack_ready_at` untouched — review major: a swing survived into the new room carrying the old room's target tile and the old `EnemyId`s, so it could damage an unrelated enemy standing on the stale tile while leaving the new enemy at the matching index immune — why leave `attack_ready_at` alone: the cooldown was already paid when the swing started, and clearing it too would let the hero re-attack instantly on room entry, which nothing in §6 asks for — alternatives: clearing the whole `Swing` including refunding the cooldown (an unrequested buff for walking through a door mid-swing), leaving `resolve_swing`/`expire_swing` to guard on `state.room` instead (adds a room check to two functions that currently don't need to know about rooms at all, for the same outcome).
- [phase-3/review-fix-r1] `AiState` gains a ninth variant, `SlimeWander { until }`, entered from `SlimeIdle` (and from `SlimeWander` itself) when the idle timer expires with the hero still outside `SLIME_AGGRO_RADIUS`; it wanders identically to `SlimeIdle` but is a distinct discriminant — review major: the liveness test's `AiState != AiState` comparison (full equality, `until` included) could not tell a genuine state change from `SlimeIdle{until:60}` re-timering to `SlimeIdle{until:120}`, and the slime actually does exactly that forever whenever the hero stays out of range, which is most of the 1000-tick maze fixture — why a new variant over the review's other suggested fix (an unconditional chase attempt on timeout, mirroring the guardian's unconditional patrol-to-telegraph timeout): an unconditional chase would make the slime close distance across the whole room on a timer regardless of `SLIME_AGGRO_RADIUS`, silently deleting the only thing that constant gates; a same-behaviour twin state preserves the authored aggro-range design while still guaranteeing the discriminant changes on schedule — alternatives: the unconditional-chase fix (rejected above), relaxing the test to allow a stable idle state out of range (removes the only automated guard RISKS #14 has), leaving `SLIME_AGGRO_RADIUS` unused and always chasing (same rejection as the unconditional-chase option).
- [phase-3/review-fix-r1] `tests/ai.rs::each_kind_keeps_changing_state_for_1000_ticks` now compares `std::mem::discriminant(&AiState)` rather than the whole value — review major, paired with the `SlimeWander` fix above: this is what `ai::step` itself already treats as "a state change" (it emits `EnemyAiChanged` on a discriminant change, not a full-value change), so the test now checks the same property the implementation guarantees instead of a stricter one no enemy actually satisfies.
- [phase-3/review-fix-r1] `tests/determinism.rs::different_seeds_diverge` now compares `enemies[..].pos` traces captured from `room.west_grove` (the nearest authored room with a live slime) instead of comparing `state_hash()` — review minor: `state_hash` writes `rng.raw_state()`, which differs between `Rng::new(7)`/`Rng::new(8)` from construction alone, so the old assertion held even if the seed never reached gameplay; starting in the empty start room wouldn't fix this either, since nothing there consumes the RNG — alternatives: hashing everything except `rng.raw_state()` (works, but a hand-maintained "hash minus one field" duplicates `StateHasher` for a single test), asserting on `hero.pos` instead of enemy positions (the hero's path is driven by the fixed `action_sequence`, not by `state.rng`, so it wouldn't prove the seed does anything).
- [phase-3/review-fix-r1] `tests/mode_machine.rs` gains `retry_restores_the_room_entry_checkpoint_not_a_fresh_hero`, which damages the hero (to 3 of 6 half-hearts) before crossing a door and dies in the *new* room — review minor: the existing death/retry test only killed the hero in the start room at full health, so the `RoomEntered => checkpoint = state.clone()` branch in `App::tick` had no coverage, and a checkpoint at full health is indistinguishable from a freshly reset hero.
- [phase-3/review-fix-r1] `ai::path_step` now returns `None` immediately when `from` or `to` falls outside `ROOM_W`/`ROOM_H`, and the `.expect("a visited non-start cell always has a recorded first step")` is replaced with a `let-else { continue }` — review minor: the function indexed straight into fixed `[ROOM_H][ROOM_W]` arrays with an unchecked `from`, so any out-of-range `Pos` (the type itself is `u8`, far wider than the room) would panic in a module documented as total; a new `tests/ai.rs::path_step_is_total_for_out_of_bounds_positions` pins the fix — alternatives: leaving it to caller discipline (every current caller happens to pass in-bounds positions, but "happens to" is not what "total" means), returning a `Result` instead of `None` (every other navigation function in this module already uses `Option` for "no answer").
- [phase-3/review-fix-r1] `docs/user/controls.md`'s retry line now reads "with the health they had on entering it" instead of "health restored" — review minor: the old wording reads as "restored to full", but the checkpoint's health is whatever the hero had on entering that room, which can be less than full.
- [phase-3/review-fix-r1] `render::scene::telegraph_lane` now stops at the same predicate the dash itself uses (`is_walkable() && !is_hazard()`), not `is_walkable()` alone — review nit, fixed as trivial: the cue could otherwise promise `!` on a hazard tile the dash will never actually reach. Occupancy is deliberately not folded into the same predicate — the lane is computed once when `GuardianTelegraph` begins and occupancy changes every tick, so matching it exactly would need re-deriving the lane per frame from live state rather than from the enemy's own position and facing, which is a larger change than a one-line predicate swap for a readability nit.
- [plan/03] The vestigial `serde::Serialize` derives on `Hero`, `Facing` and `Rng` are dropped — why: nothing serializes them (the only former consumer was removed in phase 2's review fixes), `Swing` would otherwise have to derive it too, and phase 6 defines the on-disk shape from scratch — the same reasoning already applied to `GameState`/`Progress` — alternatives: keeping and extending the derives (an on-disk shape nobody chose, inherited by accident), adding `#[serde(skip)]` to the new fields (the misleading derive stays).

## PHASE 3 — REVIEW FIXES, round 2 (2026-09-14)

- [phase-3/review-fix-r2] `combat::direction_away` is renamed `direction_toward` and the contact-damage call site now reads `direction_toward(source_pos, hero_pos, hero.facing)` instead of `direction_away(hero_pos, source_pos, hero.facing)` — review major: the old call computed the direction *from the hero toward the enemy* and used it as the knockback facing, so the hero was pushed straight into the attacker; because the attacker's own tile is in `apply_contact_damage`'s `occupied` set, `knockback()` hit that check on its very first step and was a silent no-op in every reachable configuration, despite CHANGELOG/PLAN/DECISIONS all describing working knockback. Fixed at the call site (swap the argument order) rather than changing `knockback()` itself, and the helper renamed to match what it actually computes (direction from the first argument toward the second) so the next call site can't repeat the mix-up by reading the name backwards — alternatives: keeping the old name and adding a comment warning about argument order (a comment doesn't stop the next transposition the way a name that matches behaviour does), inverting inside `knockback()` instead of at the call site (would also invert every existing direct caller, i.e. the two knockback-into-a-wall/door tests, which pass `Facing` values that are already correct for `knockback()`'s own semantics).
- [phase-3/review-fix-r2] `AiState::SlimeWander` gains a `facing: Facing` field, drawn once on entry and held for the whole phase (redrawn only if a step is blocked); `SlimeIdle` still redraws every step — review minor: the two variants used to be line-for-line identical movement, so the liveness test's discriminant flip was the only thing distinguishing "idle" from "wander", and a slime's actual on-screen behaviour never changed between them — alternatives: dropping `SlimeWander` and asserting the slime's position changes at least once per `MAX_STALL_TICKS` instead (weakens the round-1 fix's guarantee that the discriminant itself must flip, which is the property `ai::step`'s `EnemyAiChanged` emission and the review's own round-1 fix both rely on), redrawing every step but committing to it for `SLIME_STEP_TICKS * N` ticks via a counter (an extra field for no behavioural difference from storing the drawn facing directly).
- [phase-3/review-fix-r2] `Hero` gains a `died: bool` latch, set the first tick `update` observes `health_halves == 0`; `GameEvent::HeroDied` fires only when the flag is false, and the flag is included in `state_hash()` for consistency with every other `Hero` field — review nit: `HeroDied` used to fire on every tick health stayed at zero, not just the transition. A plain "was health > 0 at the start of *this* `update` call" check (the review's own suggested fix) was tried first and reverted: `tests/mode_machine.rs`'s death tests set `hero.health_halves = 0` directly (bypassing `update`) and then call `tick` once, expecting `HeroDied` on that first call — a start-of-tick check sees health already at zero *before* the call and never fires. The latch fires exactly once regardless of whether zero health was reached through combat mid-tick or through a test harness poking the field directly, which is what "once per death" has to mean if external callers are allowed to set health at all — alternatives: the start-of-tick check (breaks the three existing death tests named above), rewriting those tests to kill the hero only through `combat::apply_contact_damage` (turns three tests about `GameOver`/checkpoint/retry into contact-damage tests, and loses direct control over which room/health the checkpoint captures), gating on `App`-level state instead of `Hero` (the nit is explicitly about direct `update()` callers, which have no `App`).
- [phase-3/review-fix-r2] Hero movement in `state::update` now also refuses a target tile occupied by a live enemy (emitting `MoveBlocked`, same as a wall) — review minor: the check was `Tile::is_walkable` only, so the hero could overlap a live enemy's tile; `draw_scene` paints enemies before the hero, so the enemy glyph vanished under the hero's, and `apply_contact_damage`'s adjacency-or-same check then took the degenerate `dx==0/dy==0` knockback fallback (push along the hero's own facing) instead of a real push away from the source. A dead enemy's tile is left walkable, matching `ai::try_move`'s own live-only occupancy rule — alternatives: leaving the overlap allowed and fixing only the render order (spec §6 describes solid objects as blocking movement; a walkable enemy tile is also a walkable-through hazard-free tile from the hero's perspective, which is not what "solid" means), blocking on `orthogonally_adjacent_or_same` instead of exact-tile occupancy (would forbid the hero from ever standing next to an enemy at all, which is not the bug being fixed — the bug is standing *on* it).
- [phase-3/review-fix-r2] `tests/determinism.rs::different_seeds_diverge` now asserts both the enemy-position divergence (round-1's fix) and a `state_hash()` divergence, captured from the same `room.west_grove` traces — review minor: criterion 10 is worded in terms of `state_hash()` differing, and round-1's fix replaced that assertion outright rather than adding to it; position divergence implies hash divergence today (positions are hashed), but only by inspection, and a future field added to `Enemy` without a matching `StateHasher` write would go uncaught by position-only comparison — alternatives: keeping only the position assertion (loses direct coverage of `state_hash` itself, which is the property criterion 10 and ADR 0002 actually name), reverting to a hash-only assertion (round-1 already showed this passes vacuously off `rng.raw_state()` alone and proves nothing about gameplay).
- [plan/04] Authored chests, NPCs and torches occupy a tile and are **solid**; plates are the one non-solid object; all of them are acted on by facing the tile and pressing `Interact`/`UseLantern` — why: it is the same hitbox rule the sword already uses ("the single tile the hero faces", §6) and the same rule §6 states for the lantern ("activates the torch in the tile in front of the hero"), so one predicate covers interaction, lighting and collision, and "the lantern lights a torch on the faced tile only" becomes a direct assertion — alternatives: a proximity radius (needs a tie-break when two objects are in range, and no radius is in the spec), objects living on a separate non-blocking layer (a chest you can stand on has no facing to interact from, and §6 calls solid objects blocking).
- [plan/04] `Progress` names authored objects with `ObjectRef { room: RoomIdx, index: u16 }` rather than interning global chest/torch/puzzle ids — why: it needs no loader change, is `Ord` (so `BTreeSet` iteration stays deterministic for `state_hash`), and resolves back to the authored string id through `World` when phase 6 defines the on-disk shape; phase 3 already established that `Progress` is deliberately not `Serialize` for exactly this reason — alternatives: a global interned `ChestIdx`/`TorchIdx` table (a loader refactor for no runtime gain, and still not the save vocabulary), storing the authored `String` ids at runtime (allocates on every checkpoint clone and hashes a string per object every tick).
- [plan/04] Story flags are stored as `BTreeSet<String>` of authored flag ids, not interned indices — why: flags are few (four in the whole game) and short, they *are* the save vocabulary ARCHITECTURE fixes, and the validator checks every referenced flag is set by some dialogue node, so a typo is a build error rather than a silently dead flag — alternatives: interning to `FlagIdx` (a third id table for four values), a bitfield over authored flags (breaks the "save stores strings, not indices" rule the moment phase 6 writes it).
- [plan/04] While `GameState::dialogue` is `Some`, `update()` processes only `Confirm`/`Cancel` and the six-step per-tick pipeline does not run; `App` routes dialogue keys into `update` at the **current** `tick_counter` without incrementing it — why: dialogue writes story flags, which are simulation state, so the cursor has to live in the pure layer to keep `App` from mutating `GameState` directly; suspending the pipeline is what makes "the simulation is paused while the dialogue is open" provable as `state.tick` and `state_hash()` not moving, and reusing the pinned `update` signature avoids a second simulation entry point — alternatives: the dialogue cursor in `App` with flags applied on close (App mutating game state, and the flag no longer lands on the node that declares it), ticking normally while a dialogue is open (enemies hit the player through a modal window), a separate `game::dialogue::advance` entry point (breaks CLAUDE.md's single-entry-point rule).
- [plan/04] `Action::Attack` without `hero.has_sword` emits a message and starts no swing (and costs no cooldown) — why: §7's ramp is "teach movement, then interaction, then attack", and RISKS #2 says that ramp must be content structure rather than intent; a hero who can already fight makes the sword chest decorative and the "no enemy reachable before the sword" criterion meaningless — alternatives: leaving `Attack` always available and relying on placement alone (the ramp then rests entirely on where enemies are authored), a zero-damage swing without the sword (an animation that does nothing is worse feedback than an explicit message). Phase-3 combat tests set `has_sword = true` in their fixture helpers; no assertion is weakened.
- [plan/04] The validator's `Loc` changes from `(RoomIdx, component)` to `(RoomIdx, Pos)` and `flood` becomes a plain cross-room tile BFS; `compute_components` is deleted — why: torch reveals change a room's connectivity, so a precomputed component map would have to be recomputed (and renumbered) on every reveal, and component ids from one fixpoint iteration would then be compared against another's; a tile BFS over 15 rooms is 5 760 nodes, needs no renumbering, and lets "object X is reachable" be asked directly as "some orthogonal neighbour of X's tile is reached" — which is the right question now that objects are solid — alternatives: recomputing components per reveal and carrying the map alongside each state (the renumbering hazard, for a search this small), computing components once with every hidden tile optimistically open (would declare a passage reachable whose torch is not).
- [plan/04] `ReachabilitySearch`'s item fixpoint grows two more monotone sets — lit torches (reachable torch + lantern) and story flags (reachable NPC whose condition holds) — and `LockKind::Flag` stops being unconditionally `LockNeverUnlockable` — why: hidden passages and a flag-locked door are authored for the first time this phase, and RISKS #1's rule is that a new authored mechanic gets its check in the same phase it appears; both sets only ever grow, so the loop terminates for the same reason the item fixpoint does — alternatives: leaving reveals out of the search (a secret behind a torch nobody can reach would validate), keeping `Flag` locks unconditionally rejected (then no flag lock can ever be authored, and the `LockKind` variant is dead content).
- [plan/04] "Main route" is computed as the union, over every route-critical target (the rooms holding a non-secret `Sword`/`Lantern`/`Ember` chest, plus `route.goal` and `route.home`), of every room on **any** shortest path from `start.room`; `Route` gains a `goal: String` field — why: the acceptance criterion asks for secrets that are not on the validator's main-route path, which needs a definition that is deterministic and has no arbitrary tie-break; union-of-all-shortest-paths has both, and an explicit `goal` avoids inferring the route's end from content (which would be ambiguous until the ember exists in phase 5) — alternatives: a single shortest path (an arbitrary choice between two equal paths, so the check's verdict would depend on room ordering), defining the main route as everything reachable before the lantern (most secrets are reachable then too, so the check would be vacuous).
- [plan/04] Secrets are marked as `Chest.secret: bool` rather than ARCHITECTURE's separate `Room.secrets` vector — why: a secret *is* a chest behind a hidden passage; a parallel vector would duplicate the reward/placement/opened-once machinery and give the validator two object families to check instead of one — alternatives: `Room.secrets: Vec<Secret>` as listed (duplication), inferring "secret" from "sits behind a hidden tile" (implicit, and it would silently reclassify a main-route chest if a passage were ever authored in front of it).
- [plan/04] The overworld puzzle of §7 step 4 is a new `PuzzleKind::StepPlates` — press every plate in the room, in any order, to reveal the alcove holding the lantern — why: the puzzle must be solvable *before* the lantern exists, which rules out the torch-sequence kind, and implementing block pushing here would pull phase 5's `BlockOnPlates` forward; pressure that holds once pressed makes an irreversible wrong state impossible, which is §6's no-soft-lock rule satisfied by construction — alternatives: reusing `PushBlock` early (phase-5 scope, and a block pushed into a corner is exactly the soft-lock case that then needs handling), a torch puzzle (needs the lantern the puzzle is supposed to reward).
- [plan/04] `door.lighthouse.east` and `door.lighthouse.west` (and their reciprocals in `room.south_shore` / `room.fallen_pines`, the four `+` tiles and the four spawns) are removed, making the start room a cul-de-sac whose only exit leads to the sword room — why: both side neighbours hold phase-3 enemy spawns, so with those doors open an enemy is reachable before the sword and the pacing criterion is unsatisfiable without emptying two more rooms; removing two doors keeps the 3×3 ring fully connected and encodes the ramp in geometry instead of in enemy bookkeeping — alternatives: moving the enemies out of `room.fallen_pines` and `room.south_shore` (four rooms then have to stay clear, and the two secret rooms become the safest places in the game), gating the side doors behind a flag (a lock on the first minute of play, for pacing the player cannot see).
- [plan/04] `room.sanctuary_gate` — a one-room dungeon vestibule — is authored now, so the lantern-locked entrance has a real, reachable, validating target; phase 5 expands the dungeon from it and counts it as one of the six rooms — why: the criterion "the dungeon entrance opens with the lantern" needs a door that actually leads somewhere, and a door whose `to_room` does not resolve is a validator error — alternatives: deferring the entrance door to phase 5 (drops an acceptance criterion), pointing the door back at an overworld room as a placeholder (a lie in the content that phase 5 has to remember to undo).
- [plan/04] `Some(LockKind::SmallKey)` doors block the hero with a message this phase; key *consumption* and `Progress::unlocked_doors` are phase 5 — why: no small-key door is authored before the dungeon exists, so this is unreachable content rather than a half-implemented mechanic on the main route, and the roadmap places key consumption in phase 5 — alternatives: implementing consumption now (phase-5 scope, untestable against real content until the dungeon exists), leaving `SmallKey` doors freely passable (a lock that does not lock, which the validator's `LockNeverUnlockable` check would not catch).
- [plan/04] `GameEvent` gains `ChestOpened`, `ItemPicked`, `DialogueStarted`, `DialogueAdvanced`, `DialogueEnded`, `FlagSet`, `TorchLit`, `PassageRevealed`, `PlatePressed`, `PuzzleSolved` and `DoorBlocked` — the last four beyond ARCHITECTURE's list — why: `App::tick` sets the dirty flag only when `update` returns events, so every state change with a visible consequence has to announce itself (the same reasoning that added four variants in phase 3); `DoorBlocked` additionally carries which lock refused, so the message row can say *why* — alternatives: folding them into `Message` (the mode machine and the autosave triggers of phase 6 would then have to parse text), no event for reveals and plate presses (the frame that opens a passage would not be redrawn).
- [plan/04] `HeartContainer` raises `max_health_halves` by 2 up to a hard cap of 10 (5 hearts, §2) and heals the same amount — why: §2 fixes 3 hearts at the start and 5 as the maximum, and the roadmap's assumption closes that gap with exactly two containers (one secret here, one dungeon reward in phase 5); the cap makes a third container authored by mistake harmless rather than a stat overflow — alternatives: no cap (content error becomes a balance bug), granting a full heart with no heal (the player would have to go and find healing to use the reward they just earned).

## PHASE 4 — IMPLEMENT (2026-09-14)

- [phase-4/implement] The start room's authored `hint` is shown by `App::apply_main_menu`'s New
  Game branch directly, not by the `GameEvent::RoomEntered` handler in `App::tick` — why:
  `GameState::new` inserts the start room into `progress.visited` without ever emitting
  `RoomEntered` for it (phase 3's rule: `RoomEntered` marks a *transition*, and the start room is
  never transitioned into), so a hint that only fired on that event would never show for the very
  room §7 wants it in — alternatives: emitting a synthetic `RoomEntered` for the start room at
  construction (breaks the existing `room_entered_is_emitted_once_per_transition_and_not_for_the_start_room`
  test and ADR-level meaning of the event), duplicating the hint into a main-menu message field
  (two sources of truth for the same string).
- [phase-4/implement] `render/overlays.rs::draw_box` uses `Wrap { trim: false }`, not `trim: true`
  — why: `trim: true` strips leading whitespace from each wrapped line, which silently shifted the
  Map overlay's `@`/`C`/`>`/`N` markers two columns left (the grid's leading spaces are load-bearing
  alignment, not incidental indentation); caught by
  `tests/render.rs::map_overlay_hides_unvisited_rooms_and_marks_the_current_one` failing at the
  computed cell rather than by inspection — alternatives: keeping `trim: true` and hand-computing
  the post-trim column for the test (couples the test to a wrap corner case instead of the actual
  layout), building the grid as three separate `set_string` calls instead of `Paragraph` lines
  (loses the automatic wrap the dialogue window's longer node text still wants).
- [phase-4/implement] The HUD's `Item:` slot shows `sword` if held, else `lantern` if held, else
  `none` — never both, even once the overworld gives the hero both by the last milestone — why:
  ARCHITECTURE's HUD is a single-item slot (`Item: lantern` / `sword` / `none`, PLAN.md's own
  wording), matching the one-tile-at-a-time equipment display genre convention it is drawn from;
  the Inventory screen (`I`) already lists every piece of equipment at once — alternatives: a
  multi-item HUD string (wider than the budgeted HUD row leaves room for once keys/hearts are also
  shown), always showing the most-recently-acquired item (adds a field to `Hero` for a cosmetic
  ordering the fixed sword-then-lantern priority already gives for free, since the sword always
  comes first on the main route).
- [phase-4/implement] `check_puzzles` does not add a distinct error for a `StepPlates` puzzle
  authored with zero plates, despite PLAN.md's design prose listing it alongside `UnknownPlate` —
  why: `puzzles::on_hero_moved` already treats an empty `plates` list as permanently unsolved and
  skips it outright (dead but harmless content, not a runtime hazard), no fixture in PLAN.md's own
  T9 fixture list exercises it, and the two declared error variants (`UnknownPlate`,
  `RevealNotHidden`) fully cover the checks that *do* have fixtures — alternatives: adding an
  `EmptyPuzzle` variant with no fixture or test (an error path nothing proves fires), overloading
  `UnknownPlate` with an empty-string `plate` field to signal "no plates at all" (a confusing
  double meaning for one variant, for content that authoring the real world never produces).
- [phase-4/implement] `npc.keeper`'s dialogue is two nodes (an intro line, then the line that sets
  `flag.told_about_sanctuary`) rather than PLAN.md's originally-sketched single node — why: a
  one-node dialogue can never exercise "advances in node order," and T5's own acceptance list asks
  for node-order coverage; splitting the existing line in two costs nothing in validator or
  reachability terms and reads better in play — alternatives: keeping one node and testing order
  against a throwaway fixture NPC instead of real content (real content is what T13's milestone
  test already walks to, so a second NPC would be pure test scaffolding).
- [phase-4/implement] `tests/common/mod.rs` provides two drivers, not one: `App`-based
  helpers (`new_game`, `step`, `walk_to`, `face`, matching PLAN.md's literal T12 signatures) for
  mode-machine-level tests, and a `Runner` wrapping `GameState` + `game::update` directly for tests
  that need the raw `GameEvent`s a step produced — why: `App::tick` only returns whether *something*
  changed, folding every event into mode/message/checkpoint side effects, so an App-only driver
  cannot assert "this reward was `ItemPicked`" or "this dialogue node set this flag" the way
  `tests/overworld.rs`/`tests/dialogue.rs` need to; every other phase-3 test already asserts
  directly on `game::update`'s return value, so `Runner` keeps that precedent rather than inventing
  an event-inspection back door on `App` for tests only — alternatives: adding a test-only event
  buffer to `App` (a production-code seam that exists only to be read by tests), driving every new
  test through `App` and asserting on resulting `GameState` fields alone (loses the event-level
  assertions the taxonomy in PLAN.md's "additional cases" list explicitly asks for, e.g. the exact
  `Message` text for an empty chest or an unlit torch).
- [phase-4/implement] `Runner::cross_door` retries the same movement action until the room
  actually changes, instead of the single `step` call `walk_to`'s own last step already used —
  why: the hero's step cooldown (`HERO_STEP_TICKS = 4`) does not reset at a room boundary, so a
  `walk_to` that lands a hero next to a door and a single follow-up `MoveNorth` in the very next
  tick can land on a cooldown tick — facing updates (spec §6: an attempted move always sets facing,
  even when blocked) but the actual step, and the door crossing with it, does not happen yet;
  `main_route_milestones_happen_in_order` failed against `RoomIdx(7)` (still `room.lighthouse`)
  before this fix, for exactly that reason — alternatives: padding every door-approach `walk_to`
  target so the door tile is never the very next step (fragile — depends on which direction the
  BFS's last leg happened to arrive from), busy-waiting a fixed number of idle ticks before
  crossing (couples the test to `HERO_STEP_TICKS`'s numeric value instead of to "the room changed").
- [phase-4/implement] `tests/transitions.rs`'s generic per-door walker (`every_door_leads_to_its_target_room`,
  `reciprocal_door_returns_to_the_origin_door`) grants the hero the lantern in its shared `state_at`
  helper, and exempts exactly `door.east_marsh.dungeon_entrance` from the `door.<room>.<edge>`
  naming-convention assertion — why: those tests predate locks and enumerate *every* authored door
  generically; without the lantern the one locked door in the world would report `MoveBlocked`
  instead of a room change, which is a lock-gating assertion these tests were never meant to make
  (that is `tests/overworld.rs::dungeon_entrance_needs_the_lantern`'s job). The entrance is also
  deliberately named for what it is rather than for the wall edge it sits on (PLAN.md's own
  wording), which the ordinary convention check would otherwise flag as a wiring mistake —
  alternatives: giving every door-walker test its own copy of `state_at` with the lantern only in
  the copies that need it (duplicates the helper for a one-line difference), renaming the door to
  `door.east_marsh.east` to satisfy the convention (loses the name PLAN.md's design section itself
  chose for this door and CHANGELOG/DECISIONS already reference).
- [phase-4/review_fix] `apply_reward` now emits a short `GameEvent::Message` alongside
  `ItemPicked` for every reward except `Reward::Message` itself ("You take the forest sword.",
  "You take the lantern.", "You take a small key.", "Your heart grows stronger.", "You take the
  ancient ember.") — why: PLAN.md's own interaction table specified `ChestOpened` + `ItemPicked` +
  `Message` for an unopened chest, but only `Reward::Message` ever produced one; every other pickup
  left the HUD message row showing whatever was there before, which is the one place the game has
  been teaching the player to look (round-2 review, minor) — alternatives: leaving the wording to a
  later content pass (defers a one-line fix the review already scoped precisely; the exact strings
  are cosmetic and trivially rewritten later without touching the event shape).
- [phase-4/review_fix] `tests/overworld.rs`'s "zero enemies before the sword" test now BFS's the
  room-adjacency graph from `world.start_room()`, treating the room holding the non-secret
  `Reward::Sword` chest as a frontier entered but not expanded past, instead of hardcoding
  `room.lighthouse`/`room.crossroads` — why: the hardcoded version could not fail if a door back
  into the cul-de-sac were re-added (round-2 review, major), which is exactly the regression RISKS
  #2 says must stay structurally impossible — alternatives: none considered; the review's suggested
  fix was adopted directly.
- [phase-4/review_fix] `draw_map`'s unopened-chest check now skips `Chest.secret` chests —
  why: the map is meant to show discovered objects (docs/spec.md:210), and a secret chest behind an
  unlit torch is by definition undiscovered; marking it anyway pointed the map at all three secrets
  the moment their room was first entered (round-2 review, minor) — alternatives: gating on
  `is_revealed` instead of `secret` (closer to "discovered" in the literal sense, but every secret
  chest in this phase sits behind a torch reveal already, so the simpler `!c.secret` check is
  equivalent today and does not need the render layer to reason about reveals at all).
- [phase-4/review_fix] `mutating_any_hashed_field_changes_the_hash` (`src/game/state.rs`) gets one
  row per field `state_hash` grew in this phase: `hero.has_sword`/`has_lantern`/`has_ember`,
  `progress.opened_chests`/`lit_torches`/`solved_puzzles`/`flags`, `dialogue`, `plates.pressed` —
  why: PLAN.md T2 promised this and was checked off, but the table still ended at
  `progress.visited`, so the hash's own "a field added later without a mutation here fails loudly"
  doc comment was false for nine of its own fields (round-2 review, major) — alternatives: none;
  the fix is mechanical, one row per field already listed in `state_hash`'s body.
- [phase-4/review_fix] `map_overlay_hides_unvisited_rooms_and_marks_the_current_one`
  (`tests/render.rs`) now also asserts a neighbouring unvisited room's cell is blank, and a new test
  `map_overlay_reveals_a_visited_non_current_room_with_its_unopened_chest` inserts a room directly
  into `progress.visited` (without moving the hero) and asserts its cell renders `C` — why: the
  original test only ever visited one room, so it could not tell "hides unvisited rooms" from
  "renders every room"; deleting `draw_map`'s visited-gate would still have passed it (round-2
  review, major) — alternatives: driving a real door crossing instead of inserting into `visited`
  directly (correct too, but the room the hero would then stand in reports `@`, not `C`, per
  `draw_map`'s own priority order, so it cannot exercise the "visited but not current" case the
  fix needs; direct insertion isolates the visited-gate from door traversal, which is already
  covered elsewhere).
- [phase-4/review_fix] RISKS #10's glyph-distinguishability mitigation gets real assertions instead
  of vacuous ones: `tiles::every_kind_has_a_glyph` is replaced by a uniqueness check (`ALL_KINDS`
  mapped through `glyph` into a `HashSet`, asserting no two kinds share a character); `theme.rs`'s
  glyph-table test gets a real assertion that `Mono` returns only a white/black/gray family colour
  for every kind; `combat_scene_setup` (`tests/render.rs`) moves its slime off `npc.keeper`'s tile
  (`(5,5)` -> `(5,10)`) since the scene paints objects before enemies and the overlap silently hid
  the NPC glyph from the theme-identity assertion; a new test
  `object_kind_glyphs_render_identically_under_every_theme` renders `room.crossroads` (unopened
  chest), `room.old_mill` (one plate pressed, its chest opened), `room.north_ridge` (unlit torch)
  and `room.south_shore` (lit torch) under all three themes, so `Chest`/`ChestOpen`/`Torch`/
  `TorchLit`/`Plate`/`PlatePressed` each render at least once under the check — why: none of the
  three tests the plan credited this risk to could actually fail (`glyph(kind) != '\0'` is true of
  every char literal in the match; the theme test had no assertion at all; the one scene extended
  for "the new objects" only ever showed `Npc`, and hid even that) (round-2 review, major) —
  alternatives: one single giant scene holding all seven kinds at once (no authored room carries
  chest + torch + plate + npc together, so this would need a synthetic room outside the validated
  content, defeating the point of testing the real glyph/theme pipeline against real content).
- [phase-4/review_fix] `check_transitions` now validates `route.goal` resolves to a room, mirroring
  the existing `route.home` check, with fixture `tests/fixtures/broken_unknown_goal.ron` and
  `tests/content.rs::unknown_route_goal_is_rejected` — why: `route.goal` was a new authored
  reference this phase added, and `main_route_rooms` silently dropped an unresolvable goal from its
  target set instead of erroring, which would falsely widen `check_secrets`' allowed-secret rooms
  on a typo (round-2 review, minor) — alternatives: none; this is the exact mirror of an existing
  check.
- [phase-4/review_fix] `main_route_milestones_happen_in_order` (`tests/overworld.rs`) gets a third
  milestone assertion, between taking the sword and crossing into `room.stone_circle`: that
  `room.west_grove` carries a `Slime` spawn and is one door from the sword room — why: acceptance
  criterion 1 lists "first slime encounter possible" as one of five ordered milestones, but the
  test asserted the other four and skipped this one entirely (round-2 review, minor); PLAN.md's own
  verification table is also updated to name the test functions that actually exist (six of ten
  rows named functions that were never written under those names) so the table stays a usable
  traceability artifact for phase 5 — alternatives: walking the hero into `room.west_grove` and
  provoking a real slime encounter (exercises more code but adds a several-tile detour to an
  already-long milestone test for a fact the room's own authored content already proves; asserting
  the spawn and the door adjacency is the same claim without the detour).
- [phase-4/review_fix] `tests/common/mod.rs` drops the `App`-based `next_step`, and `walk_to` now
  calls the `GameState`-based `next_step_in(&app.state, target)` directly — why: the two functions
  were ~40 identical lines differing only in whether they read `app.state` or an owned `&GameState`,
  and phase 5's `tests/playthrough.rs` is explicitly meant to reuse this module unchanged, so two
  copies of the room-local BFS would drift the moment one of them changed (round-2 review, minor)
  — alternatives: none; deleting the duplicate was the review's own suggested fix.
- [phase-05/plan] `PuzzleKind::{PushBlock, Switches}` are renamed to `{BlockOnPlates, TorchSequence}` —
  why: `.autodev/ARCHITECTURE.md` and this phase's deliverables both name the two kinds
  `BlockOnPlates`/`TorchSequence`, neither variant is referenced by `assets/world.ron` yet, so the rename
  is free and stops the schema and the design docs describing the same two puzzles under different names —
  alternatives: keep the phase-2 names and correct ARCHITECTURE.md instead (loses the deliverable's own
  vocabulary); keep both as aliases (two spellings of one concept in the content surface).
- [phase-05/plan] Boss defeat is represented as an authored story flag (`EnemySpawn::defeat_flag`, set into
  `Progress::flags`) rather than the `Progress::boss_defeated: bool` field ARCHITECTURE.md lists — why:
  flags are already hashed, already the save vocabulary, and already usable by `LockKind::Flag` and
  `Npc::condition`, so one content-driven mechanism covers respawn suppression, post-victory dialogue and
  any future flag-locked door, where a dedicated bool would need parallel handling in the hash, the save
  and the validator — alternatives: the bool as specified (a second mechanism for the same fact);
  inferring defeat from the ember (conflates the reward with the event).
- [phase-05/plan] The boss deals no contact damage: `combat::apply_contact_damage` skips `EnemyKind::Boss`
  and all boss damage comes from telegraphed strikes resolved by `combat::apply_boss_strike` — why: the
  sword hits only the tile the hero faces, so damaging the boss requires standing orthogonally adjacent to
  it; a contact-damaging boss would make the phase's own scripted no-damage acceptance test impossible by
  construction and turn the fight into the damage race spec §6 forbids ("вікно вразливості після атаки") —
  alternatives: keep contact damage and drop the no-damage criterion (contradicts the phase acceptance
  list); give the hero a ranged attack (new mechanic, out of scope, breaks §5's key map).
- [phase-05/plan] The boss lives in `src/game/ai.rs` as a fourth `EnemyKind` state machine, and its damage
  application in `src/game/combat.rs`; no `src/game/boss.rs` is added — why: the boss reuses `Room::enemies`,
  `EnemyId`, the swing hit-list, occupancy and `revealed_set` unchanged, ARCHITECTURE.md already lists
  `Boss` in `EnemySpawn.kind` and `Boss: PhaseOne|PhaseTwo|Stunned|Dead` in the AI enum, and the module
  layout in CLAUDE.md/PROFILE.md names no boss module — alternatives: a new `boss.rs` (a layout deviation
  for ~150 lines that would still need `ai.rs` and `combat.rs` hooks).
- [phase-05/plan] `game::puzzles::PlateState` becomes `PuzzleState { pressed, blocks, sequence }`,
  `GameState::plates` becomes `GameState::puzzle`, and `GameState::spawn_enemies` becomes
  `GameState::enter_room` — why: all three names now describe less than the thing does (the function
  already reset plates and rebuilt enemies, and now also reseeds block positions and clears the torch
  sequence), and the single "everything room-local is reseeded on entry" rule is what implements both the
  unsolved-puzzle reset and the anti-soft-lock guarantee — alternatives: keep the names and add a second
  reset function (two places to forget one field).
- [phase-05/plan] A `BlockOnPlates` puzzle may name exactly one block (validator-enforced), and the
  validator proves it solvable with an exact BFS over `(block_pos, hero_pos)` seeded from the reached tiles
  of the room — why: one block makes the push search exactly one-block Sokoban, which is decidable in
  ≈ 147k states per puzzle (milliseconds) and therefore an honest check, where multi-block Sokoban is
  PSPACE-complete and would have to degrade to an optimistic "the plates are reachable" heuristic on the
  single most route-critical property in RISKS #1 — alternatives: allow N blocks with a heuristic check
  (weakens the validator exactly where it matters); allow N blocks with a bounded search (silently
  unsound past the bound).
- [phase-05/plan] A block's push legality is one shared function, `game::puzzles::block_push_target`,
  called by both `game::update` and `content::validate` — why: the validator's solvability proof is only
  worth anything if it models the same push rule the simulation implements, and two copies of the rule
  would drift on the first tweak (`src/content/` already imports `game::entities`/`game::tuning`, so the
  direction of the dependency is established) — alternatives: duplicate the predicate in the validator
  (drift); move the rule into `content/` (puts simulation semantics in the content module).
- [phase-05/plan] A block may never be pushed onto a `Tile::Door` or `Tile::Stairs` tile — why: this is the
  one wrong state the room-reset rule cannot undo cheaply, since a block parked in a doorway could seal the
  hero out of (or into) a room before they can leave and trigger the reseed; every other wrong position is
  recoverable by walking out and back in — alternatives: allow it and rely on the reset (a doorway block
  can block the very exit the reset needs); make blocks pullable (a new key and a new input mode, against
  §5's single-press map).
- [phase-05/plan] A torch named by a `TorchSequence` puzzle is transient (`PuzzleState::sequence`) and never
  enters `Progress::lit_torches`; the validator forbids such a torch from carrying its own `reveals` —
  why: a sequence has to be resettable on a wrong order and on room exit, which a permanent `lit_torches`
  entry cannot express, and a torch that both resets and permanently reveals a passage would be two
  contradictory mechanics on one object — alternatives: extinguishable permanent torches (makes every
  reveal in the world revocable); a separate `SequenceTorch` authored type (a second torch family in the
  schema for one behavioural bit).
- [phase-05/plan] Unlocking a `SmallKey` door also marks its reciprocal door unlocked in
  `Progress::unlocked_doors` — why: the acceptance criterion is that a second pass through the same doorway
  costs nothing, and a player returning from the far side traverses the *reciprocal* `Door` record, which
  would otherwise demand a second key for a door that is visibly already open — alternatives: author locks
  on both sides and consume a key each way (charges two keys for one door); author locks one-sided only and
  leave the reciprocal free (works for the authored world but silently breaks the moment a two-sided lock
  is authored).
- [phase-05/plan] The dungeon is a linear six-room chain with no secrets; §2's "≥ 3 secrets" is satisfied by
  the three overworld secret chests — why: every dungeon room lies on the main route to `route.goal`, and
  `check_secrets` correctly rejects a secret in a main-route room, so a dungeon secret would require a
  branch room that §2's six-room budget has no space for — alternatives: spend one of the six rooms on an
  off-route branch (drops the chain to five rooms of content); relax `check_secrets` (weakens the check
  that proves no secret gates progress).
- [phase-05/plan] The validator treats a reachable boss spawn as granting its `drops` and its
  `defeat_flag`, and a `TorchSequence` as solved once the lantern is held and every one of its torches is
  adjacent-reachable — why: `content::validate` is a static content check with no combat model and no
  notion of input ordering, so it asserts what it can (the arena is reachable, the reward and flag are
  wired; the torches are all reachable and a wrong order costs only a retry) and leaves the fight and the
  ordering to `tests/boss.rs` and `tests/playthrough.rs` — alternatives: simulate the boss inside the
  validator (a second combat implementation to keep in sync); omit the boss from reachability entirely
  (`EmberUnreachable`/`HomeUnreachableWithEmber` would stop covering the ember once it moved off a chest).

## IMPLEMENT phase 5 (2026-09-14)

- [phase-05/t14] Both `SmallKey` doors in `assets/world.ron` are authored one-way: the forward door
  (`door.flooded_hall.east`, `door.warden_walk.east`) carries `lock: Some(SmallKey)`, its reciprocal
  (`door.plate_chamber.west`, `door.boss_arena.west`) carries no `lock` at all — why: matches the existing
  `door.east_marsh.dungeon_entrance`/`door.sanctuary_gate.west` `Lantern`-lock precedent already in the
  file, is simpler to author and reason about than a two-sided lock, and the "return trip is free" property
  holds trivially (an unlocked door is always open) — alternatives: author both sides `Some(SmallKey)` and
  rely on `resolve_door`'s reciprocal-unlock insertion (the mechanism `Progress::unlocked_doors` exists for,
  per the plan-time decision above); this stays available for future content but nothing in this phase
  needs it, so `tests/dungeon.rs` builds its own synthetic two-sided-lock fixture to exercise that path
  directly rather than relying on real content to happen to cover it.
- [phase-05/t2] `GameState::walkable` grew a `block_free` term (a current-room `puzzle.blocks` position is
  solid) that the design's T2 description of the change didn't spell out explicitly, only naming
  `ai::occupied_positions` — why: `walkable` is the one predicate hero movement, `Interact`/`UseLantern`
  targeting and knockback all go through, and without it a block was solid only via the movement branch's
  own push-detection special case, not via the shared predicate every other caller trusts (caught by
  `tests/puzzles.rs::a_block_blocks_the_hero_and_enemies` failing) — alternatives: none; this is a bugfix
  within T2's own stated scope, not a design change.
- [phase-05/tests] `tests/puzzles.rs`, `tests/dungeon.rs` and `tests/boss.rs` teleport the hero directly into
  the room under test (`state_in(room_id, pos)` + `state.enter_room()`), the same pattern
  `tests/overworld.rs::state_in` already established for phase 4 — why: reaching the dungeon's later rooms
  via the full overworld route (sword, lantern, both keys, every puzzle) for every unit test would make each
  test slow and would couple unrelated mechanics together; the full route is independently proved once by
  `tests/playthrough.rs` — alternatives: drive every test through `tests/common::Runner::walk_to`/`cross_door`
  from a fresh game (slower, and a change to an earlier room's layout would ripple into unrelated tests).

## APPLY REVIEW FIXES phase 5 round 1 (2026-09-14)

- [phase-05/review-r1] Fixed all five MAJOR findings from REVIEW-r1.md: `ai::occupied_positions` now
  includes every current-room block position (a block was solid to the hero via `GameState::walkable` but
  invisible to enemy pathfinding/stepping); `try_push_block` also refuses a push when a live enemy already
  occupies the block's own tile, as a second, independent guard; `tests/boss.rs::each_phase_uses_a_distinct_attack_pattern`
  now drives the fight to a real phase-1 and a real phase-2 telegraph and compares the *observed* tile sets
  against `strike_tiles`, instead of comparing two hand-built `strike_tiles` calls to each other;
  `tests/boss.rs::every_strike_is_preceded_by_a_telegraph_of_at_least_the_tuned_warning` now checks every
  `BossStruck` in a run spanning both phases (matching each strike's tile-set payload to the telegraph that
  carried the same tiles) instead of returning after the first, comfortable-margin phase-1 strike;
  `tests/puzzles.rs::every_wrong_torch_order_still_leaves_the_puzzle_solvable` now iterates all six torch
  permutations instead of four hand-picked ones; `tests/puzzles.rs::every_reachable_wrong_block_position_still_leaves_the_room_solvable`
  now BFS-enumerates every block position a real push sequence can reach in `room.plate_chamber` and
  re-proves plate-reachability from each one, instead of driving to one hand-picked wrong tile;
  `tests/mode_machine.rs` gained the two promised `Mode::Victory` tests (`game_won_enters_victory_mode_and_stops_simulating`,
  `victory_confirm_returns_to_the_main_menu`) that PLAN.md's T10 had marked done without writing — why: each
  was a real gap the review demonstrated concretely (an exploit path, or a test that cannot fail on the bug
  it is named for) — alternatives: none considered; these are correctness fixes and coverage gaps, not
  design trade-offs.
- [phase-05/review-r1] Fixed all three MINOR findings: `content::validate::walkable_for_search` now treats a
  room's authored block position as solid, matching `GameState::walkable`'s `block_free` term, so the
  reachability flood cannot optimistically call a block's own tile walkable before it is pushed clear;
  `ai::boss_ai_phase`/`step_boss`'s `unreachable!()` arms for a non-boss `AiState` on a `Boss` enemy now
  recover to a fresh phase-1 stalk instead of panicking, keeping `game::update`'s "never panics" contract
  total against a future corrupt save (phase 6 deserializes `AiState`); relighting a solved `TorchSequence`
  torch now emits `Message("The flames are already steady.")` instead of silently doing nothing, matching
  every other lantern dead end.
- [phase-05/review-r1] Left the DECISIONS.md `[phase-05/t2]` entry as written rather than editing it: the
  review's complaint is that `tests/puzzles.rs::a_block_blocks_the_hero_and_enemies`'s *name* overclaims
  (it asserted only `state.walkable`, never an enemy), not that the entry's specific claim — that this test
  caught the `walkable`/`block_free` bug — is false; that narrower claim is accurate. The test itself is
  now extended (this round) to actually drive an enemy at the block and assert it never stands there, so the
  name and the coverage agree.

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

# Review — phase 1 round 1

**Verdict:** changes_requested

Phase 1 is structurally strong — clean pure-simulation boundary, a single terminal owner, the `TerminalOps` seam, the crossterm lock-file guard, and the documented deviations all logged in DECISIONS.md. The gate (`fmt --check`, `clippy -D warnings`, `cargo test --locked`) passes on a clean re-run. But the small-terminal path, which is a named acceptance criterion and a risk row, is broken at startup: nothing probes the terminal size, so `TooSmall` is unreachable until a resize event arrives, and launching in a terminal shorter than 21 rows panics inside ratatui (verified in a real pty: exit 101, `index outside of buffer ... index is (25, 20)`). Three further criteria have proofs that cannot fail or do not match the code: "never enabled raw mode" is a comment, "no stale moves replay" is tested on a pure function while the real loop leaves surplus events queued (contradicting the policy in DECISIONS.md), and `--help`/`--version` print to stderr and exit 2 against both convention and the project's own cli.md. `TooSmall` also swallows Quit and Ctrl+C, leaving no keyboard exit — contrary to spec §11.

## [BLOCKER] No terminal size probe at startup — game panics in a terminal shorter than 21 rows
`src/main.rs`

`main.rs` never calls `terminal::size()` (grep: the function is defined at src/terminal.rs:156 and called from nowhere), and `App::new` (src/app.rs:74) hardcodes `size: (MIN_COLS, MIN_ROWS)`. `Mode::TooSmall` is therefore reachable only through a crossterm `Resize` event, and crossterm emits none at startup. Consequences, both verified against the built binary in a real pty:

1. rows < 21: `scene::Layout::compute` floor-divides to `y0 = 0`, `hint_row() = 20` falls outside the buffer, and `Buffer::set_string` hard-panics. Run in a 100x20 pty: `index outside of buffer: the area is Rect { x: 0, y: 0, width: 100, height: 20 } but index is (25, 20)`, process exits 101. `ratatui-core` `index_of` panics in release too (it is `panic!`, not `debug_assert`).
2. 21 <= rows <= 23, or cols < 60: the full game is drawn into a terminal below the documented minimum instead of the TooSmall notice.

This breaks acceptance criterion 8 for the startup case (`.autodev/phases/01-skeleton-terminal-loop/PLAN.md` states "Below 60×24 (at startup or on resize) → TooSmall"), and leaves RISKS #11 unmitigated on the very path the criterion names. `tests/render.rs` misses it because `render_at` always calls `app.on_resize(w, h)` first, and `scripts/terminal-restore-check.sh` misses it because expect's default pty is 80x24.

**Fix:** In `run()`, before the loop: `if let Ok((w, h)) = terminal::size() { app.on_resize(w, h); }`. Belt-and-braces, make `render::draw` guard on the frame it is actually given — `render::is_too_small(frame.area())` already exists (src/render/mod.rs:49) and is currently dead code — so a stale `app.size` can never index outside the buffer. Add a test that calls `render::draw` at 100x20 and 59x23 with no prior `on_resize` and asserts the too-small notice renders without panicking.

## [MAJOR] TooSmall mode swallows every action, including Quit and Ctrl+C
`src/app.rs`

`App::apply` has `Mode::TooSmall => {}` (src/app.rs:119), so no action is handled at all while the terminal is too small. Ctrl+C maps to `Action::Quit` (src/input.rs:17) but is discarded here, and raw mode means the kernel does not turn it into SIGINT; `SignalFlags` registers only SIGTERM and SIGHUP. A player who shrinks the terminal has no keyboard route out of the process and must kill it from another session. Spec §11 ("Обробляти Ctrl+C як запит на коректне завершення") requires Ctrl+C to be a graceful-shutdown request in every mode.

**Fix:** Handle `Action::Quit` in the `Mode::TooSmall` arm — either set `self.quit = Some(ExitReason::Confirmed)` directly (a confirm dialog cannot be rendered at this size anyway) or transition to `ConfirmQuit` and render the notice plus a one-line quit hint. Add a `mode_machine.rs` test: resize to 40x15, apply `Action::Quit`, assert `app.quit.is_some()`.

## [MAJOR] `--help` and `--version` print to stderr and exit 2
`src/config.rs`

`Config::from_args` maps every `clap` error — including `DisplayHelp` and `DisplayVersion` — to `StartupError { code: 2 }`, and `main.rs:26` prints it with `eprintln!`. Verified: `mosslight --help` → 0 bytes on stdout, help text on stderr, exit 2; `mosslight --version` → exit 2. `--help`/`--version` are part of the §12 CLI surface; convention (and clap's own default) is stdout with exit 0, and `mosslight --help | less` currently shows nothing. `docs/user/cli.md` compounds this by asserting "the same code clap itself uses for a usage error (bad flag, `--help`, `--version`)" — clap exits 0 for the latter two, so the doc states something false.

**Fix:** Match on `e.kind()`: for `ErrorKind::DisplayHelp | DisplayVersion | DisplayHelpOnMissingArgumentOrSubcommand`, print via `e.print()` (which writes to stdout) and exit 0; keep code 2 for genuine usage errors. Add tests asserting `--help` exits 0 with non-empty stdout and empty stderr. Correct the `docs/user/cli.md` startup-refusal paragraph.

## [MAJOR] Acceptance criterion 5's "without ever enabling raw mode" is asserted by a comment, not a test
`tests/terminal_guard.rs`

`preflight_refusal_records_no_terminal_calls` constructs a `Probe`, asserts `err.code == 2` (already covered by `preflight_table`) and then states the actual property in a comment: "No `TerminalOps` impl is ever constructed on this path in `main.rs`". Nothing in the test observes `main.rs`, so it cannot fail if someone moves `TerminalGuard::enter` above `preflight`. Separately, the spawned-binary test in `tests/config.rs` covers only non-TTY stdin — the `TERM=dumb` half of the criterion has no end-to-end test, and neither asserts that stdout carries no terminal-mutating escape sequence.

**Fix:** In `non_tty_stdin_exits_2_with_one_stderr_line`, also assert `output.stdout.is_empty()` (in particular no `\x1b[?1049h`) — that is a real, cheap proof that raw mode/alt screen were never entered. Add a sibling case spawning the binary with `.env("TERM", "dumb")` asserting the same three properties. Delete or rewrite the comment-only test.

## [MAJOR] Loop leaves surplus input events queued, contradicting the documented overflow policy; criterion 9's "no stale replay" is untested
`src/main.rs`

`poll_events` (src/main.rs:176-181) stops reading at `INPUT_EVENTS_PER_ITER * 8` = 256 events and `break`s, leaving the remainder in crossterm's queue to be read on subsequent iterations. PLAN.md and `.autodev/DECISIONS.md` both specify the opposite: "past the 32-event per-iteration cap the remaining immediately-available events are drained and discarded... leaving the surplus queued would let a 200-event burst replay as steps over the following iterations, which is exactly what §5 forbids." The implementation is the rejected alternative, undocumented as a change, so RISKS #6 is only partly mitigated: a >256-event burst (held key over a laggy SSH link plus a stall) does trickle into later iterations as further steps.

The named proof, `tests/input_policy.rs::burst_of_200_moves_yields_one_action`, feeds 200 already-mapped `Action`s into the pure `apply_overflow_policy`; it never involves a queue, so the "nothing left queued afterwards" half of criterion 9 is asserted by no test at all.

**Fix:** Drain to exhaustion in `poll_events` (keep a hard ceiling only as a livelock guard, and count what was discarded into `Diagnostics` as PLAN.md specifies) instead of `break`ing with events still pending. Then make the property testable: extract the read loop over an event-source trait and add a test feeding 500 synthetic key events that asserts the source is empty after one iteration and that only one movement action survives.

## [MINOR] Panic path restores twice, emitting escape sequences after the panic message
`src/main.rs`

`install_panic_hook(restore_raw_terminal)` restores through a fresh `CrosstermOps`, but the live `TerminalGuard` still has `restored == false`, so unwinding runs the whole sequence a second time — after the previous hook printed the panic text. Captured from a real pty run of `--debug-panic`: `...panicked at...\r\nnote: run with RUST_BACKTRACE=1...\r\n\x1b[?25h\x1b[0m\x1b[?1049l`. Invisible in practice, but it is stray output after the diagnostic, which is the ordering §11 cares about, and `tests/terminal_guard.rs::explicit_restore_then_drop_runs_restore_sequence_once` only pins idempotence within one guard, not across the hook and the guard.

**Fix:** Share one `Arc<AtomicBool>` "restored" flag between the panic-hook closure and `TerminalGuard`, so whichever runs first wins and the other becomes a no-op.

## [MINOR] The fps-independence test never varies wall-clock pacing
`tests/loop_timing.rs`

`run_schedule` advances the pacer by exactly one tick interval (1/30 s) per scheduled action regardless of fps, and ignores `Due.draw` entirely. It does fail if someone derived the tick interval from fps, so it is not vacuous, but it does not exercise the scenario criterion 3 describes — the same wall-clock time chopped into 10, 20 and 30 frames per second.

**Fix:** Drive each fps for a fixed wall-clock span (e.g. 60 iterations advancing by `NANOS_PER_SEC / fps` each, injecting the action schedule at fixed elapsed-time marks) and compare `serde_json::to_vec(&app.state)` across the three runs.

## [MINOR] `render::is_too_small` and `terminal::size` are dead code
`src/render/mod.rs`

Neither is called anywhere in `src/` — only their `pub` visibility keeps clippy quiet. They are precisely the two functions the startup-size blocker needs, so this is unfinished wiring rather than speculative API.

**Fix:** Call `terminal::size()` during startup and use `is_too_small(frame.area())` as the render-side guard; both then have a real caller.

## [MINOR] T15 claims the PTY restoration check was run, but no transcript is recorded
`.autodev/phases/01-skeleton-terminal-loop/PLAN.md`

T15 states the script was "Actually run" on this host with specific results (exits 0 and 101, `lflags` byte-identical before/after, restore sequences before the panic text). `TEST_OUTPUT.txt` contains only cargo output; there is no captured transcript anywhere in the diff. CLAUDE.md ("Do not claim an environment was verified unless it was actually run") and RISKS #17 make recorded evidence part of the deliverable, and a future reader cannot distinguish a real run from a plausible one. Note also that expect's default pty is 80x24, so this check could not have caught the startup-size blocker.

**Fix:** Commit the script's captured output (e.g. `.autodev/phases/01-skeleton-terminal-loop/TERMINAL_RESTORE.txt`) alongside the cargo log, and add a small-terminal case (`stty rows 20` inside the expect session) once the size probe lands.

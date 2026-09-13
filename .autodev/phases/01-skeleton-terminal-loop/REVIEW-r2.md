# Review — phase 1 round 2

**Verdict:** changes_requested

Round-2 verdict: changes_requested. All five round-1 blocker/major findings were genuinely fixed and are now backed by tests that can fail: the startup `terminal::size()` probe plus the `is_too_small(frame.area())` render guard (with a test at 100x20 and 59x23), `Quit` handled in `TooSmall`, `--help`/`--version` on stdout with exit 0 (unit + spawned-binary tests), the drain-to-exhaustion `EventSource`/`drain_ready` seam with a 500-event test, and the shared `Arc<AtomicBool>` restore flag. The full gate (`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`) and `cargo build --release --locked` all pass on a clean run here. Structure, purity boundaries, and documentation are in good shape. But the game does not actually play: `main.rs` discards the iteration's simulation actions whenever the pacer reports zero sim steps, which is the normal outcome when a keypress wakes `event::poll` early — verified in a real 80x24 pty, where 4 isolated presses produced no movement at all and 60 rapid presses produced exactly one step. Criterion 2 is therefore unmet. Two supporting gaps let it through: no test drives `main.rs`'s loop wiring (the fps test always advances by exactly one tick), and the committed PTY transcript was captured in a 0x0 pty where the app sat in `TooSmall` and never rendered, so its "normal exit" and "resize to 100x20" labels describe scenarios that did not run.

## [BLOCKER] Movement actions are discarded whenever the loop iteration does not cross a tick boundary — the hero is effectively unable to walk
`src/main.rs`

`run()` recomputes `let sim_actions = app.apply(&actions)` (src/main.rs:137) every iteration and passes it to the simulation only inside `for step in 0..due.sim_steps` (src/main.rs:148-150). `sim_actions` is a local: if `due.sim_steps == 0`, it is dropped on the floor.

That is the normal case for a keypress. The poll deadline is `pacer.next_deadline_ns()` = time remaining until the next 33.3 ms tick, so a key arriving mid-interval returns from `event::poll` early; `pacer.advance(elapsed_ns)` then sees less than one tick interval and yields `sim_steps == 0`. The action is thrown away, and the following iteration ticks the simulation with an empty action slice.

Verified against the release binary in a real pty at 80x24 (`spawn bash -c "stty rows 24 columns 80; exec target/release/mosslight ..."`, TERM=xterm-256color):
- 4 isolated `d` presses, 1 s apart: 0 cursor-positioning writes, 0 `@` glyphs emitted after the initial frame — the hero never moved.
- 60 `d` presses at 20 ms intervals (~1.2 s): exactly 1 cursor move and 1 `@` redraw, i.e. one step where `HERO_STEP_TICKS` would have permitted ~9. An action survives only when its iteration happens to straddle a tick boundary.

This breaks acceptance criterion 2 ("New Game puts a hero in a room that can be walked around with arrows and WASD"). Mode-level actions (Esc, Q, Ctrl+C, menu navigation) are unaffected because they are resolved inside `App::apply`, which is why the committed PTY transcript and every existing test look healthy.

**Fix:** Keep a `pending: Vec<Action>` across iterations in `run()`: `pending.extend(app.apply(&actions)); coalesce(&mut pending);` and consume/clear it only when a simulation step actually runs (`if due.sim_steps > 0 { app.tick(&pending); pending.clear(); ... }`). Coalescing the buffer preserves the one-step-per-tick and no-stale-replay properties. Add a loop-level test (see the next finding) that presses a movement key at a sub-tick offset and asserts the hero moved.

## [MAJOR] No test exercises main.rs's loop wiring, so criterion 2 has no automated proof
`tests/loop_timing.rs`

`run_schedule` (tests/loop_timing.rs) calls `pacer.advance(tick_ns)` once per scheduled action, so `due.sim_steps` is always exactly 1 and the action always reaches `app.tick`. The real loop's timing — poll returns early on a keypress, elapsed time is a fraction of a tick — is never reproduced, which is precisely why the action-dropping blocker above is invisible to the whole suite. Every other test (`mode_machine.rs`, `movement.rs`, `input_policy.rs`) drives `App`/`game::update` directly and never goes through the iteration structure that owns the defect. The plan's own criteria table maps criterion 2 to "the interactive run is the manual check in T15's log" — and that log (next finding) did not exercise it either.

**Fix:** Extract the iteration body of `run()` into a testable function over the `EventSource` seam that already exists (e.g. `fn step_once(app, pacer, source, elapsed_ns) -> ...`), and add a test that: feeds one movement event with `elapsed_ns = tick_interval / 4` for several iterations and asserts the hero's position advances at the cooldown rate; and asserts a keypress delivered in a `sim_steps == 0` iteration is not lost.

## [MAJOR] The committed PTY transcript was captured in a 0x0 pty: all three cases ran in TooSmall, so the labelled scenarios were not the ones exercised
`.autodev/phases/01-skeleton-terminal-loop/TERMINAL_RESTORE.txt`

In the recorded transcript the game emits only `\x1b[?1049h\x1b[?25l ... \x1b[?1049l` with no scene, HUD, menu or too-small text in any of the three cases. I reproduced this: `expect` in this environment spawns a pty with an unset window size, `terminal::size()` returns 0x0, the startup probe puts the app straight into `Mode::TooSmall`, and `render::draw` paints the notice into a zero-area rect (nothing). Adding `stty rows 24 columns 80` before `exec`ing the binary makes the game render normally.

Consequences for the evidence:
- Case 1 "normal exit (Q, then Enter to confirm quit)" actually exited on `Q` alone through `apply_too_small`'s immediate quit; the confirm dialog was never shown, and the trailing `\r` went to an already-dead process. The interactive path criterion 2 describes was not exercised.
- Case 3's `stty rows 20 columns 100` runs against expect's own stdin, not the spawned pty (`$spawn_out(slave,name)`), so the resize never reached the game; it is byte-for-byte the same scenario as case 1.
The restoration evidence itself (identical `lflags` before/after, restore sequences before the panic text) is still valid, so criterion 6 holds — but the scenario labels overstate what ran, which CLAUDE.md explicitly forbids, and it is the reason the movement blocker survived a round of review.

**Fix:** In `scripts/terminal-restore-check.sh`, set the pty size explicitly (`spawn bash -c "stty rows 24 columns 80; exec $BIN ..."`, and for the resize case `stty rows 20 columns 100 < $spawn_out(slave,name)`), and assert the transcript contains drawn content (e.g. the `HP` HUD string or the `Quit?` dialog) so a 0-size pty fails the check instead of passing silently. Re-run and re-commit the transcript; correct T15's claims to match.

## [MINOR] `term_dumb_exits_2_with_one_stderr_line_and_no_stdout` never reaches the TERM=dumb branch
`tests/config.rs`

The test (tests/config.rs:201) spawns with `.stdin(Stdio::null())`, and `terminal::preflight` checks `!p.stdin_tty || !p.stdout_tty` before it ever looks at `TERM` (src/terminal.rs). The refusal is therefore always the TTY message, and the test's three assertions (exit 2, one stderr line, empty stdout) hold identically with `TERM=xterm-256color` — it is a duplicate of `non_tty_stdin_exits_2_with_one_stderr_line` and cannot fail if the `dumb` branch is deleted. The unit-level `preflight_table` does cover the branch, so criterion 5 is not unproven, but this end-to-end half is vacuous.

**Fix:** Assert on the message content: `assert!(stderr.contains("dumb"))`. Since a TTY cannot be handed to a spawned test process, either reorder `preflight` to check `TERM` first, or drop the spawned `dumb` case and keep the assertion at the unit level where the probe can be constructed with `stdin_tty: true`.

## [MINOR] `apply_overflow_policy` keeps the oldest 32 actions, so a large burst applies a stale movement instead of the newest
`src/input.rs`

`apply_overflow_policy` splits at `INPUT_EVENTS_PER_ITER` (src/input.rs:89-90) and keeps `head` — the first 32 actions read — discarding all non-control surplus. `coalesce` then keeps the last movement *of that head*. For a burst larger than 32 events, the movement that is applied is the 32nd-oldest keypress while every newer one, including the direction the player pressed most recently, is thrown away. That inverts the documented intent ("keeps the LAST movement") and is the same stale-input behaviour spec §5 is trying to prevent. `burst_of_200_moves_yields_one_action` cannot catch it because all 200 actions are identical.

**Fix:** Retain the *last* `INPUT_EVENTS_PER_ITER` actions instead of the first (or simply coalesce first and then cap), and change the test to a burst of 200 `MoveNorth` followed by one `MoveSouth`, asserting the surviving movement is `MoveSouth`.

## [MINOR] `App::tick` marks the frame dirty every tick, so a frame is written every fps interval while Playing even when nothing changed
`src/app.rs`

`App::tick` sets `self.dirty = true` unconditionally (src/app.rs:204) whenever `mode == Playing`, so the `due.draw && (app.take_dirty() || any_step)` gate in `run()` is always satisfied while the game is running. Confirmed in the pty transcript: standing still in `Playing` emits `\x1b[39m\x1b[49m\x1b[59m\x1b[0m\x1b[?25l` (~20 bytes) per frame forever — ~400 B/s at the default 20 fps. The payload is tiny thanks to ratatui's diff, but CLAUDE.md states plainly "do not draw when nothing changed", and RISKS #12 is a row this phase claims to mitigate via the dirty flag. `no_draw_is_due_while_idle_with_nothing_dirty` cannot catch it: it stays in `MainMenu`, where `tick` early-returns before touching `dirty`.

**Fix:** Set `dirty` only when the simulation actually changed something — e.g. `let events = update(...); self.dirty |= !events.is_empty();` — and extend the idle test to run in `Playing` with no actions for 100 iterations, asserting zero draws.

## [MINOR] A partial failure in `TerminalGuard::enter` leaves raw mode enabled with no guard to undo it
`src/terminal.rs`

`enter` runs `ops.enable_raw()?; ops.enter_alt()?; ops.hide_cursor()?;` (src/terminal.rs:74-76). If `enter_alt` or `hide_cursor` fails, `?` returns `Err` before the `TerminalGuard` value exists, so nothing owns the already-enabled raw mode and nothing runs `Drop`. `main.rs` then pushes a diagnostic and returns 1, leaving the user's shell in raw mode — the exact failure RISKS #4 is about, on an error path the tests do not cover (`RecordingOps` never returns `Err`).

**Fix:** Construct the guard immediately after `enable_raw` succeeds and perform `enter_alt`/`hide_cursor` through the guard, so an early return unwinds through `Drop`; or wrap the tail in a closure and call `restore()` explicitly before propagating. Add a `RecordingOps` variant that fails on the Nth call and assert `disable_raw` was still recorded.

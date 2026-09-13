# 0007. Terminal state is owned by one RAII guard, backed by a panic hook and flag-based signal handling

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§11 makes terminal restoration a correctness requirement on four distinct exit paths — a normal exit, a
handled error, an unwinding panic, and a signal. On each of them raw mode must be disabled, the alternate
screen left, the cursor shown, and styles reset, **and only then** may any diagnostic be printed. §15 repeats
this as an acceptance criterion: a clean exit must leave a usable terminal.

Ctrl+C must be treated as a graceful shutdown request. SIGTERM and SIGHUP must reach the main loop through a
safe signal mechanism, and file operations must not happen in a signal handler. Write errors after an SSH
drop must be handled, and restoration attempted within whatever connection remains.

§3 additionally requires exiting with a short explanation for a missing TTY or `TERM=dumb` **before** raw
mode is enabled.

The failure this prevents is concrete and unforgiving: a TUI that dies in raw mode leaves the user's shell
with no echo and no line editing. Over SSH, they often cannot even see what they type to fix it.

## Decision

- **`TerminalGuard`** is the only type that mutates terminal state. `TerminalGuard::enter()` enables raw
  mode first (so the guard exists, and can undo it, even if a later step fails), then switches to the
  alternate screen and hides the cursor; if either of those two fails, `enter()` restores immediately and
  returns the error, rather than leaving raw mode enabled with nothing left to undo it. `Drop` reverses
  everything in order and is idempotent. No other module calls `enable_raw_mode`, `EnterAlternateScreen`, or
  touches the cursor.
- **Pre-flight checks run before `enter()`**: if `TERM` is unset/`dumb`, or stdin or stdout is not a TTY,
  print one line to stderr and exit 2. The terminal is never modified in that path (§3).
- **The panic hook is installed separately from `enter()`**, by a plain `install_panic_hook(restored, restore)`
  function `main.rs` calls right after `enter()` succeeds — not by `enter()` itself, so a `RecordingOps`-backed
  guard in tests can be built without touching the process-global hook. The guard and the hook share one
  `Arc<AtomicBool>` "restored" flag (`TerminalGuard::restored_flag()`): whichever of "the guard's `Drop`" or
  "a panic unwinding" runs first flips the flag and performs the restoration; the other observes it already
  flipped and is a no-op. The hook restores first and *then* calls the previous hook to print the panic
  message, so a panic inside a ratatui draw call still leaves a usable terminal, and the terminal is never
  restored a second time after the panic message has already printed.
- **Signals via `signal-hook`**: SIGTERM and SIGHUP are registered with `signal_hook::flag::register`, which
  only sets an `AtomicBool` — async-signal-safe, no allocation, no file I/O in the handler (§11). The main
  loop checks the flag each iteration and exits through the normal path, so restoration and diagnostics
  follow the same code as any other exit. Progress is protected by the last autosave, not by saving from a
  handler.
- **Ctrl+C** does not arrive as SIGINT in raw mode; crossterm delivers it as a key event. `input` maps it to
  the same quit request as `Q`, with the same confirmation.
- **Write errors are fatal but clean**: a failed write to stdout ends the loop, attempts restoration, and
  exits non-zero. It is never retried in a loop, because a dropped SSH connection will not recover.
- **Diagnostics are deferred**: messages collected during the run are buffered and flushed to stderr after
  the guard has dropped, so no log line can ever land on top of the game (§11).
- **SIGKILL and power loss** are explicitly out of scope for cleanup; data safety comes from atomic saves
  (ADR 0006) and the README tells the user `reset` fixes the terminal.

## Alternatives considered

- **Scattered enable/disable calls at each exit point** — rejected: it is the standard way this bug ships.
  Every new `return` or `?` becomes an opportunity to skip restoration.
- **`catch_unwind` around the loop instead of a panic hook** — rejected: it does not cover panics in threads
  or in `Drop`, and it changes the backtrace. A hook plus an idempotent `Drop` covers both directions.
- **`signal_hook::iterator::Signals` on a dedicated thread** — rejected: it contradicts the single-threaded
  design (ADR 0001) for a flag we poll once per 33 ms anyway.
- **Saving the game from inside a signal handler** — rejected explicitly by §11, and unsound: allocation and
  file I/O are not async-signal-safe.
- **`ctrlc` crate** — rejected: it covers only SIGINT, which raw mode already turns into a key event, and it
  would not handle SIGHUP, which is the one that actually fires when an SSH connection drops.

## Consequences

- Terminal restoration is provable by inspection: one type, one `Drop`, one hook.
- It is testable in the way that matters — a deliberate panic behind a hidden `--debug-panic` flag lets the
  manual check in §13 ("correct terminal after a controlled panic") actually be run rather than asserted.
- `signal-hook` is an extra dependency beyond the four §8 names. It is justified by a spec requirement
  crossterm does not cover, and it exposes no crossterm types, so it cannot cause the version skew ADR 0003
  guards against.
- A SIGHUP mid-frame means the last autosave is what survives; that is the designed guarantee, and the
  autosave triggers in §10 are frequent enough that the loss is at most one room transition.
- `Drop` cannot report an error. Restoration failures are best-effort by nature — if the connection is gone,
  nothing can be written anyway.

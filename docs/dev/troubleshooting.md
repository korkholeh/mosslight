# Troubleshooting

## Terminal looks broken after running the game

**Symptom:** no echo, no line editing, or leftover escape codes in the shell after mosslight exits
or is killed.

**Cause:** `TerminalGuard` restores raw mode, the alternate screen, the cursor and text styles on
every exit path it can reach (normal exit, a handled error, a panic, SIGTERM/SIGHUP) — but
`SIGKILL` and a hard power loss give it no chance to run at all (ADR 0007).

**Fix:**

```sh
reset
```

## `scripts/terminal-restore-check.sh` hangs or shows a garbled transcript in a sandboxed/CI-like environment

**Symptom:** the script times out waiting for output, or the captured transcript shows a keystroke
echoed in cleartext instead of driving the game.

**Cause:** this script needs a real PTY with an explicit size. Environments with no controlling
terminal of their own (some sandboxes, some CI runners) hand `expect` a pty with no size at all;
`terminal::size()` then reads `0x0`, the game enters `Mode::TooSmall` immediately, and nothing is
rendered — a transcript captured this way describes a scenario that never actually ran, which is
why the script pins an explicit size (`stty rows 24 columns 80`) before the binary starts rather
than trusting the inherited one. A related race exists even with a sized pty: a keystroke sent
immediately after spawning can land while the shell is still in canonical/echo mode, before
mosslight has enabled raw mode, and gets consumed as ordinary shell input instead of reaching the
program.

**Fix:** confirm the script pins the pty size (`stty rows 24 columns 80`) before the binary starts
and gives the game a settle delay before sending the first key. A parent shell without a
controlling TTY is *not* itself the problem — `expect`'s `spawn` allocates a fresh, sized pty for
the child either way, and all three scripts have been run to completion in exactly such an
environment (`docs/dev/verification-report.md` has the transcripts). An earlier phase-7 report
blamed the sandbox for what turned out to be four bugs in the scripts themselves; if a script hangs
now, suspect the script, not the environment.

`tests/terminal_guard.rs` is the automated, PTY-free proof of the restoration ordering and does not
depend on a real pty at all; the shell script is a manual, host-run confirmation on top of it (see
`docs/dev/testing.md`).

## Game refuses to start with a one-line message and exits with code 2

**Symptom:** `mosslight requires an interactive terminal (stdin/stdout must be a TTY)` or
`mosslight does not support TERM=dumb` on stderr, nothing else printed, exit code 2.

**Cause:** stdin or stdout is not a TTY (e.g. running through a pipe, or in some CI log capture),
or `TERM` is unset or `dumb`. This check runs before raw mode is ever touched, by design (spec §3)
— exit code 2 matches clap's own usage-error convention.

**Fix:** run interactively in a real terminal, or over `ssh -t` (the `-t` flag forces a PTY). This
is not a bug to work around; there is no way to play mosslight without an interactive terminal.

## The game exits immediately with a list of content errors and code 2

**Symptom:** one or more lines like
`room.west_grove: spawn 'spawn.west_grove.east' at (3, 7) is not walkable (Wall)` on stderr,
nothing rendered, exit code 2.

**Cause:** `content::validate` rejected the world before the terminal was ever touched (spec §7,
§15) — either the embedded `assets/world.ron`, or the file passed to the hidden
`--debug-content PATH` flag. This check runs before the TTY/`TERM` preflight, so it fires even
without a real terminal.

**Fix:** read `docs/dev/content.md` for the tile table and the id convention, fix the offending
room/door/spawn in the RON file named by the error, and re-run
`cargo test --locked --test content` to confirm the fix before rebuilding the binary.

## The hero does not seem to move, or only moves once after several key presses

This was a real bug during phase 1 development (fixed before release): the main loop discarded a
keypress whenever the current iteration did not cross a simulation tick boundary, which is the
common case. If you see this again after a code change to `src/main.rs` or `src/app.rs`, check that
`advance_iteration`'s `pending` buffer is still being carried across loop iterations rather than
recomputed as a local each time — see `docs/dev/loop-and-modes.md` and
`tests/loop_timing.rs::a_keypress_delivered_when_no_sim_step_is_due_is_not_lost`.

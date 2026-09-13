#!/usr/bin/env bash
# Runs the release binary under a real PTY for the normal-exit, --debug-panic and live-resize
# paths, printing `stty -a`'s line-discipline flags before and after each run so the actual
# terminal state can be read back (spec §11, §13). Requires `expect` (present by default on macOS
# and most Linux distributions); if it is missing, this script says so and exits rather than
# skipping silently.
#
# Round-2 review: the previous version of this script did not set the spawned pty's size, so in
# some sandboxed environments `expect` allocates a 0x0 pty, `terminal::size()` reads 0x0, the game
# starts straight into `Mode::TooSmall` and renders nothing — making every case below silently
# exercise the too-small path instead of the scenario its label claims. Every case now sets an
# explicit pty size before the binary starts, and the transcript is grepped for content that could
# only have been drawn by the scenario it claims to run, so a misconfigured pty fails the check
# instead of passing with an empty transcript.
#
# Also while fixing the above: a plain Tcl `sleep` between `send`s never reads the spawned
# process's output, and expect's own default `match_max` (2000 bytes) is smaller than one rendered
# frame here (the scene draw alone is several KB of escape codes). Between the two, a case that
# `sleep`s across more than one rendered frame can let the child block on a full pty write buffer
# forever — it never gets back around to reading the keystroke already sitting in its input queue.
# Confirmed directly against a `pty.fork()`-driven harness outside `expect` entirely: a `Q` sent
# without ever draining output first reproducibly hangs (0% CPU, blocked in `write`, not a busy
# loop), while the same `Q` sent after draining the backlog exits cleanly every time. `match_max`
# is raised and every wait is now a `drain_for` that keeps reading while it waits, so output never
# backs up in the first place.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v expect >/dev/null 2>&1; then
    echo "expect(1) not found: cannot drive a real PTY session without it. Skipping." >&2
    exit 1
fi

cargo build --release --locked
BIN="$(pwd)/target/release/mosslight"
SAVE_DIR="$(mktemp -d)"
trap 'rm -rf "$SAVE_DIR"' EXIT

FAILED=0

# Runs one case, printing its transcript, then asserts every string in `needles` (space-separated,
# one word each — no spaces in a needle) appears somewhere in the captured output.
run_case() {
    local label="$1"
    local extra_args="$2"
    local keystrokes="$3" # expect `send`/`stty` commands, or empty for none
    shift 3
    local needles=("$@")

    echo "=== $label ==="
    local out
    out="$(expect <<EOF
set timeout 10
match_max 1000000
proc drain_for {secs} {
    global timeout
    set old \$timeout
    set timeout \$secs
    expect {
        eof { }
        timeout { }
    }
    set timeout \$old
}
spawn bash -c "stty rows 24 columns 80; stty -a | sed -n 2p; $BIN --save-dir $SAVE_DIR $extra_args; echo TERMINAL_RESTORE_CHECK_EXIT:\\\$?; stty -a | sed -n 2p"
drain_for 0.5
$keystrokes
expect eof
EOF
)"
    echo "$out"
    echo

    for needle in "${needles[@]}"; do
        if [[ "$out" != *"$needle"* ]]; then
            echo "FAIL: expected transcript for '$label' to contain '$needle' but it did not." >&2
            FAILED=1
        fi
    done
}

echo "Terminal restoration check ($(uname -s) $(uname -m)), run on $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo

run_case "normal exit (Q, then Enter to confirm quit)" "" 'send "q"
drain_for 0.5
send "\r"
drain_for 0.5
send "\r"
drain_for 0.5
send "q\r"' "HP" "Quit?" "TERMINAL_RESTORE_CHECK_EXIT:0"

run_case "controlled panic (--debug-panic)" "--debug-panic" "" "panicked" "TERMINAL_RESTORE_CHECK_EXIT:101"

run_case "too-small terminal via resize (100x20, then Q to quit with no confirm dialog)" "" 'stty rows 20 columns 100 < $spawn_out(slave,name)
drain_for 0.5
send "q"
drain_for 0.5
send "q"
drain_for 0.5
send "q"' "HP" "60x24" "TERMINAL_RESTORE_CHECK_EXIT:0"

echo "Not covered by this script: a terminal that is already too small at startup, before any"
echo "resize event (this script always sets an explicit starting pty size, and this project builds"
echo "no PTY harness that could spawn one pre-sized below 60x24). That startup path is covered"
echo "instead by the headless test"
echo "tests/render.rs::draw_never_panics_on_a_too_small_frame_even_without_a_prior_resize, and by"
echo "the explicit terminal::size() probe main.rs now runs before the loop starts."
echo "Also not covered by this script (needs a human at the keyboard or a fault-injection harness"
echo "this project does not build): a genuine post-disconnect write-error exit path. Record"
echo "that gap in the phase report rather than claiming it was verified."

if [[ "$FAILED" -ne 0 ]]; then
    echo "One or more cases did not render what their label claims. See FAIL lines above." >&2
    exit 1
fi

#!/usr/bin/env bash
# Measures CPU and RSS while playing and while paused (spec §13's "measured CPU", RISKS-adjacent):
# `expect` spawns the release binary on a sized pty, sends a scripted stream of key presses for
# 60s of play then 60s of pause, while a background `ps -o %cpu=,rss=` sampler runs once a second.
# Prints every sample, the mean and max, and the host environment, so the transcript itself carries
# the numbers copied into docs/dev/verification-report.md. Not wired into `cargo test` or CI — a
# measurement, not a gate (CLAUDE.md/PLAN.md Design §4).
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v expect >/dev/null 2>&1; then
    echo "expect(1) not found: cannot drive a real PTY session without it. Skipping." >&2
    exit 1
fi

cargo build --release --locked
BIN="$(pwd)/target/release/mosslight"
SAVE_DIR="$(mktemp -d)"
SAMPLES_FILE="$(mktemp)"
# Written by the expect block below the instant it sends Esc, so the sampling loop can split
# samples into a play window and a pause window instead of averaging both into one meaningless
# number (spec §13 asks for CPU "за хвилину гри та паузи" — play minute and pause minute,
# separately; see DECISIONS.md phase-07/review-r1).
PAUSE_MARKER="$(mktemp -u)"
# The wrapper's own pid, written by the expect block right after spawn. `bash -c "stty ...; $BIN
# ...; echo ..."` runs all three commands under one shell without exec'ing any of them, so the
# binary is a direct child of this pid for its whole life — sampling `pgrep -P` against it finds
# only the game, never the wrapper itself. A plain `pgrep -f "$BIN --save-dir $SAVE_DIR"` also
# matches that wrapper's own command line (it contains the same string as an argument to `-c`), and
# picking "-n" (newest) to dodge it is not something the sampler can control (see
# DECISIONS.md phase-07/review-r2).
WRAPPER_PID_FILE="$(mktemp -u)"
trap 'rm -rf "$SAVE_DIR"; rm -f "$SAMPLES_FILE" "$PAUSE_MARKER" "$WRAPPER_PID_FILE"' EXIT

echo "Environment: $(uname -a)"
if command -v sw_vers >/dev/null 2>&1; then
    sw_vers
fi
rustc -V
echo

expect <<EOF &
set timeout 150
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
spawn bash -c "stty rows 24 columns 80; $BIN --save-dir $SAVE_DIR; echo TERMINAL_RESTORE_CHECK_EXIT:\\\$?"
set f [open "$WRAPPER_PID_FILE" w]
puts \$f [exp_pid]
close \$f
drain_for 0.5
send "\r"
drain_for 0.5
# 60s of active play: a short repeating movement/attack burst.
for {set i 0} {\$i < 30} {incr i} {
    send "wjasd"
    drain_for 2
}
# 60s of pause: Esc, then nothing. Mark the moment Esc is sent so the sampler can split its
# samples into a play window and a pause window.
send "\x1b"
exec date +%s > $PAUSE_MARKER
drain_for 60
send "q"
drain_for 0.5
send "\r"
expect eof
EOF
EXPECT_PID=$!
PLAY_START="$(date +%s)"

echo "sample_epoch_s pct_cpu rss_kb" > "$SAMPLES_FILE"
for _ in $(seq 1 130); do
    PID=""
    if [[ -s "$WRAPPER_PID_FILE" ]]; then
        WRAPPER_PID="$(cat "$WRAPPER_PID_FILE")"
        PID="$(pgrep -P "$WRAPPER_PID" -f "$BIN --save-dir $SAVE_DIR" || true)"
    fi
    if [[ -n "$PID" ]]; then
        SAMPLE="$(ps -o %cpu=,rss= -p "$PID" 2>/dev/null || true)"
        if [[ -n "$SAMPLE" ]]; then
            echo "$(date +%s) $SAMPLE" >> "$SAMPLES_FILE"
        fi
    fi
    sleep 1
    if ! kill -0 "$EXPECT_PID" 2>/dev/null; then
        break
    fi
done
wait "$EXPECT_PID" 2>/dev/null || true

echo
echo "=== samples ==="
cat "$SAMPLES_FILE"

PAUSE_START="$(cat "$PAUSE_MARKER" 2>/dev/null || true)"
echo
if [[ -z "$PAUSE_START" ]]; then
    echo "=== summary (undifferentiated: the pause marker was never written) ==="
    awk 'NR>1 {cpu+=$2; if($2>maxcpu) maxcpu=$2; rss+=$3; if($3>maxrss) maxrss=$3; n++} END {
        if (n>0) {
            printf "mean cpu: %.2f%%  max cpu: %.2f%%\n", cpu/n, maxcpu
            printf "mean rss: %d KB  max rss: %d KB\n", rss/n, maxrss
        } else {
            print "no samples captured"
        }
    }' "$SAMPLES_FILE"
else
    echo "=== summary: play window ($PLAY_START to $PAUSE_START) ==="
    awk -v pause_start="$PAUSE_START" 'NR>1 && $1<pause_start {cpu+=$2; if($2>maxcpu) maxcpu=$2; rss+=$3; if($3>maxrss) maxrss=$3; n++} END {
        if (n>0) {
            printf "samples: %d\n", n
            printf "mean cpu: %.2f%%  max cpu: %.2f%%\n", cpu/n, maxcpu
            printf "mean rss: %d KB  max rss: %d KB\n", rss/n, maxrss
        } else {
            print "no samples captured in the play window"
        }
    }' "$SAMPLES_FILE"
    echo
    echo "Note: on macOS, 'ps -o %cpu' reports a decaying average over roughly the preceding"
    echo "minute of real time, so the first pause-window samples can still carry over some of the"
    echo "play-window load — a non-zero pause mean is not automatically a regression."
    echo "=== summary: pause window ($PAUSE_START onward) ==="
    awk -v pause_start="$PAUSE_START" 'NR>1 && $1>=pause_start {cpu+=$2; if($2>maxcpu) maxcpu=$2; rss+=$3; if($3>maxrss) maxrss=$3; n++} END {
        if (n>0) {
            printf "samples: %d\n", n
            printf "mean cpu: %.2f%%  max cpu: %.2f%%\n", cpu/n, maxcpu
            printf "mean rss: %d KB  max rss: %d KB\n", rss/n, maxrss
        } else {
            print "no samples captured in the pause window"
        }
    }' "$SAMPLES_FILE"
fi

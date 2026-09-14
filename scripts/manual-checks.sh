#!/usr/bin/env bash
# One case per spec §13 manual-check line that is reachable on this host, following
# `terminal-restore-check.sh`'s rule: set the pty size explicitly, drain output while waiting, and
# grep the transcript for a string only that scenario could produce, so a broken case fails loudly
# instead of passing on an empty transcript. Every case runs with its own scratch --save-dir.
# Cases this host cannot reach (Linux, a real networked SSH session, ~150ms RTT, Intel macOS,
# aarch64 Linux) are not faked — see the SKIPPED lines below and docs/dev/verification-report.md.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v expect >/dev/null 2>&1; then
    echo "expect(1) not found: cannot drive a real PTY session without it. Skipping." >&2
    exit 1
fi

cargo build --release --locked
BIN="$(pwd)/target/release/mosslight"

FAILED=0

# run_case LABEL WIDTH HEIGHT EXTRA_ARGS SAVE_DIR KEYSTROKES NEEDLE...
run_case() {
    local label="$1" width="$2" height="$3" extra_args="$4" save_dir="$5" keystrokes="$6"
    shift 6
    local needles=("$@")

    echo "=== $label ==="
    local out
    out="$(expect <<EOF
set timeout 15
match_max 1000000
proc drain_for {secs} {
    global timeout
    set old \$timeout
    set timeout \$secs
    expect { eof { } timeout { } }
    set timeout \$old
}
spawn bash -c "stty rows $height columns $width; $BIN --save-dir $save_dir $extra_args; echo MANUAL_CHECK_EXIT:\\\$?"
drain_for 1
$keystrokes
expect eof
EOF
)"
    echo "$out"
    echo

    # ratatui writes only the changed cells of each frame, so a cursor-move escape can land in the
    # middle of an unchanged word and split it across two writes; a literal substring grep then
    # misses text that is genuinely on screen. Strip CSI sequences and carriage returns before
    # matching so needles are checked against what a human actually reads, not the wire bytes (see
    # DECISIONS.md phase-07/review-r2).
    local normalized
    normalized="$(printf '%s' "$out" | LC_ALL=C sed -E $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g; s/\r//g')"

    for needle in "${needles[@]}"; do
        if [[ "$normalized" != *"$needle"* ]]; then
            echo "FAIL: expected transcript for '$label' to contain '$needle' but it did not." >&2
            FAILED=1
        fi
    done
}

echo "Manual checks ($(uname -s) $(uname -m)), run on $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo

SD1="$(mktemp -d)"
run_case "Local run, 80x24" 80 24 "" "$SD1" \
    'send "\r"
drain_for 1
send "w"
drain_for 1
send "q"
drain_for 0.5
send "\r"' "HP" "pause/back"

SD2="$(mktemp -d)"
run_case "Local run, 60x24 (minimum size)" 60 24 "" "$SD2" \
    'send "\r"
drain_for 1
send "q"
drain_for 0.5
send "\r"' "HP" "pause/back"

SD3="$(mktemp -d)"
# `q` maps to Action::Quit in every mode (src/input.rs) and Mode::TooSmall's handler quits
# immediately without a confirm step (src/app.rs `apply_too_small`), so this exits deterministically
# instead of running until expect's timeout — a prior version left this case's child orphaned at
# ~100% CPU because nothing ever quit it (see DECISIONS.md phase-07/review-r2).
run_case "Below minimum (59x24)" 59 24 "" "$SD3" \
    'drain_for 1
send "q"
drain_for 0.5' "60x24" "59x24"

SD4="$(mktemp -d)"
# `$spawn_out(slave,name)` here must stay unescaped: it reaches expect only through `$keystrokes`'s
# variable substitution inside `run_case`'s heredoc, and a substituted value's bytes are never
# rescanned for further `$`/`\` processing by bash — a `\$` written at the call site would arrive at
# Tcl still carrying that backslash and be read as a literal, unsubstituted `$` (verified directly:
# escaping this line makes `stty` fail with "couldn't open $spawn_out(slave,name)"). The `\$timeout`
# forms elsewhere in this file are different: those are typed straight into the heredoc body, where
# bash's single expansion pass over literal text does consume the backslash.
run_case "Live resize: 80x24 -> 59x24 -> back, then Paused on recovery" 80 24 "" "$SD4" \
    'send "\r"
drain_for 1
stty rows 24 columns 59 < $spawn_out(slave,name)
drain_for 1
stty rows 24 columns 80 < $spawn_out(slave,name)
drain_for 1
send "q"
drain_for 0.5
send "\r"' "60x24" "59x24" "Paused"

SD5="$(mktemp -d)"
run_case "Monochrome: --theme mono --color never (glyphs present, no SGR colour)" 80 24 \
    "--theme mono --color never" "$SD5" \
    'send "\r"
drain_for 1
send "q"
drain_for 0.5
send "\r"'
# The generic run_case above only checks for substrings, not absence — re-run with a direct
# capture so we can also assert no `\x1b[` SGR colour sequence appears.
MONO_OUT="$(expect <<EOF
set timeout 15
match_max 1000000
proc drain_for {secs} { global timeout; set old \$timeout; set timeout \$secs; expect { eof {} timeout {} }; set timeout \$old }
spawn bash -c "stty rows 24 columns 80; $BIN --save-dir $SD5 --theme mono --color never; echo MANUAL_CHECK_EXIT:\\\$?"
drain_for 0.5
send "\r"
drain_for 1
send "q"
drain_for 0.5
send "\r"
expect eof
EOF
)"
echo "=== Monochrome: no SGR colour sequence check ==="
# Grep for actual colour SGR parameters (30-38, 40-48, 90-97, or an indexed/truecolour SGR), not
# just any ESC-bracket-...-m: the guard's teardown always writes the unconditional `\x1b[0m` reset
# even under `--color never` (src/terminal.rs), and ranges that reach 39/49 catch *default*
# foreground/background resets, which the game also emits unconditionally on teardown and which are
# not colour at all — see DECISIONS.md phase-07/review-r1 and phase-07/review-r2 (round 1 excluded
# only `0m`; round 2 found 39/49 slipping through the same net).
ESC=$'\x1b'
# grep exits 1 on zero matches — the outcome a correct monochrome build actually produces — and
# `set -o pipefail` would otherwise turn that into an immediate script abort before OK/FAIL can be
# printed, silently truncating every case after this one. `|| true` keeps a clean run from crashing
# the script that is supposed to report it.
COLOUR_SGR_COUNT="$(printf '%s' "$MONO_OUT" \
    | { LC_ALL=C grep -Eo "${ESC}\[[0-9;]*(3[0-8]|4[0-8]|9[0-7]|10[0-7])m|${ESC}\[38;5;|${ESC}\[48;5;" || true; } \
    | wc -l | tr -d ' ')"
if [[ "$COLOUR_SGR_COUNT" -ne 0 ]]; then
    echo "FAIL: monochrome transcript contains $COLOUR_SGR_COUNT SGR colour escape sequence(s)." >&2
    FAILED=1
else
    echo "OK: no SGR colour escape sequence found."
fi
echo

SD6="$(mktemp -d)"
# Unlike the other cases, this one has no needle that only a passing run could produce — a hero that
# replayed all 40 keys still renders "HP" — so it is visual inspection only, not an automated pass;
# `tests/loop_timing.rs` already proves the §5 no-backlog property headlessly. See DECISIONS.md
# phase-07/review-r1.
run_case "Held key: 40 movement keys in one burst (§5 no-backlog rule) — visual inspection only" \
    80 24 "" "$SD6" \
    'send "\r"
drain_for 1
send [string repeat "w" 40]
drain_for 2
send "q"
drain_for 0.5
send "\r"' "HP"
echo "Read the transcript above: the hero's position after the 40-key burst should reflect far"
echo "fewer than 40 tiles of movement (one step per HERO_STEP_TICKS, not one per key). This case is"
echo "not auto-verified — it can only fail loudly on a rendering crash, not on a backlog regression."
echo

SD7="$(mktemp -d)"
run_case "Save / quit / continue" 80 24 "" "$SD7" \
    'send "\r"
drain_for 1
send "w"
drain_for 1
send "\x1b"
drain_for 1
send "\r"
drain_for 1
send "q"
drain_for 0.5
send "\r"' "HP"
echo "--- relaunching with the same --save-dir to check Continue ---"
# From MainMenu the cursor starts on NewGame; MoveNorth ("w") moves it to Continue
# (MenuCursor::next/prev in src/app.rs), then Confirm ("\r") calls continue_from_menu, which loads
# the save and resumes in the saved room. "m" then opens the map overlay, which is the one screen
# that renders the current room's name (src/render/overlays.rs draw_map) — the HUD never does.
CONTINUE_OUT="$(expect <<EOF
set timeout 15
match_max 1000000
proc drain_for {secs} { global timeout; set old \$timeout; set timeout \$secs; expect { eof {} timeout {} }; set timeout \$old }
spawn bash -c "stty rows 24 columns 80; $BIN --save-dir $SD7; echo MANUAL_CHECK_EXIT:\\\$?"
drain_for 1
send "w"
drain_for 0.5
send "\r"
drain_for 1
send "m"
drain_for 1
send "q"
drain_for 0.5
send "\r"
expect eof
EOF
)"
echo "$CONTINUE_OUT"
CONTINUE_NORMALIZED="$(printf '%s' "$CONTINUE_OUT" | LC_ALL=C sed -E $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g; s/\r//g')"
if [[ "$CONTINUE_NORMALIZED" != *"Continue"* || "$CONTINUE_NORMALIZED" == *"no save yet"* ]]; then
    echo "FAIL: relaunch transcript does not show a present-save Continue item." >&2
    FAILED=1
fi
if [[ "$CONTINUE_NORMALIZED" != *"Lighthouse"* ]]; then
    echo "FAIL: relaunch transcript does not show the restored room's name (Lighthouse)." >&2
    FAILED=1
fi
echo

# Both stty comparisons below sample `stty -a` *inside the spawned pty*, before and after the
# binary runs in the same `expect` transcript — like `terminal-restore-check.sh` does. Sampling the
# outer shell's own `stty -a` (the previous version of this script) measures a tty the game never
# touches: `expect`'s `spawn` allocates a brand-new pty for the child, so an outer-shell BEFORE/AFTER
# pair is identical no matter what the game does and the check can never fail. See DECISIONS.md
# phase-07/review-r1. The `stty rows/columns` prologue also guards against a 0x0 pty silently
# exercising `Mode::TooSmall` instead of the labelled scenario.
SD8="$(mktemp -d)"
echo "=== Terminal after normal exit ==="
NORMAL_EXIT_OUT="$(expect <<EOF
set timeout 15
match_max 1000000
proc drain_for {secs} {
    global timeout
    set old \$timeout
    set timeout \$secs
    expect { eof { } timeout { } }
    set timeout \$old
}
spawn bash -c "stty rows 24 columns 80; stty -a | sed -n 2p; $BIN --save-dir $SD8; echo MANUAL_CHECK_EXIT:\\\$?; stty -a | sed -n 2p"
drain_for 0.5
send "\r"
drain_for 1
send "q"
drain_for 0.5
send "\r"
expect eof
EOF
)"
echo "$NORMAL_EXIT_OUT"
STTY_LINES=()
while IFS= read -r line; do
    STTY_LINES+=("$line")
done < <(printf '%s\n' "$NORMAL_EXIT_OUT" | grep '^lflags:')
LAST_IDX=$((${#STTY_LINES[@]} - 1))
if [[ "${#STTY_LINES[@]}" -lt 2 ]]; then
    echo "FAIL: expected two 'lflags:' stty lines (before and after) in the transcript, found ${#STTY_LINES[@]}." >&2
    FAILED=1
elif [[ "${STTY_LINES[0]}" != "${STTY_LINES[$LAST_IDX]}" ]]; then
    echo "FAIL: stty -a line-discipline flags differ before/after a normal exit." >&2
    echo "before: ${STTY_LINES[0]}" >&2
    echo "after:  ${STTY_LINES[$LAST_IDX]}" >&2
    FAILED=1
else
    echo "OK: stty -a line-discipline flags unchanged (${STTY_LINES[0]})."
fi
echo

SD9="$(mktemp -d)"
echo "=== Terminal after a controlled panic (--debug-panic) ==="
PANIC_OUT="$(expect <<EOF
set timeout 15
match_max 1000000
spawn bash -c "stty rows 24 columns 80; stty -a | sed -n 2p; $BIN --save-dir $SD9 --debug-panic 2>&1; echo MANUAL_CHECK_EXIT:\\\$?; stty -a | sed -n 2p"
expect eof
EOF
)"
echo "$PANIC_OUT"
STTY_LINES=()
while IFS= read -r line; do
    STTY_LINES+=("$line")
done < <(printf '%s\n' "$PANIC_OUT" | grep '^lflags:')
LAST_IDX=$((${#STTY_LINES[@]} - 1))
if [[ "${#STTY_LINES[@]}" -lt 2 ]]; then
    echo "FAIL: expected two 'lflags:' stty lines (before and after) in the transcript, found ${#STTY_LINES[@]}." >&2
    FAILED=1
elif [[ "${STTY_LINES[0]}" != "${STTY_LINES[$LAST_IDX]}" ]]; then
    echo "FAIL: stty -a line-discipline flags differ before/after a controlled panic." >&2
    echo "before: ${STTY_LINES[0]}" >&2
    echo "after:  ${STTY_LINES[$LAST_IDX]}" >&2
    FAILED=1
else
    echo "OK: stty -a line-discipline flags unchanged (${STTY_LINES[0]})."
fi
if [[ "$PANIC_OUT" != *"panicked"* ]]; then
    echo "FAIL: no panic message found on stderr." >&2
    FAILED=1
fi
echo

echo "=== tmux ==="
if command -v tmux >/dev/null 2>&1; then
    SD10="$(mktemp -d)"
    SESSION="mosslight-manual-check-$$"
    tmux new-session -d -s "$SESSION" -x 80 -y 24 \
        "$BIN --save-dir $SD10"
    sleep 1
    tmux send-keys -t "$SESSION" Enter
    sleep 1
    PANE="$(tmux capture-pane -t "$SESSION" -p)"
    tmux send-keys -t "$SESSION" q
    sleep 1
    tmux send-keys -t "$SESSION" Enter
    sleep 1
    tmux kill-session -t "$SESSION" 2>/dev/null || true
    echo "$PANE"
    if [[ "$PANE" != *"HP"* ]]; then
        echo "FAIL: tmux captured pane does not show the scene." >&2
        FAILED=1
    fi
else
    echo "SKIPPED (no such environment on this host): tmux is not installed."
fi
echo

echo "Not reachable on this host and therefore not faked (printed here, not silently skipped):"
echo "  SKIPPED (no such environment on this host): Linux"
echo "  SKIPPED (no such environment on this host): a real SSH session over a network"
echo "  SKIPPED (no such environment on this host): ~150ms RTT playability check"
echo "  SKIPPED (no such environment on this host): Intel macOS"
echo "  SKIPPED (no such environment on this host): aarch64 Linux"

if [[ "$FAILED" -ne 0 ]]; then
    echo "One or more cases did not show what their label claims. See FAIL lines above." >&2
    exit 1
fi

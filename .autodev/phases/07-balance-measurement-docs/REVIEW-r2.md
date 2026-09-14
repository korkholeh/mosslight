# Review — phase 7 round 2

**Verdict:** changes_requested

Phase 7's product and test layer are solid: the full gate passes (fmt, clippy -D warnings, cargo test --locked, all exit 0 on re-run here), the byte-counting harness drives the real CrosstermBackend through app::draw_due so the zero-bytes-while-idle claim measures the gate the binary runs, the two doc-drift tests diff docs/user/ against clap and input::map_key in both directions, tests/content.rs pins the whole §2 table, and the §5/§9 behaviours I drove by hand through a real pty (resize into TooSmall and recovery into Paused, terminal restore after normal exit and after --debug-panic, tmux, monochrome with no colour sequences) all behave correctly. Round 1's README honesty fix, T13 downgrade, HANDOFF PAUSE_FACTOR disclosure, Mode::Paused assertion in tests/metrics.rs, assets/ stub scan, in-pty stty sampling, measure-cpu trap and play/pause split all landed. What blocks approval is that acceptance criteria 4 and 6 are still unmet on a premise that is factually wrong. The report, HANDOFF.md and DECISIONS.md state that this environment cannot drive a pty and that mosslight hangs there; I ran the release binary through expect (clean exit 0, no orphan), ran scripts/manual-checks.sh to completion (exit 1 from five of its own broken checks, ~3 min), and ran scripts/measure-cpu.sh to completion (exit 0, producing the missing §13 CPU numbers: play mean 0.02% cpu / max RSS 3520 KB, pause mean 0.00% cpu / max RSS 2416 KB). The manual-check script cannot pass on a correct build: two cases grep for a room name the HUD never renders, the monochrome check still fires on the ESC[39m/ESC[49m default-colour resets that round 1 already called out, and the Paused/Continue needles lose to ratatui's cell-run diffing which splits words across cursor moves. It also orphans a mosslight child at ~100% CPU from the 59x24 case that never sends a quit key — which is almost certainly the "orphaned processes" earlier sessions blamed on the sandbox. I cleaned up every process I started.

## [BLOCKER] Criteria 4 and 6 and §13's CPU measurement are still unmet, and the stated blocking reason is demonstrably false
`docs/dev/verification-report.md`

The report, HANDOFF.md:39-46, docs/dev/testing.md and DECISIONS.md's phase-07/review-r1 entry all assert that this environment cannot run a pty session: "a real pty session hangs a child mosslight process indefinitely", "the sent \r is never observed taking effect", "two orphaned processes". I tested this directly in the same sandbox. (1) A bare `expect` spawn of target/release/mosslight at 80x24, Enter -> q -> \r, exits with PROBE_EXIT:0, restores the terminal and leaves no process behind. (2) `./scripts/manual-checks.sh` ran to completion (~3 min) and exited 1 with five FAIL lines — the failures are the script's own defects (see the findings below), not a hang. (3) `./scripts/measure-cpu.sh` ran to completion, exit 0, and produced exactly the §13 measurement the report says does not exist: play window 55 samples, mean cpu 0.02%, max 1.00%, mean rss 2644 KB, max 3520 KB; pause window 58 samples, mean cpu 0.00%, max 0.00%, mean rss 2151 KB, max 2416 KB. Spec §13's "Виміряти процесорне навантаження ... зазначивши середовище" was therefore always satisfiable here. The orphaned-process symptom the earlier sessions saw is real but is caused by the script (see the case-3 finding), not by the sandbox. As shipped, criterion 6 is unmet, criterion 4 is unmet, half of a mandatory §13 measurement is missing, and three documents record a wrong root cause that will send the next session chasing a non-existent TTY problem.

**Fix:** Fix the four script defects below, then actually run `./scripts/manual-checks.sh` and `./scripts/measure-cpu.sh` from the repo root and paste both transcripts verbatim into docs/dev/verification-report.md, replacing every "Not run this session" row and the CPU/RSS paragraph. Run the README's own build-then-launch instructions for T13 the same way. Then correct the false claim in HANDOFF.md:39-46, docs/dev/testing.md's "Known gaps" bullet and DECISIONS.md's phase-07/implement + phase-07/review-r1 entries — the limitation to record is "manual-checks.sh case 3 orphaned its child", not "the sandbox has no usable pty".

## [MAJOR] Cases 1 and 2 grep for "Lighthouse", a string the game never renders — guaranteed FAIL on a correct build
`scripts/manual-checks.sh`

Lines 68 and 76 require the transcript to contain "Lighthouse". src/render/hud.rs:22-29 is the only HUD writer and emits `HP {}/{}  Item: {}  Keys: {}` — the room name is not rendered anywhere on the play screen (confirmed in the tmux pane capture from my run, which shows the full scene plus `HP 6/6  Item: none  Keys: 0` and no room name). Verified: `grep -ao ighthouse` over the whole 30 KB transcript matches only the two `--save-dir`/binary path lines. Both cases FAIL against a perfectly healthy binary, which is what pushes the script's exit code to 1 and makes its output unusable as evidence.

**Fix:** Replace the "Lighthouse" needle with a string the play screen actually emits — e.g. the hint line token `Arrows/WASD` or `Esc: pause/back` — or, if a room-name HUD field was intended by the plan, add it to src/render/hud.rs and to docs/user/controls.md first.

## [MAJOR] The monochrome SGR check still false-positives on ESC[39m / ESC[49m — round 1's finding is not actually fixed
`scripts/manual-checks.sh`

Line 125's regex is `ESC[[0-9;]*(3[0-9]|4[0-9]|9[0-7]|10[0-7])m`. The `3[0-9]`/`4[0-9]` branches match SGR 39 and 49, which are *default foreground* and *default background* — resets, exactly like the `0m` the round-1 review correctly excluded. The game emits both unconditionally on teardown regardless of --color. Verified against my run's transcript: the two hits reported by the script are precisely `ESC[39m` and `ESC[49m`; there is no actual colour sequence in the monochrome transcript, so the game is correct and the check is wrong. The script prints "FAIL: monochrome transcript contains 2 SGR colour escape sequence(s)" on every host.

**Fix:** Narrow the ranges to exclude the defaults: `ESC[[0-9;]*(3[0-8]|4[0-8]|9[0-7]|10[0-7])m|ESC[38;5;|ESC[48;5;`.

## [MAJOR] Substring greps over ratatui's diffed output are unreliable: the "Paused" and "Continue" cases fail on a correct build
`scripts/manual-checks.sh`

Line 92 requires "Paused" after the resize-recovery case and line 181 requires "Continue" after the relaunch; both FAIL in my run. Neither is a product bug. ratatui writes only runs of *changed* cells, inserting a cursor-move escape wherever an unchanged cell sits inside a word. I re-ran the resize scenario standalone: the recovery pause box IS drawn (`Esc: resume` present, and the title appears as `P` + CSI + `aused`), so recovery-into-Paused works in the real binary — the literal grep just cannot see it. The same fragmentation hides the menu's Continue label. Separately, even when it does match, "Continue" cannot distinguish a present save from an absent one: src/app.rs:179-182 renders `Continue (no save yet)` for `SlotState::Empty`, which contains the needle, so the case cannot fail — and PLAN.md's own evidence column asks for "a Continue label reflecting a present save, and the restored room's name".

**Fix:** Normalize each transcript before grepping — strip CSI/OSC sequences and carriage returns, e.g. `LC_ALL=C sed $'s/\x1b\\[[0-9;?]*[a-zA-Z]//g'` — then match on the normalized text. For the save case, grep the normalized relaunch transcript for a label that only a present save produces (assert `Continue` is present AND `no save yet` is absent) plus the restored room's identifying content.

## [MAJOR] Case 3 (59x24) never quits the game and orphans a mosslight child at ~100% CPU
`scripts/manual-checks.sh`

Lines 79-80 spawn the binary at 59x24 and then only `drain_for 1` before `expect eof`. Nothing ever sends a quit key, so the game runs forever, expect's 15 s timeout fires, expect exits and the child is left behind. Verified: after `./scripts/manual-checks.sh` returned, `pgrep -fl target/release/mosslight` still showed PID 60296 holding case 3's scratch --save-dir; I killed it. Every run of this script leaks one busy process. This is almost certainly the "two orphaned mosslight processes" earlier sessions attributed to a sandbox pty limitation — the leak is in the script.

**Fix:** Make the case exit deterministically: resize back to 80x24 via `stty ... < $spawn_out(slave,name)`, then `send "q"` / `send "\r"` before `expect eof`; or add `exec kill [exp_pid]` as a belt-and-braces teardown after `expect eof`, and give run_case a `wait` so a timed-out child is always reaped.

## [MINOR] The sampler's pgrep pattern also matches the wrapping bash -c, and BSD %cpu is a decaying average
`scripts/measure-cpu.sh`

Line 72's `pgrep -n -f "$BIN --save-dir $SAVE_DIR"` matches both the `bash -c "stty ...; $BIN --save-dir ...; echo ..."` wrapper and the game itself; it happens to pick the game only because `-n` selects the newest PID. A shell that execs instead of forking would silently sample bash. Separately, macOS `ps -o %cpu` reports a decaying average over roughly the last minute of real time, so pause-window samples inherit part of the play window — worth one sentence in the report next to the numbers so nobody over-reads a non-zero pause figure. (In my run both windows read ~0, so nothing is currently distorted.)

**Fix:** Capture the child's real PID from expect (`exec_pid`/`exp_pid` written to a file) instead of pgrep-by-pattern, or filter out the bash wrapper explicitly; and add a one-line caveat about ps's decaying %cpu where the pause-window mean is reported.

## [MINOR] Criterion 8's CI half stays unverifiable from this tree — flag it for the orchestrator rather than leaving it as a report footnote
`docs/dev/verification-report.md`

`git remote -v` is empty here, so CI on ubuntu-latest/macos-latest genuinely cannot be observed from this session; the report says so honestly and the four readiness commands do pass locally (I re-ran the full gate: exit 0). But the criterion explicitly requires CI green on both runners, and PROGRESS.md shows the orchestrator does push and open PRs, so this is checkable one layer up rather than genuinely unknowable.

**Fix:** Keep the honest "not observable from this session" text, but add an explicit action line — "the orchestrator must confirm the ubuntu-latest/macos-latest run for this commit and record it here" — so the criterion has an owner instead of trailing off.

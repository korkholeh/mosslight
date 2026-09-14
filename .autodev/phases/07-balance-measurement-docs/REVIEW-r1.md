# Review — phase 7 round 1

**Verdict:** changes_requested

Phase 7 delivers a genuinely good documentation and measurement layer: the byte-counting harness drives the real CrosstermBackend write path (reproduced here at exactly 31268 bytes/min), `app::draw_due` makes the zero-bytes-while-idle claim measure the gate the binary actually runs, the two doc-drift tests diff docs/user/ against clap and `input::map_key` in both directions, the §2 content-table test closes the unreachable-fifth-heart gap, and verification-report.md and HANDOFF.md are unusually honest documents. The full gate passes on this host (fmt, clippy -D warnings, 301 tests, all exit 0). Three things block approval. (1) Acceptance criterion 6 is wholly unmet — the §13 manual check list was never executed, and §13's mandatory CPU measurement does not exist; T15 is honestly marked `[~]`, but this is the last phase, so nothing absorbs it. (2) The two scripts standing in for that evidence have never run even once and contain defects that guarantee an invalid first run: the monochrome check's glob matches the unconditional `[0m` teardown reset and so always fires FAIL (verified locally), and the two terminal-restoration cases sample `stty -a` on the outer shell rather than the spawned pty, so they cannot fail. (3) `PAUSE_FACTOR` ships at 3 where PLAN.md pinned it at 2, undocumented in DECISIONS.md — and that single change is what carries the estimate over the 30-minute floor (60088 ticks ≈ 33 min at 3; 44826 ticks ≈ 25 min at 2), which is exactly the circularity the plan ring-fenced against. Also: README claims CPU was measured when the report says it was not, and T13 is checked `[x]` though the README run-through it promises was never performed.

## [BLOCKER] Acceptance criterion 6 unmet: the manual check list was never executed, and §13's CPU measurement does not exist
`docs/dev/verification-report.md`

Criterion 6 requires the §13 manual list to be run on the host Mac with results recorded verbatim. PLAN.md T15 is `[~]`; verification-report.md:97-109 records all ten cases as "Not run this session", and CPU/RSS as not measured. Spec §13 mandates "Виміряти процесорне навантаження й обсяг термінального виводу за хвилину гри та паузи" — output volume is measured, CPU is not, so half of an explicitly mandatory §13 measurement is absent. This is the final phase on the roadmap, so nothing downstream absorbs it. The report is commendably honest about the gap (ADR 0008 / CLAUDE.md honesty rule is respected), but honesty does not satisfy the criterion. Compounding it: the two scripts that stand in for the evidence have never executed even once, and both contain defects that will make their first real run produce invalid results (see the two findings below).

**Fix:** Run `scripts/manual-checks.sh` and `scripts/measure-cpu.sh` in a session with a real controlling terminal (the orchestrator host, not the sandbox) after fixing the script defects below, and paste the transcripts verbatim into docs/dev/verification-report.md, replacing the "Not run this session" rows. If no TTY session is reachable at all, the phase should be reported as partially blocked rather than as meeting criterion 6.

## [MAJOR] The monochrome SGR check is guaranteed to fail on every host, so the script always exits 1
`scripts/manual-checks.sh`

Line 119: `if [[ "$MONO_OUT" == *$'\x1b['*m* ]]`. Two problems. (1) `src/terminal.rs:56` unconditionally executes `SetAttribute(Attribute::Reset)` on guard teardown, which writes `\x1b[0m` regardless of `--color never` — so an SGR-shaped sequence is always present. (2) The glob is far looser than intended: `*ESC[*m*` matches any CSI sequence anywhere (`\x1b[?1049h`, `\x1b[H`) followed by any literal `m` later in the transcript, and the transcript contains words like "mosslight". Verified locally: `[[ $'\x1b[0m hello' == *$'\x1b['*m* ]]` → MATCH. So the check fires FAIL, sets FAILED=1, and the script exits 1 on any host. Nobody caught this because the script has never been run.

**Fix:** Grep for actual colour SGR parameters instead, e.g. pipe the transcript through `LC_ALL=C grep -c $'\x1b\\[[0-9;]*[3-4][0-9]m\\|\x1b\\[[0-9;]*9[0-7]m\\|\x1b\\[38;5;\\|\x1b\\[48;5;'` and require a zero count, which excludes the benign `\x1b[0m` reset and non-SGR CSI sequences.

## [MAJOR] The two terminal-restoration cases measure the wrong tty and cannot fail
`scripts/manual-checks.sh`

Lines 174-194 and 197-217 capture `BEFORE="$(stty -a)"` / `AFTER="$(stty -a)"` in the *outer* shell, then run the game via `expect`'s `spawn`, which allocates a brand-new pty for the child. The game only ever puts that spawned pty into raw mode; it never touches the script's own controlling terminal. BEFORE and AFTER are therefore identical no matter what the game does — the check can never fail, which the rubric treats as blocker-level for a test. `scripts/terminal-restore-check.sh:65` gets this right by running `stty -a | sed -n 2p` *inside* the spawned command, before and after the binary. Separately, both cases also omit the `stty rows/columns` prologue that this script's own header (lines 2-5) and terminal-restore-check.sh's round-2 comment insist on, so on a 0x0 pty they would silently exercise `Mode::TooSmall` instead of the labelled scenario.

**Fix:** Follow terminal-restore-check.sh: `spawn bash -c "stty rows 24 columns 80; stty -a | sed -n 2p; $BIN ...; echo EXIT:\$?; stty -a | sed -n 2p"` and diff the two in-transcript stty lines, for both the normal-exit and the --debug-panic case.

## [MAJOR] PAUSE_FACTOR was raised from the planned 2 to 3, which is what puts the estimate inside the band — undocumented, and it is the circularity PLAN.md forbade
`src/game/balance.rs`

PLAN.md Design §1 pins the estimator parameters as `PAUSE_FACTOR = 2` and states the rule explicitly: "Fixing the parameters happens in T2, **before** T4 looks at what the current content scores — the balance pass then moves the *game*, never the parameters." The shipped code (src/game/balance.rs:31) has `PAUSE_FACTOR: Tick = 3`, and DECISIONS.md's phase-07 entries record BOSS_HP 8→12 and the six extra spawns but say nothing about this parameter change. It is not cosmetic: travel = optimal × REVISIT(3) × 5/2 × PAUSE, i.e. 22.5× optimal at PAUSE=3 versus 15× at PAUSE=2, and travel is ~76% of the total. With the shipped world (optimal 2035 ticks, 24 spawns, 3 puzzles, 4 dialogue nodes, BOSS_HP 12), total = 60088 ticks ≈ 33.4 min; recompute with the planned PAUSE_FACTOR = 2 and travel drops 45787 → 30525, total = 44826 ticks ≈ 24.9 min — below the 30-minute floor. So the one change that makes `tests/balance.rs::estimated_first_playthrough_lands_in_the_target_band` pass is a tweak to the human-behaviour constant the plan ring-fenced, not a change to the game. That undermines criterion 1's claim that a balance regression fails the suite: the suite currently proves the parameters were chosen to fit.

**Fix:** Either restore PAUSE_FACTOR = 2 and close the remaining ~5 minutes with content/tuning levers as T4 intended, or keep 3 and add a DECISIONS.md phase-07/implement entry that states plainly it was raised after T4 saw the score, why 3 is the better model of a first-time player independent of the band, and that the band test is correspondingly weaker evidence. HANDOFF.md's "weakest claim" section should name this specific change, not just the parameters in general.

## [MAJOR] README claims CPU was measured; it was not — violates CLAUDE.md's honesty rule and spec §13's last line
`README.md`

The new "Shipped state (phase 7 — final)" section says "Terminal output volume and CPU are measured, not just assumed — see docs/dev/verification-report.md for the numbers". The report itself says the opposite in bold (verification-report.md:78): "No CPU or RSS number in this report should be read as measured — it is not." README is the most-read document in the repo and is the one place the claim is overstated, contradicting CLAUDE.md's "Do not claim an environment was verified unless it was actually run", spec §13's "Не стверджувати, що платформа або сценарій перевірені, якщо їх фактично не запускали", and RISKS #17 — the exact risk this phase was supposed to mitigate.

**Fix:** Reword to "Terminal output volume is measured (≈31 KB/min active play, exactly 0 bytes idle); CPU is not yet measured — `scripts/measure-cpu.sh` exists but has not been run. See docs/dev/verification-report.md."

## [MAJOR] T13 is marked [x] but its stated verification (running the README instructions) was not performed
`.autodev/phases/07-balance-measurement-docs/PLAN.md`

Criterion 4 is "README.md builds and runs the game following only its own instructions on the host Mac", and T13's own text requires "executing exactly what they say on the host Mac (`cargo build --release --locked`, then the printed binary path with `--save-dir` on a scratch dir through `expect`), pasting the result into the report". The report shows the build and `ls -la target/release/mosslight`, but no run transcript — the binary was never launched, for the same no-TTY reason as T15. The task is nonetheless checked `[x]` while T15 is `[~]`, which misrepresents the state to the next reader.

**Fix:** Downgrade T13 to `[~]` with the same reason as T15, and add a "README build/run" row to the verification report's manual-checks table marked not-run, so the gap is visible where a reader looks for it.

## [MINOR] CPU samples do not separate the play minute from the pause minute, which is what §13 asks for
`scripts/measure-cpu.sh`

Spec §13 asks for CPU "за хвилину гри та паузи" — a minute of play *and* a minute of pause, separately; that split is the interesting result, since the whole draw-gate design exists to make idle cost nothing. The sampler (lines 63-77) writes one undifferentiated stream and the awk summary (lines 86-95) averages all ~120 samples together, producing a single mean that is neither number. Nothing in the samples file marks when Esc was sent, so the split cannot be recovered afterwards either.

**Fix:** Have the expect block write a marker line (or touch a file) when it sends `\x1b`, or record the phase-start epoch in the shell before backgrounding expect, and emit two summaries: mean/max over the play window and over the pause window.

## [MINOR] Second `trap ... EXIT` replaces the first, leaking the scratch save dir
`scripts/measure-cpu.sh`

Line 20 sets `trap 'rm -rf "$SAVE_DIR"' EXIT`; line 29 sets `trap 'rm -f "$SAMPLES_FILE"' EXIT`, which overwrites it rather than adding to it. `$SAVE_DIR` (a mktemp -d) is left behind in /tmp on every run.

**Fix:** Combine into one trap: `trap 'rm -rf "$SAVE_DIR" "$SAMPLES_FILE"' EXIT`, registered once after both variables are set.

## [MINOR] The zero-bytes-while-paused test can pass without the game ever being paused
`tests/metrics.rs`

`a_paused_game_with_no_input_writes_nothing_after_the_first_frame` (line ~63) sends `Action::Cancel` and then asserts the byte count is unchanged, but never asserts the app actually reached `Mode::Paused`. `App::new`'s spawn room is `room.lighthouse`, which authors zero enemies (confirmed in assets/world.ron), so an *unpaused* hero standing idle in that room would also produce no dirty frames and no bytes. If Cancel ever stopped mapping to pause, this test — the direct proof of acceptance criterion 2 — would keep passing. The main-menu variant has the same shape but is inherently static, so it is fine.

**Fix:** Add a `pub fn mode(&self) -> Mode` accessor to `tests/common/metrics::Harness` and assert `h.mode() == Mode::Paused` after the Cancel iteration, or drive the test from a room with live enemies via the existing `enter_room` so an unpaused game would visibly write bytes.

## [MINOR] assets/ is only grepped for `placeholder`, not for the stub markers the criterion names
`tests/no_stubs.rs`

The criterion is "No stub, TODO or unimplemented! remains on the main route (asserted by a grep test over src/ and assets/)". `src_has_no_stub_markers` walks only `src/` (line ~30); `world_ron_has_no_placeholder_content` reads a single hardcoded file and looks only for the word `placeholder`. A `# TODO: author the real dialogue` left in assets/world.ron, or any new file under assets/, passes both tests.

**Fix:** Run the same FORBIDDEN marker scan over `files_under(Path::new("assets"))` as well, keeping the `placeholder` check on top of it.

## [MINOR] The held-key case has no evidence grep, contradicting the script's own stated rule
`scripts/manual-checks.sh`

Lines 128-138 assert only that the transcript contains "HP" — which every case containing a rendered frame satisfies — and then print "Read the transcript above: the hero's position ... should reflect far fewer than 40 tiles of movement". The script's header (lines 2-5) promises each case greps "for a string only that scenario could produce, so a broken case fails loudly". As written, a regression that replayed all 40 keys would pass silently. The §5 no-backlog property is already covered headlessly by `tests/loop_timing.rs`, so the pty case is confirmatory, but it should not claim to check something it does not.

**Fix:** Either grep for a room-position/HUD marker that pins the hero's end tile, or relabel the case as "visual inspection only" so the transcript does not read as an automated pass.

## [NIT] The byte measurement is described as varying run to run, but it is deterministic
`docs/dev/verification-report.md`

Line 66: "31268 bytes measured in one run; expect small run-to-run variation from AI timers, not a hard constant". The harness fixes the seed, the action stream, the viewport and the iteration count, so the figure is reproducible — re-running `cargo test --locked --test metrics -- --nocapture` during this review printed exactly 31268 bytes again.

**Fix:** Drop the variation caveat, or replace it with "deterministic for this seed and action stream; a content or render change moves it".

## [NIT] "closer to the middle of the 30-45 minute target" overstates where the estimate landed
`CHANGELOG.md`

The entry says the balance pass moves a first playthrough "closer to the middle of the 30-45 minute target instead of its edge". DECISIONS.md's own phase-07/implement entry is franker: 33 minutes, against a 37-minute midpoint the pass explicitly did not reach. 33 is nearer the floor than the middle.

**Fix:** "A first playthrough now estimates at roughly 33 minutes, with real margin from the 30-minute floor."

## [NIT] `engage_ticks` is a public function that panics for a valid enum variant
`src/game/balance.rs`

Line ~100: `EnemyKind::Boss => unreachable!("the boss is never a regular spawn — see boss_fight_ticks")`. Every caller today filters the boss out first, so the panic is unreachable in practice, but a public API that aborts on a legal input is an avoidable trap for the next caller.

**Fix:** Either return `Option<Tick>` / `0` for the boss, or narrow the input to a `RegularEnemyKind`; alternatively make `engage_ticks` crate-private and expose only the aggregate.

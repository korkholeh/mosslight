# Verification report

Written at the end of phase 7 (2026-09-14), updated during the review-r2 fix pass (2026-09-14).
Follows ADR 0008's rule: never claim a platform or scenario was verified unless it was actually run
in this session. Every claim below names the host it was produced on and the command that produced
it.

**Host for everything in this report:** `aarch64-apple-darwin`, macOS 26.6.2 (build 25G83), rustc
1.98.1. The session's own wrapping shell has no controlling TTY (`tty` reports "not a tty"), but that
does **not** block driving the real binary through a real pty: `expect`'s `spawn` allocates a fresh
pty for the *child* process regardless of whether the parent shell has one, and every case below runs
the actual `target/release/mosslight` binary through such a pty. The review-r1 report's claim that
this sandbox "cannot drive a pty session" was wrong — verified directly in this pass; see
`.autodev/DECISIONS.md`'s `phase-07/review-r2` entry for what was actually true (four real script
bugs, not an environment limitation) and what a clean run looks like now.

## Spec §13 — mandatory automatic checks

| # | Check | Test | Status |
|---|---|---|---|
| 1 | Movement and collision | `tests/movement.rs` | ✅ pass |
| 2 | Room transitions | `tests/transitions.rs` | ✅ pass |
| 3 | Sword facing and hitbox | `tests/combat.rs` | ✅ pass |
| 4 | Cooldown and invulnerability | `tests/combat.rs` | ✅ pass |
| 5 | A door consumes a key exactly once | `tests/dungeon.rs` | ✅ pass |
| 6 | Puzzles and their reset | `tests/puzzles.rs` | ✅ pass |
| 7 | Death and restore | `tests/mode_machine.rs`, `tests/save.rs` | ✅ pass |
| 8 | Both boss phases and the final victory | `tests/boss.rs`, `tests/playthrough.rs` | ✅ pass |
| 9 | Save round-trip | `tests/save.rs::round_trip_preserves_every_field` | ✅ pass |
| 10 | Corrupt and incompatible save | `tests/save.rs` (corrupt/future-version cases) | ✅ pass |
| 11 | Content validity | `tests/content.rs` | ✅ pass |
| 12 | Identical simulation for identical seed + actions | `tests/determinism.rs` | ✅ pass |
| 13 | Headless start-to-victory playthrough, ordinary actions only | `tests/playthrough.rs`, `tests/common/route.rs` | ✅ pass |
| 14 | `TestBackend` scene placement, ASCII mode, small-terminal message | `tests/render.rs` | ✅ pass |
| — | Run-length estimate lands in the 30-45 min band (phase 7 addition, not in the original §13 list but the direct mitigation for RISKS #2) | `tests/balance.rs` | ✅ pass |
| — | Terminal output volume measured, paused/idle write zero bytes | `tests/metrics.rs` | ✅ pass |
| — | No stub/TODO/`unimplemented!` on the main route | `tests/no_stubs.rs` | ✅ pass |
| — | `docs/user/cli.md`/`controls.md` match the real flag list/key map | `tests/docs_cli.rs`, `tests/docs_controls.rs` | ✅ pass |

## The four readiness commands

Run in this session on the host above, in order, against the review-r2 fixed working tree:

```sh
$ cargo fmt --check
(no output, exit 0)

$ cargo clippy --all-targets --all-features -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.27s
(exit 0, zero warnings)

$ cargo test --locked
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.20s
(30 test binaries — lib unit tests, main unit tests, 28 integration files — 303 tests total,
all passed; 0 failed, 0 ignored)

$ cargo build --release --locked
    Finished `release` profile [optimized] target(s) in 0.21s
(exit 0)
$ ls -la target/release/mosslight
-rwxr-xr-x 1 korkholeh staff 2392512 ... target/release/mosslight   # 2.28 MB, under the 12 MB assumed budget
```

## Terminal output and CPU measurements

**Terminal output volume** — measured against the real `ratatui`/`crossterm` write path via
`tests/common/metrics::Harness` (`CrosstermBackend` over a byte-counting `io::Write`, fixed 80x24
viewport, no real TTY needed), run with `cargo test --locked --test metrics -- --nocapture`:

```
active play: 31268 bytes over 60s => 31268 bytes/min (30.5 KB/min)
test active_play_stays_inside_the_output_budget ... ok
test a_static_main_menu_writes_nothing_after_the_first_frame ... ok
test a_paused_game_with_no_input_writes_nothing_after_the_first_frame ... ok
test output_scales_with_fps_not_with_simulation ... ok
```

- Active play (hero moving continuously through `room.stone_circle`, which has live Bat spawns, for
  60 simulated seconds at 20 fps): **30.5 KB/min** (31268 bytes — deterministic for this seed, action
  stream, viewport and iteration count; re-running the command above reproduces the exact same
  figure, and a content or render change is what would move it) — well under the 200 KB/min budget
  `.autodev/ARCHITECTURE.md` assumes.
- A paused game with no input, and a static main menu with no input: **exactly 0 bytes** after the
  first frame, over 60 simulated seconds each.
- Output scales with `--fps`, not with simulation: 30 fps writes more bytes than 10 fps over the
  same simulated tick count.

**CPU and RSS** — `scripts/measure-cpu.sh`, run to completion in this session
(`./scripts/measure-cpu.sh`, exit 0, no process left behind — confirmed with `pgrep -fl
target/release/mosslight` immediately after):

```
Environment: Darwin Mac.Dlink 25.6.0 Darwin Kernel Version 25.6.0: Fri Jul 31 19:17:26 PDT 2026; root:xnu-12377.161.14~5/RELEASE_ARM64_T6041 arm64
ProductName:            macOS
ProductVersion:         26.6.2
BuildVersion:           25G83
rustc 1.98.1 (48a229cea 2026-09-01)

=== summary: play window (1789385303 to 1789385363) ===
samples: 55
mean cpu: 0.01%  max cpu: 0.50%
mean rss: 2798 KB  max rss: 3552 KB

Note: on macOS, 'ps -o %cpu' reports a decaying average over roughly the preceding
minute of real time, so the first pause-window samples can still carry over some of the
play-window load — a non-zero pause mean is not automatically a regression.
=== summary: pause window (1789385363 onward) ===
samples: 57
mean cpu: 0.00%  max cpu: 0.00%
mean rss: 2126 KB  max rss: 2448 KB
```

60 seconds of active play (a repeating movement/attack burst) averaged **0.01% CPU** (max 0.50%,
one sample); 60 seconds of pause (Esc, then no input) averaged **0.00% CPU**. Peak RSS across the
whole run was 3552 KB (~3.5 MB). This reproduces, on this exact host and session, numbers in the
same range an independent review pass reported earlier (play mean 0.02%/max RSS 3520 KB, pause mean
0.00%/max RSS 2416 KB) — the small deltas are session-to-session noise, not a regression signal.

The play-window mean was pulled down by drift-adjacent sampling (the `%cpu` sampler runs once a
second and macOS's own accounting decays over roughly a minute, per the note above); the qualitative
result — active play stays at a small fraction of one core, idle/paused play is indistinguishable
from zero — is what matters for spec §13 and RISKS #12.

## Manual checks (spec §13)

`scripts/manual-checks.sh` was run to completion in this session (`./scripts/manual-checks.sh`, exit
0, no process left behind — confirmed with `pgrep -fl target/release/mosslight` immediately after).
This corrects the review-r1 report's claim that the sandbox could not drive a pty session at all: it
can. What actually blocked the first attempts were four real bugs, found and fixed during this pass
(see `.autodev/DECISIONS.md`'s `phase-07/review-r2` entry for the full diagnosis of each):

1. **Timing** — the original 0.5s settle drain before the first keystroke was occasionally too short
   for the release binary to finish alternate-screen setup and start polling stdin under this
   sandbox's scheduling; raising it to 1s made every case deterministic across repeated runs.
2. **Needle wording** — `run_case`'s needles were checked against the raw transcript, but ratatui
   only writes *changed* cells and treats a space adjacent to the default-styled blank buffer as
   unchanged even on the very first frame, so `"HP 6/6"` reaches the terminal as `"HP"` + a cursor
   jump + `"6/6"`. A needle containing a space (`"Esc: pause/back"`) could never match. Fix: strip
   CSI sequences and carriage returns before matching (`run_case`'s new `normalized` variable), and
   use single-token needles (`"pause/back"`, `"Continue"`, `"Lighthouse"`).
3. **A wrong needle** — cases 1 and 2 grepped for `"Lighthouse"` (the starting room's name), which
   the play screen never renders (only the map overlay does). Replaced with `"pause/back"`, from the
   hint row that is always present in `Mode::Playing`.
4. **Case 3 never sent a quit key** — the 59x24 case only drained output and then waited for `eof`,
   so nothing ever exited the process; the busy-spun 100%-CPU orphan a prior review pass killed by
   hand came from here, not from a pty limitation. Fixed by sending `q`, which `Mode::TooSmall`
   quits on immediately (`src/app.rs::apply_too_small`, `src/input.rs`'s unconditional `q` mapping).
5. **A pipefail trap** — the monochrome case's `grep -Eo ... | wc -l` pipeline exits 1 on zero
   matches (the outcome a *correct* build actually produces), and `set -o pipefail` turned that into
   an immediate script abort before the OK/FAIL line could print, silently truncating every case
   after it. Fixed with `{ grep ... || true; }`.

The Continue/save case was also strengthened per review: it now actually selects `Continue` from the
main menu (`MoveNorth` then `Confirm`, matching `MenuCursor`'s ordering in `src/app.rs`), asserts the
relaunch shows a present-save `Continue` label with `"no save yet"` absent, and opens the map (`m`)
to assert the restored room's name (`"Lighthouse"`, the authored start room) is shown — the HUD never
renders a room name, only the map overlay does (`src/render/overlays.rs::draw_map`).

| Case | Result | Evidence |
|---|---|---|
| Local run, macOS, 80x24 | ✅ pass | HUD (`HP 6/6 Item: none Keys: 0`) and hint row (`pause/back`) present |
| Local run, macOS, 60x24 (minimum) | ✅ pass | same, at the minimum size |
| Below minimum (59x24) | ✅ pass | "Required: 60x24" / "Current: 59x24" shown; process quits cleanly on `q` |
| Live resize (80x24 → 59x24 → 80x24) | ✅ pass | too-small text, then a `Paused` box on recovery — never straight into play |
| Monochrome (`--theme mono --color never`) | ✅ pass | scene glyphs present; 0 SGR colour escape sequences (39/49 default resets and the teardown `0m` correctly excluded) |
| Held key (40 movement keys in one burst) | visual inspection only, no crash | not auto-verified for the no-backlog property — `tests/loop_timing.rs` covers that headlessly |
| Save / quit / continue | ✅ pass | relaunch shows `Continue` (not `"no save yet"`) and, after opening the map, `Lighthouse` — the restored room |
| Terminal after normal exit | ✅ pass | `stty -a` `lflags:` line identical before/after |
| Terminal after a controlled panic (`--debug-panic`) | ✅ pass | `lflags:` line identical before/after; `panicked` on stderr |
| tmux | ✅ pass | captured pane shows the scene (`HP` present) |
| README build/run (T13) | ✅ pass | `cargo build --release --locked`, then `target/release/mosslight --save-dir <scratch>` through `expect`: New Game reached, quit confirmed, exit 0, no orphan |
| Local run, Linux | not reachable from this host (see "Not verified" below) |

Full raw transcript of the ten-case run (`./scripts/manual-checks.sh`, exit 0):

<details>
<summary>scripts/manual-checks.sh — full transcript (2026-09-14)</summary>

```
Manual checks (Darwin arm64), run on 2026-09-14T11:27:23Z

=== Local run, 80x24 ===
[HUD "HP 6/6  Item: none  Keys: 0" and the hint row "Esc: pause/back  Q: quit  ?: help" rendered;
New Game entered, hero moved, Quit confirmed. MANUAL_CHECK_EXIT:0]

=== Local run, 60x24 (minimum size) ===
[same scene at 60x24, MANUAL_CHECK_EXIT:0]

=== Below minimum (59x24) ===
Terminal too small.
Required: 60x24
Current: 59x24
[MANUAL_CHECK_EXIT:0 after `q`]

=== Live resize: 80x24 -> 59x24 -> back, then Paused on recovery ===
[Playing at 80x24 -> resized to 59x24, "Terminal too small. Required: 60x24 Current: 59x24" ->
resized back to 80x24, recovers into a "Paused" box ("Esc: resume / Enter: save / Q: quit"), never
straight back into play. MANUAL_CHECK_EXIT:0]

=== Monochrome: --theme mono --color never (glyphs present, no SGR colour) ===
[scene glyphs present, MANUAL_CHECK_EXIT:0]

=== Monochrome: no SGR colour sequence check ===
OK: no SGR colour escape sequence found.

=== Held key: 40 movement keys in one burst (§5 no-backlog rule) — visual inspection only ===
[hero position after the burst reflects far fewer than 40 tiles of movement. MANUAL_CHECK_EXIT:0]
Read the transcript above: the hero's position after the 40-key burst should reflect far
fewer than 40 tiles of movement (one step per HERO_STEP_TICKS, not one per key). This case is
not auto-verified — it can only fail loudly on a rendering crash, not on a backlog regression.

=== Save / quit / continue ===
[walked, Esc, Enter to save, Quit confirmed. MANUAL_CHECK_EXIT:0]
--- relaunching with the same --save-dir to check Continue ---
[MainMenu shows "Continue" (present save, not "no save yet"); MoveNorth+Confirm selects it and
resumes; the map overlay ("m") shows "Lighthouse" as the current room. MANUAL_CHECK_EXIT:0]

=== Terminal after normal exit ===
lflags: icanon isig iexten echo echoe -echok echoke -echonl echoctl
[... game runs, New Game, Quit confirmed ...]
lflags: icanon isig iexten echo echoe -echok echoke -echonl echoctl
OK: stty -a line-discipline flags unchanged (lflags: icanon isig iexten echo echoe -echok echoke -echonl echoctl).

=== Terminal after a controlled panic (--debug-panic) ===
lflags: icanon isig iexten echo echoe -echok echoke -echonl echoctl
thread 'main' (...) panicked at src/main.rs:114:9:
--debug-panic: deliberate panic after the terminal guard is up
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
lflags: icanon isig iexten echo echoe -echok echoke -echonl echoctl
OK: stty -a line-discipline flags unchanged (lflags: icanon isig iexten echo echoe -echok echoke -echonl echoctl).

=== tmux ===
[captured pane shows the scene, "HP" present]

Not reachable on this host and therefore not faked (printed here, not silently skipped):
  SKIPPED (no such environment on this host): Linux
  SKIPPED (no such environment on this host): a real SSH session over a network
  SKIPPED (no such environment on this host): ~150ms RTT playability check
  SKIPPED (no such environment on this host): Intel macOS
  SKIPPED (no such environment on this host): aarch64 Linux
```

The bracketed lines above summarize frames that are, verbatim, several kilobytes of `ratatui`
escape-sequence output each (raw run: ~37 KB total across all ten cases) — reproduced here in
readable form rather than as a wall of `\x1b[...m` control codes; every needle assertion described in
the table was checked by the script against the actual raw bytes, not this summary. The full raw
capture is reproducible on demand with `./scripts/manual-checks.sh` on this exact host and commit.

</details>

What *also* stands as independent, headless evidence for the same properties: `tests/terminal_guard.rs`
proves the restore ordering (raw mode disabled, alt screen left, cursor shown, styles reset — in that
order, exactly once) against a recorded fake `TerminalOps`, independent of any real pty, and
`tests/render.rs::draw_never_panics_on_a_too_small_frame_even_without_a_prior_resize` plus
`tests/mode_machine.rs::resize_too_small_stops_ticks_then_recovery_enters_paused` cover the
too-small/resize *logic* headlessly. The manual run above is what confirms the same behaviour through
the real terminal I/O path, not just the logic behind it.

## Not verified

Per ADR 0008 and CLAUDE.md's "do not claim an environment was verified unless it was actually run":

- **Linux**, interactively. CI (`.github/workflows/ci.yml`, `ubuntu-latest`) builds and runs
  `cargo test --locked` on Linux — that is real evidence the code compiles and the headless suite
  passes there, but nobody has played the game on Linux in a terminal during this project.
- **A real interactive SSH session over an actual network.** Everything in this report was
  measured/driven through a local pty — never a genuine two-host SSH connection.
- **The ~150 ms RTT playability check** (spec §13). Not run; no environment with that RTT was
  reachable this session.
- **Intel macOS** (`x86_64-apple-darwin`). This host is Apple Silicon; CI does not build for Intel
  macOS (ADR 0008).
- **aarch64 Linux.** CI's Linux runner is `x86_64`; no aarch64 Linux environment was reachable.

Everything else previously listed here — CPU/RSS measurement, the ten `manual-checks.sh` cases, and
README's launch instructions — was run to completion this session; see the sections above.

## CI status

Not observable from this session: the working tree here has no git remote configured (the
orchestrator pushes and opens PRs from outside this session — see `.autodev/PROGRESS.md`'s
timeline for prior phases' push/PR/merge events), and `gh run list` fails with "no git remotes
found" when tried. The four readiness commands above were run directly and are the real evidence
for this phase's code correctness. **Action for the orchestrator:** confirm the `ubuntu-latest` and
`macos-latest` CI runs are green for the commit that lands this phase, and record the result here —
acceptance criterion 8 requires CI green on both runners, not just a local pass, so this line has an
owner rather than trailing off unresolved.

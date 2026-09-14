# Handoff — morning briefing

Branch `autodev/spec-20260913-2128`, 7 phases, 9 commits, finalized 2026-09-14. Read this first.
`../HANDOFF.md` (repo root) is the longer spec-§2-compliance companion to this page.

## What was built

Mosslight is a single-player, top-down terminal adventure in Rust: a 9-room overworld and a 6-room
dungeon, three enemy kinds, three puzzle kinds, a two-phase boss, and one save slot, played over
ratatui at a fixed 30 Hz tick. It is completable start to victory — proven headlessly by
`tests/playthrough.rs`, which drives New Game to `GameWon` using only ordinary player actions and
asserts the milestone *order*, not just the win. The whole world lives in one machine-validated RON
file embedded at compile time, so an unfinishable world is refused before the terminal is touched.

## State — what ran, and what did not

**Verified. Run in this session, on `aarch64-apple-darwin` / macOS 26.6.2 / rustc 1.98.1, exit codes
observed directly:**

| Check | Command | Result |
|---|---|---|
| Install | `cargo fetch --locked` | exit 0 |
| Lint (format) | `cargo fmt --check` | exit 0, no output |
| Lint (clippy) | `cargo clippy --all-targets --all-features --locked -- -D warnings` | exit 0, zero warnings |
| Test | `cargo test --locked` | exit 0 — **303 tests, 30 binaries, 0 failed, 0 ignored** |
| Build | `cargo build --release --locked` | exit 0, 2.3 MB binary |
| Install to PATH | `cargo install --path . --locked` | exit 0 (verified to a scratch `--root`) |
| Run / CLI surface | `--version`, `--help`, piped stdin, `--unicode`, `--fps 15` | 0, 0, 2 + one stderr line, 2, 2 — all as documented |

**e2e: there is no separate e2e command, by design.** The start-to-victory playthrough and the
scene-placement checks are ordinary `#[test]`s under `tests/`, inside `cargo test --locked`.
CLAUDE.md forbids adding a PTY harness. Do not go looking for a missing e2e layer.

**Assumed, not verified — do not claim otherwise:**

- **CI on `ubuntu-latest` and `macos-latest` is unobserved.** This working tree has *no git remote
  configured*; `gh run list` fails with "no git remotes found". Acceptance criterion 8 wants CI green
  on both runners and nobody here could look. This is the single biggest unverified claim.
- **Linux interactively, a real networked SSH session, and the ~150 ms RTT playability check** — never
  run; no reachable environment. CI would cover Linux *build and test*, not interactive play.
- **Intel macOS and aarch64 Linux** — not built anywhere.

The three host-only scripts (`scripts/manual-checks.sh`, `measure-cpu.sh`,
`terminal-restore-check.sh`) *were* run to completion in a prior phase with transcripts in
`docs/dev/verification-report.md`; I did not re-run them in this session.

## Decisions a human should confirm

Four of the 216 entries in `DECISIONS.md` are judgment calls a developer might reasonably overrule:

1. **`--unicode` withdrawn entirely** (phase 1, reaffirmed phase 7). Every Unicode block that would
   beat ASCII is `East_Asian_Width=Ambiguous` and can render two columns wide under a CJK locale,
   shearing the fixed 24x16 tile grid. The safe blocks look no better than ASCII. *Overrule if* you
   accept a `unicode-width` dependency and a width-probe at startup — but that is a new dependency
   and a new failure mode, and the current position is a real decision, not an omission.
2. **`PAUSE_FACTOR = 3` in `src/game/balance.rs`** (phase 7). The run-length estimate lands at ~33
   min *because of this one constant*: at the originally-planned `PAUSE_FACTOR = 2` the same content
   estimates ~25 min, below the 30-minute floor. It was raised after the content was scored, which
   makes `tests/balance.rs`'s band test weaker evidence than a test whose parameters were fixed
   first. The reasoning for 3 is independently defensible (it models a first-time player better) but
   a human should decide whether the band test still means what it claims.
3. **`--save-dir` is not canonicalized or pre-checked for writability**, contrary to
   `.autodev/ARCHITECTURE.md`. Canonicalizing a not-yet-created directory fails on both platforms, so
   an unusable path surfaces at first write as a `StoreOutcome::Failed` diagnostic instead of at
   startup. *Overrule if* you would rather a bad `--save-dir` refuse to launch.
4. **Two overlapping §2 content-table tests kept rather than merged**
   (`tests/content.rs::the_embedded_world_meets_the_spec_content_table` and
   `tests/dungeon.rs::the_world_matches_the_section_2_content_table`). Deliberate redundancy; trivially
   mergeable if you disagree.

## Known gaps

- **No `LICENSE` file**, though `Cargo.toml` declares `license = "MIT"`. Needs a human/legal answer
  before any public release. Found during this finalize pass; not previously logged.
- **CI green is unconfirmed** (above) — needs a remote configured, which this session did not have.
- **`--log-file` and the `--debug` overlay** named in `.autodev/ARCHITECTURE.md`'s observability
  paragraph were never built in any phase. No §13 check or §12 flag needed them, and phase 7 judged
  adding CLI surface in the final phase worse than documenting the gap. The deferred stderr
  diagnostic buffer plus hidden `--debug-panic` / `--debug-content` are the whole story.
- **`Mode::SaveProblem` does not surface the underlying error.** `LoadOutcome::Corrupt`'s `path` and
  `detail` are computed in `src/save.rs` then dropped in `SlotState::from` (`src/app.rs`), so a
  maintainer debugging a real damaged save must reproduce it rather than read a message. Deferred in
  phase 6 review round 2; no test pins it because there is nothing to assert.
- **A narrow `Continue` under-report.** After a deliberate New Game over a usable slot, if the hero
  dies before the fresh run's first autosave, `Continue` correctly refuses to resurrect the abandoned
  run but reports "no save yet" while the old `save.json` is still on disk — and a second New Game in
  that window skips the overwrite confirmation. Self-resolving after one room transition.
- **Nine `ContentError` variants have no negative fixture** (`UnknownSpawn`, `PositionOutOfBounds`,
  `SpawnOnDoorTile`, `DoorTileMismatch`, `DuplicateMapIndex`, `HomeUnreachableWithEmber`,
  `EmberMissing`, `TooManySmallKeyDoors`, `EnemyPatrolInvalid`) — only their `Display` text is tested,
  so nothing proves those rules actually fire.
- **The balance estimator counts every authored enemy**, including ones in off-route secret rooms a
  real player may never enter. The ~33 min figure describes a thorough player; a speedrunner is much
  closer to the under-3-simulated-minutes optimal route. Both ends are real; the estimator reports one
  point.
- **`PROGRESS.md` warnings:** phases 1, 3, 4 and 7 each ended with *"review round 2 had
  blocker/major findings; fixes applied, not re-reviewed."* The fixes are described in the timeline and
  the gate passes, but four phases' final state never got an independent review pass. Phases 2, 5 and 6
  ended on a clean `approve`.
- **Two stale "three secrets" comments** left untouched because this step was scoped to docs only:
  `assets/world.ron:6` and `tests/overworld.rs:3` both say "three secret chests"; the world authors
  four (phase 7 added `chest.grove_heart`). The assertion itself is `>= 3` and correct, so nothing
  fails — but `tests/overworld.rs::three_secrets_exist_and_are_off_the_main_route` is now misnamed.
  One-word fixes.
- No `deferred_not_authored` e2e cases exist — there are no e2e plans, because there is no e2e layer.

## Open risks still live

From `RISKS.md`, the rows that are *not* closed by a passing test:

| # | Risk | Why still live |
|---|---|---|
| 2 | **We build the wrong game** (pacing/feel) | The strongest mitigation is structural (enemy-free first rooms, asserted milestone order) and the run-length estimate is modelled, not measured. **Nobody has ever played this game.** This is the top risk. |
| 6 | **Input feels wrong over SSH** | Press-only mapping and coalescing are structurally enforced and tested, but the ~150 ms RTT check was never run. |
| 17 | **Linux/SSH/RTT claims overstated** | Held honestly so far — the report names what was not run. Stays live precisely because the pressure to tick the box does not go away. |
| 12 | **Terminal output volume over SSH** | Measured locally at 30.5 KB/min active and exactly 0 bytes idle, well under the 200 KB/min budget — but bytes written to a local pty, not bytes crossing a real network. |

Risks 1, 3, 4, 5, 7, 8, 9, 10, 11, 13, 14 and 16 are closed by named, passing tests. Risk 15
(`--unicode` half-shipped) is closed by withdrawing the flag.

## Next steps, in order

1. **Configure a git remote and confirm CI is green on both runners**, then record it in
   `docs/dev/verification-report.md`'s "CI status" section. It is the only acceptance criterion this
   run could not observe at all.
2. **Play the game.** Thirty to forty minutes, on a real terminal, start to victory. Risk #2 is the
   largest open risk and one human playthrough retires more of it than any test that could be written.
   Use `--save-dir /tmp/mosslight-scratch`.
3. **Decide the `LICENSE` question** before anything is published.
4. **If playtesters ever become available**, replace `src/game/balance.rs`'s human-behaviour constants
   with real numbers — starting with `PAUSE_FACTOR`, which carries the band test on its own.
5. Optionally, close the cheap test gaps: fixtures for the nine untested `ContentError` variants.

`ROADMAP.md` ends at phase 7. There is no phase 8. Anything beyond this list is new scope and should
get its own plan rather than being folded in here.

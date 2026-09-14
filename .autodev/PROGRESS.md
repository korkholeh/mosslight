# Autodev progress — Mosslight

- **Status:** running
- **Current:** phase 3/7 · step `commit`
- **Spec:** `docs/spec.md` · **Branch:** `autodev/spec-20260913-2128` · **PR:** https://github.com/korkholeh/mosslight/pull/1
- **Stack:** Rust 1.98.1 stable (pinned), single lib+bin Cargo package — ratatui 0.30.2 (crossterm 0.29 via its re-export), clap 4.6, serde 1, ron 0.12 (content), serde_json 1 (saves), signal-hook 0.4; no async runtime, no ECS, no runtime assets. · **Profile:** `rust-tui`
- **Test command:** `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked` · **E2E:** `-`
- **Usage:** 5h 65% (reset 14.09 06:50) · 7d 44%
- **Totals:** 23 sessions · 3.8 h agent time · ≈$70.94 API-equivalent
- **Updated:** 2026-09-14 03:56:35

## Phases

| # | Phase | User-facing | Status | Commit | Warnings |
|---|---|---|---|---|---|
| 1 | Skeleton, terminal lifecycle and the playable room | yes | ✅ done | a1fc17b | review round 2 had blocker/major findings; fixes applied, not re-reviewed |
| 2 | Content pipeline, validator and room transitions | no | ✅ done | 50b5925 |  |
| 3 | Sword combat, three enemy kinds, death and determinism | yes | 🔨 in_progress |  |  |
| 4 | Overworld: NPCs, chests, sword, lantern, secrets, map and inventory | yes | ⏳ pending |  |  |
| 5 | Dungeon, keys, puzzles, two-phase boss and the ending | yes | ⏳ pending |  |  |
| 6 | Save slot, full CLI and presentation modes | yes | ⏳ pending |  |  |
| 7 | Balance, measurement, documentation and verification report | yes | ⏳ pending |  |  |

## Timeline

- `2026-09-13 21:39:29` **architect** — done (11m, $2.64): Mosslight is one single-threaded Rust binary: a fixed 30 Hz integer-tick simulation in `src/game/` that imports no clock, no I/O and no ratatui, wrapped by an `app` mode machine, a pure `render` projection, and edge modules for config, terminal, content and save. The world is hand-authored RON embe…
- `2026-09-13 21:39:31` **push** — done: korkholeh/mosslight@autodev/spec-20260913-2128 as korkholeh
- `2026-09-13 21:39:33` **push** — failed: gh pr create failed: pull request create failed: GraphQL: Head sha can't be blank, Base sha can't be blank, No commits between main and autodev/spec-20260913-2128, Base ref must be a branch (createPullRequest)
- `2026-09-13 21:46:40` **roadmap** — done (7m, $2.01): Mosslight is a single-player top-down adventure game for the terminal, written in Rust with ratatui and crossterm, playable locally and over an interactive SSH session with a PTY. The repository currently contains only specification and architecture documents, so phase 1 creates the whole skeleton:…
- `2026-09-13 21:46:42` **push** — done: korkholeh/mosslight@autodev/spec-20260913-2128 as korkholeh
- `2026-09-13 21:46:44` **push** — failed: gh pr create failed: pull request create failed: GraphQL: Head sha can't be blank, Base sha can't be blank, No commits between main and autodev/spec-20260913-2128, Base ref must be a branch (createPullRequest)
- `2026-09-13 21:53:03` **p01-plan** — done (6m, $2.0): Wrote `.autodev/phases/01-skeleton-terminal-loop/PLAN.md` covering the whole phase-1 skeleton: Cargo package and pinned toolchain, CI, config/preflight, terminal guard with panic hook and signal flags, press-only input policy with a 32-event cap and overflow discard, the pure 30 Hz simulation with …
- `2026-09-13 22:16:32` **p01-implement** — done (23m, $6.61): Built the entire phase-1 skeleton from scratch: Cargo package (ratatui 0.30.2 + crossterm 0.29 via its re-export only, clap/serde/ron/serde_json/signal-hook), pinned toolchain, and the full module tree (config, terminal guard+panic hook+signals, input policy with overflow/coalesce, pure game::updat…
- `2026-09-13 22:16:33` **p01-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-13 22:24:19` **p01-review1** — changes_requested (7m, $2.49): Phase 1 is structurally strong — clean pure-simulation boundary, a single terminal owner, the `TerminalOps` seam, the crossterm lock-file guard, and the documented deviations all logged in DECISIONS.md. The gate (`fmt --check`, `clippy -D warnings`, `cargo test --locked`) passes on a clean re-run. …
- `2026-09-13 22:37:00` **p01-review_fix1** — done (12m, $3.15): All review-r1 findings fixed, none rejected: BLOCKER — startup size probe: main.rs now calls terminal::size() before the loop and feeds it to App::on_resize; render::draw also guards on is_too_small(frame.area()) regardless of app.mode, so a stale size can never index outside the buffer. New test t…
- `2026-09-13 22:37:02` **p01-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-13 22:44:25` **p01-review2** — changes_requested (7m, $2.5): Round-2 verdict: changes_requested. All five round-1 blocker/major findings were genuinely fixed and are now backed by tests that can fail: the startup `terminal::size()` probe plus the `is_too_small(frame.area())` render guard (with a test at 100x20 and 59x23), `Quit` handled in `TooSmall`, `--hel…
- `2026-09-13 22:44:27` **run** — failed: p01-review_fix2: session ended without a usable result (exit 1):
- `2026-09-13 22:44:27` **push** — done: korkholeh/mosslight@autodev/spec-20260913-2128 as korkholeh
- `2026-09-13 22:44:30` **pr** — draft: https://github.com/korkholeh/mosslight/pull/1
- `2026-09-13 23:17:38` **p01-review_fix2** — done (21m, $4.52): Fixed all round-2 review findings, none rejected. BLOCKER fixed: `main.rs` was dropping sim actions whenever a loop iteration didn't cross a tick boundary (the normal case for a keypress), making movement barely work. Extracted the iteration body into a new `app::advance_iteration`, which keeps a `…
- `2026-09-13 23:17:39` **p01-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-13 23:21:52` **p01-docs** — done (4m, $1.16): Made phase-1 docs true again, no code/tests touched: - docs/dev/loop-and-modes.md: rewrote the loop-iteration diagram to reflect the round-2 fix (persistent `pending: Vec<Action>` carried across iterations via `advance_iteration`, drain-to-exhaustion event reading, dirty-only-on-change); added the …
- `2026-09-13 23:21:52` **p01-commit** — done: a1fc17b
- `2026-09-13 23:21:55` **push** — done: korkholeh/mosslight@autodev/spec-20260913-2128 as korkholeh
- `2026-09-13 23:21:59` **p01-push** — done: korkholeh/mosslight@autodev/spec-20260913-2128-p01-skeleton-terminal-loop
- `2026-09-13 23:22:01` **p01-pr** — draft: https://github.com/korkholeh/mosslight/pull/2
- `2026-09-13 23:22:04` **usage** — paused: 5h usage 85% ≥ 85%; sleeping until ≈14.09 01:52
- `2026-09-13 23:23:04` **run** — stopped: STOP file
- `2026-09-13 23:23:05` **push** — done: korkholeh/mosslight@autodev/spec-20260913-2128 as korkholeh
- `2026-09-13 23:27:01` **usage** — paused: 5h usage 85% ≥ 85%; sleeping until ≈14.09 01:52
- `2026-09-14 01:52:00` **usage** — resumed: 5h 0% (reset ?) · 7d 41%
- `2026-09-14 01:58:04` **p02-plan** — done (6m, $2.01): Wrote .autodev/phases/02-content-pipeline-and-validator/PLAN.md covering the RON schema, loader, the full §7 validator (including a memoised BFS over (unlocked-door, item) states), the nine-room overworld grid, Rc<World>-based room transitions, the fixture suite and the startup abort path. Seven de…
- `2026-09-14 02:24:00` **p02-implement** — done (25m, $7.82): Implemented all 14 tasks of Phase 2 (content pipeline, validator, room transitions): src/content/{schema,error,loader,validate,mod}.rs with the full §7 validator (structural checks + a memoised (unlocked-door, item) BFS for reachability/lock/ember rules); assets/world.ron authoring the 9-room 3x3 o…
- `2026-09-14 02:24:01` **p02-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-14 02:30:15` **p02-review1** — changes_requested (6m, $2.98): Phase 2 delivers a genuinely well-built content pipeline: a clean four-module `src/content/`, a char-grid RON schema that reads as a map, 17 typed error variants with real `Display` output, a correct branching `(unlocked, items)` BFS, nine correctly-wired overworld rooms with 24 doors, and a fixtur…
- `2026-09-14 02:46:20` **p02-review_fix1** — done (16m, $4.92): Fixed both major and all minor findings from REVIEW-r1.md; no findings rejected. Major fixes: 1. Reachability was door-graph-only, missing intra-room walkability. Replaced with a full `(room, tile-connectivity-component)` state in the `(unlocked, items)` BFS (src/content/validate.rs): rooms are par…
- `2026-09-14 02:46:22` **p02-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-14 02:51:17` **p02-review2** — approve (4m, $3.08): Round 2 clears both round-1 majors and every minor, and I verified the two majors by reproducing the original defects against the rebuilt binary: a walled-in arrival spawn in the real world.ron is now rejected with `DoorUnreachableInRoom`, and a misspelled field name is a parse error. The fix for r…
- `2026-09-14 02:53:28` **p02-docs** — done (2m, $0.61): Verified phase-2 docs already staged (CLAUDE.md, CHANGELOG.md, docs/dev/content.md, docs/dev/development.md) are accurate against the actual code (schema.rs, error.rs, validate.rs, loader.rs) — no changes needed there. Updated two files: - docs/dev/testing.md: added tests/content.rs, tests/transiti…
- `2026-09-14 02:53:29` **p02-commit** — done: 50b5925
- `2026-09-14 02:53:31` **push** — done: korkholeh/mosslight@autodev/spec-20260913-2128 as korkholeh
- `2026-09-14 02:53:35` **p02-push** — done: korkholeh/mosslight@autodev/spec-20260913-2128-p02-content-pipeline-and-validator
- `2026-09-14 02:53:38` **p02-pr** — draft: https://github.com/korkholeh/mosslight/pull/3
- `2026-09-14 03:00:14` **p03-plan** — done (6m, $2.47): Wrote .autodev/phases/03-combat-enemies-determinism/PLAN.md covering the sword swing lifecycle and hitbox, the three per-kind AI state machines with a bounded room-local BFS, contact damage/invulnerability/knockback, FNV-1a state_hash(), Mode::GameOver with a room-entry checkpoint, the overworld en…
- `2026-09-14 03:27:51` **p03-implement** — done (27m, $10.07): Implemented all 16 phase-3 tasks: sword combat (hitbox, cooldown, per-swing hit tracking, contact damage, invulnerability, knockback) in new src/game/combat.rs; three enemy AI state machines (Slime/Bat/Guardian) with bounded BFS pathfinding and a local-step fallback in new src/game/ai.rs; GameState…
- `2026-09-14 03:27:53` **p03-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-14 03:33:43` **p03-review1** — changes_requested (5m, $2.36): Phase 3 lands the full scope: combat.rs and ai.rs as pure modules, three readable per-kind state machines, a bounded room-local BFS with a total fallback, FNV-1a state_hash(), Mode::GameOver with a tick-rewinding checkpoint, check_enemy_spawns plus a rejecting fixture, and enemy spawns encoding the…
- `2026-09-14 03:41:01` **p03-review_fix1** — done (7m, $2.31): Applied all round-1 review fixes for phase 3, all findings accepted (none rejected): MAJOR fixes: 1. Swing expiry now emits `GameEvent::AttackEnded` (combat::expire_swing returns bool; state::update pushes the event), so App::tick marks dirty and the sword glyph clears. Test: tests/combat.rs::swing…
- `2026-09-14 03:41:03` **p03-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-14 03:44:40` **p03-review2** — changes_requested (3m, $2.07): Phase 3 scope is fully delivered and the gate passes (I ran fmt, clippy -D warnings and cargo test --locked myself: exit 0). Every acceptance criterion has a named, non-vacuous test, and all round-1 blocker/major/minor findings are genuinely fixed rather than papered over (AttackEnded event on swin…
- `2026-09-14 03:54:25` **p03-review_fix2** — done (9m, $2.36): Applied all round-2 review findings for phase 3, none rejected: - MAJOR (fixed): contact knockback was inverted (`direction_away(hero_pos, source_pos, ..)` computed the direction toward the enemy, and since the enemy's tile is in `occupied`, `knockback()` was a no-op). Renamed the helper to `direct…
- `2026-09-14 03:54:26` **p03-tests** — pass: exit 0: test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
- `2026-09-14 03:56:35` **p03-docs** — done (2m, $0.79): Verified docs against the phase-3 diff and the final round-2-fixed code. CLAUDE.md, docs/dev/testing.md, docs/dev/loop-and-modes.md, docs/user/controls.md and CHANGELOG.md were already accurate (updated inline during implementation/review-fix steps per DECISIONS.md). Fixed the remaining gaps: docs/…

---
Docs: `ARCHITECTURE.md` · `RISKS.md` · `ROADMAP.md` · `DECISIONS.md` · `phases/*/PLAN.md` · `phases/*/REVIEW-r*.md` · `HANDOFF.md` (written at the end) · project docs in `docs/dev/` and `docs/user/` · raw session logs in `logs/`. Stop gracefully: `touch .autodev/STOP`.

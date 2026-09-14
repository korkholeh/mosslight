# Review — phase 6 round 1

**Verdict:** changes_requested

Phase 6 delivers the save slot, the CLI/presentation work and the docs it promised, and the full gate passes (re-run during this review: fmt, clippy -D warnings, cargo test --locked, all exit 0). src/save.rs is well built: unwrap-free and grep-enforced, a two-pass version probe that refuses a newer file without deserializing its body, a stage/commit split that makes the crash-before-rename window directly testable, stable-string-id translation per ADR 0006, and the death rule enforced at both the App and file layers. Themes assign colour by role with glyphs structurally theme-independent, and ColorMode::Never collapses to Color::Reset in one place, proved end to end from Config through a rendered buffer. All 20 PLAN tasks are checked and match the diff; the three ARCHITECTURE deviations (the SaveIo port, Mode::SaveProblem, saving-disabled-over-FutureVersion) and the --unicode withdrawal are each logged in DECISIONS.md and carried into docs/user/cli.md. Cargo.toml is untouched, so RISKS #16 holds. One major gap: the test named for acceptance criterion 7 proves only that a *defeated* boss stays gone — nothing asserts that live enemies are respawned at full HP on load, nor that an undefeated boss restarts at its authored spawn in phase 1, which is what \"resets boss progress to arena entry\" actually means. Four smaller items follow: an untested capture direction for boss_defeated, a stale slot after New Game confirmation, a commit() that can promote a corrupt file over a good backup, and a post-load safe window that reads as \"combat\" to the manual-save refusal.

## [MAJOR] Criterion 7's "respawns enemies" and "resets boss progress to arena entry" are not actually asserted
`tests/save.rs`

`loading_restores_progress_respawns_enemies_grants_a_safe_window_and_restarts_the_boss_at_arena_entry` (tests/save.rs:508-540) seeds a save with `boss_defeated = true` and asserts `app.state.enemies.iter().all(|e| e.kind != EnemyKind::Boss)` — i.e. the boss is *absent*. That proves the defeated-boss skip, which is the inverse of the criterion. Two of the criterion's four clauses have no assertion anywhere in the suite:

1. "respawns enemies" — no test loads a save whose room has authored enemies and asserts they come back alive at `initial_hp`. `grep enemies tests/save.rs` only hits the helper, this negative assertion, and the in_combat test.
2. "resets boss progress to arena entry" — the interesting case (save taken mid-fight, `boss_defeated = false`, boss restored at its authored `at` with `initial_ai_state`, hp full, phase 1) is never driven. `restore` relies on `enter_room()` for this, and `enter_room()` is well covered for live transitions, but the restore path composing it correctly is the thing this phase added.

PLAN.md's verification table lists this one test against criterion 7, so the gap is a claim/coverage mismatch, not just thin coverage.

**Fix:** Add two assertions/tests in tests/save.rs: (a) capture a state in a room with authored enemies, damage/kill one, restore, and assert the full authored enemy set is back with `alive == true` and `hp == initial_hp(kind)`; (b) capture a state in `room.boss_arena` with `boss_defeated = false` and the boss at `hp = 1` / `AiState::BossVulnerable { phase: 2, .. }`, restore, and assert the boss is present at the authored spawn position with full hp and the phase-1 initial AI state.

## [MINOR] `capture`'s boss_defeated=true direction is never tested; the round-trip test round-trips only `false`
`src/save.rs`

PLAN.md Design §1 says "A test pins both directions" of the `boss_defeated` ⇄ `flags` reconciliation. Only the restore direction is pinned (`loading_restores_...` sets the field by hand). No test ever puts the boss spawn's `defeat_flag` into `progress.flags` and asserts `SaveFile::capture` sets `boss_defeated = true` — so a wrong `boss_defeat_flag(world)` lookup (src/save.rs:336) would go unnoticed. Related: acceptance criterion 1 names "boss victory" among the fields that must survive the round trip, but `round_trip_preserves_every_field` (tests/save.rs:86) leaves `boss_defeated` at its `false` default. Impact is limited because `flags` carries the same information redundantly, so gameplay would still skip the boss.

**Fix:** In `round_trip_preserves_every_field`, insert the boss spawn's `defeat_flag` into `state.progress.flags` before capture, assert `save.game.boss_defeated` is `true`, and assert it survives the store/load.

## [MINOR] Confirming New Game over a usable slot leaves the old save as the retry target
`src/app.rs`

`apply_confirm_new_game` → `start_new_game()` (src/app.rs:224) resets `state` and `tick_counter` but leaves `self.slot` as `SlotState::Usable(old_save)`. Until the fresh run hits its first autosave trigger, a `HeroDied` → `GameOver` → `Confirm` restores the *previous* run (`apply_game_over` reads `self.slot`), silently dropping the player back into the run they just chose to abandon. The `ConfirmNewGame` overlay's "This will overwrite your saved progress." is also not yet true at that point. Practically narrow (the start room has no enemies, and the first room transition autosaves), but the state is inconsistent by construction.

**Fix:** In `start_new_game()`, set `self.slot = SlotState::Empty` when the fresh run is started deliberately from a menu path, so retry-before-first-autosave starts fresh rather than resurrecting the abandoned run. (Leave `apply_game_over`'s empty-slot branch as is — it already handles that.)

## [MINOR] The first store after a backup restore overwrites the good `.bak` with the corrupt file
`src/save.rs`

`commit` unconditionally does `fs::copy(save.json -> save.json.bak)` whenever `save.json` exists, with no check that the file it is promoting to backup is loadable. Sequence: `save.json` is corrupt, the player picks `Restore backup` (App::restore_backup, src/app.rs:278) and plays on from the good `.bak`; the next autosave's `commit` copies the still-corrupt `save.json` over `save.json.bak`, destroying the last valid copy, then renames the new good file into place. `save.json` ends up valid, so no progress is lost, but spec §10's "Зберігати попередню валідну копію" and RISKS #7's "one retained `.bak`" are weakened exactly in the recovery scenario they exist for.

**Fix:** In `commit`, probe the existing `save.json` with `load_file` first and only copy it to `.bak` when the outcome is `Ok` or `FutureVersion`; when it is `Corrupt`, skip the copy and leave the retained backup alone.

## [NIT] `in_combat`'s invulnerability term makes manual save unreachable while paused after a load
`src/game/state.rs`

`in_combat()` returns true while `self.tick < self.hero.invuln_until`. A load sets `invuln_until = LOAD_SAFE_WINDOW_TICKS` (60) at `tick = 0`, and `Mode::Paused` freezes the clock, so a player who loads and immediately pauses gets "Cannot save during combat." with no way to clear it except resuming and playing for 2 s. Harmless, and the conservative reading is the right one per DECISIONS, but the post-load window is not really "combat".

**Fix:** Optional: track the post-load safe window as a separate field from combat invulnerability, or accept it and note the behaviour in docs/user/controls.md's Save bullet (which currently says the refusal means "the hero is still invulnerable from a recent hit").

# Review — phase 5 round 2

**Verdict:** approve

Phase 5 is complete and the phase gate passes (fmt + clippy -D warnings + cargo test, exit 0, re-run during this review). Every acceptance criterion maps to a test that can actually fail: tests/playthrough.rs drives New Game to GameWon with only Actions and verifies the milestone order from the real event stream; tests/dungeon.rs covers key consumption, the free return trip (plus a synthetic two-sided-lock fixture for the reciprocal path) and the §2 content table; tests/puzzles.rs covers both new puzzle kinds, the reset rule, all six torch permutations and a BFS-exhaustive sweep of reachable block positions; tests/boss.rs covers the phase threshold, per-phase patterns observed in-game, per-strike telegraph lead time across both phases including phase 2's exact 18-tick floor, vulnerability-only damage, no contact damage and a no-damage scripted kill; tests/content.rs validates the 15-room world with six new negative fixtures. All five MAJOR and all three MINOR findings from REVIEW-r1.md are genuinely fixed in the code, not just in DECISIONS.md. PLAN.md's 16 tasks all match what the diff contains, the three architecture deviations are logged with rationale, and docs/user/controls.md, docs/dev/ and CHANGELOG.md all cover the user-visible surface. No blockers or majors; the remaining items are consistency and coverage polish.

## [MINOR] The playthrough would still report GameWon with a dead hero
`tests/playthrough.rs`

`game::update` never gates actions on `hero.died` (src/game/state.rs:521 only sets the flag and emits `HeroDied`); only `App` reacts by switching to `Mode::GameOver`. `tests/playthrough.rs` drives `Runner`, i.e. raw `GameState`/`update`, and asserts nothing about health or `died`. So if a tuning or content change made the flooded-hall/torch-vault stretch lethal at the run's fixed seed, the hero would keep walking, relight the beacon, and both tests would stay green. The criterion the phase is selling — the game is completable — is only proved navigable, not survivable.

**Fix:** Add `assert!(!runner.state.hero.died)` and `assert!(runner.state.hero.health_halves > 0)` after the `GameWon` assertion (and ideally record the minimum health seen along the route), so a lethality regression on the main route fails the playthrough.

## [MINOR] Knockback ignores blocks, so the hero can be knocked onto a block's tile
`src/game/combat.rs`

`apply_contact_damage` (src/game/combat.rs:286) and `apply_boss_strike` (src/game/combat.rs:222) build `occupied` from live enemies only and never add `state.puzzle.blocks`; `knockback` (src/game/combat.rs:161) checks tile + `room.object_at` + `occupied`, and a `Block` is deliberately not an `ObjectKind`. Reachable in shipped content: in `room.plate_chamber` a slime hits the hero standing next to `block.chamber` from the opposite side and the hero lands on the block's tile — the exact hero/solid overlap the round-1 fix just re-established for movement (`GameState::walkable`'s `block_free`) and enemy pathing (`ai::occupied_positions`). Cosmetic today (the hero glyph is drawn last, and the hero can walk off), but the "a block is solid" invariant now holds in three of four places.

**Fix:** Extend `occupied` in both damage paths with `state.puzzle.blocks.iter().copied()` before calling `knockback`, and add a case to `tests/puzzles.rs::a_block_blocks_the_hero_and_enemies` that knocks the hero toward a block and asserts the hero never ends on the block's tile.

## [MINOR] The validator proves the lighthouse room reachable with the ember, but never that the beacon is approachable
`src/content/validate.rs`

`check_beacon` (src/content/validate.rs:~512) only asserts exactly one beacon and that it sits in `route.home`; `check_reachability_and_route` (src/content/validate.rs:1374) then checks `home_reachable_with_ember` at *room* granularity. A beacon authored inside a walled-off pocket of `room.lighthouse` would pass validation while the game could never be finished — precisely the RISKS #1 class the tile-level BFS exists to rule out. Chests, NPCs and the boss drop all already use the stronger "an orthogonal neighbour is in `reached`" test; the beacon is the one interactable left out.

**Fix:** In the ember-carrying states, require `orthogonal_neighbours(beacon.at)` to intersect `reached` for the beacon's room, reusing the chest/boss machinery, and report `HomeUnreachableWithEmber` (or a new `BeaconUnreachable`) otherwise.

## [MINOR] `PuzzleState.sequence` is per-room but a room may author two TorchSequence puzzles
`src/game/puzzles.rs`

`PuzzleState.sequence` (src/game/puzzles.rs:23) is a single per-room vec, and `handle_use_lantern` (src/game/state.rs:664) indexes `puzzle.torches` by `state.puzzle.sequence.len()`. `check_puzzles` (src/content/validate.rs:441) only forbids a torch being claimed by two puzzles — nothing forbids two distinct `TorchSequence` puzzles in one room, in which case both would read and clear the same vec and interleave into states neither puzzle's author intended, while the reachability rule (which ignores order) would still declare both solvable. No current content hits this; the validator is exactly the layer meant to keep it that way.

**Fix:** Either add a `check_puzzles` rule rejecting more than one `TorchSequence` per room, or key the sequence per puzzle (`BTreeMap<u16, Vec<u16>>`) in `PuzzleState`.

## [MINOR] Five new validator rules have no test that makes them fire, and the known-gaps doc wasn't updated
`src/content/validate.rs`

`PuzzleObjectClaimedTwice` (src/content/validate.rs:465,473), `BossDropMissingFlag` (:494), `MultipleBosses` (:506), `BeaconMissing` (:521) and `MultipleBeacons` (:523) are only covered by `error.rs`'s `Display` table — no fixture or unit test makes the validator produce them, so any of those conditions could be inverted or dropped and the suite stays green. `docs/dev/testing.md`'s known-gaps paragraph still lists only the older untested variants (`TooManySmallKeyDoors`, `EnemyPatrolInvalid`), so the gap is now larger than the doc claims.

**Fix:** Add fixtures (or `validate.rs` unit tests built from schema types, as `more_than_64_smallkey_doors...` already does) for at least `MultipleBosses`, `MultipleBeacons`/`BeaconMissing` and `PuzzleObjectClaimedTwice`, and extend the known-gaps list in `docs/dev/testing.md` with whatever stays uncovered.

## [MINOR] A block pushed one tile past the plate seals the plate chamber's east exit until re-entry
`assets/world.ron`

In `room.plate_chamber` the only route east is the revealed `Hidden` tile at (15,8). `block_push_target` legalises only plain `Tile::Floor`, so a revealed passage tile is never a legal push target. After solving, the player can push the block from the plate (13,8) to (14,8) — the single corridor tile before the passage — and it can then never be pushed east out of the way, blocking the east door until the player walks out west and back in. Recoverable (the reset rule), so not a soft-lock, but `tests/puzzles.rs::every_reachable_wrong_block_position_still_leaves_the_room_solvable` enumerates only the pre-solve state and only asserts the *west* approach, so this configuration is outside its coverage.

**Fix:** Extend that test with the solved configuration (reveal applied) and assert the west approach stays reachable from every block position there too; optionally author a second floor lane around (14,8) so the chokepoint cannot be plugged at all.

## [NIT] `enter_room`'s doc comment claims an index invariant the boss filter breaks
`src/game/state.rs`

src/game/state.rs:102-103 says "`EnemyId(i)` is always `enemies[i]`", but `enter_room` now filters out a boss whose `defeat_flag` is set, so after a defeated boss the vector index and the id diverge. The ids themselves are still authored indices (enumerate precedes filter), which is what `combat.rs:91`'s `room.enemies[enemy.id.0]` relies on — only the comment is wrong.

**Fix:** Reword to "`EnemyId(i)` is the authored spawn index `room.enemies[i]`; the live vector may be shorter when a defeated boss is skipped".

## [NIT] The full route script is duplicated verbatim across both tests
`tests/playthrough.rs`

`new_game_reaches_game_won_using_only_actions` (lines 190-279) and `the_run_finishes_inside_a_generous_tick_ceiling` (lines 371-446) repeat the same ~60-line walk/cross/interact sequence. Any content or layout change has to be made twice, and the two copies can silently drift.

**Fix:** Extract the route into one `fn run_full_route(runner: &mut Runner) -> Vec<GameEvent>` and have both tests call it, asserting their own property afterwards.

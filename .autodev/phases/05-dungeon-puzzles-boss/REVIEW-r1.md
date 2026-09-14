# Review — phase 5 round 1

**Verdict:** changes_requested

Phase 5 delivers the full dungeon, keys, both new puzzle kinds, the two-phase boss and the ending; the gate passes locally (fmt + clippy -D warnings + cargo test, exit 0) and the headless playthrough genuinely drives New Game to GameWon with an ordered milestone assertion. Five issues block approval. One is a real simulation bug: pushable blocks are solid to the hero but invisible to enemy AI (ai::occupied_positions/tile_walkable never read state.puzzle.blocks), contradicting PLAN T2, letting enemies stand on a block in room.plate_chamber and letting a push drop the hero onto an enemy's tile. The other four are tests that do not prove the criteria they are named for: each_phase_uses_a_distinct_attack_pattern compares two hand-built tile sets and cannot fail on a boss_pattern_for_phase bug; the telegraph test returns on the first phase-1 strike and ignores the tile payloads the events carry for exactly that comparison, leaving phase 2's boundary-value windup untested; both "every wrong state" tests drive a single sample each; and PLAN T10's two Victory mode-machine tests were never written, so Mode::Victory's wiring is untested. Three minor items: the validator's flood walks through authored block tiles, two unreachable!() arms in the boss AI break game/'s never-panics contract, and the lantern on a solved sequence torch gives no feedback. Documentation, DECISIONS.md deviations and CHANGELOG are thorough and accurate apart from the T2 entry citing a test that does not cover what it claims.

## [MAJOR] Blocks are not solid to enemies; pushing into an occupied block tile overlaps hero and enemy
`src/game/ai.rs`

`GameState::walkable` (src/game/state.rs:176) treats a current-room block as solid, but `ai::occupied_positions` (src/game/ai.rs:40) and `ai::tile_walkable` (src/game/ai.rs:30) never consult `state.puzzle.blocks`. So enemies walk straight through and stand on a pushable block. PLAN T2 explicitly required "`walkable` and `ai::occupied_positions` treat a current-room block as solid"; only half of that shipped, and the DECISIONS.md `[phase-05/t2]` entry claims `tests/puzzles.rs::a_block_blocks_the_hero_and_enemies` caught it — that test (tests/puzzles.rs:137) asserts only `state.walkable`, never an enemy, so its name overclaims and it cannot detect this.

Second-order defect: `try_push_block` (src/game/state.rs:901) passes live-enemy positions to `block_push_target` for the *push target* but never checks whether an enemy stands on `block_pos` before moving the hero there. With a slime parked on the block, one MoveEast puts the hero and the slime on the same tile — reintroducing the overlap the round-2 review fixed for ordinary steps (see `update`'s doc comment at src/game/state.rs:406).

Reachable in shipped content: `room.plate_chamber` spawns `(kind: Slime, at: (7,4))` alongside `block.chamber` at (9,8) in an open room.

**Fix:** Add block positions to `ai::occupied_positions` (or to `tile_walkable` via a blocks parameter) so enemies treat a block as solid, and reject the push in `try_push_block` when a live enemy occupies `block_pos`. Extend `a_block_blocks_the_hero_and_enemies` to actually step an enemy into the block tile and assert it does not move, plus a case that pushes into a block an enemy stands on and asserts hero and enemy never share a tile.

## [MAJOR] `each_phase_uses_a_distinct_attack_pattern` cannot fail on a phase/pattern bug
`tests/boss.rs`

tests/boss.rs:209 builds `strike_tiles(BossPattern::Slam, pos, room)` and `strike_tiles(BossPattern::Sweep, pos, room)` by hand and asserts the two sets differ. It never observes which pattern the boss actually uses in phase 1 vs phase 2. `ai::boss_pattern_for_phase` (src/game/ai.rs:431) could be inverted, or return a constant, and this test still passes — it restates the implementation of `strike_tiles` rather than proving the acceptance criterion "each phase has a distinct attack pattern".

**Fix:** Drive the boss to a phase-1 windup, capture the `BossTelegraph { tiles }`; force it below `BOSS_PHASE_TWO_HP`, drive it to a phase-2 windup, capture those tiles; assert the two observed tile sets differ and match `strike_tiles(Slam, ..)` / `strike_tiles(Sweep, ..)` respectively at the boss's recorded position.

## [MAJOR] Telegraph criterion proven for one phase-1 strike only, and the tile sets are never compared
`tests/boss.rs`

The criterion is "a telegraph precedes **every** dangerous action by at least the tuned warning". `every_strike_is_preceded_by_a_telegraph_of_at_least_the_tuned_warning` (tests/boss.rs:230) `return`s on the very first `BossStruck`, so it only covers phase 1 (windup 24 ticks, comfortably above the 18-tick floor). Phase 2 — whose `BOSS_P2_WINDUP_TICKS` equals `TELEGRAPH_MIN_TICKS` exactly and is therefore the boundary case — is never exercised. The test also matches `BossTelegraph { .. }` / `BossStruck { .. }` and ignores the `tiles` payload, even though PLAN.md's stated rationale for putting tiles on those events was "tick of the telegraph for a tile set vs. tick of the strike on that same tile set".

**Fix:** Run the fight through both phases (damage the boss to cross the threshold mid-run) and check every `BossStruck` in the stream, not just the first: keep a map from telegraphed tile set to its tick and assert each strike's tile set was telegraphed at least `TELEGRAPH_MIN_TICKS` earlier.

## [MAJOR] "Every wrong state" tests drive one wrong state each, not every one
`tests/puzzles.rs`

The criterion is "A test drives each puzzle into **every** wrong state it allows and asserts the room is still solvable afterwards", and PLAN T5 promised "drive the block to each reachable wrong tile". `every_reachable_wrong_block_position_still_leaves_the_room_solvable` (tests/puzzles.rs:333) drives the block to exactly one position, (9,6), and recovers via re-entry. `every_wrong_torch_order_still_leaves_the_puzzle_solvable` (tests/puzzles.rs:212) tries four hand-picked sequences out of the six orderings plus their prefixes. Both are samples. The exhaustive form is cheap here and is what would actually catch a wrong position that seals the room — e.g. a block parked at (1,8) in `room.plate_chamber`, on the only approach to the west exit door.

**Fix:** For blocks: BFS the reachable `(block_pos, hero_pos)` space of `room.plate_chamber` (the validator's `block_puzzle_solvable` already has the machinery), then for each reachable block position assert either that the plate is still pushable from there or that exit + re-entry restores the authored state — and assert the exit door's approach tile is reachable in every one. For torches: iterate all permutations of the three torch ids and assert each leaves the puzzle solvable.

## [MAJOR] PLAN T10 marked [x] but its Victory mode-machine tests were never written
`src/app.rs`

T10 promises `tests/mode_machine.rs` — `game_won_enters_victory_mode_and_stops_simulating`, `victory_confirm_returns_to_the_main_menu`. `tests/mode_machine.rs` is absent from this phase's diff entirely; `grep -rn "Victory" tests/` finds only `tests/render.rs:454`, which sets `app.mode = Mode::Victory` by hand to render the overlay. So the phase's headline ending is untested at the `App` layer: that `GameEvent::GameWon` actually sets `Mode::Victory` (src/app.rs:319), that `simulating()` then stops advancing `tick_counter`, and that `apply_victory` (src/app.rs:250) returns to the main menu — none is covered. The playthrough only asserts the `GameWon` *event*, below `App`.

**Fix:** Add the two promised tests to `tests/mode_machine.rs`: drive an `App` to a `GameWon` event and assert `mode == Mode::Victory` and that `tick_counter` stops advancing across subsequent `tick(&[])` calls; then feed `Action::Confirm` and `Action::Cancel` and assert both land on `Mode::MainMenu`.

## [MINOR] The reachability flood walks through authored block tiles
`src/content/validate.rs`

`ReachabilitySearch::walkable_for_search` (src/content/validate.rs:965) checks tile + `room.object_at`, but a `Block` is deliberately not an `ObjectKind`, so the flood treats a block's authored reset position as ordinary floor. The simulation refuses that tile (`GameState::walkable`'s `block_free` term) until the block is pushed. That makes the flood optimistic on exactly the property RISKS #1 is about: a block authored in a one-tile corridor would let the validator declare a route reachable that a player cannot walk. Harmless in the current world (the block sits in an open chamber with a row-7 bypass), but the check is unsound in general.

**Fix:** Treat a block's authored position as solid in `walkable_for_search` and let `block_puzzle_solvable` remain the mechanism that proves the block can be moved out of the way — or, if the pessimistic direction is too strict, model the block position explicitly in the flood state.

## [MINOR] `unreachable!()` in the boss AI can panic the simulation
`src/game/ai.rs`

`boss_ai_phase` (src/game/ai.rs:437) and `step_boss`'s final match arm panic if a `EnemyKind::Boss` enemy ever carries a non-boss `AiState`. `game::update`'s own contract is "Total: never panics" (src/game/state.rs:394), and every other invariant in this module is written total. Today it is safe by construction — only `initial_ai_state` writes a Boss's initial state — but phase 6 deserializes `AiState` from a save file, where a corrupt or hand-edited slot would reach this.

**Fix:** Replace both `unreachable!()` arms with a recovery: reset to `AiState::BossStalk { phase: 1, until: tick + BOSS_P1_STALK_TICKS }` and carry on, keeping the simulation total.

## [MINOR] Lantern on a torch of an already-solved sequence gives no feedback at all
`src/game/state.rs`

In `handle_use_lantern`, the solved-puzzle branch (`if state.progress.solved_puzzles.contains(&obj) { return events; }`, around src/game/state.rs:664) returns an empty event vec. Every other dead-end lantern press emits a `Message` ("You have no lantern.", and the wrong-order "The flames gutter out."). A player pressing K on a solved vault torch sees nothing happen and no explanation.

**Fix:** Emit a short `GameEvent::Message` (e.g. "The flames are already steady.") in that branch, matching the feedback every other lantern outcome gives.

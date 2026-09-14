# Review — phase 4 round 2

**Verdict:** changes_requested

Phase 4 delivers the overworld content, the interaction/lantern/lock/puzzle mechanics, the three new modes and a substantially extended validator, and the full gate passes cleanly. The simulation stays pure, `GameState::walkable` is threaded consistently through hero movement, enemy AI and knockback, the dialogue-suspends-`update` rule is properly proven, and every new validator check has a fixture. Deviations from ARCHITECTURE are logged in DECISIONS.md. Four findings block approval, all in the test layer rather than the product: criterion 2's reachability clause is asserted only as a two-room spot check that a re-added door would not trip; PLAN T2's promised `state_hash` guard rows for the nine new hashed fields were never added, so the save/determinism hash surface is unprotected; the map test never checks that an unvisited room is blank, leaving half of criterion 6 unproven; and the claimed RISKS #10 mitigation rests on two tests that cannot fail plus a theme scene that contains none of the new objects. Beyond those: `route.goal` is a new authored reference with no validator check, the map paints `C` on all three undiscovered secrets, and chest pickups produce no message-row feedback. Note that no `REVIEW-r1.md` exists for this phase, so there were no prior blocker/major findings to re-verify.

## [MAJOR] Criterion 2's reachability half is not tested — the test name overstates what it asserts
`tests/overworld.rs`

`start_room_and_crossroads_have_zero_enemies_and_no_enemy_is_reachable_before_the_sword` (tests/overworld.rs:59-68) only asserts that two hardcoded rooms (`room.lighthouse`, `room.crossroads`) carry zero authored enemy spawns. It performs no reachability computation at all, so the second clause of its own name — and of acceptance criterion 2 — is unproven. Re-adding `door.lighthouse.east` (which DECISIONS.md `[plan/04]` records as removed precisely to satisfy this criterion) would put `room.south_shore`'s slime one step from the start spawn, and this test would still pass. RISKS #2 says the ramp must not be silently regressible; as written it is.

**Fix:** BFS the room-adjacency graph from `world.start_room()` treating the room holding the non-secret `Reward::Sword` chest as a frontier that is entered but not expanded past, then assert every room reached has `room.enemies.is_empty()`. That makes the criterion structural rather than a two-room spot check.

## [MAJOR] `state_hash`'s table-driven guard was not extended with the nine new hashed fields, though PLAN T2 is marked [x]
`src/game/state.rs`

PLAN.md T2 says "Extend the table-driven `mutating_any_hashed_field_changes_the_hash` test with one row per new field" and is checked off. The table at src/game/state.rs:948-1014 still ends at `progress.visited` — there is no row for `hero.has_sword`, `has_lantern`, `has_ember`, `progress.opened_chests`, `lit_torches`, `solved_puzzles`, `flags`, `state.dialogue`, or `state.plates.pressed`, all nine of which `state_hash` now writes (lines 809-887). The test's own doc comment claims "a field added to `state_hash` later without a mutation here fails loudly instead of the hash silently ignoring a real state change" — that claim is now false. No other test covers it: `tests/determinism.rs` only compares traces of identical runs, so deleting `h.write_u32(state.progress.flags.len()...)` and its loop from `state_hash` leaves the whole suite green. This is the hash phase 6 builds the save/replay comparison on.

**Fix:** Add one `GameStateMutation` row per new hashed field (e.g. `("progress.flags", Box::new(|s| { s.progress.flags.insert("flag.x".into()); }))`, `("hero.has_sword", ...)`, `("plates.pressed", ...)`, `("dialogue", ...)`) so the doc comment's promise actually holds.

## [MAJOR] Map test never asserts an unvisited room is blank — half of criterion 6 is unproven
`tests/render.rs`

`map_overlay_hides_unvisited_rooms_and_marks_the_current_one` (tests/render.rs:216-238) asserts the title, the current room's name, the legend words, and `cell(&buf, 12, 9) == "@"`. Nothing asserts that any other grid cell is blank. Because only `room.lighthouse` is visited at that point, the buffer cannot distinguish "hides unvisited rooms" from "renders every room"; if `draw_map`'s `if !state.progress.visited.contains(&room_idx) { continue; }` (src/render/overlays.rs:110-112) were deleted, this test would still pass. Spec §(docs/spec.md:210) — "Невідвідані кімнати приховані" — and acceptance criterion 6 both name this behaviour explicitly.

**Fix:** Assert a neighbouring cell is blank before it is visited and non-blank after: e.g. `assert_eq!(cell(&buf, 12, 8), " ")` for `room.crossroads` (map_index (1,1)) in the fresh-game buffer, then re-render after `cross_door`ing north and assert that same cell now renders `C` (crossroads holds the unopened sword chest).

## [MAJOR] The RISKS #10 glyph-distinguishability mitigation rests on two tests that cannot fail
`src/render/tiles.rs`

PLAN.md's risk table claims RISKS #10 (monochrome unreadable) "stays structurally satisfied" because "Each added object type is distinguishable by glyph alone" and "the existing 'identical characters under every theme' test is extended to a scene holding the new objects". Neither test backs that.

1. `src/render/tiles.rs::every_kind_has_a_glyph` asserts `glyph(kind) != '\0'` over an exhaustive match of char literals, none of which is `'\0'` — it cannot fail, and in particular it does not check the 20 glyphs are distinct. (They are, in fact, distinct today; nothing protects that.)
2. `src/render/theme.rs::every_theme_uses_the_identical_glyph_table` is `let _ = theme.color_for(kind); let _ = glyph(kind);` — zero assertions.
3. `tests/render.rs::scene_renders_identical_characters_under_every_theme` uses `combat_scene_setup`, which is `room.lighthouse` plus two hand-placed enemies. The only new object in that room is `npc.keeper` at (5,5), and the test's own slime is placed at (5,5) too, painting over it (src/render/scene.rs paints objects before enemies). No chest, torch, plate, or `ChestOpen`/`TorchLit`/`PlatePressed` variant ever reaches the buffer, so the scene was not extended to hold the new objects.

All seven new kinds were added to the two `ALL_KINDS` arrays, which gives the appearance of coverage without adding any.

**Fix:** In `tiles.rs`, replace `every_kind_has_a_glyph` with a uniqueness assertion (collect `ALL_KINDS.map(glyph)` into a `HashSet` and assert `len() == ALL_KINDS.len()`). Point `scene_renders_identical_characters_under_every_theme` at a state in `room.old_mill` with one plate pressed, the puzzle solved (so a revealed `Hidden` tile renders as floor) and `chest.mill_lantern` opened, so every new glyph and its state variant is actually on screen under all three themes.

## [MINOR] `route.goal` is a new authored room reference with no validator check
`src/content/validate.rs`

`check_transitions` (src/content/validate.rs:154-159) validates `route.home` resolves to a room but not `route.goal`, which this phase added to `Route`. `main_route_rooms` swallows an unresolvable goal silently (`if let Some(goal) = world.room_idx(&world.route.goal)`, line 453). A typo in `route.goal` therefore shrinks the main-route set with no diagnostic, which false-accepts secrets in `check_secrets` — the exact class of silent content defect RISKS #1 and the plan's "every new authored reference gets a validator check in this phase" rule exist to prevent.

**Fix:** Add the mirror of the `route.home` check: `if world.room_idx(&world.route.goal).is_none() { errors.push(ContentError::UnknownRoom { referenced_by: "route.goal".to_string(), room: world.route.goal.clone() }) }`, with a fixture.

## [MINOR] The map marks secret chests the player has not discovered
`src/render/overlays.rs`

`draw_map`'s `has_unopened_chest` (src/render/overlays.rs:114-121) counts every chest in a visited room, including `secret: true` ones sitting behind an unlit torch. Visiting `room.north_ridge`, `room.fallen_pines` or `room.south_shore` therefore paints `C` on the map before the player has any way to know a chest is there, pointing at all three secrets. Spec docs/spec.md:210 says the map shows "відкриті важливі об'єкти" — discovered objects — not undiscovered ones.

**Fix:** Skip secret chests, or better, count a chest only when its tile is currently adjacent to walkable space (i.e. its alcove has been revealed): `!c.secret || state.is_revealed(...)`. Skipping `c.secret` is the one-line version.

## [MINOR] Opening a chest produces no message row feedback for item rewards
`src/game/state.rs`

PLAN.md's interaction table specifies `Interact` on an unopened chest emits `ChestOpened` + `ItemPicked` + `Message`. `handle_interact` (src/game/state.rs:502-517) emits only `ChestOpened` plus whatever `apply_reward` returns, and `apply_reward` (line 584) returns a `Message` only for `Reward::Message`. Since `App::tick` writes `self.message` only on `GameEvent::Message` (src/app.rs:308), taking the sword, the lantern, a key or a heart container leaves the message row showing whatever was there before — the HUD `Item:` slot and the `C`→`c` glyph are the only feedback, and neither is where the game has been teaching the player to look (the start room's `hint` uses that row).

**Fix:** Have `apply_reward` also return a `GameEvent::Message` naming the pickup ("You take the forest sword." etc.), or push one from `handle_interact` after `apply_reward`.

## [MINOR] Criterion 1's "first slime encounter possible" milestone is not asserted, and PLAN.md's verification table names tests that do not exist
`tests/overworld.rs`

Acceptance criterion 1 lists five milestones in order: NPC dialogue flag set, sword acquired, *first slime encounter possible*, puzzle solved, lantern acquired. `main_route_milestones_happen_in_order` (tests/overworld.rs:451-584) asserts the other four but nothing about a slime encounter — the walked route is lighthouse → crossroads → stone_circle (bats) → old_mill → east_marsh, and the first slime room, `room.west_grove`, is never entered or referenced.

Separately, six of the ten rows in PLAN.md's verification table name functions that do not exist under those names (`no_enemy_is_reachable_before_the_sword`, `lantern_lights_only_the_faced_torch`, `hidden_passage_opens_only_after_its_torch`, `chest_opens_once`, `reopening_a_chest_yields_nothing`, `three_secrets_exist_reachable_and_off_the_main_route`, and `dialogue.rs::closing_a_dialogue_drops_a_queued_move` which actually lives in `mode_machine.rs`). The table is the traceability artifact the next phase reads.

**Fix:** After taking the sword, assert the sword-carrying hero can reach an authored slime spawn (e.g. assert `room.west_grove`/`room.east_marsh` has a `Slime` spawn and is one door from the current room), and reconcile PLAN.md's verification table with the actual test names.

## [MINOR] The room-local BFS is implemented twice, verbatim
`tests/common/mod.rs`

`next_step(app: &App, target)` (tests/common/mod.rs:88-128) and `next_step_in(state: &GameState, target)` (lines 150-190) are ~40 lines each and differ only in whether they read `app.state` or `state`. Two copies of the driver's pathfinder will drift, and phase 5's `tests/playthrough.rs` is explicitly meant to reuse this module unchanged.

**Fix:** Delete `next_step` and make `walk_to` call `next_step_in(&app.state, target)`.

## [NIT] Stale comments describing pre-phase-4 behaviour in changed code
`src/content/schema.rs`

`Tile::is_walkable`'s doc (src/content/schema.rs:67) still reads "`Hidden` is inert until phase 4: not walkable, no reveal mechanic yet" — the reveal mechanic now exists, and the sentence no longer explains why this predicate deliberately stays reveal-blind (that reason is now in `GameState::walkable`). Relatedly, `combat::knockback`'s doc still lists "a hazard" among the stopping conditions, but the `!t.is_hazard()` term was dropped from the predicate in this diff (src/game/combat.rs:140-143); it is harmless today because `is_walkable` (Floor/Door/Stairs) and `is_hazard` (Water/Pit) are disjoint, but the comment now describes a check that is not there.

**Fix:** Reword both comments to state the current rule and why (content-level predicate vs. simulation-level `GameState::walkable`; hazards excluded implicitly because no hazard tile is walkable).

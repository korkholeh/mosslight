# Review — phase 3 round 2

**Verdict:** changes_requested

Phase 3 scope is fully delivered and the gate passes (I ran fmt, clippy -D warnings and cargo test --locked myself: exit 0). Every acceptance criterion has a named, non-vacuous test, and all round-1 blocker/major/minor findings are genuinely fixed rather than papered over (AttackEnded event on swing expiry, swing cleared at a door transition, discriminant-based liveness comparison, seed-reaches-gameplay divergence test, room-entry checkpoint test at reduced health, path_step bounds guard, controls.md wording, telegraph-lane predicate). One new major: contact knockback computes the direction toward the attacker instead of away, and because the attacker's tile is in the occupancy set, knockback is a no-op in every reachable configuration — a behaviour CHANGELOG, PLAN and DECISIONS all claim works, with no test covering the call site. Minors: SlimeWander is a byte-identical clone of SlimeIdle so the liveness criterion is met by a discriminant rename rather than real alternation; different_seeds_diverge no longer asserts hashes differ (criterion 10's literal wording); the hero can walk onto a live enemy's tile and hide it; HeroDied re-fires every tick at zero health.

## [MAJOR] Contact knockback is inverted and therefore never happens
`src/game/combat.rs`

direction_away(from, toward, fallback) returns the direction from `from` TOWARD `toward` (its own doc comment says so), but apply_contact_damage calls it as direction_away(hero_pos, source_pos, hero.facing) at src/game/combat.rs:184 and knocks the hero in that direction. With an enemy east of the hero it returns East — straight into the enemy. Since `occupied` (src/game/combat.rs:177-183) contains every live enemy position (only the hero's own tile is removed), knockback() (src/game/combat.rs:133-145) hits the occupied check on its first step and returns `from` unchanged. Enemies never enter the hero's tile, so the dx==0 && dy==0 fallback is only reachable when the hero walked onto an enemy, where it pushes the hero forward along their own facing. Net effect: contact knockback is dead in every reachable configuration, while CHANGELOG.md ('knocks the hero back'), PLAN.md pipeline step 4 and the DECISIONS contact-damage entry all state it works. tests/combat.rs::contact_damage_costs_one_half_heart_and_grants_invulnerability asserts health and invulnerability only, never hero.pos; knockback_stops_adjacent_to_an_obstacle calls combat::knockback directly, so neither test exercises the call site.

**Fix:** Invert the direction at the call site (or in the helper, renaming it to match): `let facing = direction_away(source_pos, hero_pos, state.hero.facing);`. Add a test placing a slime east of the hero on open floor asserting the hero's x decreased by KNOCKBACK_TILES after the contact tick.

## [MINOR] SlimeWander is an exact clone of SlimeIdle, so slime 'state changes' are cosmetic
`src/game/ai.rs`

The SlimeIdle arm (src/game/ai.rs:213-230) and the SlimeWander arm (src/game/ai.rs:231-248) are line-for-line identical: same random_facing/try_move, same SLIME_STEP_TICKS re-arm, same aggro test, differing only in which variant they flip to. tests/ai.rs::each_kind_keeps_changing_state_for_1000_ticks now compares discriminants, so it is satisfied by that flip even though nothing about the slime's behaviour changes. Gameplay is fine (the slime random-walks every 12 ticks, so RISKS #14 is mitigated in substance), but criterion 6's liveness assertion is vacuous for the slime and two identical arms invite divergence on the next edit.

**Fix:** Either give SlimeWander distinct behaviour (commit to one drawn direction for the whole wander phase, or attempt a chase step regardless of range), or drop the variant and have the liveness test assert what the slime actually does — position changed at least once per MAX_STALL_TICKS.

## [MINOR] different_seeds_diverge no longer asserts hashes differ
`tests/determinism.rs`

Criterion 10 is worded 'two different seeds over the same action sequence and asserts the hashes differ'. The round-1 fix replaced the hash comparison with an enemy-position comparison (tests/determinism.rs:95-98) — the stronger property — but state_hash() is now entirely absent from this test. Position divergence does imply hash divergence (positions are hashed at src/game/state.rs:475), but that implication is inferred rather than checked; if state_hash later stops covering an enemy field, nothing here notices.

**Fix:** Keep the position assertion and add the hash one beside it: capture hash traces for seeds 7 and 8 from the same west_grove setup and assert they differ at some tick.

## [MINOR] The hero can walk onto a live enemy's tile
`src/game/state.rs`

Hero movement (src/game/state.rs:246-252) checks Tile::is_walkable only; nothing consults enemy occupancy, while ai::try_move refuses the hero's tile. So the hero can step onto a slime and stand there: draw_scene paints enemies before the hero (src/render/scene.rs:117-121), so the enemy vanishes under the hero glyph, and apply_contact_damage then takes the dx==0/dy==0 branch and knocks the hero along their own facing. Spec §6 says solid objects block movement and contested tiles resolve deterministically; the overlap is deterministic but reads as a rendering bug.

**Fix:** Treat a live enemy's tile as blocked for hero movement (emit MoveBlocked as for a wall), or state in PLAN/DECISIONS that overlap is allowed and make the renderer show it.

## [NIT] HeroDied is emitted on every tick while health is zero
`src/game/state.rs`

src/game/state.rs:292-294 pushes HeroDied whenever health_halves == 0, not once on the transition, so any caller that keeps calling update() after death (a headless test, a future replay tool) gets one HeroDied per tick. App::tick masks this because Mode::GameOver stops the simulation, and PLAN.md step 5 says 'emits HeroDied once'.

**Fix:** Emit it only on the transition — track that the hero was alive at the start of the tick, or gate on health having just reached zero.

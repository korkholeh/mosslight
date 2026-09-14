# Review — phase 3 round 1

**Verdict:** changes_requested

Phase 3 lands the full scope: combat.rs and ai.rs as pure modules, three readable per-kind state machines, a bounded room-local BFS with a total fallback, FNV-1a state_hash(), Mode::GameOver with a tick-rewinding checkpoint, check_enemy_spawns plus a rejecting fixture, and enemy spawns encoding the §7 pacing ramp. All 16 PLAN tasks are checked and genuinely present; deviations from ARCHITECTURE.md (EnemyKind rename, flat AiState, four extra GameEvent variants) are logged in DECISIONS.md. The gate passes as claimed — I ran fmt, clippy -D warnings and cargo test --locked myself: 124 tests, 0 failures. Every acceptance criterion has a named test. Three majors, all verified by reading the code paths end to end. (1) Swing expiry emits no GameEvent, and App::tick sets dirty only on a non-empty event list, so in the two rooms this phase deliberately leaves enemy-free the sword glyph stays painted after the swing is gone. (2) The door-transition early return in update() does not clear hero.attack, so a swing crosses into the next room carrying a stale target tile and stale EnemyIds — it can damage a new-room enemy standing on an unrelated tile and makes the enemy at the matching index immune. (3) The 1000-tick liveness test compares the whole AiState including its `until` timer, so it cannot detect an enemy that never leaves a state — and the slime does exactly that whenever the hero is outside its aggro radius, which is most of the maze fixture, contradicting the alternation DECISIONS.md claims. Minors cover a vacuous different-seeds assertion, the untested checkpoint-on-RoomEntered path, a panic-capable index in a module documented as total, and a docs line that overstates what retry restores.

## [MAJOR] Swing expiry emits no event, so the sword glyph sticks on screen
`src/game/state.rs`

`combat::expire_swing` clears `hero.attack` silently and `update` returns no event for it. `App::tick` sets `dirty` only when `update` returns a non-empty event list (src/app.rs:243) and `main.rs:182` draws only `if due.draw && app.take_dirty()`. Concrete repro: in `room.lighthouse` or `room.crossroads` — which by the phase's own pacing table author zero enemies — the hero presses Attack at tick t. Tick t emits `AttackSwung` -> dirty -> the frame with `/` at `swing.at` is drawn. Ticks t+1, t+2 produce no events. At tick t+3 `expire_swing` sets `hero.attack = None` and still produces no event, so no redraw happens. If the player then stops pressing keys, the `/` glyph remains on screen indefinitely while `hero.attack` is `None` — a render that no longer matches state. This is exactly the failure mode PLAN.md's own `GameEvent` rationale describes ("must announce itself or the frame is never redrawn"), applied to every event except swing end. The scene-render test never exercises it because it sets `hero.attack` by hand and draws once.

**Fix:** Make `expire_swing` report the change: return `bool` (or emit a `GameEvent::AttackEnded`) and OR it into the event list / `changed` in `state::update`, so the tick that clears the swing marks the app dirty.

## [MAJOR] Door transition leaves a live swing pointing at the old room's tile and old enemy ids
`src/game/state.rs`

`update` returns early on a door transition (src/game/state.rs:264) without clearing `hero.attack` or calling `expire_swing`, while `spawn_enemies()` rebuilds `enemies` with fresh `EnemyId(0..n)`. The swing survives into the new room carrying (a) `swing.at`, a coordinate computed from the hero's pre-transition position, and (b) `swing.hit`, ids that now refer to different enemies. Repro: tick t the hero presses Attack (active window t..t+2, cooldown to t+9); tick t+1 the hero steps onto a door. At tick t+2 `combat::resolve_swing` runs in the *new* room: any live enemy standing on the stale `at` tile takes `SWORD_DAMAGE_HP` with no swing anywhere near the hero, and the new enemy whose index is in `swing.hit` is immune to that swing. `draw_scene` also paints `Kind::Sword` at the stale tile. This contradicts the pipeline contract the same function documents ("steps 2-5 never run against a room the hero has already left").

**Fix:** Set `state.hero.attack = None;` (keeping `attack_ready_at` as-is so the cooldown still costs) in the transition branch immediately before `state.spawn_enemies(); return events;`.

## [MAJOR] Liveness test compares the AI timer, not the AI state, so it cannot detect a stalled enemy — and the slime does stall
`tests/ai.rs`

`each_kind_keeps_changing_state_for_1000_ticks` (tests/ai.rs:100) compares `state.enemies[0].ai != before` using full `PartialEq`, which includes the `until: Tick` payload. `SlimeIdle { until: 60 }` -> `SlimeIdle { until: 120 }` therefore counts as an "AiState change" even though the state machine never left `SlimeIdle`. That matters because the slime genuinely does not alternate: in `step_slime`'s `SlimeIdle` arm (src/game/ai.rs:205-215), when `tick >= until` and the hero is outside `SLIME_AGGRO_RADIUS` (5), the slime re-enters `SlimeIdle` with a refreshed timer, forever. In the maze fixture the hero sits at (1,1) and the slime starts at (10,8), Manhattan 16 — so for most or all of the 1000 ticks the slime never changes state. The implementation itself agrees the two are different: `ai::step` emits `EnemyAiChanged` only on a `discriminant` change (src/game/ai.rs:393). The test therefore passes on behaviour that does not satisfy the criterion ("never stops changing AI state"), and DECISIONS.md's claim that "each enemy machine alternates on a timer even with a motionless, out-of-range hero" is false for the slime. Note this test is the only automated guard RISKS #14 has.

**Fix:** Compare `std::mem::discriminant(&enemy.ai)` instead of the whole value (matching what `ai::step` already treats as a state change), and either give the slime a real out-of-range alternation (e.g. an explicit wander state distinct from idle, or a chase attempt on timeout regardless of range, as the guardian's unconditional `GUARDIAN_PATROL_TICKS` timeout does) or restate the criterion in PLAN.md/DECISIONS.md to what the slime actually guarantees.

## [MINOR] `different_seeds_diverge` is satisfied before the simulation runs
`tests/determinism.rs`

`state_hash` writes `state.rng.raw_state()` (src/game/state.rs:424), and `Rng::new(7)`/`Rng::new(8)` differ in that field from construction. So `hash_trace(7, ..)[0] != hash_trace(8, ..)[0]` holds unconditionally — the assertion at tests/determinism.rs:63 would pass even if the seed had zero influence on hero position, enemy positions or health. The criterion it is meant to prove ("so the determinism test cannot pass on a constant") is only satisfied in the narrowest reading; nothing here proves the seed reaches gameplay, and the 2000-action random walk is not guaranteed to reach one of the six rooms that actually author enemies.

**Fix:** Assert divergence in something the RNG has to drive, e.g. put the hero in a room with enemies up front and assert the two runs' `enemies[..].pos` (or a hash computed over everything except `rng.raw_state()`) differ at some tick.

## [MINOR] The "room-entry" half of the retry criterion is untested
`tests/mode_machine.rs`

Criterion 8 says retry restores "the checkpointed room-entry state", but `death_opens_game_over_and_retry_restores_the_checkpoint` only kills the hero in the start room and retries into the New Game checkpoint. The `GameEvent::RoomEntered => self.checkpoint = self.state.clone()` branch added in src/app.rs:249 — the part that makes the checkpoint follow the hero — has no coverage, and neither does the case where the checkpointed health is below full (the test asserts `health_halves == 6`, which is also the New Game value, so it cannot distinguish "restored from the checkpoint" from "reset to a fresh hero").

**Fix:** Walk the hero through a door, damage them, then kill them, and assert retry lands back at the second room's arrival tile with the post-entry (not full) health.

## [MINOR] `path_step` can panic on an out-of-range `Pos` in a module documented as total
`src/game/ai.rs`

`visited[from.y as usize][from.x as usize] = true` (src/game/ai.rs:49) indexes the fixed `[ROOM_H][ROOM_W]` arrays with `from` before any bounds check, unlike every subsequent `next` which is guarded at line 59. `Pos` is `u8`, so a caller passing a position outside 24x16 panics. `path_step` is `pub` and re-exported through `game::ai`, and the module header claims "Total: no unwrap, no clock" is the standard here. Same class: the `.expect("a visited non-start cell always has a recorded first step")` at line 78.

**Fix:** Return `None` early when `from` (or `to`) is outside `ROOM_W`/`ROOM_H`, and replace the `expect` with a `?`/`continue` so the function is total by construction rather than by argument.

## [MINOR] Docs say retry restores health; it restores the checkpoint's health
`docs/user/controls.md`

docs/user/controls.md:33 reads "Confirm retries from the last room the hero entered, health restored". The implementation restores `self.checkpoint`, whose `health_halves` is whatever the hero had on entering that room — possibly half a heart. CHANGELOG.md words the same change correctly ("health included"). A player entering a room at one half-heart and dying will retry at one half-heart, not at full health.

**Fix:** Change to "...retries from the last room the hero entered, with the health they had on entering it".

## [NIT] Telegraph lane and the actual dash disagree on hazards and occupancy
`src/render/scene.rs`

`telegraph_lane` (src/render/scene.rs:66) stops only at `!is_walkable`, while the dash's `try_move` (src/game/ai.rs:173) also refuses hazard tiles and occupied tiles. The danger cue can therefore promise `!` on tiles the dash will never reach — a small readability mismatch in a cue whose whole job is telling the player where not to stand.

**Fix:** Reuse the same predicate (`is_walkable() && !is_hazard()`) in `telegraph_lane`.

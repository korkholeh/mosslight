# Roadmap — Mosslight

Spec: `docs/spec.md` · generated 2026-09-13 21:46:40

Mosslight is a single-player top-down adventure game for the terminal, written in Rust with ratatui and crossterm, playable locally and over an interactive SSH session with a PTY. The repository currently contains only specification and architecture documents, so phase 1 creates the whole skeleton: the Cargo package, the pinned toolchain, CI, the terminal lifecycle guard, the fixed 30 Hz loop, and a playable room. The roadmap then follows the spec's §14 implementation order with one deviation — the content pipeline and its validator are pulled forward into phase 2, ahead of any authored world, because unfinishable content is the highest-cost risk in the register and the validator is its only real mitigation. Phases 3 to 5 build combat, the overworld and the dungeon, ending with a headless start-to-victory playthrough that proves the game is completable well before the run's time ceiling. Phases 6 and 7 add the save slot, themes and monochrome, then balance, measurement, documentation and a verification report that marks Linux, a real SSH PTY session and the RTT check as unverified rather than claiming them.

**Test command:** `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked`

## Assumptions

- test_command is the unit/integration suite runner (cargo test --locked); the combined gate `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked` is the per-phase exit condition and is recorded in ROADMAP.md and CLAUDE.md, with lint tracked separately by the orchestrator.
- --fps accepts only the three values documented in §12 ({10, 20, 30}) and rejects others with a clap error, rather than clamping silently.
- The 'at least 3 secrets' of §2 are three hidden overworld rewards reachable with the lantern, none of them on the main route, so no secret can gate progress and contradict §7's no-dead-ends rule.
- Both required puzzle kinds (block-on-plate, torch sequence) are instantiated in the dungeon; the overworld carries only the single simple pre-lantern puzzle named in §7 step 4.
- The 3 to 5 heart gap of §2 is closed by two heart containers: one placed as a secret, one as a dungeon reward.
- Death and retry in phase 3 restore from an in-memory checkpoint taken on room entry; phase 6 repoints the restore at the save file, since §14 places the save system after combat but §13 requires a death-and-restore test as soon as combat exists.
- The full §12 flag surface is parsed in phase 1 so that Config's shape does not churn; only the presentation semantics of those flags (themes, palettes, Unicode) are deferred to phase 6.
- Phase 2 authors the nine overworld rooms as geometry and doors only — no NPCs, items or enemies — so the validator and transition tests run against real content while pacing-sensitive content decisions stay in phase 4.
- Unicode mode is treated as one atomic deliverable in phase 6: either every tile has a verified-width glyph or the --unicode flag is not offered, with the shortfall named in the verification report.
- Section numbers written as 'phase N' in RISKS.md and ARCHITECTURE.md refer to spec §14's six stages; ROADMAP.md records the mapping to this roadmap's seven phases.

## Phases

### 1. Skeleton, terminal lifecycle and the playable room

**Goal:** Create the project from nothing and make it a runnable terminal game: a hero walking around one room at a fixed 30 Hz, with a terminal that is always restored on exit and an input policy that is safe over SSH.

**User-facing:** yes — gets end-to-end cases and user docs

**Deliverables:**
- Cargo package (lib + bin `mosslight`), rust-toolchain.toml pinned to 1.98.1 with rustfmt and clippy, committed Cargo.lock, .gitignore
- Dependency set exactly as fixed in PROFILE.md; crossterm never declared, used via ratatui::crossterm
- tests/deps.rs asserting the lock file contains exactly one crossterm entry
- .github/workflows/ci.yml running the full gate on ubuntu-latest and macos-latest
- src/config.rs: clap parser for the full §12 flag surface plus the hidden --debug-panic, NO_COLOR/--color precedence resolution, and the pre-raw-mode TTY / TERM=dumb refusal
- src/terminal.rs: TerminalGuard with idempotent Drop, panic hook that restores before printing, SIGTERM/SIGHUP atomic flags via signal-hook, size probe
- src/input.rs: map_key(mode, key) -> Option<Action>, movement coalescing, <=32 events per iteration, pending-action drop on menu/dialogue close; no key-release path anywhere
- src/game/: GameState, Tick, tuning.rs, rng.rs (SplitMix64), update(&mut GameState, &[Action], Tick) -> Vec<GameEvent> with movement, facing-on-blocked-move and tile collision
- src/app.rs: mode machine with MainMenu, Playing, Paused, ConfirmQuit, TooSmall; pause semantics; dirty flag
- src/render/: theme.rs, tiles.rs, hud.rs, scene.rs, overlays.rs — pure projection, centred 50x21 layout, HUD row, message row, hint row
- src/main.rs: the loop — poll with deadline, no busy-wait, <=5 catch-up steps with surplus time discarded, fps-gated draw, no full-screen clear
- Tests: movement and collision, input mapping and coalescing, guard restoration, TestBackend scene placement at 60x24 and 80x24, small-terminal notice, resume-into-Paused after resize, fps-independence of simulation state

**Acceptance criteria:**
- `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked` and `cargo build --release --locked` all succeed from a clean checkout at the repository root
- `cargo run --release` shows the main menu; New Game puts a hero in a room that can be walked around with arrows and WASD; Esc pauses; Q asks for confirmation and exits
- A test drives the same action sequence through the loop at --fps 10, 20 and 30 and asserts the resulting GameState is byte-identical, proving render rate does not affect simulation speed
- A test asserts Cargo.lock contains exactly one crossterm package entry
- Running with stdin not a TTY, or with TERM=dumb, exits non-zero with a one-line explanation and without ever enabling raw mode (asserted by test)
- After a normal exit, an error exit, and a --debug-panic run, `stty -a` shows the terminal out of raw mode and the alternate screen left; the panic message appears after restoration, not inside the alternate screen
- A TestBackend test at 60x24 asserts the hero glyph, wall glyphs, HUD and hint row land at fixed expected positions; at 80x24 the scene is centred
- Below 60x24 the app enters TooSmall, pauses, prints required and current size; on recovery it resumes into Paused, not into play (asserted by test)
- A test feeds 200 queued movement events in one iteration and asserts at most one step per tick occurs and no stale moves replay afterwards
- grep of src/ finds no reference to key release, KeyEventKind::Release, or extended keyboard protocol enabling

### 2. Content pipeline, validator and room transitions

**Goal:** Turn the world into embedded data and make 'the world is completable' a machine-checked property before any content is authored, then wire room transitions against the real nine-room overworld grid.

**User-facing:** no

**Deliverables:**
- src/content/schema.rs: serde types for World, Room, Tile, Door, Chest, Npc, EnemySpawn, Puzzle, Spawn, with stable namespaced string ids
- src/content/loader.rs: include_str! of assets/world.ron parsed once, returning Result<World, ContentError>; ids interned to dense indices in memory
- src/content/validate.rs implementing the full §7 list: 24x16 dimensions and legal tiles, id uniqueness, door targets exist, spawn points walkable and non-hazard, two-way transitions reciprocal, locks reachable only after a reachable key, and a BFS over (room, item-set) states proving the ember is obtainable and the lighthouse reachable afterwards; collects all errors rather than failing on the first
- Validation invoked at startup as well as in tests; failure aborts with a readable list before the terminal is touched
- assets/world.ron: the nine overworld rooms as a 3x3 grid — tile geometry, doors and spawn points only, no NPCs, items or enemies yet
- Room transition handling in game::update: exit through a door moves to the target room's spawn, hero lands on a safe tile, visited set updated, RoomEntered event emitted
- tests/fixtures/: deliberately broken worlds — missing door target, non-reciprocal two-way door, spawn inside a wall, duplicate id, wrong map dimensions, illegal tile, key behind the lock it opens, ember unreachable
- tests/content.rs and tests/transitions.rs

**Acceptance criteria:**
- content::validate over the real assets/world.ron returns Ok, asserted by a test that runs in CI
- Each of the eight broken fixture worlds is rejected, and each test asserts the specific error variant it expects — so a validator that silently passes everything cannot make the suite green
- The validator returns all errors from a fixture containing three distinct defects, not just the first
- A test walks the hero through every door in the overworld grid and asserts the resulting room id, the hero's spawn tile is walkable, and the reciprocal door leads back to the origin tile
- A test asserts the visited-room set grows exactly as rooms are entered and that RoomEntered is emitted once per transition
- Starting the binary with a deliberately corrupted embedded world (via a test harness entry point) reports the validation errors and exits non-zero without enabling raw mode
- The full phase gate passes

### 3. Sword combat, three enemy kinds, death and determinism

**Goal:** Deliver a complete short game loop: the hero swings a sword, three enemy kinds threaten them with readable state machines, damage and death work, and the whole simulation is provably deterministic for a given seed and action sequence.

**User-facing:** yes — gets end-to-end cases and user docs

**Deliverables:**
- src/game/combat.rs: sword strikes the single tile the hero faces, active window and cooldown from tuning, each target damaged at most once per swing, half-heart damage units, invulnerability window, knockback that stops at obstacles
- src/game/ai.rs: Slime (Idle|Chase), Bat (Dart|Rest), Guardian (Patrol|Telegraph|Dash|Recover) as explicit per-kind state machines; room-local BFS bounded to 24x16 with deterministic local-movement fallback
- Contested-tile resolution by stable Vec<Enemy> iteration order
- Telegraph signalling for the guardian's dash with at least the 600 ms warning from §6, expressed in ticks
- Death handling: HeroDied event, GameOver screen with retry, restore from an in-memory checkpoint taken on room entry (repointed at the save file in phase 6)
- Enemy spawns added to the overworld rooms authored in phase 2, respecting the §7 pacing order
- src/game/state.rs: state_hash() over the full durable simulation state
- Short attack animation in render, plus a visible danger cue for telegraphed attacks
- tests/combat.rs, tests/ai.rs, tests/determinism.rs

**Acceptance criteria:**
- Tests assert the sword hits exactly the tile in front of the hero for each of the four facings, and misses tiles beside and behind
- A test places two enemies on the struck tile path and asserts each takes damage at most once per swing
- A test asserts a second attack within the cooldown window is ignored and the first attack outside it lands
- A test asserts contact damage costs one half-heart, grants invulnerability for the tuned window, and that a second contact inside that window costs nothing
- A test asserts knockback into a wall leaves the entity adjacent to the wall and never inside or beyond it
- Each enemy kind runs for 1000 ticks in a maze room and the test asserts it never stops changing AI state and never leaves the room bounds
- A test asserts the guardian emits its telegraph at least 600 ms (in ticks) before the dash and is vulnerable during Recover
- A test asserts the hero reaching zero health emits HeroDied, opens GameOver, and that retry restores the checkpointed room-entry state
- A test runs the same seed and the same 2000-action sequence twice and asserts identical state_hash() at every tick
- A test runs two different seeds over the same action sequence and asserts the hashes differ, so the determinism test cannot pass on a constant
- The full phase gate passes

### 4. Overworld: NPCs, chests, sword, lantern, secrets, map and inventory

**Goal:** Fill the nine-room overworld with the content and the pacing the spec asks for, so the player learns movement, then interaction, then attack, and leaves the overworld holding the sword and the lantern.

**User-facing:** yes — gets end-to-end cases and user docs

**Deliverables:**
- Dialogue system: DialogueNode content, dialogue window with an explicit advance step, simulation paused while it is open, pending game actions dropped on close
- Three NPCs with dialogue, including the one that reveals the sanctuary and sets the story flag
- Chests and rewards: sword, lantern, small keys, heart containers, messages; ChestOpened persists in Progress
- Lantern mechanics: lights the torch on the faced tile, reveals marked hidden passages, no fuel, required to open the dungeon entrance
- The single pre-lantern overworld puzzle from §7 step 4
- Three secrets, none on the main route, one of them a heart container
- Map screen showing visited rooms, current position and revealed key objects, with unvisited rooms hidden; inventory screen; help screen; all pause the simulation
- Pacing encoded structurally: the first room has no enemies and shows a movement/interaction hint, the sword is on the critical path in the second overworld room, the first slime appears only after it
- tests/overworld.rs, tests/dialogue.rs, extended TestBackend coverage for the map, inventory and dialogue overlays

**Acceptance criteria:**
- A test drives the hero from the start to the lantern using only ordinary Actions and asserts the milestone order: NPC dialogue flag set, sword acquired, first slime encounter possible, puzzle solved, lantern acquired
- A test asserts the starting room contains zero enemy spawns and that no enemy spawn is reachable before the sword chest
- A test asserts the lantern lights a torch on the faced tile only, and that a hidden passage becomes walkable only after the marked torch is lit
- A test asserts the dungeon entrance stays impassable without the lantern and opens with it
- A test asserts each chest can be opened exactly once and that reopening yields nothing
- A test asserts the map screen renders only visited rooms, marks the current one, and that opening it pauses the simulation (tick does not advance)
- A test asserts closing a dialogue or a menu drops queued movement actions so the hero does not step on close
- A test asserts three secret rewards exist, are reachable, and that none of them appears on the validator's main-route BFS path
- content::validate still passes over the expanded world
- The full phase gate passes

### 5. Dungeon, keys, puzzles, two-phase boss and the ending

**Goal:** Complete the game: six dungeon rooms with keys and puzzles, a two-phase boss with telegraphs and a vulnerability window, the ember, and the return to the lighthouse — proved by a headless start-to-victory playthrough.

**User-facing:** yes — gets end-to-end cases and user docs

**Deliverables:**
- Six dungeon rooms authored in assets/world.ron including the boss arena, with locked doors and small keys
- Locked door handling: a door consumes exactly one key, once, and stays unlocked in Progress
- src/game/puzzles.rs: BlockOnPlates (push a block onto a plate) and TorchSequence (light torches in a hinted order), with visible feedback, no irreversible soft-lock, reset on room exit while unsolved, and persistence once solved
- Boss: two phases with distinct attack patterns, an explicit telegraph before each dangerous action, a vulnerability window after each attack, no frame-precise input required, PhaseChanged and BossDefeated events, ember reward
- Ending: carrying the ember to the lighthouse and interacting triggers GameWon and the victory screen
- tests/dungeon.rs, tests/puzzles.rs, tests/boss.rs
- tests/playthrough.rs: headless start-to-victory run driving only ordinary Actions, never setting a story flag directly

**Acceptance criteria:**
- tests/playthrough.rs completes from New Game to GameWon using only Actions, and asserts the main-route milestone order (sanctuary learned, sword, lantern, dungeon entered, keys, both puzzles, boss phase 2, ember, lighthouse relit)
- A test asserts unlocking a door decrements the key count by exactly one, that a second pass through the same door costs nothing, and that the key count never goes negative
- A test asserts each puzzle kind can be solved, that leaving and re-entering the room resets an unsolved puzzle to its authored state, and that a solved puzzle stays solved
- A test drives each puzzle into every wrong state it allows and asserts the room is still solvable afterwards — no configuration soft-locks the route
- A test asserts the boss transitions from phase 1 to phase 2 at the tuned threshold, that each phase has a distinct attack pattern, and that a telegraph precedes every dangerous action by at least the tuned warning
- A scripted no-damage test beats the boss without the hero ever losing health, proving the vulnerability window is real and the fight is not a damage race
- content::validate passes over the finished 15-room world, including the (room, item-set) BFS proving the ember is obtainable and the lighthouse reachable afterwards
- A test asserts the world contains exactly 9 overworld rooms and 6 dungeon rooms, 3 NPCs, 3 regular enemy kinds and 1 boss, matching the §2 content table
- The full phase gate passes

### 6. Save slot, full CLI and presentation modes

**Goal:** Make progress durable and the game legible in every mode: one atomic save slot that survives corruption and version skew, and gameboy/ansi/mono themes where colour is never the only distinction.

**User-facing:** yes — gets end-to-end cases and user docs

**Deliverables:**
- src/save.rs: SaveFile with format_version, atomic temp + sync_all + rename, retained save.json.bak, two-pass version probe, LoadOutcome { Ok, Missing, Corrupt, FutureVersion }, no unwrap on the path
- Save path resolution: $XDG_DATA_HOME/mosslight or ~/.local/share/mosslight on Linux, ~/Library/Application Support/mosslight on macOS, --save-dir override
- Autosave triggers: after a safe room transition, an important item, a solved puzzle, and the boss victory; manual save from the pause menu outside combat
- Death state never overwrites a usable save; on load, enemies respawn, the hero gets a short safe window, and the boss fight restarts from entering the arena
- Corrupt / future-version handling in the UI: offer backup restore or a new game, never silently overwrite
- Main menu Continue / New Game / Help / Quit, with New Game over an existing run requiring confirmation
- Phase-3's death checkpoint repointed at the save file
- render/theme.rs: gameboy, ansi and mono themes changing colour only; glyph table identical across themes; the Unicode decision made and either shipped complete or the flag withdrawn
- tests/save.rs, tests/render_modes.rs

**Acceptance criteria:**
- A test round-trips a fully-populated SaveFile through store and load and asserts every field survives, including visited rooms, opened chests, unlocked doors, solved puzzles, flags and boss victory
- A test writes truncated, garbage and empty save files and asserts load returns Corrupt without panicking and without modifying the file on disk
- A test writes a save whose format_version is higher than the current one and asserts load returns FutureVersion and that a subsequent store refuses to overwrite it
- A test asserts a store interrupted before rename leaves the previous save intact and the .bak recoverable
- A test asserts a death state is never written over the last usable save
- A test asserts each of the four autosave triggers writes exactly once, and that manual save is unavailable during combat
- A test asserts loading restores progress, respawns enemies, grants the hero a safe window, and resets boss progress to arena entry
- TestBackend tests assert the rendered characters of the same scene are identical under gameboy, ansi and mono, so monochrome legibility cannot regress
- A test asserts NO_COLOR disables colour, and that an explicit --color always overrides it
- --unicode either renders a complete tile set (asserted glyph-by-glyph against the ASCII table) or the flag is absent from --help
- The full phase gate passes

### 7. Balance, measurement, documentation and verification report

**Goal:** Tune the game to its 30–45 minute target, measure what §13 asks to be measured, document the product, and report honestly which environments were actually exercised.

**User-facing:** yes — gets end-to-end cases and user docs

**Deliverables:**
- Balance pass over game/tuning.rs against the playthrough length, enemy damage and boss difficulty
- A byte-counting writer harness measuring terminal output per minute during play and while paused, plus a CPU measurement, both with the environment named
- docs/user/: controls for every mode, the full CLI surface, save file locations per OS, corrupt-save behaviour, running over SSH and under tmux
- README.md: build, controls, SSH launch, and a pointer to the docs
- docs/dev/architecture.md: architecture summary alongside the existing ADRs
- docs/dev/verification-report.md: the §13 check list with what was run, on what, and what was not run
- Manual check pass on the host Mac: local terminal, 60x24 and 80x24, live resize, monochrome, held key, save/quit/continue, terminal state after a controlled panic
- HANDOFF.md naming any shortfall against the §2 content table as remaining work

**Acceptance criteria:**
- A test asserts the headless playthrough's tick count corresponds to a run length inside the 30–45 minute band at the tuned step and combat intervals, so a balance regression fails the suite
- The measurement harness reports bytes written per minute during play and while paused, and a test asserts a paused game with no input emits zero bytes after the first frame
- docs/user/ documents every binding in the §5 table and every flag in the §12 table, verified by a test that diffs the documented flag list against clap's own
- README.md builds and runs the game following only its own instructions on the host Mac
- docs/dev/verification-report.md lists every §13 mandatory check with its status, and explicitly marks Linux, a real PTY SSH session, the ~150 ms RTT check, Intel macOS and aarch64 Linux as unverified
- The manual check list is executed on the host Mac and its results — including failures — are recorded verbatim in the report
- No stub, TODO or unimplemented! remains on the main route (asserted by a grep test over src/ and assets/)
- cargo fmt --check, cargo clippy --all-targets --all-features -- -D warnings, cargo test --locked and cargo build --release --locked all pass, and CI is green on ubuntu-latest and macos-latest

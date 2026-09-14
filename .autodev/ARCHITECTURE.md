# Architecture — Mosslight

Design of record. Source: `docs/spec.md` (Ukrainian), `.autodev/INTAKE.md`.
Written 2026-09-13. Section numbers in parentheses refer to the spec.

## Problem

Mosslight is a single-player, top-down adventure game that runs entirely inside a terminal (§1).
The forest lighthouse has gone dark; the hero must find the ancient ember in a flooded sanctuary and
relight it. One playthrough targets 30–45 minutes.

The player explores a 3×3 overworld of 9 screens plus a 6-room dungeon, talks to 3 NPCs, fights 3 enemy
types with a real-time sword, uses a lantern to light torches and reveal hidden passages, solves
block/pressure-plate and torch puzzles, beats a two-phase boss, and carries the ember home (§2, §7).

The audience is a person at a terminal — local, or over an interactive SSH session with a PTY, possibly
inside tmux or screen (§3). The game is offline, needs no graphics server, no fonts beyond ASCII, and no
external asset files at runtime.

The product bet is that a terminal game can feel like a small handheld adventure rather than a monitoring
dashboard (§4). Everything below serves that: small fixed viewport, quiet redraws, single-keypress
control, no reliance on modern terminal protocols.

## Constraints

**Fixed by the spec**

| Constraint | Value |
|---|---|
| Language / toolchain | Rust stable, pinned toolchain + committed `Cargo.lock` (§8, §15) |
| Rendering | ratatui (§8) |
| Terminal I/O | crossterm (§8) |
| Content / save serialization | serde + RON or JSON (§8) |
| CLI | clap (§8) |
| Forbidden | game engine, async runtime, full ECS, extra libraries without a concrete need (§8) |
| Concurrency | single thread is sufficient and intended (§8) |
| Platforms | macOS Apple Silicon + Intel, Linux x86_64 + aarch64 (§3). **No Windows** (§2) |
| Delivery channel | one self-contained binary, `cargo run --release` / `mosslight` (§3) |
| Network | none after install (§3) |
| Simulation rate | fixed 30 updates/s; render ≤ 20 fps default; `--fps` caps rendering only (§9) |
| Room size | 24×16 tiles; 1 tile = 2 terminal columns × 1 row (§4) |
| Minimum terminal | 60×24 (§4) |
| Base glyph set | ASCII; Unicode is optional, verified-width characters only; no emoji/Nerd Fonts/Sixel/Kitty (§4) |
| Colour | 4-shade green "gameboy" theme, ANSI-16 palette, full monochrome mode; colour is never the only distinction (§4) |
| Input protocol | key **press** events only — no key-release, no simultaneous-press, no kitty/extended keyboard protocols (§5) |
| Save slots | exactly one, with autosave + manual save (§2, §10) |
| Out of scope (v1) | procedural generation, multiplayer, crafting, skill tree, shops, audio, map editor, Windows (§2) |

**Fixed by the intake**

- Docs, code, identifiers, comments, commit messages: **English**.
- Phase gate: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked`
  green at the end of every phase. Lint debt is never deferred.
- **No separate e2e surface.** The headless start-to-victory playthrough and the `TestBackend` scene checks
  from §13 are ordinary Rust tests under `tests/`.
- `--unicode` may be deferred as a whole; if it ships partial it does not ship.
- Team size: one unattended agent pipeline, ~8 hours of wall clock. Operability cost is a design input:
  nothing here may require an operator.
- Verification honesty: macOS/local terminal/tmux checks are reachable on the host Mac; Linux, real SSH with
  a PTY, and the ~150 ms RTT check are **not** and must be marked unverified.

**Environmental**

- Host toolchain observed: `rustc 1.98.1` on `aarch64-apple-darwin`. Pinned in `rust-toolchain.toml`.
- No TTY, or `TERM=dumb`: exit with a short explanation **before** raw mode is enabled (§3).

## Shape

One process, one thread, one binary. The crate is split `lib` + thin `bin` so that everything except
terminal setup is testable without a terminal.

```
src/main.rs        argv -> Config -> TerminalGuard -> run loop -> exit code      (only file that owns a terminal)
src/lib.rs         module tree, public API for tests
src/config.rs      clap parser, Config, TERM/NO_COLOR/TTY probing
src/terminal.rs    raw mode, alternate screen, RAII guard, panic hook, signal flags, size probe
src/input.rs       KeyEvent -> Action, per-mode key maps, coalescing, queue cap
src/app.rs         App: screen/mode state machine, pause semantics, event -> effect, autosave triggers
src/render/        pure: (&App, Rect) -> frame. theme.rs, tiles.rs, hud.rs, scene.rs, overlays.rs
src/game/          pure simulation: state.rs world.rs entities.rs combat.rs ai.rs puzzles.rs tuning.rs rng.rs
src/content/       RON schema types, loader (include_str!), validator
src/save.rs        SaveFile, versioning, path resolution, atomic write, backup rotation
assets/world.ron   the whole world, embedded at compile time
tests/             integration tests incl. headless playthrough and TestBackend scene checks
```

### Components and why each exists

| Component | Responsibility | Why it exists (one sentence) |
|---|---|---|
| `config` | Parse argv into an immutable `Config`; decide ASCII/Unicode, colour, theme, fps, save dir, seed; refuse a non-TTY or `TERM=dumb` before any terminal mutation | §12 defines a CLI surface and §3 requires a clean pre-raw-mode refusal, so the decision must happen before the terminal is touched. |
| `terminal` | Enter/leave raw mode and the alternate screen, own the `TerminalGuard`, install the panic hook, register SIGTERM/SIGHUP flags, report terminal size | §11 makes terminal restoration a correctness requirement on four exit paths; concentrating it in one RAII type is the only way to prove it. |
| `input` | Translate `KeyEvent` to semantic `Action` per mode; cap events read per iteration; coalesce redundant movement; drop pending game actions when a menu/dialogue closes | §5 forbids relying on key-release and forbids a long queue of stale moves; that policy is a unit-testable pure function, not loop trivia. |
| `app` | The screen state machine (MainMenu, Playing, Dialogue, Map, Inventory, Paused, ConfirmQuit, ConfirmNewGame, SaveProblem, GameOver, Victory, TooSmall); decides when simulation is paused; turns `GameEvent`s into UI effects and autosave calls; owns a `Box<dyn SaveIo>` (the port; see `save` below) and the cached `SlotState` it probes at startup | Pausing, dialogue, and autosave triggers are cross-cutting policy that belongs to neither the pure simulation nor the renderer. `App` deciding *when* to save while a separate port decides *where* keeps the filesystem out of every test that only cares about policy (phase 6, deviates from this table's original free-function sketch below — logged in DECISIONS). |
| `game` | The pure simulation: `update(&mut GameState, &[Action], tick) -> Vec<GameEvent>`; movement, collision, combat, enemy AI, puzzles, boss phases, room transitions | §8's headline requirement is that the simulation does not depend on the terminal; this module is that boundary. |
| `game::tuning` | Every combat/movement constant in one place, expressed in ticks | §6 explicitly requires all combat values kept together for balancing. |
| `game::rng` | Seeded SplitMix64; lives in `GameState`, advanced only inside `update` | §8 requires seeded, controlled randomness, and §13 requires identical simulation for identical seed + action sequence. |
| `content` | RON schema types, compile-time embedding via `include_str!`, and the validator | §7 requires a readable world description embedded into the binary and a named list of validity checks. |
| `render` | Read-only projection of `App` into a ratatui frame; glyph table, palette, layout, centring, small-terminal notice | §8 says rendering only reads state; keeping it a pure function makes `TestBackend` assertions possible (§13). |
| `save` | One slot: versioned JSON, path resolution, atomic temp+rename, previous-copy backup, corruption and future-version handling | §10 describes durability rules that are a component's job, not scattered calls. |

Deliberately absent: no ECS, no scene graph, no plugin system, no event bus beyond a returned `Vec<GameEvent>`,
no input thread, no async runtime, no asset pipeline. Each was considered and rejected (see *Rejected alternatives*).

### Contracts

```rust
// game — the only simulation entry point. No wall clock inside.
pub fn update(state: &mut GameState, actions: &[Action], tick: Tick) -> Vec<GameEvent>;

// input — pure mapping, no I/O
pub fn map_key(mode: Mode, key: KeyEvent) -> Option<Action>;
pub fn coalesce(actions: &mut Vec<Action>);           // collapse redundant movement

// render — pure, read-only
pub fn draw(frame: &mut Frame, app: &App, theme: &Theme);

// content
pub fn load() -> Result<World, ContentError>;          // parses the embedded RON
pub fn validate(world: &World) -> Result<(), Vec<ContentError>>;

// save — accessed only through the port; App owns a Box<dyn SaveIo>
pub trait SaveIo {
    fn load(&mut self) -> LoadOutcome;         // Ok | Missing | Corrupt | FutureVersion
    fn load_backup(&mut self) -> LoadOutcome;
    fn store(&mut self, save: &SaveFile) -> StoreOutcome;  // Ok | RefusedFutureVersion | RefusedDeathState | Failed
}
// FileSaveIo (production, atomic stage-then-commit with a retained .bak) and
// MemorySaveIo (tests, zero disk I/O) are the two implementations.
```

`Action` is semantic (`MoveNorth`, `Attack`, `UseLantern`, `Interact`, `Confirm`, `Cancel`, `ToggleMap`,
`ToggleInventory`, `Quit`), never a key code. `GameEvent` is what the simulation announces
(`RoomEntered`, `ItemPicked`, `ChestOpened`, `DoorUnlocked`, `PuzzleSolved`, `HeroDamaged`, `HeroDied`,
`EnemyKilled`, `BossPhaseChanged`, `BossDefeated`, `DialogueStarted`, `GameWon`, `Message`).

### Control flow

```
main
 └─ Config::from_args           (may exit 2 before touching the terminal)
 └─ TerminalGuard::enter        (raw mode + alt screen + panic hook + signal flags)
 └─ loop:
      poll crossterm events until the next tick deadline (never busy-wait)
      read at most INPUT_EVENTS_PER_ITER events; map to Actions; coalesce
      catch up at most 5 fixed 1/30 s steps; discard surplus accumulated time
      if !app.paused(): game::update(...)  -> events -> app.apply(events) -> maybe autosave
      if render due (fps cap) AND (dirty OR simulation ran): terminal.draw(render::draw)
 └─ TerminalGuard::drop         (leave alt screen, disable raw mode, show cursor, reset styles)
 └─ print any diagnostic AFTER the guard has restored the terminal
```

### Where state lives

| State | Home | Lifetime |
|---|---|---|
| World geometry, dialogue, item placement | `World`, parsed once from embedded RON | process, immutable |
| Hero, enemies, projectiles, timers, puzzle state, room contents | `GameState` (in memory) | process; only the durable subset is saved |
| Progress: position, health, equipment, keys, visited rooms, opened chests/doors, solved puzzles, story flags | `SaveFile` on disk, one slot | across runs |
| Screen/mode, dialogue cursor, message log, dirty flag | `App` | process, never saved |
| Render options (ASCII/colour/theme/fps), save dir, seed | `Config`, immutable after parse | process |
| Terminal raw/alt-screen state | `TerminalGuard` | process, restored on every exit path |

### Trust boundaries

1. **Save file on disk** — untrusted input. May be truncated, garbage, hand-edited, or written by a newer
   version. Must never panic, must never be silently overwritten (§10). Parsing goes through a fallible
   path that returns `LoadOutcome`, never `unwrap`.
2. **Terminal and environment** — `TERM`, `NO_COLOR`, terminal size, and the ability to write to stdout are
   all outside our control and can change mid-run (resize, SSH drop). Probed and re-probed, never assumed.
3. **CLI arguments** — user-supplied; `--save-dir` is an arbitrary path, `--fps`/`--seed` arbitrary numbers.
   Validated by clap plus range checks; `--save-dir` is created if missing and reported clearly if unusable.
4. **Embedded content** — trusted at build time but validated at load, so a bad edit fails loudly rather
   than producing an unwinnable world.

There is no network, no other user, no privilege boundary, and no secret. Authentication and authorization
are not applicable (single-player, local, offline — §1, §2) and this is stated rather than implied.

## Data

### Entities

**Content (immutable, authored in `assets/world.ron`, embedded at compile time)**

- `World { rooms, items, npcs, enemies_defs, puzzles, start, version }`
- `Room { id: RoomId, name, kind: Overworld|Dungeon, map_index: Option<(x,y)>, tiles: [[Tile; 24]; 16], doors, spawns, chests, plates, torches, blocks, npcs, enemies, secrets }`
- `Tile` — `Floor | Wall | Water | Bush | Door(DoorId) | Stairs | Pit | Hidden(Tile)` plus decoration variants.
- `Door { id: DoorId, at: Pos, to: RoomId, to_spawn: SpawnId, lock: Option<LockKind>, two_way: bool }`
- `Chest { id: ChestId, at: Pos, contains: Reward }` — `Reward = Sword | Lantern | SmallKey | HeartContainer | Ember | Message`
- `Npc { id: NpcId, at: Pos, dialogue: Vec<DialogueNode>, condition: Option<FlagId> }`
- `EnemySpawn { kind: Slime|Bat|Guardian|Boss, at: Pos, patrol: Option<Vec<Pos>> }`
- `Puzzle { id: PuzzleId, kind: BlockOnPlates|TorchSequence, room: RoomId, requirement, reward: PuzzleReward }`

**Runtime (in memory)**

- `GameState { tick, rng, hero, room: RoomId, entities: Vec<Enemy>, blocks, torches, plates, hazards, progress: Progress, pending_transition }`
- `Hero { pos, facing, health_halves, max_health_halves, step_cooldown, attack_state, invuln_until, keys, equipment }`
- `Enemy { id, kind, pos, facing, hp, ai: AiState, timers }` — `AiState` is an explicit enum per kind
  (`Slime: Idle|Chase`, `Bat: Dart|Rest`, `Guardian: Patrol|Telegraph|Dash|Recover`, `Boss: PhaseOne(..)|PhaseTwo(..)|Stunned|Dead`).
- `Progress { visited: RoomSet, opened_chests: ChestSet, unlocked_doors: DoorSet, solved_puzzles: PuzzleSet, flags: FlagSet, boss_defeated: bool }`

### Identity and ownership

- Every authored object carries a **stable string id** in RON (`room.lighthouse`, `door.dungeon_entrance`,
  `chest.forest_sword`, `flag.told_about_sanctuary`). Ids are the save-file vocabulary and therefore part of
  the compatibility surface: renaming one is a save-format break, and the validator enforces uniqueness (§7).
- At load, ids are interned into dense indices for fast in-memory use; the save file stores **strings**, not
  indices, so reordering content does not corrupt an existing save.
- `GameState` owns all mutable runtime data. `World` is shared immutably. Nothing else holds mutable state.

### What must never be lost

The single save slot is the only thing the player cannot recreate: position, health and max health,
equipment, keys, visited rooms, opened chests, unlocked doors, solved puzzles, story flags, boss victory.
Protection: atomic temp-file + rename in the same directory, one retained previous valid copy
(`save.json.bak`), refusal to overwrite a newer format version, and a hard rule that a death state never
overwrites the last usable save (§10).

Explicitly **not** saved (§10): the in-flight attack animation, transient combat state, enemy positions and
health, boss fight progress. On load, enemies respawn, the hero gets a short safe window, and the boss
fight restarts from entering the arena.

## Cross-cutting

**Authentication / authorization** — none, by design. Single-player, single local user, no accounts, no
roles, no multi-user data, no network listener (§1, §2). The only access-control-shaped decision is that the
save directory is per-user under the OS data directory.

**Input validation** — three untrusted inputs, each with a named owner:
- CLI: clap types plus range checks (`--fps` ∈ {10,20,30}, `--color` and `--theme` enums, `--seed` u64,
  `--save-dir` canonicalized and checked writable).
- Save file: two-pass read — first the `version` field alone, then the body. Any parse failure yields
  `Corrupt`; a higher version yields `FutureVersion`. Neither panics, neither overwrites.
- Content: the validator (below) runs in tests **and** at startup; a failure aborts with a readable list.

**Content validation** (§7) — checked invariants: map dimensions are exactly 24×16 and every tile is legal;
all ids unique; every door target room and spawn exists; every spawn point is a walkable, non-hazard tile;
two-way transitions are reciprocal; every lock is reachable only after a key that is itself reachable;
a forward-only traversal from the start reaches the ember and returns to the lighthouse. The reachability
check is a breadth-first search over (room, acquired-items) states — it is what makes "no dead ends through
keys or puzzles" (§15) a machine-checked property rather than a hope.

**Error handling** — the simulation is total: it returns events, never `Result`, and cannot fail at runtime.
Fallible work lives at the edges (`config`, `save`, `content`, `terminal`) and returns `Result` with a
concrete error enum. `panic!`/`unwrap` are forbidden outside tests and genuinely impossible states; clippy is
configured to deny the obvious offenders. A panic that still happens is caught by the hook, the terminal is
restored first, and only then is the message printed (§11).

**Logging / observability** — no log output may reach the game screen (§11). A single optional file sink
(`--log-file PATH`, off by default) plus a deferred diagnostic buffer flushed to stderr after the terminal
guard drops. The developer's answer to "it does not work" is: the deferred diagnostics, the save file (plain
JSON, hand-readable), and a debug overlay behind `--debug` showing tick, fps, entity count, and bytes written
per second.

**Configuration and secrets** — configuration is CLI-only; there is no config file in v1 and no environment
variable of our own beyond respecting `NO_COLOR`, `TERM`, `XDG_DATA_HOME`, and `HOME`. There are no secrets:
nothing is transmitted, nothing is authenticated, nothing sensitive is stored. This removes an entire class
of risk and is the reason no secret-handling machinery appears anywhere in this design.

**Internationalization** — not required. The spec is Ukrainian but the intake fixes all shipped text and
docs as English. All game text lives in `assets/world.ron` and the message table rather than inline string
literals, so a later translation is a content change, not a code change. No runtime locale handling in v1.

**Accessibility** — the product has a UI, and the spec makes three accessibility rules load-bearing (§4):
colour is never the only differentiator (every object has a distinct ASCII glyph, and the glyph is the
primary channel); a full monochrome mode must remain completely legible; no flicker and no decorative
animation. Added to these: all state changes are also announced in the message line as text; dialogue
advances on an explicit keypress with a visible prompt rather than on a timer; every action is reachable
from single, non-chorded keypresses (§5), which also serves anyone who cannot press two keys at once.

## Non-functional requirements

Values marked **(assumed)** are not in the spec; they are chosen here and are the numbers the verification
report will be measured against.

| Property | Target | Source |
|---|---|---|
| Simulation rate | exactly 30 Hz, fixed | §9 |
| Render rate | ≤ 20 fps default; `--fps` ∈ {10,20,30} caps rendering only | §9, §12 |
| Catch-up | ≤ 5 fixed steps per iteration; surplus accumulated time discarded | §9 |
| Input events per iteration | ≤ 32 read, ≤ 1 movement step applied per tick, redundant moves coalesced **(assumed)** | §5 |
| Hero step interval | 4 ticks = 133 ms (spec's 120 ms rounded to the tick grid) **(assumed)** | §6 |
| Sword cooldown / active phase | 9 ticks = 300 ms / 3 ticks = 100 ms | §6 |
| Invulnerability after damage | 24 ticks = 800 ms | §6 |
| Heavy-attack telegraph | ≥ 18 ticks = 600 ms | §6 |
| Health | half-heart integer units; 6 at start, 10 maximum | §2, §6 |
| Room grid | 24×16 tiles; tile = 2 cols × 1 row | §4 |
| Minimum terminal | 60×24; layout needs 50×21, so the fit has slack | §4 |
| World size | 15 rooms (9 overworld + 6 dungeon), 3 NPCs, 3 enemy types, 1 two-phase boss, ≥ 3 secrets | §2 |
| First playthrough | 30–45 minutes | §1 |
| Enemies live per room | ≤ 12 **(assumed)** — bounds AI cost and screen noise |
| Startup to main menu | < 200 ms on the reference Mac **(assumed)** |
| Input-to-visible-effect | < 1 render period (≤ 50 ms at 20 fps), excluding network RTT **(assumed)** |
| Playability at RTT | controllable at ~150 ms RTT **(unverified — no reachable environment)** | §13 |
| Terminal output | < 200 KB per minute of active play; ≈ 0 bytes per minute while paused or in a static menu **(assumed)** | §9, §13 |
| CPU | < 5 % of one core during play, ≈ 0 % while paused **(assumed)** | §13 |
| Resident memory | < 32 MB **(assumed)** |
| Release binary | < 12 MB **(assumed)** — matters because the user copies it to a server |
| Save file | < 64 KB **(assumed)**; write completes in < 50 ms |
| Runtime dependencies | none — no network, no graphics server, no fonts, no external asset files | §3 |
| Supported OS | macOS 13+ (aarch64, x86_64), Linux with glibc 2.31+ (x86_64, aarch64) **(assumed floor)** | §3 |
| Rust toolchain | pinned `1.98.1` via `rust-toolchain.toml`, `Cargo.lock` committed | §8, §15 |
| Determinism | identical seed + identical action sequence ⇒ identical state, verified by state hash | §13 |

## Failure modes

### External dependencies

| Dependency | Failure | Handling |
|---|---|---|
| **Terminal (stdout)** | Write fails after an SSH drop or a closed PTY | Treat a write error as a fatal-but-clean shutdown: stop the loop, attempt guard restoration within whatever connection remains, exit non-zero. Never retry in a loop (§11). |
| **Terminal (stdin)** | Not a TTY, or `TERM=dumb` | Detected in `config`, **before** raw mode: print one explanatory line to stderr and exit 2 (§3). |
| **Terminal (size)** | Below 60×24 at startup or after a resize | Enter the `TooSmall` screen, pause the simulation, show required and current size; when the size returns, resume into **Paused**, never straight into play, so the hero cannot take a surprise hit (§9). |
| **Terminal (resize storm)** | Many resize events while dragging | Coalesce to the last size seen per iteration; re-layout only on an actual change. |
| **Filesystem (save dir)** | Missing, not writable, disk full, `--save-dir` points at a file | Create the directory on first use; on failure report once in the message line and in the deferred diagnostics, keep playing (a save failure must not end a session), and retry at the next autosave point. |
| **Filesystem (save file)** | Truncated, corrupt, hand-edited | `LoadOutcome::Corrupt` → offer "restore backup" or "new game"; never panic, never silently overwrite (§10). |
| **Filesystem (save file)** | Version newer than this build | `LoadOutcome::FutureVersion` → refuse to load **and** refuse to overwrite; main menu's `Continue` opens `Mode::SaveProblem` with `New game`, gated by `ConfirmNewGame`. That new run plays with autosave disabled (every `store` call keeps returning `RefusedFutureVersion`, surfaced once per attempt) rather than writing into a second, differently-named slot — §2 fixes exactly one slot, so a second slot was considered and rejected (DECISIONS `[phase-6/plan]`). |
| **Filesystem (backup)** | Backup also corrupt | Report both as unusable; only New Game remains. The original files are left untouched on disk for the user to recover manually. |
| **Signals** (SIGTERM, SIGHUP) | Delivered mid-frame, e.g. SSH hang-up | `signal-hook` sets an `AtomicBool`; the main loop observes it, leaves the loop, restores the terminal, and exits. **No file I/O in a handler** (§11). An autosave already on disk is what protects progress. |
| **Ctrl+C** | Raw mode suppresses SIGINT, so it arrives as a key event | Handled in `input` as a quit request with the same confirmation path as `Q` (§11). |
| **SIGKILL / power loss** | No cleanup runs | Not defensible in-process. Data safety comes solely from atomic saves; the terminal is the shell's problem (`reset`). Stated in the README (§11). |
| **Embedded content** | Invalid after an edit | Validator runs in CI, in `cargo test`, and at startup; startup failure aborts with a list before the terminal is touched. |
| **System clock** | Jumps backwards/forwards (NTP, laptop sleep) | Pacing uses `Instant` (monotonic) only; the simulation never reads a clock at all. A large elapsed gap is clamped to 5 catch-up steps and the rest discarded (§9). |

### Multi-step operations

| Operation | Interruption | Outcome |
|---|---|---|
| **Save write** (serialize → temp file in the same dir → `sync` → rename over `save.json` → rotate previous to `.bak`) | Crash before rename | On-disk save unchanged; stray `.tmp` is ignored and overwritten next time. |
| | Crash after rename, before backup rotation | New save valid; backup is one generation older than intended — harmless. |
| | Unknown outcome (process dies during rename) | Rename is atomic on POSIX: the file is either wholly old or wholly new. No third state exists. |
| | Duplicate/concurrent save (autosave racing a manual save) | Impossible by construction — single thread, saves happen at explicit loop points, never reentrantly. |
| **Room transition + autosave** | Crash between placing the hero and writing the save | The save is written **after** the hero is at a safe spawn; a crash before it means the player restarts from the previous safe point. Never a save pointing into a wall or hazard (§10). |
| **Chest open → grant item → set flag → autosave** | Crash mid-sequence | The mutation is applied to `GameState` in one step and only then persisted; the durable outcome is all-or-nothing. Worst case the player reopens the chest — the chest id is in `opened_chests`, so the reward is granted once per persisted state and never duplicated. |
| **Locked door → consume key → mark unlocked → autosave** | Crash after consuming, before the save | Both the key count and `unlocked_doors` live in the same snapshot, so a lost write loses both together; the key is never consumed without the door opening in the same persisted state (§13 requires exactly this test). |
| **Puzzle solve → reward → autosave** | Crash mid-sequence | Unsolved puzzles reset on room re-entry by design (§6), so the failure degrades to "solve it again" — never to a blocked route. |
| **Boss defeat → grant ember → autosave** | Crash mid-sequence | Boss fight restarts from entering the arena (§10). Because the fight is restartable and the ember is granted with the victory flag in one snapshot, there is no state with the ember but no victory, or a consumed boss with no ember. |
| **Death** | — | Death is never persisted; the last usable save stays intact and the retry screen restarts from it (§10). |
| **Restart mid-flight (any)** | — | There is exactly one durable artifact and it is only ever replaced atomically, so "restart mid-flight" reduces to "the last completed save wins". |

## Delivery and operations

**Build** — a single Cargo crate, `lib` + `bin`, no build script beyond `include_str!` of `assets/world.ron`
(content is embedded, so `cargo build` is the entire asset pipeline). Toolchain pinned by
`rust-toolchain.toml` (1.98.1, with `rustfmt` and `clippy`); `Cargo.lock` committed and every CI/gate command
uses `--locked`.

**Gate** (every phase, intake-mandated):
`cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --locked`

**Package** — one self-contained executable named `mosslight`. No runtime asset files, no config file, no
installer. Distribution is `cargo install --path .`, `cargo build --release` plus copying the binary, or a
release archive per target triple. Copying the binary to a server and running it over SSH must be sufficient.

**CI** — GitHub Actions matrix over `ubuntu-latest` (x86_64 Linux) and `macos-latest` (aarch64 macOS),
running fmt, clippy, tests, and a release build. CI is what covers Linux, which the unattended run cannot
reach locally; the verification report says so explicitly rather than claiming a local Linux check.

**Release and upgrade** — versioned by `Cargo.toml`; `--version` prints it. Upgrading is replacing the
binary. There is nothing to migrate except the save file.

**How user data survives an upgrade** — the save carries `format_version`. A newer binary reading an older
save runs a forward migration chain (`v1 -> v2 -> ...`) keyed on that field; an older binary reading a newer
save refuses and does not overwrite. Because ids are stable strings, adding rooms, chests, or flags is a
non-breaking content change: unknown ids in a save are dropped with a note, and missing ids default to
"not yet done". Renaming or deleting an id is a breaking change that requires a migration step and a
`format_version` bump — this is written down here precisely so a later phase does not do it casually.

**Operations** — none. There is no service, no deployment, no on-call, no telemetry. The only operational
question a user can ask is "where is my save?", answered by the README and by `--save-dir`.

## Rejected alternatives

- **Separate input thread feeding a channel** — the conventional ratatui pattern. Rejected: `event::poll`
  with a deadline already gives non-blocking input on one thread without busy-waiting, and a thread would add
  a synchronization boundary to the one thing §13 requires to be perfectly deterministic. Single-threaded
  also makes "no reentrant save" true by construction.
- **`tokio` or any async runtime** — rejected by §8 outright, and there is nothing to await: no network, no
  concurrent I/O, one fixed-rate loop.
- **A full ECS (`bevy_ecs`, `hecs`)** — rejected. Fifteen rooms with ≤ 12 enemies each do not need archetype
  storage, and an ECS would make the save/restore boundary and the determinism proof harder, not easier.
  Plain `Vec<Enemy>` with an explicit `AiState` enum per kind is directly readable and directly testable (§8).
- **Wall-clock time inside the simulation** (`Instant::now()` in combat/AI) — rejected by §8 and fatal to
  the determinism test in §13. All durations are integer tick counts.
- **Floating-point positions and sub-tile movement** — rejected. §6 defines positions as integer tile
  coordinates; integers make collision, the sword's single-tile hitbox, and cross-platform determinism exact.
- **Rendering only the changed tiles by hand** — rejected as premature: ratatui already diffs its buffer
  against the previous frame and emits only the changed cells, which is exactly what §9 asks for. The work
  that remains is not redrawing when nothing changed (a dirty flag) and never calling a full clear.
- **Depending on `crossterm` directly alongside `ratatui`** — rejected. §8 names version skew between the
  two as a concrete hazard, and having two version requirements in one manifest is the mechanism by which
  that hazard occurs. We depend on `ratatui` only and use its `ratatui::crossterm` re-export (ADR 0003).
- **`directories`/`dirs` crate for the save path** — rejected. `ProjectDirs` produces
  `~/Library/Application Support/<qualifier>.<org>.<app>` on macOS, which is not the literal
  `~/Library/Application Support/mosslight` the spec requires; twenty lines of `std::env` match the spec
  exactly with no dependency.
- **`rand` for randomness** — rejected. A dozen lines of SplitMix64 give reproducible output that is
  identical across platforms and versions forever, which is what §13's determinism test needs; `rand` makes
  no such cross-version guarantee.
- **RON for the save file as well as content** — rejected. Reading only the `version` field before
  committing to a full parse is the mechanism that makes future-version refusal safe, and `serde_json::Value`
  makes that a two-line probe. RON stays where it is better: hand-authored content (ADR 0005, ADR 0006).
- **An external PTY-driven e2e suite** (`expectrl`/`portable-pty`) — rejected by the intake. §13's
  requirements are a headless playthrough and scene assertions, both of which `TestBackend` and ordinary
  `cargo test` cover without a second test surface to keep alive.
- **Unicode as the base tile set** — rejected by §4. ASCII is the base; Unicode is an enhancement that ships
  complete or not at all. Phase 6 resolved that enhancement to "not at all": every Unicode block that would
  look better than ASCII is `East_Asian_Width=Ambiguous` and risks silently doubling a tile's width, and the
  fixed dependency set excludes `unicode-width` to verify it. `--unicode` is withdrawn from the CLI rather
  than shipped half-populated (`docs/user/cli.md`, DECISIONS `[phase-6/plan]`).
- **Storing enemy state in the save** — rejected by §10, and it is the better design anyway: respawning
  enemies on load removes a large class of "save scummed into an unwinnable fight" states.

# Phase 6 — Save slot, full CLI and presentation modes

Goal: make progress durable and the game legible in every mode. One atomic save slot that survives
corruption, truncation and version skew; autosave at the four §10 triggers plus a manual save from the
pause menu; a main menu that can actually continue a run; and three palettes in which colour is never the
only distinction.

## Context

**What exists after phase 5**

| Area | State |
|---|---|
| `src/save.rs` | **Does not exist.** Nothing is ever written to disk. |
| `src/config.rs` | `Config { glyphs, color, theme, fps, save_dir, seed, debug_panic, debug_content }`. `default_save_dir` already implements §10's three paths (macOS / `$XDG_DATA_HOME` / `~/.local/share`), `--save-dir` already overrides, `resolve_color` already implements the §12 `NO_COLOR`/`--color` precedence — all covered by `tests/config.rs`. `GlyphSet::{Ascii, Unicode}` is parsed and stored but **never read by the renderer**. |
| `src/app.rs` | `Mode { MainMenu, Playing, Paused, ConfirmQuit, Help, TooSmall, GameOver, Dialogue, Map, Inventory, Victory }`. `MenuCursor { Continue, NewGame, Help, Quit }` — `Continue` prints `"No save yet"` and does nothing. `checkpoint: GameState` is an in-memory clone refreshed on every `RoomEntered`; `Mode::GameOver` + `Confirm` restores it. No save call sites anywhere. |
| `src/game/state.rs` | `GameState { tick, rng, hero, enemies, world, room, progress, dialogue, puzzle }`. `Progress { visited: BTreeSet<RoomIdx>, opened_chests, lit_torches, solved_puzzles, unlocked_doors: BTreeSet<ObjectRef>, flags: BTreeSet<String> }`. `enter_room()` already rebuilds enemies from authored spawns (skipping a boss whose `defeat_flag` is set) and resets `PuzzleState` — i.e. "enemies respawn, puzzles reset, the boss restarts" is already one function call. |
| `src/content/schema.rs` | Every authored object carries a **globally unique string id** (`check_unique_ids` proves uniqueness per kind across the whole world). `RoomIdx(u16)` is a dense index interned by the loader; its doc comment already names "a future save file" as the string-id boundary. |
| `src/render/theme.rs` | `Theme { name }`, `color_for(kind) -> Color`. `Ansi` aliases `Gameboy`; `Mono` returns `White` for everything. The palette is not four shades of green and `ColorMode` never reaches the renderer at all. |
| `src/render/tiles.rs` | One ASCII `glyph(kind) -> char` table over 24 `Kind`s, with a `HashSet` distinctness test. No theme parameter exists, so a glyph cannot vary by theme — that is the structural half of RISKS #10. |
| `src/main.rs` | Owns the terminal, the pacer and the loop; `Theme::new(config.theme)` is built once. `Diagnostics` buffers messages and flushes to stderr after the guard drops. |
| `tests/` | 24 files. `tests/common/mod.rs` is the shared headless driver (`cfg`, `new_game`, `step`, `idle`, `face`, `walk_to`, `Runner`). `App::new(&cfg(), world())` appears ~30 times across test files. No test ever touches the filesystem. |

**What this phase changes**

1. **`src/save.rs` is new**: the versioned JSON slot, its path helpers, the atomic write split into a
   `stage`/`commit` pair, the two-pass version probe, `LoadOutcome`/`StoreOutcome`, and the
   `GameState <-> SaveFile` translation (dense indices ⇄ stable string ids).
2. **`App` grows a save port.** `App::new` takes a `Box<dyn SaveIo>` third argument; `main` injects
   `FileSaveIo`, tests inject `MemorySaveIo`. `App` decides *when* to save; `SaveIo` decides *where*.
3. **`App` grows the save-facing modes and state**: `SlotState`, `Mode::ConfirmNewGame`,
   `Mode::SaveProblem`, the four autosave triggers, manual save from the pause menu, and the death rule.
4. **`checkpoint: GameState` is deleted**; retry-after-death restores the save slot instead.
5. **`render/theme.rs` gets three real palettes** and learns about `ColorMode`, so `NO_COLOR` and
   `--color never` actually stop colour reaching the terminal.
6. **`--unicode` is withdrawn** (see Design → *The Unicode decision*), taking `GlyphSet::Unicode` with it.

**Key files:** `src/save.rs` (new), `src/app.rs`, `src/config.rs`, `src/main.rs`, `src/lib.rs`,
`src/game/state.rs` (one pure predicate), `src/game/tuning.rs` (one constant), `src/render/theme.rs`,
`src/render/{mod,scene,overlays}.rs`, `tests/save.rs` (new), `tests/render_modes.rs` (new),
`tests/common/mod.rs`, `tests/{config,mode_machine,render,loop_timing,determinism}.rs`.

## Design

### 1. The on-disk format (`src/save.rs`)

Follows ADR 0006 literally. Three paths inside the save dir:

```
<dir>/save.json       the slot
<dir>/save.json.bak   the retained previous copy
<dir>/save.json.tmp   the write staging file
```

```rust
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFile { pub format_version: u32, pub game: SaveGame }

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaveGame {
    pub room: String,                        // room id, not RoomIdx
    pub hero: SaveHero,
    #[serde(default)] pub visited_rooms:   BTreeSet<String>,
    #[serde(default)] pub opened_chests:   BTreeSet<String>,
    #[serde(default)] pub lit_torches:     BTreeSet<String>,
    #[serde(default)] pub solved_puzzles:  BTreeSet<String>,
    #[serde(default)] pub unlocked_doors:  BTreeSet<String>,
    #[serde(default)] pub flags:           BTreeSet<String>,
    #[serde(default)] pub boss_defeated:   bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaveHero {
    pub x: u8, pub y: u8, pub facing: SaveFacing,
    pub health_halves: u8, pub max_health_halves: u8, pub keys: u8,
    pub has_sword: bool, pub has_lantern: bool, pub has_ember: bool,
}
```

`SaveFacing` is save.rs's own `serde` mirror of `game::Facing`, so **no `serde` derive is added to any
`game` type**: the simulation stays free of a serialization format, exactly as `game` is free of `ratatui`
and `std::io`. `BTreeSet<String>` (not `Vec`) makes the JSON ordering stable, so two saves of the same
progress are byte-identical.

Every id set holds **bare authored ids** (`chest.forest_sword`, `door.sanctuary.inner`), which the
validator already proves globally unique per kind (`check_unique_ids`). Storing indices would make
reordering `assets/world.ron` silently corrupt a save (ADR 0005).

`boss_defeated` is stored explicitly because §10 names it, even though it is derivable from `flags`. On
restore the two are reconciled in one direction only: `boss_defeated == true` re-inserts the boss spawn's
`defeat_flag` into `flags` if it is missing. Capture sets it from the flag. A test pins both directions.

Deliberately **not** stored (§10): the in-flight `Swing`, enemy positions/HP/AI, `PuzzleState`, the
dialogue cursor, `tick`, and the RNG state. Also not stored: the seed — see DECISIONS (`[phase-06/plan]`).

### 2. Outcomes and the write sequence

```rust
pub enum LoadOutcome {
    Ok(Box<SaveFile>),
    Missing,
    Corrupt { path: PathBuf, detail: String },
    FutureVersion { found: u32, supported: u32 },
}

pub enum StoreOutcome {
    Ok,
    RefusedFutureVersion { found: u32, supported: u32 },
    RefusedDeathState,
    Failed { detail: String },
}
```

`load(dir)` is the two-pass probe:

1. `fs::read_to_string(save.json)` — `ErrorKind::NotFound` ⇒ `Missing`; any other io error ⇒ `Corrupt`.
2. Parse into `serde_json::Value`. Parse failure (empty file, truncated JSON, garbage) ⇒ `Corrupt`.
3. Read `format_version` as a `u64` from that `Value`. Absent or non-numeric ⇒ `Corrupt`.
   `> FORMAT_VERSION` ⇒ `FutureVersion` — **without ever deserializing the body**, which is exactly what
   makes refusing a newer file safe.
4. `< FORMAT_VERSION` ⇒ run the migration chain (empty at v1; the `match` arm exists and is `unreachable`
   only in the sense that v1 is the lowest version, so it is written as a `Corrupt` fallback, not a panic).
5. `serde_json::from_value::<SaveFile>` ⇒ `Ok` or `Corrupt`.

`load_backup(dir)` is the same function pointed at `save.json.bak`.

Nothing on this path uses `unwrap`, `expect`, `panic!` or indexing; every step is a `match` on a `Result`.
A grep test (`tests/save.rs::no_unwrap_on_the_save_path`) asserts this over `src/save.rs`, mirroring the
existing content-path convention in CLAUDE.md.

`store(dir, save)` is split so the crash window is directly testable:

```rust
pub fn stage(dir: &Path, save: &SaveFile) -> io::Result<()>;  // serialize -> save.json.tmp -> sync_all
pub fn commit(dir: &Path) -> io::Result<()>;                  // copy save.json -> .bak, then rename .tmp -> save.json
pub fn store(dir: &Path, save: &SaveFile) -> StoreOutcome;    // refusals + create_dir_all + stage + commit
```

`store` refuses **before** staging:

- the existing `save.json` probes as `FutureVersion` ⇒ `RefusedFutureVersion`, nothing written;
- `save.game.hero.health_halves == 0` ⇒ `RefusedDeathState`, nothing written. `App` already never asks,
  but the file layer is where the rule is provable without driving a whole game.

A test calls `stage` alone (the "crash before rename" case) and asserts `save.json` is byte-identical and
`load_backup` still returns the previous content.

### 3. Translation: `GameState` ⇄ `SaveFile`

```rust
impl SaveFile {
    pub fn capture(state: &GameState) -> SaveFile;
    pub fn restore(&self, world: Rc<World>, seed: u64) -> Result<Restored, RestoreError>;
}

pub struct Restored { pub state: GameState, pub dropped_ids: Vec<String> }

pub enum RestoreError { UnknownRoom(String), NoSafeSpawn(String) }
```

- `capture` walks each `ObjectRef { room, index }` back to its authored id through
  `world.room(room).chests[index].id` (and the door/torch/puzzle equivalents), skipping any ref that no
  longer resolves.
- `restore` does the inverse by scanning rooms for a matching id. **Unknown ids are dropped into
  `dropped_ids`, never an error** (ADR 0006: adding or renaming content must not make a save unloadable).
  `App` surfaces the count in the message row and pushes the detail into `Diagnostics`.
- The saved `room` id failing to resolve is the one fatal case ⇒ `RestoreError::UnknownRoom` ⇒ `App` treats
  it as `Corrupt`.
- Hero placement: the saved `(x, y)` is used only if `GameState::walkable` accepts it in the restored room;
  otherwise the room's first authored `Spawn` is used; if the room has none ⇒ `RestoreError::NoSafeSpawn`.
  A save is only ever taken at a safe position, so the fallback is a guard against a hand-edited file.
- Post-conditions, all of them already one call away:
  - `tick = 0` and **every hero timer is rebuilt relative to 0** (`step_ready_at = 0`,
    `attack_ready_at = 0`, `attack = None`, `died = false`). Timers are absolute `Tick`s, so restoring a
    saved absolute deadline against a zeroed clock is the same class of bug phase 3 fixed for retry.
  - `hero.invuln_until = tuning::LOAD_SAFE_WINDOW_TICKS` (new constant, `60` = 2 s at 30 Hz) — §10's
    "short safe window".
  - `enter_room()` is called last, which respawns every authored enemy at full HP in its initial AI state,
    skips a boss whose `defeat_flag` is set, and resets `PuzzleState` — so "enemies respawn" and "the boss
    fight restarts from entering the arena" are the same existing mechanism, not new code.
  - `progress.visited` always contains the restored room.

### 4. The save port (`SaveIo`)

```rust
pub trait SaveIo {
    fn load(&mut self) -> LoadOutcome;
    fn load_backup(&mut self) -> LoadOutcome;
    fn store(&mut self, save: &SaveFile) -> StoreOutcome;
}

pub struct FileSaveIo { dir: PathBuf }     // the three free functions above
pub struct MemorySaveIo { … }              // an in-process slot + backup, plus store_count()
```

`App::new(cfg, world, io: Box<dyn SaveIo>)` — the port is a **required** argument, so `main` cannot forget
to inject the real one and silently ship a game that never saves. Existing test call sites gain
`Box::new(MemorySaveIo::new())` (mechanical; `tests/common::new_game` covers most of them). `MemorySaveIo`
lives in the library rather than behind `#[cfg(test)]` because integration tests under `tests/` link
against the library as an external crate.

`MemorySaveIo` also gives every existing headless test real save semantics with zero disk I/O, which is
what lets the retry-after-death tests in `tests/mode_machine.rs` keep working after `checkpoint` is
deleted — with a strictly stronger meaning than before.

This is the only deviation worth naming from ARCHITECTURE's sketch, which put `store`/`load` as free
functions called from the loop; `App` calling them through a port keeps the autosave *policy* in `app`
(where ARCHITECTURE puts it: "turns `GameEvent`s into UI effects and autosave calls") while keeping the
filesystem in one injectable place. Logged in DECISIONS.

### 5. `App`: slot state, modes and triggers

```rust
pub enum SlotState {
    Empty,
    Usable(Box<SaveFile>),
    Corrupt { detail: String },
    FutureVersion { found: u32, supported: u32 },
}
```

`App::new` probes the slot once (`io.load()`) and records `SlotState`; nothing else reads the disk at
startup. A successful `store` replaces `slot` with the file just written, so `Continue` and
retry-after-death never re-read the disk either.

**Modes.** Two are added: `Mode::ConfirmNewGame` and `Mode::SaveProblem`. `ConfirmNewGame` is in
ARCHITECTURE's list already; `SaveProblem` is not — it is the §10 "offer backup restore or a new game"
screen, and folding it into `MainMenu` would mean encoding a second cursor and two item lists into the
menu overlay. Logged in DECISIONS.

Both new screens reuse the existing main-menu interaction shape — `MoveNorth`/`MoveSouth` move a cursor,
`Confirm` selects, `Cancel` backs out — so **`input.rs` is not touched at all**, and §5's "single presses
only, no chords" stays true by construction.

`Mode::SaveProblem` item lists:

| Slot | Items |
|---|---|
| `Corrupt` | `Restore backup`, `New game`, `Back` |
| `FutureVersion` | `New game (this save will not be overwritten)`, `Back` |

`Restore backup` calls `io.load_backup()`: `Ok` ⇒ the backup becomes the slot and the run continues from
it; anything else ⇒ the message row says so and the screen stays open. No path writes anything here, so a
corrupt slot is never silently overwritten.

**Main menu.** `Continue` is labelled from `slot`: `Continue`, `Continue (no save yet)`,
`Continue (save damaged)`, `Continue (save is from a newer version)`. Selecting it with `Empty` keeps the
current "No save yet" message; with `Usable` it restores; with `Corrupt`/`FutureVersion` it opens
`Mode::SaveProblem`. `New Game` over a `Usable` or `FutureVersion` slot routes through
`Mode::ConfirmNewGame` first (§12: "Перезапис наявного проходження через New Game потребує
підтвердження"); over `Empty` it starts directly, as today.

A New Game started over a `FutureVersion` slot plays with saving disabled: each autosave attempt returns
`RefusedFutureVersion`, which `App` turns into one message-row line and one diagnostic. ARCHITECTURE
suggests a differently-named second slot instead; that contradicts §2 ("exactly one slot"), so it is not
implemented. Logged in DECISIONS.

**Autosave triggers.** Folded out of the `GameEvent` stream in `App::tick`, at most one per tick:

| Event | `SaveReason` |
|---|---|
| `RoomEntered { .. }` | `RoomTransition` — the hero is already at the destination spawn when the event is emitted, so the saved position is safe by construction |
| `ItemPicked { reward }` where reward is `Sword \| Lantern \| SmallKey \| HeartContainer \| Ember` | `ImportantItem` (`Reward::Message` is lore, not an item) |
| `PuzzleSolved { .. }` | `PuzzleSolved` |
| `BossDefeated { .. }` | `BossVictory` |

The events are edges, so each fires once by construction; `pending_save: Option<SaveReason>` additionally
collapses a tick that produces two of them into one write. If `HeroDied` appears anywhere in the same
batch, `pending_save` is cleared before it is used — the §10 death rule, enforced twice (here and in
`save::store`).

**Manual save.** `Mode::Paused` + `Confirm` (E/Enter, already mapped). Refused with
`"Cannot save during combat."` when `state.in_combat()`.

```rust
// src/game/state.rs — pure, no new imports
pub fn in_combat(&self) -> bool {
    self.hero.attack.is_some()
        || self.tick < self.hero.invuln_until
        || self.enemies.iter().any(|e| e.alive)
}
```

"A live enemy is in the room" is the conservative reading and the only one that is stable under AI
changes; the swing and invulnerability terms cover the tail of a fight in a room that has just been
cleared. Logged in DECISIONS.

**Retry after death.** `checkpoint: GameState` is deleted. `Mode::GameOver` + `Confirm` restores
`slot` when it is `Usable` (and rewinds `tick_counter` to the restored `state.tick`, i.e. 0); with an
`Empty` slot it starts a fresh run from the world's start, with a message saying so. This is the
deliverable "phase-3's death checkpoint repointed at the save file". `tests/mode_machine.rs`'s two
checkpoint tests are rewritten to assert the *stronger* new guarantee (retry restores the last autosave,
which for a room transition is the same position the old checkpoint held) — no assertion is dropped.

**Save failures never end a session** (ARCHITECTURE's filesystem row): `StoreOutcome::Failed` produces one
message-row line and one `Diagnostics` entry, and play continues; the next trigger retries.
`Diagnostics` reaches `App` as a `Vec<String>` drained by `main` each iteration
(`App::take_diagnostics()`), so `app` still performs no I/O of its own.

### 6. Presentation: three palettes, and colour as a mode

`Theme::new(name: ThemeName, color: ColorMode)`. When `color == ColorMode::Never`, `color_for` returns
`Color::Reset` for every kind — the terminal's own default foreground, i.e. no SGR colour is written at
all. `NO_COLOR` and `--color never` therefore stop at the renderer, not merely in `Config`. `main.rs`
passes `config.color`; `render::draw`'s signature is unchanged.

| Theme | Palette | Rule |
|---|---|---|
| `gameboy` (default) | four `Color::Indexed` greens — `Indexed(155)` lightest, `Indexed(149)`, `Indexed(107)`, `Indexed(22)` darkest | §4's "чотири відтінки зеленого". 256-colour indexed, not truecolor; `--theme ansi` is the documented fallback for a 16-colour terminal |
| `ansi` | only the 16 named `Color` variants (`Black`…`White`, `LightRed`…`LightWhite`) | §4's "палітра ANSI 16 кольорів" |
| `mono` | `White` for everything foreground-significant, `DarkGray` for floor/pit/opened-chest/unlit-torch | §4's "повноцінний монохромний режим" |

Assignment is by *role*, not by object, so the same four greens cover 24 kinds: hero and rewards get the
lightest shade, live threats and interactables the second, structure the third, background the darkest.
Several kinds deliberately share a colour — the glyph is the distinction, which is the whole point of
RISKS #10.

`tiles::glyph` keeps taking no theme argument, so a theme *cannot* change a glyph. `tests/render_modes.rs`
adds the behavioural half: the same `App`, rendered through `TestBackend` under all three themes, produces
byte-identical `symbol()` content for every cell.

### 7. The Unicode decision — the flag is withdrawn

§4 requires "тільки символи з перевіреною шириною" (only characters of verified width) and forbids emoji
and Nerd Fonts. Every Unicode block that would make the tiles *look* better than ASCII — Box Drawing,
Geometric Shapes, Block Elements, arrows, card suits, most Miscellaneous Symbols — is
`East_Asian_Width=Ambiguous`, which renders two columns wide under a CJK locale or with a terminal's
"ambiguous characters are wide" setting enabled. A tile occupies exactly two terminal columns (§4), so an
ambiguous glyph that expands silently shears the whole 24×16 grid. The blocks that *are* unambiguously
Neutral are Latin Extended, IPA and Runic: the Latin ones look like letters (no improvement over ASCII),
and Runic is missing from common macOS monospace fonts (tofu — strictly worse than ASCII). Verifying the
width class in-repo would also need `unicode-width`, which the fixed dependency set excludes.

So: `--unicode` is removed from the CLI, `GlyphSet::Unicode` is deleted, and `GlyphSet` keeps `Ascii`
alone. `--ascii` stays (it is in §12's list and is now simply an explicit affirmation of the default).
This is the branch RISKS #15 and the roadmap deliverable pre-authorise ("or the flag is not offered"), and
it is a named deviation from §12's flag list, recorded in DECISIONS and carried into phase 7's
verification report and `docs/user/cli.md`. `tests/config.rs` proves it: `--help` output contains no
`--unicode`, and `--unicode` is rejected with exit code 2 on stderr.

### 8. Error handling summary

| Failure | Result |
|---|---|
| Save dir missing | `create_dir_all` on first store; failure ⇒ `StoreOutcome::Failed`, one message line, play continues |
| `save.json` missing | `LoadOutcome::Missing` ⇒ `SlotState::Empty` ⇒ "Continue (no save yet)" |
| Empty / truncated / garbage `save.json` | `LoadOutcome::Corrupt` ⇒ `Mode::SaveProblem`, file untouched |
| `format_version` newer | `LoadOutcome::FutureVersion`; `store` refuses; nothing is ever written over it |
| Unknown ids inside a valid save | Dropped into `Restored::dropped_ids`; one message line, run continues |
| Saved room id unknown | `RestoreError::UnknownRoom` ⇒ treated as `Corrupt` ⇒ `Mode::SaveProblem` |
| Saved position not walkable | Falls back to the room's first authored spawn |
| Crash between `stage` and `commit` | `save.json` unchanged, stray `.tmp` overwritten next time |
| Crash between `.bak` copy and `rename` | `save.json` unchanged, `.bak` equals it — both recoverable |

## Tasks

- [x] **T1: `src/save.rs` — the format and the path helpers.** New module, registered in `src/lib.rs`.
  `FORMAT_VERSION`, `SaveFile`/`SaveGame`/`SaveHero`/`SaveFacing` with `serde` derives,
  `save_path`/`backup_path`/`temp_path`. Unit tests in-module: the three path helpers compose the
  documented names; `SaveFacing` round-trips every `game::Facing` variant.
- [x] **T2: `save::{stage, commit, store}` and the refusals.** Atomic write per ADR 0006 (serialize →
  `save.json.tmp` → `sync_all` → copy `save.json` to `save.json.bak` → `rename`), `create_dir_all`,
  `StoreOutcome` with `RefusedFutureVersion` and `RefusedDeathState` checked before staging. No
  `unwrap`/`expect`/`panic!` anywhere in the file.
- [x] **T3: `save::{load, load_backup}` — the two-pass probe.** `LoadOutcome`, `serde_json::Value` version
  probe before body deserialization, `NotFound` ⇒ `Missing`, everything else ⇒ `Corrupt { path, detail }`.
- [x] **T4: `SaveFile::capture` / `SaveFile::restore`.** Dense-index ⇄ string-id translation for rooms,
  chests, torches, puzzles and doors; `Restored { state, dropped_ids }`; `RestoreError`; the walkable-position
  fallback; `tick = 0` with every hero timer rebuilt; `tuning::LOAD_SAFE_WINDOW_TICKS = 60` (new constant in
  `src/game/tuning.rs`); `enter_room()` last; `boss_defeated` ⇄ flag reconciliation.
- [x] **T5: the `SaveIo` port.** `trait SaveIo`, `FileSaveIo`, `MemorySaveIo` (in-process slot + backup +
  `store_count()`), all in `src/save.rs`.
- [x] **T6: `tests/common/mod.rs` — `ScratchDir` and `memory_io()`.** `ScratchDir` creates
  `std::env::temp_dir()/mosslight-test-<name>-<pid>-<counter>`, exposes `path()`, and removes the tree on
  `Drop` (no `tempfile` crate — the dependency set is fixed). `memory_io()` returns a
  `Box<dyn SaveIo>` plus the inspectable handle. Update `cfg()` so `save_dir` is no longer the shared
  `/tmp`.
- [x] **T7: `tests/save.rs` — the file layer.** `round_trip_preserves_every_field` (populated
  `SaveFile` incl. visited/chests/doors/puzzles/flags/boss, asserted field by field *and* as a whole);
  `empty_truncated_and_garbage_saves_load_as_corrupt_and_leave_the_file_untouched` (three cases, bytes
  compared before and after); `a_future_version_is_refused_on_load_and_store_refuses_to_overwrite_it`;
  `a_store_interrupted_before_rename_leaves_the_save_intact_and_the_backup_recoverable` (calls `stage`
  only); `store_refuses_a_death_state`; `a_missing_file_is_missing_not_corrupt`;
  `no_unwrap_on_the_save_path` (grep over `src/save.rs`).
- [x] **T8: `tests/save.rs` — capture/restore.** `restore_rebuilds_progress_from_string_ids`;
  `unknown_ids_are_dropped_and_the_rest_of_the_save_survives`;
  `restore_places_the_hero_on_a_walkable_tile_when_the_saved_position_is_not`;
  `restore_zeroes_the_tick_and_rebuilds_every_hero_timer`.
- [x] **T9: `App` takes the port and the slot.** `App::new(cfg, world, io)` (required third argument),
  `SlotState`, the startup probe, `App::take_diagnostics()`. Mechanically update the ~30 `App::new` call
  sites across `src/main.rs` and `tests/{common,render,mode_machine,loop_timing,determinism}.rs`.
- [x] **T10: `App` — the four autosave triggers, the death rule and the manual save.** `SaveReason`,
  `pending_save` collapsing a tick to one write, the `HeroDied` clear, `GameState::in_combat()` in
  `src/game/state.rs`, `Mode::Paused` + `Confirm` ⇒ manual save, `StoreOutcome::Failed` ⇒ message +
  diagnostic without ending the session.
- [x] **T11: `App` — main menu, `ConfirmNewGame` and `SaveProblem`.** Slot-dependent `Continue` behaviour
  and label, the New Game confirmation over an existing run, the two `SaveProblem` item lists and the
  backup restore. No changes to `src/input.rs`.
- [x] **T12: `App` — retry from the save file.** Delete `checkpoint: GameState`; `Mode::GameOver` +
  `Confirm` restores `slot`, or starts fresh with a message when the slot is empty; `tick_counter` follows
  the restored `state.tick`. Rewrite `tests/mode_machine.rs`'s two checkpoint tests to assert the new,
  stronger guarantee and add `retry_never_writes_a_save`.
- [x] **T13: `src/main.rs` wiring.** Inject `FileSaveIo::new(config.save_dir.clone())`; drain
  `app.take_diagnostics()` into `Diagnostics` each iteration so save failures reach stderr after the guard
  drops and never touch the screen.
- [x] **T14: `tests/save.rs` — the App layer.** `each_autosave_trigger_writes_exactly_once` (four drives:
  room transition, sword pickup, puzzle solve, boss defeat — each asserting `store_count()` goes up by
  exactly one); `a_death_state_never_overwrites_the_last_usable_save`;
  `manual_save_is_refused_during_combat_and_accepted_outside_it`;
  `loading_restores_progress_respawns_enemies_grants_a_safe_window_and_restarts_the_boss_at_arena_entry`;
  `new_game_over_an_existing_run_requires_confirmation`;
  `a_corrupt_slot_offers_backup_restore_and_never_overwrites_the_file`;
  `a_future_version_slot_is_never_overwritten_by_a_new_run`.
- [x] **T15: `render/theme.rs` — three palettes and `ColorMode`.** `Theme::new(name, color)`, the four
  indexed greens, the 16-colour ANSI palette, the mono palette, `Color::Reset` under `ColorMode::Never`.
  Update `src/main.rs`, `src/render/mod.rs` and `tests/render.rs` call sites. Keep and extend the existing
  in-module mono test.
- [x] **T16: withdraw `--unicode`.** Remove the flag and `GlyphSet::Unicode` from `src/config.rs` (and the
  now-pointless `conflicts_with`); `src/render/tiles.rs`'s doc comment updated to say the ASCII table is
  the only table. `tests/config.rs` gains `unicode_flag_is_not_offered` (`--help` text) and
  `unicode_flag_is_rejected_with_exit_2`. Adjust `tests/config.rs::every_flag_parses` and
  `ascii_and_unicode_together_is_rejected` accordingly.
- [x] **T17: `tests/render_modes.rs`.** `the_same_scene_renders_identical_characters_under_every_theme`
  (full-buffer `symbol()` comparison across gameboy/ansi/mono, on a scene containing the hero, a wall, a
  bush, a chest, an NPC and an enemy); `gameboy_uses_only_the_four_authored_greens`;
  `ansi_uses_only_the_sixteen_named_ansi_colors`; `mono_uses_only_white_family_colors`;
  `color_mode_never_writes_no_color_at_all` (every cell `fg == Color::Reset`);
  `no_color_env_disables_color_and_an_explicit_color_flag_overrides_it` (through `Config` into a rendered
  buffer, so the precedence is proved end to end rather than only in `resolve_color`).
- [x] **T18: overlays for the new screens.** `draw_main_menu` takes the slot label, `draw_confirm_new_game`,
  `draw_save_problem`, `draw_pause` gains the `Enter: save` line, `draw_help` gains the save line.
  `tests/render.rs` gains one `TestBackend` assertion per new overlay.
- [x] **T19: docs.** `docs/user/cli.md` (the `--unicode` withdrawal and why, save locations per OS,
  corrupt-save behaviour), `docs/user/controls.md` (manual save from the pause menu),
  `docs/dev/loop-and-modes.md` (the two new modes, the save port, the autosave triggers),
  `docs/dev/testing.md` (`ScratchDir`, why there is no `tempfile`), `CHANGELOG.md`. The full user-doc pass
  belongs to phase 7; this is the delta this phase creates.
- [x] **T20: the phase gate.** `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings
  && cargo test --locked`, then `cargo build --release --locked`.

## Verification

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

Manual, host-run, not part of any gate:

```sh
cargo run --release -- --save-dir /tmp/mosslight-scratch --theme mono
cargo run --release -- --save-dir /tmp/mosslight-scratch --color never
```

| # | Acceptance criterion | Test that proves it |
|---|---|---|
| 1 | Round-trips a fully-populated `SaveFile`; every field survives incl. visited rooms, opened chests, unlocked doors, solved puzzles, flags, boss victory | `tests/save.rs::round_trip_preserves_every_field` |
| 2 | Truncated, garbage and empty save files ⇒ `Corrupt`, no panic, file unmodified | `tests/save.rs::empty_truncated_and_garbage_saves_load_as_corrupt_and_leave_the_file_untouched` |
| 3 | A higher `format_version` ⇒ `FutureVersion`, and a subsequent `store` refuses to overwrite it | `tests/save.rs::a_future_version_is_refused_on_load_and_store_refuses_to_overwrite_it`; app-level: `tests/save.rs::a_future_version_slot_is_never_overwritten_by_a_new_run` |
| 4 | A store interrupted before rename leaves the previous save intact and the `.bak` recoverable | `tests/save.rs::a_store_interrupted_before_rename_leaves_the_save_intact_and_the_backup_recoverable` |
| 5 | A death state is never written over the last usable save | `tests/save.rs::store_refuses_a_death_state` (file layer) + `tests/save.rs::a_death_state_never_overwrites_the_last_usable_save` (app layer) + `tests/mode_machine.rs::retry_never_writes_a_save` |
| 6 | Each of the four autosave triggers writes exactly once; manual save unavailable during combat | `tests/save.rs::each_autosave_trigger_writes_exactly_once`; `tests/save.rs::manual_save_is_refused_during_combat_and_accepted_outside_it` |
| 7 | Loading restores progress, respawns enemies, grants a safe window, resets boss progress to arena entry | `tests/save.rs::loading_restores_progress_respawns_enemies_grants_a_safe_window_and_restarts_the_boss_at_arena_entry`; supported by `tests/save.rs::restore_rebuilds_progress_from_string_ids` and `::restore_zeroes_the_tick_and_rebuilds_every_hero_timer` |
| 8 | `TestBackend`: the rendered characters of the same scene are identical under gameboy, ansi and mono | `tests/render_modes.rs::the_same_scene_renders_identical_characters_under_every_theme` (plus the structural `src/render/tiles.rs::every_glyph_is_distinct`) |
| 9 | `NO_COLOR` disables colour; an explicit `--color` always overrides it | `tests/render_modes.rs::no_color_env_disables_color_and_an_explicit_color_flag_overrides_it` and `::color_mode_never_writes_no_color_at_all`; `tests/config.rs::color_precedence_*` (already present) |
| 10 | `--unicode` renders a complete tile set **or** is absent from `--help` | `tests/config.rs::unicode_flag_is_not_offered` and `::unicode_flag_is_rejected_with_exit_2` (the flag is withdrawn — see Design §7) |
| 11 | Main menu Continue / New Game / Help / Quit; New Game over an existing run requires confirmation | `tests/save.rs::new_game_over_an_existing_run_requires_confirmation`; `tests/render.rs` main-menu and `ConfirmNewGame` overlay assertions |
| 12 | Corrupt / future-version handled in the UI: backup restore or new game, never a silent overwrite | `tests/save.rs::a_corrupt_slot_offers_backup_restore_and_never_overwrites_the_file`; `tests/render.rs` `SaveProblem` overlay assertion |
| 13 | Save path resolution per OS, with `--save-dir` overriding | `tests/config.rs::save_dir_macos_default`, `::save_dir_linux_xdg_default`, `::save_dir_linux_fallback_default`, `::explicit_save_dir_overrides_default` (already present); `tests/save.rs` drives a real `ScratchDir` through `FileSaveIo` |
| 14 | Phase-3's death checkpoint repointed at the save file | `tests/mode_machine.rs::death_opens_game_over_and_retry_restores_the_last_autosave`, `::retry_restores_the_autosaved_room_not_a_fresh_hero` |
| 15 | No `unwrap` on the save path | `tests/save.rs::no_unwrap_on_the_save_path` |
| 16 | The full phase gate passes | The four commands above, run at T20 |

## Risks

| Row | How this phase touches it | What the plan does |
|---|---|---|
| **#7 Save data loss or a panic on a bad save file** | This is the phase that creates the risk surface. | Every mitigation the row names is a task with a test: atomic `stage`/`commit` with `sync_all` (T2), one retained `.bak` (T2, T7), the two-pass version probe before any body parse (T3), `LoadOutcome`/`StoreOutcome` with no `unwrap` and a grep test enforcing it (T3, T7), refusal to overwrite a future version at *both* the file and the app layer (T2, T11), and the death rule enforced twice (T2, T10). |
| **#10 Monochrome or ASCII mode unreadable** | Three palettes ship here. | `tiles::glyph` still takes no theme argument, so a theme cannot change a glyph; T17 adds the behavioural proof that a full rendered buffer is character-identical under all three themes, and asserts each palette stays inside its documented colour family. |
| **#15 Unicode mode ships half-populated** | The decision point is here. | Withdrawn, not half-shipped (Design §7): `GlyphSet::Unicode` is deleted so a partial table cannot exist, and T16 asserts the flag is neither advertised nor accepted. The deviation from §12's flag list goes into DECISIONS and into phase 7's verification report. |
| **#16 Extra dependencies creep in** | Temp directories in tests and save paths both invite a crate. | `ScratchDir` (T6) is ~20 lines over `std::env::temp_dir` instead of `tempfile`; path resolution already exists in `config.rs` without `directories`. `Cargo.toml` is not touched, and `tests/deps.rs` keeps guarding the lock file. |
| **#6 Input feels wrong over SSH** | Two new modes could tempt new key bindings. | `ConfirmNewGame` and `SaveProblem` reuse the existing cursor/`Confirm`/`Cancel` shape; `src/input.rs` is not in any task's file list, so `tests/no_key_release.rs` and `tests/input_policy.rs` keep their meaning unchanged. |
| **#12 Terminal output volume too high over SSH** | A save must not force a redraw storm. | A save sets `dirty` only when it changes the message row, which is at most one line per trigger; no task adds a per-frame write. |

## Out of scope

- **Balance and measurement** (phase 7): tuning values, the playthrough-length assertion, the
  byte-counting writer harness and the CPU measurement.
- **The full user and developer documentation pass** (phase 7): `README.md`,
  `docs/dev/architecture.md`, `docs/dev/verification-report.md`, `HANDOFF.md`, and the complete §5/§12
  tables. T19 writes only the delta this phase creates.
- **The manual host check pass and the SSH/PTY checks** (phase 7), including
  `scripts/terminal-restore-check.sh` runs.
- **A save-format migration chain with real steps**: `FORMAT_VERSION` is 1, so the chain has no entries
  yet. The `match` arm and its `Corrupt` fallback exist; a v1→v2 step is a future change.
- **More than one save slot, save-file checksums, cloud/roaming saves** — excluded by §2 and ADR 0006.
- **A second content pass** (new rooms, dialogue, secrets): `assets/world.ron` is not touched.

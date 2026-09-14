//! The versioned save slot: on-disk format, atomic write, the two-pass version probe, and the
//! `GameState` <-> `SaveFile` translation between dense in-memory indices and the stable authored
//! ids the format is built on (ADR 0006).
//!
//! Nothing here calls `unwrap`/`expect`/`panic!`/`unreachable!` (CLAUDE.md's save-path rule,
//! mirrored for the content path) — `tests/save.rs::no_unwrap_on_the_save_path` greps this file to
//! prove it, so keep every fallible step behind an explicit `match`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::game::{
    EnemyKind, Facing, GameState, Hero, ObjectRef, Pos, Progress, PuzzleState, Rng, RoomIdx, World,
};

/// The on-disk schema version. No migration exists yet (v1 is the floor), so `load`'s "older than
/// current" branch is unreachable in practice but still coded as a `Corrupt` outcome rather than a
/// panic — see `load_value`.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SaveFacing {
    North,
    East,
    #[default]
    South,
    West,
}

impl From<Facing> for SaveFacing {
    fn from(f: Facing) -> Self {
        match f {
            Facing::North => SaveFacing::North,
            Facing::East => SaveFacing::East,
            Facing::South => SaveFacing::South,
            Facing::West => SaveFacing::West,
        }
    }
}

impl From<SaveFacing> for Facing {
    fn from(f: SaveFacing) -> Self {
        match f {
            SaveFacing::North => Facing::North,
            SaveFacing::East => Facing::East,
            SaveFacing::South => Facing::South,
            SaveFacing::West => Facing::West,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaveHero {
    pub x: u8,
    pub y: u8,
    pub facing: SaveFacing,
    pub health_halves: u8,
    pub max_health_halves: u8,
    pub keys: u8,
    pub has_sword: bool,
    pub has_lantern: bool,
    pub has_ember: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SaveGame {
    pub room: String,
    pub hero: SaveHero,
    #[serde(default)]
    pub visited_rooms: BTreeSet<String>,
    #[serde(default)]
    pub opened_chests: BTreeSet<String>,
    #[serde(default)]
    pub lit_torches: BTreeSet<String>,
    #[serde(default)]
    pub solved_puzzles: BTreeSet<String>,
    #[serde(default)]
    pub unlocked_doors: BTreeSet<String>,
    #[serde(default)]
    pub flags: BTreeSet<String>,
    #[serde(default)]
    pub boss_defeated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFile {
    pub format_version: u32,
    pub game: SaveGame,
}

/// What `restore` produced: the rebuilt `GameState`, plus every authored id from the save that no
/// longer resolves against the current world (content grew or was renamed since the save was
/// written). Unknown ids are dropped, never an error — ADR 0006: adding or renaming content must
/// not make an old save unloadable.
pub struct Restored {
    pub state: GameState,
    pub dropped_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    UnknownRoom(String),
    NoSafeSpawn(String),
}

impl SaveFile {
    /// Walks `state.progress` back to authored ids through `world`. A `progress` reference that no
    /// longer resolves (should not happen for a live `GameState`, but content could in principle
    /// have been hot-swapped mid-process by a test) is silently skipped rather than panicking.
    pub fn capture(state: &GameState) -> SaveFile {
        let world = &state.world;
        let room_id = world.room(state.room).id.clone();

        let visited_rooms = state
            .progress
            .visited
            .iter()
            .filter_map(|&idx| world.rooms.get(idx.0 as usize).map(|r| r.id.clone()))
            .collect();
        let opened_chests = state
            .progress
            .opened_chests
            .iter()
            .filter_map(|obj| {
                world
                    .rooms
                    .get(obj.room.0 as usize)
                    .and_then(|r| r.chests.get(obj.index as usize))
                    .map(|c| c.id.clone())
            })
            .collect();
        let lit_torches = state
            .progress
            .lit_torches
            .iter()
            .filter_map(|obj| {
                world
                    .rooms
                    .get(obj.room.0 as usize)
                    .and_then(|r| r.torches.get(obj.index as usize))
                    .map(|t| t.id.clone())
            })
            .collect();
        let solved_puzzles = state
            .progress
            .solved_puzzles
            .iter()
            .filter_map(|obj| {
                world
                    .rooms
                    .get(obj.room.0 as usize)
                    .and_then(|r| r.puzzles.get(obj.index as usize))
                    .map(|p| p.id.clone())
            })
            .collect();
        let unlocked_doors = state
            .progress
            .unlocked_doors
            .iter()
            .filter_map(|obj| {
                world
                    .rooms
                    .get(obj.room.0 as usize)
                    .and_then(|r| r.doors.get(obj.index as usize))
                    .map(|d| d.id.clone())
            })
            .collect();
        let flags = state.progress.flags.clone();
        let boss_defeated = boss_defeat_flag(world).is_some_and(|f| flags.contains(&f));

        let hero = SaveHero {
            x: state.hero.pos.x,
            y: state.hero.pos.y,
            facing: state.hero.facing.into(),
            health_halves: state.hero.health_halves,
            max_health_halves: state.hero.max_health_halves,
            keys: state.hero.keys,
            has_sword: state.hero.has_sword,
            has_lantern: state.hero.has_lantern,
            has_ember: state.hero.has_ember,
        };

        SaveFile {
            format_version: FORMAT_VERSION,
            game: SaveGame {
                room: room_id,
                hero,
                visited_rooms,
                opened_chests,
                lit_torches,
                solved_puzzles,
                unlocked_doors,
                flags,
                boss_defeated,
            },
        }
    }

    /// The inverse of `capture`: rebuilds a fresh `GameState` at `tick = 0` with every hero timer
    /// zeroed and a short post-load invulnerability window (spec §10), enemies respawned and the
    /// puzzle state reseeded via the ordinary `GameState::enter_room` path. The saved room id
    /// failing to resolve is the one fatal case; every other unresolved id is dropped and reported
    /// via `Restored::dropped_ids` instead.
    pub fn restore(&self, world: Rc<World>, seed: u64) -> Result<Restored, RestoreError> {
        let game = &self.game;
        let Some(room) = world.room_idx(&game.room) else {
            return Err(RestoreError::UnknownRoom(game.room.clone()));
        };

        let mut dropped_ids = Vec::new();

        let mut visited = BTreeSet::new();
        visited.insert(room);
        for id in &game.visited_rooms {
            match world.room_idx(id) {
                Some(idx) => {
                    visited.insert(idx);
                }
                None => dropped_ids.push(id.clone()),
            }
        }

        let mut opened_chests = BTreeSet::new();
        for id in &game.opened_chests {
            match find_chest(&world, id) {
                Some(obj) => {
                    opened_chests.insert(obj);
                }
                None => dropped_ids.push(id.clone()),
            }
        }

        let mut lit_torches = BTreeSet::new();
        for id in &game.lit_torches {
            match find_torch(&world, id) {
                Some(obj) => {
                    lit_torches.insert(obj);
                }
                None => dropped_ids.push(id.clone()),
            }
        }

        let mut solved_puzzles = BTreeSet::new();
        for id in &game.solved_puzzles {
            match find_puzzle(&world, id) {
                Some(obj) => {
                    solved_puzzles.insert(obj);
                }
                None => dropped_ids.push(id.clone()),
            }
        }

        let mut unlocked_doors = BTreeSet::new();
        for id in &game.unlocked_doors {
            match find_door(&world, id) {
                Some(obj) => {
                    unlocked_doors.insert(obj);
                }
                None => dropped_ids.push(id.clone()),
            }
        }

        let mut flags = game.flags.clone();
        if game.boss_defeated {
            if let Some(flag) = boss_defeat_flag(&world) {
                flags.insert(flag);
            }
        }

        let progress = Progress {
            visited,
            opened_chests,
            lit_torches,
            solved_puzzles,
            unlocked_doors,
            flags,
        };

        let Some(fallback) = world.room(room).spawns.first().map(|s| s.at) else {
            return Err(RestoreError::NoSafeSpawn(game.room.clone()));
        };

        let mut state = GameState {
            tick: 0,
            rng: Rng::new(seed),
            hero: Hero::at_spawn(fallback),
            enemies: Vec::new(),
            world,
            room,
            progress,
            dialogue: None,
            puzzle: PuzzleState::default(),
        };
        // Rebuilds enemies/puzzle state from the room's authored content, using `progress.flags`
        // to skip a boss already marked defeated — the same mechanism a live room transition uses.
        state.enter_room();

        let saved_pos = Pos {
            x: game.hero.x,
            y: game.hero.y,
        };
        state.hero.pos = if state.walkable(room, saved_pos) {
            saved_pos
        } else {
            fallback
        };
        state.hero.facing = game.hero.facing.into();
        state.hero.health_halves = game.hero.health_halves;
        state.hero.max_health_halves = game.hero.max_health_halves;
        state.hero.keys = game.hero.keys;
        state.hero.has_sword = game.hero.has_sword;
        state.hero.has_lantern = game.hero.has_lantern;
        state.hero.has_ember = game.hero.has_ember;
        // Every hero timer is an absolute `Tick`; restoring one against a clock rewound to 0 would
        // fire every cooldown and invulnerability window at once if left untouched (the same class
        // of bug phase 3 fixed for death/retry — see DECISIONS.md).
        state.hero.step_ready_at = 0;
        state.hero.attack_ready_at = 0;
        state.hero.attack = None;
        state.hero.died = false;
        state.hero.invuln_until = crate::game::tuning::LOAD_SAFE_WINDOW_TICKS;

        Ok(Restored { state, dropped_ids })
    }
}

/// The world's one boss spawn's defeat flag, if any — `boss_defeated` is reconciled against this
/// in one direction only (see `capture`/`restore`): capture reads it from `progress.flags`, restore
/// re-inserts it into `flags` if `boss_defeated` is set but the flag itself was dropped.
fn boss_defeat_flag(world: &World) -> Option<String> {
    world
        .rooms
        .iter()
        .flat_map(|r| r.enemies.iter())
        .find(|e| e.kind == EnemyKind::Boss)
        .and_then(|e| e.defeat_flag.clone())
}

fn find_chest(world: &World, id: &str) -> Option<ObjectRef> {
    world.rooms.iter().enumerate().find_map(|(ri, r)| {
        r.chests
            .iter()
            .position(|c| c.id == id)
            .map(|ci| ObjectRef {
                room: RoomIdx(ri as u16),
                index: ci as u16,
            })
    })
}

fn find_torch(world: &World, id: &str) -> Option<ObjectRef> {
    world.rooms.iter().enumerate().find_map(|(ri, r)| {
        r.torches
            .iter()
            .position(|t| t.id == id)
            .map(|ti| ObjectRef {
                room: RoomIdx(ri as u16),
                index: ti as u16,
            })
    })
}

fn find_puzzle(world: &World, id: &str) -> Option<ObjectRef> {
    world.rooms.iter().enumerate().find_map(|(ri, r)| {
        r.puzzles
            .iter()
            .position(|p| p.id == id)
            .map(|pi| ObjectRef {
                room: RoomIdx(ri as u16),
                index: pi as u16,
            })
    })
}

fn find_door(world: &World, id: &str) -> Option<ObjectRef> {
    world.rooms.iter().enumerate().find_map(|(ri, r)| {
        r.doors.iter().position(|d| d.id == id).map(|di| ObjectRef {
            room: RoomIdx(ri as u16),
            index: di as u16,
        })
    })
}

pub fn save_path(dir: &Path) -> PathBuf {
    dir.join("save.json")
}

pub fn backup_path(dir: &Path) -> PathBuf {
    dir.join("save.json.bak")
}

pub fn temp_path(dir: &Path) -> PathBuf {
    dir.join("save.json.tmp")
}

#[derive(Debug)]
pub enum LoadOutcome {
    Ok(Box<SaveFile>),
    Missing,
    Corrupt { path: PathBuf, detail: String },
    FutureVersion { found: u32, supported: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOutcome {
    Ok,
    RefusedFutureVersion { found: u32, supported: u32 },
    RefusedDeathState,
    Failed { detail: String },
}

/// The two-pass probe: read the version out of a generic `serde_json::Value` before ever
/// deserializing the body into `SaveFile`, so a newer save's unknown fields never need to parse
/// successfully for `store` to safely refuse overwriting it.
fn load_file(path: &Path) -> LoadOutcome {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return LoadOutcome::Missing,
        Err(e) => {
            return LoadOutcome::Corrupt {
                path: path.to_path_buf(),
                detail: e.to_string(),
            }
        }
    };

    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return LoadOutcome::Corrupt {
                path: path.to_path_buf(),
                detail: e.to_string(),
            }
        }
    };

    let version = match value.get("format_version").and_then(|v| v.as_u64()) {
        Some(v) => v,
        None => {
            return LoadOutcome::Corrupt {
                path: path.to_path_buf(),
                detail: "missing or non-numeric format_version".to_string(),
            }
        }
    };

    if version > u64::from(FORMAT_VERSION) {
        return LoadOutcome::FutureVersion {
            found: version as u32,
            supported: FORMAT_VERSION,
        };
    }
    if version < u64::from(FORMAT_VERSION) {
        // No migration exists below v1; a lower version is content this build has never written.
        return LoadOutcome::Corrupt {
            path: path.to_path_buf(),
            detail: format!("no migration is defined from format_version {version}"),
        };
    }

    match serde_json::from_value::<SaveFile>(value) {
        Ok(save) => LoadOutcome::Ok(Box::new(save)),
        Err(e) => LoadOutcome::Corrupt {
            path: path.to_path_buf(),
            detail: e.to_string(),
        },
    }
}

pub fn load(dir: &Path) -> LoadOutcome {
    load_file(&save_path(dir))
}

pub fn load_backup(dir: &Path) -> LoadOutcome {
    load_file(&backup_path(dir))
}

/// Serializes `save` to `save.json.tmp` and `sync_all`s it — the half of the write that can run
/// alone (see `tests/save.rs`'s crash-before-rename case): nothing at `save_path` is touched yet.
pub fn stage(dir: &Path, save: &SaveFile) -> io::Result<()> {
    let bytes = match serde_json::to_vec_pretty(save) {
        Ok(b) => b,
        Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
    };
    let mut file = fs::File::create(temp_path(dir))?;
    file.write_all(&bytes)?;
    file.sync_all()
}

/// Copies the current `save.json` to `save.json.bak` (if one exists *and* it still loads, i.e. is
/// not itself `Corrupt`) and renames the staged `save.json.tmp` over `save.json`. A crash between
/// the copy and the rename still leaves `save.json` untouched and `save.json.bak` recoverable.
///
/// The loadability check is what keeps a backup restore useful: without it, the very next
/// autosave after `App::restore_backup` would promote the still-corrupt `save.json` over the good
/// `.bak` it was just recovered from, destroying the only valid copy before the rename below even
/// runs (review round 1, minor #3).
pub fn commit(dir: &Path) -> io::Result<()> {
    let save = save_path(dir);
    let backup = backup_path(dir);
    let temp = temp_path(dir);
    if matches!(
        load_file(&save),
        LoadOutcome::Ok(_) | LoadOutcome::FutureVersion { .. }
    ) {
        fs::copy(&save, &backup)?;
    }
    fs::rename(&temp, &save)
}

/// `stage` + `commit`, gated by the two refusals ADR 0006 requires: a death state is never written
/// (the retry rule is enforced here as well as at the `App` layer), and an existing save that
/// probes as a newer format version is never overwritten by an older build.
pub fn store(dir: &Path, save: &SaveFile) -> StoreOutcome {
    if save.game.hero.health_halves == 0 {
        return StoreOutcome::RefusedDeathState;
    }
    if let LoadOutcome::FutureVersion { found, supported } = load(dir) {
        return StoreOutcome::RefusedFutureVersion { found, supported };
    }
    if let Err(e) = fs::create_dir_all(dir) {
        return StoreOutcome::Failed {
            detail: e.to_string(),
        };
    }
    if let Err(e) = stage(dir, save) {
        return StoreOutcome::Failed {
            detail: e.to_string(),
        };
    }
    if let Err(e) = commit(dir) {
        return StoreOutcome::Failed {
            detail: e.to_string(),
        };
    }
    StoreOutcome::Ok
}

/// Where `App` decides *when* to save; a `SaveIo` decides *where*. `FileSaveIo` is what `main`
/// injects; `MemorySaveIo` is what every test injects instead, so `App`'s save-facing behaviour is
/// provable without touching a filesystem.
pub trait SaveIo {
    fn load(&mut self) -> LoadOutcome;
    fn load_backup(&mut self) -> LoadOutcome;
    fn store(&mut self, save: &SaveFile) -> StoreOutcome;
}

pub struct FileSaveIo {
    dir: PathBuf,
}

impl FileSaveIo {
    pub fn new(dir: PathBuf) -> Self {
        FileSaveIo { dir }
    }
}

impl SaveIo for FileSaveIo {
    fn load(&mut self) -> LoadOutcome {
        load(&self.dir)
    }

    fn load_backup(&mut self) -> LoadOutcome {
        load_backup(&self.dir)
    }

    fn store(&mut self, save: &SaveFile) -> StoreOutcome {
        store(&self.dir, save)
    }
}

#[derive(Default)]
struct MemorySlot {
    slot: Option<SaveFile>,
    backup: Option<SaveFile>,
    store_count: u32,
}

/// An in-process slot + backup, so a headless test gets real save semantics (including the
/// future-version and death-state refusals) with zero disk I/O. `Clone`s share the same
/// `Rc<RefCell<_>>` slot, which is what lets a test keep an inspectable handle after moving a
/// `Box<dyn SaveIo>` into an `App`.
#[derive(Clone, Default)]
pub struct MemorySaveIo(Rc<RefCell<MemorySlot>>);

impl MemorySaveIo {
    pub fn new() -> Self {
        MemorySaveIo::default()
    }

    /// Pre-loads the slot, e.g. to test `Continue`/retry against an existing (possibly
    /// future-version) save without a `store` call first.
    pub fn seeded(save: SaveFile) -> Self {
        let io = MemorySaveIo::default();
        io.0.borrow_mut().slot = Some(save);
        io
    }

    pub fn store_count(&self) -> u32 {
        self.0.borrow().store_count
    }

    pub fn slot(&self) -> Option<SaveFile> {
        self.0.borrow().slot.clone()
    }
}

fn memory_load(save: &Option<SaveFile>) -> LoadOutcome {
    match save {
        None => LoadOutcome::Missing,
        Some(s) if s.format_version > FORMAT_VERSION => LoadOutcome::FutureVersion {
            found: s.format_version,
            supported: FORMAT_VERSION,
        },
        Some(s) => LoadOutcome::Ok(Box::new(s.clone())),
    }
}

impl SaveIo for MemorySaveIo {
    fn load(&mut self) -> LoadOutcome {
        memory_load(&self.0.borrow().slot)
    }

    fn load_backup(&mut self) -> LoadOutcome {
        memory_load(&self.0.borrow().backup)
    }

    fn store(&mut self, save: &SaveFile) -> StoreOutcome {
        if save.game.hero.health_halves == 0 {
            return StoreOutcome::RefusedDeathState;
        }
        let mut inner = self.0.borrow_mut();
        if let Some(existing) = &inner.slot {
            if existing.format_version > FORMAT_VERSION {
                return StoreOutcome::RefusedFutureVersion {
                    found: existing.format_version,
                    supported: FORMAT_VERSION,
                };
            }
        }
        inner.backup = inner.slot.clone();
        inner.slot = Some(save.clone());
        inner.store_count += 1;
        StoreOutcome::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_helpers_compose_the_documented_names() {
        let dir = Path::new("/tmp/mosslight-example");
        assert_eq!(save_path(dir), dir.join("save.json"));
        assert_eq!(backup_path(dir), dir.join("save.json.bak"));
        assert_eq!(temp_path(dir), dir.join("save.json.tmp"));
    }

    #[test]
    fn save_facing_round_trips_every_facing_variant() {
        for facing in [Facing::North, Facing::East, Facing::South, Facing::West] {
            let saved: SaveFacing = facing.into();
            let back: Facing = saved.into();
            assert_eq!(back, facing);
        }
    }

    #[test]
    fn memory_save_io_reports_missing_then_usable_then_counts_stores() {
        let mut io = MemorySaveIo::new();
        assert!(matches!(io.load(), LoadOutcome::Missing));

        let save = SaveFile {
            format_version: FORMAT_VERSION,
            game: SaveGame {
                room: "room.a".to_string(),
                hero: SaveHero {
                    health_halves: 6,
                    max_health_halves: 6,
                    ..SaveHero::default()
                },
                ..SaveGame::default()
            },
        };
        assert_eq!(io.store(&save), StoreOutcome::Ok);
        assert_eq!(io.store_count(), 1);
        assert!(matches!(io.load(), LoadOutcome::Ok(_)));
    }

    #[test]
    fn memory_save_io_refuses_a_death_state() {
        let mut io = MemorySaveIo::new();
        let save = SaveFile {
            format_version: FORMAT_VERSION,
            game: SaveGame {
                room: "room.a".to_string(),
                hero: SaveHero {
                    health_halves: 0,
                    ..SaveHero::default()
                },
                ..SaveGame::default()
            },
        };
        assert_eq!(io.store(&save), StoreOutcome::RefusedDeathState);
        assert_eq!(io.store_count(), 0);
    }
}

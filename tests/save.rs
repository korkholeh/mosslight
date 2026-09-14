//! The save slot (spec §10, ADR 0006): the file layer (round-trip, corruption, version skew,
//! the atomic write's crash windows), the `GameState` <-> `SaveFile` translation, and the `App`
//! layer built on top of it (the four autosave triggers, the death rule, manual save, retry, the
//! main menu's slot-dependent behaviour, and recovery from a corrupt or future-version slot).

mod common;

use std::rc::Rc;

use mosslight::app::{App, MenuCursor, Mode, SaveProblemAction, SlotState};
use mosslight::game::tuning::{BAT_HP, BOSS_HP, LOAD_SAFE_WINDOW_TICKS};
use mosslight::game::{
    Action, AiState, Enemy, EnemyId, EnemyKind, Facing, GameState, ObjectRef, Pos, RoomIdx, World,
};
use mosslight::save::{
    self, FileSaveIo, LoadOutcome, MemorySaveIo, SaveFile, SaveGame, SaveHero, StoreOutcome,
    FORMAT_VERSION,
};

fn sample_save(health_halves: u8) -> SaveFile {
    SaveFile {
        format_version: FORMAT_VERSION,
        game: SaveGame {
            room: "room.test".to_string(),
            hero: SaveHero {
                health_halves,
                max_health_halves: 6,
                ..SaveHero::default()
            },
            ..SaveGame::default()
        },
    }
}

fn any_chest_ref(world: &World) -> ObjectRef {
    for (ri, room) in world.rooms.iter().enumerate() {
        if !room.chests.is_empty() {
            return ObjectRef {
                room: RoomIdx(ri as u16),
                index: 0,
            };
        }
    }
    panic!("the embedded world has no chest");
}

fn any_torch_ref(world: &World) -> ObjectRef {
    for (ri, room) in world.rooms.iter().enumerate() {
        if !room.torches.is_empty() {
            return ObjectRef {
                room: RoomIdx(ri as u16),
                index: 0,
            };
        }
    }
    panic!("the embedded world has no torch");
}

fn any_puzzle_ref(world: &World) -> ObjectRef {
    for (ri, room) in world.rooms.iter().enumerate() {
        if !room.puzzles.is_empty() {
            return ObjectRef {
                room: RoomIdx(ri as u16),
                index: 0,
            };
        }
    }
    panic!("the embedded world has no puzzle");
}

fn any_door_ref(world: &World) -> ObjectRef {
    for (ri, room) in world.rooms.iter().enumerate() {
        if !room.doors.is_empty() {
            return ObjectRef {
                room: RoomIdx(ri as u16),
                index: 0,
            };
        }
    }
    panic!("the embedded world has no door");
}

// --- File layer (T7) ---

#[test]
fn round_trip_preserves_every_field() {
    let world = common::world();
    let mut state = GameState::new(1, Rc::clone(&world));
    state.hero.has_sword = true;
    state.hero.has_lantern = true;
    state.hero.has_ember = true;
    state.hero.keys = 2;
    state.hero.health_halves = 5;
    state.hero.max_health_halves = 8;
    state.hero.facing = Facing::West;
    state.progress.visited.insert(state.room);
    state.progress.opened_chests.insert(any_chest_ref(&world));
    state.progress.lit_torches.insert(any_torch_ref(&world));
    state.progress.solved_puzzles.insert(any_puzzle_ref(&world));
    state.progress.unlocked_doors.insert(any_door_ref(&world));
    state.progress.flags.insert("flag.example".to_string());
    // Pins the `capture` direction of the boss_defeated <-> flags reconciliation (PLAN.md Design
    // §1: "A test pins both directions") — without it, a wrong `boss_defeat_flag(world)` lookup
    // in `SaveFile::capture` would go unnoticed (review round 1, minor #1).
    state
        .progress
        .flags
        .insert("flag.boss_defeated".to_string());

    let save = SaveFile::capture(&state);
    assert!(
        save.game.boss_defeated,
        "capture must set boss_defeated from the boss spawn's defeat_flag in progress.flags"
    );

    let scratch = common::ScratchDir::new("round-trip");
    assert_eq!(save::store(scratch.path(), &save), StoreOutcome::Ok);
    let loaded = match save::load(scratch.path()) {
        LoadOutcome::Ok(loaded) => loaded,
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(*loaded, save);
    assert_eq!(loaded.game.hero.keys, 2);
    assert_eq!(loaded.game.hero.health_halves, 5);
    assert_eq!(loaded.game.hero.max_health_halves, 8);
    assert!(loaded.game.hero.has_sword);
    assert!(loaded.game.hero.has_lantern);
    assert!(loaded.game.hero.has_ember);
    assert_eq!(loaded.game.opened_chests.len(), 1);
    assert_eq!(loaded.game.lit_torches.len(), 1);
    assert_eq!(loaded.game.solved_puzzles.len(), 1);
    assert_eq!(loaded.game.unlocked_doors.len(), 1);
    assert!(loaded.game.flags.contains("flag.example"));
    assert!(
        loaded.game.boss_defeated,
        "boss_defeated must survive the store/load round trip"
    );
}

#[test]
fn empty_truncated_and_garbage_saves_load_as_corrupt_and_leave_the_file_untouched() {
    for (case, bytes) in [
        ("empty", &b""[..]),
        ("truncated", &br#"{"format_version": 1, "game": {"#[..]),
        ("garbage", &b"not json at all"[..]),
    ] {
        let scratch = common::ScratchDir::new(&format!("corrupt-{case}"));
        std::fs::write(save::save_path(scratch.path()), bytes).expect("write fixture");

        match save::load(scratch.path()) {
            LoadOutcome::Corrupt { .. } => {}
            other => panic!("{case}: expected Corrupt, got {other:?}"),
        }

        let after = std::fs::read(save::save_path(scratch.path())).expect("read back");
        assert_eq!(after, bytes, "{case}: load must never modify the file");
    }
}

#[test]
fn a_future_version_is_refused_on_load_and_store_refuses_to_overwrite_it() {
    let scratch = common::ScratchDir::new("future-version-file");
    let future = serde_json::json!({"format_version": 999, "game": {}});
    std::fs::write(save::save_path(scratch.path()), future.to_string()).expect("write fixture");

    match save::load(scratch.path()) {
        LoadOutcome::FutureVersion { found, supported } => {
            assert_eq!(found, 999);
            assert_eq!(supported, FORMAT_VERSION);
        }
        other => panic!("expected FutureVersion, got {other:?}"),
    }

    let before = std::fs::read(save::save_path(scratch.path())).expect("read before");
    let outcome = save::store(scratch.path(), &sample_save(6));
    assert_eq!(
        outcome,
        StoreOutcome::RefusedFutureVersion {
            found: 999,
            supported: FORMAT_VERSION
        }
    );
    let after = std::fs::read(save::save_path(scratch.path())).expect("read after");
    assert_eq!(before, after, "store must never overwrite a newer save");
}

#[test]
fn a_store_interrupted_before_rename_leaves_the_save_intact_and_the_backup_recoverable() {
    let scratch = common::ScratchDir::new("stage-only");
    let save1 = sample_save(6);
    let save2 = sample_save(5);
    let save3 = sample_save(4);

    assert_eq!(save::store(scratch.path(), &save1), StoreOutcome::Ok);
    assert_eq!(save::store(scratch.path(), &save2), StoreOutcome::Ok);
    // Simulates a crash between staging and the rename: `save.json` must stay exactly `save2`,
    // and the retained backup must stay exactly `save1`.
    save::stage(scratch.path(), &save3).expect("stage must not fail");

    match save::load(scratch.path()) {
        LoadOutcome::Ok(loaded) => assert_eq!(*loaded, save2),
        other => panic!("expected Ok(save2), got {other:?}"),
    }
    match save::load_backup(scratch.path()) {
        LoadOutcome::Ok(loaded) => assert_eq!(*loaded, save1),
        other => panic!("expected Ok(save1), got {other:?}"),
    }
}

/// Review round 1, minor #3: `commit` used to copy `save.json` to `.bak` unconditionally, so the
/// very next store after a corrupt `save.json` (e.g. right after the player picks "Restore
/// backup" from `Mode::SaveProblem` and keeps playing) would promote the still-corrupt file over
/// the one good backup it was just recovered from. `commit` now probes `save.json` first and only
/// promotes it when it still loads.
#[test]
fn commit_does_not_promote_a_corrupt_save_file_over_a_good_backup() {
    let scratch = common::ScratchDir::new("commit-corrupt-promote");
    let save1 = sample_save(6);
    let save2 = sample_save(5);
    assert_eq!(save::store(scratch.path(), &save1), StoreOutcome::Ok);
    assert_eq!(save::store(scratch.path(), &save2), StoreOutcome::Ok);
    // save.json = save2, save.json.bak = save1, both valid.

    // Simulates the save file getting corrupted out of band (e.g. a crash mid-write on some
    // earlier version of this code, or disk damage) — never through this module's own atomic
    // write path.
    std::fs::write(save::save_path(scratch.path()), b"not json").expect("corrupt save.json");

    let save3 = sample_save(4);
    assert_eq!(save::store(scratch.path(), &save3), StoreOutcome::Ok);

    match save::load_backup(scratch.path()) {
        LoadOutcome::Ok(loaded) => assert_eq!(
            *loaded, save1,
            "the last good backup must survive a store over a corrupt save.json"
        ),
        other => panic!("expected the backup to still be Ok(save1), got {other:?}"),
    }
    match save::load(scratch.path()) {
        LoadOutcome::Ok(loaded) => assert_eq!(*loaded, save3),
        other => panic!("expected Ok(save3), got {other:?}"),
    }
}

#[test]
fn store_refuses_a_death_state() {
    let scratch = common::ScratchDir::new("death-state");
    let outcome = save::store(scratch.path(), &sample_save(0));
    assert_eq!(outcome, StoreOutcome::RefusedDeathState);
    assert!(
        !save::save_path(scratch.path()).exists(),
        "a death state must never be written at all"
    );
}

#[test]
fn a_missing_file_is_missing_not_corrupt() {
    let scratch = common::ScratchDir::new("missing");
    assert!(matches!(save::load(scratch.path()), LoadOutcome::Missing));
}

/// Mirrors CLAUDE.md's content-path convention: no `unwrap`/`expect`/`panic!`/`unreachable!`
/// anywhere on the save path. Only scans the portion of the file above its own `#[cfg(test)]`
/// unit-test module, so `src/save.rs`'s in-module tests (which are allowed to use them) are not
/// part of what is checked.
#[test]
fn no_unwrap_on_the_save_path() {
    let src = std::fs::read_to_string("src/save.rs").expect("read src/save.rs");
    let code = src
        .split("#[cfg(test)]")
        .next()
        .expect("split always yields at least one part");
    for token in ["unwrap(", ".expect(", "panic!(", "unreachable!("] {
        assert!(
            !code.contains(token),
            "found forbidden token {token:?} in src/save.rs outside its test module"
        );
    }
}

// --- Capture/restore (T8) ---

#[test]
fn restore_rebuilds_progress_from_string_ids() {
    let world = common::world();
    let mut state = GameState::new(1, Rc::clone(&world));
    state.progress.opened_chests.insert(any_chest_ref(&world));
    state.progress.lit_torches.insert(any_torch_ref(&world));
    state.progress.solved_puzzles.insert(any_puzzle_ref(&world));
    state.progress.unlocked_doors.insert(any_door_ref(&world));
    state.progress.flags.insert("flag.example".to_string());

    let save = SaveFile::capture(&state);
    let restored = save
        .restore(Rc::clone(&world), 1)
        .expect("restore succeeds");
    assert!(restored.dropped_ids.is_empty());
    assert_eq!(restored.state.progress, state.progress);
}

#[test]
fn unknown_ids_are_dropped_and_the_rest_of_the_save_survives() {
    let world = common::world();
    let real_chest = any_chest_ref(&world);
    let real_chest_id = world.room(real_chest.room).chests[real_chest.index as usize]
        .id
        .clone();

    let save = SaveFile {
        format_version: FORMAT_VERSION,
        game: SaveGame {
            room: world.room(world.start_room().unwrap()).id.clone(),
            hero: SaveHero {
                health_halves: 6,
                max_health_halves: 6,
                ..SaveHero::default()
            },
            opened_chests: [real_chest_id, "chest.does_not_exist".to_string()]
                .into_iter()
                .collect(),
            ..SaveGame::default()
        },
    };

    let restored = save
        .restore(Rc::clone(&world), 1)
        .expect("restore succeeds");
    assert_eq!(
        restored.dropped_ids,
        vec!["chest.does_not_exist".to_string()]
    );
    assert_eq!(restored.state.progress.opened_chests.len(), 1);
    assert!(restored.state.progress.opened_chests.contains(&real_chest));
}

#[test]
fn restore_places_the_hero_on_a_walkable_tile_when_the_saved_position_is_not() {
    let world = common::world();
    let start_room = world.start_room().unwrap();
    let save = SaveFile {
        format_version: FORMAT_VERSION,
        game: SaveGame {
            room: world.room(start_room).id.clone(),
            hero: SaveHero {
                // (0, 0) is always the wall-ring corner (see `tests/render.rs`'s scene-placement
                // comments), never walkable.
                x: 0,
                y: 0,
                health_halves: 6,
                max_health_halves: 6,
                ..SaveHero::default()
            },
            ..SaveGame::default()
        },
    };

    let restored = save
        .restore(Rc::clone(&world), 1)
        .expect("restore succeeds");
    let fallback = world
        .room(start_room)
        .spawns
        .first()
        .expect("a spawn exists")
        .at;
    assert_eq!(restored.state.hero.pos, fallback);
}

#[test]
fn restore_zeroes_the_tick_and_rebuilds_every_hero_timer() {
    let world = common::world();
    let mut state = GameState::new(1, Rc::clone(&world));
    state.tick = 500;
    state.hero.step_ready_at = 500;
    state.hero.attack_ready_at = 500;
    state.hero.invuln_until = 500;
    state.hero.health_halves = 6;

    let save = SaveFile::capture(&state);
    let restored = save
        .restore(Rc::clone(&world), 1)
        .expect("restore succeeds");
    assert_eq!(restored.state.tick, 0);
    assert_eq!(restored.state.hero.step_ready_at, 0);
    assert_eq!(restored.state.hero.attack_ready_at, 0);
    assert!(restored.state.hero.attack.is_none());
    assert!(!restored.state.hero.died);
    assert_eq!(restored.state.hero.invuln_until, LOAD_SAFE_WINDOW_TICKS);
}

/// Closes the gap review round 1's MAJOR finding raised: criterion 7's "respawns enemies" was
/// never actually driven — only the boss-skip path was. `room.stone_circle` authors two `Bat`
/// spawns; one is killed before capture, and `restore` must bring both back alive at full hp on
/// their authored tiles, exactly as a live `enter_room()` would.
#[test]
fn restore_respawns_every_authored_enemy_at_full_health() {
    let world = common::world();
    let stone_circle = world
        .room_idx("room.stone_circle")
        .expect("room.stone_circle exists in the embedded world");
    let mut state = GameState::new(1, Rc::clone(&world));
    state.room = stone_circle;
    state.enter_room();
    assert_eq!(
        state.enemies.len(),
        2,
        "room.stone_circle authors two enemies"
    );
    state.enemies[0].alive = false;
    state.enemies[0].hp = 0;

    let save = SaveFile::capture(&state);
    let restored = save
        .restore(Rc::clone(&world), 1)
        .expect("restore succeeds");

    assert_eq!(restored.state.enemies.len(), 2);
    for enemy in &restored.state.enemies {
        assert!(enemy.alive, "every authored enemy must respawn alive");
        assert_eq!(
            enemy.hp, BAT_HP,
            "every authored enemy must respawn at full hp"
        );
    }
    let mut positions: Vec<Pos> = restored.state.enemies.iter().map(|e| e.pos).collect();
    positions.sort_by_key(|p| (p.x, p.y));
    assert_eq!(positions, vec![Pos { x: 9, y: 5 }, Pos { x: 15, y: 10 }]);
}

/// The other half of the same MAJOR finding: "resets boss progress to arena entry" was only ever
/// proven for an *already-defeated* boss (which stays gone). Here the boss is captured mid-fight
/// (low hp, `AiState::BossVulnerable` in phase 2, `boss_defeated == false`) and `restore` must
/// bring it back at its authored spawn, full hp, and the same phase-1 `BossStalk` state a fresh
/// `enter_room()` produces — the fight restarts from arena entry, not from where it was left.
#[test]
fn restore_resets_an_undefeated_boss_to_its_arena_entry_state() {
    let world = common::world();
    let boss_room = world
        .room_idx("room.boss_arena")
        .expect("room.boss_arena exists in the embedded world");
    let mut state = GameState::new(1, Rc::clone(&world));
    state.room = boss_room;
    state.enter_room();
    {
        let boss = state
            .enemies
            .iter_mut()
            .find(|e| e.kind == EnemyKind::Boss)
            .expect("room.boss_arena has a boss spawn");
        boss.hp = 1;
        boss.ai = AiState::BossVulnerable {
            phase: 2,
            until: 1_000_000,
        };
    }

    let save = SaveFile::capture(&state);
    assert!(
        !save.game.boss_defeated,
        "the boss was hit, not defeated, so boss_defeated must stay false"
    );

    let restored = save
        .restore(Rc::clone(&world), 1)
        .expect("restore succeeds");

    let boss = restored
        .state
        .enemies
        .iter()
        .find(|e| e.kind == EnemyKind::Boss)
        .expect("an undefeated boss must respawn");
    assert_eq!(boss.pos, Pos { x: 18, y: 8 }, "boss.at in assets/world.ron");
    assert_eq!(boss.hp, BOSS_HP);
    assert!(
        matches!(boss.ai, AiState::BossStalk { phase: 1, .. }),
        "boss ai was {:?}, expected the fresh-entry phase-1 BossStalk state",
        boss.ai
    );
}

// --- App layer (T14) ---

/// Sets `room.boss_arena` as the current room and returns the boss's `EnemyId`, mirroring the
/// `state_in_boss_arena` pattern `tests/boss.rs` already uses to reach the arena directly.
fn enter_boss_arena(app: &mut App) -> EnemyId {
    let room_idx = app
        .state
        .world
        .room_idx("room.boss_arena")
        .expect("room.boss_arena exists in the embedded world");
    app.state.room = room_idx;
    app.state.enter_room();
    app.state.hero.pos = Pos { x: 1, y: 1 };
    app.state.hero.facing = Facing::East;
    app.state.hero.has_sword = true;
    let boss = app
        .state
        .enemies
        .iter_mut()
        .find(|e| e.kind == EnemyKind::Boss)
        .expect("room.boss_arena has a boss spawn");
    boss.pos = Pos { x: 2, y: 1 };
    boss.hp = 1;
    boss.ai = AiState::BossVulnerable {
        phase: 1,
        until: 1_000_000,
    };
    boss.id
}

#[test]
fn each_autosave_trigger_writes_exactly_once() {
    // Room transition.
    {
        let (io, handle) = common::memory_io();
        let mut app = App::new(&common::cfg(1), common::world(), io);
        app.apply(&[Action::Confirm]);
        app.state.hero.pos = Pos { x: 12, y: 1 };
        common::face(&mut app, Facing::North); // crosses room.lighthouse's north door
        assert_eq!(
            handle.store_count(),
            1,
            "room transition must autosave once"
        );
    }

    // Important item (a chest reward).
    {
        let (io, handle) = common::memory_io();
        let mut app = App::new(&common::cfg(1), common::world(), io);
        app.apply(&[Action::Confirm]);
        let crossroads = app
            .state
            .world
            .room_idx("room.crossroads")
            .expect("room.crossroads exists");
        app.state.room = crossroads;
        app.state.enter_room();
        app.state.hero.pos = Pos { x: 1, y: 1 };
        let chest_pos = app.state.world.room(crossroads).chests[0].at;
        common::walk_to(
            &mut app,
            Pos {
                x: chest_pos.x,
                y: chest_pos.y + 1,
            },
        );
        common::face(&mut app, Facing::North);
        common::step(&mut app, &[Action::Interact]);
        assert_eq!(handle.store_count(), 1, "an item pickup must autosave once");
    }

    // Puzzle solved (room.old_mill's StepPlates, per tests/overworld.rs).
    {
        let (io, handle) = common::memory_io();
        let mut app = App::new(&common::cfg(1), common::world(), io);
        app.apply(&[Action::Confirm]);
        let old_mill = app
            .state
            .world
            .room_idx("room.old_mill")
            .expect("room.old_mill exists");
        app.state.room = old_mill;
        app.state.enter_room();
        app.state.hero.pos = Pos { x: 12, y: 9 };
        common::walk_to(&mut app, Pos { x: 10, y: 9 });
        common::walk_to(&mut app, Pos { x: 5, y: 9 });
        assert_eq!(
            handle.store_count(),
            0,
            "only the third plate solves the puzzle"
        );
        common::walk_to(&mut app, Pos { x: 15, y: 9 });
        assert_eq!(
            handle.store_count(),
            1,
            "solving the puzzle must autosave once"
        );
    }

    // Boss victory.
    {
        let (io, handle) = common::memory_io();
        let mut app = App::new(&common::cfg(1), common::world(), io);
        app.apply(&[Action::Confirm]);
        enter_boss_arena(&mut app);
        common::step(&mut app, &[Action::Attack]);
        assert_eq!(
            handle.store_count(),
            1,
            "defeating the boss must autosave once"
        );
    }
}

#[test]
fn a_death_state_never_overwrites_the_last_usable_save() {
    let (io, handle) = common::memory_io();
    let mut app = App::new(&common::cfg(1), common::world(), io);
    app.apply(&[Action::Confirm]);
    app.state.hero.pos = Pos { x: 12, y: 1 };
    common::face(&mut app, Facing::North); // autosave #1
    assert_eq!(handle.store_count(), 1);

    app.state.hero.health_halves = 0;
    common::idle(&mut app); // HeroDied
    assert_eq!(app.mode, Mode::GameOver);
    assert_eq!(
        handle.store_count(),
        1,
        "a death state must not be written over the last usable save"
    );
}

#[test]
fn manual_save_is_refused_during_combat_and_accepted_outside_it() {
    let (io, handle) = common::memory_io();
    let mut app = App::new(&common::cfg(1), common::world(), io);
    app.apply(&[Action::Confirm]);

    app.state.enemies.push(Enemy {
        id: EnemyId(0),
        kind: EnemyKind::Slime,
        pos: Pos { x: 0, y: 0 },
        facing: Facing::South,
        hp: 2,
        ai: AiState::SlimeIdle { until: 1_000_000 },
        patrol: Vec::new(),
        move_ready_at: 1_000_000,
        alive: true,
    });
    app.apply(&[Action::Cancel]); // -> Paused
    app.apply(&[Action::Confirm]); // manual save, refused
    assert_eq!(handle.store_count(), 0);
    assert!(
        app.message.contains("combat"),
        "message was {:?}",
        app.message
    );
    app.apply(&[Action::Cancel]); // -> Playing

    app.state.enemies.clear();
    app.apply(&[Action::Cancel]); // -> Paused
    app.apply(&[Action::Confirm]); // manual save, accepted
    assert_eq!(handle.store_count(), 1);
}

#[test]
fn loading_restores_progress_respawns_enemies_grants_a_safe_window_and_restarts_the_boss_at_arena_entry(
) {
    let world = common::world();
    let state = GameState::new(1, Rc::clone(&world));
    let mut save = SaveFile::capture(&state);
    save.game.room = "room.boss_arena".to_string();
    save.game.boss_defeated = true;
    save.game.hero.has_sword = true;
    save.game.hero.health_halves = 4;
    save.game.hero.max_health_halves = 6;

    let io = MemorySaveIo::seeded(save);
    let mut app = App::new(&common::cfg(1), Rc::clone(&world), Box::new(io));
    assert!(matches!(app.slot, SlotState::Usable(_)));

    app.menu = MenuCursor::Continue;
    app.apply(&[Action::Confirm]);
    assert_eq!(app.mode, Mode::Playing);

    let boss_room = world.room_idx("room.boss_arena").expect("exists");
    assert_eq!(app.state.room, boss_room);
    assert!(
        app.state.enemies.iter().all(|e| e.kind != EnemyKind::Boss),
        "a defeated boss must not respawn"
    );
    assert_eq!(app.state.hero.invuln_until, LOAD_SAFE_WINDOW_TICKS);
    assert_eq!(app.state.hero.step_ready_at, 0);
    assert_eq!(app.state.hero.attack_ready_at, 0);
    assert!(!app.state.hero.died);
    assert_eq!(app.state.hero.health_halves, 4);
    assert_eq!(app.state.tick, 0);
}

#[test]
fn new_game_over_an_existing_run_requires_confirmation() {
    let (io, handle) = common::memory_io();
    {
        let mut app = App::new(&common::cfg(1), common::world(), io);
        app.apply(&[Action::Confirm]); // New Game -> Playing
        app.apply(&[Action::Cancel]); // -> Paused
        app.apply(&[Action::Confirm]); // manual save
    }
    assert_eq!(handle.store_count(), 1);

    let mut app2 = App::new(&common::cfg(1), common::world(), Box::new(handle.clone()));
    assert!(matches!(app2.slot, SlotState::Usable(_)));
    assert_eq!(app2.menu, MenuCursor::NewGame);

    app2.apply(&[Action::Confirm]);
    assert_eq!(
        app2.mode,
        Mode::ConfirmNewGame,
        "New Game over a usable slot must ask for confirmation"
    );

    app2.apply(&[Action::Cancel]);
    assert_eq!(app2.mode, Mode::MainMenu);

    app2.apply(&[Action::Confirm]);
    app2.apply(&[Action::Confirm]); // confirm the overwrite
    assert_eq!(app2.mode, Mode::Playing);
}

/// Review round 1, minor #2: confirming New Game over a `Usable` slot used to leave `self.slot`
/// pointing at the abandoned run, so a death before the fresh run's first autosave would have
/// `GameOver` -> `Confirm` resurrect it instead of starting over. `start_new_game` now clears a
/// `Usable` slot to `Empty` up front, so the retry here must land back at the world's start room
/// with the "no save to retry from" message, not at the old save's room.
#[test]
fn retry_before_first_autosave_after_a_deliberate_new_game_never_resurrects_the_abandoned_run() {
    let (io, handle) = common::memory_io();
    {
        let mut app = App::new(&common::cfg(1), common::world(), io);
        app.apply(&[Action::Confirm]); // New Game -> Playing
        app.state.hero.pos = Pos { x: 12, y: 1 };
        common::face(&mut app, Facing::North); // autosave #1, well past the start room
    }
    assert_eq!(handle.store_count(), 1);
    let abandoned_room = match handle.slot() {
        Some(save) => save.game.room,
        None => panic!("expected a usable slot from the first run's autosave"),
    };

    let mut app2 = App::new(&common::cfg(1), common::world(), Box::new(handle.clone()));
    assert!(matches!(app2.slot, SlotState::Usable(_)));

    app2.apply(&[Action::Confirm]); // New Game -> ConfirmNewGame
    assert_eq!(app2.mode, Mode::ConfirmNewGame);
    app2.apply(&[Action::Confirm]); // confirm the overwrite -> fresh run, Playing
    assert_eq!(app2.mode, Mode::Playing);
    assert!(
        matches!(app2.slot, SlotState::Empty),
        "the abandoned run's slot must be cleared before the fresh run's first autosave"
    );

    // Die before this fresh run ever autosaves.
    app2.state.hero.health_halves = 0;
    common::idle(&mut app2);
    assert_eq!(app2.mode, Mode::GameOver);

    app2.apply(&[Action::Confirm]); // retry
    assert_eq!(app2.mode, Mode::Playing);
    assert!(
        app2.message.contains("No save to retry from"),
        "message was {:?}",
        app2.message
    );
    let start_room_id = app2
        .state
        .world
        .room(app2.state.world.start_room().expect("a start room exists"))
        .id
        .clone();
    let current_room_id = app2.state.world.room(app2.state.room).id.clone();
    assert_eq!(current_room_id, start_room_id);
    assert_ne!(
        current_room_id, abandoned_room,
        "retry must not resurrect the abandoned run's room"
    );
}

#[test]
fn a_corrupt_slot_offers_backup_restore_and_never_overwrites_the_file() {
    let scratch = common::ScratchDir::new("corrupt-slot-app");
    std::fs::write(save::save_path(scratch.path()), b"not json").expect("write fixture");

    let mut app = App::new(
        &common::cfg(1),
        common::world(),
        Box::new(FileSaveIo::new(scratch.path().to_path_buf())),
    );
    assert!(matches!(app.slot, SlotState::Corrupt { .. }));

    app.menu = MenuCursor::Continue;
    app.apply(&[Action::Confirm]);
    assert_eq!(app.mode, Mode::SaveProblem);
    assert_eq!(
        app.save_problem_items(),
        vec![
            SaveProblemAction::RestoreBackup,
            SaveProblemAction::NewGame,
            SaveProblemAction::Back
        ]
    );

    app.apply(&[Action::Confirm]); // Restore backup: none exists
    assert_eq!(app.mode, Mode::SaveProblem);
    assert!(
        app.message.contains("backup"),
        "message was {:?}",
        app.message
    );

    let after = std::fs::read(save::save_path(scratch.path())).expect("read after");
    assert_eq!(
        after, b"not json",
        "a failed backup restore must not touch the file"
    );
}

#[test]
fn a_future_version_slot_is_never_overwritten_by_a_new_run() {
    let scratch = common::ScratchDir::new("future-version-app");
    let future = serde_json::json!({"format_version": 999, "game": {}});
    std::fs::write(save::save_path(scratch.path()), future.to_string()).expect("write fixture");
    let before = std::fs::read(save::save_path(scratch.path())).expect("read before");

    let mut app = App::new(
        &common::cfg(1),
        common::world(),
        Box::new(FileSaveIo::new(scratch.path().to_path_buf())),
    );
    assert!(matches!(app.slot, SlotState::FutureVersion { .. }));

    app.menu = MenuCursor::Continue;
    app.apply(&[Action::Confirm]);
    assert_eq!(app.mode, Mode::SaveProblem);
    assert_eq!(
        app.save_problem_items(),
        vec![SaveProblemAction::NewGame, SaveProblemAction::Back]
    );
    app.apply(&[Action::Confirm]); // New game, playing on without saving
    assert_eq!(app.mode, Mode::Playing);

    // Drive a real autosave trigger; the file on disk must stay exactly what it was.
    app.state.hero.pos = Pos { x: 12, y: 1 };
    common::face(&mut app, Facing::North);
    assert!(
        app.message.contains("newer version"),
        "message was {:?}",
        app.message
    );

    let after = std::fs::read(save::save_path(scratch.path())).expect("read after");
    assert_eq!(
        before, after,
        "a future-version save must never be overwritten by a new run's autosave"
    );
}

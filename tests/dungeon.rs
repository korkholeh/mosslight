//! Keys and locked doors (spec §7, phase 5), the beacon without the ember, and the finished
//! world's room/content counts (spec §2).

use std::collections::HashMap;
use std::rc::Rc;

use mosslight::content::{Door, LockKind, Room, RoomKind, Route, Spawn, StartPoint, Tile};
use mosslight::game::tuning::{ROOM_H, ROOM_W};
use mosslight::game::{
    update, Action, EnemyKind, Facing, GameEvent, GameState, Pos, RoomIdx, Tick, World,
};

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

fn state_in(room_id: &str, pos: Pos) -> GameState {
    let world = world();
    let room = world
        .room_idx(room_id)
        .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
    let mut state = GameState::new(1, world);
    state.room = room;
    state.hero.pos = pos;
    state.enter_room();
    state
}

fn advance(state: &mut GameState, tick: &mut Tick, actions: &[Action]) -> Vec<GameEvent> {
    *tick += 1;
    update(state, actions, *tick)
}

// --- Keys and locked doors (room.flooded_hall <-SmallKey-> room.plate_chamber, one-way authored;
// see DECISIONS.md) ---

#[test]
fn unlocking_a_door_consumes_exactly_one_key() {
    let mut state = state_in("room.flooded_hall", Pos { x: 22, y: 8 });
    state.hero.keys = 1;
    let mut tick = 0u64;

    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);

    assert_eq!(state.hero.keys, 0, "the one key must be spent");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DoorUnlocked { .. })),
        "{events:?}"
    );
    assert_eq!(
        state.room,
        world().room_idx("room.plate_chamber").unwrap(),
        "the door must open once affordable"
    );
}

#[test]
fn a_locked_door_with_no_key_blocks_and_never_makes_the_count_negative() {
    let mut state = state_in("room.flooded_hall", Pos { x: 22, y: 8 });
    assert_eq!(state.hero.keys, 0);
    let mut tick = 0u64;

    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);

    assert_eq!(
        state.room,
        world().room_idx("room.flooded_hall").unwrap(),
        "must not cross without a key"
    );
    assert_eq!(state.hero.keys, 0, "keys must never go negative");
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::DoorBlocked {
                lock: LockKind::SmallKey
            }
        )),
        "{events:?}"
    );
}

#[test]
fn a_second_pass_through_the_same_door_costs_nothing() {
    let mut state = state_in("room.flooded_hall", Pos { x: 22, y: 8 });
    state.hero.keys = 1;
    let mut tick = 0u64;

    advance(&mut state, &mut tick, &[Action::MoveEast]); // unlock + cross into plate_chamber
    assert_eq!(state.hero.keys, 0);
    assert_eq!(state.room, world().room_idx("room.plate_chamber").unwrap());

    // Walk back through plate_chamber's (unlocked, reciprocal) west door.
    let mut budget = 0;
    loop {
        budget += 1;
        assert!(budget < 200, "never returned to flooded_hall");
        let events = advance(&mut state, &mut tick, &[Action::MoveWest]);
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { .. }))
        {
            break;
        }
    }
    assert_eq!(state.room, world().room_idx("room.flooded_hall").unwrap());

    // Cross east again: the door is already in `unlocked_doors`, so no key is needed and no
    // second `DoorUnlocked` fires.
    state.hero.pos = Pos { x: 22, y: 8 };
    let mut saw_unlocked = false;
    let mut budget = 0;
    loop {
        budget += 1;
        assert!(budget < 200, "never crossed back into plate_chamber");
        let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
        saw_unlocked |= events
            .iter()
            .any(|e| matches!(e, GameEvent::DoorUnlocked { .. }));
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { .. }))
        {
            break;
        }
    }
    assert_eq!(state.hero.keys, 0, "no key should be spent the second time");
    assert!(!saw_unlocked, "the door was already unlocked");
    assert_eq!(state.room, world().room_idx("room.plate_chamber").unwrap());
}

/// A synthetic two-room world whose `SmallKey` lock is authored on **both** sides — the real
/// dungeon authors every lock one-way (see DECISIONS.md), so nothing in `assets/world.ron`
/// exercises `resolve_door`'s reciprocal-unlock insertion. Built directly from `schema` types
/// (as `content::validate`'s own `more_than_64_smallkey_doors...` test does) since this is a pure
/// simulation-mechanics fixture, not content this phase needs `content::validate` to accept.
fn two_room_locked_world() -> Rc<World> {
    fn bordered_floor() -> [[Tile; ROOM_W]; ROOM_H] {
        let mut tiles = [[Tile::Floor; ROOM_W]; ROOM_H];
        tiles[0] = [Tile::Wall; ROOM_W];
        tiles[ROOM_H - 1] = [Tile::Wall; ROOM_W];
        for row in tiles.iter_mut() {
            row[0] = Tile::Wall;
            row[ROOM_W - 1] = Tile::Wall;
        }
        tiles
    }

    let mut tiles_a = bordered_floor();
    tiles_a[8][ROOM_W - 1] = Tile::Door;
    let mut tiles_b = bordered_floor();
    tiles_b[8][0] = Tile::Door;

    let room_a = Room {
        id: "room.a".to_string(),
        name: "A".to_string(),
        kind: RoomKind::Overworld,
        map_index: Some((0, 0)),
        rows: Vec::new(),
        tiles: tiles_a,
        doors: vec![Door {
            id: "door.a.east".to_string(),
            at: Pos {
                x: (ROOM_W - 1) as u8,
                y: 8,
            },
            to_room: "room.b".to_string(),
            to_spawn: "spawn.b.west".to_string(),
            lock: Some(LockKind::SmallKey),
            two_way: true,
        }],
        spawns: vec![
            Spawn {
                id: "spawn.a.start".to_string(),
                at: Pos { x: 2, y: 8 },
            },
            Spawn {
                id: "spawn.a.east".to_string(),
                at: Pos {
                    x: (ROOM_W - 2) as u8,
                    y: 8,
                },
            },
        ],
        chests: Vec::new(),
        npcs: Vec::new(),
        enemies: Vec::new(),
        puzzles: Vec::new(),
        torches: Vec::new(),
        plates: Vec::new(),
        blocks: Vec::new(),
        beacons: Vec::new(),
        hint: None,
    };
    let room_b = Room {
        id: "room.b".to_string(),
        name: "B".to_string(),
        kind: RoomKind::Overworld,
        map_index: Some((1, 0)),
        rows: Vec::new(),
        tiles: tiles_b,
        doors: vec![Door {
            id: "door.b.west".to_string(),
            at: Pos { x: 0, y: 8 },
            to_room: "room.a".to_string(),
            to_spawn: "spawn.a.east".to_string(),
            lock: Some(LockKind::SmallKey),
            two_way: true,
        }],
        spawns: vec![Spawn {
            id: "spawn.b.west".to_string(),
            at: Pos { x: 1, y: 8 },
        }],
        chests: Vec::new(),
        npcs: Vec::new(),
        enemies: Vec::new(),
        puzzles: Vec::new(),
        torches: Vec::new(),
        plates: Vec::new(),
        blocks: Vec::new(),
        beacons: Vec::new(),
        hint: None,
    };

    Rc::new(World {
        version: 1,
        start: StartPoint {
            room: "room.a".to_string(),
            spawn: "spawn.a.start".to_string(),
        },
        route: Route {
            ember_required: false,
            home: "room.a".to_string(),
            goal: "room.b".to_string(),
        },
        rooms: vec![room_a, room_b],
        room_index: HashMap::from([
            ("room.a".to_string(), RoomIdx(0)),
            ("room.b".to_string(), RoomIdx(1)),
        ]),
    })
}

#[test]
fn the_reciprocal_side_is_free_after_unlocking() {
    let world = two_room_locked_world();
    let mut state = GameState::new(1, Rc::clone(&world));
    state.hero.pos = Pos {
        x: (ROOM_W - 2) as u8,
        y: 8,
    };
    state.hero.keys = 1;
    let mut tick = 0u64;

    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
    assert_eq!(state.hero.keys, 0);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::DoorUnlocked { .. })));
    assert_eq!(state.room, RoomIdx(1));

    // door.b.west carries its own SmallKey lock and was never separately unlocked by the player —
    // only the reciprocal-insertion at door.a.east's unlock time can have freed it.
    let mut saw_blocked = false;
    let mut budget = 0;
    loop {
        budget += 1;
        assert!(budget < 200, "never returned to room.a");
        let events = advance(&mut state, &mut tick, &[Action::MoveWest]);
        saw_blocked |= events
            .iter()
            .any(|e| matches!(e, GameEvent::DoorBlocked { .. }));
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { .. }))
        {
            break;
        }
    }
    assert_eq!(
        state.room,
        RoomIdx(0),
        "the reciprocal side must be free after the forward side was unlocked"
    );
    assert_eq!(state.hero.keys, 0, "no second key should be needed");
    assert!(!saw_blocked);
}

// --- The beacon (room.lighthouse) ---

#[test]
fn the_beacon_without_the_ember_only_reports_a_cold_brazier() {
    let mut state = state_in("room.lighthouse", Pos { x: 20, y: 6 });
    state.hero.facing = Facing::North; // beacon.lighthouse sits at (20, 5)
    let mut tick = 0u64;

    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m.contains("cold"))),
        "{events:?}"
    );
    assert!(!events.iter().any(|e| matches!(e, GameEvent::GameWon)));

    state.hero.has_ember = true;
    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::GameWon)),
        "{events:?}"
    );
    assert!(state.progress.flags.contains("flag.lighthouse_relit"));
}

// --- The finished world's shape (spec §2) ---

#[test]
fn the_world_has_nine_overworld_and_six_dungeon_rooms() {
    let world = world();
    let overworld = world
        .rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Overworld)
        .count();
    let dungeon = world
        .rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Dungeon)
        .count();
    assert_eq!(overworld, 9);
    assert_eq!(dungeon, 6);
}

#[test]
fn the_world_matches_the_section_2_content_table() {
    let world = world();

    let npc_count: usize = world.rooms.iter().map(|r| r.npcs.len()).sum();
    assert_eq!(npc_count, 3, "spec §2 authors exactly 3 NPCs");

    let mut has_slime = false;
    let mut has_bat = false;
    let mut has_guardian = false;
    let mut boss_count = 0;
    for room in &world.rooms {
        for enemy in &room.enemies {
            match enemy.kind {
                EnemyKind::Slime => has_slime = true,
                EnemyKind::Bat => has_bat = true,
                EnemyKind::Guardian => has_guardian = true,
                EnemyKind::Boss => boss_count += 1,
            }
        }
    }
    assert!(
        has_slime && has_bat && has_guardian,
        "all 3 regular enemy kinds must be spawned"
    );
    assert_eq!(boss_count, 1, "exactly one boss");

    let secret_count: usize = world
        .rooms
        .iter()
        .map(|r| r.chests.iter().filter(|c| c.secret).count())
        .sum();
    assert!(secret_count >= 3, "spec §2 asks for at least 3 secrets");
}

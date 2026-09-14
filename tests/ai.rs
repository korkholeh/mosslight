//! Enemy AI: liveness (never gets stuck), containment, the guardian's telegraph/vulnerability
//! window, contested-tile resolution order, and the pacing-safe rooms (spec §6, §13, RISKS #14).

use std::collections::HashMap;
use std::rc::Rc;

use mosslight::game::tuning::{
    GUARDIAN_HP, MAX_STALL_TICKS, ROOM_H, ROOM_W, SLIME_STEP_TICKS, TELEGRAPH_MIN_TICKS,
};
use mosslight::game::{
    ai, combat, AiState, Enemy, EnemyId, EnemyKind, Facing, GameEvent, GameState, Pos, Room,
    RoomIdx, RoomKind, Route, Spawn, StartPoint, Tick, Tile, World,
};

/// A single hand-built room with a couple of interior walls, so `path_step`/`local_step` see real
/// obstacles rather than an empty box. No doors, no authored enemies — every test here places its
/// own.
fn maze_world() -> Rc<World> {
    let mut tiles = [[Tile::Floor; ROOM_W]; ROOM_H];
    tiles[0] = [Tile::Wall; ROOM_W];
    tiles[ROOM_H - 1] = [Tile::Wall; ROOM_W];
    for row in tiles.iter_mut() {
        row[0] = Tile::Wall;
        row[ROOM_W - 1] = Tile::Wall;
    }
    for row in tiles.iter_mut().take(10).skip(2) {
        row[14] = Tile::Wall;
    }
    tiles[10][14] = Tile::Floor; // the one gap through the interior wall

    let room = Room {
        id: "room.maze".to_string(),
        name: "Maze".to_string(),
        kind: RoomKind::Overworld,
        map_index: Some((0, 0)),
        rows: Vec::new(),
        tiles,
        doors: Vec::new(),
        spawns: vec![Spawn {
            id: "spawn.maze.start".to_string(),
            at: Pos { x: 1, y: 1 },
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
            room: "room.maze".to_string(),
            spawn: "spawn.maze.start".to_string(),
        },
        route: Route {
            ember_required: false,
            home: "room.maze".to_string(),
            goal: "room.maze".to_string(),
        },
        rooms: vec![room],
        room_index: HashMap::from([("room.maze".to_string(), RoomIdx(0))]),
    })
}

fn state_with_enemy(kind: EnemyKind) -> GameState {
    let mut state = GameState::new(1, maze_world());
    state.hero.pos = Pos { x: 1, y: 1 };

    let ai = match kind {
        EnemyKind::Slime => AiState::SlimeIdle { until: 0 },
        EnemyKind::Bat => AiState::BatRest { until: 0 },
        EnemyKind::Guardian => AiState::GuardianPatrol {
            waypoint: 0,
            until: 0,
        },
        EnemyKind::Boss => AiState::BossStalk { phase: 1, until: 0 },
    };
    let patrol = if kind == EnemyKind::Guardian {
        vec![
            Pos { x: 5, y: 5 },
            Pos { x: 18, y: 5 },
            Pos { x: 18, y: 12 },
            Pos { x: 5, y: 12 },
        ]
    } else {
        Vec::new()
    };

    state.enemies = vec![Enemy {
        id: EnemyId(0),
        kind,
        pos: Pos { x: 10, y: 8 },
        facing: Facing::South,
        hp: 10,
        ai,
        patrol,
        move_ready_at: 0,
        alive: true,
    }];
    state
}

#[test]
fn each_kind_keeps_changing_state_for_1000_ticks() {
    // Compares discriminants, not full `AiState` equality (round-1 review, major): a re-timered
    // re-entry into the *same* variant (e.g. `SlimeIdle { until: 60 }` -> `SlimeIdle { until:
    // 120 }`) must not count as a state change, or this test cannot detect an enemy that never
    // actually leaves its state — which the slime used to do whenever the hero stayed outside
    // `SLIME_AGGRO_RADIUS`, as it does here (hero at (1,1), slime at (10,8)).
    for kind in [
        EnemyKind::Slime,
        EnemyKind::Bat,
        EnemyKind::Guardian,
        EnemyKind::Boss,
    ] {
        let mut state = state_with_enemy(kind);
        let mut last_change: Tick = 0;
        for tick in 1..=1000u64 {
            let before = std::mem::discriminant(&state.enemies[0].ai);
            ai::step(&mut state, tick);
            if std::mem::discriminant(&state.enemies[0].ai) != before {
                last_change = tick;
            }
            assert!(
                tick - last_change <= MAX_STALL_TICKS,
                "{kind:?} stalled: no AiState change between tick {last_change} and {tick}"
            );
        }
    }
}

#[test]
fn enemies_stay_inside_the_room_bounds() {
    for kind in [
        EnemyKind::Slime,
        EnemyKind::Bat,
        EnemyKind::Guardian,
        EnemyKind::Boss,
    ] {
        let mut state = state_with_enemy(kind);
        for tick in 1..=1000u64 {
            ai::step(&mut state, tick);
            let pos = state.enemies[0].pos;
            assert!(
                (pos.x as usize) < ROOM_W && (pos.y as usize) < ROOM_H,
                "{kind:?} left the room at {pos:?}"
            );
            assert!(
                state
                    .room()
                    .tile_at(pos)
                    .is_some_and(|t| t.is_walkable() && !t.is_hazard()),
                "{kind:?} stands on a non-walkable tile at {pos:?}"
            );
        }
    }
}

#[test]
fn guardian_telegraphs_at_least_the_minimum_warning() {
    let mut state = state_with_enemy(EnemyKind::Guardian);
    let mut telegraph_start: Option<Tick> = None;

    for tick in 1..=1000u64 {
        let was_telegraph = matches!(state.enemies[0].ai, AiState::GuardianTelegraph { .. });
        ai::step(&mut state, tick);
        let is_telegraph = matches!(state.enemies[0].ai, AiState::GuardianTelegraph { .. });
        let is_dash = matches!(state.enemies[0].ai, AiState::GuardianDash { .. });

        if !was_telegraph && is_telegraph {
            telegraph_start = Some(tick);
        }
        if is_dash {
            let start = telegraph_start.expect("a dash is always preceded by a telegraph");
            assert!(
                tick - start >= TELEGRAPH_MIN_TICKS,
                "dash started only {} ticks after the telegraph began",
                tick - start
            );
            return;
        }
    }
    panic!("guardian never reached GuardianDash within 1000 ticks");
}

#[test]
fn guardian_takes_damage_only_while_recovering() {
    let ai_states = [
        AiState::GuardianPatrol {
            waypoint: 0,
            until: 100,
        },
        AiState::GuardianTelegraph {
            until: 100,
            facing: Facing::South,
        },
        AiState::GuardianDash {
            steps_left: 2,
            facing: Facing::South,
        },
        AiState::GuardianRecover { until: 100 },
    ];

    for ai_state in ai_states {
        let mut state = state_with_enemy(EnemyKind::Guardian);
        state.hero.pos = Pos { x: 1, y: 1 };
        state.hero.facing = Facing::East;
        state.enemies = vec![Enemy {
            id: EnemyId(0),
            kind: EnemyKind::Guardian,
            pos: Pos { x: 2, y: 1 },
            facing: Facing::South,
            hp: GUARDIAN_HP,
            ai: ai_state,
            patrol: Vec::new(),
            move_ready_at: 100,
            alive: true,
        }];

        combat::start_swing(&mut state.hero, 0);
        let events = combat::resolve_swing(&mut state, 0);

        if matches!(ai_state, AiState::GuardianRecover { .. }) {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, GameEvent::EnemyDamaged { .. })),
                "{ai_state:?} should take damage: {events:?}"
            );
        } else {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, GameEvent::AttackDeflected { .. })),
                "{ai_state:?} should deflect: {events:?}"
            );
        }
    }
}

#[test]
fn slime_wander_commits_to_one_direction_for_the_whole_phase() {
    // Round-2 review, minor: `SlimeWander` used to redraw a random direction on every step,
    // making its movement byte-identical to `SlimeIdle`'s and leaving the discriminant rename the
    // only real difference between the two states. An unblocked wander must now keep the one
    // direction it drew for the whole phase.
    let mut state = state_with_enemy(EnemyKind::Slime);
    state.hero.pos = Pos { x: 1, y: 1 };
    state.enemies[0].pos = Pos { x: 10, y: 1 };
    state.enemies[0].ai = AiState::SlimeWander {
        until: 10_000,
        facing: Facing::South,
    };
    state.enemies[0].move_ready_at = 0;

    for tick in 1..=SLIME_STEP_TICKS * 4 {
        ai::step(&mut state, tick);
        let AiState::SlimeWander { facing, .. } = state.enemies[0].ai else {
            panic!("must stay in SlimeWander for the whole test window");
        };
        assert_eq!(
            facing,
            Facing::South,
            "an unblocked wander must keep its drawn direction for the whole phase"
        );
    }
    assert_eq!(
        state.enemies[0].pos,
        Pos { x: 10, y: 5 },
        "wandering south every SLIME_STEP_TICKS for 4 steps must move 4 tiles south"
    );
}

#[test]
fn two_enemies_contesting_one_tile_resolve_by_vec_order() {
    let mut state = state_with_enemy(EnemyKind::Guardian);
    state.enemies = vec![
        Enemy {
            id: EnemyId(0),
            kind: EnemyKind::Guardian,
            pos: Pos { x: 9, y: 5 },
            facing: Facing::East,
            hp: GUARDIAN_HP,
            ai: AiState::GuardianDash {
                steps_left: 5,
                facing: Facing::East,
            },
            patrol: Vec::new(),
            move_ready_at: 0,
            alive: true,
        },
        Enemy {
            id: EnemyId(1),
            kind: EnemyKind::Guardian,
            pos: Pos { x: 11, y: 5 },
            facing: Facing::West,
            hp: GUARDIAN_HP,
            ai: AiState::GuardianDash {
                steps_left: 5,
                facing: Facing::West,
            },
            patrol: Vec::new(),
            move_ready_at: 0,
            alive: true,
        },
    ];

    ai::step(&mut state, 1);

    assert_eq!(
        state.enemies[0].pos,
        Pos { x: 10, y: 5 },
        "the earlier enemy in Vec order claims the contested tile"
    );
    assert_eq!(
        state.enemies[1].pos,
        Pos { x: 11, y: 5 },
        "the later enemy is blocked by the tile the first one already took"
    );
}

#[test]
fn path_step_is_total_for_out_of_bounds_positions() {
    // `Pos` is `u8`, far wider than `ROOM_W`/`ROOM_H`, so nothing at the type level stops a
    // caller from handing `path_step` a position outside the fixed grid. It used to index
    // straight into a `[ROOM_H][ROOM_W]` array with an unchecked `from`, which panics rather than
    // returning `None` in a module documented as total (round-1 review, minor).
    let world = maze_world();
    let room = world.room(RoomIdx(0));
    let occupied = std::collections::HashSet::new();
    let out_of_bounds = Pos { x: 250, y: 250 };

    assert_eq!(
        ai::path_step(
            room,
            &occupied,
            &occupied,
            out_of_bounds,
            Pos { x: 1, y: 1 }
        ),
        None
    );
    assert_eq!(
        ai::path_step(
            room,
            &occupied,
            &occupied,
            Pos { x: 1, y: 1 },
            out_of_bounds
        ),
        None
    );
}

#[test]
fn start_room_and_crossroads_have_zero_enemy_spawns() {
    let world = mosslight::content::load().expect("embedded world validates");
    for id in ["room.lighthouse", "room.crossroads"] {
        let idx = world
            .room_idx(id)
            .unwrap_or_else(|| panic!("{id} must exist"));
        assert!(
            world.room(idx).enemies.is_empty(),
            "{id} must stay on the pacing-safe first stretch (RISKS #2)"
        );
    }
}

//! Movement and collision (spec §6, §13): steps, blocking, cooldown, and determinism.

use std::rc::Rc;

use mosslight::game::tuning::HERO_STEP_TICKS;
use mosslight::game::{
    update, Action, AiState, Enemy, EnemyId, EnemyKind, Facing, GameEvent, GameState, Pos,
};

fn fresh() -> GameState {
    let world = Rc::new(mosslight::content::load().expect("embedded world validates"));
    GameState::new(1, world)
}

/// A `GameState` dropped directly into `room_id` at `pos` — for the solid-object tests below,
/// which need specific authored objects (a chest, an NPC, a torch, a plate) rather than
/// `room.lighthouse`'s empty floor.
fn state_in(room_id: &str, pos: Pos) -> GameState {
    let world = Rc::new(mosslight::content::load().expect("embedded world validates"));
    let room = world
        .room_idx(room_id)
        .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
    let mut state = GameState::new(1, world);
    state.room = room;
    state.hero.pos = pos;
    state.enter_room();
    state
}

#[test]
fn no_actions_is_a_noop_except_tick() {
    let mut state = fresh();
    let before = state.clone();
    let events = update(&mut state, &[], 5);
    assert!(events.is_empty());
    assert_eq!(state.hero, before.hero);
    assert_eq!(state.tick, 5);
}

#[test]
fn steps_in_each_direction() {
    for (action, facing) in [
        (Action::MoveNorth, Facing::North),
        (Action::MoveEast, Facing::East),
        (Action::MoveSouth, Facing::South),
        (Action::MoveWest, Facing::West),
    ] {
        let mut state = fresh();
        let from = state.hero.pos;
        let events = update(&mut state, &[action], 0);
        assert_eq!(state.hero.facing, facing);
        assert_ne!(state.hero.pos, from);
        assert!(matches!(events[0], GameEvent::HeroMoved { .. }));
    }
}

#[test]
fn blocked_by_wall_keeps_position_sets_facing() {
    let mut state = fresh();
    state.hero.pos = Pos { x: 1, y: 1 };
    let events = update(&mut state, &[Action::MoveNorth], 0);
    assert_eq!(state.hero.pos, Pos { x: 1, y: 1 });
    assert_eq!(state.hero.facing, Facing::North);
    assert_eq!(
        events[0],
        GameEvent::MoveBlocked {
            at: Pos { x: 1, y: 1 },
            facing: Facing::North
        }
    );
}

#[test]
fn blocked_by_water() {
    // room.lighthouse has a 2x2 water patch at (3..=4, 3..=4).
    let mut state = fresh();
    state.hero.pos = Pos { x: 2, y: 3 };
    let events = update(&mut state, &[Action::MoveEast], 0);
    assert_eq!(state.hero.pos, Pos { x: 2, y: 3 });
    assert!(matches!(events[0], GameEvent::MoveBlocked { .. }));
}

#[test]
fn blocked_by_bush() {
    // room.lighthouse has a 2x2 bush patch at (18..=19, 11..=12).
    let mut state = fresh();
    state.hero.pos = Pos { x: 18, y: 10 };
    let events = update(&mut state, &[Action::MoveSouth], 0);
    assert_eq!(state.hero.pos, Pos { x: 18, y: 10 });
    assert!(matches!(events[0], GameEvent::MoveBlocked { .. }));
}

#[test]
fn blocked_out_of_bounds() {
    let mut state = fresh();
    state.hero.pos = Pos { x: 0, y: 0 };
    let events = update(&mut state, &[Action::MoveNorth], 0);
    assert_eq!(state.hero.pos, Pos { x: 0, y: 0 });
    assert!(matches!(events[0], GameEvent::MoveBlocked { .. }));
}

#[test]
fn one_step_per_cooldown() {
    let mut state = fresh();
    let start = state.hero.pos;
    update(&mut state, &[Action::MoveSouth], 0);
    let after_first = state.hero.pos;
    assert_ne!(after_first, start);
    // Still within cooldown: facing updates, but no move happens.
    update(&mut state, &[Action::MoveSouth], 1);
    assert_eq!(state.hero.pos, after_first);
    // Cooldown elapsed.
    update(&mut state, &[Action::MoveSouth], HERO_STEP_TICKS);
    assert_ne!(state.hero.pos, after_first);
}

#[test]
fn hero_cannot_step_onto_a_chest_but_can_step_onto_a_plate() {
    // chest.forest_sword sits at (5, 5) in room.crossroads: solid, blocks the step.
    let mut state = state_in("room.crossroads", Pos { x: 5, y: 6 });
    let events = update(&mut state, &[Action::MoveNorth], 0);
    assert_eq!(state.hero.pos, Pos { x: 5, y: 6 });
    assert!(matches!(events[0], GameEvent::MoveBlocked { .. }));

    // plate.mill1 sits at (5, 9) in room.old_mill: not solid, steps onto it normally.
    let mut state = state_in("room.old_mill", Pos { x: 5, y: 10 });
    update(&mut state, &[Action::MoveNorth], 0);
    assert_eq!(state.hero.pos, Pos { x: 5, y: 9 });
}

#[test]
fn hero_cannot_step_onto_an_npc_or_a_torch() {
    // npc.keeper sits at (5, 5) in room.lighthouse.
    let mut state = state_in("room.lighthouse", Pos { x: 5, y: 6 });
    update(&mut state, &[Action::MoveNorth], 0);
    assert_eq!(state.hero.pos, Pos { x: 5, y: 6 });

    // torch.shore sits at (18, 1) in room.south_shore.
    let mut state = state_in("room.south_shore", Pos { x: 17, y: 1 });
    update(&mut state, &[Action::MoveEast], 0);
    assert_eq!(state.hero.pos, Pos { x: 17, y: 1 });
}

#[test]
fn enemy_cannot_step_onto_a_solid_object_tile() {
    // chest.forest_sword sits at (5, 5) in room.crossroads; a slime chasing a hero on the far
    // side of it would cross that exact tile on the shortest path if it were walkable.
    let mut state = state_in("room.crossroads", Pos { x: 7, y: 5 });
    state.enemies = vec![Enemy {
        id: EnemyId(0),
        kind: EnemyKind::Slime,
        pos: Pos { x: 3, y: 5 },
        facing: Facing::East,
        hp: 2,
        ai: AiState::SlimeChase { until: 10_000 },
        patrol: Vec::new(),
        move_ready_at: 0,
        alive: true,
    }];

    for tick in 1..200 {
        update(&mut state, &[], tick);
        assert_ne!(
            state.enemies[0].pos,
            Pos { x: 5, y: 5 },
            "a chasing enemy must never stand on the chest's tile (tick {tick})"
        );
    }
}

#[test]
fn knockback_stops_before_a_solid_object() {
    // chest.forest_sword sits at (5, 5); the hero stands one tile south of it with an enemy
    // pressed against their own south side, so contact damage knocks the hero north, straight at
    // the chest — one knockback tile (spec §6) is exactly far enough to reach it.
    let mut state = state_in("room.crossroads", Pos { x: 5, y: 6 });
    state.enemies = vec![Enemy {
        id: EnemyId(0),
        kind: EnemyKind::Slime,
        pos: Pos { x: 5, y: 7 },
        facing: Facing::North,
        hp: 2,
        ai: AiState::SlimeIdle { until: 10_000 },
        patrol: Vec::new(),
        move_ready_at: 10_000,
        alive: true,
    }];

    let events = update(&mut state, &[], 1);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::HeroDamaged { .. })));
    assert_eq!(
        state.hero.pos,
        Pos { x: 5, y: 6 },
        "knockback must stop before the chest's tile, not land on or past it"
    );
}

#[test]
fn determinism_same_seed_same_actions() {
    let mut a = fresh();
    let mut b = fresh();
    let actions = [Action::MoveSouth, Action::MoveEast, Action::MoveEast];
    for (i, &act) in actions.iter().enumerate() {
        update(&mut a, &[act], i as u64 * HERO_STEP_TICKS);
        update(&mut b, &[act], i as u64 * HERO_STEP_TICKS);
    }
    assert_eq!(a, b);
}

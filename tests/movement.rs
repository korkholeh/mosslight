//! Movement and collision (spec §6, §13): steps, blocking, cooldown, and determinism.

use std::rc::Rc;

use mosslight::game::tuning::HERO_STEP_TICKS;
use mosslight::game::{update, Action, Facing, GameEvent, GameState, Pos};

fn fresh() -> GameState {
    let world = Rc::new(mosslight::content::load().expect("embedded world validates"));
    GameState::new(1, world)
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

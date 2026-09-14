//! The overworld's authored content (spec §7, §13): pacing (zero enemies before the sword),
//! chests and rewards, the lantern/torch/door-lock mechanics, the mill's step-plate puzzle, and
//! the three off-route secrets. See PLAN.md's acceptance table for the criterion each test below
//! proves.

mod common;

use std::rc::Rc;

use mosslight::content;
use mosslight::game::{
    update, Action, Facing, GameEvent, GameState, LockKind, Pos, Reward, RoomIdx, Tick,
};

fn world() -> Rc<mosslight::game::World> {
    Rc::new(content::load().expect("embedded world validates"))
}

/// A `GameState` dropped directly into `room_id` at `pos`, bypassing the normal walk from the
/// start room — these tests are about a single room's mechanics, not full traversal (that is
/// `main_route_milestones_happen_in_order`, below). `spawn_enemies` rebuilds the room's enemies
/// and resets its transient plate state, exactly as a real door transition would.
fn state_in(room_id: &str, pos: mosslight::game::Pos) -> GameState {
    let world = world();
    let room = world
        .room_idx(room_id)
        .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
    let mut state = GameState::new(1, world);
    state.room = room;
    state.hero.pos = pos;
    state.spawn_enemies();
    state
}

fn advance(state: &mut GameState, tick: &mut Tick, actions: &[Action]) -> Vec<GameEvent> {
    *tick += 1;
    update(state, actions, *tick)
}

/// Moves `tiles` steps in `action`'s direction, one real step at a time (spec §6's cooldown), for
/// a straight-line path already known to be clear. Panics rather than looping forever if a step
/// never lands, so a wrong assumption about the room's layout fails loudly.
fn move_steps(state: &mut GameState, tick: &mut Tick, action: Action, tiles: u32) {
    for _ in 0..tiles {
        let before = state.hero.pos;
        let mut attempts = 0;
        loop {
            attempts += 1;
            assert!(
                attempts < 1000,
                "move_steps: {action:?} never advanced from {before:?}"
            );
            advance(state, tick, &[action]);
            if state.hero.pos != before {
                break;
            }
        }
    }
}

#[test]
fn start_room_and_crossroads_have_zero_enemies_and_no_enemy_is_reachable_before_the_sword() {
    // Structural, not a spot check: BFS the room-adjacency graph from the start room, treating the
    // room holding the non-secret Sword chest as a frontier that is entered but not expanded past
    // (the player cannot go further without the sword the ramp is gating on). Every room in that
    // reached set must carry zero enemy spawns — re-adding a door out of the cul-de-sac (e.g.
    // `door.lighthouse.east`, removed in DECISIONS.md `[plan/04]` for exactly this reason) would
    // pull a slime room into the set and fail this test.
    let world = world();
    let start = world.start_room().expect("start room resolves");
    let sword_room = world
        .rooms
        .iter()
        .position(|r| {
            r.chests
                .iter()
                .any(|c| !c.secret && c.contains == Reward::Sword)
        })
        .map(|i| RoomIdx(i as u16))
        .expect("a non-secret Sword chest is authored somewhere");

    let mut reached: std::collections::BTreeSet<RoomIdx> = std::collections::BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    reached.insert(start);
    queue.push_back(start);
    while let Some(room_idx) = queue.pop_front() {
        if room_idx == sword_room {
            continue; // entered, but not expanded past — nothing beyond it is "before the sword"
        }
        for door in &world.room(room_idx).doors {
            if let Some(next) = world.room_idx(&door.to_room) {
                if reached.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }

    for room_idx in &reached {
        let room = world.room(*room_idx);
        assert!(
            room.enemies.is_empty(),
            "{} is reachable before the sword and must have zero enemy spawns",
            room.id
        );
    }
}

#[test]
fn chest_opens_once_and_reopening_yields_nothing() {
    let mut state = state_in("room.crossroads", Pos { x: 5, y: 6 });
    let mut tick = 0u64;

    // Face the chest at (5, 5) by walking into it — blocked, but facing still updates.
    advance(&mut state, &mut tick, &[Action::MoveNorth]);
    assert_eq!(state.hero.facing, Facing::North);

    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::ChestOpened { .. })),
        "{events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::Sword
            }
        )),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "You take the forest sword.")),
        "opening a chest must report the pickup in the message row: {events:?}"
    );
    assert!(state.hero.has_sword);
    assert_eq!(state.progress.opened_chests.len(), 1);

    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "The chest is empty.")),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::ChestOpened { .. })),
        "reopening must not open it again: {events:?}"
    );
    assert_eq!(state.progress.opened_chests.len(), 1);
}

#[test]
fn interact_facing_empty_floor_is_a_no_op() {
    let mut state = state_in("room.crossroads", Pos { x: 10, y: 10 });
    let mut tick = 0u64;
    advance(&mut state, &mut tick, &[Action::MoveNorth]);
    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn attack_without_the_sword_emits_a_message_and_starts_no_swing() {
    let mut state = state_in("room.crossroads", Pos { x: 10, y: 10 });
    let mut tick = 0u64;
    assert!(!state.hero.has_sword);
    let events = advance(&mut state, &mut tick, &[Action::Attack]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "You have no weapon.")),
        "{events:?}"
    );
    assert!(state.hero.attack.is_none());
}

#[test]
fn heart_container_grants_two_halves_and_caps_at_five_hearts() {
    // `chest.pines_heart` sits behind fallen_pines' secret passage; Interact does not itself
    // check walkability (only what the hero faces), so the cap behaviour is testable directly
    // without first lighting the torch.
    let mut state = state_in("room.fallen_pines", Pos { x: 20, y: 1 });
    state.hero.max_health_halves = 9;
    state.hero.health_halves = 9;
    let mut tick = 0u64;
    advance(&mut state, &mut tick, &[Action::MoveEast]);
    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::HeartContainer
            }
        )),
        "{events:?}"
    );
    assert_eq!(state.hero.max_health_halves, 10, "capped at 5 hearts");
    assert_eq!(state.hero.health_halves, 10);
}

#[test]
fn use_lantern_without_the_lantern_reports_it_and_does_not_light() {
    let mut state = state_in("room.south_shore", Pos { x: 17, y: 1 });
    let mut tick = 0u64;
    advance(&mut state, &mut tick, &[Action::MoveEast]); // face torch.shore at (18, 1)
    let events = advance(&mut state, &mut tick, &[Action::UseLantern]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "You have no lantern.")),
        "{events:?}"
    );
    assert!(state.progress.lit_torches.is_empty());
}

#[test]
fn use_lantern_facing_a_non_torch_tile_reports_nothing_to_light() {
    let mut state = state_in("room.south_shore", Pos { x: 17, y: 1 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;
    let events = advance(&mut state, &mut tick, &[Action::UseLantern]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "Nothing to light here.")),
        "{events:?}"
    );
}

#[test]
fn lantern_lights_only_the_faced_torch_and_opens_its_passage() {
    let mut state = state_in("room.south_shore", Pos { x: 17, y: 1 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;

    // Face away from the torch (into the north wall, blocked — facing still updates).
    advance(&mut state, &mut tick, &[Action::MoveNorth]);
    assert_eq!(state.hero.facing, Facing::North);
    let events = advance(&mut state, &mut tick, &[Action::UseLantern]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "Nothing to light here.")),
        "facing away from the torch must not light it: {events:?}"
    );
    assert!(state.progress.lit_torches.is_empty());
    assert!(!state.walkable(state.room, Pos { x: 20, y: 1 }));

    // Face the torch at (18, 1) and light it.
    advance(&mut state, &mut tick, &[Action::MoveEast]);
    assert_eq!(state.hero.facing, Facing::East);
    let events = advance(&mut state, &mut tick, &[Action::UseLantern]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::TorchLit { .. })),
        "{events:?}"
    );
    assert!(
        events.iter().any(
            |e| matches!(e, GameEvent::PassageRevealed { at, .. } if *at == Pos { x: 20, y: 1 })
        ),
        "{events:?}"
    );
    assert!(!state.progress.lit_torches.is_empty());
    assert!(
        state.walkable(state.room, Pos { x: 20, y: 1 }),
        "the hidden passage opens only after its torch is lit"
    );

    // Walk around the (still solid) torch to the newly-opened passage and claim the reward.
    move_steps(&mut state, &mut tick, Action::MoveSouth, 1); // (17,1) -> (17,2)
    move_steps(&mut state, &mut tick, Action::MoveEast, 3); // -> (20,2)
    move_steps(&mut state, &mut tick, Action::MoveNorth, 1); // -> (20,1), now walkable
    assert_eq!(state.hero.pos, Pos { x: 20, y: 1 });
    advance(&mut state, &mut tick, &[Action::MoveEast]); // face chest.shore_key at (21, 1)
    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::SmallKey
            }
        )),
        "{events:?}"
    );
    assert_eq!(state.hero.keys, 1);
}

#[test]
fn every_secret_chest_is_reachable_once_its_torch_is_lit() {
    for (room_id, chest_id) in [
        ("room.south_shore", "chest.shore_key"),
        ("room.fallen_pines", "chest.pines_heart"),
        ("room.north_ridge", "chest.ridge_lore"),
    ] {
        let mut state = state_in(room_id, Pos { x: 17, y: 1 });
        state.hero.has_lantern = true;
        let mut tick = 0u64;

        advance(&mut state, &mut tick, &[Action::MoveEast]); // face the torch at (18, 1)
        let events = advance(&mut state, &mut tick, &[Action::UseLantern]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::TorchLit { .. })),
            "{room_id}: torch did not light: {events:?}"
        );

        move_steps(&mut state, &mut tick, Action::MoveSouth, 1);
        move_steps(&mut state, &mut tick, Action::MoveEast, 3);
        move_steps(&mut state, &mut tick, Action::MoveNorth, 1);
        advance(&mut state, &mut tick, &[Action::MoveEast]); // face the chest at (21, 1)
        let events = advance(&mut state, &mut tick, &[Action::Interact]);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::ChestOpened { .. })),
            "{room_id}: {chest_id} was not reachable/openable: {events:?}"
        );
    }
}

#[test]
fn three_secrets_exist_and_are_off_the_main_route() {
    let world = content::load().expect("embedded world validates");
    let main_route = content::main_route_rooms(&world);

    let secrets: Vec<(String, String, Reward)> = world
        .rooms
        .iter()
        .flat_map(|r| {
            r.chests
                .iter()
                .filter(|c| c.secret)
                .map(|c| (r.id.clone(), c.id.clone(), c.contains.clone()))
        })
        .collect();
    assert_eq!(secrets.len(), 3, "{secrets:?}");

    for (room_id, chest_id, reward) in &secrets {
        assert!(
            !matches!(reward, Reward::Sword | Reward::Lantern | Reward::Ember),
            "{chest_id} is secret but holds a route-critical reward"
        );
        let idx = world.room_idx(room_id).unwrap();
        assert!(
            !main_route.contains(&idx),
            "{chest_id} in {room_id} must be off the main route"
        );
    }
}

#[test]
fn dungeon_entrance_needs_the_lantern() {
    let world = world();
    let east_marsh = world.room_idx("room.east_marsh").unwrap();
    let sanctuary_gate = world.room_idx("room.sanctuary_gate").unwrap();

    let mut state = state_in("room.east_marsh", Pos { x: 22, y: 8 });
    let mut tick = 0u64;

    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
    assert_eq!(state.room, east_marsh, "must not cross without the lantern");
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::DoorBlocked {
                lock: LockKind::Lantern
            }
        )),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::MoveBlocked { .. })),
        "{events:?}"
    );

    state.hero.has_lantern = true;
    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
    assert_eq!(state.room, sanctuary_gate);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { .. })),
        "{events:?}"
    );
}

#[test]
fn npc_with_an_unsatisfied_condition_reports_a_message_and_starts_no_dialogue() {
    let mut state = state_in("room.east_marsh", Pos { x: 17, y: 5 });
    let mut tick = 0u64;
    assert!(!state.progress.flags.contains("flag.told_about_sanctuary"));

    advance(&mut state, &mut tick, &[Action::MoveEast]); // face npc.warden at (18, 5)
    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "They have nothing to say yet.")),
        "{events:?}"
    );
    assert!(state.dialogue.is_none());
}

#[test]
fn step_plates_puzzle_solves_in_any_press_order() {
    // Press plate.mill2 (10,9), then plate.mill1 (5,9), then plate.mill3 (15,9) — not the
    // authored plates-list order — and reach the alcove only once all three are down.
    let mut state = state_in("room.old_mill", Pos { x: 12, y: 9 });
    let mut tick = 0u64;

    move_steps(&mut state, &mut tick, Action::MoveWest, 2); // -> plate.mill2 @ (10,9)
    assert_eq!(state.plates.pressed.len(), 1);
    assert!(state.progress.solved_puzzles.is_empty());

    move_steps(&mut state, &mut tick, Action::MoveWest, 5); // -> plate.mill1 @ (5,9)
    assert_eq!(state.plates.pressed.len(), 2);
    assert!(state.progress.solved_puzzles.is_empty());

    move_steps(&mut state, &mut tick, Action::MoveEast, 10); // -> plate.mill3 @ (15,9)
    assert_eq!(state.plates.pressed.len(), 3);
    assert!(
        !state.progress.solved_puzzles.is_empty(),
        "all three plates pressed must solve the puzzle"
    );
    assert!(state.walkable(state.room, Pos { x: 20, y: 1 }));

    // The reward chest is now reachable.
    move_steps(&mut state, &mut tick, Action::MoveNorth, 8); // row9 -> row1
    move_steps(&mut state, &mut tick, Action::MoveEast, 5); // -> (20, 1)
    assert_eq!(state.hero.pos, Pos { x: 20, y: 1 });
    advance(&mut state, &mut tick, &[Action::MoveEast]); // face chest.mill_lantern
    let events = advance(&mut state, &mut tick, &[Action::Interact]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::Lantern
            }
        )),
        "{events:?}"
    );
    assert!(state.hero.has_lantern);
}

#[test]
fn leaving_the_room_mid_puzzle_resets_the_pressed_plates() {
    let mut state = state_in("room.old_mill", Pos { x: 5, y: 10 });
    let mut tick = 0u64;
    move_steps(&mut state, &mut tick, Action::MoveNorth, 1); // -> plate.mill1 @ (5,9)
    assert_eq!(state.plates.pressed.len(), 1);

    // `spawn_enemies` is exactly what a room transition runs (see `state::update`'s door branch)
    // — simulating leaving and returning without walking the whole way there.
    state.spawn_enemies();
    assert!(state.plates.pressed.is_empty());
    assert!(state.progress.solved_puzzles.is_empty());
}

#[test]
fn a_solved_puzzle_stays_solved_after_leaving_and_returning() {
    let mut state = state_in("room.old_mill", Pos { x: 5, y: 10 });
    let mut tick = 0u64;
    move_steps(&mut state, &mut tick, Action::MoveNorth, 1); // plate.mill1
    move_steps(&mut state, &mut tick, Action::MoveEast, 5); // plate.mill2
    move_steps(&mut state, &mut tick, Action::MoveEast, 5); // plate.mill3
    assert!(!state.progress.solved_puzzles.is_empty());

    state.spawn_enemies(); // leave and return
    assert!(
        !state.progress.solved_puzzles.is_empty(),
        "solved puzzles persist across room re-entry"
    );
    assert!(
        state.walkable(state.room, Pos { x: 20, y: 1 }),
        "its reveal stays applied"
    );
}

/// The start-to-lantern main route, driven only by ordinary `Action`s, asserting the §7 milestone
/// order (talk to the keeper, take the sword, solve the mill puzzle for the lantern, talk to the
/// warden, cross the now-open dungeon entrance). Intra-room movement goes through
/// `common::Runner::walk_to` (a real BFS over `GameState::walkable`) rather than hand-counted
/// tile offsets, so this does not depend on memorising the room grids by hand.
#[test]
fn main_route_milestones_happen_in_order() {
    let mut runner = common::Runner::new(1);
    let world = Rc::clone(&runner.state.world);

    assert!(!runner.state.hero.has_sword);
    assert!(!runner.state.hero.has_lantern);
    assert!(runner.state.progress.flags.is_empty());

    // Talk to the keeper: two nodes, the second sets the sanctuary flag.
    runner.walk_to(Pos { x: 5, y: 6 });
    runner.face(Facing::North); // npc.keeper at (5, 5)
    let events = runner.step(&[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueStarted { .. })),
        "{events:?}"
    );
    let events = runner.step(&[Action::Confirm]); // -> node 1, sets the flag
    assert!(
        events.iter().any(
            |e| matches!(e, GameEvent::FlagSet { flag } if flag == "flag.told_about_sanctuary")
        ),
        "{events:?}"
    );
    let events = runner.step(&[Action::Confirm]); // past the last node -> closes
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueEnded { .. })),
        "{events:?}"
    );
    assert!(runner
        .state
        .progress
        .flags
        .contains("flag.told_about_sanctuary"));

    // Cross north into the crossroads and take the sword.
    runner.walk_to(Pos { x: 12, y: 1 });
    runner.cross_door(Action::MoveNorth); // door.lighthouse.north
    assert_eq!(
        runner.state.room,
        world.room_idx("room.crossroads").unwrap()
    );

    runner.walk_to(Pos { x: 5, y: 6 });
    runner.face(Facing::North); // chest.forest_sword at (5, 5)
    let events = runner.step(&[Action::Interact]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::Sword
            }
        )),
        "{events:?}"
    );
    assert!(runner.state.hero.has_sword);
    assert!(
        !runner.state.hero.has_lantern,
        "milestone order: the sword comes before the lantern"
    );

    // Milestone 3: a first slime encounter is now possible — `room.west_grove`, one door from
    // the sword-carrying hero's current room, carries an authored Slime spawn.
    let west_grove = world.room(world.room_idx("room.west_grove").unwrap());
    assert!(
        west_grove
            .enemies
            .iter()
            .any(|e| e.kind == mosslight::game::EnemyKind::Slime),
        "room.west_grove must carry a Slime spawn for the first slime encounter"
    );
    assert!(
        world
            .room(runner.state.room)
            .doors
            .iter()
            .any(|d| d.to_room == "room.west_grove"),
        "room.west_grove must be one door from the sword room"
    );

    // Cross north into stone_circle, then east into the mill.
    runner.walk_to(Pos { x: 12, y: 1 });
    runner.cross_door(Action::MoveNorth); // door.crossroads.north
    assert_eq!(
        runner.state.room,
        world.room_idx("room.stone_circle").unwrap()
    );

    runner.walk_to(Pos { x: 22, y: 8 });
    runner.cross_door(Action::MoveEast); // door.stone_circle.east
    assert_eq!(runner.state.room, world.room_idx("room.old_mill").unwrap());

    // Solve the mill puzzle (pressing plates in whatever order the BFS routes them) and take the
    // lantern from the now-reachable alcove.
    runner.walk_to(Pos { x: 5, y: 9 }); // plate.mill1
    runner.walk_to(Pos { x: 10, y: 9 }); // plate.mill2
    runner.walk_to(Pos { x: 15, y: 9 }); // plate.mill3
    assert!(!runner.state.progress.solved_puzzles.is_empty());

    runner.walk_to(Pos { x: 20, y: 1 });
    runner.face(Facing::East); // chest.mill_lantern at (21, 1)
    let events = runner.step(&[Action::Interact]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::Lantern
            }
        )),
        "{events:?}"
    );
    assert!(
        runner.state.hero.has_lantern,
        "milestone order: the lantern comes after the sword"
    );

    // Cross south into east_marsh, talk to the now-unlocked warden, then cross the dungeon
    // entrance the lantern just opened.
    runner.walk_to(Pos { x: 12, y: 14 });
    runner.cross_door(Action::MoveSouth); // door.old_mill.south
    assert_eq!(
        runner.state.room,
        world.room_idx("room.east_marsh").unwrap()
    );

    runner.walk_to(Pos { x: 17, y: 5 });
    runner.face(Facing::East); // npc.warden at (18, 5)
    let events = runner.step(&[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueStarted { .. })),
        "the warden must now have something to say: {events:?}"
    );
    runner.step(&[Action::Confirm]); // single node -> closes

    runner.walk_to(Pos { x: 22, y: 8 });
    let events = runner.cross_door(Action::MoveEast); // door.east_marsh.dungeon_entrance
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { .. })),
        "{events:?}"
    );
    assert_eq!(
        runner.state.room,
        world.room_idx("room.sanctuary_gate").unwrap(),
        "milestone order: the dungeon entrance opens only after the lantern"
    );
}

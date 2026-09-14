//! Room transitions (spec §7, §13): walking through a door moves the hero into the target room
//! at a walkable, non-door tile; `progress.visited` and `RoomEntered` track exactly that.

use std::collections::BTreeSet;
use std::rc::Rc;

use mosslight::content::World;
use mosslight::game::tuning::{ROOM_H, ROOM_W};
use mosslight::game::{update, Action, GameEvent, GameState, Pos, RoomIdx, Tile};

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

fn state_at(world: &Rc<World>, room: RoomIdx, pos: Pos) -> GameState {
    let mut state = GameState::new(1, Rc::clone(world));
    state.room = room;
    state.progress.visited = BTreeSet::from([room]);
    state.hero.pos = pos;
    // Phase 4 authors a lantern-locked door (`door.east_marsh.dungeon_entrance`); phase 5 adds two
    // `SmallKey`-locked doors. These tests are generic door-wiring/geometry checks, not
    // lock-gating (see `tests/overworld.rs`/`tests/dungeon.rs` for that), so the precondition
    // every lock needs is established directly.
    state.hero.has_lantern = true;
    state.hero.keys = 10;
    state
}

/// Which wall of the room a door sits on. Doors are named after this edge
/// (`door.crossroads.north`), and the target room's `map_index` must sit exactly one grid step
/// away from the source room's, in this edge's direction — the geometry
/// `every_door_leads_to_its_target_room` checks the wiring against (round-1 review, minor: the
/// test used to only check that the engine honours whatever a door declares, never that the
/// declaration itself matches the grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    North,
    South,
    East,
    West,
}

impl Edge {
    fn label(self) -> &'static str {
        match self {
            Edge::North => "north",
            Edge::South => "south",
            Edge::East => "east",
            Edge::West => "west",
        }
    }

    /// `(map_index.0 delta, map_index.1 delta)` for stepping through a door on this edge.
    fn map_index_delta(self) -> (i32, i32) {
        match self {
            Edge::North => (0, -1),
            Edge::South => (0, 1),
            Edge::East => (1, 0),
            Edge::West => (-1, 0),
        }
    }
}

fn edge_of(at: Pos) -> Edge {
    if at.y == 0 {
        Edge::North
    } else if at.y == (ROOM_H - 1) as u8 {
        Edge::South
    } else if at.x == 0 {
        Edge::West
    } else if at.x == (ROOM_W - 1) as u8 {
        Edge::East
    } else {
        panic!("door at {at:?} is not on the wall ring — update `edge_of` for interior doors");
    }
}

/// The tile one step in front of a door on the wall ring, and the action that steps onto it.
/// Every door in `assets/world.ron` sits on a mid-edge wall tile (see PLAN.md), so this is a
/// generic, geometry-driven approach rather than a hardcoded per-room position.
fn approach(at: Pos) -> (Pos, Action) {
    match edge_of(at) {
        Edge::North => (Pos { x: at.x, y: 1 }, Action::MoveNorth),
        Edge::South => (
            Pos {
                x: at.x,
                y: at.y - 1,
            },
            Action::MoveSouth,
        ),
        Edge::West => (Pos { x: 1, y: at.y }, Action::MoveWest),
        Edge::East => (
            Pos {
                x: at.x - 1,
                y: at.y,
            },
            Action::MoveEast,
        ),
    }
}

fn orthogonally_adjacent(a: Pos, b: Pos) -> bool {
    let dx = (a.x as i32 - b.x as i32).abs();
    let dy = (a.y as i32 - b.y as i32).abs();
    (dx == 1 && dy == 0) || (dx == 0 && dy == 1)
}

#[test]
fn every_door_leads_to_its_target_room() {
    let world = world();
    for (i, room) in world.rooms.iter().enumerate() {
        let room_idx = RoomIdx(i as u16);
        for door in &room.doors {
            let (approach_pos, action) = approach(door.at);
            let mut state = state_at(&world, room_idx, approach_pos);

            update(&mut state, &[action], 0);

            let target_idx = world
                .room_idx(&door.to_room)
                .unwrap_or_else(|| panic!("{} targets an unknown room", door.id));
            assert_eq!(
                state.room, target_idx,
                "{} should lead to room '{}'",
                door.id, door.to_room
            );

            // Geometry check: the target room's map_index must be exactly one grid step from
            // this room's, in the direction the door sits on the wall ring. Without this, a door
            // wired to the wrong room (e.g. `door.crossroads.north` pointing at `room.old_mill`
            // instead of `room.stone_circle`) would still pass every assertion above — the
            // engine faithfully honours a wrong declaration (round-1 review, minor).
            //
            // The grid itself is the 9-room overworld only: a door into the dungeon vestibule
            // (phase 4's `room.sanctuary_gate`, `map_index: None`) has no grid cell to check
            // against.
            let edge = edge_of(door.at);
            let target_room = world.room(target_idx);
            if let Some((col, row)) = room.map_index {
                if let Some((tc, tr)) = target_room.map_index {
                    let (dx, dy) = edge.map_index_delta();
                    let expected = (col as i32 + dx, row as i32 + dy);
                    assert_eq!(
                        (tc as i32, tr as i32),
                        expected,
                        "{}: the {} neighbour of {:?} should be map_index {expected:?}, but '{}' has map_index {:?}",
                        door.id,
                        edge.label(),
                        room.map_index,
                        door.to_room,
                        target_room.map_index
                    );
                }
            }
            // The lantern-locked dungeon entrance is named for what it is, not for the wall edge
            // it sits on (PLAN.md); every ordinary overworld door still follows the convention.
            if door.id != "door.east_marsh.dungeon_entrance" {
                assert!(
                    door.id.ends_with(edge.label()),
                    "{}: door id should end with its own edge '{}'",
                    door.id,
                    edge.label()
                );
            }
        }
    }
}

#[test]
fn arrival_tile_is_walkable_and_not_a_door() {
    let world = world();
    for (i, room) in world.rooms.iter().enumerate() {
        let room_idx = RoomIdx(i as u16);
        for door in &room.doors {
            let (approach_pos, action) = approach(door.at);
            let mut state = state_at(&world, room_idx, approach_pos);

            update(&mut state, &[action], 0);

            let tile = state.room().tile_at(state.hero.pos);
            assert!(
                tile.is_some_and(Tile::is_walkable),
                "{}: arrival tile {:?} is not walkable ({:?})",
                door.id,
                state.hero.pos,
                tile
            );
            assert_ne!(
                tile,
                Some(Tile::Door),
                "{}: arrival tile must not itself be a door",
                door.id
            );
        }
    }
}

#[test]
fn reciprocal_door_returns_to_the_origin_door() {
    let world = world();
    for (i, room) in world.rooms.iter().enumerate() {
        let room_idx = RoomIdx(i as u16);
        for door in &room.doors {
            if !door.two_way {
                continue;
            }
            let (approach_pos, action) = approach(door.at);
            let mut state = state_at(&world, room_idx, approach_pos);
            update(&mut state, &[action], 0);
            let target_idx = state.room;
            let target_room = world.room(target_idx);

            // The reciprocal door: it leads back to `room` (each room pair in this world is
            // joined by exactly one two-way door, guaranteed by the validator's reciprocity
            // check, so there is exactly one match).
            let reciprocal = target_room
                .doors
                .iter()
                .find(|b| b.two_way && b.to_room == room.id)
                .unwrap_or_else(|| {
                    panic!("{} has no reciprocal door back to '{}'", door.id, room.id)
                });

            let (back_approach, back_action) = approach(reciprocal.at);
            let mut back_state = state_at(&world, target_idx, back_approach);
            update(&mut back_state, &[back_action], 0);

            assert_eq!(
                back_state.room, room_idx,
                "{} should return to '{}'",
                reciprocal.id, room.id
            );
            assert!(
                orthogonally_adjacent(back_state.hero.pos, door.at),
                "{}: returning via {} should land adjacent to the original door {:?}, landed at {:?}",
                door.id,
                reciprocal.id,
                door.at,
                back_state.hero.pos
            );
        }
    }
}

#[test]
fn visited_set_grows_once_per_new_room() {
    let world = world();
    let room = &world.rooms[0];
    let door = room
        .doors
        .first()
        .expect("every overworld room has at least one door");
    let (approach_pos, action) = approach(door.at);
    let mut state = state_at(&world, RoomIdx(0), approach_pos);
    assert_eq!(state.progress.visited.len(), 1);

    let events = update(&mut state, &[action], 0);
    assert_eq!(state.progress.visited.len(), 2);
    assert!(state.progress.visited.contains(&state.room));
    let room = state.room;
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::RoomEntered { room: r, first_visit: true } if *r == room
        )),
        "entering a never-before-seen room must emit first_visit: true: {events:?}"
    );
}

#[test]
fn re_entering_a_visited_room_does_not_grow_the_set() {
    let world = world();
    let room = &world.rooms[0];
    let door = room
        .doors
        .first()
        .expect("every overworld room has at least one door");
    let target_idx = world.room_idx(&door.to_room).unwrap();
    let target_room = world.room(target_idx);
    let reciprocal = target_room
        .doors
        .iter()
        .find(|b| b.to_room == room.id)
        .expect("reciprocal door exists");

    let (approach_pos, action) = approach(door.at);
    let mut state = state_at(&world, RoomIdx(0), approach_pos);
    update(&mut state, &[action], 0);
    assert_eq!(state.progress.visited.len(), 2);

    // Walk straight back out through the reciprocal door: room.a is already visited.
    let (back_approach, back_action) = approach(reciprocal.at);
    state.hero.pos = back_approach;
    let events = update(
        &mut state,
        &[back_action],
        mosslight::game::tuning::HERO_STEP_TICKS,
    );
    assert_eq!(state.room, RoomIdx(0));
    assert_eq!(
        state.progress.visited.len(),
        2,
        "re-entering must not grow the set"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::RoomEntered {
                room: RoomIdx(0),
                first_visit: false
            }
        )),
        "re-entering an already-visited room must emit first_visit: false: {events:?}"
    );
}

#[test]
fn room_entered_is_emitted_once_per_transition_and_not_for_the_start_room() {
    let world = world();
    let room = &world.rooms[0];
    let door = room
        .doors
        .first()
        .expect("every overworld room has at least one door");
    let (approach_pos, action) = approach(door.at);
    let mut state = state_at(&world, RoomIdx(0), approach_pos);

    // A non-transitioning step first: no RoomEntered.
    let target_idx = world.room_idx(&door.to_room).unwrap();
    let no_op_events = update(&mut state, &[], 0);
    assert!(no_op_events
        .iter()
        .all(|e| !matches!(e, GameEvent::RoomEntered { .. })));

    let events = update(
        &mut state,
        &[action],
        mosslight::game::tuning::HERO_STEP_TICKS,
    );
    let room_entered_count = events
        .iter()
        .filter(|e| matches!(e, GameEvent::RoomEntered { room, .. } if *room == target_idx))
        .count();
    assert_eq!(room_entered_count, 1);
}

#[test]
fn a_blocked_move_into_a_wall_next_to_a_door_emits_no_room_entered() {
    let world = world();
    let room = &world.rooms[0];
    let door = room
        .doors
        .first()
        .expect("every overworld room has at least one door");
    // Stand right beside the door (not on the approach line) and walk into the solid wall next
    // to it instead of through the door.
    let beside = if door.at.y == 0 || door.at.y == (ROOM_H - 1) as u8 {
        Pos {
            x: door.at.x.saturating_sub(2).max(1),
            y: 1,
        }
    } else {
        Pos {
            x: 1,
            y: door.at.y.saturating_sub(2).max(1),
        }
    };
    let mut state = state_at(&world, RoomIdx(0), beside);
    let action = if door.at.y == 0 || door.at.y == (ROOM_H - 1) as u8 {
        Action::MoveNorth
    } else {
        Action::MoveWest
    };

    let events = update(&mut state, &[action], 0);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::MoveBlocked { .. })));
    assert!(events
        .iter()
        .all(|e| !matches!(e, GameEvent::RoomEntered { .. })));
}

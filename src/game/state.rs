//! `GameState`: the whole simulated world, plus the pure `update` entry point.

use std::collections::BTreeSet;
use std::rc::Rc;

use super::entities::{Facing, Hero, Pos};
use super::rng::Rng;
use super::world::{Room, RoomIdx, Tile, World};

pub type Tick = u64;

/// What of the world the hero has done so far. `visited` is the only field this phase; phase 4-6
/// add `opened_chests`, `unlocked_doors`, `solved_puzzles`, `flags`. `BTreeSet` keeps iteration
/// order deterministic for hashing and saving.
///
/// Deliberately not `Serialize`: `RoomIdx` is a dense index assigned by file order
/// (`loader::intern_room_ids`), so serializing it directly would make a save format built on it
/// invalidated by content reordering — exactly what DECISIONS.md's ids-not-indices rationale for
/// the save format (phase 6) rules out. Phase 6 defines the on-disk shape (ids, not indices)
/// rather than this phase accidentally fixing the wrong one (round-1 review, minor).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub visited: BTreeSet<RoomIdx>,
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub tick: Tick,
    pub rng: Rng,
    pub hero: Hero,
    /// Immutable, shared, never part of a save or a state hash — see DECISIONS.md ("plan/02").
    pub world: Rc<World>,
    pub room: RoomIdx,
    pub progress: Progress,
}

impl GameState {
    /// Builds a fresh state at `world.start`. The world is assumed already validated (content
    /// preflight runs before any `GameState` is constructed), so the start room/spawn resolve.
    pub fn new(seed: u64, world: Rc<World>) -> Self {
        let start_room = world
            .start_room()
            .expect("validated world resolves start.room");
        let spawn = world
            .spawn_pos(start_room, &world.start.spawn)
            .expect("validated world resolves start.spawn");

        let mut visited = BTreeSet::new();
        visited.insert(start_room);

        GameState {
            tick: 0,
            rng: Rng::new(seed),
            hero: Hero::at_spawn(spawn),
            world,
            room: start_room,
            progress: Progress { visited },
        }
    }

    pub fn room(&self) -> &Room {
        self.world.room(self.room)
    }
}

impl PartialEq for GameState {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
            && self.rng == other.rng
            && self.hero == other.hero
            && self.room == other.room
            && self.progress == other.progress
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MoveNorth,
    MoveEast,
    MoveSouth,
    MoveWest,
    Attack,
    UseLantern,
    Interact,
    Confirm,
    Cancel,
    ToggleMap,
    ToggleInventory,
    Help,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    HeroMoved { from: Pos, to: Pos },
    MoveBlocked { at: Pos, facing: Facing },
    Message(String),
    RoomEntered { room: RoomIdx, first_visit: bool },
}

fn facing_for(action: Action) -> Option<Facing> {
    match action {
        Action::MoveNorth => Some(Facing::North),
        Action::MoveEast => Some(Facing::East),
        Action::MoveSouth => Some(Facing::South),
        Action::MoveWest => Some(Facing::West),
        _ => None,
    }
}

fn step_target(pos: Pos, facing: Facing) -> Option<Pos> {
    match facing {
        Facing::North => pos.y.checked_sub(1).map(|y| Pos { x: pos.x, y }),
        Facing::South => pos.y.checked_add(1).map(|y| Pos { x: pos.x, y }),
        Facing::East => pos.x.checked_add(1).map(|x| Pos { x, y: pos.y }),
        Facing::West => pos.x.checked_sub(1).map(|x| Pos { x, y: pos.y }),
    }
}

/// The only simulation entry point. Total: never panics, never touches a clock, returns events.
///
/// An attempted move always sets facing; a step applies only once the cooldown has elapsed; it is
/// refused off-grid or onto a non-walkable tile, in which case facing still updates and a
/// `MoveBlocked` event is emitted. Stepping onto a door tile relocates the hero to the target
/// room's spawn, grows `progress.visited` and emits `RoomEntered` after `HeroMoved` — the door's
/// target is validator-guaranteed to resolve, so an unresolved target leaves the hero standing on
/// the door tile instead of panicking (the simulation is total). Non-movement actions are
/// accepted and ignored this phase.
pub fn update(state: &mut GameState, actions: &[Action], tick: Tick) -> Vec<GameEvent> {
    state.tick = tick;
    let mut events = Vec::new();

    for &action in actions {
        if let Some(facing) = facing_for(action) {
            state.hero.facing = facing;
            if !state.hero.can_step(tick) {
                continue;
            }
            let from = state.hero.pos;
            let target = step_target(from, facing);
            let walkable = target
                .and_then(|p| state.room().tile_at(p))
                .map(Tile::is_walkable)
                .unwrap_or(false);
            if walkable {
                let to = target.expect("walkable implies a valid target");
                state.hero.pos = to;
                state.hero.mark_stepped(tick);
                events.push(GameEvent::HeroMoved { from, to });

                if let Some(transition) = resolve_door(&state.world, state.room, to) {
                    state.room = transition.room;
                    state.hero.pos = transition.spawn;
                    let first_visit = state.progress.visited.insert(transition.room);
                    events.push(GameEvent::RoomEntered {
                        room: transition.room,
                        first_visit,
                    });
                }
            } else {
                events.push(GameEvent::MoveBlocked { at: from, facing });
            }
        }
        // Attack, UseLantern, Interact, ToggleMap, ToggleInventory: accepted, no-op this phase.
    }

    events
}

struct Transition {
    room: RoomIdx,
    spawn: Pos,
}

fn resolve_door(world: &World, room: RoomIdx, at: Pos) -> Option<Transition> {
    let door = world.door_at(room, at)?;
    let target_room = world.room_idx(&door.to_room)?;
    let spawn = world.spawn_pos(target_room, &door.to_spawn)?;
    Some(Transition {
        room: target_room,
        spawn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> GameState {
        let world = Rc::new(crate::content::load().expect("embedded world validates"));
        GameState::new(1, world)
    }

    #[test]
    fn stepping_onto_a_door_tile_changes_room_and_lands_on_a_walkable_tile() {
        let mut state = fresh();
        let start_room = state.room;

        // room.lighthouse's north door sits at (12, 0); one tile south of it is walkable floor.
        state.hero.pos = Pos { x: 12, y: 1 };
        let door_at = state
            .world
            .door_at(start_room, Pos { x: 12, y: 0 })
            .expect("room.lighthouse has a north door in the real world");
        let target_room = state
            .world
            .room_idx(&door_at.to_room)
            .expect("validated door target resolves");

        let events = update(&mut state, &[Action::MoveNorth], 0);

        assert_eq!(state.room, target_room);
        assert_ne!(state.room, start_room);
        assert!(state
            .room()
            .tile_at(state.hero.pos)
            .is_some_and(Tile::is_walkable));
        assert_ne!(
            state.room().tile_at(state.hero.pos),
            Some(Tile::Door),
            "arrival tile must not itself be a door"
        );
        assert!(state.progress.visited.contains(&target_room));
        assert!(events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { room, .. } if *room == target_room)));
    }
}

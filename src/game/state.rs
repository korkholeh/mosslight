//! `GameState`: the whole simulated world, plus the pure `update` entry point.

use super::entities::{Facing, Hero, Pos};
use super::rng::Rng;
use super::world::{Room, Tile};

pub type Tick = u64;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GameState {
    pub tick: Tick,
    pub rng: Rng,
    pub hero: Hero,
    pub room: Room,
}

impl GameState {
    pub fn new(seed: u64, room: Room) -> Self {
        let spawn = room.spawn;
        GameState {
            tick: 0,
            rng: Rng::new(seed),
            hero: Hero::at_spawn(spawn),
            room,
        }
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
/// Phase-1 rules: an attempted move always sets facing; a step applies only once the cooldown has
/// elapsed; it is refused off-grid or onto a non-`Floor` tile, in which case facing still updates
/// and a `MoveBlocked` event is emitted. Non-movement actions are accepted and ignored this phase.
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
                .and_then(|p| state.room.tile_at(p))
                .map(Tile::is_walkable)
                .unwrap_or(false);
            if walkable {
                let to = target.expect("walkable implies a valid target");
                state.hero.pos = to;
                state.hero.mark_stepped(tick);
                events.push(GameEvent::HeroMoved { from, to });
            } else {
                events.push(GameEvent::MoveBlocked { at: from, facing });
            }
        }
        // Attack, UseLantern, Interact, ToggleMap, ToggleInventory: accepted, no-op this phase.
    }

    events
}

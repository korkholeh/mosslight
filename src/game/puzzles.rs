//! `StepPlates`, `BlockOnPlates` and `TorchSequence` (spec §7 step 4).
//!
//! `TorchSequence` lighting itself lives in `state::handle_use_lantern` (a sequence torch is a
//! variant of "use the lantern on a torch", not room-movement-triggered like the other two kinds).

use std::collections::BTreeSet;
use std::rc::Rc;

use super::entities::{Facing, ObjectRef, Pos};
use super::state::{apply_reward, GameEvent, GameState, Tick};
use super::world::{Puzzle, PuzzleKind, Room, RoomIdx};

/// Transient room-local puzzle state: which plates are pressed, the live position of every block
/// in the room, and the sequence of `TorchSequence` torches lit so far. Cleared and reseeded from
/// the authored content on every room entry (`GameState::enter_room`), so an unsolved puzzle
/// resets on re-entry (spec §6) and a solved puzzle's effect lives only in
/// `Progress::solved_puzzles`. This is also the anti-soft-lock guarantee: any wrong state a player
/// can reach is undone by walking out of the room and back in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PuzzleState {
    pub pressed: BTreeSet<u16>,
    pub blocks: Vec<Pos>,
    pub sequence: Vec<u16>,
}

impl PuzzleState {
    /// Seeds `blocks` from `room`'s authored reset positions; `pressed`/`sequence` start empty.
    pub fn for_room(room: &Room) -> Self {
        PuzzleState {
            pressed: BTreeSet::new(),
            blocks: room.blocks.iter().map(|b| b.at).collect(),
            sequence: Vec::new(),
        }
    }
}

/// The one push-legality predicate, shared by the simulation (`state::update`'s movement branch)
/// and the content validator's one-block push search, so the two can never disagree about what a
/// player can do. `state_blocks` is every block currently in the room (so two blocks can never
/// overlap); `occupied` is every live enemy's position (the hero is the one doing the pushing, so
/// it is never itself a candidate obstacle). Legal only onto plain `Tile::Floor` — a plate's tile
/// is authored as `Tile::Floor` underneath the plate, so this single check covers "empty floor or
/// a plate" without a special case, while a door, stairs, water, pit, bush, wall or an unrevealed
/// `Hidden` tile are all refused by the same `tile == Tile::Floor` test.
pub fn block_push_target(
    room: &Room,
    state_blocks: &[Pos],
    occupied: &std::collections::HashSet<Pos>,
    block_pos: Pos,
    facing: Facing,
) -> Option<Pos> {
    let target = super::state::step_target(block_pos, facing)?;
    if room.tile_at(target) != Some(super::world::Tile::Floor) {
        return None;
    }
    if room.object_at(target).is_some() {
        return None;
    }
    if state_blocks.contains(&target) {
        return None;
    }
    if occupied.contains(&target) {
        return None;
    }
    Some(target)
}

/// Marks `puzzle` (at `room_idx`/`index`) solved: applies `reveals`, grants `reward`, and records
/// it in `progress.solved_puzzles`. Shared by `on_hero_moved` (`StepPlates`/`BlockOnPlates`) and
/// `state::handle_use_lantern` (`TorchSequence`).
pub(super) fn solve_puzzle(
    state: &mut GameState,
    events: &mut Vec<GameEvent>,
    room_idx: RoomIdx,
    index: u16,
    puzzle: &Puzzle,
) {
    let obj = ObjectRef {
        room: room_idx,
        index,
    };
    state.progress.solved_puzzles.insert(obj);
    for &pos in &puzzle.reveals {
        events.push(GameEvent::PassageRevealed {
            room: room_idx,
            at: pos,
        });
    }
    if let Some(reward) = &puzzle.reward {
        events.extend(apply_reward(&mut state.hero, reward));
    }
    events.push(GameEvent::PuzzleSolved { puzzle: obj });
}

/// Called from `update` right after a successful hero step (ordinary or a block push). If the
/// hero's new tile holds a plate, marks it pressed (stepping off never releases it — pressure
/// holds, so `StepPlates` has no ordering and no soft-lock). Then re-evaluates every unsolved
/// `StepPlates`/`BlockOnPlates` puzzle in the room: `StepPlates` solves when every named plate has
/// ever been pressed; `BlockOnPlates` solves when every named plate currently holds a block
/// (positional, so pushing a block *off* a plate before the puzzle completes un-presses it —
/// visible feedback, never a soft-lock since the block can be pushed back or the room re-entered).
/// `TorchSequence` is untouched here — see `state::handle_use_lantern`.
pub fn on_hero_moved(state: &mut GameState, _tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let room_idx = state.room;
    let world = Rc::clone(&state.world);
    let room = world.room(room_idx);

    if let Some(plate_index) = room.plate_at(state.hero.pos) {
        if state.puzzle.pressed.insert(plate_index) {
            events.push(GameEvent::PlatePressed { index: plate_index });
        }
    }

    for (i, puzzle) in room.puzzles.iter().enumerate() {
        let index = i as u16;
        let obj = ObjectRef {
            room: room_idx,
            index,
        };
        if state.progress.solved_puzzles.contains(&obj) {
            continue;
        }
        let solved = match puzzle.kind {
            PuzzleKind::StepPlates => {
                !puzzle.plates.is_empty()
                    && puzzle.plates.iter().all(|id| {
                        room.plate_index(id)
                            .is_some_and(|idx| state.puzzle.pressed.contains(&idx))
                    })
            }
            PuzzleKind::BlockOnPlates => {
                !puzzle.plates.is_empty()
                    && puzzle.plates.iter().all(|id| {
                        room.plate_index(id)
                            .and_then(|idx| room.plates.get(idx as usize))
                            .is_some_and(|p| state.puzzle.blocks.contains(&p.at))
                    })
            }
            PuzzleKind::TorchSequence => false,
        };
        if solved {
            solve_puzzle(state, &mut events, room_idx, index, puzzle);
        }
    }

    events
}

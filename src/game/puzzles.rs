//! `StepPlates`: the overworld puzzle kind (spec §7 step 4). `PushBlock`/`Switches` are phase 5.

use std::collections::BTreeSet;
use std::rc::Rc;

use super::entities::ObjectRef;
use super::state::{apply_reward, GameEvent, GameState, Tick};
use super::world::PuzzleKind;

/// Transient: which plates of the current room are currently pressed, by index into
/// `Room::plates`. Cleared on room exit (`GameState::spawn_enemies`), so an unsolved puzzle
/// resets on re-entry (spec §6); a solved puzzle lives in `Progress::solved_puzzles`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlateState {
    pub pressed: BTreeSet<u16>,
}

/// Called from `update` right after a successful hero step. If the hero's new tile holds a
/// plate, marks it pressed; if every plate an unsolved `StepPlates` puzzle in this room names is
/// now pressed, solves it: applies `reveals`, grants `reward`, and records it in
/// `progress.solved_puzzles`. Stepping off a plate never releases it — pressure holds, so no
/// ordering and no soft-lock is possible.
pub fn on_hero_moved(state: &mut GameState, _tick: Tick) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let room_idx = state.room;
    let world = Rc::clone(&state.world);
    let room = world.room(room_idx);

    let Some(plate_index) = room.plate_at(state.hero.pos) else {
        return events;
    };
    if state.plates.pressed.insert(plate_index) {
        events.push(GameEvent::PlatePressed { index: plate_index });
    }

    for (i, puzzle) in room.puzzles.iter().enumerate() {
        if puzzle.kind != PuzzleKind::StepPlates || puzzle.plates.is_empty() {
            continue;
        }
        let obj = ObjectRef {
            room: room_idx,
            index: i as u16,
        };
        if state.progress.solved_puzzles.contains(&obj) {
            continue;
        }
        let all_pressed = puzzle.plates.iter().all(|id| {
            room.plate_index(id)
                .is_some_and(|idx| state.plates.pressed.contains(&idx))
        });
        if !all_pressed {
            continue;
        }

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

    events
}

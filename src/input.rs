//! `KeyEvent` -> `Action` mapping, per-mode maps, coalescing, and the overflow policy (spec §5).
//!
//! `map_key` matches on `KeyCode` only and never inspects a key event's press/release kind, so
//! there is no key-release path to remove later (checked structurally by
//! `tests/no_key_release.rs`).

use std::io;

use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::Mode;
use crate::game::Action;

/// Maps a pressed key to a semantic action for the given mode. Movement and global keys
/// (Quit, Help, Cancel) are the same in every mode; `Confirm`/`Interact` share the E/Enter keys
/// but are named differently to keep menu selection and world interaction visually distinct at
/// call sites.
pub fn map_key(mode: Mode, key: KeyEvent) -> Option<Action> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => Some(Action::MoveNorth),
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => Some(Action::MoveSouth),
        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::MoveWest),
        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::MoveEast),
        KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Char(' ') => Some(Action::Attack),
        KeyCode::Char('k') | KeyCode::Char('K') => Some(Action::UseLantern),
        KeyCode::Char('m') | KeyCode::Char('M') => Some(Action::ToggleMap),
        KeyCode::Char('i') | KeyCode::Char('I') => Some(Action::ToggleInventory),
        KeyCode::Char('?') => Some(Action::Help),
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
        KeyCode::Esc => Some(Action::Cancel),
        KeyCode::Enter | KeyCode::Char('e') | KeyCode::Char('E') => match mode {
            Mode::MainMenu
            | Mode::Paused
            | Mode::ConfirmQuit
            | Mode::Help
            | Mode::TooSmall
            | Mode::GameOver => Some(Action::Confirm),
            Mode::Playing => Some(Action::Interact),
        },
        _ => None,
    }
}

/// Keeps at most one movement action (the last one seen), preserving relative order of the rest.
pub fn coalesce(actions: &mut Vec<Action>) {
    let last_move_idx = actions.iter().rposition(|a| is_movement(*a));
    let mut kept = Vec::with_capacity(actions.len());
    for (i, action) in actions.iter().enumerate() {
        if is_movement(*action) {
            if Some(i) == last_move_idx {
                kept.push(*action);
            }
        } else {
            kept.push(*action);
        }
    }
    *actions = kept;
}

/// Clears buffered game actions, e.g. when a menu or dialogue closes (spec §5).
pub fn drop_pending(actions: &mut Vec<Action>) {
    actions.clear();
}

fn is_movement(action: Action) -> bool {
    matches!(
        action,
        Action::MoveNorth | Action::MoveEast | Action::MoveSouth | Action::MoveWest
    )
}

/// Maximum non-movement actions (Quit/Cancel/Confirm) retained once the per-iteration event cap
/// is hit and the surplus is drained and discarded (see DECISIONS.md).
pub const MAX_RETAINED_CONTROL_ACTIONS: usize = 8;

fn is_control(action: Action) -> bool {
    matches!(action, Action::Quit | Action::Cancel | Action::Confirm)
}

/// Applies the overflow policy to a raw batch of already-mapped actions read in one iteration:
/// the *last* `INPUT_EVENTS_PER_ITER` actions are kept as-is (the freshest input, since
/// `coalesce` only keeps the last movement it sees — keeping the oldest actions here would let a
/// large burst apply a stale movement instead of the one the player pressed most recently); from
/// the discarded, older surplus, up to `MAX_RETAINED_CONTROL_ACTIONS` control actions
/// (Quit/Cancel/Confirm) are rescued and kept ahead of the tail, so a queued quit is not silently
/// swallowed by a movement burst that follows it. Callers run `coalesce` on the result to collapse
/// it to at most one movement action.
pub fn apply_overflow_policy(actions: &[Action]) -> Vec<Action> {
    use crate::game::tuning::INPUT_EVENTS_PER_ITER;

    if actions.len() <= INPUT_EVENTS_PER_ITER {
        return actions.to_vec();
    }

    let split = actions.len() - INPUT_EVENTS_PER_ITER;
    let (surplus, tail) = actions.split_at(split);

    let mut kept = Vec::with_capacity(INPUT_EVENTS_PER_ITER + MAX_RETAINED_CONTROL_ACTIONS);
    for &action in surplus {
        if is_control(action)
            && kept.iter().filter(|a| is_control(**a)).count() < MAX_RETAINED_CONTROL_ACTIONS
        {
            kept.push(action);
        }
    }
    kept.extend_from_slice(tail);
    kept
}

/// A source of already-buffered terminal events, read without blocking. The seam that makes
/// "the per-iteration drain empties the queue" testable without a PTY: `main.rs`'s real source
/// wraps `crossterm::event::poll`/`read` with a zero timeout, tests use an in-memory queue.
pub trait EventSource {
    /// Returns the next already-buffered event, or `Ok(None)` if none are ready right now.
    fn next_ready(&mut self) -> io::Result<Option<Event>>;
}

/// What one call to [`drain_ready`] read.
#[derive(Debug, Default)]
pub struct DrainResult {
    pub events: Vec<Event>,
    /// Raw events read past `hard_cap` and thrown away immediately. Distinct from the
    /// movement/control overflow policy (`apply_overflow_policy`), which runs on the mapped
    /// actions afterwards — this only guards against a pathological flood.
    pub discarded: usize,
}

/// Drains every immediately-available event from `source` until it reports none ready, so a burst
/// can never leave events queued for a later iteration to replay as stale input (spec §5). A
/// `hard_cap` on the returned events is a livelock guard only, not the overflow policy itself.
pub fn drain_ready(source: &mut impl EventSource, hard_cap: usize) -> io::Result<DrainResult> {
    let mut result = DrainResult::default();
    while let Some(event) = source.next_ready()? {
        if result.events.len() < hard_cap {
            result.events.push(event);
        } else {
            result.discarded += 1;
        }
    }
    Ok(result)
}

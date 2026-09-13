//! Input policy: key bindings (spec §5), coalescing, the overflow cap, and pending-drop on
//! overlay close.

use std::collections::VecDeque;
use std::io;

use mosslight::app::Mode;
use mosslight::game::Action;
use mosslight::input::{
    apply_overflow_policy, coalesce, drain_ready, drop_pending, map_key, EventSource,
};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn arrows_and_wasd_map_to_moves() {
    for code in [KeyCode::Up, KeyCode::Char('w')] {
        assert_eq!(map_key(Mode::Playing, key(code)), Some(Action::MoveNorth));
    }
    for code in [KeyCode::Down, KeyCode::Char('s')] {
        assert_eq!(map_key(Mode::Playing, key(code)), Some(Action::MoveSouth));
    }
    for code in [KeyCode::Left, KeyCode::Char('a')] {
        assert_eq!(map_key(Mode::Playing, key(code)), Some(Action::MoveWest));
    }
    for code in [KeyCode::Right, KeyCode::Char('d')] {
        assert_eq!(map_key(Mode::Playing, key(code)), Some(Action::MoveEast));
    }
}

#[test]
fn combat_and_menu_keys_map_per_spec() {
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('j'))),
        Some(Action::Attack)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char(' '))),
        Some(Action::Attack)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('k'))),
        Some(Action::UseLantern)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('m'))),
        Some(Action::ToggleMap)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('i'))),
        Some(Action::ToggleInventory)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Esc)),
        Some(Action::Cancel)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('?'))),
        Some(Action::Help)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('q'))),
        Some(Action::Quit)
    );
}

#[test]
fn ctrl_c_maps_to_quit() {
    let event = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    assert_eq!(map_key(Mode::Playing, event), Some(Action::Quit));
}

#[test]
fn enter_and_e_differ_by_mode() {
    assert_eq!(
        map_key(Mode::MainMenu, key(KeyCode::Enter)),
        Some(Action::Confirm)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Enter)),
        Some(Action::Interact)
    );
    assert_eq!(
        map_key(Mode::Playing, key(KeyCode::Char('e'))),
        Some(Action::Interact)
    );
    assert_eq!(
        map_key(Mode::Paused, key(KeyCode::Char('E'))),
        Some(Action::Confirm)
    );
}

#[test]
fn coalesce_keeps_last_movement_and_other_actions() {
    let mut actions = vec![
        Action::MoveNorth,
        Action::Attack,
        Action::MoveEast,
        Action::MoveSouth,
    ];
    coalesce(&mut actions);
    assert_eq!(actions, vec![Action::Attack, Action::MoveSouth]);
}

#[test]
fn burst_of_200_moves_yields_one_action() {
    let raw: Vec<Action> = std::iter::repeat_n(Action::MoveNorth, 200).collect();
    let mut kept = apply_overflow_policy(&raw);
    coalesce(&mut kept);
    assert_eq!(kept, vec![Action::MoveNorth]);
}

/// Regression test (round-2 review): the overflow cap must keep the *freshest* movement, not the
/// 32nd-oldest one, so a burst that ends with a direction change actually applies that change.
#[test]
fn burst_ending_in_a_direction_change_yields_the_newest_movement() {
    let mut raw: Vec<Action> = std::iter::repeat_n(Action::MoveNorth, 200).collect();
    raw.push(Action::MoveSouth);
    let mut kept = apply_overflow_policy(&raw);
    coalesce(&mut kept);
    assert_eq!(kept, vec![Action::MoveSouth]);
}

#[test]
fn overflow_retains_up_to_eight_control_actions_queued_ahead_of_a_movement_burst() {
    // The 20 queued quits are pushed out of the retained "freshest 32" window by the movement
    // burst that follows them, so up to MAX_RETAINED_CONTROL_ACTIONS (8) must be rescued from the
    // discarded surplus rather than lost outright.
    let mut raw: Vec<Action> = std::iter::repeat_n(Action::Quit, 20).collect();
    raw.extend(std::iter::repeat_n(Action::MoveNorth, 40));
    let kept = apply_overflow_policy(&raw);
    assert_eq!(kept.iter().filter(|a| **a == Action::Quit).count(), 8);
    assert_eq!(kept.iter().filter(|a| **a == Action::MoveNorth).count(), 32);
}

#[test]
fn drop_pending_clears_buffered_actions() {
    let mut actions = vec![Action::MoveNorth, Action::Attack];
    drop_pending(&mut actions);
    assert!(actions.is_empty());
}

/// An in-memory queue standing in for the real crossterm event source, so the "drains to
/// exhaustion" property is provable without a PTY.
struct FakeSource(VecDeque<Event>);

impl EventSource for FakeSource {
    fn next_ready(&mut self) -> io::Result<Option<Event>> {
        Ok(self.0.pop_front())
    }
}

#[test]
fn drain_ready_empties_a_500_event_burst_in_one_call_and_yields_one_step() {
    let events: VecDeque<Event> = std::iter::repeat_n(Event::Key(key(KeyCode::Up)), 500).collect();
    let mut source = FakeSource(events);

    let drained = drain_ready(&mut source, 4096).unwrap();
    assert_eq!(drained.events.len(), 500);
    assert_eq!(drained.discarded, 0);
    assert!(
        source.0.is_empty(),
        "nothing may be left queued for a later iteration to replay as stale input"
    );

    let mapped: Vec<Action> = drained
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Key(k) => map_key(Mode::Playing, *k),
            _ => None,
        })
        .collect();
    let mut kept = apply_overflow_policy(&mapped);
    coalesce(&mut kept);
    assert_eq!(kept, vec![Action::MoveNorth]);
}

#[test]
fn drain_ready_still_empties_the_source_past_the_hard_cap() {
    let events: VecDeque<Event> = std::iter::repeat_n(Event::Key(key(KeyCode::Up)), 10).collect();
    let mut source = FakeSource(events);

    let drained = drain_ready(&mut source, 3).unwrap();
    assert_eq!(drained.events.len(), 3);
    assert_eq!(drained.discarded, 7);
    assert!(source.0.is_empty());
}

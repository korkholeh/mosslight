//! Dialogue as a simulation modal (spec §4, §5, §13): node order and flag application, `Cancel`
//! closing early, an unsatisfied condition never starting one, and `update()`'s dialogue branch
//! genuinely suspending the six-step per-tick pipeline.

mod common;

use mosslight::game::{Action, Facing, GameEvent, Pos};

use common::Runner;

/// Faces the hero at `pos` towards `facing` without a wasted walk: every fixture here starts
/// already adjacent to its NPC (avoids re-deriving a path just to turn around).
fn at_npc(room_id: &str, pos: Pos, facing: Facing) -> Runner {
    let mut runner = Runner::new(1);
    let room = runner
        .state
        .world
        .room_idx(room_id)
        .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
    runner.state.room = room;
    runner.state.hero.pos = pos;
    runner.state.enter_room();
    runner.face(facing);
    runner
}

#[test]
fn interacting_with_a_dialogue_npc_opens_it_and_applies_node_zeros_flag() {
    // npc.keeper's first node carries no flag; only the second does (see PLAN.md's content
    // table) — a plain node without `sets_flag` must not emit `FlagSet`.
    let mut runner = at_npc("room.lighthouse", Pos { x: 5, y: 6 }, Facing::North);
    let events = runner.step(&[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueStarted { .. })),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::FlagSet { .. })),
        "node 0 sets no flag: {events:?}"
    );
    assert!(runner.state.dialogue.is_some());
}

#[test]
fn dialogue_advances_in_node_order_and_the_named_node_sets_its_flag() {
    let mut runner = at_npc("room.lighthouse", Pos { x: 5, y: 6 }, Facing::North);
    runner.step(&[Action::Interact]);
    let node0 = runner.state.dialogue.expect("dialogue open").node;
    assert_eq!(node0, 0);

    let events = runner.step(&[Action::Confirm]);
    let node1 = runner.state.dialogue.expect("still open at node 1").node;
    assert_eq!(node1, 1, "must advance one node at a time, in order");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueAdvanced { node: 1, .. })),
        "{events:?}"
    );
    assert!(
        events.iter().any(
            |e| matches!(e, GameEvent::FlagSet { flag } if flag == "flag.told_about_sanctuary")
        ),
        "node 1 sets flag.told_about_sanctuary: {events:?}"
    );
    assert!(runner
        .state
        .progress
        .flags
        .contains("flag.told_about_sanctuary"));

    // Past the last node: closes instead of advancing further.
    let events = runner.step(&[Action::Confirm]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueEnded { .. })),
        "{events:?}"
    );
    assert!(runner.state.dialogue.is_none());
}

#[test]
fn cancel_closes_a_dialogue_at_the_first_node() {
    let mut runner = at_npc("room.lighthouse", Pos { x: 5, y: 6 }, Facing::North);
    runner.step(&[Action::Interact]);
    assert!(runner.state.dialogue.is_some());

    let events = runner.step(&[Action::Cancel]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueEnded { .. })),
        "{events:?}"
    );
    assert!(runner.state.dialogue.is_none());
    // Cancelling early must not apply node 1's flag — the dialogue never advanced to it.
    assert!(!runner
        .state
        .progress
        .flags
        .contains("flag.told_about_sanctuary"));
}

#[test]
fn an_npc_with_an_unsatisfied_condition_never_starts_a_dialogue() {
    let mut runner = at_npc("room.east_marsh", Pos { x: 17, y: 5 }, Facing::East);
    assert!(!runner
        .state
        .progress
        .flags
        .contains("flag.told_about_sanctuary"));

    let events = runner.step(&[Action::Interact]);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueStarted { .. })),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m == "They have nothing to say yet.")),
        "{events:?}"
    );
    assert!(runner.state.dialogue.is_none());
}

#[test]
fn satisfying_the_condition_lets_the_npc_talk() {
    let mut runner = at_npc("room.east_marsh", Pos { x: 17, y: 5 }, Facing::East);
    runner
        .state
        .progress
        .flags
        .insert("flag.told_about_sanctuary".to_string());

    let events = runner.step(&[Action::Interact]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::DialogueStarted { .. })),
        "{events:?}"
    );
    assert!(runner.state.dialogue.is_some());
}

#[test]
fn nothing_moves_while_a_dialogue_is_open() {
    // Two bats share stone_circle with npc.forester (see the world's content table); an
    // uninterrupted dialogue must not let them move. `update()`'s dialogue branch returns before
    // the six-step pipeline runs at all (see `game::state::update`), so every field but `tick`
    // itself (which `Runner` always advances by construction, one per `step()` call — a property
    // of this test driver, not of dialogue) must stay exactly as it was.
    let mut runner = at_npc("room.stone_circle", Pos { x: 5, y: 6 }, Facing::North);
    runner.step(&[Action::Interact]);
    assert!(runner.state.dialogue.is_some());

    let tick_before = runner.state.tick;
    let hero_before = runner.state.hero.clone();
    let enemies_before = runner.state.enemies.clone();
    let rng_before = runner.state.rng.clone();
    let progress_before = runner.state.progress.clone();
    let dialogue_before = runner.state.dialogue;

    // Feed a burst of ordinary actions while the dialogue is open: none of them should reach the
    // simulation pipeline.
    for action in [
        Action::MoveNorth,
        Action::MoveEast,
        Action::Attack,
        Action::UseLantern,
    ] {
        runner.step(&[action]);
    }

    assert_eq!(runner.state.tick, tick_before + 4, "tick still advances");
    assert_eq!(runner.state.hero, hero_before, "the hero must not move");
    assert_eq!(
        runner.state.enemies, enemies_before,
        "enemies must not move"
    );
    assert_eq!(runner.state.rng, rng_before, "no AI step means no RNG draw");
    assert_eq!(runner.state.progress, progress_before);
    assert_eq!(runner.state.dialogue, dialogue_before);
}

#[test]
fn closing_a_dialogue_drops_a_queued_movement_action_from_the_same_batch() {
    let mut runner = at_npc("room.lighthouse", Pos { x: 5, y: 6 }, Facing::North);
    runner.step(&[Action::Interact]);
    runner.step(&[Action::Confirm]); // -> node 1

    let pos_before = runner.state.hero.pos;
    // `update()`'s dialogue branch re-checks `state.dialogue` before each action in the slice and
    // stops the moment it is `None` (see `game::state::update`), so once this `Confirm` closes the
    // dialogue, the `MoveNorth` queued right behind it in the same call never reaches the
    // movement pipeline at all — this is the pure-`update()` half of the same rule
    // `tests/mode_machine.rs` proves at the `App`/mode level.
    let events = runner.step(&[Action::Confirm, Action::MoveNorth]);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::DialogueEnded { .. })));
    assert_eq!(
        runner.state.hero.pos, pos_before,
        "a move queued behind the closing Confirm must not land this tick"
    );
}

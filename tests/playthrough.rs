//! The headless start-to-victory run (spec §13, §15): New Game to `GameWon` using only ordinary
//! `Action`s — no test ever writes a `Progress` field directly. The route walker itself lives in
//! `tests/common/route.rs` (phase 7), shared with `tests/balance.rs`.

use std::rc::Rc;

use mosslight::game::{GameEvent, Reward, RoomIdx, Tick, World};

mod common;
use common::route::play_to_victory;
use common::Runner;

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

fn room_idx(world: &World, id: &str) -> RoomIdx {
    world
        .room_idx(id)
        .unwrap_or_else(|| panic!("{id} exists in the embedded world"))
}

#[test]
fn new_game_reaches_game_won_using_only_actions() {
    let world = world();
    let mut runner = Runner::new(1);
    let outcome = play_to_victory(&mut runner);
    let log = outcome.events;

    // --- The milestone order (spec §13/§15), verified from the actual event stream rather than
    // trusted from the script's own structure.
    let index_of = |pred: &dyn Fn(&GameEvent) -> bool| -> usize {
        log.iter()
            .position(pred)
            .unwrap_or_else(|| panic!("milestone event never occurred"))
    };
    let plate_chamber = room_idx(&world, "room.plate_chamber");
    let torch_vault = room_idx(&world, "room.torch_vault");
    let sanctuary_gate = room_idx(&world, "room.sanctuary_gate");

    let mut key_picks = log.iter().enumerate().filter(|(_, e)| {
        matches!(
            e,
            GameEvent::ItemPicked {
                reward: Reward::SmallKey
            }
        )
    });
    let first_key = key_picks.next().expect("two keys must be picked up").0;
    let second_key = key_picks.next().expect("two keys must be picked up").0;

    let milestones = [
        index_of(
            &|e| matches!(e, GameEvent::FlagSet { flag } if flag == "flag.told_about_sanctuary"),
        ),
        index_of(&|e| {
            matches!(
                e,
                GameEvent::ItemPicked {
                    reward: Reward::Sword
                }
            )
        }),
        index_of(&|e| {
            matches!(
                e,
                GameEvent::ItemPicked {
                    reward: Reward::Lantern
                }
            )
        }),
        index_of(&|e| matches!(e, GameEvent::RoomEntered { room, .. } if *room == sanctuary_gate)),
        first_key,
        index_of(
            &|e| matches!(e, GameEvent::PuzzleSolved { puzzle } if puzzle.room == plate_chamber),
        ),
        index_of(
            &|e| matches!(e, GameEvent::PuzzleSolved { puzzle } if puzzle.room == torch_vault),
        ),
        second_key,
        index_of(&|e| matches!(e, GameEvent::BossPhaseChanged { phase: 2, .. })),
        index_of(&|e| {
            matches!(
                e,
                GameEvent::ItemPicked {
                    reward: Reward::Ember
                }
            )
        }),
        index_of(&|e| matches!(e, GameEvent::GameWon)),
    ];

    for pair in milestones.windows(2) {
        assert!(
            pair[0] < pair[1],
            "milestones out of order: event {} did not precede event {} ({milestones:?})",
            pair[0],
            pair[1]
        );
    }
}

/// A tick ceiling well under spec §15's 30-45 minute target playthrough (30 Hz * 45 min ~=
/// 81,000 ticks) — this scripted, no-dawdling run should finish in a small fraction of that.
#[test]
fn the_run_finishes_inside_a_generous_tick_ceiling() {
    let mut runner = Runner::new(1);
    let outcome = play_to_victory(&mut runner);

    assert!(outcome
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::GameWon)));
    let ceiling: Tick = 30 * 60 * 40; // 40 simulated minutes at 30 Hz
    assert!(
        runner.state.tick < ceiling,
        "run took {} ticks, expected well under the {ceiling}-tick ceiling",
        runner.state.tick
    );
}

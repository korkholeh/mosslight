//! Turns the headless optimal route into the §1 30-45 minute target-band assertion (phase 7):
//! `src/game/balance.rs`'s itemised estimator, fed the tick count the perfect route actually
//! took, must land inside `TARGET_MIN_TICKS..=TARGET_MAX_TICKS`.

use std::rc::Rc;

use mosslight::game::{
    engage_ticks, estimate_first_playthrough, EnemyKind, World, TARGET_MAX_TICKS, TARGET_MIN_TICKS,
};

mod common;
use common::route::play_to_victory;
use common::Runner;

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

#[test]
fn estimated_first_playthrough_lands_in_the_target_band() {
    let world = world();
    let mut runner = Runner::new(1);
    let outcome = play_to_victory(&mut runner);

    let estimate = estimate_first_playthrough(&world, outcome.ticks);

    assert!(
        estimate.total_ticks >= TARGET_MIN_TICKS && estimate.total_ticks <= TARGET_MAX_TICKS,
        "estimated playthrough is {} min ({} ticks), outside the {}-{} min target band \
         (optimal route measured at {} ticks). Breakdown: {estimate:#?}",
        estimate.total_minutes(),
        estimate.total_ticks,
        TARGET_MIN_TICKS / 60 / 30,
        TARGET_MAX_TICKS / 60 / 30,
        outcome.ticks,
    );
}

/// A perfect run must stay a perfect run: catches a tuning change that inflates travel (e.g. a
/// slower hero step) without anyone noticing, since the estimator only ever scales this number up.
#[test]
fn the_optimal_route_itself_stays_under_three_simulated_minutes() {
    let mut runner = Runner::new(1);
    let outcome = play_to_victory(&mut runner);

    let ceiling = 3 * 60 * 30; // 3 simulated minutes at 30 Hz
    assert!(
        outcome.ticks < ceiling,
        "optimal route took {} ticks, expected well under the {ceiling}-tick ceiling",
        outcome.ticks
    );
}

/// The estimate's combat term actually responds to content: it must equal the sum of
/// `engage_ticks` over every authored regular (non-boss) spawn, so deleting or adding an enemy
/// moves the total.
#[test]
fn the_combat_term_sums_engage_ticks_over_authored_spawns() {
    let world = world();
    let mut runner = Runner::new(1);
    let outcome = play_to_victory(&mut runner);
    let estimate = estimate_first_playthrough(&world, outcome.ticks);

    let expected: u64 = world
        .rooms
        .iter()
        .flat_map(|r| r.enemies.iter())
        .filter(|spawn| spawn.kind != EnemyKind::Boss)
        .map(|spawn| engage_ticks(spawn.kind))
        .sum();

    assert_eq!(estimate.combat_ticks, expected);
    assert!(expected > 0, "the embedded world authors regular enemies");
}

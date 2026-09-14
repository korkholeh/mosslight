//! Terminal output volume (spec §13, RISKS #12): drives `tests/common/metrics::Harness` — the
//! real `CrosstermBackend` write path — over simulated wall time and counts bytes. `--nocapture`
//! prints the numbers copied into `docs/dev/verification-report.md`.

use mosslight::app::Mode;
use mosslight::game::Action;

mod common;
use common::metrics::Harness;

const TICK_NS: u64 = 1_000_000_000 / 30;

/// Cycles through a small, non-trivial action stream so the hero (and the room's live enemies)
/// keep moving — the worst case for output volume, not the best.
fn scripted_action(i: usize) -> Action {
    match i % 4 {
        0 => Action::MoveNorth,
        1 => Action::MoveEast,
        2 => Action::MoveSouth,
        _ => Action::MoveWest,
    }
}

#[test]
fn active_play_stays_inside_the_output_budget() {
    let mut h = Harness::new((80, 24), 20, 1);
    h.iterate(TICK_NS, &[Action::Confirm]); // MainMenu -> Playing
    h.enter_room("room.stone_circle"); // has live Bat spawns (spec §6)

    let simulated_seconds = 60u32;
    let iterations = simulated_seconds * 30; // one tick per iteration
    for i in 0..iterations {
        h.iterate(TICK_NS, &[scripted_action(i as usize)]);
    }

    let bytes = h.bytes();
    let bytes_per_min = bytes as u64 * 60 / u64::from(simulated_seconds);
    println!(
        "active play: {bytes} bytes over {simulated_seconds}s => {bytes_per_min} bytes/min \
         ({:.1} KB/min)",
        bytes_per_min as f64 / 1024.0
    );
    assert!(
        bytes < 200_000,
        "active play wrote {bytes} bytes in {simulated_seconds}s, expected under the 200 KB/min \
         budget (ARCHITECTURE's assumed non-functional number)"
    );
}

/// One frame interval's worth of elapsed time, so an `iterate` call is guaranteed to cross the
/// pacer's draw threshold (a plain `TICK_NS` step is not enough at every fps — e.g. two 30Hz tick
/// intervals are still short of a 20fps frame interval) and actually flush a frame if one is due.
fn frame_ns(fps: u32) -> u64 {
    1_000_000_000 / u64::from(fps)
}

#[test]
fn a_paused_game_with_no_input_writes_nothing_after_the_first_frame() {
    let fps = 20;
    let mut h = Harness::new((80, 24), fps, 1);
    h.iterate(frame_ns(fps), &[Action::Confirm]); // MainMenu -> Playing, draws once
    h.iterate(frame_ns(fps), &[Action::Cancel]); // Playing -> Paused, draws once
    assert_eq!(
        h.mode(),
        Mode::Paused,
        "Cancel from Playing must reach Mode::Paused, or this test proves nothing about pausing"
    );
    let baseline = h.bytes();
    assert!(
        baseline > 0,
        "the flush frames above must have drawn something"
    );

    for _ in 0..(60 * 20) {
        h.iterate(TICK_NS, &[]);
    }

    assert_eq!(
        h.bytes(),
        baseline,
        "a paused game with no input must write zero bytes after its first frame"
    );
}

#[test]
fn a_static_main_menu_writes_nothing_after_the_first_frame() {
    let fps = 20;
    let mut h = Harness::new((80, 24), fps, 1);
    h.iterate(frame_ns(fps), &[]); // draws the main menu once
    let baseline = h.bytes();
    assert!(
        baseline > 0,
        "the flush frame above must have drawn something"
    );

    for _ in 0..(60 * 20) {
        h.iterate(TICK_NS, &[]);
    }

    assert_eq!(
        h.bytes(),
        baseline,
        "a static main menu with no input must write zero bytes after its first frame"
    );
}

#[test]
fn output_scales_with_fps_not_with_simulation() {
    let run = |fps: u32| -> (usize, u64) {
        let mut h = Harness::new((80, 24), fps, 1);
        h.iterate(TICK_NS, &[Action::Confirm]);
        h.enter_room("room.stone_circle");
        for i in 0..(60 * 30u32) {
            h.iterate(TICK_NS, &[scripted_action(i as usize)]);
        }
        (h.bytes(), h.ticks())
    };

    let (bytes10, ticks10) = run(10);
    let (bytes30, ticks30) = run(30);

    assert_eq!(
        ticks10, ticks30,
        "the same elapsed wall time must simulate the same tick count regardless of --fps"
    );
    assert!(
        bytes30 > bytes10,
        "30 fps ({bytes30} bytes) should write more than 10 fps ({bytes10} bytes) over the same \
         simulated time"
    );
}

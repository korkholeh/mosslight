//! Fixed 30 Hz simulation independent of `--fps` (spec §9); the pacer's catch-up cap and its
//! interaction with pause/dirty state.

use std::path::PathBuf;

use mosslight::app::{advance_iteration, App, Pacer};
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::{tuning, Action};

const NANOS_PER_SEC: u64 = 1_000_000_000;

fn cfg(fps: Fps) -> Config {
    Config {
        glyphs: GlyphSet::Ascii,
        color: ColorMode::Never,
        theme: ThemeName::Gameboy,
        fps,
        save_dir: PathBuf::from("/tmp"),
        seed: 7,
        debug_panic: false,
    }
}

/// Drives an `App` through a fixed action schedule at a given `--fps`, ticking the simulation at
/// the fixed 30 Hz rate regardless of the render fps, and returns the final serialized state.
fn run_schedule(fps: Fps) -> Vec<u8> {
    let mut app = App::new(&cfg(fps));
    app.apply(&[Action::Confirm]); // -> Playing

    let mut pacer = Pacer::new(fps.as_u32());
    let tick_ns = NANOS_PER_SEC / tuning::TICK_HZ;
    let schedule = [
        Action::MoveSouth,
        Action::MoveSouth,
        Action::MoveEast,
        Action::MoveEast,
        Action::MoveNorth,
    ];

    for action in schedule {
        let sim_actions = app.apply(&[action]);
        let due = pacer.advance(tick_ns);
        for step in 0..due.sim_steps {
            let step_actions: &[Action] = if step == 0 { &sim_actions } else { &[] };
            app.tick(step_actions);
        }
    }

    serde_json::to_vec(&app.state).expect("serialize state")
}

#[test]
fn state_is_identical_at_every_fps() {
    let f10 = run_schedule(Fps::F10);
    let f20 = run_schedule(Fps::F20);
    let f30 = run_schedule(Fps::F30);
    assert_eq!(f10, f20);
    assert_eq!(f20, f30);
}

#[test]
fn a_long_stall_yields_at_most_five_steps_and_discards_the_surplus() {
    let mut pacer = Pacer::new(30);
    let due = pacer.advance(900_000_000); // 900ms stall
    assert_eq!(due.sim_steps, tuning::MAX_CATCHUP_STEPS);
    let due_next = pacer.advance(1);
    assert_eq!(
        due_next.sim_steps, 0,
        "surplus must not accelerate the next iteration"
    );
}

/// Round-2 review blocker: a keypress delivered in an iteration whose elapsed time does not cross
/// a tick boundary (`due.sim_steps == 0`, the normal outcome when a key wakes `event::poll` early)
/// must survive into the following iteration instead of being dropped, or the hero is effectively
/// unable to walk. Drives `advance_iteration` — the real loop's timing-dependent wiring, not just
/// `App`/`Pacer` in isolation — through a sub-tick-then-remainder pair of iterations.
#[test]
fn a_keypress_delivered_when_no_sim_step_is_due_is_not_lost() {
    let mut app = App::new(&cfg(Fps::F20));
    app.apply(&[Action::Confirm]); // -> Playing
    let start = app.state.hero.pos;

    let mut pacer = Pacer::new(20);
    let mut pending = Vec::new();
    let tick_ns = NANOS_PER_SEC / tuning::TICK_HZ;

    // Iteration 1: half a tick elapses with a MoveSouth key pressed. Not enough time has passed
    // to cross a tick boundary, so no simulation step runs yet.
    let due1 = advance_iteration(
        &mut app,
        &mut pacer,
        &mut pending,
        &[Action::MoveSouth],
        tick_ns / 2,
    );
    assert_eq!(due1.sim_steps, 0);
    assert_eq!(
        app.state.hero.pos, start,
        "no simulation step is due yet, so the hero must not have moved"
    );

    // Iteration 2: the rest of the tick elapses with no new key pressed. The buffered action from
    // iteration 1 must still be applied here.
    let due2 = advance_iteration(
        &mut app,
        &mut pacer,
        &mut pending,
        &[],
        tick_ns - tick_ns / 2,
    );
    assert_eq!(due2.sim_steps, 1);
    assert_ne!(
        app.state.hero.pos, start,
        "the keypress buffered from iteration 1 must not have been dropped"
    );
    assert!(
        pending.is_empty(),
        "pending actions must clear once a step consumes them"
    );
}

/// Holds a movement key across many sub-tick iterations and checks that the hero still advances
/// only at the `HERO_STEP_TICKS` cooldown rate — i.e. the loop-wiring extraction in
/// `advance_iteration` does not let a buffered action replay on more than one step.
#[test]
fn a_held_movement_key_advances_the_hero_at_the_step_cooldown_rate_across_iterations() {
    let mut app = App::new(&cfg(Fps::F30));
    app.apply(&[Action::Confirm]); // -> Playing

    let mut pacer = Pacer::new(30);
    let mut pending = Vec::new();
    let quarter_tick_ns = (NANOS_PER_SEC / tuning::TICK_HZ) / 4;

    let mut step_ticks = Vec::new();
    let mut last_pos = app.state.hero.pos;
    for _ in 0..(4 * tuning::HERO_STEP_TICKS * 4) {
        advance_iteration(
            &mut app,
            &mut pacer,
            &mut pending,
            &[Action::MoveSouth],
            quarter_tick_ns,
        );
        if app.state.hero.pos != last_pos {
            step_ticks.push(app.state.tick);
            last_pos = app.state.hero.pos;
        }
    }

    assert!(
        step_ticks.len() >= 2,
        "expected several steps over this span, got {step_ticks:?}"
    );
    for pair in step_ticks.windows(2) {
        assert!(
            pair[1] - pair[0] >= tuning::HERO_STEP_TICKS,
            "steps must be spaced by at least HERO_STEP_TICKS: {step_ticks:?}"
        );
    }
}

/// Round-2 review minor finding: `App::tick` used to mark every `Playing` tick dirty even when
/// nothing changed, so a frame was written on every fps interval forever. Runs `Playing` with no
/// actions (the hero is already at rest) for 100 iterations and asserts nothing is ever drawn.
#[test]
fn no_draw_is_due_while_playing_idle_with_no_actions() {
    let mut app = App::new(&cfg(Fps::F30));
    app.apply(&[Action::Confirm]); // -> Playing
    app.take_dirty(); // drain the mode-transition dirty bit

    let mut pacer = Pacer::new(30);
    let mut pending = Vec::new();
    let tick_ns = NANOS_PER_SEC / tuning::TICK_HZ;
    let mut spurious_draws = 0;
    for _ in 0..100 {
        let due = advance_iteration(&mut app, &mut pacer, &mut pending, &[], tick_ns);
        if due.draw && app.take_dirty() {
            spurious_draws += 1;
        }
    }
    assert_eq!(spurious_draws, 0);
}

#[test]
fn no_draw_is_due_while_idle_with_nothing_dirty() {
    let mut app = App::new(&cfg(Fps::F10));
    // Stays in MainMenu; take_dirty() is drained once, then nothing changes for 100 iterations.
    app.take_dirty();

    let mut pacer = Pacer::new(10);
    let tick_ns = NANOS_PER_SEC / tuning::TICK_HZ;
    let mut spurious_draws = 0;
    for _ in 0..100 {
        let due = pacer.advance(tick_ns);
        for _ in 0..due.sim_steps {
            app.tick(&[]);
        }
        if due.draw && app.take_dirty() {
            spurious_draws += 1;
        }
    }
    assert_eq!(spurious_draws, 0);
}

/// Drives one simulated second (`TICK_HZ` iterations, each advancing by exactly one tick
/// interval — the real loop's poll deadline is tick-based, not fps-based, so this is the actual
/// iteration cadence) and returns how many iterations were due to draw and how many simulation
/// ticks ran in total.
fn drive_one_second(fps: Fps) -> (u32, u32) {
    let mut pacer = Pacer::new(fps.as_u32());
    let tick_ns = NANOS_PER_SEC / tuning::TICK_HZ;
    let mut draws = 0u32;
    let mut sim_ticks = 0u32;
    for _ in 0..tuning::TICK_HZ {
        let due = pacer.advance(tick_ns);
        draws += u32::from(due.draw);
        sim_ticks += due.sim_steps;
    }
    (draws, sim_ticks)
}

#[test]
fn draw_cadence_scales_with_fps_while_simulation_tick_count_does_not() {
    let (draws10, ticks10) = drive_one_second(Fps::F10);
    let (draws20, ticks20) = drive_one_second(Fps::F20);
    let (draws30, ticks30) = drive_one_second(Fps::F30);

    // `Due.draw` is exercised here (unlike `run_schedule`, which never reads it): the same one
    // second of simulated time is chopped into progressively more draws as fps rises...
    assert!(draws10 <= 10);
    assert!(draws20 <= 20);
    assert!(draws30 <= 30);
    assert!(
        draws10 < draws30,
        "expected more draws at 30fps than 10fps over the same second: {draws10} vs {draws30}"
    );

    // ...while the simulation itself always ran the full 30 ticks regardless, proving fps has no
    // bearing on `Pacer`'s tick accounting (only on `frame_acc_ns`/`due.draw`).
    assert_eq!(ticks10, tuning::TICK_HZ as u32);
    assert_eq!(ticks10, ticks20);
    assert_eq!(ticks20, ticks30);
}

#[test]
fn draws_per_second_never_exceed_configured_fps() {
    let mut pacer = Pacer::new(10);
    let one_second = NANOS_PER_SEC;
    let steps = 1000;
    let mut draws = 0;
    for _ in 0..steps {
        if pacer.advance(one_second / steps).draw {
            draws += 1;
        }
    }
    assert!(draws <= 10, "draws = {draws}, expected at most 10");
}

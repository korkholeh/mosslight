//! Mode transitions and pause semantics (spec §5, §9, §12).

use std::path::PathBuf;

use mosslight::app::{App, ExitReason, Mode};
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::Action;

fn cfg() -> Config {
    Config {
        glyphs: GlyphSet::Ascii,
        color: ColorMode::Never,
        theme: ThemeName::Gameboy,
        fps: Fps::F20,
        save_dir: PathBuf::from("/tmp"),
        seed: 1,
        debug_panic: false,
    }
}

#[test]
fn new_game_enters_playing_at_spawn() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Confirm]);
    assert_eq!(app.mode, Mode::Playing);
    assert_eq!(app.state.hero.pos, app.state.room.spawn);
}

#[test]
fn esc_pauses_and_stops_simulation() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Confirm]);
    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::Paused);
    assert!(!app.simulating());
}

#[test]
fn esc_resumes_and_drops_a_trailing_move_from_the_same_batch() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Confirm]);
    let sim_actions = app.apply(&[Action::MoveNorth]);
    assert_eq!(sim_actions, vec![Action::MoveNorth]);

    app.apply(&[Action::Cancel]);
    // Cancel resumes Playing; a movement key queued right behind it in the same batch must not
    // leak into gameplay this iteration (spec §5).
    let sim_actions = app.apply(&[Action::Cancel, Action::MoveNorth]);
    assert_eq!(app.mode, Mode::Playing);
    assert!(sim_actions.is_empty());
}

#[test]
fn quit_requires_confirmation() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Quit]);
    assert_eq!(app.mode, Mode::ConfirmQuit);
    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::MainMenu);
    assert!(app.quit.is_none());

    app.apply(&[Action::Quit, Action::Confirm]);
    assert_eq!(app.quit, Some(ExitReason::Confirmed));
}

#[test]
fn resize_too_small_stops_ticks_then_recovery_enters_paused() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Confirm]);
    let before = app.state.tick;
    app.on_resize(59, 24);
    assert_eq!(app.mode, Mode::TooSmall);
    app.tick(&[]);
    assert_eq!(app.state.tick, before);

    app.on_resize(80, 24);
    assert_eq!(app.mode, Mode::Paused);
}

#[test]
fn quit_in_too_small_mode_quits_immediately_with_no_confirmation() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Confirm]); // -> Playing
    app.on_resize(40, 15);
    assert_eq!(app.mode, Mode::TooSmall);

    app.apply(&[Action::Quit]);
    assert_eq!(
        app.quit,
        Some(ExitReason::Confirmed),
        "a confirm-quit dialog cannot render at this size, and Ctrl+C/Q must still work"
    );
}

#[test]
fn help_returns_to_previous_mode() {
    let mut app = App::new(&cfg());
    app.apply(&[Action::Help]);
    assert_eq!(app.mode, Mode::Help);
    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::MainMenu);

    app.apply(&[Action::Confirm]);
    app.apply(&[Action::Help]);
    assert_eq!(app.mode, Mode::Help);
    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::Playing);
}

//! Mode transitions and pause semantics (spec §5, §9, §12).

use std::path::PathBuf;
use std::rc::Rc;

use mosslight::app::{App, ExitReason, Mode};
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::{Action, Pos, World};
use mosslight::save::MemorySaveIo;

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

fn cfg() -> Config {
    Config {
        glyphs: GlyphSet::Ascii,
        color: ColorMode::Never,
        theme: ThemeName::Gameboy,
        fps: Fps::F20,
        save_dir: PathBuf::from("/tmp"),
        seed: 1,
        debug_panic: false,
        debug_content: None,
    }
}

#[test]
fn new_game_enters_playing_at_spawn() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    assert_eq!(app.mode, Mode::Playing);
    let expected = app
        .state
        .world
        .spawn_pos(app.state.room, "spawn.lighthouse.start")
        .expect("the world's start spawn must resolve");
    assert_eq!(app.state.hero.pos, expected);
}

#[test]
fn esc_pauses_and_stops_simulation() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::Paused);
    assert!(!app.simulating());
}

#[test]
fn esc_resumes_and_drops_a_trailing_move_from_the_same_batch() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
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
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
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
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
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
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
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
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
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

/// Phase 6 repoints death/retry at the save file (see DECISIONS.md): retry now restores the last
/// *autosave*, not an in-memory checkpoint, so this drives a real save (a manual one, from
/// `Paused`) before killing the hero, and inspects the shared `MemorySaveIo` handle to prove retry
/// reads it without ever writing to it.
#[test]
fn death_opens_game_over_and_retry_restores_the_last_autosave() {
    let io = MemorySaveIo::new();
    let mut app = App::new(&cfg(), world(), Box::new(io.clone()));
    app.apply(&[Action::Confirm]); // -> Playing
    let saved_pos = app.state.hero.pos;

    app.apply(&[Action::Cancel]); // -> Paused
    app.apply(&[Action::Confirm]); // manual save at full health
    assert_eq!(io.store_count(), 1);
    app.apply(&[Action::Cancel]); // -> Playing

    app.state.hero.pos = Pos { x: 5, y: 5 };
    app.state.hero.health_halves = 0;
    app.tick(&[]); // tick 1: HeroDied
    assert_eq!(app.mode, Mode::GameOver);
    assert!(!app.simulating(), "the simulation must not run in GameOver");

    let tick_at_death = app.state.tick;
    app.tick(&[]); // must be a no-op: GameOver freezes the simulation
    assert_eq!(app.state.tick, tick_at_death);

    app.apply(&[Action::Confirm]); // retry
    assert_eq!(app.mode, Mode::Playing);
    assert_eq!(app.state.hero.pos, saved_pos);
    assert_eq!(app.state.hero.health_halves, 6);
    assert_eq!(
        io.store_count(),
        1,
        "retry must only ever read the slot, never write it"
    );

    app.tick(&[]);
    assert_eq!(
        app.state.tick, 1,
        "tick_counter must rewind to the restored save's tick, not keep counting from tick_at_death"
    );
}

#[test]
fn retry_restores_the_autosaved_room_not_a_fresh_hero() {
    // Round-1 review, minor: the only prior coverage of criterion 8's "room-entry" half killed
    // the hero in the *start* room, so the `RoomEntered`-triggered autosave had no test, and a
    // save taken at less than full health was indistinguishable from a freshly reset hero (both
    // read `health_halves == 6`). Here the autosave is taken at a damaged, non-start room, so
    // "restored from the save" and "reset to a fresh hero" produce different, checkable outcomes.
    let io = MemorySaveIo::new();
    let mut app = App::new(&cfg(), world(), Box::new(io.clone()));
    app.apply(&[Action::Confirm]); // -> Playing
    let start_room = app.state.room;

    // room.lighthouse's north door sits at (12, 0); one tile south of it is walkable floor (see
    // `state::tests::stepping_onto_a_door_tile_...`).
    app.state.hero.pos = Pos { x: 12, y: 1 };
    app.state.hero.health_halves = 3;
    app.tick(&[Action::MoveNorth]); // crosses the door -> RoomEntered -> autosaves
    assert_ne!(
        app.state.room, start_room,
        "must have actually changed rooms"
    );
    assert_eq!(
        app.state.hero.health_halves, 3,
        "crossing a door costs no health"
    );
    assert_eq!(io.store_count(), 1, "the room transition must autosave");
    let room_after_entry = app.state.room;
    let pos_after_entry = app.state.hero.pos;

    app.state.hero.health_halves = 0;
    app.tick(&[]); // HeroDied
    assert_eq!(app.mode, Mode::GameOver);
    assert_eq!(
        io.store_count(),
        1,
        "a death state must never be written over the last usable save"
    );

    app.apply(&[Action::Confirm]); // retry
    assert_eq!(app.mode, Mode::Playing);
    assert_eq!(
        app.state.room, room_after_entry,
        "retry must land in the room the hero actually entered, not the start room"
    );
    assert_eq!(app.state.hero.pos, pos_after_entry);
    assert_eq!(
        app.state.hero.health_halves, 3,
        "retry must restore the post-entry health (3), not a fresh hero's full health (6)"
    );
    assert_eq!(
        io.store_count(),
        1,
        "retry must only ever read the slot, never write it"
    );
}

#[test]
fn retry_never_writes_a_save() {
    let io = MemorySaveIo::new();
    let mut app = App::new(&cfg(), world(), Box::new(io.clone()));
    app.apply(&[Action::Confirm]);
    app.state.hero.health_halves = 0;
    app.tick(&[]); // HeroDied, no save yet exists

    app.apply(&[Action::Confirm]); // retry with an Empty slot: starts a fresh run
    assert_eq!(app.mode, Mode::Playing);
    assert_eq!(
        io.store_count(),
        0,
        "retry from an empty slot must start a fresh run without ever writing a save"
    );
}

#[test]
fn game_over_cancel_returns_to_the_main_menu() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    app.state.hero.health_halves = 0;
    app.tick(&[]);
    assert_eq!(app.mode, Mode::GameOver);

    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::MainMenu);
}

#[test]
fn opening_the_map_freezes_the_tick() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    let sim_actions = app.apply(&[Action::ToggleMap]);
    assert_eq!(app.mode, Mode::Map);
    assert!(sim_actions.is_empty());

    let before = app.state.tick;
    app.tick(&[]);
    assert_eq!(before, app.state.tick, "Map must freeze the simulation");
}

#[test]
fn opening_the_inventory_freezes_the_tick() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    let sim_actions = app.apply(&[Action::ToggleInventory]);
    assert_eq!(app.mode, Mode::Inventory);
    assert!(sim_actions.is_empty());

    let before = app.state.tick;
    app.tick(&[]);
    assert_eq!(
        before, app.state.tick,
        "Inventory must freeze the simulation"
    );
}

#[test]
fn closing_an_overlay_drops_a_queued_move_from_the_same_batch() {
    for (open, close) in [
        (Action::ToggleMap, Action::ToggleMap),
        (Action::ToggleInventory, Action::ToggleInventory),
        (Action::ToggleMap, Action::Cancel),
    ] {
        let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
        app.apply(&[Action::Confirm]);
        app.apply(&[open]);
        let pos_before = app.state.hero.pos;

        let sim_actions = app.apply(&[close, Action::MoveNorth]);
        assert_eq!(app.mode, Mode::Playing);
        assert!(
            sim_actions.is_empty(),
            "a move queued behind {close:?} must not leak into gameplay this batch"
        );
        app.tick(&sim_actions);
        assert_eq!(app.state.hero.pos, pos_before);
    }
}

/// npc.keeper sits at (5, 5) in the start room (`assets/world.ron`); standing one tile south and
/// facing north is enough to `Interact` with it without walking there.
fn face_the_keeper(app: &mut App) {
    app.state.hero.pos = Pos { x: 5, y: 6 };
    app.state.hero.facing = mosslight::game::Facing::North;
}

#[test]
fn interacting_with_an_npc_opens_dialogue_mode_and_freezes_the_tick() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    face_the_keeper(&mut app);

    let sim_actions = app.apply(&[Action::Interact]);
    app.tick(&sim_actions);
    assert_eq!(app.mode, Mode::Dialogue);
    assert!(!app.simulating());

    let before = app.state.tick;
    app.tick(&[]);
    assert_eq!(
        before, app.state.tick,
        "Dialogue must freeze the simulation"
    );
}

/// `beacon.lighthouse` sits at (20, 5) in `room.lighthouse` — the start room, so no room
/// transition is needed to face it.
fn face_the_beacon(app: &mut App) {
    app.state.hero.pos = Pos { x: 20, y: 6 };
    app.state.hero.facing = mosslight::game::Facing::North;
}

#[test]
fn game_won_enters_victory_mode_and_stops_simulating() {
    // Round-1 review, major: T10's ending was only proved below `App` (the `GameWon` *event*),
    // never that `App` actually reacts to it — this and the test below are the promised
    // `tests/mode_machine.rs` coverage.
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    face_the_beacon(&mut app);
    app.state.hero.has_ember = true;

    let sim_actions = app.apply(&[Action::Interact]);
    app.tick(&sim_actions);
    assert_eq!(app.mode, Mode::Victory);
    assert!(!app.simulating(), "the simulation must not run in Victory");

    let before = app.state.tick;
    app.tick(&[]);
    app.tick(&[]);
    assert_eq!(before, app.state.tick, "Victory must freeze the simulation");
}

#[test]
fn victory_confirm_returns_to_the_main_menu() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    face_the_beacon(&mut app);
    app.state.hero.has_ember = true;
    let sim_actions = app.apply(&[Action::Interact]);
    app.tick(&sim_actions);
    assert_eq!(app.mode, Mode::Victory);

    app.apply(&[Action::Confirm]);
    assert_eq!(app.mode, Mode::MainMenu);

    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    face_the_beacon(&mut app);
    app.state.hero.has_ember = true;
    let sim_actions = app.apply(&[Action::Interact]);
    app.tick(&sim_actions);
    assert_eq!(app.mode, Mode::Victory);

    app.apply(&[Action::Cancel]);
    assert_eq!(app.mode, Mode::MainMenu);
}

#[test]
fn closing_a_dialogue_drops_a_queued_move_from_the_same_batch() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]);
    face_the_keeper(&mut app);
    let sim_actions = app.apply(&[Action::Interact]);
    app.tick(&sim_actions);
    assert_eq!(app.mode, Mode::Dialogue);

    let pos_before = app.state.hero.pos;
    let sim_actions = app.apply(&[Action::Cancel, Action::MoveNorth]);
    assert_eq!(app.mode, Mode::Playing);
    assert!(sim_actions.is_empty());
    app.tick(&sim_actions);
    assert_eq!(app.state.hero.pos, pos_before);
}

#[test]
fn main_menu_cursor_move_requests_a_redraw() {
    let mut app = App::new(&cfg(), world(), Box::new(MemorySaveIo::new()));
    // Clear the startup dirty bit the way a drawn frame does.
    assert!(app.take_dirty());

    let before = app.menu;
    app.apply(&[Action::MoveSouth]);
    assert_ne!(app.menu, before);
    assert!(
        app.take_dirty(),
        "moving the menu cursor must mark the frame dirty, or the selection never redraws"
    );

    app.apply(&[Action::MoveNorth]);
    assert_eq!(app.menu, before);
    assert!(app.take_dirty());
}

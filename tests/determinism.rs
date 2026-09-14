//! Determinism (spec §8, §13, ADR 0002): the same seed plus the same action sequence produces an
//! identical `state_hash()` at every tick, two different seeds diverge, and the hash sequence
//! does not depend on how ticks are batched across `--fps` values.

use std::path::PathBuf;
use std::rc::Rc;

use mosslight::app::{advance_iteration, App, Pacer};
use mosslight::config::{ColorMode, Config, Fps, GlyphSet, ThemeName};
use mosslight::game::rng::Rng;
use mosslight::game::{state_hash, tuning, update, Action, GameState, Pos, World};
use mosslight::save::MemorySaveIo;

const NANOS_PER_SEC: u64 = 1_000_000_000;
const SEQUENCE_LEN: usize = 2000;

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

/// A pseudo-random action-per-tick sequence, drawn from a test-local RNG independent of the
/// game's own `state.rng` — deterministic given `seed`, but not meant to resemble anything a
/// player would actually press.
fn action_sequence(seed: u64, len: usize) -> Vec<Action> {
    let choices = [
        Action::MoveNorth,
        Action::MoveEast,
        Action::MoveSouth,
        Action::MoveWest,
        Action::Attack,
    ];
    let mut rng = Rng::new(seed);
    (0..len)
        .map(|_| choices[rng.below(choices.len() as u32) as usize])
        .collect()
}

/// Replays `actions` one per tick against a fresh `GameState` seeded with `seed`, returning the
/// `state_hash()` after every tick.
fn hash_trace(seed: u64, actions: &[Action]) -> Vec<u64> {
    let mut state = GameState::new(seed, world());
    // Phase 4 gates `Attack` on `has_sword`; established directly so the action sequence's
    // `Attack` picks actually exercise the swing pipeline instead of silently no-oping.
    state.hero.has_sword = true;
    let mut hashes = Vec::with_capacity(actions.len());
    for (i, &action) in actions.iter().enumerate() {
        let tick = (i + 1) as u64;
        update(&mut state, &[action], tick);
        hashes.push(state_hash(&state));
    }
    hashes
}

#[test]
fn same_seed_same_actions_hash_identically_every_tick() {
    let actions = action_sequence(1234, SEQUENCE_LEN);
    let a = hash_trace(7, &actions);
    let b = hash_trace(7, &actions);
    assert_eq!(a, b);
}

/// Replays `actions` one per tick against a fresh `GameState` seeded with `seed`, starting in
/// `room.west_grove` (authored with one slime, see `assets/world.ron`) instead of the empty start
/// room — so the RNG-driven slime wander/aggro roll actually runs from tick 1. Returns every live
/// enemy's position and the `state_hash()` after each tick.
fn enemy_position_and_hash_trace(seed: u64, actions: &[Action]) -> (Vec<Vec<Pos>>, Vec<u64>) {
    let world = world();
    let room = world
        .room_idx("room.west_grove")
        .expect("room.west_grove exists in the embedded world");
    let spawn = world
        .spawn_pos(room, "spawn.west_grove.north")
        .expect("spawn.west_grove.north resolves");

    let mut state = GameState::new(seed, world);
    state.room = room;
    state.hero.pos = spawn;
    state.hero.has_sword = true;
    state.enter_room();

    let mut positions = Vec::with_capacity(actions.len());
    let mut hashes = Vec::with_capacity(actions.len());
    for (i, &action) in actions.iter().enumerate() {
        let tick = (i + 1) as u64;
        update(&mut state, &[action], tick);
        positions.push(state.enemies.iter().map(|e| e.pos).collect());
        hashes.push(state_hash(&state));
    }
    (positions, hashes)
}

#[test]
fn different_seeds_diverge() {
    // Round-1 review, minor: comparing full `state_hash()` is satisfied unconditionally, because
    // `state_hash` writes `rng.raw_state()` and `Rng::new(7)`/`Rng::new(8)` differ there from
    // construction — the assertion would pass even if the seed had zero influence on gameplay.
    // Comparing enemy positions in a room the RNG-driven slime actually occupies instead proves
    // the seed reaches gameplay, not just the hash. Round-2 review, minor: criterion 10 is worded
    // in terms of `state_hash()` differing, so keep that assertion too, alongside the stronger
    // position one, in case a future field is added to `Enemy` without being hashed.
    let actions = action_sequence(1234, SEQUENCE_LEN);
    let (positions_a, hashes_a) = enemy_position_and_hash_trace(7, &actions);
    let (positions_b, hashes_b) = enemy_position_and_hash_trace(8, &actions);
    assert!(
        positions_a
            .iter()
            .zip(positions_b.iter())
            .any(|(x, y)| x != y),
        "two different seeds over the same action sequence must move the slime differently somewhere"
    );
    assert!(
        hashes_a.iter().zip(hashes_b.iter()).any(|(x, y)| x != y),
        "two different seeds over the same action sequence must diverge in state_hash() somewhere"
    );
}

fn cfg(fps: Fps, seed: u64) -> Config {
    Config {
        glyphs: GlyphSet::Ascii,
        color: ColorMode::Never,
        theme: ThemeName::Gameboy,
        fps,
        save_dir: PathBuf::from("/tmp"),
        seed,
        debug_panic: false,
        debug_content: None,
    }
}

/// Drives an `App` through `actions` at a given `--fps`, ticking the fixed-30Hz simulation exactly
/// once per action (one elapsed tick per loop iteration, as `hash_trace` does), and returns the
/// `state_hash()` after every tick — so the whole trace, not just the final state, can be compared
/// across fps values.
fn app_hash_trace(fps: Fps, seed: u64, actions: &[Action]) -> Vec<u64> {
    let mut app = App::new(&cfg(fps, seed), world(), Box::new(MemorySaveIo::new()));
    app.apply(&[Action::Confirm]); // -> Playing
                                   // Phase 4 gates `Attack` on `has_sword`; established directly, see `hash_trace` above.
    app.state.hero.has_sword = true;

    let mut pacer = Pacer::new(fps.as_u32());
    let mut pending = Vec::new();
    let tick_ns = NANOS_PER_SEC / tuning::TICK_HZ;
    let mut hashes = Vec::with_capacity(actions.len());

    for &action in actions {
        advance_iteration(&mut app, &mut pacer, &mut pending, &[action], tick_ns);
        hashes.push(state_hash(&app.state));
    }
    hashes
}

#[test]
fn hash_sequence_is_unaffected_by_fps_batching() {
    let actions = action_sequence(4321, SEQUENCE_LEN);
    let f10 = app_hash_trace(Fps::F10, 7, &actions);
    let f20 = app_hash_trace(Fps::F20, 7, &actions);
    let f30 = app_hash_trace(Fps::F30, 7, &actions);
    assert_eq!(f10, f20);
    assert_eq!(f20, f30);
}

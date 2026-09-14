//! `BlockOnPlates` and `TorchSequence` (spec §7 step 4, phase 5): solving, visible feedback, the
//! reset-on-exit rule, and "every wrong state a puzzle allows still leaves the room solvable".
//!
//! `StepPlates` is covered by `tests/overworld.rs` (authored in phase 4); these two kinds are new
//! this phase.

use std::collections::{HashSet, VecDeque};
use std::rc::Rc;

use mosslight::game::ai::{occupied_positions, step as ai_step};
use mosslight::game::puzzles::block_push_target;
use mosslight::game::{
    update, Action, AiState, EnemyId, Facing, GameEvent, GameState, Pos, Room, Tick, World,
};

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

/// Teleports the hero into `room_id` at `pos` and runs `enter_room` — the same reset the real
/// per-door transition runs — matching the `state_in` pattern `tests/overworld.rs` already uses
/// for reaching a mid-game room directly rather than walking the whole overworld first.
fn state_in(room_id: &str, pos: Pos) -> GameState {
    let world = world();
    let room = world
        .room_idx(room_id)
        .unwrap_or_else(|| panic!("{room_id} exists in the embedded world"));
    let mut state = GameState::new(1, world);
    state.room = room;
    state.hero.pos = pos;
    state.enter_room();
    state
}

fn advance(state: &mut GameState, tick: &mut Tick, actions: &[Action]) -> Vec<GameEvent> {
    *tick += 1;
    update(state, actions, *tick)
}

/// Teleports the hero to just south of `torch` facing north and uses the lantern.
fn light_torch(state: &mut GameState, tick: &mut Tick, torch: Pos) -> Vec<GameEvent> {
    state.hero.pos = Pos {
        x: torch.x,
        y: torch.y + 1,
    };
    state.hero.facing = Facing::North;
    advance(state, tick, &[Action::UseLantern])
}

// --- BlockOnPlates (room.plate_chamber: block.chamber at (9,8), plate.chamber at (13,8)) ---

#[test]
fn block_on_plates_can_be_solved() {
    let mut state = state_in("room.plate_chamber", Pos { x: 8, y: 8 });
    let mut tick = 0u64;
    let mut solved = false;
    let mut revealed = false;
    let mut pushes = 0;
    for _ in 0..200 {
        let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
        for e in &events {
            match e {
                GameEvent::BlockPushed { .. } => pushes += 1,
                GameEvent::PuzzleSolved { .. } => solved = true,
                GameEvent::PassageRevealed { .. } => revealed = true,
                _ => {}
            }
        }
        if solved {
            break;
        }
    }
    assert!(solved, "block-on-plates puzzle never solved");
    assert!(
        revealed,
        "solving must reveal the hidden passage at (15, 8)"
    );
    assert_eq!(
        pushes, 4,
        "expected exactly 4 pushes from (9,8) to the plate at (13,8)"
    );
    assert_eq!(state.puzzle.blocks[0], Pos { x: 13, y: 8 });
    assert!(state.is_revealed(state.room, Pos { x: 15, y: 8 }));
}

#[test]
fn pushing_a_block_emits_visible_feedback() {
    let mut state = state_in("room.plate_chamber", Pos { x: 8, y: 8 });
    let mut tick = 0u64;
    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::BlockPushed {
                index: 0,
                from: Pos { x: 9, y: 8 },
                to: Pos { x: 10, y: 8 },
            }
        )),
        "{events:?}"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        GameEvent::HeroMoved {
            to: Pos { x: 9, y: 8 },
            ..
        }
    )));
}

#[test]
fn a_block_is_never_pushed_onto_a_door_or_stairs_tile() {
    let world = world();
    let room = world.room(world.room_idx("room.plate_chamber").unwrap());
    let no_enemies = std::collections::HashSet::new();
    // The west door sits at (0, 8); a block at (1, 8) pushed west would land on it.
    let target = block_push_target(
        room,
        &[Pos { x: 1, y: 8 }],
        &no_enemies,
        Pos { x: 1, y: 8 },
        Facing::West,
    );
    assert_eq!(
        target, None,
        "a block must never be pushed onto a door tile"
    );

    // A wall (bounding the chamber) refuses the push outright.
    let target = block_push_target(
        room,
        &[Pos { x: 0, y: 1 }],
        &no_enemies,
        Pos { x: 0, y: 1 },
        Facing::North,
    );
    assert_eq!(target, None, "a block must never be pushed onto a wall");
}

#[test]
fn a_block_blocks_the_hero_and_enemies() {
    let mut state = state_in("room.plate_chamber", Pos { x: 8, y: 8 });
    assert!(
        !state.walkable(state.room, Pos { x: 9, y: 8 }),
        "the block's own tile must not be walkable"
    );

    let occupied = occupied_positions(&state, EnemyId(999));
    assert!(
        occupied.contains(&Pos { x: 9, y: 8 }),
        "ai::occupied_positions must treat a current-room block as solid to enemies too"
    );

    // Force the room's slime right next to the block, aggroed past it toward the hero on the far
    // side, and confirm no number of AI steps ever lets it stand on the block's own tile
    // (round-1 review, major: enemies previously walked straight through a block).
    state.enemies[0].pos = Pos { x: 8, y: 8 };
    state.enemies[0].ai = AiState::SlimeChase { until: 10_000 };
    state.hero.pos = Pos { x: 10, y: 8 };
    for tick in 1..200u64 {
        ai_step(&mut state, tick);
        assert_ne!(
            state.enemies[0].pos,
            Pos { x: 9, y: 8 },
            "an enemy must never stand on a block's tile"
        );
    }
}

#[test]
fn pushing_a_block_onto_an_enemy_is_refused() {
    // An enemy standing on the block's own tile (bypassing normal AI, to isolate `try_push_block`'s
    // own guard rather than `ai::occupied_positions`'s) must stop a push from landing the hero on
    // top of it (round-1 review, major second-order defect).
    let mut state = state_in("room.plate_chamber", Pos { x: 8, y: 8 });
    state.enemies[0].pos = Pos { x: 9, y: 8 };
    let mut tick = 0u64;

    let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::MoveBlocked { .. })),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, GameEvent::BlockPushed { .. })),
        "{events:?}"
    );
    assert_eq!(
        state.puzzle.blocks[0],
        Pos { x: 9, y: 8 },
        "block must not move"
    );
    assert_eq!(state.hero.pos, Pos { x: 8, y: 8 }, "hero must not move");
    assert_ne!(
        state.hero.pos, state.enemies[0].pos,
        "hero and enemy must never share a tile"
    );
}

// --- TorchSequence (room.torch_vault: moss (5,5) -> water (10,5) -> stone (15,5)) ---

const MOSS: Pos = Pos { x: 5, y: 5 };
const WATER: Pos = Pos { x: 10, y: 5 };
const STONE: Pos = Pos { x: 15, y: 5 };

#[test]
fn torch_sequence_solves_in_the_hinted_order() {
    let mut state = state_in("room.torch_vault", Pos { x: 5, y: 6 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;

    let e1 = light_torch(&mut state, &mut tick, MOSS);
    assert!(e1.iter().any(|e| matches!(e, GameEvent::TorchLit { .. })));
    let e2 = light_torch(&mut state, &mut tick, WATER);
    assert!(e2.iter().any(|e| matches!(e, GameEvent::TorchLit { .. })));
    let e3 = light_torch(&mut state, &mut tick, STONE);
    assert!(
        e3.iter()
            .any(|e| matches!(e, GameEvent::PuzzleSolved { .. })),
        "{e3:?}"
    );
    assert!(
        e3.iter()
            .any(|e| matches!(e, GameEvent::PassageRevealed { .. })),
        "{e3:?}"
    );
    assert!(state.is_revealed(state.room, Pos { x: 20, y: 1 }));
}

#[test]
fn a_wrong_torch_resets_the_sequence_with_a_message() {
    let mut state = state_in("room.torch_vault", Pos { x: 5, y: 6 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;

    light_torch(&mut state, &mut tick, MOSS);
    assert_eq!(state.puzzle.sequence.len(), 1);
    let events = light_torch(&mut state, &mut tick, STONE); // out of order
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::PuzzleReset { .. })),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m.contains("gutter"))),
        "{events:?}"
    );
    assert!(state.puzzle.sequence.is_empty());
}

#[test]
fn a_sequence_torch_never_enters_progress_lit_torches() {
    let mut state = state_in("room.torch_vault", Pos { x: 5, y: 6 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;
    light_torch(&mut state, &mut tick, MOSS);
    assert!(
        state.progress.lit_torches.is_empty(),
        "a sequence torch is transient, not a permanent progress entry"
    );
}

#[test]
fn lighting_a_solved_sequence_torch_again_gives_feedback() {
    let mut state = state_in("room.torch_vault", Pos { x: 5, y: 6 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;
    light_torch(&mut state, &mut tick, MOSS);
    light_torch(&mut state, &mut tick, WATER);
    light_torch(&mut state, &mut tick, STONE);
    assert!(state
        .progress
        .solved_puzzles
        .iter()
        .any(|o| o.room == state.room));

    let events = light_torch(&mut state, &mut tick, MOSS);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::Message(m) if m.contains("steady"))),
        "a solved sequence torch must still say something when relit: {events:?}"
    );
}

/// The six orderings of the three `TorchSequence` torches (round-1 review, major: the previous
/// version sampled four hand-picked sequences out of six; the exhaustive form is cheap and is what
/// would actually catch a wrong order that accidentally sealed the puzzle).
fn torch_permutations() -> [[Pos; 3]; 6] {
    [
        [MOSS, WATER, STONE],
        [MOSS, STONE, WATER],
        [WATER, MOSS, STONE],
        [WATER, STONE, MOSS],
        [STONE, MOSS, WATER],
        [STONE, WATER, MOSS],
    ]
}

#[test]
fn every_wrong_torch_order_still_leaves_the_puzzle_solvable() {
    const HINTED: [Pos; 3] = [MOSS, WATER, STONE];

    for order in torch_permutations() {
        let mut state = state_in("room.torch_vault", Pos { x: 5, y: 6 });
        state.hero.has_lantern = true;
        let mut tick = 0u64;

        for &torch in &order {
            light_torch(&mut state, &mut tick, torch);
        }
        let solved_directly = state
            .progress
            .solved_puzzles
            .iter()
            .any(|o| o.room == state.room);
        assert_eq!(
            solved_directly,
            order == HINTED,
            "order {order:?}: only the hinted order must solve the puzzle directly"
        );

        // Whatever partial sequence state this order left behind, re-entry (the anti-soft-lock
        // guarantee) plus the hinted order must always still solve the puzzle.
        state.enter_room();
        for &torch in &HINTED {
            light_torch(&mut state, &mut tick, torch);
        }
        assert!(
            state
                .progress
                .solved_puzzles
                .iter()
                .any(|o| o.room == state.room),
            "order {order:?} must still leave the room solvable"
        );
    }
}

// --- Reset on room exit / persistence once solved (spec §6) ---

#[test]
fn leaving_and_re_entering_resets_an_unsolved_block_puzzle() {
    let mut state = state_in("room.plate_chamber", Pos { x: 8, y: 8 });
    let mut tick = 0u64;
    for _ in 0..20 {
        let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::BlockPushed { .. }))
        {
            break;
        }
    }
    assert_ne!(
        state.puzzle.blocks[0],
        Pos { x: 9, y: 8 },
        "the block should have moved"
    );

    state.enter_room();

    assert_eq!(
        state.puzzle.blocks[0],
        Pos { x: 9, y: 8 },
        "re-entry must reseed the block at its authored position"
    );
    assert!(!state
        .progress
        .solved_puzzles
        .iter()
        .any(|o| o.room == state.room));
}

#[test]
fn leaving_and_re_entering_resets_an_unsolved_torch_sequence() {
    let mut state = state_in("room.torch_vault", Pos { x: 5, y: 6 });
    state.hero.has_lantern = true;
    let mut tick = 0u64;
    light_torch(&mut state, &mut tick, MOSS);
    assert_eq!(state.puzzle.sequence.len(), 1);

    state.enter_room();

    assert!(
        state.puzzle.sequence.is_empty(),
        "re-entry must clear an unsolved sequence"
    );
    assert!(!state
        .progress
        .solved_puzzles
        .iter()
        .any(|o| o.room == state.room));
}

#[test]
fn a_solved_puzzle_and_its_reveals_survive_re_entry() {
    let mut state = state_in("room.plate_chamber", Pos { x: 8, y: 8 });
    let mut tick = 0u64;
    for _ in 0..200 {
        let events = advance(&mut state, &mut tick, &[Action::MoveEast]);
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::PuzzleSolved { .. }))
        {
            break;
        }
    }
    assert!(state
        .progress
        .solved_puzzles
        .iter()
        .any(|o| o.room == state.room));

    state.enter_room();

    assert!(
        state
            .progress
            .solved_puzzles
            .iter()
            .any(|o| o.room == state.room),
        "a solved puzzle must stay solved across re-entry"
    );
    assert!(
        state.is_revealed(state.room, Pos { x: 15, y: 8 }),
        "its reveal must survive too, even though the block itself resets"
    );
    assert_eq!(
        state.puzzle.blocks[0],
        Pos { x: 9, y: 8 },
        "the block's live position is still transient even once the puzzle is solved"
    );
}

/// One tile in `dir` from `pos`, or `None` off-grid — mirrors `game::state::step_target`, which is
/// `pub(super)` to `game` and so not reusable directly from an integration test.
fn step_pos(pos: Pos, dir: Facing) -> Option<Pos> {
    match dir {
        Facing::North => pos.y.checked_sub(1).map(|y| Pos { x: pos.x, y }),
        Facing::South => pos.y.checked_add(1).map(|y| Pos { x: pos.x, y }),
        Facing::East => pos.x.checked_add(1).map(|x| Pos { x, y: pos.y }),
        Facing::West => pos.x.checked_sub(1).map(|x| Pos { x, y: pos.y }),
    }
}

const PUSH_DIRS: [Facing; 4] = [Facing::North, Facing::East, Facing::South, Facing::West];

/// Every `(block_pos, hero_pos)` reachable in `room` from `block_start`/`hero_start` by ordinary
/// hero steps and pushes through `block_push_target` — the same predicate the simulation and
/// `content::validate`'s (private, unreachable from here) `block_puzzle_solvable` both use, so
/// this test can never disagree with either about what a player can do. Unlike that validator
/// search, this one never stops early on reaching a target, so it can enumerate every reachable
/// block position rather than just answering yes/no.
fn reachable_block_states(room: &Room, block_start: Pos, hero_start: Pos) -> HashSet<(Pos, Pos)> {
    let no_enemies: HashSet<Pos> = HashSet::new();
    let mut seen = HashSet::new();
    let start = (block_start, hero_start);
    seen.insert(start);
    let mut queue = VecDeque::new();
    queue.push_back(start);

    while let Some((block_pos, hero_pos)) = queue.pop_front() {
        for dir in PUSH_DIRS {
            let Some(next) = step_pos(hero_pos, dir) else {
                continue;
            };
            if next == block_pos {
                let Some(target) =
                    block_push_target(room, &[block_pos], &no_enemies, block_pos, dir)
                else {
                    continue;
                };
                let state = (target, block_pos);
                if seen.insert(state) {
                    queue.push_back(state);
                }
                continue;
            }
            if room.tile_at(next).is_some_and(|t| t.is_walkable()) && room.object_at(next).is_none()
            {
                let state = (block_pos, next);
                if seen.insert(state) {
                    queue.push_back(state);
                }
            }
        }
    }
    seen
}

#[test]
fn every_reachable_wrong_block_position_still_leaves_the_room_solvable() {
    // Exhaustive over every block position a real push sequence can reach in room.plate_chamber
    // (round-1 review, major: the previous version drove the block to exactly one hand-picked
    // wrong tile). Once the block is pushed flush against the interior wall at column 15 (or the
    // west boundary at column 1), pure re-pushing genuinely cannot recover it: reaching the tile
    // needed to push it back requires standing on that same wall, which is never floor. That is
    // by design, not a bug — the room's actual safety net is leaving and re-entering (spec §6:
    // "any wrong state a player can reach is undone by walking out of the room and back in"), so
    // the property this test proves is the one that guarantee actually depends on: the west
    // door's approach tile, (1, 8), must stay reachable from every one of those block positions —
    // including one wedged onto (1, 8) itself, the danger case the review named, which is only
    // safe because the block can still be pushed off that tile north/south before the hero needs
    // to stand there.
    let world = world();
    let room_idx = world.room_idx("room.plate_chamber").unwrap();
    let room = world.room(room_idx);
    let block_start = Pos { x: 9, y: 8 };
    let hero_start = Pos { x: 8, y: 8 };
    let door_approach = Pos { x: 1, y: 8 }; // spawn.plate_chamber.west, one tile in from the door

    let states = reachable_block_states(room, block_start, hero_start);
    let mut checked: HashSet<Pos> = HashSet::new();
    for &(block_pos, hero_pos) in &states {
        if !checked.insert(block_pos) {
            continue;
        }
        let door_still_reachable = reachable_block_states(room, block_pos, hero_pos)
            .iter()
            .any(|&(_, h)| h == door_approach);
        assert!(
            door_still_reachable,
            "block at {block_pos:?} (reached with the hero at {hero_pos:?}) seals off the west \
             door's approach tile — the room could never be left to re-enter and reset it"
        );
    }
    assert!(
        checked.len() > 20,
        "expected the open chamber to make many block positions reachable, found {}",
        checked.len()
    );
}

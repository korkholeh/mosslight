//! The optimal start-to-victory route (spec §13, §15), shared by `tests/playthrough.rs` and
//! `tests/balance.rs`: New Game to `GameWon` using only ordinary `Action`s, never touching a
//! `Progress` field directly. Moved out of `tests/playthrough.rs` in phase 7 so a second test file
//! can drive the identical route without either file depending on the other.

use mosslight::game::ai::strike_tiles;
use mosslight::game::{Action, AiState, Enemy, EnemyKind, Facing, GameEvent, GameState, Pos, Tick};

use super::Runner;

/// Everything the route produced: the full event log (for milestone-order assertions) and the
/// tick count the run actually took (for the tick-ceiling and balance-estimate assertions).
pub struct RouteOutcome {
    pub events: Vec<GameEvent>,
    pub ticks: Tick,
}

fn cross(runner: &mut Runner, log: &mut Vec<GameEvent>, approach: Pos, action: Action) {
    log.extend(runner.walk_to(approach));
    log.extend(runner.cross_door(action));
}

fn interact_facing(runner: &mut Runner, log: &mut Vec<GameEvent>, approach: Pos, facing: Facing) {
    log.extend(runner.walk_to(approach));
    log.extend(runner.face(facing));
    log.extend(runner.step(&[Action::Interact]));
}

fn light(runner: &mut Runner, log: &mut Vec<GameEvent>, torch: Pos) {
    log.extend(runner.walk_to(Pos {
        x: torch.x,
        y: torch.y + 1,
    }));
    log.extend(runner.face(Facing::North));
    log.extend(runner.step(&[Action::UseLantern]));
}

/// Pushes the block east, one step per tick, until the room's `BlockOnPlates` puzzle solves.
fn solve_block_puzzle(runner: &mut Runner, log: &mut Vec<GameEvent>, approach: Pos) {
    log.extend(runner.walk_to(approach));
    for _ in 0..500u32 {
        let events = runner.step(&[Action::MoveEast]);
        let solved = events
            .iter()
            .any(|e| matches!(e, GameEvent::PuzzleSolved { .. }));
        log.extend(events);
        if solved {
            return;
        }
    }
    panic!("block-on-plates puzzle never solved");
}

fn boss(state: &GameState) -> Option<Enemy> {
    state
        .enemies
        .iter()
        .find(|e| e.kind == EnemyKind::Boss)
        .cloned()
}

fn orthogonally_adjacent(a: Pos, b: Pos) -> bool {
    let dx = (a.x as i32 - b.x as i32).abs();
    let dy = (a.y as i32 - b.y as i32).abs();
    (dx == 1 && dy == 0) || (dx == 0 && dy == 1)
}

fn toward(from: Pos, to: Pos) -> Option<Action> {
    let dx = to.x as i32 - from.x as i32;
    let dy = to.y as i32 - from.y as i32;
    if dx == 0 && dy == 0 {
        return None;
    }
    if dx.abs() >= dy.abs() {
        Some(if dx > 0 {
            Action::MoveEast
        } else {
            Action::MoveWest
        })
    } else {
        Some(if dy > 0 {
            Action::MoveSouth
        } else {
            Action::MoveNorth
        })
    }
}

fn facing_action(facing: Facing) -> Action {
    match facing {
        Facing::North => Action::MoveNorth,
        Facing::South => Action::MoveSouth,
        Facing::East => Action::MoveEast,
        Facing::West => Action::MoveWest,
    }
}

fn dodge(at: Pos, danger: &[Pos]) -> Action {
    const CANDIDATES: [(Action, i32, i32); 4] = [
        (Action::MoveNorth, 0, -1),
        (Action::MoveSouth, 0, 1),
        (Action::MoveEast, 1, 0),
        (Action::MoveWest, -1, 0),
    ];
    for (action, dx, dy) in CANDIDATES {
        let nx = at.x as i32 + dx;
        let ny = at.y as i32 + dy;
        if !(1..23).contains(&nx) || !(1..15).contains(&ny) {
            continue;
        }
        let next = Pos {
            x: nx as u8,
            y: ny as u8,
        };
        if !danger.contains(&next) {
            return action;
        }
    }
    panic!("no safe tile to dodge to from {at:?} given danger {danger:?}");
}

fn continue_action(state: &GameState, b: &Enemy) -> Action {
    let dist = (state.hero.pos.x as i32 - b.pos.x as i32).abs()
        + (state.hero.pos.y as i32 - b.pos.y as i32).abs();
    if dist > 2 {
        toward(state.hero.pos, b.pos).unwrap_or(Action::Confirm)
    } else {
        Action::Confirm
    }
}

/// The reactive boss-fight script `tests/boss.rs` already proves is a clean, no-damage kill: this
/// reuses the same shape, driven through `Runner` instead of raw `GameState`/`Tick`.
fn fight_boss(runner: &mut Runner, log: &mut Vec<GameEvent>) {
    for _ in 0..20_000u32 {
        let Some(b) = boss(&runner.state) else {
            break;
        };
        if !b.alive {
            break;
        }
        let room = runner.state.room().clone();

        let action = match b.ai {
            AiState::BossVulnerable { .. } => {
                if orthogonally_adjacent(runner.state.hero.pos, b.pos) {
                    let want = toward(runner.state.hero.pos, b.pos);
                    if want == Some(facing_action(runner.state.hero.facing))
                        && runner.state.hero.attack.is_none()
                    {
                        Action::Attack
                    } else {
                        want.unwrap_or(Action::Attack)
                    }
                } else {
                    toward(runner.state.hero.pos, b.pos).expect("not yet adjacent")
                }
            }
            AiState::BossWindup { pattern, .. } | AiState::BossStrike { pattern, .. } => {
                let danger = strike_tiles(pattern, b.pos, &room);
                if danger.contains(&runner.state.hero.pos) {
                    dodge(runner.state.hero.pos, &danger)
                } else {
                    continue_action(&runner.state, &b)
                }
            }
            AiState::BossStalk { .. } => continue_action(&runner.state, &b),
            _ => unreachable!("boss_arena spawns only the boss"),
        };

        log.extend(runner.step(&[action]));
    }
}

/// New Game → `GameWon`, using only `Action`s: learn of the sanctuary, take the sword, solve the
/// mill and take the lantern, clear the dungeon's two puzzles and both keys, beat the boss, then
/// carry the ember home to relight the beacon. Panics with a located message if the route breaks
/// (a door never opens, a puzzle never solves, the boss fight runs away) rather than looping
/// forever.
pub fn play_to_victory(runner: &mut Runner) -> RouteOutcome {
    let mut log: Vec<GameEvent> = Vec::new();

    // --- Overworld: learn of the sanctuary, take the sword, solve the mill and take the lantern.
    log.extend(runner.walk_to(Pos { x: 5, y: 6 }));
    log.extend(runner.face(Facing::North)); // npc.keeper at (5, 5)
    log.extend(runner.step(&[Action::Interact]));
    log.extend(runner.step(&[Action::Confirm])); // -> node 1, sets flag.told_about_sanctuary
    log.extend(runner.step(&[Action::Confirm])); // closes

    cross(runner, &mut log, Pos { x: 12, y: 1 }, Action::MoveNorth); // -> crossroads
    interact_facing(runner, &mut log, Pos { x: 5, y: 6 }, Facing::North); // chest.forest_sword

    cross(runner, &mut log, Pos { x: 12, y: 1 }, Action::MoveNorth); // -> stone_circle
    cross(runner, &mut log, Pos { x: 22, y: 8 }, Action::MoveEast); // -> old_mill

    log.extend(runner.walk_to(Pos { x: 5, y: 9 })); // plate.mill1
    log.extend(runner.walk_to(Pos { x: 10, y: 9 })); // plate.mill2
    log.extend(runner.walk_to(Pos { x: 15, y: 9 })); // plate.mill3
    interact_facing(runner, &mut log, Pos { x: 20, y: 1 }, Facing::East); // chest.mill_lantern

    cross(runner, &mut log, Pos { x: 12, y: 14 }, Action::MoveSouth); // -> east_marsh
    cross(runner, &mut log, Pos { x: 22, y: 8 }, Action::MoveEast); // -> sanctuary_gate (needs lantern)

    // --- The dungeon.
    cross(runner, &mut log, Pos { x: 12, y: 14 }, Action::MoveSouth); // -> flooded_hall
    interact_facing(runner, &mut log, Pos { x: 5, y: 11 }, Facing::North); // chest.hall_key
    cross(runner, &mut log, Pos { x: 22, y: 8 }, Action::MoveEast); // -> plate_chamber (spends 1 key)

    solve_block_puzzle(runner, &mut log, Pos { x: 8, y: 8 });
    cross(runner, &mut log, Pos { x: 22, y: 8 }, Action::MoveEast); // -> torch_vault

    light(runner, &mut log, Pos { x: 5, y: 5 }); // moss
    light(runner, &mut log, Pos { x: 10, y: 5 }); // water
    light(runner, &mut log, Pos { x: 15, y: 5 }); // stone
    interact_facing(runner, &mut log, Pos { x: 20, y: 1 }, Facing::East); // chest.vault_key

    cross(runner, &mut log, Pos { x: 12, y: 14 }, Action::MoveSouth); // -> warden_walk
    cross(runner, &mut log, Pos { x: 22, y: 8 }, Action::MoveEast); // -> boss_arena (spends 2nd key)

    fight_boss(runner, &mut log);
    assert!(runner.state.hero.has_ember, "the boss must drop the ember");

    // --- Home to the lighthouse.
    cross(runner, &mut log, Pos { x: 1, y: 8 }, Action::MoveWest); // -> warden_walk
    cross(runner, &mut log, Pos { x: 12, y: 1 }, Action::MoveNorth); // -> torch_vault
    cross(runner, &mut log, Pos { x: 1, y: 8 }, Action::MoveWest); // -> plate_chamber
    cross(runner, &mut log, Pos { x: 1, y: 8 }, Action::MoveWest); // -> flooded_hall
    cross(runner, &mut log, Pos { x: 12, y: 1 }, Action::MoveNorth); // -> sanctuary_gate
    cross(runner, &mut log, Pos { x: 1, y: 8 }, Action::MoveWest); // -> east_marsh
    cross(runner, &mut log, Pos { x: 1, y: 8 }, Action::MoveWest); // -> crossroads
    cross(runner, &mut log, Pos { x: 12, y: 14 }, Action::MoveSouth); // -> lighthouse

    interact_facing(runner, &mut log, Pos { x: 20, y: 6 }, Facing::North); // the beacon

    assert!(
        log.iter().any(|e| matches!(e, GameEvent::GameWon)),
        "the run must end in GameWon"
    );
    assert!(runner
        .state
        .progress
        .flags
        .contains("flag.lighthouse_relit"));

    RouteOutcome {
        events: log,
        ticks: runner.state.tick,
    }
}

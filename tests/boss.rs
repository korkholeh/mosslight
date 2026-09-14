//! The two-phase boss (spec §6, phase 5): phase transition, distinct per-phase patterns,
//! telegraph lead time, damage only while vulnerable, no contact damage, a no-damage scripted
//! kill, the ember/flag grant, and no respawn once defeated.

use std::rc::Rc;

use mosslight::game::ai::strike_tiles;
use mosslight::game::tuning::{BOSS_PHASE_TWO_HP, TELEGRAPH_MIN_TICKS};
use mosslight::game::{
    update, Action, AiState, BossPattern, EnemyKind, Facing, GameEvent, GameState, Pos, Tick, World,
};

fn world() -> Rc<World> {
    Rc::new(mosslight::content::load().expect("embedded world validates"))
}

fn state_in_boss_arena(hero_pos: Pos) -> GameState {
    let world = world();
    let room = world
        .room_idx("room.boss_arena")
        .expect("room.boss_arena exists in the embedded world");
    let mut state = GameState::new(1, world);
    state.room = room;
    state.hero.pos = hero_pos;
    state.hero.has_sword = true;
    state.enter_room();
    state
}

fn advance(state: &mut GameState, tick: &mut Tick, actions: &[Action]) -> Vec<GameEvent> {
    *tick += 1;
    update(state, actions, *tick)
}

fn boss(state: &GameState) -> Option<mosslight::game::Enemy> {
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

/// The single-axis direction from `from` toward `to` (bigger delta first); once orthogonally
/// adjacent this always resolves to the one non-zero axis, so the same helper both approaches and
/// faces the boss.
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

/// A step off `at` that lands outside `danger` and inside the arena's floor — the reactive
/// script's dodge move while the boss is windup/strike-telegraphing a pattern the hero stands in.
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

/// Drives the fight reactively: approaches and strikes only during `BossVulnerable`, and dodges
/// off the telegraphed tile set whenever windup or strike would otherwise hit the hero — proving
/// the vulnerability window is real by winning without ever losing a half-heart.
fn run_scripted_fight(state: &mut GameState, tick: &mut Tick) -> Vec<GameEvent> {
    let mut all_events = Vec::new();
    let start_health = state.hero.health_halves;
    let room = state.room().clone();

    for _ in 0..20_000u32 {
        let Some(b) = boss(state) else {
            break;
        };
        if !b.alive {
            break;
        }

        let action = match b.ai {
            AiState::BossVulnerable { .. } => {
                if orthogonally_adjacent(state.hero.pos, b.pos) {
                    let want = toward(state.hero.pos, b.pos);
                    if want == Some(facing_action(state.hero.facing))
                        && state.hero.can_attack(*tick + 1)
                    {
                        Action::Attack
                    } else {
                        want.unwrap_or(Action::Attack)
                    }
                } else {
                    toward(state.hero.pos, b.pos).expect("not yet adjacent")
                }
            }
            AiState::BossWindup { pattern, .. } | AiState::BossStrike { pattern, .. } => {
                let danger = strike_tiles(pattern, b.pos, &room);
                if danger.contains(&state.hero.pos) {
                    dodge(state.hero.pos, &danger)
                } else {
                    continue_action(state, &b)
                }
            }
            AiState::BossStalk { .. } => continue_action(state, &b),
            _ => unreachable!("boss_arena spawns only the boss"),
        };

        let events = advance(state, tick, &[action]);
        assert_eq!(
            state.hero.health_halves, start_health,
            "hero must never be hit during the scripted fight (tick {tick})"
        );
        all_events.extend(events);
    }

    all_events
}

/// Outside a vulnerable/danger tick: close in a little if far away, otherwise hold — approaching
/// too eagerly during `BossStalk` just means arriving next to the boss when its telegraph starts,
/// which the windup/strike arm already dodges.
fn continue_action(state: &GameState, b: &mosslight::game::Enemy) -> Action {
    let dist = (state.hero.pos.x as i32 - b.pos.x as i32).abs()
        + (state.hero.pos.y as i32 - b.pos.y as i32).abs();
    if dist > 2 {
        toward(state.hero.pos, b.pos).unwrap_or(Action::Confirm)
    } else {
        Action::Confirm // accepted and ignored by `update` — an explicit no-op wait
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

#[test]
fn the_boss_enters_phase_two_at_the_tuned_threshold() {
    let mut state = state_in_boss_arena(Pos { x: 1, y: 8 });
    let mut tick = 0u64;

    // Deal damage directly, bypassing navigation, to isolate the phase-threshold rule itself.
    while boss(&state).unwrap().hp > BOSS_PHASE_TWO_HP {
        for e in state.enemies.iter_mut() {
            if e.kind == EnemyKind::Boss {
                e.ai = AiState::BossVulnerable {
                    phase: 1,
                    until: tick + 1000,
                };
                e.hp = e.hp.saturating_sub(1);
            }
        }
    }
    let events = advance(&mut state, &mut tick, &[]);
    let b = boss(&state).unwrap();
    assert!(
        matches!(b.ai, AiState::BossStalk { phase: 2, .. }),
        "expected phase 2 at hp <= {BOSS_PHASE_TWO_HP}, got {:?} at hp {}",
        b.ai,
        b.hp
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::BossPhaseChanged { phase: 2, .. })),
        "{events:?}"
    );
}

/// Drives the fight far enough to observe the boss's real, in-game telegraphed tile set: the
/// pattern `ai::boss_pattern_for_phase` actually chose, not one hand-built by the test — a test
/// that only compares two `strike_tiles` calls it built itself cannot fail on a
/// `boss_pattern_for_phase` bug (round-1 review, major).
fn telegraphed_tiles(state: &mut GameState, tick: &mut Tick) -> Vec<Pos> {
    for _ in 0..3000u32 {
        let events = advance(state, tick, &[]);
        for e in events {
            if let GameEvent::BossTelegraph { tiles, .. } = e {
                return tiles;
            }
        }
    }
    panic!("boss never telegraphed within the tick budget");
}

#[test]
fn each_phase_uses_a_distinct_attack_pattern() {
    let mut state = state_in_boss_arena(Pos { x: 1, y: 1 });
    let mut tick = 0u64;

    let phase1_tiles = telegraphed_tiles(&mut state, &mut tick);
    let phase1_pos = boss(&state).unwrap().pos;
    let phase1_set: std::collections::HashSet<Pos> = phase1_tiles.into_iter().collect();
    assert_eq!(
        phase1_set,
        strike_tiles(BossPattern::Slam, phase1_pos, &state.room().clone())
            .into_iter()
            .collect(),
        "phase 1 must telegraph the Slam pattern"
    );

    // Force the phase-two threshold, mid-fight, and observe the next real telegraph.
    for e in state.enemies.iter_mut() {
        if e.kind == EnemyKind::Boss {
            e.hp = BOSS_PHASE_TWO_HP;
        }
    }
    let phase2_tiles = telegraphed_tiles(&mut state, &mut tick);
    let phase2_pos = boss(&state).unwrap().pos;
    let phase2_set: std::collections::HashSet<Pos> = phase2_tiles.into_iter().collect();
    assert_eq!(
        phase2_set,
        strike_tiles(BossPattern::Sweep, phase2_pos, &state.room().clone())
            .into_iter()
            .collect(),
        "phase 2 must telegraph the Sweep pattern"
    );

    assert_ne!(
        phase1_set, phase2_set,
        "the two phases must hit different tile sets"
    );
}

/// Sorted so the tile set is a stable map key regardless of the order `strike_tiles` built it in.
fn sorted_tiles(tiles: &[Pos]) -> Vec<Pos> {
    let mut sorted = tiles.to_vec();
    sorted.sort_by_key(|p| (p.x, p.y));
    sorted
}

#[test]
fn every_strike_is_preceded_by_a_telegraph_of_at_least_the_tuned_warning() {
    // Hero stays far away, so the boss's stalk-to-windup transition is timer-driven, not
    // hero-triggered. Every `BossStruck` in the whole run is checked against the telegraph that
    // carried its exact tile set (round-1 review, major: the previous version returned on the
    // first strike, covering only phase 1's comfortable 24-tick windup, never phase 2's boundary
    // case whose 18-tick windup equals `TELEGRAPH_MIN_TICKS` exactly), forcing the phase-two
    // threshold right after the first strike so both phases are exercised in one run.
    let mut state = state_in_boss_arena(Pos { x: 1, y: 1 });
    let mut tick = 0u64;
    let mut telegraphed: std::collections::HashMap<Vec<Pos>, Tick> =
        std::collections::HashMap::new();
    let mut checked_phase1 = false;
    let mut checked_phase2 = false;

    for _ in 0..5000u32 {
        let events = advance(&mut state, &mut tick, &[]);
        for e in &events {
            match e {
                GameEvent::BossTelegraph { tiles, .. } => {
                    telegraphed.insert(sorted_tiles(tiles), tick);
                }
                GameEvent::BossStruck { tiles } => {
                    let key = sorted_tiles(tiles);
                    let start = *telegraphed
                        .get(&key)
                        .expect("a strike's tile set must have been telegraphed first");
                    assert!(
                        tick - start >= TELEGRAPH_MIN_TICKS,
                        "strike came only {} ticks after its telegraph",
                        tick - start
                    );
                    if !checked_phase1 {
                        checked_phase1 = true;
                        // Force the phase-two boundary right after the first (phase-1) strike, so
                        // the very next strike observed is phase 2's exact-floor windup.
                        for e in state.enemies.iter_mut() {
                            if e.kind == EnemyKind::Boss {
                                e.hp = BOSS_PHASE_TWO_HP;
                            }
                        }
                    } else {
                        checked_phase2 = true;
                    }
                }
                _ => {}
            }
        }
        if checked_phase2 {
            break;
        }
    }
    assert!(
        checked_phase1 && checked_phase2,
        "expected to observe and check a telegraphed strike in both phase 1 and phase 2"
    );
}

#[test]
fn the_boss_only_takes_damage_during_its_vulnerability_window() {
    use mosslight::game::combat;

    let ai_states = [
        AiState::BossStalk {
            phase: 1,
            until: 1000,
        },
        AiState::BossWindup {
            phase: 1,
            until: 1000,
            pattern: BossPattern::Slam,
        },
        AiState::BossStrike {
            phase: 1,
            pattern: BossPattern::Slam,
        },
        AiState::BossVulnerable {
            phase: 1,
            until: 1000,
        },
    ];

    for ai_state in ai_states {
        let mut state = state_in_boss_arena(Pos { x: 1, y: 8 });
        let b_pos = boss(&state).unwrap().pos;
        state.hero.pos = Pos {
            x: b_pos.x - 1,
            y: b_pos.y,
        };
        state.hero.facing = Facing::East;
        for e in state.enemies.iter_mut() {
            if e.kind == EnemyKind::Boss {
                e.ai = ai_state;
            }
        }

        combat::start_swing(&mut state.hero, 0);
        let events = combat::resolve_swing(&mut state, 0);

        if matches!(ai_state, AiState::BossVulnerable { .. }) {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, GameEvent::EnemyDamaged { .. })),
                "{ai_state:?} should take damage: {events:?}"
            );
        } else {
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, GameEvent::AttackDeflected { .. })),
                "{ai_state:?} should deflect: {events:?}"
            );
        }
    }
}

#[test]
fn the_boss_deals_no_contact_damage() {
    // Standing right next to the boss for a long window (through stalk, telegraph and strike)
    // will legitimately take strike damage from a `Slam` that covers the adjacent tile — that is
    // not what this test forbids. It asserts the stronger, precise claim: `HeroDamaged` never
    // fires on a tick without a `BossStruck` alongside it, i.e. `apply_contact_damage` never
    // finds the boss (see its doc comment: the boss is excluded from that search by construction).
    let mut state = state_in_boss_arena(Pos { x: 1, y: 8 });
    let mut tick = 0u64;
    let b_pos = boss(&state).unwrap().pos;
    state.hero.pos = Pos {
        x: b_pos.x.saturating_sub(1),
        y: b_pos.y,
    };
    let mut any_damage = false;
    for _ in 0..300u32 {
        let events = advance(&mut state, &mut tick, &[]);
        let struck = events
            .iter()
            .any(|e| matches!(e, GameEvent::BossStruck { .. }));
        let damaged = events
            .iter()
            .any(|e| matches!(e, GameEvent::HeroDamaged { .. }));
        any_damage |= damaged;
        assert!(
            !damaged || struck,
            "hero damage on tick {tick} with no BossStruck alongside it — contact damage from the boss"
        );
    }
    assert!(
        any_damage,
        "the test setup should have taken at least one strike hit"
    );
}

#[test]
fn a_scripted_fight_beats_the_boss_with_full_health_intact() {
    let mut state = state_in_boss_arena(Pos { x: 1, y: 8 });
    let mut tick = 0u64;
    let start_health = state.hero.health_halves;

    let events = run_scripted_fight(&mut state, &mut tick);

    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::BossDefeated { .. })),
        "the boss must be defeated"
    );
    assert_eq!(state.hero.health_halves, start_health, "no half-heart lost");
    assert!(boss(&state).is_none() || !boss(&state).unwrap().alive);
}

#[test]
fn defeating_the_boss_grants_the_ember_and_sets_the_flag() {
    let mut state = state_in_boss_arena(Pos { x: 1, y: 8 });
    let mut tick = 0u64;

    let events = run_scripted_fight(&mut state, &mut tick);

    assert!(state.hero.has_ember, "the boss's drop must be granted");
    assert!(
        state.progress.flags.contains("flag.boss_defeated"),
        "the authored defeat_flag must be set"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::ItemPicked { .. })));
}

#[test]
fn a_defeated_boss_does_not_respawn_on_re_entry() {
    let mut state = state_in_boss_arena(Pos { x: 1, y: 8 });
    let mut tick = 0u64;
    run_scripted_fight(&mut state, &mut tick);
    assert!(state.progress.flags.contains("flag.boss_defeated"));

    state.enter_room();

    assert!(
        state.enemies.is_empty(),
        "a defeated boss must not respawn on re-entry"
    );
}

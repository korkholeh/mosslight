//! Sword combat: hitbox, cooldown, per-swing hit tracking, contact damage, invulnerability,
//! knockback and death (spec §6, §13).

use std::collections::HashSet;
use std::rc::Rc;

use mosslight::game::combat;
use mosslight::game::tuning::{KNOCKBACK_TILES, SWORD_ACTIVE_TICKS};
use mosslight::game::{
    update, Action, AiState, Enemy, EnemyId, EnemyKind, Facing, GameEvent, GameState, Pos, Tile,
};

#[test]
fn swing_expiry_emits_an_event_so_the_sword_glyph_gets_cleared() {
    // Round-1 review, major: `expire_swing` used to clear `hero.attack` silently, so the tick
    // that ends the animation reported no change and `App::tick` never marked the frame dirty —
    // the sword glyph stayed on screen after the swing was actually gone.
    let mut state = fresh();
    state.hero.pos = Pos { x: 10, y: 5 };
    state.hero.facing = Facing::North;

    update(&mut state, &[Action::Attack], 0);
    assert!(state.hero.attack.is_some());

    for tick in 1..SWORD_ACTIVE_TICKS {
        let events = update(&mut state, &[], tick);
        assert!(
            state.hero.attack.is_some(),
            "swing must still be active mid-window at tick {tick}"
        );
        assert!(
            events.is_empty(),
            "nothing should happen while the swing is still active and untouched: {events:?}"
        );
    }

    let events = update(&mut state, &[], SWORD_ACTIVE_TICKS);
    assert!(
        state.hero.attack.is_none(),
        "swing must be cleared once its active window has passed"
    );
    assert!(
        events.iter().any(|e| matches!(e, GameEvent::AttackEnded)),
        "swing expiry must emit an event so the tick is not silently dropped: {events:?}"
    );
}

#[test]
fn door_transition_clears_a_live_swing() {
    // Round-1 review, major: a swing surviving a door transition carries the old room's target
    // tile and enemy ids into the new room, so it could damage an unrelated enemy on the stale
    // tile while leaving the enemy at the matching index immune.
    let mut state = fresh();
    let start_room = state.room;

    // room.lighthouse's north door sits at (12, 0); starting one tile south of it lets a single
    // MoveNorth cross it (mirrors `state::tests::stepping_onto_a_door_tile_...`).
    state.hero.pos = Pos { x: 12, y: 1 };

    update(&mut state, &[Action::Attack], 0);
    assert!(state.hero.attack.is_some());

    let events = update(&mut state, &[Action::MoveNorth], 1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::RoomEntered { .. })),
        "the door must have actually been crossed: {events:?}"
    );
    assert_ne!(state.room, start_room);
    assert!(
        state.hero.attack.is_none(),
        "a swing must not survive into a room the hero has already left"
    );
}

fn fresh() -> GameState {
    // room.lighthouse authors zero enemy spawns (see PLAN.md's pacing table), so `fresh()` starts
    // with an empty `enemies` vec that every test here populates explicitly.
    let world = Rc::new(mosslight::content::load().expect("embedded world validates"));
    GameState::new(1, world)
}

fn slime_with_hp(id: EnemyId, pos: Pos, hp: u8) -> Enemy {
    Enemy {
        id,
        kind: EnemyKind::Slime,
        pos,
        facing: Facing::South,
        hp,
        ai: AiState::SlimeIdle { until: 10_000 },
        patrol: Vec::new(),
        move_ready_at: 10_000,
        alive: true,
    }
}

fn target_tile(pos: Pos, facing: Facing) -> Pos {
    match facing {
        Facing::North => Pos {
            x: pos.x,
            y: pos.y - 1,
        },
        Facing::South => Pos {
            x: pos.x,
            y: pos.y + 1,
        },
        Facing::East => Pos {
            x: pos.x + 1,
            y: pos.y,
        },
        Facing::West => Pos {
            x: pos.x - 1,
            y: pos.y,
        },
    }
}

#[test]
fn sword_hits_only_the_faced_tile() {
    for facing in [Facing::North, Facing::East, Facing::South, Facing::West] {
        let mut state = fresh();
        let pos = Pos { x: 10, y: 5 };
        state.hero.pos = pos;
        state.hero.facing = facing;
        let target = target_tile(pos, facing);
        state.enemies = vec![slime_with_hp(EnemyId(0), target, 2)];

        let events = update(&mut state, &[Action::Attack], 0);

        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::EnemyDamaged { id, .. } if *id == EnemyId(0))),
            "{facing:?}: {events:?}"
        );
        assert_eq!(state.enemies[0].hp, 1);
    }
}

#[test]
fn sword_misses_side_and_rear_tiles() {
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    state.enemies = vec![
        slime_with_hp(
            EnemyId(0),
            Pos {
                x: pos.x - 1,
                y: pos.y,
            },
            2,
        ), // west: a side tile
        slime_with_hp(
            EnemyId(1),
            Pos {
                x: pos.x + 1,
                y: pos.y,
            },
            2,
        ), // east: a side tile
        slime_with_hp(
            EnemyId(2),
            Pos {
                x: pos.x,
                y: pos.y + 1,
            },
            2,
        ), // south: behind the hero
    ];

    let events = update(&mut state, &[Action::Attack], 0);

    assert!(!events.iter().any(|e| matches!(
        e,
        GameEvent::EnemyDamaged { .. } | GameEvent::EnemyKilled { .. }
    )));
    for enemy in &state.enemies {
        assert_eq!(enemy.hp, 2, "{enemy:?} must be untouched by a north swing");
    }
}

#[test]
fn each_target_is_damaged_at_most_once_per_swing() {
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    let target = target_tile(pos, Facing::North);
    state.enemies = vec![
        slime_with_hp(EnemyId(0), target, 5),
        slime_with_hp(EnemyId(1), target, 5),
    ];

    update(&mut state, &[Action::Attack], 0);
    // The active window covers ticks 0..SWORD_ACTIVE_TICKS; step through the rest of it with no
    // further `Attack` action, so the only source of damage is the one swing already started.
    for tick in 1..SWORD_ACTIVE_TICKS {
        update(&mut state, &[], tick);
    }

    assert_eq!(state.enemies[0].hp, 4, "hit more than once by one swing");
    assert_eq!(state.enemies[1].hp, 4, "hit more than once by one swing");
}

#[test]
fn attack_inside_cooldown_is_ignored_and_the_next_one_lands() {
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    let target = target_tile(pos, Facing::North);
    state.enemies = vec![slime_with_hp(EnemyId(0), target, 5)];
    // The sword's one-tile hitbox is by construction orthogonally adjacent, so the same enemy
    // would also deal contact damage and knock the hero off this tile (round-2 review fix to
    // `direction_away`/`knockback`). This test is about the cooldown, not contact, so keep the
    // hero invulnerable throughout.
    state.hero.invuln_until = 1000;

    update(&mut state, &[Action::Attack], 0);
    assert_eq!(state.enemies[0].hp, 4);

    // Still inside the cooldown (attack_ready_at = 0 + SWORD_COOLDOWN_TICKS = 9).
    let events = update(&mut state, &[Action::Attack], 5);
    assert!(!events
        .iter()
        .any(|e| matches!(e, GameEvent::AttackSwung { .. })));
    assert_eq!(state.enemies[0].hp, 4);

    let events = update(&mut state, &[Action::Attack], 9);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::AttackSwung { .. })));
    assert_eq!(state.enemies[0].hp, 3);
}

#[test]
fn contact_damage_costs_one_half_heart_and_grants_invulnerability() {
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.enemies = vec![slime_with_hp(
        EnemyId(0),
        Pos {
            x: pos.x + 1,
            y: pos.y,
        },
        5,
    )];

    let events = update(&mut state, &[], 0);
    assert!(events.iter().any(
        |e| matches!(e, GameEvent::HeroDamaged { remaining_halves } if *remaining_halves == 5)
    ));
    assert_eq!(state.hero.health_halves, 5);
    assert!(state.hero.is_invulnerable(1));

    // Reposition the enemy adjacent to the hero's post-knockback tile so the second contact is
    // actually exercised rather than skipped for lack of proximity.
    let hero_pos = state.hero.pos;
    state.enemies[0].pos = Pos {
        x: hero_pos.x + 1,
        y: hero_pos.y,
    };

    let events = update(&mut state, &[], 1);
    assert!(!events
        .iter()
        .any(|e| matches!(e, GameEvent::HeroDamaged { .. })));
    assert_eq!(
        state.hero.health_halves, 5,
        "still invulnerable, no second hit"
    );
}

#[test]
fn contact_knockback_pushes_the_hero_away_from_the_enemy() {
    // Round-2 review, major: the call site passed `direction_away(hero_pos, source_pos, ..)`,
    // which computes the direction FROM the hero TOWARD the enemy — knocking the hero straight
    // into the attacker instead of away from it. Because the attacker's own tile is in the
    // `occupied` set, `knockback()` hit that check on its first step and was a no-op in every
    // reachable configuration.
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    state.enemies = vec![slime_with_hp(
        EnemyId(0),
        Pos {
            x: pos.x + 1,
            y: pos.y,
        },
        5,
    )];

    update(&mut state, &[], 0);

    assert_eq!(
        state.hero.pos,
        Pos {
            x: pos.x - KNOCKBACK_TILES,
            y: pos.y,
        },
        "a slime east of the hero must knock the hero west, away from it"
    );
}

#[test]
fn knockback_stops_adjacent_to_an_obstacle() {
    let state = fresh();
    let room = state.room();
    let occupied = HashSet::new();

    // room.lighthouse's west wall sits at x = 0; walking 5 tiles west from x = 3 must stop at
    // x = 1, adjacent to the wall, never on or past it.
    let result = combat::knockback(room, &occupied, Pos { x: 3, y: 5 }, Facing::West, 5);
    assert_eq!(result, Pos { x: 1, y: 5 });
}

#[test]
fn knockback_never_lands_on_a_door_tile() {
    let state = fresh();
    let room = state.room();
    let occupied = HashSet::new();

    // room.lighthouse has a west door at (0, 8); walking west from (2, 8) must stop at (1, 8),
    // never stepping onto the door tile itself.
    let result = combat::knockback(room, &occupied, Pos { x: 2, y: 8 }, Facing::West, 5);
    assert_eq!(result, Pos { x: 1, y: 8 });
    assert_ne!(room.tile_at(result), Some(Tile::Door));
}

#[test]
fn hero_cannot_step_onto_a_live_enemys_tile() {
    // Round-2 review, minor: hero movement checked only `Tile::is_walkable`, so the hero could
    // overlap a live enemy's tile — the renderer paints enemies before the hero, so the enemy
    // glyph vanished under the hero, and contact damage then took the degenerate
    // dx==0/dy==0 knockback fallback instead of a real push away from the enemy.
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    let target = Pos {
        x: pos.x,
        y: pos.y - 1,
    };
    state.enemies = vec![slime_with_hp(EnemyId(0), target, 5)];

    let events = update(&mut state, &[Action::MoveNorth], 0);

    // The enemy is orthogonally adjacent either way, so contact damage and its knockback still
    // apply this same tick; what this test pins is that the hero never lands on the enemy's tile.
    assert_ne!(
        state.hero.pos, target,
        "the hero must not enter the enemy's tile"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, GameEvent::MoveBlocked { .. })),
        "stepping onto a live enemy must report MoveBlocked: {events:?}"
    );
}

#[test]
fn hero_can_step_into_a_dead_enemys_tile() {
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    let target = Pos {
        x: pos.x,
        y: pos.y - 1,
    };
    let mut enemy = slime_with_hp(EnemyId(0), target, 5);
    enemy.alive = false;
    state.enemies = vec![enemy];

    update(&mut state, &[Action::MoveNorth], 0);

    assert_eq!(
        state.hero.pos, target,
        "a dead enemy must not block movement"
    );
}

#[test]
fn hero_died_fires_only_on_the_tick_health_reaches_zero() {
    // Round-2 review, nit: `HeroDied` used to be pushed whenever `health_halves == 0`, not once
    // on the alive-to-dead transition, so any caller still ticking the simulation after death
    // (a headless test, a future replay tool) got one `HeroDied` per tick.
    let mut state = fresh();
    state.hero.health_halves = 1;
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.enemies = vec![slime_with_hp(
        EnemyId(0),
        Pos {
            x: pos.x + 1,
            y: pos.y,
        },
        5,
    )];

    let events = update(&mut state, &[], 0);
    assert!(events.iter().any(|e| matches!(e, GameEvent::HeroDied)));
    assert_eq!(state.hero.health_halves, 0);

    for tick in 1..10 {
        let events = update(&mut state, &[], tick);
        assert!(
            !events.iter().any(|e| matches!(e, GameEvent::HeroDied)),
            "HeroDied must fire once, not every tick at zero health: tick {tick}, {events:?}"
        );
    }
}

#[test]
fn an_enemy_at_zero_hp_emits_enemy_killed_and_stops_acting() {
    let mut state = fresh();
    let pos = Pos { x: 10, y: 5 };
    state.hero.pos = pos;
    state.hero.facing = Facing::North;
    let target = target_tile(pos, Facing::North);
    state.enemies = vec![slime_with_hp(EnemyId(0), target, 1)];

    let events = update(&mut state, &[Action::Attack], 0);
    assert!(events
        .iter()
        .any(|e| matches!(e, GameEvent::EnemyKilled { id, .. } if *id == EnemyId(0))));
    assert!(!state.enemies[0].alive);

    let pos_before = state.enemies[0].pos;
    let ai_before = state.enemies[0].ai;
    for tick in 1..50 {
        update(&mut state, &[], tick);
    }
    assert_eq!(
        state.enemies[0].pos, pos_before,
        "a dead enemy must not move"
    );
    assert_eq!(
        state.enemies[0].ai, ai_before,
        "a dead enemy must not change AI state"
    );
}

//! Content validity (spec §7, §13): the real world validates, and every deliberately broken
//! fixture is rejected with the specific error it was built to trigger.

use std::collections::HashSet;
use std::fs;

use mosslight::content::{self, ContentError, EnemyKind, LockKind, PuzzleKind, Reward, RoomKind};

fn fixture(name: &str) -> String {
    fs::read_to_string(format!("tests/fixtures/{name}")).unwrap_or_else(|e| {
        panic!("failed to read tests/fixtures/{name}: {e}");
    })
}

#[test]
fn real_world_validates() {
    let world = content::load().expect("assets/world.ron must validate");
    // 9 overworld rooms (see `real_world_has_nine_rooms_in_a_3x3_grid`) plus the phase 5 six-room
    // dungeon (room.sanctuary_gate, flooded_hall, plate_chamber, torch_vault, warden_walk,
    // boss_arena).
    assert_eq!(world.rooms.len(), 15);
    // The acceptance criterion names `content::validate` itself, not just `load` (which already
    // implies it via `loader::parse` -> `collect_errors`) — exercise the named entry point too.
    assert!(content::validate(&world).is_ok());
}

/// The whole §2 content table asserted against the embedded world in one place, so it stops
/// being something a human has to re-count by hand: 9 overworld + 6 dungeon rooms, 3 NPCs, all 3
/// regular enemy kinds plus exactly 1 boss, both required items, both puzzle kinds, at least 3
/// secrets, and at least 2 `HeartContainer`s (so 5 hearts — `state.rs`'s cap — is actually
/// reachable).
#[test]
fn the_embedded_world_meets_the_spec_content_table() {
    let world = content::load().expect("assets/world.ron must validate");

    let overworld = world
        .rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Overworld)
        .count();
    let dungeon = world
        .rooms
        .iter()
        .filter(|r| r.kind == RoomKind::Dungeon)
        .count();
    assert_eq!(overworld, 9, "spec §2 wants 9 overworld rooms");
    assert_eq!(dungeon, 6, "spec §2 wants 6 dungeon rooms");

    let npc_count: usize = world.rooms.iter().map(|r| r.npcs.len()).sum();
    assert_eq!(npc_count, 3, "spec §2 wants exactly 3 NPCs");

    let mut has_slime = false;
    let mut has_bat = false;
    let mut has_guardian = false;
    let mut boss_count = 0;
    for spawn in world.rooms.iter().flat_map(|r| r.enemies.iter()) {
        match spawn.kind {
            EnemyKind::Slime => has_slime = true,
            EnemyKind::Bat => has_bat = true,
            EnemyKind::Guardian => has_guardian = true,
            EnemyKind::Boss => boss_count += 1,
        }
    }
    assert!(
        has_slime && has_bat && has_guardian,
        "spec §2 wants all 3 regular enemy kinds"
    );
    assert_eq!(boss_count, 1, "spec §2 wants exactly 1 boss");

    let chests: Vec<_> = world.rooms.iter().flat_map(|r| r.chests.iter()).collect();
    assert!(
        chests.iter().any(|c| c.contains == Reward::Sword),
        "spec §2 wants a sword"
    );
    assert!(
        chests.iter().any(|c| c.contains == Reward::Lantern),
        "spec §2 wants a lantern"
    );
    let heart_containers = chests
        .iter()
        .filter(|c| c.contains == Reward::HeartContainer)
        .count();
    assert!(
        heart_containers >= 2,
        "spec §2's 5-heart maximum needs at least 2 HeartContainers on top of the starting 3 \
         hearts, found {heart_containers}"
    );

    let secret_count = chests.iter().filter(|c| c.secret).count();
    assert!(secret_count >= 3, "spec §2 wants at least 3 secrets");

    let mut has_block_on_plates = false;
    let mut has_torch_sequence = false;
    for puzzle in world.rooms.iter().flat_map(|r| r.puzzles.iter()) {
        match puzzle.kind {
            PuzzleKind::BlockOnPlates => has_block_on_plates = true,
            PuzzleKind::TorchSequence => has_torch_sequence = true,
            PuzzleKind::StepPlates => {}
        }
    }
    assert!(
        has_block_on_plates && has_torch_sequence,
        "spec §6 wants both a block-on-plates and a torch-sequence puzzle"
    );
}

#[test]
fn real_world_has_nine_rooms_in_a_3x3_grid() {
    let world = content::load().expect("assets/world.ron must validate");
    let indices: HashSet<(u8, u8)> = world.rooms.iter().filter_map(|r| r.map_index).collect();
    assert_eq!(indices.len(), 9, "expected 9 distinct map_index values");
    for col in 0..3u8 {
        for row in 0..3u8 {
            assert!(
                indices.contains(&(col, row)),
                "missing map_index ({col}, {row})"
            );
        }
    }
}

#[test]
fn base_fixture_validates() {
    let world = content::parse(&fixture("base.ron"));
    assert!(
        world.is_ok(),
        "base.ron is the valid source every broken fixture is derived from: {:?}",
        world.err()
    );
}

#[test]
fn missing_door_target_is_rejected() {
    let errors =
        content::parse(&fixture("broken_missing_door_target.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::UnknownRoom { room, .. } if room == "room.nonexistent"
        )),
        "{errors:?}"
    );
}

#[test]
fn non_reciprocal_door_is_rejected() {
    let errors =
        content::parse(&fixture("broken_non_reciprocal.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::NonReciprocalDoor { door, .. } if door == "door.a.toB"
        )),
        "{errors:?}"
    );
}

#[test]
fn spawn_in_wall_is_rejected() {
    let errors =
        content::parse(&fixture("broken_spawn_in_wall.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::SpawnNotWalkable { spawn, .. } if spawn == "spawn.a.fromB"
        )),
        "{errors:?}"
    );
}

#[test]
fn enemy_spawn_in_wall_is_rejected() {
    let errors = content::parse(&fixture("broken_enemy_spawn.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::EnemySpawnNotWalkable { room, .. } if room == "room.a"
        )),
        "{errors:?}"
    );
}

#[test]
fn duplicate_id_is_rejected() {
    let errors = content::parse(&fixture("broken_duplicate_id.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::DuplicateId { id, .. } if id == "door.b.toC"
        )),
        "{errors:?}"
    );
}

#[test]
fn wrong_dimensions_is_rejected() {
    let errors = content::parse(&fixture("broken_dimensions.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::RoomDimensions { room, .. } if room == "room.c"
        )),
        "{errors:?}"
    );
}

#[test]
fn illegal_tile_is_rejected() {
    let errors = content::parse(&fixture("broken_illegal_tile.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::IllegalTile { room, ch, .. } if room == "room.c" && *ch == 'z'
        )),
        "{errors:?}"
    );
}

#[test]
fn key_behind_its_own_lock_is_rejected() {
    let errors =
        content::parse(&fixture("broken_key_behind_lock.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::LockNeverUnlockable { door, lock: LockKind::SmallKey } if door == "door.b.toC"
        )),
        "{errors:?}"
    );
}

#[test]
fn ember_unreachable_is_rejected() {
    let errors =
        content::parse(&fixture("broken_ember_unreachable.ron")).expect_err("must be rejected");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ContentError::EmberUnreachable)),
        "{errors:?}"
    );
}

/// Round-1 review, major: walls in `spawn.b.fromA` behind a floor-then-wall pocket so the hero
/// arriving there can never walk to `door.b.toC`, even though the door graph alone (pre-fix) saw
/// nothing wrong. Also proves the resulting orphaned `room.c` is reported as `RoomUnreachable`.
#[test]
fn walled_in_spawn_is_rejected() {
    let errors =
        content::parse(&fixture("broken_walled_in_spawn.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::DoorUnreachableInRoom { room, door, .. }
                if room == "room.b" && door == "door.b.toC"
        )),
        "missing DoorUnreachableInRoom: {errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ContentError::RoomUnreachable { room } if room == "room.c")),
        "missing cascading RoomUnreachable: {errors:?}"
    );
}

/// Round-1 review, major: renaming `lock:` to `lockk:` used to be silently accepted (the field
/// just vanished, along with the lock it authored) because no content struct rejected unknown
/// fields. `deny_unknown_fields` turns that into a parse error instead.
#[test]
fn unknown_field_is_rejected() {
    let errors =
        content::parse(&fixture("broken_unknown_field.ron")).expect_err("must be rejected");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ContentError::Parse { .. })),
        "{errors:?}"
    );
}

#[test]
fn three_defects_are_all_reported() {
    let errors =
        content::parse(&fixture("broken_three_defects.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::DuplicateId { id, .. } if id == "door.b.toC"
        )),
        "missing DuplicateId: {errors:?}"
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::UnknownRoom { room, .. } if room == "room.nonexistent"
        )),
        "missing UnknownRoom: {errors:?}"
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::SpawnNotWalkable { spawn, .. } if spawn == "spawn.a.fromB"
        )),
        "missing SpawnNotWalkable: {errors:?}"
    );
}

#[test]
fn object_on_a_wall_is_rejected() {
    let errors =
        content::parse(&fixture("broken_object_on_wall.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::ObjectNotOnFloor { what, .. } if what.contains("chest.key")
        )),
        "{errors:?}"
    );
}

#[test]
fn two_objects_on_one_tile_is_rejected() {
    let errors =
        content::parse(&fixture("broken_object_tile_conflict.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(
            |e| matches!(e, ContentError::ObjectTileConflict { room, .. } if room == "room.a")
        ),
        "{errors:?}"
    );
}

#[test]
fn unknown_plate_id_is_rejected() {
    let errors =
        content::parse(&fixture("broken_unknown_plate.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::UnknownPlate { plate, .. } if plate == "plate.missing"
        )),
        "{errors:?}"
    );
}

#[test]
fn reveal_position_not_hidden_is_rejected() {
    let errors =
        content::parse(&fixture("broken_reveal_not_hidden.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(
            |e| matches!(e, ContentError::RevealNotHidden { what, .. } if what.contains("torch.a"))
        ),
        "{errors:?}"
    );
}

#[test]
fn flag_never_set_is_rejected() {
    let errors =
        content::parse(&fixture("broken_flag_never_set.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::FlagNeverSet { flag } if flag == "flag.ghost"
        )),
        "{errors:?}"
    );
}

#[test]
fn secret_on_the_main_route_is_rejected() {
    let errors =
        content::parse(&fixture("broken_secret_on_main_route.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::SecretOnMainRoute { chest } if chest == "chest.secret"
        )),
        "{errors:?}"
    );
}

#[test]
fn secret_holding_a_route_critical_reward_is_rejected() {
    let errors =
        content::parse(&fixture("broken_secret_route_critical.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::SecretRouteCritical { chest } if chest == "chest.secret"
        )),
        "{errors:?}"
    );
}

#[test]
fn unknown_route_goal_is_rejected() {
    let errors = content::parse(&fixture("broken_unknown_goal.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::UnknownRoom { referenced_by, room }
                if referenced_by == "route.goal" && room == "room.nonexistent"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_block_puzzle_naming_no_block_is_rejected() {
    let errors =
        content::parse(&fixture("broken_block_puzzle_shape.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::BlockPuzzleShape { puzzle, .. } if puzzle == "puzzle.a"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_torch_sequence_naming_unknown_torches_is_rejected() {
    let errors = content::parse(&fixture("broken_torch_sequence_unknown.ron"))
        .expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::TorchSequenceShape { puzzle, .. } if puzzle == "puzzle.a"
        )),
        "{errors:?}"
    );
}

#[test]
fn boss_only_fields_on_a_regular_enemy_are_rejected() {
    let errors =
        content::parse(&fixture("broken_boss_field_on_slime.ron")).expect_err("must be rejected");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ContentError::BossFieldOnRegularEnemy { .. })),
        "{errors:?}"
    );
}

#[test]
fn a_beacon_outside_route_home_is_rejected() {
    let errors =
        content::parse(&fixture("broken_beacon_not_at_home.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::BeaconNotAtHome { room } if room == "room.b"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_block_puzzle_with_no_pushable_plate_is_rejected() {
    let errors =
        content::parse(&fixture("broken_block_unsolvable.ron")).expect_err("must be rejected");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            ContentError::BlockPuzzleUnsolvable { puzzle, .. } if puzzle == "puzzle.a"
        )),
        "{errors:?}"
    );
}

#[test]
fn an_ember_behind_an_unreachable_boss_is_rejected() {
    let errors = content::parse(&fixture("broken_ember_behind_unreachable_boss.ron"))
        .expect_err("must be rejected");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ContentError::EmberUnreachable)),
        "{errors:?}"
    );
}

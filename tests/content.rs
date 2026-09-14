//! Content validity (spec §7, §13): the real world validates, and every deliberately broken
//! fixture is rejected with the specific error it was built to trigger.

use std::collections::HashSet;
use std::fs;

use mosslight::content::{self, ContentError, LockKind};

fn fixture(name: &str) -> String {
    fs::read_to_string(format!("tests/fixtures/{name}")).unwrap_or_else(|e| {
        panic!("failed to read tests/fixtures/{name}: {e}");
    })
}

#[test]
fn real_world_validates() {
    let world = content::load().expect("assets/world.ron must validate");
    assert_eq!(world.rooms.len(), 9);
    // The acceptance criterion names `content::validate` itself, not just `load` (which already
    // implies it via `loader::parse` -> `collect_errors`) — exercise the named entry point too.
    assert!(content::validate(&world).is_ok());
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

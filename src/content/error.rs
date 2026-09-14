//! One variant per §7 validity rule. Every variant carries the ids needed to fix the file, and
//! `Display` prints one readable line so `report` can be handed straight to stderr.

use std::fmt;

use crate::game::entities::Pos;

use super::schema::{LockKind, Tile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    Room,
    Door,
    Spawn,
    Chest,
    Npc,
    Puzzle,
}

impl fmt::Display for IdKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IdKind::Room => "room",
            IdKind::Door => "door",
            IdKind::Spawn => "spawn",
            IdKind::Chest => "chest",
            IdKind::Npc => "npc",
            IdKind::Puzzle => "puzzle",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentError {
    Parse {
        message: String,
    },
    RoomDimensions {
        room: String,
        expected: (usize, usize),
        found: (usize, usize),
    },
    IllegalTile {
        room: String,
        at: Pos,
        ch: char,
    },
    DuplicateId {
        kind: IdKind,
        id: String,
    },
    UnknownRoom {
        referenced_by: String,
        room: String,
    },
    UnknownSpawn {
        referenced_by: String,
        room: String,
        spawn: String,
    },
    PositionOutOfBounds {
        room: String,
        what: String,
        at: Pos,
    },
    SpawnNotWalkable {
        room: String,
        spawn: String,
        at: Pos,
        tile: Tile,
    },
    SpawnOnDoorTile {
        room: String,
        spawn: String,
        at: Pos,
    },
    DoorTileMismatch {
        room: String,
        at: Pos,
    },
    NonReciprocalDoor {
        door: String,
        to_room: String,
    },
    LockNeverUnlockable {
        door: String,
        lock: LockKind,
    },
    RoomUnreachable {
        room: String,
    },
    EmberUnreachable,
    HomeUnreachableWithEmber {
        home: String,
    },
    EmberMissing,
    DuplicateMapIndex {
        at: (u8, u8),
    },
    DoorUnreachableInRoom {
        room: String,
        door: String,
        from_spawn: String,
    },
    TooManySmallKeyDoors {
        count: usize,
    },
    EnemySpawnNotWalkable {
        room: String,
        at: Pos,
    },
    EnemyPatrolInvalid {
        room: String,
        at: Pos,
    },
}

impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContentError::Parse { message } => write!(f, "parse error: {message}"),
            ContentError::RoomDimensions {
                room,
                expected,
                found,
            } => write!(
                f,
                "{room}: expected {}x{} rows, found {}x{}",
                expected.0, expected.1, found.0, found.1
            ),
            ContentError::IllegalTile { room, at, ch } => {
                write!(f, "{room}: illegal tile {ch:?} at ({}, {})", at.x, at.y)
            }
            ContentError::DuplicateId { kind, id } => {
                write!(f, "duplicate {kind} id '{id}'")
            }
            ContentError::UnknownRoom {
                referenced_by,
                room,
            } => write!(f, "{referenced_by}: references unknown room '{room}'"),
            ContentError::UnknownSpawn {
                referenced_by,
                room,
                spawn,
            } => write!(
                f,
                "{referenced_by}: references unknown spawn '{spawn}' in room '{room}'"
            ),
            ContentError::PositionOutOfBounds { room, what, at } => {
                write!(f, "{room}: {what} at ({}, {}) is out of bounds", at.x, at.y)
            }
            ContentError::SpawnNotWalkable {
                room,
                spawn,
                at,
                tile,
            } => write!(
                f,
                "{room}: spawn '{spawn}' at ({}, {}) is not walkable ({tile:?})",
                at.x, at.y
            ),
            ContentError::SpawnOnDoorTile { room, spawn, at } => write!(
                f,
                "{room}: spawn '{spawn}' at ({}, {}) sits on a door tile",
                at.x, at.y
            ),
            ContentError::DoorTileMismatch { room, at } => write!(
                f,
                "{room}: door tile / door entry mismatch at ({}, {})",
                at.x, at.y
            ),
            ContentError::NonReciprocalDoor { door, to_room } => write!(
                f,
                "door '{door}' to room '{to_room}' has no matching reciprocal door"
            ),
            ContentError::LockNeverUnlockable { door, lock } => {
                write!(f, "door '{door}' lock {lock:?} can never be unlocked")
            }
            ContentError::RoomUnreachable { room } => {
                write!(f, "room '{room}' is unreachable from the start")
            }
            ContentError::EmberUnreachable => write!(f, "the ember is unreachable from the start"),
            ContentError::HomeUnreachableWithEmber { home } => write!(
                f,
                "home room '{home}' is unreachable after collecting the ember"
            ),
            ContentError::EmberMissing => {
                write!(
                    f,
                    "route.ember_required is true but no chest grants the ember"
                )
            }
            ContentError::DuplicateMapIndex { at } => {
                write!(f, "duplicate overworld map_index ({}, {})", at.0, at.1)
            }
            ContentError::DoorUnreachableInRoom {
                room,
                door,
                from_spawn,
            } => write!(
                f,
                "{room}: door '{door}' cannot be reached on foot from spawn(s) {from_spawn}"
            ),
            ContentError::TooManySmallKeyDoors { count } => write!(
                f,
                "{count} SmallKey-locked doors exceeds the reachability search's 64-door limit"
            ),
            ContentError::EnemySpawnNotWalkable { room, at } => write!(
                f,
                "{room}: enemy spawn at ({}, {}) is not walkable",
                at.x, at.y
            ),
            ContentError::EnemyPatrolInvalid { room, at } => write!(
                f,
                "{room}: enemy patrol waypoint at ({}, {}) is not walkable",
                at.x, at.y
            ),
        }
    }
}

/// Renders one error per line, in the order given (`parse`, then `decode`, then `validate` — see
/// `loader::parse`), for a startup refusal or a test assertion.
pub fn report(errors: &[ContentError]) -> String {
    errors
        .iter()
        .map(ContentError::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_variants() -> Vec<ContentError> {
        vec![
            ContentError::Parse {
                message: "bad ron".into(),
            },
            ContentError::RoomDimensions {
                room: "room.a".into(),
                expected: (24, 16),
                found: (1, 1),
            },
            ContentError::IllegalTile {
                room: "room.a".into(),
                at: Pos { x: 1, y: 2 },
                ch: 'x',
            },
            ContentError::DuplicateId {
                kind: IdKind::Room,
                id: "room.a".into(),
            },
            ContentError::UnknownRoom {
                referenced_by: "door.a.north".into(),
                room: "room.missing".into(),
            },
            ContentError::UnknownSpawn {
                referenced_by: "door.a.north".into(),
                room: "room.a".into(),
                spawn: "spawn.missing".into(),
            },
            ContentError::PositionOutOfBounds {
                room: "room.a".into(),
                what: "spawn 'spawn.a'".into(),
                at: Pos { x: 99, y: 99 },
            },
            ContentError::SpawnNotWalkable {
                room: "room.a".into(),
                spawn: "spawn.a".into(),
                at: Pos { x: 3, y: 7 },
                tile: Tile::Wall,
            },
            ContentError::SpawnOnDoorTile {
                room: "room.a".into(),
                spawn: "spawn.a".into(),
                at: Pos { x: 3, y: 7 },
            },
            ContentError::DoorTileMismatch {
                room: "room.a".into(),
                at: Pos { x: 3, y: 7 },
            },
            ContentError::NonReciprocalDoor {
                door: "door.a.north".into(),
                to_room: "room.b".into(),
            },
            ContentError::LockNeverUnlockable {
                door: "door.a.north".into(),
                lock: LockKind::SmallKey,
            },
            ContentError::RoomUnreachable {
                room: "room.orphan".into(),
            },
            ContentError::EmberUnreachable,
            ContentError::HomeUnreachableWithEmber {
                home: "room.lighthouse".into(),
            },
            ContentError::EmberMissing,
            ContentError::DuplicateMapIndex { at: (1, 1) },
            ContentError::DoorUnreachableInRoom {
                room: "room.b".into(),
                door: "door.b.toC".into(),
                from_spawn: "spawn.b.fromA".into(),
            },
            ContentError::TooManySmallKeyDoors { count: 65 },
            ContentError::EnemySpawnNotWalkable {
                room: "room.a".into(),
                at: Pos { x: 3, y: 7 },
            },
            ContentError::EnemyPatrolInvalid {
                room: "room.a".into(),
                at: Pos { x: 3, y: 7 },
            },
        ]
    }

    #[test]
    fn display_mentions_the_offending_id() {
        for err in all_variants() {
            let text = err.to_string();
            let needle = match &err {
                ContentError::Parse { message } => message.clone(),
                ContentError::RoomDimensions { room, .. } => room.clone(),
                ContentError::IllegalTile { room, .. } => room.clone(),
                ContentError::DuplicateId { id, .. } => id.clone(),
                ContentError::UnknownRoom { room, .. } => room.clone(),
                ContentError::UnknownSpawn { spawn, .. } => spawn.clone(),
                ContentError::PositionOutOfBounds { room, .. } => room.clone(),
                ContentError::SpawnNotWalkable { spawn, .. } => spawn.clone(),
                ContentError::SpawnOnDoorTile { spawn, .. } => spawn.clone(),
                ContentError::DoorTileMismatch { room, .. } => room.clone(),
                ContentError::NonReciprocalDoor { door, .. } => door.clone(),
                ContentError::LockNeverUnlockable { door, .. } => door.clone(),
                ContentError::RoomUnreachable { room } => room.clone(),
                ContentError::EmberUnreachable => "ember".into(),
                ContentError::HomeUnreachableWithEmber { home } => home.clone(),
                ContentError::EmberMissing => "ember_required".into(),
                ContentError::DuplicateMapIndex { .. } => "map_index".into(),
                ContentError::DoorUnreachableInRoom { door, .. } => door.clone(),
                ContentError::TooManySmallKeyDoors { .. } => "64-door limit".into(),
                ContentError::EnemySpawnNotWalkable { room, .. } => room.clone(),
                ContentError::EnemyPatrolInvalid { room, .. } => room.clone(),
            };
            if needle == "ember_required" {
                assert!(text.contains("ember_required"), "{text}");
            } else if needle == "map_index" {
                assert!(text.contains("map_index"), "{text}");
            } else if needle == "64-door limit" {
                assert!(text.contains("64-door limit"), "{text}");
            } else {
                assert!(text.contains(&needle), "{text} did not mention {needle}");
            }
        }
    }

    #[test]
    fn report_joins_one_line_per_error() {
        let errors = vec![
            ContentError::EmberMissing,
            ContentError::RoomUnreachable {
                room: "room.orphan".into(),
            },
        ];
        let text = report(&errors);
        assert_eq!(text.lines().count(), 2);
    }
}

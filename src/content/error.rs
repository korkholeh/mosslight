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
    Torch,
    Plate,
    Block,
    Beacon,
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
            IdKind::Torch => "torch",
            IdKind::Plate => "plate",
            IdKind::Block => "block",
            IdKind::Beacon => "beacon",
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
    ObjectNotOnFloor {
        room: String,
        what: String,
        at: Pos,
    },
    ObjectTileConflict {
        room: String,
        at: Pos,
    },
    FlagNeverSet {
        flag: String,
    },
    UnknownPlate {
        room: String,
        puzzle: String,
        plate: String,
    },
    RevealNotHidden {
        room: String,
        what: String,
        at: Pos,
    },
    SecretOnMainRoute {
        chest: String,
    },
    SecretRouteCritical {
        chest: String,
    },
    /// A `BlockOnPlates` puzzle does not name exactly one block, or names no plates.
    BlockPuzzleShape {
        room: String,
        puzzle: String,
    },
    /// A `TorchSequence` puzzle names fewer than two torches, a duplicate, an unknown torch, or a
    /// torch that also carries its own `reveals` (the puzzle owns the reveal instead).
    TorchSequenceShape {
        room: String,
        puzzle: String,
    },
    /// A torch or block is named by more than one puzzle in its room.
    PuzzleObjectClaimedTwice {
        room: String,
        what: String,
    },
    /// The one-block push search found no path from a `BlockOnPlates` puzzle's block to any of
    /// its plates.
    BlockPuzzleUnsolvable {
        room: String,
        puzzle: String,
    },
    /// `EnemySpawn::{drops,defeat_flag}` is set on a spawn that is not `EnemyKind::Boss`.
    BossFieldOnRegularEnemy {
        room: String,
    },
    /// A boss spawn drops a reward but carries no `defeat_flag` to record that it was collected.
    BossDropMissingFlag {
        room: String,
    },
    /// More than one `EnemyKind::Boss` spawn exists world-wide.
    MultipleBosses {
        count: usize,
    },
    /// No `Beacon` is authored anywhere, though the game needs exactly one ending object.
    BeaconMissing,
    /// A beacon exists but not in `route.home`.
    BeaconNotAtHome {
        room: String,
    },
    /// More than one `Beacon` is authored world-wide.
    MultipleBeacons {
        count: usize,
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
            ContentError::ObjectNotOnFloor { room, what, at } => write!(
                f,
                "{room}: {what} at ({}, {}) must sit on plain floor, clear of spawns and enemies",
                at.x, at.y
            ),
            ContentError::ObjectTileConflict { room, at } => write!(
                f,
                "{room}: more than one object occupies ({}, {})",
                at.x, at.y
            ),
            ContentError::FlagNeverSet { flag } => {
                write!(f, "flag '{flag}' is never set by any dialogue node")
            }
            ContentError::UnknownPlate {
                room,
                puzzle,
                plate,
            } => write!(
                f,
                "{room}: puzzle '{puzzle}' references unknown plate '{plate}'"
            ),
            ContentError::RevealNotHidden { room, what, at } => write!(
                f,
                "{room}: {what} reveals ({}, {}), which is not a Hidden tile",
                at.x, at.y
            ),
            ContentError::SecretOnMainRoute { chest } => {
                write!(f, "secret chest '{chest}' sits in a room on the main route")
            }
            ContentError::SecretRouteCritical { chest } => {
                write!(f, "secret chest '{chest}' contains a route-critical reward")
            }
            ContentError::BlockPuzzleShape { room, puzzle } => write!(
                f,
                "{room}: puzzle '{puzzle}' must name exactly one block and at least one plate"
            ),
            ContentError::TorchSequenceShape { room, puzzle } => write!(
                f,
                "{room}: puzzle '{puzzle}' must name at least two distinct torches, none of them self-revealing"
            ),
            ContentError::PuzzleObjectClaimedTwice { room, what } => write!(
                f,
                "{room}: {what} is claimed by more than one puzzle"
            ),
            ContentError::BlockPuzzleUnsolvable { room, puzzle } => write!(
                f,
                "{room}: puzzle '{puzzle}' has no push path from its block to any of its plates"
            ),
            ContentError::BossFieldOnRegularEnemy { room } => write!(
                f,
                "{room}: an enemy spawn has 'drops' or 'defeat_flag' set but is not a Boss"
            ),
            ContentError::BossDropMissingFlag { room } => write!(
                f,
                "{room}: a boss spawn drops a reward but has no defeat_flag"
            ),
            ContentError::MultipleBosses { count } => {
                write!(f, "{count} Boss spawns exist; exactly one is allowed")
            }
            ContentError::BeaconMissing => write!(f, "no beacon is authored anywhere"),
            ContentError::BeaconNotAtHome { room } => {
                write!(f, "beacon in '{room}' does not sit in route.home")
            }
            ContentError::MultipleBeacons { count } => {
                write!(f, "{count} beacons exist; exactly one is allowed")
            }
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
            ContentError::ObjectNotOnFloor {
                room: "room.a".into(),
                what: "chest 'chest.a'".into(),
                at: Pos { x: 3, y: 7 },
            },
            ContentError::ObjectTileConflict {
                room: "room.a".into(),
                at: Pos { x: 3, y: 7 },
            },
            ContentError::FlagNeverSet {
                flag: "flag.told_about_sanctuary".into(),
            },
            ContentError::UnknownPlate {
                room: "room.a".into(),
                puzzle: "puzzle.a".into(),
                plate: "plate.missing".into(),
            },
            ContentError::RevealNotHidden {
                room: "room.a".into(),
                what: "torch 'torch.a'".into(),
                at: Pos { x: 3, y: 7 },
            },
            ContentError::SecretOnMainRoute {
                chest: "chest.a".into(),
            },
            ContentError::SecretRouteCritical {
                chest: "chest.a".into(),
            },
            ContentError::BlockPuzzleShape {
                room: "room.a".into(),
                puzzle: "puzzle.a".into(),
            },
            ContentError::TorchSequenceShape {
                room: "room.a".into(),
                puzzle: "puzzle.a".into(),
            },
            ContentError::PuzzleObjectClaimedTwice {
                room: "room.a".into(),
                what: "torch 'torch.a'".into(),
            },
            ContentError::BlockPuzzleUnsolvable {
                room: "room.a".into(),
                puzzle: "puzzle.a".into(),
            },
            ContentError::BossFieldOnRegularEnemy {
                room: "room.a".into(),
            },
            ContentError::BossDropMissingFlag {
                room: "room.a".into(),
            },
            ContentError::MultipleBosses { count: 2 },
            ContentError::BeaconMissing,
            ContentError::BeaconNotAtHome {
                room: "room.a".into(),
            },
            ContentError::MultipleBeacons { count: 2 },
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
                ContentError::ObjectNotOnFloor { what, .. } => what.clone(),
                ContentError::ObjectTileConflict { room, .. } => room.clone(),
                ContentError::FlagNeverSet { flag } => flag.clone(),
                ContentError::UnknownPlate { plate, .. } => plate.clone(),
                ContentError::RevealNotHidden { what, .. } => what.clone(),
                ContentError::SecretOnMainRoute { chest } => chest.clone(),
                ContentError::SecretRouteCritical { chest } => chest.clone(),
                ContentError::BlockPuzzleShape { puzzle, .. } => puzzle.clone(),
                ContentError::TorchSequenceShape { puzzle, .. } => puzzle.clone(),
                ContentError::PuzzleObjectClaimedTwice { room, .. } => room.clone(),
                ContentError::BlockPuzzleUnsolvable { puzzle, .. } => puzzle.clone(),
                ContentError::BossFieldOnRegularEnemy { room } => room.clone(),
                ContentError::BossDropMissingFlag { room } => room.clone(),
                ContentError::MultipleBosses { .. } => "Boss spawns".into(),
                ContentError::BeaconMissing => "beacon".into(),
                ContentError::BeaconNotAtHome { room } => room.clone(),
                ContentError::MultipleBeacons { .. } => "beacons".into(),
            };
            if needle == "ember_required" {
                assert!(text.contains("ember_required"), "{text}");
            } else if needle == "map_index" {
                assert!(text.contains("map_index"), "{text}");
            } else if needle == "64-door limit" {
                assert!(text.contains("64-door limit"), "{text}");
            } else if needle == "Boss spawns" {
                assert!(text.contains("Boss spawns"), "{text}");
            } else if needle == "beacons" {
                assert!(text.contains("beacons"), "{text}");
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

//! Authored (RON) and runtime content types (spec §7, §8).
//!
//! A room's tiles are authored as 16 character rows and decoded into a fixed grid by the loader
//! (`ContentError::RoomDimensions`/`IllegalTile` guard the decode); every other authored type
//! mirrors its runtime shape exactly, so there is no separate "authoring" struct family.

use serde::Deserialize;

use crate::game::entities::Pos;
use crate::game::tuning::{ROOM_H, ROOM_W};

/// A room's decoded tile grid: `tiles[y][x]`.
pub type TileGrid = [[Tile; ROOM_W]; ROOM_H];

fn empty_tile_grid() -> TileGrid {
    [[Tile::Wall; ROOM_W]; ROOM_H]
}

/// Dense index into `World::rooms`, interned by the loader from a room's string id. Runtime code
/// (and `GameState::room`) uses this instead of the string id; only durability boundaries such as
/// a future save file keep using the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct RoomIdx(pub u16);

/// One character cell of a room (spec §7's tile table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Tile {
    Floor,
    Wall,
    Water,
    Bush,
    Door,
    Stairs,
    Pit,
    Hidden,
}

impl Tile {
    /// The single char -> tile table (spec §7). Any other character is illegal content.
    pub fn from_char(c: char) -> Option<Tile> {
        match c {
            '.' => Some(Tile::Floor),
            '#' => Some(Tile::Wall),
            '~' => Some(Tile::Water),
            '"' => Some(Tile::Bush),
            '+' => Some(Tile::Door),
            '>' => Some(Tile::Stairs),
            'v' => Some(Tile::Pit),
            '?' => Some(Tile::Hidden),
            _ => None,
        }
    }

    /// The tile's authored character; the inverse of `from_char`.
    pub fn to_char(self) -> char {
        match self {
            Tile::Floor => '.',
            Tile::Wall => '#',
            Tile::Water => '~',
            Tile::Bush => '"',
            Tile::Door => '+',
            Tile::Stairs => '>',
            Tile::Pit => 'v',
            Tile::Hidden => '?',
        }
    }

    /// Whether the hero can step onto this tile, ignoring reveals (spec §6). `Hidden` is always
    /// `false` here — this is the content-level predicate the validator's geometry checks use, and
    /// content has no notion of which torch is lit. The simulation instead walks through
    /// `GameState::walkable`, which treats a revealed `Hidden` tile as walkable too.
    pub fn is_walkable(self) -> bool {
        matches!(self, Tile::Floor | Tile::Door | Tile::Stairs)
    }

    /// Whether standing on this tile is dangerous (spec §6/§7).
    pub fn is_hazard(self) -> bool {
        matches!(self, Tile::Water | Tile::Pit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum RoomKind {
    Overworld,
    Dungeon,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum LockKind {
    SmallKey,
    Lantern,
    Flag(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum Reward {
    Sword,
    Lantern,
    SmallKey,
    HeartContainer,
    Ember,
    Message(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum PuzzleKind {
    StepPlates,
    BlockOnPlates,
    TorchSequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EnemyKind {
    Slime,
    Bat,
    Guardian,
    Boss,
}

fn default_two_way() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Door {
    pub id: String,
    pub at: Pos,
    pub to_room: String,
    pub to_spawn: String,
    #[serde(default)]
    pub lock: Option<LockKind>,
    #[serde(default = "default_two_way")]
    pub two_way: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub id: String,
    pub at: Pos,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chest {
    pub id: String,
    pub at: Pos,
    pub contains: Reward,
    /// A secret reward: off the main route, never route-critical (validator-enforced).
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueNode {
    pub text: String,
    /// Set on the tick this node becomes the shown node.
    #[serde(default)]
    pub sets_flag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Npc {
    pub id: String,
    pub at: Pos,
    #[serde(default)]
    pub dialogue: Vec<DialogueNode>,
    /// The NPC only talks once this flag is set; `None` means always.
    #[serde(default)]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Torch {
    pub id: String,
    pub at: Pos,
    /// `Tile::Hidden` positions in this room that become walkable once this torch is lit.
    #[serde(default)]
    pub reveals: Vec<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plate {
    pub id: String,
    pub at: Pos,
}

/// A pushable block's authored reset position (spec §7). Its live position is mutable simulation
/// state (`game::puzzles::PuzzleState.blocks`), not an authored placement, so it is not an
/// `ObjectKind` — see `Room::block_at`/`block_index`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: String,
    pub at: Pos,
}

/// The lighthouse brazier the ember relights (spec §2/§7). An ordinary solid object, interacted
/// with exactly like a chest.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beacon {
    pub id: String,
    pub at: Pos,
}

/// An authored object solid enough to block the hero and every enemy, plus its index within its
/// own `Room::{chests,npcs,torches,beacons}` vec. A `Plate` is authored separately
/// (`Room::plate_at`) — it is the one object kind that is not solid. A `Block` is not here either
/// (see `Block`'s doc comment).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Chest(u16),
    Npc(u16),
    Torch(u16),
    Beacon(u16),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemySpawn {
    pub kind: EnemyKind,
    pub at: Pos,
    #[serde(default)]
    pub patrol: Option<Vec<Pos>>,
    /// Boss only: the reward granted on defeat (typically `Ember`).
    #[serde(default)]
    pub drops: Option<Reward>,
    /// Boss only: the story flag set on defeat, so `enter_room` never respawns it.
    #[serde(default)]
    pub defeat_flag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Puzzle {
    pub id: String,
    pub kind: PuzzleKind,
    /// Plate ids that must all be pressed (`StepPlates`) or hold a block (`BlockOnPlates`).
    #[serde(default)]
    pub plates: Vec<String>,
    /// Block ids belonging to this puzzle: exactly one (`BlockOnPlates`, validator-enforced).
    #[serde(default)]
    pub blocks: Vec<String>,
    /// Torch ids naming the required lighting order (`TorchSequence`).
    #[serde(default)]
    pub torches: Vec<String>,
    /// `Tile::Hidden` positions revealed when the puzzle is solved.
    #[serde(default)]
    pub reveals: Vec<Pos>,
    #[serde(default)]
    pub reward: Option<Reward>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub kind: RoomKind,
    #[serde(default)]
    pub map_index: Option<(u8, u8)>,
    pub rows: Vec<String>,
    #[serde(skip, default = "empty_tile_grid")]
    pub tiles: TileGrid,
    #[serde(default)]
    pub doors: Vec<Door>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
    #[serde(default)]
    pub chests: Vec<Chest>,
    #[serde(default)]
    pub npcs: Vec<Npc>,
    #[serde(default)]
    pub enemies: Vec<EnemySpawn>,
    #[serde(default)]
    pub puzzles: Vec<Puzzle>,
    #[serde(default)]
    pub torches: Vec<Torch>,
    #[serde(default)]
    pub plates: Vec<Plate>,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub beacons: Vec<Beacon>,
    /// Shown in the message row on entry. The §7 teaching prompt of the first room.
    #[serde(default)]
    pub hint: Option<String>,
}

impl PartialEq for Room {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.kind == other.kind
            && self.map_index == other.map_index
            && self.rows == other.rows
            && self.tiles == other.tiles
            && self.doors == other.doors
            && self.spawns == other.spawns
            && self.chests == other.chests
            && self.npcs == other.npcs
            && self.enemies == other.enemies
            && self.puzzles == other.puzzles
            && self.torches == other.torches
            && self.plates == other.plates
            && self.blocks == other.blocks
            && self.beacons == other.beacons
            && self.hint == other.hint
    }
}

impl Room {
    pub fn tile_at(&self, pos: Pos) -> Option<Tile> {
        self.tiles
            .get(pos.y as usize)
            .and_then(|row| row.get(pos.x as usize))
            .copied()
    }

    pub fn spawn_at(&self, spawn_id: &str) -> Option<Pos> {
        self.spawns.iter().find(|s| s.id == spawn_id).map(|s| s.at)
    }

    pub fn door_at(&self, pos: Pos) -> Option<&Door> {
        self.doors.iter().find(|d| d.at == pos)
    }

    /// The solid authored object (chest, NPC, torch or beacon) at `pos`, if any.
    pub fn object_at(&self, pos: Pos) -> Option<ObjectKind> {
        if let Some(i) = self.chests.iter().position(|c| c.at == pos) {
            return Some(ObjectKind::Chest(i as u16));
        }
        if let Some(i) = self.npcs.iter().position(|n| n.at == pos) {
            return Some(ObjectKind::Npc(i as u16));
        }
        if let Some(i) = self.torches.iter().position(|t| t.at == pos) {
            return Some(ObjectKind::Torch(i as u16));
        }
        if let Some(i) = self.beacons.iter().position(|b| b.at == pos) {
            return Some(ObjectKind::Beacon(i as u16));
        }
        None
    }

    /// The index of the plate at `pos`, if any. Plates are not solid, so they are not part of
    /// `object_at`.
    pub fn plate_at(&self, pos: Pos) -> Option<u16> {
        self.plates
            .iter()
            .position(|p| p.at == pos)
            .map(|i| i as u16)
    }

    /// Index of the plate with the given authored id, if any.
    pub fn plate_index(&self, id: &str) -> Option<u16> {
        self.plates
            .iter()
            .position(|p| p.id == id)
            .map(|i| i as u16)
    }

    /// Index of the block with the given authored id, if any.
    pub fn block_index(&self, id: &str) -> Option<u16> {
        self.blocks
            .iter()
            .position(|b| b.id == id)
            .map(|i| i as u16)
    }

    /// Index of the block authored at `pos`, if any. A block's *live* position tracks
    /// `PuzzleState.blocks`, not this — this is only the authored reset position.
    pub fn block_at(&self, pos: Pos) -> Option<u16> {
        self.blocks
            .iter()
            .position(|b| b.at == pos)
            .map(|i| i as u16)
    }

    /// The index of the beacon at `pos`, if any.
    pub fn beacon_at(&self, pos: Pos) -> Option<u16> {
        self.beacons
            .iter()
            .position(|b| b.at == pos)
            .map(|i| i as u16)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartPoint {
    pub room: String,
    pub spawn: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub ember_required: bool,
    pub home: String,
    /// The deepest room the main route must reach. Phase 4: the dungeon vestibule.
    pub goal: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub version: u32,
    pub start: StartPoint,
    pub route: Route,
    pub rooms: Vec<Room>,
    #[serde(skip)]
    pub room_index: std::collections::HashMap<String, RoomIdx>,
}

impl World {
    pub fn room_idx(&self, id: &str) -> Option<RoomIdx> {
        self.room_index.get(id).copied()
    }

    pub fn room(&self, idx: RoomIdx) -> &Room {
        &self.rooms[idx.0 as usize]
    }

    pub fn spawn_pos(&self, idx: RoomIdx, spawn_id: &str) -> Option<Pos> {
        self.room(idx).spawn_at(spawn_id)
    }

    pub fn door_at(&self, idx: RoomIdx, at: Pos) -> Option<&Door> {
        self.room(idx).door_at(at)
    }

    pub fn start_room(&self) -> Option<RoomIdx> {
        self.room_idx(&self.start.room)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CHARS: [char; 8] = ['.', '#', '~', '"', '+', '>', 'v', '?'];

    #[test]
    fn every_char_maps_to_a_tile_and_back() {
        for c in ALL_CHARS {
            let tile = Tile::from_char(c).unwrap_or_else(|| panic!("no tile for {c:?}"));
            assert_eq!(tile.to_char(), c);
        }
    }

    #[test]
    fn illegal_char_maps_to_nothing() {
        assert_eq!(Tile::from_char('x'), None);
        assert_eq!(Tile::from_char(' '), None);
    }

    #[test]
    fn walkable_set_matches_the_table() {
        let walkable: Vec<Tile> = ALL_CHARS
            .iter()
            .filter_map(|&c| Tile::from_char(c))
            .filter(|t| t.is_walkable())
            .collect();
        assert_eq!(
            walkable,
            vec![Tile::Floor, Tile::Door, Tile::Stairs],
            "walkable set changed — update the §7 table comment alongside this test"
        );
    }

    #[test]
    fn hazard_set_matches_the_table() {
        let hazards: Vec<Tile> = ALL_CHARS
            .iter()
            .filter_map(|&c| Tile::from_char(c))
            .filter(|t| t.is_hazard())
            .collect();
        assert_eq!(hazards, vec![Tile::Water, Tile::Pit]);
    }

    #[test]
    fn hidden_is_neither_walkable_nor_hazardous() {
        assert!(!Tile::Hidden.is_walkable());
        assert!(!Tile::Hidden.is_hazard());
    }

    fn test_room() -> Room {
        Room {
            id: "room.a".into(),
            name: "A".into(),
            kind: RoomKind::Overworld,
            map_index: None,
            rows: Vec::new(),
            tiles: empty_tile_grid(),
            doors: Vec::new(),
            spawns: Vec::new(),
            chests: vec![Chest {
                id: "chest.a".into(),
                at: Pos { x: 1, y: 1 },
                contains: Reward::SmallKey,
                secret: false,
            }],
            npcs: vec![Npc {
                id: "npc.a".into(),
                at: Pos { x: 2, y: 2 },
                dialogue: Vec::new(),
                condition: None,
            }],
            enemies: Vec::new(),
            puzzles: Vec::new(),
            torches: vec![Torch {
                id: "torch.a".into(),
                at: Pos { x: 3, y: 3 },
                reveals: Vec::new(),
            }],
            plates: vec![Plate {
                id: "plate.a".into(),
                at: Pos { x: 4, y: 4 },
            }],
            blocks: vec![Block {
                id: "block.a".into(),
                at: Pos { x: 5, y: 5 },
            }],
            beacons: vec![Beacon {
                id: "beacon.a".into(),
                at: Pos { x: 6, y: 6 },
            }],
            hint: None,
        }
    }

    #[test]
    fn object_at_finds_each_solid_kind_by_index() {
        let room = test_room();
        assert_eq!(
            room.object_at(Pos { x: 1, y: 1 }),
            Some(ObjectKind::Chest(0))
        );
        assert_eq!(room.object_at(Pos { x: 2, y: 2 }), Some(ObjectKind::Npc(0)));
        assert_eq!(
            room.object_at(Pos { x: 3, y: 3 }),
            Some(ObjectKind::Torch(0))
        );
        assert_eq!(
            room.object_at(Pos { x: 6, y: 6 }),
            Some(ObjectKind::Beacon(0))
        );
        assert_eq!(room.object_at(Pos { x: 0, y: 0 }), None);
    }

    #[test]
    fn plate_at_is_separate_from_object_at() {
        let room = test_room();
        assert_eq!(room.plate_at(Pos { x: 4, y: 4 }), Some(0));
        assert_eq!(room.object_at(Pos { x: 4, y: 4 }), None);
        assert_eq!(room.plate_at(Pos { x: 1, y: 1 }), None);
    }

    #[test]
    fn a_block_is_not_an_object_kind() {
        let room = test_room();
        assert_eq!(room.block_at(Pos { x: 5, y: 5 }), Some(0));
        assert_eq!(room.object_at(Pos { x: 5, y: 5 }), None);
        assert_eq!(room.block_index("block.a"), Some(0));
        assert_eq!(room.block_index("block.missing"), None);
    }
}

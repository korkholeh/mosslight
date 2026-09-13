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

    /// Whether the hero can step onto this tile (spec §6). `Hidden` is inert until phase 4: not
    /// walkable, no reveal mechanic yet.
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
    PushBlock,
    Switches,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum EnemyKind {
    Slime,
    Bandit,
    Wisp,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Npc {
    pub id: String,
    pub at: Pos,
    #[serde(default)]
    pub dialogue: Vec<String>,
    #[serde(default)]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemySpawn {
    pub kind: EnemyKind,
    pub at: Pos,
    #[serde(default)]
    pub patrol: Option<Vec<Pos>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Puzzle {
    pub id: String,
    pub kind: PuzzleKind,
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
}

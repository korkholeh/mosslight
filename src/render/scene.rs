//! Layout arithmetic and the bordered room scene (spec §4).
//!
//! Fixed 50x21 budget centred by floor division: row 0 HUD, rows 1..=18 a bordered 50x18 scene
//! box (interior 48x16 = 24 tiles x 2 columns), row 19 message, row 20 hint.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Widget};

use super::theme::Theme;
use super::tiles::{glyph, Kind};
use crate::config::GlyphSet;
use crate::game::ai::strike_tiles;
use crate::game::tuning::GUARDIAN_DASH_TILES;
use crate::game::{AiState, Facing, GameState, ObjectRef, Pos, PuzzleKind, Room, Tile};

pub const BLOCK_W: u16 = 50;
pub const BLOCK_H: u16 = 21;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub x0: u16,
    pub y0: u16,
}

impl Layout {
    pub fn compute(w: u16, h: u16) -> Self {
        Layout {
            x0: w.saturating_sub(BLOCK_W) / 2,
            y0: h.saturating_sub(BLOCK_H) / 2,
        }
    }

    pub fn hud_row(self) -> u16 {
        self.y0
    }

    pub fn scene_area(self) -> Rect {
        Rect {
            x: self.x0,
            y: self.y0 + 1,
            width: BLOCK_W,
            height: 18,
        }
    }

    pub fn message_row(self) -> u16 {
        self.y0 + 19
    }

    pub fn hint_row(self) -> u16 {
        self.y0 + 20
    }

    /// Screen column of tile `tx`'s glyph, inside the bordered scene box.
    pub fn tile_col(self, tx: u8) -> u16 {
        self.x0 + 1 + 2 * u16::from(tx)
    }

    /// Screen row of tile `ty`'s glyph, inside the bordered scene box.
    pub fn tile_row(self, ty: u8) -> u16 {
        self.y0 + 2 + u16::from(ty)
    }
}

/// Up to `GUARDIAN_DASH_TILES` tiles ahead of `from` in `facing`, stopping at the first
/// non-walkable tile — the danger cue mirrors where a dash would actually stop.
fn telegraph_lane(room: &Room, from: Pos, facing: Facing) -> Vec<Pos> {
    let mut lane = Vec::new();
    let mut pos = from;
    for _ in 0..GUARDIAN_DASH_TILES {
        let next = match facing {
            Facing::North => pos.y.checked_sub(1).map(|y| Pos { x: pos.x, y }),
            Facing::South => pos.y.checked_add(1).map(|y| Pos { x: pos.x, y }),
            Facing::East => pos.x.checked_add(1).map(|x| Pos { x, y: pos.y }),
            Facing::West => pos.x.checked_sub(1).map(|x| Pos { x, y: pos.y }),
        };
        let Some(next) = next else { break };
        if !room
            .tile_at(next)
            .is_some_and(|t| t.is_walkable() && !t.is_hazard())
        {
            break;
        }
        lane.push(next);
        pos = next;
    }
    lane
}

/// Paints tiles (a revealed `Hidden` tile draws as floor), then plates, then chests/NPCs/torches,
/// then the guardian danger cue, then enemies, then the sword, then the hero — so the hero is
/// never hidden by an enemy and the sword is never hidden by a tile.
pub fn draw_scene(
    buf: &mut Buffer,
    layout: Layout,
    state: &GameState,
    theme: Theme,
    glyphs: GlyphSet,
) {
    let block = Block::default().borders(Borders::ALL);
    Widget::render(block, layout.scene_area(), buf);

    let room = state.room();
    let room_idx = state.room;
    for (ty, row) in room.tiles.iter().enumerate() {
        for (tx, tile) in row.iter().enumerate() {
            let pos = Pos {
                x: tx as u8,
                y: ty as u8,
            };
            let kind = if *tile == Tile::Hidden && state.is_revealed(room_idx, pos) {
                Kind::Floor
            } else {
                Kind::from(*tile)
            };
            let col = layout.tile_col(tx as u8);
            let row_y = layout.tile_row(ty as u8);
            set_tile(buf, col, row_y, kind, theme, glyphs);
        }
    }

    for (i, plate) in room.plates.iter().enumerate() {
        let pressed =
            state.puzzle.pressed.contains(&(i as u16)) || state.puzzle.blocks.contains(&plate.at);
        let kind = if pressed {
            Kind::PlatePressed
        } else {
            Kind::Plate
        };
        let col = layout.tile_col(plate.at.x);
        let row_y = layout.tile_row(plate.at.y);
        set_tile(buf, col, row_y, kind, theme, glyphs);
    }

    for (i, chest) in room.chests.iter().enumerate() {
        let opened = state
            .progress
            .opened_chests
            .iter()
            .any(|o| o.room == room_idx && o.index == i as u16);
        let kind = if opened { Kind::ChestOpen } else { Kind::Chest };
        let col = layout.tile_col(chest.at.x);
        let row_y = layout.tile_row(chest.at.y);
        set_tile(buf, col, row_y, kind, theme, glyphs);
    }
    for npc in &room.npcs {
        let col = layout.tile_col(npc.at.x);
        let row_y = layout.tile_row(npc.at.y);
        set_tile(buf, col, row_y, Kind::Npc, theme, glyphs);
    }
    for beacon in &room.beacons {
        let col = layout.tile_col(beacon.at.x);
        let row_y = layout.tile_row(beacon.at.y);
        set_tile(buf, col, row_y, Kind::Beacon, theme, glyphs);
    }
    for (i, torch) in room.torches.iter().enumerate() {
        let permanently_lit = state
            .progress
            .lit_torches
            .iter()
            .any(|o| o.room == room_idx && o.index == i as u16);
        let sequence_lit = state.puzzle.sequence.contains(&(i as u16));
        let solved_sequence = room.puzzles.iter().enumerate().any(|(pi, p)| {
            p.kind == PuzzleKind::TorchSequence
                && p.torches.contains(&torch.id)
                && state.progress.solved_puzzles.contains(&ObjectRef {
                    room: room_idx,
                    index: pi as u16,
                })
        });
        let lit = permanently_lit || sequence_lit || solved_sequence;
        let kind = if lit { Kind::TorchLit } else { Kind::Torch };
        let col = layout.tile_col(torch.at.x);
        let row_y = layout.tile_row(torch.at.y);
        set_tile(buf, col, row_y, kind, theme, glyphs);
    }
    for &pos in &state.puzzle.blocks {
        let col = layout.tile_col(pos.x);
        let row_y = layout.tile_row(pos.y);
        set_tile(buf, col, row_y, Kind::Block, theme, glyphs);
    }
    for enemy in state.enemies.iter().filter(|e| e.alive) {
        match enemy.ai {
            AiState::GuardianTelegraph { facing, .. } => {
                for lane_pos in telegraph_lane(room, enemy.pos, facing) {
                    let col = layout.tile_col(lane_pos.x);
                    let row_y = layout.tile_row(lane_pos.y);
                    set_tile(buf, col, row_y, Kind::Telegraph, theme, glyphs);
                }
            }
            AiState::BossWindup { pattern, .. } => {
                for tile_pos in strike_tiles(pattern, enemy.pos, room) {
                    let col = layout.tile_col(tile_pos.x);
                    let row_y = layout.tile_row(tile_pos.y);
                    set_tile(buf, col, row_y, Kind::Telegraph, theme, glyphs);
                }
            }
            _ => {}
        }
    }

    for enemy in state.enemies.iter().filter(|e| e.alive) {
        let kind = if matches!(enemy.ai, AiState::BossVulnerable { .. }) {
            Kind::BossVulnerable
        } else {
            Kind::from(enemy.kind)
        };
        let col = layout.tile_col(enemy.pos.x);
        let row_y = layout.tile_row(enemy.pos.y);
        set_tile(buf, col, row_y, kind, theme, glyphs);
    }

    if let Some(at) = state.hero.attack.as_ref().and_then(|swing| swing.at) {
        let col = layout.tile_col(at.x);
        let row_y = layout.tile_row(at.y);
        set_tile(buf, col, row_y, Kind::Sword, theme, glyphs);
    }

    let hero_pos: Pos = state.hero.pos;
    let col = layout.tile_col(hero_pos.x);
    let row_y = layout.tile_row(hero_pos.y);
    set_tile(buf, col, row_y, Kind::Hero, theme, glyphs);
}

fn set_tile(buf: &mut Buffer, col: u16, row: u16, kind: Kind, theme: Theme, glyphs: GlyphSet) {
    let style = Style::default().fg(theme.color_for(kind));
    let text = format!("{} ", glyph(kind, glyphs));
    buf.set_string(col, row, text, style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_60x24_matches_spec_example() {
        let layout = Layout::compute(60, 24);
        assert_eq!(layout.x0, 5);
        assert_eq!(layout.y0, 1);
        assert_eq!(layout.tile_col(12), 30);
        assert_eq!(layout.tile_row(8), 11);
        assert_eq!(layout.tile_col(0), 6);
        assert_eq!(layout.tile_row(0), 3);
        assert_eq!(layout.message_row(), 20);
        assert_eq!(layout.hint_row(), 21);
    }

    #[test]
    fn layout_80x24_is_centred() {
        let layout = Layout::compute(80, 24);
        assert_eq!(layout.x0, 15);
        assert_eq!(layout.y0, 1);
    }
}

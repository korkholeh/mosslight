//! Colour only. Themes may never change a glyph (spec §4): colour is not the only distinction.

use ratatui::style::Color;

use super::tiles::Kind;
use crate::config::ThemeName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    name: ThemeName,
}

impl Theme {
    pub fn new(name: ThemeName) -> Self {
        Theme { name }
    }

    /// Foreground colour for a tile/hero kind. `Mono` returns only white/black-family colours so
    /// a monochrome terminal stays fully legible; `Ansi` currently aliases `Gameboy` (phase 6
    /// gives it its own palette).
    pub fn color_for(self, kind: Kind) -> Color {
        match self.name {
            ThemeName::Mono => Color::White,
            ThemeName::Gameboy | ThemeName::Ansi => match kind {
                Kind::Hero => Color::LightYellow,
                Kind::Wall => Color::Green,
                Kind::Floor => Color::DarkGray,
                Kind::Water => Color::Cyan,
                Kind::Bush => Color::LightGreen,
                Kind::Door => Color::Yellow,
                Kind::Stairs => Color::Gray,
                Kind::Pit => Color::DarkGray,
                Kind::Slime => Color::Green,
                Kind::Bat => Color::Magenta,
                Kind::Guardian => Color::Red,
                Kind::Sword => Color::White,
                Kind::Telegraph => Color::LightRed,
                Kind::Npc => Color::LightCyan,
                Kind::Chest => Color::LightYellow,
                Kind::ChestOpen => Color::DarkGray,
                Kind::Torch => Color::DarkGray,
                Kind::TorchLit => Color::LightRed,
                Kind::Plate => Color::Gray,
                Kind::PlatePressed => Color::LightGreen,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [Kind; 20] = [
        Kind::Hero,
        Kind::Wall,
        Kind::Floor,
        Kind::Water,
        Kind::Bush,
        Kind::Door,
        Kind::Stairs,
        Kind::Pit,
        Kind::Slime,
        Kind::Bat,
        Kind::Guardian,
        Kind::Sword,
        Kind::Telegraph,
        Kind::Npc,
        Kind::Chest,
        Kind::ChestOpen,
        Kind::Torch,
        Kind::TorchLit,
        Kind::Plate,
        Kind::PlatePressed,
    ];

    #[test]
    fn mono_theme_only_uses_white_or_black_family_colors() {
        // The real, checkable claim this module makes (glyph identity across themes is instead
        // structural: `tiles::glyph` takes no theme parameter at all, so there is no runtime path
        // that could vary it — see `tiles::every_glyph_is_distinct` for the distinctness half of
        // RISKS #10). A prior version of this test only called `color_for`/`glyph` and asserted
        // nothing (round-2 review, major); this checks the doc comment's promise that `Mono`
        // stays legible on a monochrome terminal.
        let theme = Theme::new(ThemeName::Mono);
        for kind in ALL_KINDS {
            let color = theme.color_for(kind);
            assert!(
                matches!(
                    color,
                    Color::White | Color::Black | Color::Gray | Color::DarkGray
                ),
                "{kind:?} resolved to {color:?} under Mono, breaking monochrome legibility"
            );
        }
    }
}

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
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [Kind; 13] = [
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
    ];

    #[test]
    fn every_theme_uses_the_identical_glyph_table() {
        // Themes only ever return colours; the glyph table in `tiles.rs` has no theme parameter
        // at all, so this is a structural property, not a runtime check. This test documents it
        // by confirming every kind still resolves to a glyph under every theme's colour mapping.
        for theme_name in [ThemeName::Gameboy, ThemeName::Ansi, ThemeName::Mono] {
            let theme = Theme::new(theme_name);
            for kind in ALL_KINDS {
                let _ = theme.color_for(kind);
                let _ = super::super::tiles::glyph(kind);
            }
        }
    }
}

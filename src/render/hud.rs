//! Top bar: health, active item, key count (spec §4).

use ratatui::buffer::Buffer;
use ratatui::style::Style;

use super::scene::Layout;
use crate::game::GameState;

/// The equipment shown as `Item:` — priority order matches acquisition order in the overworld
/// (sword, then lantern); `none` before either is held.
fn item_label(state: &GameState) -> &'static str {
    if state.hero.has_sword {
        "sword"
    } else if state.hero.has_lantern {
        "lantern"
    } else {
        "none"
    }
}

pub fn draw_hud(buf: &mut Buffer, layout: Layout, state: &GameState) {
    let hp_text = format!(
        "HP {}/{}  Item: {}  Keys: {}",
        state.hero.health_halves,
        state.hero.max_health_halves,
        item_label(state),
        state.hero.keys
    );
    buf.set_string(layout.x0, layout.hud_row(), hp_text, Style::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::GameState as GS;
    use ratatui::layout::Rect;
    use std::rc::Rc;

    #[test]
    fn hud_contains_hp_label() {
        let world = Rc::new(crate::content::load().expect("embedded world validates"));
        let state = GS::new(1, world);
        let layout = Layout::compute(60, 24);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 24));
        draw_hud(&mut buf, layout, &state);
        let row: String = (0..60)
            .map(|x| {
                buf[(x, layout.hud_row())]
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(row.contains("HP"));
    }
}

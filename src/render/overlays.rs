//! Main menu, pause, confirm-quit, help and the too-small notice (spec §4, §12).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::app::{App, MenuCursor, Mode};

fn centered_box(area: Rect, w: u16, h: u16) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.min(area.width),
        height: h.min(area.height),
    }
}

fn draw_box(buf: &mut Buffer, area: Rect, title: &str, lines: &[String]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title.to_string());
    let inner = block.inner(area);
    Widget::render(block, area, buf);
    let text = lines.join("\n");
    Widget::render(Paragraph::new(text), inner, buf);
}

pub fn draw_main_menu(buf: &mut Buffer, area: Rect, cursor: MenuCursor) {
    let items = [
        (MenuCursor::Continue, "Continue (no save yet)"),
        (MenuCursor::NewGame, "New Game"),
        (MenuCursor::Help, "Help"),
        (MenuCursor::Quit, "Quit"),
    ];
    let lines: Vec<String> = items
        .iter()
        .map(|(item, label)| {
            let marker = if *item == cursor { "> " } else { "  " };
            format!("{marker}{label}")
        })
        .collect();
    let box_area = centered_box(area, 30, 8);
    draw_box(buf, box_area, "Mosslight", &lines);
}

pub fn draw_pause(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 24, 5);
    draw_box(
        buf,
        box_area,
        "Paused",
        &["Esc: resume".to_string(), "Q: quit".to_string()],
    );
}

pub fn draw_confirm_quit(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 30, 5);
    draw_box(
        buf,
        box_area,
        "Quit?",
        &[
            "Enter/E: confirm quit".to_string(),
            "Esc: cancel".to_string(),
        ],
    );
}

pub fn draw_help(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 40, 12);
    draw_box(
        buf,
        box_area,
        "Help",
        &[
            "Move: Arrows / WASD".to_string(),
            "Attack: J / Space".to_string(),
            "Lantern: K".to_string(),
            "Interact/Confirm: E / Enter".to_string(),
            "Map: M   Inventory: I".to_string(),
            "Pause/Back: Esc".to_string(),
            "Quit: Q".to_string(),
            "Esc/?: close".to_string(),
        ],
    );
}

pub fn draw_game_over(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 40, 5);
    draw_box(
        buf,
        box_area,
        "You fell",
        &[
            "Enter: retry from the last room".to_string(),
            "Esc: main menu".to_string(),
        ],
    );
}

pub fn draw_too_small(buf: &mut Buffer, area: Rect, required: (u16, u16), current: (u16, u16)) {
    let text = format!(
        "Terminal too small.\nRequired: {}x{}\nCurrent: {}x{}",
        required.0, required.1, current.0, current.1
    );
    Widget::render(
        Paragraph::new(text).style(Style::default().add_modifier(Modifier::BOLD)),
        area,
        buf,
    );
}

pub fn draw_overlay_for_mode(buf: &mut Buffer, area: Rect, app: &App) {
    match app.mode {
        Mode::MainMenu => draw_main_menu(buf, area, app.menu),
        Mode::Paused => draw_pause(buf, area),
        Mode::ConfirmQuit => draw_confirm_quit(buf, area),
        Mode::Help => draw_help(buf, area),
        Mode::GameOver => draw_game_over(buf, area),
        Mode::Playing | Mode::TooSmall => {}
    }
}

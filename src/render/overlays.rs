//! Main menu, pause, confirm-quit, help and the too-small notice (spec §4, §12).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::app::{App, MenuCursor, Mode, SlotState};
use crate::game::{RoomIdx, RoomKind};

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
    // `trim: false` — the map grid's alignment depends on its leading spaces surviving; ordinary
    // prose here is short enough never to need the wrap at all.
    Widget::render(Paragraph::new(text).wrap(Wrap { trim: false }), inner, buf);
}

pub fn draw_main_menu(buf: &mut Buffer, area: Rect, cursor: MenuCursor, continue_label: &str) {
    let items = [
        (MenuCursor::Continue, continue_label),
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
    let box_area = centered_box(area, 24, 6);
    draw_box(
        buf,
        box_area,
        "Paused",
        &[
            "Esc: resume".to_string(),
            "Enter: save".to_string(),
            "Q: quit".to_string(),
        ],
    );
}

/// New Game over a `Usable`/`FutureVersion` slot (spec §12: overwriting an existing playthrough
/// requires confirmation).
pub fn draw_confirm_new_game(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 40, 6);
    draw_box(
        buf,
        box_area,
        "Start a new game?",
        &[
            "This will overwrite your saved progress.".to_string(),
            "Enter/E: confirm".to_string(),
            "Esc: cancel".to_string(),
        ],
    );
}

/// The slot is `Corrupt` or `FutureVersion`: offers a backup restore (if the slot is `Corrupt`) or
/// a fresh run, and never writes anything (spec §10).
pub fn draw_save_problem(buf: &mut Buffer, area: Rect, app: &App) {
    let title = match &app.slot {
        SlotState::Corrupt { .. } => "Save damaged",
        SlotState::FutureVersion { .. } => "Save from a newer version",
        SlotState::Empty | SlotState::Usable(_) => "Save problem",
    };
    let labels = app.save_problem_labels();
    let lines: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let marker = if i == app.save_problem_cursor {
                "> "
            } else {
                "  "
            };
            format!("{marker}{label}")
        })
        .collect();
    let box_area = centered_box(area, 46, 4 + labels.len() as u16);
    draw_box(buf, box_area, title, &lines);
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
    let box_area = centered_box(area, 40, 14);
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
            "Dialogue: E advances, Esc closes".to_string(),
            "Pause/Back: Esc".to_string(),
            "Save: Esc to pause, then Enter".to_string(),
            "Quit: Q".to_string(),
            "Esc/?: close".to_string(),
        ],
    );
}

/// One cell per overworld `map_index`; unvisited rooms stay blank. A visited room renders, in
/// priority order, `@` (current room), `C` (an unopened chest), `>` (a door to a dungeon room),
/// `N` (an NPC), `.` (plain). Dungeon rooms have no `map_index` and never appear.
pub fn draw_map(buf: &mut Buffer, area: Rect, app: &App) {
    let state = &app.state;
    let world = &state.world;
    let mut grid = [[' '; 3]; 3];
    let mut current_name = String::new();

    for (i, room) in world.rooms.iter().enumerate() {
        let Some((mx, my)) = room.map_index else {
            continue;
        };
        let room_idx = RoomIdx(i as u16);
        if !state.progress.visited.contains(&room_idx) {
            continue;
        }
        let is_current = room_idx == state.room;
        // Secret chests are deliberately excluded: the map must not point at a reward the player
        // has no way to know is there yet (spec docs/spec.md:210 — the map shows discovered
        // objects, not undiscovered ones).
        let has_unopened_chest = room.chests.iter().enumerate().any(|(ci, c)| {
            !c.secret
                && !state
                    .progress
                    .opened_chests
                    .iter()
                    .any(|o| o.room == room_idx && o.index == ci as u16)
        });
        let has_dungeon_entrance = room.doors.iter().any(|d| {
            world
                .room_idx(&d.to_room)
                .is_some_and(|t| world.room(t).kind == RoomKind::Dungeon)
        });
        let symbol = if is_current {
            '@'
        } else if has_unopened_chest {
            'C'
        } else if has_dungeon_entrance {
            '>'
        } else if !room.npcs.is_empty() {
            'N'
        } else {
            '.'
        };
        grid[my as usize][mx as usize] = symbol;
        if is_current {
            current_name = room.name.clone();
        }
    }

    let mut lines: Vec<String> = grid
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    lines.push(String::new());
    lines.push("@ you   C chest   > dungeon   N npc   . plain".to_string());
    lines.push(current_name);

    let box_area = centered_box(area, 42, 11);
    draw_box(buf, box_area, "Map", &lines);
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// Equipment as `yes`/`no`, key count, hearts (half-heart units, matching the HUD), and the story
/// flags known so far.
pub fn draw_inventory(buf: &mut Buffer, area: Rect, app: &App) {
    let hero = &app.state.hero;
    let mut lines = vec![
        format!("Sword: {}", yes_no(hero.has_sword)),
        format!("Lantern: {}", yes_no(hero.has_lantern)),
        format!("Ember: {}", yes_no(hero.has_ember)),
        format!("Keys: {}", hero.keys),
        format!("Hearts: {}/{}", hero.health_halves, hero.max_health_halves),
    ];
    if app.state.progress.flags.is_empty() {
        lines.push("Flags: none".to_string());
    } else {
        let flags: Vec<&str> = app
            .state
            .progress
            .flags
            .iter()
            .map(String::as_str)
            .collect();
        lines.push(format!("Flags: {}", flags.join(", ")));
    }
    let box_area = centered_box(area, 40, 10);
    draw_box(buf, box_area, "Inventory", &lines);
}

/// A bordered window with the NPC's authored id, the current node's text, and an explicit
/// advance/close prompt (§4: "explicit advance step"; ARCHITECTURE's accessibility rule: advance
/// on a keypress, never on a timer). Draws nothing if no dialogue is open (defensive: `App` only
/// enters `Mode::Dialogue` when one is).
pub fn draw_dialogue(buf: &mut Buffer, area: Rect, app: &App) {
    let Some(dialogue) = app.state.dialogue else {
        return;
    };
    let room = app.state.world.room(dialogue.npc.room);
    let Some(npc) = room.npcs.get(dialogue.npc.index as usize) else {
        return;
    };
    let Some(node) = npc.dialogue.get(dialogue.node as usize) else {
        return;
    };
    let is_last = (dialogue.node as usize + 1) >= npc.dialogue.len();
    let prompt = if is_last { "[E] close" } else { "[E] continue" };
    let lines = vec![node.text.clone(), String::new(), prompt.to_string()];
    let box_area = centered_box(area, 44, 8);
    draw_box(buf, box_area, &npc.id, &lines);
}

pub fn draw_game_over(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 40, 5);
    draw_box(
        buf,
        box_area,
        "You fell",
        &[
            "Enter: retry from the last save".to_string(),
            "Esc: main menu".to_string(),
        ],
    );
}

pub fn draw_victory(buf: &mut Buffer, area: Rect) {
    let box_area = centered_box(area, 40, 5);
    draw_box(
        buf,
        box_area,
        "The lighthouse is relit",
        &[
            "Enter/E: main menu".to_string(),
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
        Mode::MainMenu => draw_main_menu(buf, area, app.menu, app.continue_label()),
        Mode::Paused => draw_pause(buf, area),
        Mode::ConfirmQuit => draw_confirm_quit(buf, area),
        Mode::Help => draw_help(buf, area),
        Mode::GameOver => draw_game_over(buf, area),
        Mode::Dialogue => draw_dialogue(buf, area, app),
        Mode::Map => draw_map(buf, area, app),
        Mode::Inventory => draw_inventory(buf, area, app),
        Mode::Victory => draw_victory(buf, area),
        Mode::ConfirmNewGame => draw_confirm_new_game(buf, area),
        Mode::SaveProblem => draw_save_problem(buf, area, app),
        Mode::Playing | Mode::TooSmall => {}
    }
}

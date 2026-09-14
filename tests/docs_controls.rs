//! `docs/user/controls.md` stays honest against the real key map (spec §5): every key that
//! `input::map_key` sends somewhere in some mode is named in the doc, and every key the doc names
//! actually maps to something — proved against the live `map_key` function, not a copied table.

use std::fs;

use mosslight::app::Mode;
use mosslight::input::map_key;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

const MODES: [Mode; 13] = [
    Mode::MainMenu,
    Mode::Playing,
    Mode::Paused,
    Mode::ConfirmQuit,
    Mode::Help,
    Mode::TooSmall,
    Mode::GameOver,
    Mode::Dialogue,
    Mode::Map,
    Mode::Inventory,
    Mode::Victory,
    Mode::ConfirmNewGame,
    Mode::SaveProblem,
];

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn maps_to_something(code: KeyCode, modifiers: KeyModifiers) -> bool {
    MODES
        .iter()
        .any(|&mode| map_key(mode, key(code, modifiers)).is_some())
}

/// The `Keys` column of every data row in `docs/user/controls.md`'s table, split into discrete
/// tokens (`Arrow keys`, `W`, `A`, ..., `Enter`, `Space`, `Esc`, `?`, `Ctrl+C`) — the same alphabet
/// `map_key` understands.
fn documented_key_tokens() -> Vec<String> {
    let text = fs::read_to_string("docs/user/controls.md")
        .expect("docs/user/controls.md must be readable");
    let mut tokens = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') || line.starts_with("|---") || line.starts_with("| Action") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        // `| Action | Keys | Available in |` -> ["", " Action ", " Keys ", " Available in ", ""]
        let Some(keys_cell) = cells.get(2) else {
            continue;
        };
        for part in keys_cell.split(',') {
            for piece in part.split(" or ") {
                for token in piece.split('/') {
                    let token = token.trim();
                    if !token.is_empty() {
                        tokens.push(token.to_string());
                    }
                }
            }
        }
    }
    tokens
}

/// Every key `map_key` recognises in this test's fixed alphabet, alongside its documented-token
/// spelling.
fn alphabet() -> Vec<(String, KeyCode, KeyModifiers)> {
    let mut a = Vec::new();
    for c in 'a'..='z' {
        a.push((
            c.to_ascii_uppercase().to_string(),
            KeyCode::Char(c),
            KeyModifiers::NONE,
        ));
    }
    for d in '0'..='9' {
        a.push((d.to_string(), KeyCode::Char(d), KeyModifiers::NONE));
    }
    a.push(("Enter".to_string(), KeyCode::Enter, KeyModifiers::NONE));
    a.push(("Space".to_string(), KeyCode::Char(' '), KeyModifiers::NONE));
    a.push(("Esc".to_string(), KeyCode::Esc, KeyModifiers::NONE));
    a.push(("?".to_string(), KeyCode::Char('?'), KeyModifiers::NONE));
    a.push((
        "Ctrl+C".to_string(),
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    ));
    a
}

#[test]
fn every_key_that_maps_to_something_is_documented() {
    let documented = documented_key_tokens();
    let arrows_documented = documented.iter().any(|t| t == "Arrow keys");

    for code in [KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right] {
        if maps_to_something(code, KeyModifiers::NONE) {
            assert!(
                arrows_documented,
                "{code:?} maps to an action but \"Arrow keys\" is not in docs/user/controls.md"
            );
        }
    }

    for (label, code, modifiers) in alphabet() {
        if maps_to_something(code, modifiers) {
            assert!(
                documented.iter().any(|t| t == &label),
                "{label} ({code:?}) maps to an action in some mode but is not named in \
                 docs/user/controls.md's Keys column (tokens found: {documented:?})"
            );
        }
    }
}

#[test]
fn every_documented_key_maps_to_something() {
    for token in documented_key_tokens() {
        let maps = match token.as_str() {
            "Arrow keys" => [KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right]
                .into_iter()
                .all(|c| maps_to_something(c, KeyModifiers::NONE)),
            "Enter" => maps_to_something(KeyCode::Enter, KeyModifiers::NONE),
            "Space" => maps_to_something(KeyCode::Char(' '), KeyModifiers::NONE),
            "Esc" => maps_to_something(KeyCode::Esc, KeyModifiers::NONE),
            "?" => maps_to_something(KeyCode::Char('?'), KeyModifiers::NONE),
            "Ctrl+C" => maps_to_something(KeyCode::Char('c'), KeyModifiers::CONTROL),
            single if single.len() == 1 && single.chars().next().unwrap().is_ascii_alphabetic() => {
                let c = single.chars().next().unwrap().to_ascii_lowercase();
                maps_to_something(KeyCode::Char(c), KeyModifiers::NONE)
            }
            other => panic!(
                "docs/user/controls.md names an unrecognised key token {other:?} — teach this \
                 test its KeyCode or fix the doc"
            ),
        };
        assert!(
            maps,
            "docs/user/controls.md names {token:?} but it maps to nothing in map_key"
        );
    }
}

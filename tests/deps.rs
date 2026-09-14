//! Guards spec §8's version-skew hazard: crossterm must reach this binary only through
//! `ratatui::crossterm`, never as a directly declared dependency (RISKS #5).

use std::fs;

#[test]
fn cargo_toml_never_declares_crossterm() {
    let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
    for line in manifest.lines() {
        let trimmed = line.trim();
        assert!(
            !trimmed.starts_with("crossterm"),
            "Cargo.toml must never declare crossterm directly; found: {trimmed}"
        );
    }
}

#[test]
fn single_crossterm_in_lockfile() {
    let lock = fs::read_to_string("Cargo.lock").expect("read Cargo.lock");
    let count = lock
        .lines()
        .filter(|l| l.trim() == r#"name = "crossterm""#)
        .count();
    assert_eq!(
        count, 1,
        "expected exactly one crossterm entry in Cargo.lock, found {count}"
    );
}

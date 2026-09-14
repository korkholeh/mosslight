//! Main-route stub freedom as a test, not a review habit (phase 7 T11): `src/` and `assets/`
//! contain no `todo!`, `unimplemented!`, `TODO`, `FIXME`, `XXX`, no `unreachable!("stub` marker,
//! and `assets/world.ron` contains no `placeholder` content.

use std::fs;
use std::path::{Path, PathBuf};

/// Every regular file under `dir`, recursively — no `walkdir` dependency needed for a tree this
/// small, and the fixed dependency set (DECISIONS.md) rules one in anyway.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap_or_else(|e| panic!("read_dir({}): {e}", d.display())) {
            let entry = entry.expect("directory entry readable");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

const FORBIDDEN: [&str; 5] = ["todo!", "unimplemented!", "TODO", "FIXME", "XXX"];

fn stub_markers_under(dir: &Path) -> Vec<String> {
    let mut offenders = Vec::new();
    for path in files_under(dir) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue; // non-UTF8 file: not source/content
        };
        for (i, line) in text.lines().enumerate() {
            for marker in FORBIDDEN {
                if line.contains(marker) {
                    offenders.push(format!("{}:{}: {marker}", path.display(), i + 1));
                }
            }
            if line.contains("unreachable!(\"stub") {
                offenders.push(format!(
                    "{}:{}: unreachable!(\"stub...",
                    path.display(),
                    i + 1
                ));
            }
        }
    }
    offenders
}

#[test]
fn src_has_no_stub_markers() {
    let offenders = stub_markers_under(Path::new("src"));
    assert!(
        offenders.is_empty(),
        "stub markers found:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn assets_has_no_stub_markers() {
    let offenders = stub_markers_under(Path::new("assets"));
    assert!(
        offenders.is_empty(),
        "stub markers found:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn world_ron_has_no_placeholder_content() {
    let text = fs::read_to_string("assets/world.ron").expect("assets/world.ron must be readable");
    let offenders: Vec<String> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| l.contains("placeholder"))
        .map(|(i, l)| format!("assets/world.ron:{}: {}", i + 1, l.trim()))
        .collect();
    assert!(
        offenders.is_empty(),
        "placeholder content found in assets/world.ron:\n{}",
        offenders.join("\n")
    );
}

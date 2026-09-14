//! Structural guard for spec §5: no key-release path or extended keyboard protocol may ever
//! appear in `src/`, so there is nothing to remove later if SSH terminals disagree on support.

use std::fs;
use std::path::Path;

const FORBIDDEN: [&str; 4] = [
    "KeyEventKind",
    "Release",
    "KeyboardEnhancementFlags",
    "PushKeyboardEnhancementFlags",
];

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_key_release_or_extended_protocol_tokens_in_src() {
    let mut files = Vec::new();
    walk(Path::new("src"), &mut files);
    assert!(!files.is_empty(), "expected to find .rs files under src/");

    for path in files {
        let contents = fs::read_to_string(&path).expect("read source file");
        for token in FORBIDDEN {
            // `KeyEventKind::Press` is fine to name explicitly when constructing test fixtures
            // for *this* forbidden-token check would be self-defeating, so the check simply
            // forbids all of these identifiers appearing anywhere, including in comments or
            // test fixtures — the whole point is that no code path ever needs to spell them.
            assert!(
                !contents.contains(token),
                "forbidden token `{token}` found in {}",
                path.display()
            );
        }
    }
}

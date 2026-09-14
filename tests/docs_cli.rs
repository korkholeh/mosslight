//! `docs/user/cli.md` stays honest against the real clap surface (spec §12): every documented
//! flag exists in the parser and every parsed non-hidden flag is documented, in both directions,
//! plus a check that every documented enum flag's listed values match clap's own.

use std::collections::{HashMap, HashSet};
use std::fs;

use mosslight::config;

/// Every `` `...` `` backtick span on a line, in order. Table cells are pipe-delimited, but a
/// literal `|` inside a cell is escaped as `\|` (see `--color <auto\|always\|never>` in the source
/// markdown) — scanning for backticks directly sidesteps having to split on unescaped `|` at all.
fn backtick_spans(line: &str) -> Vec<&str> {
    let mut spans = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else {
            break;
        };
        spans.push(&after[..end]);
        rest = &after[end + 1..];
    }
    spans
}

/// Flag name (`--color`) plus its documented enum values, if the span shows a `<a\|b\|c>` list.
fn parse_flag_span(span: &str) -> Option<(String, Option<Vec<String>>)> {
    if !span.starts_with("--") {
        return None;
    }
    let name = span.split([' ', '<']).next().unwrap_or(span).to_string();
    let values = span.find('<').and_then(|start| {
        let end = span.rfind('>')?;
        let inner = &span[start + 1..end];
        Some(
            inner
                .replace("\\|", "|")
                .split('|')
                .map(str::to_string)
                .collect(),
        )
    });
    Some((name, values))
}

/// Only the table rows of `docs/user/cli.md` (lines starting with `|`), not the fenced usage
/// synopsis above it — the synopsis is documentation prose, the table is the checked surface.
fn documented_flags() -> (HashSet<String>, HashMap<String, Vec<String>>) {
    let text = fs::read_to_string("docs/user/cli.md").expect("docs/user/cli.md must be readable");
    let mut flags = HashSet::new();
    let mut values = HashMap::new();
    for line in text.lines() {
        if !line.trim_start().starts_with('|') {
            continue;
        }
        for span in backtick_spans(line) {
            if let Some((name, vals)) = parse_flag_span(span) {
                flags.insert(name.clone());
                if let Some(vals) = vals {
                    values.insert(name, vals);
                }
            }
        }
    }
    (flags, values)
}

#[test]
fn every_non_hidden_flag_is_documented_and_vice_versa() {
    let mut cmd = config::cli_command();
    cmd.build();

    let clap_flags: HashSet<String> = cmd
        .get_arguments()
        .filter(|a| !a.is_hide_set())
        .filter_map(|a| a.get_long())
        .map(|l| format!("--{l}"))
        .collect();

    let (documented, _) = documented_flags();

    let undocumented: Vec<_> = clap_flags.difference(&documented).collect();
    assert!(
        undocumented.is_empty(),
        "flags parsed by clap but missing from docs/user/cli.md's table: {undocumented:?}"
    );

    let removed: Vec<_> = documented.difference(&clap_flags).collect();
    assert!(
        removed.is_empty(),
        "docs/user/cli.md documents flags clap no longer parses: {removed:?}"
    );
}

#[test]
fn hidden_flags_are_absent_from_the_documented_table() {
    let (documented, _) = documented_flags();
    assert!(!documented.contains("--debug-panic"));
    assert!(!documented.contains("--debug-content"));
}

#[test]
fn documented_enum_values_match_clap_possible_values() {
    let mut cmd = config::cli_command();
    cmd.build();
    let (_, documented_values) = documented_flags();

    assert!(
        !documented_values.is_empty(),
        "expected at least one documented flag with a listed value set"
    );

    for (flag, expected) in &documented_values {
        let long = flag.trim_start_matches("--");
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_long() == Some(long))
            .unwrap_or_else(|| panic!("{flag} is documented with values but not a real flag"));
        let actual: HashSet<String> = arg
            .get_possible_values()
            .iter()
            .map(|p| p.get_name().to_string())
            .collect();
        let expected_set: HashSet<String> = expected.iter().cloned().collect();
        assert_eq!(
            actual, expected_set,
            "{flag}'s documented values {expected:?} do not match clap's possible values {actual:?}"
        );
    }
}

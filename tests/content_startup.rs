//! The content preflight in `main()` runs before any terminal state is touched (spec §7/§15):
//! a broken world is reported and refused with exit code 2, before the TTY check ever runs.

use std::process::{Command, Stdio};

#[test]
fn broken_world_exits_two_with_a_readable_list() {
    let output = Command::new(env!("CARGO_BIN_EXE_mosslight"))
        .arg("--debug-content")
        .arg("tests/fixtures/broken_missing_door_target.ron")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn mosslight");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "expected no stdout output (no raw mode/alt screen ever entered), got: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("room.nonexistent"),
        "expected the content error report on stderr, got: {stderr:?}"
    );
    // Proves the abort happened before terminal::preflight: none of its three refusal messages
    // (see terminal.rs) appear here, only the content error report.
    for tty_message in [
        "requires TERM to be set",
        "does not support TERM=dumb",
        "requires an interactive terminal",
    ] {
        assert!(
            !stderr.contains(tty_message),
            "expected the content refusal, not the TTY refusal ({tty_message:?}): {stderr:?}"
        );
    }
}

#[test]
fn a_valid_debug_content_file_passes_the_content_preflight() {
    // With a valid world, the content preflight passes and the TTY preflight (which always fails
    // for a spawned test process with no TTY) is what actually refuses — proving content
    // validation ran first without itself causing the refusal.
    let output = Command::new(env!("CARGO_BIN_EXE_mosslight"))
        .arg("--debug-content")
        .arg("tests/fixtures/base.ron")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn mosslight");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let is_tty_refusal = [
        "requires TERM to be set",
        "does not support TERM=dumb",
        "requires an interactive terminal",
    ]
    .iter()
    .any(|m| stderr.contains(m));
    assert!(
        is_tty_refusal,
        "expected the TTY refusal once content validated cleanly, got: {stderr:?}"
    );
}

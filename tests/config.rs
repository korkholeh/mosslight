//! CLI parsing (spec §12) and the pre-raw-mode startup refusal (spec §3).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use mosslight::config::{ColorArg, ColorMode, Config, Env, Fps, GlyphSet, ThemeName, DEFAULT_SEED};

fn env() -> Env {
    Env {
        no_color: None,
        term: Some("xterm-256color".to_string()),
        stdout_tty: true,
        xdg_data_home: None,
        home: Some("/home/tester".to_string()),
        os_is_macos: false,
    }
}

#[test]
fn defaults_are_ascii_gameboy_20fps() {
    let cfg = Config::from_args(["mosslight"], &env()).unwrap();
    assert_eq!(cfg.glyphs, GlyphSet::Ascii);
    assert_eq!(cfg.theme, ThemeName::Gameboy);
    assert_eq!(cfg.fps, Fps::F20);
    assert_eq!(cfg.seed, DEFAULT_SEED);
    assert!(!cfg.debug_panic);
}

#[test]
fn every_flag_parses() {
    let cfg = Config::from_args(
        [
            "mosslight",
            "--ascii",
            "--color",
            "always",
            "--theme",
            "mono",
            "--fps",
            "30",
            "--save-dir",
            "/tmp/x",
            "--seed",
            "42",
        ],
        &env(),
    )
    .unwrap();
    assert_eq!(cfg.glyphs, GlyphSet::Ascii);
    assert_eq!(cfg.color, ColorMode::Always);
    assert_eq!(cfg.theme, ThemeName::Mono);
    assert_eq!(cfg.fps, Fps::F30);
    assert_eq!(cfg.save_dir, PathBuf::from("/tmp/x"));
    assert_eq!(cfg.seed, 42);
}

#[test]
fn debug_panic_flag_parses_hidden() {
    let cfg = Config::from_args(["mosslight", "--debug-panic"], &env()).unwrap();
    assert!(cfg.debug_panic);
}

#[test]
fn fps_45_is_rejected() {
    let err = Config::from_args(["mosslight", "--fps", "45"], &env()).unwrap_err();
    assert_eq!(err.code, 2);
    assert_eq!(err.stream, mosslight::config::OutputStream::Stderr);
}

#[test]
fn help_flag_exits_0_on_stdout() {
    let err = Config::from_args(["mosslight", "--help"], &env()).unwrap_err();
    assert_eq!(err.code, 0);
    assert_eq!(err.stream, mosslight::config::OutputStream::Stdout);
    assert!(!err.message.is_empty());
}

#[test]
fn version_flag_exits_0_on_stdout() {
    let err = Config::from_args(["mosslight", "--version"], &env()).unwrap_err();
    assert_eq!(err.code, 0);
    assert_eq!(err.stream, mosslight::config::OutputStream::Stdout);
    assert!(!err.message.is_empty());
}

/// `--unicode` is withdrawn (see `docs/user/cli.md` and DECISIONS.md): it must be neither
/// advertised nor accepted.
#[test]
fn unicode_flag_is_not_offered() {
    let err = Config::from_args(["mosslight", "--help"], &env()).unwrap_err();
    assert_eq!(err.code, 0);
    assert!(
        !err.message.contains("--unicode"),
        "the withdrawn --unicode flag must not appear in --help: {}",
        err.message
    );
}

#[test]
fn unicode_flag_is_rejected_with_exit_2() {
    let err = Config::from_args(["mosslight", "--unicode"], &env()).unwrap_err();
    assert_eq!(err.code, 2);
    assert_eq!(err.stream, mosslight::config::OutputStream::Stderr);
}

#[test]
fn color_precedence_explicit_flag_wins_over_no_color() {
    assert_eq!(
        mosslight::config::resolve_color(Some(ColorArg::Always), Some("1"), true, Some("xterm")),
        ColorMode::Always
    );
}

#[test]
fn color_precedence_no_color_wins_over_auto() {
    assert_eq!(
        mosslight::config::resolve_color(Some(ColorArg::Auto), Some("1"), true, Some("xterm")),
        ColorMode::Never
    );
}

#[test]
fn color_precedence_term_dumb_forces_never() {
    assert_eq!(
        mosslight::config::resolve_color(Some(ColorArg::Auto), None, true, Some("dumb")),
        ColorMode::Never
    );
}

#[test]
fn color_precedence_auto_healthy_term_is_always() {
    assert_eq!(
        mosslight::config::resolve_color(Some(ColorArg::Auto), None, true, Some("xterm")),
        ColorMode::Always
    );
}

#[test]
fn color_precedence_non_tty_forces_never() {
    assert_eq!(
        mosslight::config::resolve_color(Some(ColorArg::Auto), None, false, Some("xterm")),
        ColorMode::Never
    );
}

#[test]
fn save_dir_macos_default() {
    let e = Env {
        os_is_macos: true,
        home: Some("/Users/tester".to_string()),
        ..env()
    };
    let cfg = Config::from_args(["mosslight"], &e).unwrap();
    assert_eq!(
        cfg.save_dir,
        PathBuf::from("/Users/tester/Library/Application Support/mosslight")
    );
}

#[test]
fn save_dir_linux_xdg_default() {
    let e = Env {
        xdg_data_home: Some("/home/tester/.data".to_string()),
        ..env()
    };
    let cfg = Config::from_args(["mosslight"], &e).unwrap();
    assert_eq!(cfg.save_dir, PathBuf::from("/home/tester/.data/mosslight"));
}

#[test]
fn save_dir_linux_fallback_default() {
    let cfg = Config::from_args(["mosslight"], &env()).unwrap();
    assert_eq!(
        cfg.save_dir,
        PathBuf::from("/home/tester/.local/share/mosslight")
    );
}

#[test]
fn explicit_save_dir_overrides_default() {
    let cfg = Config::from_args(["mosslight", "--save-dir", "/scratch"], &env()).unwrap();
    assert_eq!(cfg.save_dir, PathBuf::from("/scratch"));
}

/// Spawns the real binary with no TTY on stdin/stdout (spec §3): it must refuse before raw mode,
/// exit with code 2, and print exactly one line to stderr. Asserting stdout is empty is a cheap,
/// real proof that raw mode/the alternate screen were never entered — either would have written a
/// terminal-mutating escape sequence (e.g. `\x1b[?1049h`) to stdout before the refusal.
#[test]
fn non_tty_stdin_exits_2_with_one_stderr_line() {
    let output = Command::new(env!("CARGO_BIN_EXE_mosslight"))
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
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one stderr line, got: {stderr:?}"
    );
}

/// Same guarantees as above, on the `TERM=dumb` half of the startup-refusal criterion. A spawned
/// test process never has a TTY of its own, so this can only reach the `TERM=dumb` branch (rather
/// than duplicating the TTY-refusal case above) because `terminal::preflight` checks `TERM` before
/// TTY status (see DECISIONS.md) — asserting on the message content is what makes that the actual
/// branch under test, per round-2 review.
#[test]
fn term_dumb_exits_2_with_one_stderr_line_and_no_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_mosslight"))
        .env("TERM", "dumb")
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
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one stderr line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("dumb"),
        "expected the TERM=dumb refusal message, got: {stderr:?}"
    );
}

/// `--help` is parsed before the TTY preflight, so it must succeed and print to stdout even with
/// no TTY at all.
#[test]
fn help_flag_prints_to_stdout_and_exits_0() {
    let output = Command::new(env!("CARGO_BIN_EXE_mosslight"))
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn mosslight");

    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

/// `--version` is parsed before the TTY preflight, so it must succeed and print to stdout even
/// with no TTY at all.
#[test]
fn version_flag_prints_to_stdout_and_exits_0() {
    let output = Command::new(env!("CARGO_BIN_EXE_mosslight"))
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn mosslight");

    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

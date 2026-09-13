//! CLI parsing, `Config`, and TTY/TERM/`NO_COLOR` probing (spec §12).

use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GlyphSet {
    Ascii,
    Unicode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorArg {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThemeName {
    Gameboy,
    Ansi,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Fps {
    #[value(name = "10")]
    F10,
    #[value(name = "20")]
    F20,
    #[value(name = "30")]
    F30,
}

impl Fps {
    pub fn as_u32(self) -> u32 {
        match self {
            Fps::F10 => 10,
            Fps::F20 => 20,
            Fps::F30 => 30,
        }
    }
}

/// Default seed used when `--seed` is not given, so an unseeded run is still reproducible.
pub const DEFAULT_SEED: u64 = 0x4D6F73736C696768; // "Mosslig" in ASCII hex, arbitrary but fixed.

#[derive(Debug, Parser)]
#[command(name = "mosslight", version, about = "A terminal adventure.")]
struct Cli {
    /// Render with ASCII glyphs (default).
    #[arg(long, conflicts_with = "unicode")]
    ascii: bool,

    /// Render with Unicode glyphs (parsed and stored; ASCII glyphs still render this phase).
    #[arg(long)]
    unicode: bool,

    /// Colour mode.
    #[arg(long, value_enum, default_value_t = ColorArg::Auto)]
    color: ColorArg,

    /// Visual theme.
    #[arg(long, value_enum, default_value_t = ThemeName::Gameboy)]
    theme: ThemeName,

    /// Render frame rate; caps drawing only, never the simulation.
    #[arg(long, value_enum, default_value_t = Fps::F20)]
    fps: Fps,

    /// Save directory (overrides the platform default).
    #[arg(long)]
    save_dir: Option<PathBuf>,

    /// RNG seed.
    #[arg(long)]
    seed: Option<u64>,

    /// Panic deliberately once the terminal guard is up, to exercise the restore-before-panic
    /// path for real (hidden: not part of the public §12 surface).
    #[arg(long, hide = true)]
    debug_panic: bool,

    /// Validate this RON file instead of the embedded world, to exercise the content startup
    /// refusal for real (hidden: not part of the public §12 surface).
    #[arg(long, hide = true)]
    debug_content: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub glyphs: GlyphSet,
    pub color: ColorMode,
    pub theme: ThemeName,
    pub fps: Fps,
    pub save_dir: PathBuf,
    pub seed: u64,
    pub debug_panic: bool,
    pub debug_content: Option<PathBuf>,
}

/// Which stream a [`StartupError`]'s message belongs on. `--help`/`--version` are normal output on
/// stdout with exit code 0 (clap's own convention); a genuine usage error is one line on stderr
/// with exit code 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupError {
    pub message: String,
    pub code: i32,
    pub stream: OutputStream,
}

impl Config {
    /// Parses `argv` and resolves environment-dependent fields (`NO_COLOR`, `TERM`, save-dir).
    /// `--help`/`--version` are reported as exit code 0 on stdout, matching clap's own behaviour;
    /// a genuine usage error (bad flag) is exit code 2 on stderr. Either way this runs before any
    /// terminal state is touched.
    pub fn from_args<I, T>(argv: I, env: &Env) -> Result<Config, StartupError>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let cli = Cli::try_parse_from(argv).map_err(|e| {
            let is_display = matches!(
                e.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayVersion
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            );
            StartupError {
                message: e.to_string(),
                code: if is_display { 0 } else { 2 },
                stream: if is_display {
                    OutputStream::Stdout
                } else {
                    OutputStream::Stderr
                },
            }
        })?;

        let glyphs = if cli.unicode {
            GlyphSet::Unicode
        } else {
            GlyphSet::Ascii
        };

        let color = resolve_color(
            Some(cli.color),
            env.no_color.as_deref(),
            env.stdout_tty,
            env.term.as_deref(),
        );

        let save_dir = cli.save_dir.unwrap_or_else(|| default_save_dir(env));

        Ok(Config {
            glyphs,
            color,
            theme: cli.theme,
            fps: cli.fps,
            save_dir,
            seed: cli.seed.unwrap_or(DEFAULT_SEED),
            debug_panic: cli.debug_panic,
            debug_content: cli.debug_content,
        })
    }
}

/// The environment inputs `Config` needs, gathered as plain data so tests never touch real
/// process environment variables.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub no_color: Option<String>,
    pub term: Option<String>,
    pub stdout_tty: bool,
    pub xdg_data_home: Option<String>,
    pub home: Option<String>,
    pub os_is_macos: bool,
}

impl Env {
    pub fn from_process() -> Self {
        Env {
            no_color: std::env::var("NO_COLOR").ok(),
            term: std::env::var("TERM").ok(),
            stdout_tty: is_stdout_tty(),
            xdg_data_home: std::env::var("XDG_DATA_HOME").ok(),
            home: std::env::var("HOME").ok(),
            os_is_macos: cfg!(target_os = "macos"),
        }
    }
}

#[cfg(unix)]
fn is_stdout_tty() -> bool {
    use ratatui::crossterm::tty::IsTty;
    std::io::stdout().is_tty()
}

#[cfg(not(unix))]
fn is_stdout_tty() -> bool {
    false
}

/// Colour precedence (spec §12): an explicit `--color` always wins; otherwise a non-empty
/// `NO_COLOR` forces `Never`; otherwise `auto` means colour unless stdout is not a TTY or `TERM`
/// is unset/`dumb`.
pub fn resolve_color(
    flag: Option<ColorArg>,
    no_color: Option<&str>,
    stdout_tty: bool,
    term: Option<&str>,
) -> ColorMode {
    match flag {
        Some(ColorArg::Always) => return ColorMode::Always,
        Some(ColorArg::Never) => return ColorMode::Never,
        Some(ColorArg::Auto) | None => {}
    }
    if no_color.is_some_and(|v| !v.is_empty()) {
        return ColorMode::Never;
    }
    if !stdout_tty {
        return ColorMode::Never;
    }
    match term {
        None | Some("") | Some("dumb") => ColorMode::Never,
        _ => ColorMode::Always,
    }
}

fn default_save_dir(env: &Env) -> PathBuf {
    if env.os_is_macos {
        let home = env.home.clone().unwrap_or_default();
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("mosslight")
    } else if let Some(xdg) = env.xdg_data_home.clone().filter(|v| !v.is_empty()) {
        PathBuf::from(xdg).join("mosslight")
    } else {
        let home = env.home.clone().unwrap_or_default();
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("mosslight")
    }
}

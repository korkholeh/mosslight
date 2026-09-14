# CLI reference

```text
mosslight
  --ascii
  --unicode
  --color auto|always|never
  --theme gameboy|ansi|mono
  --fps 10|20|30
  --save-dir PATH
  --seed NUMBER
  --help
  --version
```

| Flag | Default | Notes |
|---|---|---|
| `--ascii` | on | ASCII glyph table (spec §4). The default; mutually exclusive with `--unicode`. |
| `--unicode` | off | Verified-width Unicode glyph table (spec §4's optional enhancement); see "The Unicode decision" below. Mutually exclusive with `--ascii`. |
| `--color <auto\|always\|never>` | `auto` | See precedence below. |
| `--theme <gameboy\|ansi\|mono>` | `gameboy` | Colour only — every theme uses the identical glyph table, so `mono` stays fully legible. `gameboy` is four indexed greens (a 256-colour terminal); `ansi` is the 16-colour fallback for a terminal that cannot show indexed colour; `mono` is white/black-family only. |
| `--fps <10\|20\|30>` | `20` | Caps rendering only. Simulation always runs at a fixed 30 Hz regardless of this flag (spec §9); any other value is a clap usage error, not a silent clamp. |
| `--save-dir PATH` | platform default (below) | Overrides where the save file lives. Pass a scratch directory for manual testing. |
| `--seed NUMBER` | a fixed built-in seed | RNG seed for reproducible runs. |
| `--help` / `--version` | — | Prints to stdout and exits 0 (clap's own convention). |

A hidden `--debug-panic` flag (not part of the stable CLI surface) panics deliberately once the
terminal guard is up, to exercise the restore-before-panic path for real; see
`scripts/terminal-restore-check.sh`.

Running over SSH or under tmux/screen: see `docs/user/ssh.md`.

### The Unicode decision

§4 requires "only characters of verified width." Most Unicode blocks that would look better than
ASCII — box drawing, most of Block Elements, most arrows, most geometric shapes — are
`East_Asian_Width=Ambiguous`: they can silently render two columns wide under a CJK locale or a
terminal's "ambiguous characters are wide" setting, which would shear the fixed 24x16 tile grid.
An earlier revision of this project withdrew `--unicode` entirely for exactly that reason rather
than ship a half-populated or width-unsafe table (see `.autodev/DECISIONS.md`).

`--unicode` is back, on a narrower footing: `render::tiles::unicode_glyph` is a complete,
hand-picked 24-glyph table where every character was individually checked against Unicode's
`East_Asian_Width` property (via Python's `unicodedata`, which encodes the same data as
`EastAsianWidth.txt`) and is `Neutral` or `Narrow` — never `Ambiguous`, `Wide`, or `Fullwidth`.
Because the table is a fixed set of characters chosen at development time, not a width probe over
arbitrary text, no `unicode-width` runtime dependency is needed (`Cargo.toml` stays fixed). A unit
test (`render::tiles::tests::every_unicode_glyph_is_a_verified_safe_codepoint`) pins the table
against the exact characters that were checked, so a future edit that swaps in an unchecked
character fails the gate rather than shipping silently.

`--ascii` and `--unicode` are mutually exclusive; passing both is a usage error (exit code 2).

## Colour precedence

1. An explicit `--color always` or `--color never` always wins.
2. Otherwise, a non-empty `NO_COLOR` environment variable forces `never`.
3. Otherwise (`--color auto` or no flag): colour is enabled unless stdout is not a TTY, or `TERM`
   is unset, empty, or `dumb`.

## Save directory

| Platform | Default |
|---|---|
| macOS | `~/Library/Application Support/mosslight` |
| Linux | `$XDG_DATA_HOME/mosslight`, or `~/.local/share/mosslight` if that variable is unset/empty |

`--save-dir` overrides this unconditionally. There is exactly one save slot per directory:
`save.json`, a retained previous copy `save.json.bak`, and a `save.json.tmp` staging file used
only during a write (see "Corrupt or incompatible saves" below).

### Corrupt or incompatible saves

The game never panics on, and never silently overwrites, a save file it cannot make sense of:

- **Missing** — no `save.json` yet. The main menu's `Continue` reads "Continue (no save yet)" and
  starts nothing until `New Game` is chosen.
- **Corrupt** (empty, truncated, or not valid JSON) — `Continue` reads "Continue (save damaged)"
  and opens a screen offering `Restore backup` (from `save.json.bak`, if one exists), `New game`,
  or `Back`. Nothing is written until one of those is chosen.
- **From a newer version** — a save this build's `format_version` does not recognise.
  `Continue` reads "Continue (save is from a newer version)"; the only option besides `Back` is
  `New game (this save will not be overwritten)` — playing on with an older build never
  overwrites the newer save, so upgrading and downgrading a save directory is always safe.

A save is written atomically (serialize to a temporary file, `fsync`, then rename over the real
one) with the previous copy kept as `save.json.bak`, so a crash mid-write can never corrupt the
slot in place. A save is never written while the hero is at zero health, so retrying after death
always reads the last point the game actually saved, never the moment of death.

## Startup refusal

Before touching raw mode, the game checks that stdin and stdout are both a TTY and that `TERM` is
set to something other than `dumb`. If either check fails, it prints one line to stderr and exits
with code 2 — the same code clap uses for a genuine usage error (an unknown or malformed flag).
`--help` and `--version` are not usage errors: clap prints them to stdout and exits 0, and both are
parsed before this refusal even runs, so they work without a TTY at all. A runtime or write error
exits with code 1; a normal exit is 0.

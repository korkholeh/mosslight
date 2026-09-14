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
| `--ascii` | on | ASCII glyph table (spec §4). Conflicts with `--unicode`. |
| `--unicode` | off | Parsed and stored; renders the same ASCII glyph table until phase 6 adds a checked-width Unicode set. |
| `--color <auto\|always\|never>` | `auto` | See precedence below. |
| `--theme <gameboy\|ansi\|mono>` | `gameboy` | Colour only — every theme uses the identical glyph table, so `mono` stays fully legible. `ansi` aliases `gameboy`'s palette until phase 6. |
| `--fps <10\|20\|30>` | `20` | Caps rendering only. Simulation always runs at a fixed 30 Hz regardless of this flag (spec §9); any other value is a clap usage error, not a silent clamp. |
| `--save-dir PATH` | platform default (below) | Overrides where the save file lives. Pass a scratch directory for manual testing. |
| `--seed NUMBER` | a fixed built-in seed | RNG seed for reproducible runs. |
| `--help` / `--version` | — | Prints to stdout and exits 0 (clap's own convention). |

A hidden `--debug-panic` flag (not part of the stable CLI surface) panics deliberately once the
terminal guard is up, to exercise the restore-before-panic path for real; see
`scripts/terminal-restore-check.sh`.

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

`--save-dir` overrides this unconditionally. Phase 1 does not yet write a save file; see
`.autodev/ROADMAP.md` for when the save subsystem lands.

## Startup refusal

Before touching raw mode, the game checks that stdin and stdout are both a TTY and that `TERM` is
set to something other than `dumb`. If either check fails, it prints one line to stderr and exits
with code 2 — the same code clap uses for a genuine usage error (an unknown or malformed flag).
`--help` and `--version` are not usage errors: clap prints them to stdout and exits 0, and both are
parsed before this refusal even runs, so they work without a TTY at all. A runtime or write error
exits with code 1; a normal exit is 0.

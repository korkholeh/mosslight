# Changelog

All notable user-visible changes to Mosslight are recorded here.

## Unreleased

- First playable build: a main menu (Continue / New Game / Help / Quit), and New Game drops the
  hero into one 24x16 room that can be walked around with the arrow keys or WASD.
- Esc pauses and resumes; Q asks for confirmation before quitting; Ctrl+C does the same as Q. Below
  the 60x24 minimum terminal size, Q/Ctrl+C quit immediately instead, since a confirmation dialog
  cannot be shown at that size.
- The terminal is always left in a usable state on exit — normal quit, an error, or a crash — so a
  broken shell after playing should not happen; `reset` is documented as the fallback if it ever
  does.
- New CLI flags: `--ascii`/`--unicode`, `--color auto|always|never`, `--theme gameboy|ansi|mono`,
  `--fps 10|20|30`, `--save-dir PATH`, `--seed NUMBER`, `--help`, `--version`. `--unicode` and
  `--theme ansi` are accepted but not yet visually distinct from the ASCII/gameboy defaults.
- Continue is present in the main menu but reports "No save yet" — the save subsystem has not
  landed yet.

# 0006. The single save slot is versioned JSON, written atomically, with one retained backup

- **Status:** accepted
- **Date:** 2026-09-13

## Context

§10 specifies one save slot holding the format version, a safe hero position, health and maximum health,
equipment and keys, visited rooms, opened chests and doors, solved puzzles, story flags and the boss
victory — and explicitly *not* the in-flight attack animation or transient combat state. Autosave fires
after a safe transition, an important item, a solved puzzle, and the boss defeat; manual save is available
from the menu outside combat. A death state must never overwrite the last usable save.

Durability rules are named directly: write through a temporary file in the same directory and rename
atomically, keep the previous valid copy, never panic on a corrupt file, never silently overwrite one, and
never overwrite a save whose format version is newer than this build understands.

Paths are fixed: `$XDG_DATA_HOME/mosslight` (falling back to `~/.local/share/mosslight`) on Linux,
`~/Library/Application Support/mosslight` on macOS, with `--save-dir` overriding.

## Decision

- **Format: JSON via `serde_json`**, at `<dir>/save.json`, with `<dir>/save.json.bak` as the retained
  previous copy and `<dir>/save.json.tmp` as the write staging file.
- The top level is `{ "format_version": 1, "game": { … } }`. Loading is **two-pass**: parse into
  `serde_json::Value`, read `format_version` alone, and only then deserialize the body with the matching
  schema. This is what makes refusing a future version safe — we learn the version without having to
  successfully parse a body written by a newer build.
- `load` returns an explicit `LoadOutcome`: `Ok(SaveFile) | Missing | Corrupt { path, detail } |
  FutureVersion { found, supported }`. There is no `unwrap` on this path and no `panic`.
  `Corrupt` offers restoring the backup or starting a new game; `FutureVersion` refuses to load **and**
  refuses to write, so an older binary cannot destroy a newer save.
- **Write sequence:** serialize to memory → write `save.json.tmp` → `sync_all` → copy the existing
  `save.json` to `save.json.bak` → `rename(save.json.tmp, save.json)`. The rename is atomic on POSIX, so
  the file is wholly old or wholly new and there is no third state.
- **Death is never persisted.** The autosave call sites are the four §10 triggers plus the menu; none of
  them is reachable from a death or a boss fight in progress.
- Forward migration is a chain keyed on `format_version` (`v1 -> v2 -> …`). Because ids are stable strings
  (ADR 0005), unknown ids in a save are dropped with a note and missing ids default to "not yet done", so
  adding content is a non-breaking change. Renaming or removing an id requires a migration step and a
  version bump.
- The save path is resolved with ~20 lines of `std::env` matching §10 literally; no path-discovery crate.

## Alternatives considered

- **RON for saves too, for consistency with content** — rejected: probing one field before a full parse is
  the mechanism that makes future-version refusal safe, and `serde_json::Value` makes it two lines. Each
  format is used where it is better: RON for hand-authored content, JSON for a machine-written record that
  must be inspectable and version-probed.
- **A binary format (`bincode`, `postcard`)** — rejected: smaller and faster, but at a few kilobytes neither
  matters, and a corrupt binary save is undiagnosable. A player or a developer being able to open the save
  in a text editor is worth more here than the bytes.
- **Multiple save slots or rolling generations** — rejected by §2 (exactly one slot). One backup is the
  spec's requirement, not a reduced version of something larger.
- **A checksum or HMAC over the save** — rejected: it protects against tampering, which is not a threat in a
  single-player offline game, and the atomic rename already prevents the torn-write case a checksum would
  catch. A parse failure is a sufficient corruption signal.
- **`directories`/`dirs` crate** — rejected: `ProjectDirs` yields
  `~/Library/Application Support/<qualifier>.<org>.<app>` on macOS, not the literal path §10 requires.
- **Saving enemy positions and boss progress** — rejected by §10, and better regardless: respawning enemies
  on load and restarting the boss from the arena entrance removes any state where a player has saved into an
  unwinnable fight.

## Consequences

- Progress survives a crash, a SIGKILL, and a power cut, up to the last autosave point — which is the only
  guarantee available once cleanup handlers cannot be trusted (§11).
- A corrupt or future-version file produces a clear choice in the main menu instead of a panic or silent data
  loss, and the original files are always left on disk for manual recovery.
- Losing transient combat state on load is visible to the player as enemies respawning. The short safe window
  after loading (§10) is what keeps that from being unfair.
- The two-pass read costs one extra parse of a file under 64 KB — irrelevant.
- §13's required tests fall out directly: round-trip, corrupt file, incompatible version, and the rule that
  a death cannot overwrite a usable save.

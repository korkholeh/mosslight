# Mosslight

The forest lighthouse has gone dark. You play the one person who goes looking for why: nine rooms of
overworld — a keeper, a sword in a chest, a lantern behind a floor puzzle, three hidden caches — then
a six-room flooded sanctuary behind a lantern-locked door, a guardian or two, a boss with two phases,
and the ancient ember it was holding. Carry the ember back to the lighthouse and light the beacon.

It is a single-player game that runs entirely inside your terminal, drawn with plain ASCII
characters. There is no mouse, no network play, and no account. A first playthrough takes roughly
30–45 minutes. It runs on macOS and Linux, locally or over SSH.

## Install

There is no download to grab yet — you build it from source. You need Rust (install it from
<https://rustup.rs>); the correct compiler version is pinned in the repository, so `rustup` picks it
up on its own.

```sh
git clone https://github.com/korkholeh/mosslight
cd mosslight
cargo build --release --locked
```

That produces the game at `./target/release/mosslight`. If you would rather type just `mosslight`
from anywhere, run `cargo install --path . --locked` as well.

## Play

```sh
./target/release/mosslight
```

You should see a main menu with **Continue**, **New Game**, **Help** and **Quit**. Use the Up and
Down arrows (or `W` and `S`) to move between them and `E` or `Enter` to choose the highlighted one.
Pick **New Game** and you wake up in the lighthouse.

Two things worth knowing before your first run:

- **Your terminal must be at least 60 columns by 24 rows.** Below that the game pauses and shows you
  the size it needs versus the size you have; resize the window and it picks back up, paused, so
  nothing hits you while you were not looking.
- **The game will not start if it is not attached to a real terminal.** Piping it or redirecting its
  input makes it print one line of explanation and exit. This is deliberate — there is no way to play
  without an interactive terminal.

The essential keys: arrows or `WASD` to move, `J` or `Space` to swing the sword once you have one,
`E` to interact with whatever you are facing, `K` to use the lantern, `M` for the map, `I` for your
inventory, `Esc` to pause, `?` for help, `Q` to quit.

## Save files

There is one save slot. The game saves on its own when you enter a new room, pick up something
important, solve a puzzle, or beat the boss — and you can save by hand from the pause screen. It
never saves while you are at zero health, so dying cannot overwrite the last real save.

When trying things out, pass `--save-dir /tmp/mosslight-scratch` so a test run cannot touch the save
you care about.

## The pages, in the order you will want them

1. **[controls.md](controls.md)** — the full key map, every mode, and what each key does. Read this
   next; it is the one page that makes the game legible.
2. **[cli.md](cli.md)** — every command-line flag: colour themes, frame rate, where the save file
   lives on your platform, and what the game does when a save file is damaged or came from a newer
   version.
3. **[ssh.md](ssh.md)** — only if you want to play on a remote machine. Why you need `ssh -t`, how
   `--fps` behaves on a slow link, and what to do if colours look wrong inside tmux.

## If something goes wrong

- **The game exits immediately with one line about needing an interactive terminal.** You are running
  it through a pipe, a redirect, or an SSH session without a terminal. Run it directly in a terminal
  window, or over `ssh -t`. See [cli.md](cli.md).
- **Your shell looks broken after the game exits** — no echo, or stray characters. The game restores
  your terminal on every exit path it can reach, but a `kill -9` gives it no chance. Run `reset`.
- **Colours look wrong inside tmux.** Try `--theme ansi`, or `--theme mono` for no colour at all.
  Every theme uses the identical characters, so monochrome never hides information that colour shows.
  See [ssh.md](ssh.md).
- **"Continue (save damaged)" on the main menu.** Selecting it offers to restore the automatic backup
  or start fresh. Nothing is written until you choose. See [cli.md](cli.md).

## Working on the game rather than playing it

Developer documentation lives in `docs/dev/` — start with `docs/dev/architecture.md`. The
specification the game was built from is `docs/spec.md` (in Ukrainian).

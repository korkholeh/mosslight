# The loop and the mode machine

## One loop iteration (`src/main.rs`)

```
poll up to the pacer's deadline for the first event, then drain the rest of the
  currently-buffered queue to exhaustion (a hard cap only guards against a livelock;
  events discarded past it are counted into the deferred diagnostics)
  -> map key events to Actions (mode-aware)
  -> apply the overflow policy (cap + limited control-action retention)
  -> coalesce (at most one movement action survives)
on_resize(w, h) if a Resize event arrived (last-seen size wins)
advance_iteration(app, pacer, pending, actions, elapsed_ns):
  sim_actions = app.apply(actions)   // mode transitions/menu nav happen here, every iteration
  pending.extend(sim_actions); coalesce(pending)
  due = pacer.advance(elapsed_ns) -> Due { sim_steps, draw }
  for each due sim step: app.tick(pending on the first step, [] afterwards);
                         pending.clear() once the first step has run
if due.draw and app.take_dirty(): draw once
break on quit or a pending SIGTERM/SIGHUP
```

`pending` is a `Vec<Action>` owned by `main.rs` and carried across iterations, not a local. A
keypress almost always arrives in an iteration whose elapsed time does not cross a 33.3 ms tick
boundary (`due.sim_steps == 0`, since the poll deadline is tick-based) — `pending` is what stops
that action from being silently dropped instead of applied on the next iteration that actually
ticks. `App::apply` itself has no buffer of its own: it resolves mode transitions and menu
navigation immediately on every call, and only what is meant for the simulation is merged into
`pending` by `advance_iteration`.

Simulation always runs at a fixed 30 Hz (`game::tuning::TICK_HZ`); `--fps` only caps how often the
frame is drawn. A stall is capped at `MAX_CATCHUP_STEPS` (5) simulation steps per iteration, and
the surplus accumulated time is discarded rather than carried forward, so a hang cannot cause a
burst of accelerated simulation afterward. `Pacer` (`src/app.rs`) is clock-free — `main.rs` is the
only place that touches `Instant`. `App::tick` marks a frame dirty only when the simulation step
actually produced an event, so standing still while `Playing` does not force a draw every frame.

## Input policy

`src/input.rs` maps `KeyCode` only; no key event's press/release kind is ever inspected
(`tests/no_key_release.rs` checks this structurally, by scanning the source for the relevant
crossterm identifiers). Per iteration, at most `INPUT_EVENTS_PER_ITER` (32) mapped actions are kept
as-is; beyond that, only up to 8 control actions (Quit/Cancel/Confirm) from the surplus survive, so
a 200-event burst cannot replay as stale movement, but a queued quit is not silently dropped.
`coalesce` then collapses movement to at most one action. `App::apply` additionally drops
everything collected so far in the current batch the moment a Cancel/Confirm closes an overlay
back into `Playing`, so a movement key queued right behind it cannot leak into gameplay the same
iteration.

## Mode machine (`src/app.rs`)

```
MainMenu --Confirm(Continue), slot Usable--> Playing (state restored from the slot)
MainMenu --Confirm(Continue), slot Empty--> MainMenu ("No save yet")
MainMenu --Confirm(Continue), slot Corrupt/FutureVersion--> SaveProblem
MainMenu --Confirm(New Game), slot Empty/Corrupt--> Playing (fresh state)
MainMenu --Confirm(New Game), slot Usable/FutureVersion--> ConfirmNewGame
ConfirmNewGame --Confirm--> Playing (fresh state)  --Cancel--> MainMenu
SaveProblem --Confirm(Restore backup)--> Playing (state restored from save.json.bak), or stays
    on SaveProblem with a message if the backup is missing/damaged/newer
SaveProblem --Confirm(New game)--> Playing (fresh state)
SaveProblem --Confirm(Back)/Cancel--> MainMenu
MainMenu --Confirm(Help)--> Help --Cancel/Help--> MainMenu
MainMenu --Quit--> ConfirmQuit --Cancel--> MainMenu
                              --Confirm--> (process exits 0)
Playing  --Cancel--> Paused --Cancel--> Playing
Paused   --Confirm--> Paused (manual save; refused during combat, message either way)
Playing  --Help--> Help --Cancel/Help--> Playing
Playing  --Quit--> ConfirmQuit --Cancel--> Playing
Playing  --ToggleMap--> Map --ToggleMap/Cancel--> Playing
Playing  --ToggleInventory--> Inventory --ToggleInventory/Cancel--> Playing
Playing  --Interact on a talkative NPC (DialogueStarted)--> Dialogue
Dialogue --Confirm (advances a node; past the last node, or Cancel)--> Playing (DialogueEnded)
Playing  --health reaches 0 (HeroDied)--> GameOver
GameOver --Confirm (retry)--> Playing (state restored from the last autosave, or fresh if the
                                        slot is empty; never writes)
GameOver --Cancel--> MainMenu
GameOver --Quit--> ConfirmQuit --Cancel--> GameOver
Playing  --the beacon is interacted with while carrying the ember (GameWon)--> Victory
Victory  --Confirm/Cancel--> MainMenu
Victory  --Quit--> ConfirmQuit --Cancel--> Victory
any mode --resize below 60x24--> TooSmall
TooSmall --Quit (Q or Ctrl+C)--> (process exits 0, no confirmation)
TooSmall --resize back to >=60x24-->
    Paused, if the mode being recovered into was Playing (so the hero can't take an unseen hit)
    otherwise whatever mode was active before TooSmall
```

## The save port and the autosave triggers (`src/save.rs`, `src/app.rs`)

`App` never touches the filesystem itself: `App::new`'s third argument is a `Box<dyn SaveIo>`
(`FileSaveIo` from `main`, `MemorySaveIo` from every test), and `App` decides only *when* to call
`load`/`store` on it. The slot is probed once at startup into `SlotState` (`Empty` / `Usable` /
`Corrupt` / `FutureVersion`) and refreshed after every successful `store` or backup restore —
`Continue`, retry, and the `SaveProblem` screen all read `App::slot`, never the disk directly.

Four events autosave, at most one write per tick even if several fire together in the same batch
(each is an edge, so in practice at most one ever does): `RoomEntered`, an `ItemPicked` for
anything but a lore `Reward::Message`, `PuzzleSolved`, `BossDefeated`. If the same tick's batch
also contains `HeroDied`, the pending autosave is dropped instead — a death state is never
written, enforced again at the file layer (`save::store`) so the rule holds even if `App`'s side
of it is ever bypassed. Manual save (`Paused` + `Confirm`) is refused whenever
`GameState::in_combat()` is true (a live swing, a still-running invulnerability window, or any
live enemy in the room). A save failure (disk full, permissions) never ends the session: it is
reported once in the message row and once via `App::take_diagnostics()` (drained by `main` into
the same buffered `Diagnostics` that flush to stderr after the terminal guard drops), and play
continues — the next trigger simply retries.

Loading (`Continue`, retry, or a backup restore) goes through `SaveFile::restore`: `tick` resets to
0 with every hero timer rebuilt relative to it, a short invulnerability window
(`tuning::LOAD_SAFE_WINDOW_TICKS`) is granted, and `GameState::enter_room()` respawns enemies and
resets puzzle state exactly as a live room transition would — a defeated boss stays defeated via
its `defeat_flag`. An authored id in the save that no longer resolves (content renamed or removed)
is dropped rather than failing the whole load; the count reaches the message row and diagnostics.

`TooSmall` is the one mode where `Quit` skips `ConfirmQuit` entirely: a confirmation dialog cannot
be rendered at a too-small size, and a shrunk terminal must still leave the player a keyboard way
out (spec §11).

`App::tick` only advances the simulation (and its own tick counter) while `mode == Playing`; every
other mode leaves the tick counter untouched. `App::apply` returns the actions meant for this
iteration's simulation step — mode transitions and menu navigation are resolved as a side effect
of the same call, in the order the actions arrived, so a mode change partway through a batch
changes how the rest of that batch is interpreted.

`Map` and `Inventory` are read-only projections of `app.state` (`render/overlays.rs`'s
`draw_map`/`draw_inventory`); `ToggleMap`/`ToggleInventory` are intercepted directly in
`apply_playing` rather than reaching the simulation, and closing either goes through the same
`apply()` overlay-close rule as `Paused`/`Help`. `Dialogue` is different: it is simulation state
(`GameState::dialogue`, set by `Interact`'s `DialogueStarted` event), not just an app-level mode,
because a flag a dialogue node sets (`Progress.flags`) has to land inside the pure `update()`
pipeline. `App::apply_dialogue` routes `Confirm`/`Cancel` straight into `update()` at the
*current* tick (not an incremented one), so a dialogue responds within the same iteration and the
tick counter provably does not advance while it is open — `update()`'s own dialogue branch (see
`docs/dev/content.md` or `game::state::update`) is what actually suspends the per-tick pipeline;
`App` just mirrors `DialogueStarted`/`DialogueEnded` into `Mode::Dialogue`/`Mode::Playing`, and
`GameEvent::GameWon` the same way into `Mode::Victory`.

## Layout budget (`src/render/scene.rs`)

A fixed 50x21 block, centred by floor division (`x0 = (w-50)/2`, `y0 = (h-21)/2`):

| Row (relative to `y0`) | Content |
|---|---|
| 0 | HUD: health, active item, key count |
| 1..=18 | Bordered 50x18 scene box; interior 48x16 = 24 tiles x 2 columns each |
| 19 | Message line |
| 20 | Hint line |

Tile `(tx, ty)`'s glyph lands at column `x0 + 1 + 2*tx`, row `y0 + 2 + ty`. Below 60x24, the game
shows a `TooSmall` notice with the required and current size instead of the scene.

Glyphs (`src/render/tiles.rs`) are the single source of truth for what each tile/entity looks like;
themes (`src/render/theme.rs`) only ever return a colour, never a glyph, so a monochrome theme
cannot lose information a colour theme has.

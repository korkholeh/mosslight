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
MainMenu --Confirm(New Game)--> Playing
MainMenu --Confirm(Help)--> Help --Cancel/Help--> MainMenu
MainMenu --Quit--> ConfirmQuit --Cancel--> MainMenu
                              --Confirm--> (process exits 0)
Playing  --Cancel--> Paused --Cancel--> Playing
Playing  --Help--> Help --Cancel/Help--> Playing
Playing  --Quit--> ConfirmQuit --Cancel--> Playing
any mode --resize below 60x24--> TooSmall
TooSmall --Quit (Q or Ctrl+C)--> (process exits 0, no confirmation)
TooSmall --resize back to >=60x24-->
    Paused, if the mode being recovered into was Playing (so the hero can't take an unseen hit)
    otherwise whatever mode was active before TooSmall
```

`TooSmall` is the one mode where `Quit` skips `ConfirmQuit` entirely: a confirmation dialog cannot
be rendered at a too-small size, and a shrunk terminal must still leave the player a keyboard way
out (spec §11).

`App::tick` only advances the simulation (and its own tick counter) while `mode == Playing`; every
other mode leaves the tick counter untouched. `App::apply` returns the actions meant for this
iteration's simulation step — mode transitions and menu navigation are resolved as a side effect
of the same call, in the order the actions arrived, so a mode change partway through a batch
changes how the rest of that batch is interpreted.

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

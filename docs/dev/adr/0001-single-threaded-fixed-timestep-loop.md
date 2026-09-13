# 0001. The game runs as a single-threaded, fixed-timestep loop with poll-based input

- **Status:** accepted
- **Date:** 2026-09-13

## Context

The spec fixes the simulation at 30 updates per second and caps rendering at 20 fps by default, with
`--fps` affecting rendering only (§9). It requires no busy-waiting, at most 5 catch-up steps after a stall,
and the surplus accumulated time discarded. It states that a single thread is sufficient for this scope and
forbids an async runtime (§8). Separately, §13 requires that an identical seed plus an identical action
sequence produce an identical simulation.

The conventional ratatui pattern is a dedicated input thread pushing events into a channel. That pattern
exists to avoid blocking the render loop on `read()`, which `crossterm::event::poll(timeout)` already
solves.

Over SSH the loop must also survive input arriving in bursts: a hang or a batched flush must not make the
hero execute dozens of stale moves (§5).

## Decision

One thread. One loop. Its iteration is:

1. Compute the deadline for the next 1/30 s tick.
2. `crossterm::event::poll(deadline - now)` and drain events until the deadline, reading **at most 32
   events per iteration**. Map each to a semantic `Action`, then coalesce: redundant movement actions
   collapse, and at most one movement step is applied per tick.
3. Advance the simulation by whole fixed steps, **at most 5 per iteration**; discard any accumulated time
   beyond that.
4. Render only if the frame interval has elapsed **and** something changed (simulation ran, or a dirty flag
   was set by a mode/state change).

Time enters the simulation only as an integer tick count. `Instant` is used for pacing in the loop and
nowhere else. Menus, map, inventory, dialogue, pause, and the too-small-terminal screen stop step 3
entirely, so the simulation is frozen rather than running unobserved.

When a menu or dialogue closes, pending game actions are discarded so buffered keys do not leak into play.

## Alternatives considered

- **Dedicated input thread + channel** — rejected: `poll` with a deadline already gives non-blocking input,
  and a second thread introduces a synchronization boundary into the one subsystem that must be provably
  deterministic. It would also make "an autosave can never run reentrantly" an argument rather than a fact.
- **`tokio` / async runtime** — rejected by §8, and there is nothing to await: no network, no concurrent
  I/O, one fixed-rate loop.
- **Variable timestep with `dt` as a float** — rejected: it makes the simulation depend on render rate,
  which §15 explicitly forbids, and destroys reproducibility.
- **Sleeping a fixed interval and polling with a zero timeout** — rejected: it either busy-waits or adds
  latency, and §9 forbids the former.

## Consequences

- Input latency is bounded by one tick (33 ms) plus one render period, independent of load.
- The simulation is a pure function of (initial state, seed, action sequence) — the §13 determinism test and
  the headless playthrough test become straightforward, with no terminal and no clock involved.
- Any long-running work inside the loop would freeze the UI. There is none in this design; if a future
  feature needs one (say, a large content import), it must be chunked across ticks, not moved to a thread
  without revisiting this ADR.
- Burst input cannot accumulate into a long queue of stale moves: the per-iteration read cap and the
  coalescing step bound it structurally rather than by tuning.
- We would revisit this if the simulation stopped fitting in a 33 ms budget, which at 15 rooms and ≤ 12
  enemies per room it does not.

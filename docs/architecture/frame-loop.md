# The frame loop

**Decided:** 2026-09-02, during work unit `0001-hello-world-entrypoint`.
**Status:** in force. Every later system depends on it.

## The rule

**The main thread owns the clock. The worker owns rendering.**

```
main thread (boot.ts)                worker (worker.ts → wasm)
─────────────────────                ─────────────────────────
requestAnimationFrame(t)
  └─ postMessage {tick, t}  ───────▶  frame(t)
                                        └─ Renderer::render(elapsed)
                                             └─ OffscreenCanvas
```

## Why

Dedicated Web Workers have no `requestAnimationFrame`. It is tied to the
document's rendering lifecycle, which workers do not have, so only the main
thread can observe vsync. The alternatives inside the worker —
`setTimeout`/`setInterval` — are not vsync-aligned and will tear or stutter.

The game itself still lives in the worker. The main thread does no work per
frame beyond forwarding a timestamp, so a slow frame in the simulation cannot
block input or the DOM.

## Consequences

- **Rust owns the epoch.** `frame()` takes the raw rAF timestamp in
  milliseconds and treats the first one it sees as t=0. The shell forwards
  rAF's value unchanged, so time semantics live in one place rather than being
  split across two languages.
- **Ticks arrive before the GPU is ready** — the main thread starts its loop as
  soon as it has posted the canvas. Both sides drop early ticks; that is
  correct, not a race to fix.
- **A tick is not a frame.** The renderer skips frames when the surface is
  unusable (resizing, canvas hidden). Anything counting frames must count
  *presented* frames, which is why `frames_presented()` exists.
- **No `rAF` inside the worker, ever.** If some future system needs its own
  cadence, it must be driven from a tick, not from a timer.

## Measured

~120 fps in Chrome 152 on Windows against an NVIDIA RTX 2080 Ti, headed and
headless. One `postMessage` per frame is not a bottleneck at this scale. If it
ever becomes one, the fix is a `SharedArrayBuffer` for the tick — which needs
COOP/COEP headers on the dev server, currently not set.

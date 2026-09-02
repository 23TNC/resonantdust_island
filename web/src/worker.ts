/**
 * The game worker.
 *
 * Loads the wasm module, hands it the OffscreenCanvas, and forwards ticks and
 * resizes to it. There is no game logic here and there should never be any —
 * this file exists to bridge the worker message loop onto the Rust exports.
 */

import init, { start, frame, resize } from './generated/island_web.js'
// Explicit URL rather than letting the generated glue resolve the wasm itself.
// The glue falls back to `new URL('island_web_bg.wasm', import.meta.url)`,
// which relies on Vite rewriting that expression correctly inside a worker
// bundle. Naming the asset removes the dependency on that rewrite.
// See docs/work/0001-hello-world-entrypoint/issues.md §4.
import wasmUrl from './generated/island_web_bg.wasm?url'
import type { FromWorker, GpuReport, ToWorker } from './protocol'

// `self` types as a Window under the DOM lib; in a worker it is not one.
const ctx = self as unknown as DedicatedWorkerGlobalScope

function post(message: FromWorker): void {
  ctx.postMessage(message)
}

/** Set once the renderer exists and is willing to draw. */
let running = false
/** Guards against a second 'init' if the page ever sends one. */
let booting = false

async function boot(canvas: OffscreenCanvas): Promise<void> {
  if (booting || running) return
  booting = true
  try {
    await init({ module_or_path: wasmUrl })

    // `start` is typed `Promise<any>`: it returns a plain JS object so the
    // report survives being postMessage'd on to the main thread, and a plain
    // object carries no type information. GpuReport is the hand-maintained
    // description of its shape.
    const report = (await start(canvas)) as GpuReport

    running = true
    post({ type: 'ready', report })
  } catch (err) {
    // Surface failures on the page rather than only in devtools — the browser
    // under test is on Windows while the editor is in WSL, so "open the
    // console" is a more expensive instruction here than it looks.
    post({ type: 'error', message: err instanceof Error ? err.message : String(err) })
  } finally {
    booting = false
  }
}

ctx.onmessage = (event: MessageEvent<ToWorker>) => {
  const message = event.data

  switch (message.type) {
    case 'init':
      void boot(message.canvas)
      break

    case 'tick':
      // Ticks start arriving as soon as the main thread has posted the canvas,
      // which is well before the GPU is up. Dropping the early ones is correct.
      if (running) frame(message.t)
      break

    case 'resize':
      if (running) resize(message.width, message.height)
      break
  }
}

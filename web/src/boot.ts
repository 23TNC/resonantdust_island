/**
 * Main-thread boot shell.
 *
 * Responsibilities, and nothing beyond them:
 *   1. Hand the canvas to the worker as an OffscreenCanvas.
 *   2. Own the frame clock — dedicated workers have no requestAnimationFrame,
 *      so rAF lives here and the worker renders on the ticks it is sent.
 *   3. Report the worker's status to the page.
 */

import type { FromWorker, GpuReport, ToWorker } from './protocol'

const canvas = document.getElementById('viewport') as HTMLCanvasElement | null
const statusEl = document.getElementById('status')

function fail(message: string): never {
  if (statusEl) {
    statusEl.textContent = message
    statusEl.dataset.state = 'error'
  }
  throw new Error(message)
}

if (!canvas) fail('no <canvas id="viewport"> in the document')
if (!statusEl) throw new Error('no #status element in the document')

if (typeof Worker === 'undefined') {
  fail('This browser has no Web Workers. The game runs entirely inside one.')
}
if (typeof canvas.transferControlToOffscreen !== 'function') {
  fail(
    'This browser cannot transfer a canvas to a worker (no ' +
      'transferControlToOffscreen). OffscreenCanvas is required.',
  )
}

/**
 * Canvas size in physical pixels.
 *
 * The CSS size and the backing-store size are different things; rendering at
 * the CSS size on a HiDPI display gives a soft image. wgpu sets the canvas
 * backing size itself when it configures the surface, so the worker only needs
 * to be told the number.
 */
function physicalSize(el: HTMLElement): { width: number; height: number } {
  const dpr = window.devicePixelRatio || 1
  const rect = el.getBoundingClientRect()
  return {
    width: Math.max(1, Math.round(rect.width * dpr)),
    height: Math.max(1, Math.round(rect.height * dpr)),
  }
}

// Size before transferring: once the canvas is transferred, the main thread
// may no longer touch its dimensions.
const initial = physicalSize(canvas)
canvas.width = initial.width
canvas.height = initial.height

// `new URL(..., import.meta.url)` is the form Vite recognises for bundling a
// worker; a bare string path would not be rewritten for the production build.
const worker = new Worker(new URL('./worker.ts', import.meta.url), {
  type: 'module',
  name: 'island-game',
})

function send(message: ToWorker, transfer?: Transferable[]): void {
  if (transfer) worker.postMessage(message, transfer)
  else worker.postMessage(message)
}

function renderReport(report: GpuReport): void {
  const lines = [
    `adapter    ${report.adapterName}`,
    `backend    ${report.backend}`,
    `type       ${report.deviceType}`,
    `driver     ${report.driver || '(not reported)'}`,
    `surface    ${report.width}x${report.height}`,
  ]
  statusEl!.textContent = lines.join('\n')

  // A software rasteriser renders the triangle perfectly and tells us nothing
  // about GPU support, which is the entire reason this is tested in Windows
  // Chrome rather than in WSL. Make it impossible to miss.
  statusEl!.dataset.state = report.isSoftware ? 'software' : 'ok'
  if (report.isSoftware) {
    statusEl!.textContent += '\n\nSOFTWARE ADAPTER — not a real GPU.'
  }
}

worker.onmessage = (event: MessageEvent<FromWorker>) => {
  const message = event.data
  switch (message.type) {
    case 'ready':
      renderReport(message.report)
      break
    case 'error':
      statusEl!.textContent = message.message
      statusEl!.dataset.state = 'error'
      break
  }
}

worker.onerror = (event) => {
  statusEl!.textContent = `worker failed: ${event.message}`
  statusEl!.dataset.state = 'error'
}

const offscreen = canvas.transferControlToOffscreen()
send({ type: 'init', canvas: offscreen }, [offscreen])

const observer = new ResizeObserver(() => {
  const { width, height } = physicalSize(canvas)
  send({ type: 'resize', width, height })
})
observer.observe(canvas)

// The frame clock. Ticks sent before the worker is ready are dropped on the
// other side, so there is no need to wait for 'ready' to start.
function tick(t: number): void {
  send({ type: 'tick', t })
  requestAnimationFrame(tick)
}
requestAnimationFrame(tick)

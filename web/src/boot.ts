/**
 * Main-thread boot shell.
 *
 * Responsibilities, and nothing beyond them:
 *   1. Hand the canvas to the worker as an OffscreenCanvas.
 *   2. Own the frame clock — dedicated workers have no requestAnimationFrame,
 *      so rAF lives here and the worker renders on the ticks it is sent.
 *   3. Report the worker's status to the page.
 */

import type { FromWorker, GpuReport, ToWorker, WebGpuIdentity } from './protocol'

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

/** Vendor strings that mean a software rasteriser rather than a GPU. */
const SOFTWARE_VENDORS = /swiftshader|llvmpipe|lavapipe|software|basic render|warp|microsoft/i
/** Vendor strings that positively identify real hardware. */
const HARDWARE_VENDORS = /nvidia|amd|ati|intel|apple|qualcomm|arm|imagination|broadcom|mesa/i

/**
 * Decide whether we are on real hardware, from two independent signals.
 *
 * Neither is sufficient alone. wgpu's `isSoftware` is derived from
 * `DeviceType::Cpu`, which on the WebGPU backend is Chrome's
 * `isFallbackAdapter` — reliable when true, but wgpu reports every non-fallback
 * adapter as "Other" with a blank name, so a pass carries no positive
 * evidence. The vendor string supplies that evidence.
 */
function classify(
  report: GpuReport,
  identity: WebGpuIdentity | null,
): { state: 'ok' | 'software' | 'unknown'; reason: string } {
  if (report.isSoftware) {
    return { state: 'software', reason: 'wgpu reports a CPU/fallback adapter' }
  }
  if (identity?.isFallbackAdapter === true) {
    return { state: 'software', reason: 'WebGPU reports a fallback adapter' }
  }
  const vendor = identity?.vendor ?? ''
  if (vendor && SOFTWARE_VENDORS.test(vendor)) {
    return { state: 'software', reason: `vendor "${vendor}" is a software rasteriser` }
  }
  if (vendor && HARDWARE_VENDORS.test(vendor)) {
    return { state: 'ok', reason: `vendor "${vendor}" is real hardware` }
  }
  // Not a fallback adapter, but nothing positively identifies the hardware
  // either. Not a failure, but not proof.
  return { state: 'unknown', reason: 'adapter vendor not reported by the browser' }
}

function renderReport(report: GpuReport, identity: WebGpuIdentity | null): void {
  const { state, reason } = classify(report, identity)

  const lines = [
    `vendor     ${identity?.vendor || '(withheld)'}`,
    `arch       ${identity?.architecture || '(withheld)'}`,
    // wgpu leaves this blank on WebGPU — Chrome withholds the description it
    // is mapped from. Shown anyway so the blank is explained rather than
    // looking like a bug.
    `adapter    ${report.adapterName || '(not exposed by WebGPU)'}`,
    `backend    ${report.backend}`,
    `type       ${report.deviceType}`,
    `surface    ${report.width}x${report.height}`,
    ``,
    `${state.toUpperCase()} — ${reason}`,
  ]
  statusEl!.textContent = lines.join('\n')

  // A software rasteriser renders the triangle perfectly and tells us nothing
  // about GPU support, which is the entire reason this is tested in Windows
  // Chrome rather than in WSL. Make it impossible to miss.
  statusEl!.dataset.state = state
  // Read by the smoke test, which needs the verdict rather than the prose.
  document.documentElement.dataset.gpu = state
  document.documentElement.dataset.vendor = identity?.vendor ?? ''
}

worker.onmessage = (event: MessageEvent<FromWorker>) => {
  const message = event.data
  switch (message.type) {
    case 'ready':
      renderReport(message.report, message.identity)
      break
    case 'error':
      statusEl!.textContent = message.message
      statusEl!.dataset.state = 'error'
      break
    case 'stats':
      // Published on the document so the smoke test can read it twice and
      // confirm it is climbing. Presented frames, not ticks sent.
      document.documentElement.dataset.frames = String(message.frames)
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

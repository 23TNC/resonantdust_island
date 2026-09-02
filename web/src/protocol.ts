/**
 * The message protocol between the main thread and the game worker.
 *
 * Both sides import these types so a change to one is a compile error on the
 * other. Everything crossing this boundary must be structured-cloneable —
 * postMessage clones its argument, and anything holding a pointer into wasm
 * memory will not survive the trip.
 */

/**
 * What the renderer found when it acquired a GPU.
 *
 * MUST match `report_to_js` in `crates/island_web/src/lib.rs`. The two agree by
 * hand: the report is built as a plain JS object (so it survives postMessage),
 * which means wasm-bindgen types it as `any` and cannot check this for us.
 */
export interface GpuReport {
  /** Graphics backend, e.g. "BrowserWebGpu". */
  backend: string
  /** Adapter name as reported by the driver. */
  adapterName: string
  /** "DiscreteGpu", "IntegratedGpu", "Cpu", ... */
  deviceType: string
  /** Driver name and version, where the backend exposes them. */
  driver: string
  /**
   * True when this looks like a software rasteriser.
   *
   * The reason the browser under test has to be on Windows rather than in WSL:
   * a software adapter renders correctly and proves nothing about GPU support.
   */
  isSoftware: boolean
  /** Surface size in physical pixels. */
  width: number
  height: number
}

/**
 * Adapter identity read from the raw WebGPU API.
 *
 * Needed because wgpu cannot supply it. On the WebGPU backend wgpu maps
 * `AdapterInfo.name` from `GPUAdapterInfo.description`, which Chrome leaves
 * empty for fingerprinting reasons — so the Rust-side report has a blank
 * adapter name and a `deviceType` of "Other" no matter what hardware is
 * present. `vendor` and `architecture` *are* exposed, so the shell reads them
 * directly and reports them alongside.
 *
 * This is what makes "is it a real GPU?" answerable rather than a guess.
 */
export interface WebGpuIdentity {
  /** e.g. "nvidia", "amd", "intel", "apple". Empty if withheld. */
  vendor: string
  /** e.g. "turing", "rdna-3". Empty if withheld. */
  architecture: string
  /** Almost always empty in Chrome. */
  description: string
  /**
   * WebGPU's own software-adapter flag. `null` when the browser does not
   * expose it, which Chrome 152 does not — hence the vendor check as well.
   */
  isFallbackAdapter: boolean | null
}

/** Main thread → worker. */
export type ToWorker =
  /** Hand over the canvas. Sent once, with the canvas in the transfer list. */
  | { type: 'init'; canvas: OffscreenCanvas }
  /**
   * Draw a frame. `t` is the requestAnimationFrame timestamp, forwarded
   * unchanged — Rust treats the first one it sees as t=0.
   *
   * The main thread owns the clock because dedicated workers have no
   * requestAnimationFrame. See docs/work/0001-hello-world-entrypoint/issues.md §3.
   */
  | { type: 'tick'; t: number }
  /** The canvas changed size. Dimensions are physical pixels, not CSS. */
  | { type: 'resize'; width: number; height: number }

/** Worker → main thread. */
export type FromWorker =
  /** The GPU is up and the first frame can be drawn. */
  | { type: 'ready'; report: GpuReport; identity: WebGpuIdentity | null }
  /** Boot failed. Carries a human-readable reason for the page to display. */
  | { type: 'error'; message: string }
  /**
   * Frames actually presented so far, sampled periodically.
   *
   * Exists so an automated check can tell a running loop from a single frame
   * that happened to be drawn. The count comes from the renderer, so it counts
   * presented frames — not ticks sent, which would prove nothing.
   */
  | { type: 'stats'; frames: number }

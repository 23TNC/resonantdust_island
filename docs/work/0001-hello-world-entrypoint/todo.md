# 0001 — Hello World Entrypoint

**Status:** `[ ]` not started
**Goal:** Stand up the smallest end-to-end slice of the real architecture: a
Rust program compiled to WebAssembly, running inside a Web Worker, driving
`wgpu` against a real GPU through WebGPU, drawing to an `OffscreenCanvas` on
the page — booted by a thin HTML + TypeScript shell, and verified in Chrome on
Windows.

This is not a throwaway spike. The crate layout, the worker boundary, and the
build scripts created here are the ones the game is built on top of.

---

## Why this shape

The three architectural decisions being locked in, and why:

1. **Rust runs in a Web Worker, not on the main thread.** A survival RPG has a
   simulation tick that must not be starved by DOM work, and a main thread that
   must stay responsive for input. Putting the game in a worker from day one
   avoids a painful retrofit later — the alternative (start on the main thread,
   move later) forces every `web_sys::window()` call to be rewritten.

2. **Rendering goes through `OffscreenCanvas`.** It is the only way a worker can
   own a `wgpu::Surface`. The main thread calls
   `canvas.transferControlToOffscreen()` once, posts the handle to the worker,
   and never touches the canvas again.

3. **The Rust side is split into `island_core` and `island_web`.** `island_core`
   holds the renderer and game loop and knows nothing about `wasm_bindgen`;
   `island_web` is a thin `cdylib` that adapts the browser to it. This seam is
   what makes a native desktop build possible later, and native is where GPU
   debugging is far easier than in a browser.

---

## Target file tree

```
resonantdust_island/
├── Cargo.toml                      # workspace root
├── rust-toolchain.toml             # pin the toolchain
├── .gitignore
├── crates/
│   ├── island_core/                # platform-agnostic: renderer + frame loop
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── renderer.rs
│   │       └── hello.wgsl
│   └── island_web/                 # cdylib: wasm-bindgen entrypoint
│       ├── Cargo.toml
│       └── src/lib.rs
├── web/
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── boot.ts                 # main thread: worker + canvas transfer + rAF
│       ├── worker.ts               # worker: init wasm, call start()
│       └── generated/              # wasm-bindgen output (gitignored)
└── scripts/
    ├── build-wasm.sh
    ├── dev.sh
    └── test-chrome.mjs             # chromedriver smoke check
```

---

## Tasks

### A. Repository scaffolding

- [ ] `A1` Create the Cargo workspace root `Cargo.toml` with
      `members = ["crates/*"]` and a `[workspace.dependencies]` block so crate
      versions are pinned in one place.
- [ ] `A2` Add `rust-toolchain.toml` pinning the stable channel and declaring
      `targets = ["wasm32-unknown-unknown"]`, so the target is installed
      automatically on a fresh checkout.
- [ ] `A3` Add `.gitignore` covering `target/`, `node_modules/`,
      `web/src/generated/`, `web/dist/`.
- [ ] `A4` `git init` — the working directory is not yet a repository.

### B. `island_core` — the engine crate

- [ ] `B1` Create `crates/island_core` as a normal `lib` crate. Dependencies:
      `wgpu`, `log`, `bytemuck`, `thiserror`.
- [ ] `B2` Write `renderer.rs`:
      - `Renderer::new(target: impl Into<wgpu::SurfaceTarget<'static>>, width, height)`
        — async. Requests an adapter with
        `PowerPreference::HighPerformance`, requests a device, configures the
        surface.
      - Store `adapter.get_info()` so the caller can report backend/adapter
        name — this is the evidence that we are on a real GPU and not a
        software fallback.
      - `Renderer::render(&mut self, t: f32)` — acquire frame, begin a render
        pass with a clear colour, draw the triangle, submit, present.
      - `Renderer::resize(&mut self, w, h)` — reconfigure the surface.
- [ ] `B3` Write `hello.wgsl`: a vertex shader that generates a full-ish
      triangle from `@builtin(vertex_index)` (no vertex buffer needed) and a
      fragment shader that colours it. Feed a `f32` time through a small
      uniform buffer so the triangle visibly animates — a still image cannot
      distinguish "rendered once" from "loop is running".
- [ ] `B4` `lib.rs` re-exports `Renderer` and a `HelloReport` struct
      (`backend`, `adapter_name`, `is_software`) that the shell can display.

### C. `island_web` — the wasm entrypoint

- [ ] `C1` Create `crates/island_web` with `crate-type = ["cdylib", "rlib"]`.
      Dependencies: `island_core`, `wasm-bindgen`, `wasm-bindgen-futures`,
      `js-sys`, `web-sys` (features: `OffscreenCanvas`, `WorkerGlobalScope`,
      `WorkerNavigator`, `console`), `console_error_panic_hook`, `log`,
      `console_log`.
- [ ] `C2` **Pin `wasm-bindgen` to `=0.2.127`.** The installed CLI is 0.2.127
      and the crate and CLI must agree exactly or `wasm-bindgen` aborts with a
      schema-version mismatch. Verify with `wasm-bindgen --version`.
- [ ] `C3` Export `#[wasm_bindgen(start)]`-style init that installs
      `console_error_panic_hook` and `console_log`, so a Rust panic in the
      worker surfaces as a readable JS stack instead of `unreachable`.
- [ ] `C4` Export `pub async fn start(canvas: web_sys::OffscreenCanvas) -> JsValue`
      — builds the `Renderer`, and returns the `HelloReport` fields as a plain
      JS object for the shell to display.
- [ ] `C5` Export `pub fn frame(t: f32)` and `pub fn resize(w: u32, h: u32)`,
      backed by a thread-local `RefCell<Option<Renderer>>`. (Wasm in a worker
      is single-threaded here, so `thread_local!` is the right storage — no
      `Mutex`, no `unsafe`.)
- [ ] `C6` Log `"hello world from Rust"` plus the adapter info on startup.

### D. Build pipeline

- [ ] `D1` `scripts/build-wasm.sh`:
      ```
      cargo build -p island_web --target wasm32-unknown-unknown --release
      wasm-bindgen --target web --out-dir web/src/generated \
        target/wasm32-unknown-unknown/release/island_web.wasm
      ```
      Accept a `--debug` flag that swaps `--release` for a debug build and adds
      `--keep-debug` to `wasm-bindgen`.
- [ ] `D2` Add `set -euo pipefail` and make the script runnable from any cwd
      (resolve paths relative to the script location).

### E. The TypeScript shell

- [ ] `E1` `web/package.json` — dev dependencies `vite` and `typescript` only.
      Scripts: `dev`, `build`, `preview`.
- [ ] `E2` `web/vite.config.ts` — `server.host = true` so the dev server binds
      `0.0.0.0` and is reachable from Windows; `server.port = 5173`.
- [ ] `E3` `web/index.html` — a `<canvas id="viewport">`, a `<pre id="status">`
      for the adapter report, and a module script tag loading `boot.ts`.
      Include a `no-JS`/`no-worker` fallback message.
- [ ] `E4` `web/src/boot.ts`:
      - Size the canvas to the device pixel ratio.
      - `new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })`
        — the `new URL` form is what lets Vite bundle the worker.
      - `const off = canvas.transferControlToOffscreen()` then
        `worker.postMessage({ type: 'init', canvas: off }, [off])`.
      - `requestAnimationFrame` loop posting `{ type: 'tick', t }` to the
        worker. **Dedicated workers do not have `requestAnimationFrame`** — the
        main thread is the clock, the worker is the renderer. Settling this now
        is the point of including a loop in a "hello world".
      - `ResizeObserver` posting `{ type: 'resize', w, h }`.
      - Render the `ready` message from the worker into `#status`.
- [ ] `E5` `web/src/worker.ts`:
      - `import init, { start, frame, resize } from './generated/island_web.js'`
      - On `init` message: `await init()`, `await start(canvas)`, post the
        report back as `{ type: 'ready', report }`.
      - On `tick`/`resize`: forward to the wasm exports. Drop ticks that arrive
        before `start()` resolves.
      - Wrap the boot in try/catch and post `{ type: 'error', message }` so a
        failure is visible on the page rather than only in devtools.
- [ ] `E6` `web/tsconfig.json` — `"lib": ["ES2022", "DOM", "WebWorker"]`,
      `"types": ["vite/client"]`, strict on.

### F. Run it on Windows

- [ ] `F1` Start the dev server from WSL: `scripts/dev.sh` → `npm run dev` in
      `web/`.
- [ ] `F2` Confirm Windows can reach it. Try `http://localhost:5173` **first** —
      WSL2 forwards listening ports to the Windows loopback, and `localhost` is
      a *secure context*, which WebGPU requires. `http://172.16.15.60:5173`
      will serve the page but `navigator.gpu` will be `undefined` there. See
      `issues.md` §1.
- [ ] `F3` Launch Windows Chrome from WSL:
      ```
      "/mnt/c/Program Files/Google/Chrome/152.0.7977.42/chrome-win64/chrome.exe" \
        --user-data-dir=C:\\temp\\rd-island-profile \
        http://localhost:5173
      ```
      A dedicated `--user-data-dir` keeps the test profile out of the user's
      real Chrome profile and avoids "Chrome is already running" no-ops.
- [ ] `F4` Verify by eye:
      - canvas is filled with the clear colour, not white/black default
      - the triangle is drawn and **animating**
      - `#status` names a real adapter (e.g. a discrete/integrated GPU via the
        `Dawn`/D3D12 backend), **not** SwiftShader or a warp/software device
      - devtools console shows `hello world from Rust`
- [ ] `F5` Cross-check against `chrome://gpu` in the same profile: WebGPU must
      be listed as hardware accelerated.

### G. Repeatable automated check

- [ ] `G1` `scripts/test-chrome.mjs` — drive `chromedriver.exe` over WebDriver:
      launch Chrome, load the page, wait for `#status` to be populated, read
      the adapter report and the browser console log, assert no `error`
      message was posted, and assert the adapter is not a software fallback.
- [ ] `G2` Reaching a Windows-side `chromedriver` from WSL means talking to a
      port on the Windows host, not `localhost`. Resolve the host IP from the
      default gateway / `/etc/resolv.conf` nameserver. See `issues.md` §5.
- [ ] `G3` Decide headless vs headed for the automated run and record the
      decision. Headless must still use the real GPU — if it silently falls
      back to SwiftShader the check is worthless, so the software-adapter
      assertion in `G1` is what makes headless safe to trust.

### H. Wrap up

- [ ] `H1` Fill in `issues.md` with everything actually hit — the open items
      below are predictions, not findings.
- [ ] `H2` Record the frame-loop decision (main-thread rAF drives the worker)
      in `docs/` as an architecture note, since every later system depends on
      it.
- [ ] `H3` Note in `issues.md` any version pins that turned out to be load-
      bearing, so the next unit of work does not undo them.

---

## Acceptance criteria

The unit of work is done when all of the following hold:

1. `scripts/build-wasm.sh` succeeds from a clean `target/`.
2. `npm run dev` in `web/` serves the page.
3. Chrome on Windows, at `http://localhost:5173`, shows an animating triangle
   on a cleared canvas.
4. The page displays the WebGPU adapter name and backend, and it is a **real
   GPU adapter**, not a software rasteriser.
5. `hello world from Rust` appears in the browser console, logged from Rust
   code running inside a Web Worker.
6. No uncaught errors in the console.
7. `scripts/test-chrome.mjs` exits 0.
8. `issues.md` reflects what actually happened.

---

## Explicitly out of scope

Named so they do not creep in:

- Input handling (keyboard/mouse/gamepad) beyond what the shell needs to boot.
- Textures, sprites, atlases, text rendering, any asset pipeline.
- ECS or any game-state architecture.
- Audio.
- Native desktop build (the crate seam is created; the build is not).
- Any game content whatsoever.
- Save/load, persistence, IndexedDB.
- CI.

---

## Open questions to answer while doing the work

- **Which `wgpu` version?** Latest published is `30.0.1`. Confirm its
  `wasm-bindgen` requirement is compatible with the pinned `=0.2.127`; if it is
  not, the CLI gets upgraded to match the crate, not the other way round.
- **Does Vite resolve the wasm asset correctly inside a module worker?**
  `wasm-bindgen --target web` glue locates the `.wasm` via
  `new URL('island_web_bg.wasm', import.meta.url)`. If Vite mangles that in the
  worker bundle, the fallback is to pass an explicit URL to `init()`.
- **`shared-array-buffer` / cross-origin isolation?** Not needed now (no
  threads), but if `wgpu` or a future `rayon`-style thread pool needs it, the
  dev server will need COOP/COEP headers. Note whether the answer is yes now so
  it is not a surprise later.

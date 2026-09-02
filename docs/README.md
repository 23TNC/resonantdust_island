# Documentation

Project documentation for the game (working title: **Resonant Dust Island**).

A single-player survival RPG with adult content, built in Rust, rendered with
`wgpu` (WebGPU), and shipped to run in a browser.

## Layout

| Path         | Purpose                                                              |
|--------------|----------------------------------------------------------------------|
| `work/`      | Planned units of work. One folder per unit. See `work/README.md`.     |

## Design pillars (reference for planning)

- **Rust + wgpu.** All simulation and rendering logic lives in Rust. `wgpu`
  targets WebGPU in the browser and native backends (Vulkan/D3D12/Metal) on
  desktop, so the same renderer can ship to both.
- **Browser-first.** The primary distribution target is the browser. The Rust
  program is compiled to `wasm32-unknown-unknown` and runs inside a **Web
  Worker**, so the simulation and render loop never block the main thread.
  Rendering reaches the page through an `OffscreenCanvas` transferred to the
  worker.
- **Thin TypeScript shell.** HTML/TS exists only to boot the worker, hand it
  the canvas, and forward input/lifecycle events. No game logic in TypeScript.

## Development environment

Development happens in WSL2, but **the browser must be run on Windows, not in
WSL**. WSL2 has no usable GPU driver path for WebGPU, so `wgpu` falls back to a
software adapter or fails to acquire one at all. Testing against a real GPU
requires Windows-side Chrome:

```
C:\Program Files\Google\Chrome\152.0.7977.42\chrome-win64\chrome.exe
C:\Program Files\Google\Chrome\152.0.7977.42\chromedriver-win64\chromedriver.exe
```

From WSL these are reachable as `/mnt/c/Program Files/Google/Chrome/...`.

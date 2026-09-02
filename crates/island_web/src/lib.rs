//! Browser entrypoint for Resonant Dust Island.
//!
//! This crate is the adapter between the browser and [`island_core`]. It is
//! deliberately thin: it owns no game logic, only the wasm-bindgen exports the
//! Web Worker calls and the state needed to hold a [`Renderer`] between them.
//!
//! # Threading
//!
//! The wasm module runs inside a dedicated Web Worker and is single-threaded,
//! so renderer state lives in a `thread_local!` `RefCell`. No `Mutex`, no
//! `unsafe`, no `Send`/`Sync` bounds — which matters, because `wgpu`'s web
//! types are neither.
//!
//! # Who owns the clock
//!
//! Dedicated workers have no `requestAnimationFrame`, so the main thread runs
//! the rAF loop and calls [`frame`] with the timestamp it was handed. Rust
//! owns the epoch: the first timestamp seen becomes t=0, so the shell forwards
//! rAF's value verbatim and time semantics stay in one place.
//!
//! # Why the whole crate is cfg-gated
//!
//! `wgpu::SurfaceTarget::OffscreenCanvas` exists only on wasm targets, so this
//! crate cannot compile natively. Rather than let that break `cargo test
//! --workspace` and `cargo check --workspace` — which developers and CI run
//! without thinking about targets — the crate compiles to nothing off wasm.
//! `island_core` holds everything worth testing on the host anyway.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use island_core::world::{world_hash, WorldParams};
use island_core::{FrameStatus, HelloReport, Renderer};
use wasm_bindgen::prelude::*;

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

struct State {
    renderer: Renderer,
    /// Timestamp of the first frame, used as t=0.
    epoch_ms: Option<f64>,
    frames: u64,
    /// Checksum of the generated world, asserted against the native test value
    /// so a wasm/native divergence in generation cannot pass unnoticed.
    world_hash: u64,
    world_tiles: usize,
    /// Where the automatic camera pan is centred.
    pan_center: (f32, f32),
    /// How far the pan travels from centre, in world units.
    pan_radius: f32,
}

/// Module init. Runs automatically when the worker instantiates the wasm.
///
/// Installing the panic hook here — before any of our code can run — is what
/// turns a Rust panic into a readable JS stack trace instead of a bare
/// `unreachable executed`.
#[wasm_bindgen(start)]
pub fn boot() {
    console_error_panic_hook::set_once();
    // Errors here mean a logger is already installed, which is harmless.
    let _ = console_log::init_with_level(log::Level::Debug);
    log::info!("hello world from Rust — island_web module initialised");
}

/// Acquire a GPU and build the renderer for `canvas`.
///
/// Returns a plain JS object describing the adapter, for the shell to display.
/// Rejects if WebGPU is unavailable or no adapter can be acquired; the worker
/// catches that and forwards it to the page, so a failure is visible without
/// opening devtools.
#[wasm_bindgen]
pub async fn start(canvas: web_sys::OffscreenCanvas) -> Result<JsValue, JsValue> {
    // Check for WebGPU before handing off to wgpu. wgpu's "no adapter" error
    // is accurate but does not say *why*, and by far the most common cause
    // during development is an insecure context — a page served from a bare
    // IP rather than localhost. Diagnosing that from "no suitable adapter"
    // wastes a lot of time. See docs/work/0001-*/issues.md §1.
    if !webgpu_available() {
        return Err(JsValue::from_str(
            "navigator.gpu is undefined: this browser has no WebGPU here. \
             Most likely the page is not in a secure context — use \
             http://localhost or https://, not a bare IP address. \
             Otherwise check chrome://gpu.",
        ));
    }

    let width = canvas.width();
    let height = canvas.height();
    log::info!("starting renderer on {width}x{height} OffscreenCanvas");

    // wgpu has no blanket conversion for `OffscreenCanvas` — its only `From`
    // impl for `SurfaceTarget` covers raw window handles — so the variant is
    // named explicitly. Building the platform-specific target here is exactly
    // this crate's job; `island_core` stays free of browser types.
    let target = island_core::wgpu::SurfaceTarget::OffscreenCanvas(canvas);

    let mut renderer = Renderer::new(target, width, height)
        .await
        .map_err(|err| JsValue::from_str(&format!("renderer init failed: {err}")))?;

    let params = WorldParams::default();
    let map = params.generate();
    let hash = world_hash(&map);
    let tiles = map.tile_count();
    log::info!(
        "generated {}x{} world, seed {}, hash {:#018x}",
        map.width(),
        map.depth(),
        params.seed,
        hash
    );
    renderer.load_world(&map);

    let report = renderer.report().clone();
    let (width, height) = renderer.size();
    let pan_center = (map.width() as f32 * 0.5, map.depth() as f32 * 0.5);

    STATE.with_borrow_mut(|state| {
        *state = Some(State {
            renderer,
            epoch_ms: None,
            frames: 0,
            world_hash: hash,
            world_tiles: tiles,
            // Small enough that the view never reaches the world edge: the
            // camera's own half-width is roughly 44 * aspect, so a radius much
            // above 24 puts empty space on screen at the extremes of the pan.
            pan_radius: (map.width() as f32 * 0.09).min(24.0),
            pan_center,
        });
    });

    report_to_js(
        &report,
        width,
        height,
        hash,
        tiles,
        map.width(),
        map.depth(),
    )
}

/// Draw one frame.
///
/// `timestamp_ms` is the `requestAnimationFrame` timestamp from the main
/// thread, forwarded unchanged.
#[wasm_bindgen]
pub fn frame(timestamp_ms: f64) {
    STATE.with_borrow_mut(|state| {
        // Ticks can arrive before `start` resolves, because the main thread
        // begins its rAF loop as soon as it has posted the canvas. Dropping
        // them is correct.
        let Some(state) = state.as_mut() else {
            return;
        };

        let epoch = *state.epoch_ms.get_or_insert(timestamp_ms);
        let elapsed = ((timestamp_ms - epoch) / 1000.0) as f32;

        // Drift the camera in a slow circle. There is no input system yet, and
        // a static view cannot show that the world is larger than one screen.
        let angle = elapsed * 0.15;
        state.renderer.set_camera_focus(
            state.pan_center.0 + angle.cos() * state.pan_radius,
            state.pan_center.1 + angle.sin() * state.pan_radius,
        );

        if state.renderer.render(elapsed) == FrameStatus::Presented {
            state.frames += 1;
            // Evidence that the loop actually runs, without spamming the
            // console every frame.
            if state.frames == 1 {
                log::info!("first frame presented");
            } else if state.frames % 600 == 0 {
                log::debug!("{} frames presented ({elapsed:.1}s)", state.frames);
            }
        }
    });
}

/// Reconfigure the surface after the canvas changed size.
#[wasm_bindgen]
pub fn resize(width: u32, height: u32) {
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state.as_mut() {
            state.renderer.resize(width, height);
        }
    });
}

/// Number of frames presented so far. Used by the smoke test to prove the
/// loop is advancing rather than a single frame having been drawn.
#[wasm_bindgen]
pub fn frames_presented() -> f64 {
    STATE.with_borrow(|state| state.as_ref().map_or(0, |s| s.frames) as f64)
}

/// The generated world's checksum, as hex.
///
/// A string rather than a number: the hash is 64-bit and JavaScript numbers
/// lose precision above 2^53, so returning it numerically would corrupt the
/// value the smoke test is meant to compare exactly.
#[wasm_bindgen]
pub fn world_hash_hex() -> String {
    STATE.with_borrow(|state| {
        state.as_ref().map_or_else(
            || "unloaded".to_string(),
            |s| format!("{:#018x}", s.world_hash),
        )
    })
}

/// Chunks drawn versus total after culling, plus triangles, for the readout.
#[wasm_bindgen]
pub fn frame_stats() -> JsValue {
    STATE.with_borrow(|state| {
        let obj = js_sys::Object::new();
        if let Some(state) = state.as_ref() {
            let stats = state.renderer.stats();
            let _ = set(&obj, "chunksDrawn", &(stats.chunks_drawn as u32).into());
            let _ = set(&obj, "chunksTotal", &(stats.chunks_total as u32).into());
            let _ = set(&obj, "triangles", &(stats.triangles_drawn as u32).into());
            let _ = set(&obj, "tiles", &(state.world_tiles as u32).into());
        }
        obj.into()
    })
}

/// Move the camera's focus on the ground plane.
#[wasm_bindgen]
pub fn set_camera_focus(x: f32, z: f32) {
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state.as_mut() {
            state.renderer.set_camera_focus(x, z);
        }
    });
}

/// Set the zoom, as half the visible height in world units.
#[wasm_bindgen]
pub fn set_camera_zoom(half_height: f32) {
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state.as_mut() {
            state.renderer.set_camera_half_height(half_height);
        }
    });
}

/// Is WebGPU reachable from this global scope?
///
/// Uses `Reflect` rather than typed `web_sys` accessors so the same code works
/// in a worker (`WorkerNavigator`) and on the main thread (`Navigator`), and
/// so no extra `web-sys` features are needed.
fn webgpu_available() -> bool {
    let global = js_sys::global();
    let Ok(navigator) = js_sys::Reflect::get(&global, &JsValue::from_str("navigator")) else {
        return false;
    };
    if navigator.is_undefined() || navigator.is_null() {
        return false;
    }
    match js_sys::Reflect::get(&navigator, &JsValue::from_str("gpu")) {
        Ok(gpu) => !gpu.is_undefined() && !gpu.is_null(),
        Err(_) => false,
    }
}

/// Convert a [`HelloReport`] into a plain JS object.
///
/// It must be a *plain* object, not a `#[wasm_bindgen]` struct: the worker
/// forwards this to the main thread with `postMessage`, which structured-clones
/// its argument. A wasm-bindgen struct is a JS wrapper around a pointer into
/// wasm memory — it is not structured-cloneable, and would either throw or
/// arrive as something meaningless on the other side.
#[allow(clippy::too_many_arguments)]
fn report_to_js(
    report: &HelloReport,
    width: u32,
    height: u32,
    world_hash: u64,
    tiles: usize,
    world_width: u32,
    world_depth: u32,
) -> Result<JsValue, JsValue> {
    let obj = js_sys::Object::new();
    set(&obj, "backend", &report.backend.as_str().into())?;
    set(&obj, "adapterName", &report.adapter_name.as_str().into())?;
    set(&obj, "deviceType", &report.device_type.as_str().into())?;
    set(&obj, "driver", &report.driver.as_str().into())?;
    set(&obj, "isSoftware", &report.is_software.into())?;
    set(&obj, "width", &width.into())?;
    set(&obj, "height", &height.into())?;
    // Hex string, not a number: 64 bits does not survive a JS number.
    set(
        &obj,
        "worldHash",
        &format!("{world_hash:#018x}").as_str().into(),
    )?;
    set(&obj, "tiles", &(tiles as u32).into())?;
    set(&obj, "worldWidth", &world_width.into())?;
    set(&obj, "worldDepth", &world_depth.into())?;
    Ok(obj.into())
}

fn set(obj: &js_sys::Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    js_sys::Reflect::set(obj, &JsValue::from_str(key), value)?;
    Ok(())
}

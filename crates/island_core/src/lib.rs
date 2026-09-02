//! Engine core for Resonant Dust Island.
//!
//! Everything here is platform-agnostic. Browser-specific glue lives in
//! `island_web`, which adapts an `OffscreenCanvas` and the worker message loop
//! onto this crate. Keeping that seam means a native desktop build stays
//! reachable without rewriting the renderer — and native is where GPU
//! debugging is far less painful than in a browser.
//!
//! At this stage the crate contains only [`Renderer`], enough to prove the
//! Rust → wasm → Web Worker → wgpu → `OffscreenCanvas` path end to end.

mod renderer;

pub use renderer::{FrameStatus, HelloReport, Renderer, RendererError};

/// Re-export of the `wgpu` we were built against.
///
/// Platform crates need wgpu types to construct a surface target — an
/// `OffscreenCanvas` on the web has no blanket `Into<SurfaceTarget>` impl and
/// must name its variant. Going through this re-export makes it structurally
/// impossible for a platform crate to link a different wgpu version than the
/// renderer, which would fail in confusing ways at the trait boundary.
pub use wgpu;

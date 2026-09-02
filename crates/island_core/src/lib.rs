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

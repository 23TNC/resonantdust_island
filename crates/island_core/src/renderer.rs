//! The wgpu renderer.
//!
//! This module is deliberately free of browser types. It takes anything that
//! can become a [`wgpu::SurfaceTarget`] — an `OffscreenCanvas` on the web, a
//! window handle natively — so the same renderer serves both targets.

use std::borrow::Cow;

/// Background the frame is cleared to before the triangle is drawn.
///
/// Deliberately not black: a black canvas is indistinguishable from a canvas
/// that was never drawn to, which is exactly the failure we are trying to
/// detect during bring-up.
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.043,
    g: 0.055,
    b: 0.098,
    a: 1.0,
};

/// What the renderer found when it acquired a GPU.
///
/// Surfaced all the way to the page so a human (and the smoke test) can tell a
/// real GPU from a software rasteriser. See `docs/work/0001-*/issues.md` §6 for
/// why that distinction is the whole point of testing on Windows.
#[derive(Debug, Clone)]
pub struct HelloReport {
    /// Graphics backend in use, e.g. `BrowserWebGpu`, `Vulkan`, `Dx12`.
    pub backend: String,
    /// Adapter name as reported by the driver.
    pub adapter_name: String,
    /// `DiscreteGpu`, `IntegratedGpu`, `Cpu`, ...
    pub device_type: String,
    /// Driver name and version, when the backend exposes them.
    pub driver: String,
    /// True when we appear to be on a software rasteriser rather than a GPU.
    pub is_software: bool,
}

impl HelloReport {
    fn from_adapter_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            backend: format!("{:?}", info.backend),
            adapter_name: info.name.clone(),
            device_type: format!("{:?}", info.device_type),
            driver: if info.driver.is_empty() {
                info.driver_info.clone()
            } else {
                format!("{} {}", info.driver, info.driver_info)
            },
            is_software: is_software_adapter(info),
        }
    }
}

/// Best-effort detection of a software rasteriser.
///
/// `DeviceType::Cpu` is the reliable signal, but browsers do not always report
/// it accurately for their fallback path, so the well-known software adapter
/// names are checked too. Over-reporting is the safe direction: a false
/// positive fails a test that a human then looks at, while a false negative
/// lets a meaningless "pass" through.
fn is_software_adapter(info: &wgpu::AdapterInfo) -> bool {
    if info.device_type == wgpu::DeviceType::Cpu {
        return true;
    }
    const SOFTWARE_MARKERS: [&str; 6] = [
        "swiftshader",
        "llvmpipe",
        "lavapipe",
        "softpipe",
        "basic render", // "Microsoft Basic Render Driver"
        "warp",         // Direct3D's software rasteriser
    ];
    let name = info.name.to_ascii_lowercase();
    SOFTWARE_MARKERS.iter().any(|m| name.contains(m))
}

#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("could not create a surface for the render target: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),

    #[error(
        "no WebGPU adapter available: {0}. In a browser this usually means \
         WebGPU is unavailable — most often because the page is not in a secure \
         context (use http://localhost, not a bare IP)"
    )]
    RequestAdapter(#[from] wgpu::RequestAdapterError),

    #[error("could not acquire a GPU device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    #[error("adapter and surface are incompatible; no default surface configuration")]
    NoDefaultConfig,
}

/// Per-frame data handed to the shader. Field order and size must match
/// `Uniforms` in `hello.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    time: f32,
    aspect: f32,
    // Uniform-address-space structs must be a multiple of 16 bytes.
    _padding: [f32; 2],
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    report: HelloReport,
    /// Set when the surface reported itself suboptimal; acted on after the
    /// current frame is presented.
    needs_reconfigure: bool,
}

/// Outcome of a [`Renderer::render`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameStatus {
    /// A frame was drawn and handed to the compositor.
    Presented,
    /// The surface was not usable this frame and nothing was drawn. Normal
    /// during resizes and while the canvas is hidden — try again next tick.
    Skipped,
}

impl Renderer {
    /// Acquire a GPU and build the hello-world pipeline.
    ///
    /// `target` is whatever this platform renders into. On the web that is an
    /// `OffscreenCanvas` owned by the worker.
    pub async fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, RendererError> {
        // wgpu 30: `InstanceDescriptor` has no `Default`; on the web there is
        // no display handle to hand it.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(target)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                // Never silently accept a software adapter. If a real one is
                // unavailable we want the error, not a fake success.
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await?;

        let report = HelloReport::from_adapter_info(&adapter.get_info());
        log::info!(
            "adapter: {} ({}, {}) driver={:?} software={}",
            report.adapter_name,
            report.backend,
            report.device_type,
            report.driver,
            report.is_software
        );
        if report.is_software {
            log::warn!(
                "this looks like a SOFTWARE adapter ({}) — rendering will work but \
                 proves nothing about GPU support",
                report.adapter_name
            );
        }

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("island primary device"),
                required_features: wgpu::Features::empty(),
                // Ask for exactly what this adapter offers; requesting more
                // than it has is the usual cause of a failed device request.
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await?;

        // `get_default_config` picks a surface format the adapter actually
        // supports, which varies by browser and platform.
        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(RendererError::NoDefaultConfig)?;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hello triangle shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("hello.wgsl"))),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hello uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hello uniforms layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hello uniforms bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hello pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            // wgpu 30's replacement for push constants. Unused here.
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hello pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            uniform_buffer,
            bind_group,
            report,
            needs_reconfigure: false,
        })
    }

    /// What we are rendering on. Cheap to clone; the shell displays it.
    pub fn report(&self) -> &HelloReport {
        &self.report
    }

    /// Current surface size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Reconfigure for a new surface size. A zero dimension is ignored —
    /// configuring a zero-sized surface is an error, and it happens routinely
    /// when a canvas is hidden.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        if width == self.config.width && height == self.config.height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        log::debug!("surface resized to {width}x{height}");
    }

    /// Draw one frame. `time` is seconds since start and drives the animation.
    ///
    /// A skipped frame is a normal occurrence, not an error — see
    /// [`FrameStatus`].
    pub fn render(&mut self, time: f32) -> FrameStatus {
        let uniforms = Uniforms {
            time,
            aspect: self.config.width as f32 / self.config.height.max(1) as f32,
            _padding: [0.0; 2],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // wgpu 30 reports swapchain trouble as enum variants rather than an
        // `Err`, because none of these are failures the caller can act on —
        // the right response to every one of them is to try again next tick.
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,

            // Still a usable texture; the surface just wants reconfiguring.
            // Draw this frame, then reconfigure so the next one is clean.
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                log::debug!("surface suboptimal; reconfiguring after this frame");
                self.needs_reconfigure = true;
                frame
            }

            // Stale swapchain, typically a resize we have not been told about.
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                log::debug!("surface outdated or lost; reconfiguring");
                self.surface.configure(&self.device, &self.config);
                return FrameStatus::Skipped;
            }

            // Nothing is wrong and nothing is visible. Do not burn a frame.
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return FrameStatus::Skipped;
            }

            // A validation error was raised and caught by an error scope. This
            // one is on us — it means the surface configuration is wrong — so
            // it gets logged loudly rather than silently skipped.
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!(
                    "surface validation error acquiring frame at {}x{}",
                    self.config.width,
                    self.config.height
                );
                return FrameStatus::Skipped;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("hello encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hello pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            // Three vertices, generated in the shader from their index.
            pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        // wgpu 30 moved `present` from `SurfaceTexture` onto `Queue`.
        self.queue.present(frame);

        if std::mem::take(&mut self.needs_reconfigure) {
            self.surface.configure(&self.device, &self.config);
        }

        FrameStatus::Presented
    }
}

#[cfg(test)]
mod tests {
    /// Parse and validate the shader without needing a GPU.
    ///
    /// `create_shader_module` only compiles WGSL at runtime, which on the web
    /// means a typo costs a full rebuild-and-reload to discover. This runs the
    /// same frontend wgpu uses, in `cargo test`.
    #[test]
    fn hello_wgsl_is_valid() {
        let source = include_str!("hello.wgsl");

        let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|err| {
            panic!(
                "hello.wgsl failed to parse:\n{}",
                err.emit_to_string(source)
            )
        });

        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|err| panic!("hello.wgsl failed validation: {err:?}"));
    }

    /// The uniform struct is shared with the shader by memory layout alone, so
    /// a size change on either side silently corrupts every field. WGSL
    /// requires uniform structs to be a multiple of 16 bytes.
    #[test]
    fn uniforms_match_shader_layout() {
        assert_eq!(std::mem::size_of::<super::Uniforms>(), 16);
    }
}

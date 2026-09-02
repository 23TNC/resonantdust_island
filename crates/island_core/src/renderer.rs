//! The wgpu renderer.
//!
//! Free of browser types: it takes anything convertible to a
//! [`wgpu::SurfaceTarget`], so the same renderer serves an `OffscreenCanvas`
//! on the web and a window handle natively.

use std::borrow::Cow;

use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::camera::Camera;
use crate::world::{mesh_chunk, ChunkPos, TileMap, Vertex};

/// Depth format. 32-bit float is universally supported on WebGPU and gives
/// ample precision over the camera's 1..2000 unit range.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Background the frame is cleared to.
///
/// Deliberately not black: a black canvas is indistinguishable from one that
/// was never drawn to, which is exactly the failure we want to notice.
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.043,
    g: 0.055,
    b: 0.098,
    a: 1.0,
};

/// Direction the sunlight travels.
///
/// Angled rather than vertical on purpose. Straight-down light leaves every
/// vertical wall lit only by ambient, and the walls are what make elevation
/// readable. Tilted toward -X and -Z so the two wall directions facing the
/// camera are lit differently from each other, which is what gives the terrain
/// its sense of relief.
const LIGHT_DIRECTION: Vec3 = Vec3::new(-0.42, -0.80, -0.43);

/// Brightness of a face turned fully away from the light.
const AMBIENT: f32 = 0.38;

/// What the renderer found when it acquired a GPU.
///
/// Surfaced to the page so a human and the smoke test can tell a real GPU from
/// a software rasteriser. See `docs/work/0001-*/issues.md` §6 for why that
/// distinction is the point of testing on Windows.
#[derive(Debug, Clone)]
pub struct HelloReport {
    pub backend: String,
    pub adapter_name: String,
    pub device_type: String,
    pub driver: String,
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
/// On the WebGPU backend this is much weaker than it looks: wgpu maps the
/// adapter name from `GPUAdapterInfo.description`, which Chrome leaves empty,
/// and reports `DeviceType::Cpu` only for an explicit fallback adapter. A
/// `false` here is therefore not positive evidence of hardware — the shell
/// reads the WebGPU vendor string separately for that. See
/// `docs/work/0001-*/issues.md` §6.
fn is_software_adapter(info: &wgpu::AdapterInfo) -> bool {
    if info.device_type == wgpu::DeviceType::Cpu {
        return true;
    }
    const SOFTWARE_MARKERS: [&str; 6] = [
        "swiftshader",
        "llvmpipe",
        "lavapipe",
        "softpipe",
        "basic render",
        "warp",
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

/// Per-frame uniforms. Layout must match `Camera` in `terrain.wgsl`.
///
/// `vec3` aligns to 16 bytes in WGSL's uniform address space, so `ambient`
/// packs into the padding after `light_dir` rather than adding any.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 3],
    ambient: f32,
}

/// GPU-side geometry for one chunk, plus the bounds used to cull it.
struct GpuChunk {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    min: Vec3,
    max: Vec3,
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

/// Counts from the last frame, for the debug readout.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub chunks_total: usize,
    pub chunks_drawn: usize,
    pub triangles_drawn: usize,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera: Camera,
    chunks: Vec<GpuChunk>,
    stats: FrameStats,
    report: HelloReport,
    needs_reconfigure: bool,
}

impl Renderer {
    /// Acquire a GPU and build the terrain pipeline.
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
                // Never silently accept a software adapter.
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
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await?;

        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(RendererError::NoDefaultConfig)?;
        surface.configure(&device, &config);

        let depth_view = create_depth_view(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("terrain.wgsl"))),
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniforms"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera layout"),
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, // position
                        1 => Float32x3, // normal
                        2 => Float32x3, // color
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                // Counter-clockwise is wgpu's default front face, but culling
                // is NOT on by default — `cull_mode` is an Option that
                // defaults to None. Set explicitly from the first version:
                // with culling off, a wall wound the wrong way is visible from
                // both sides and the bug stays hidden until it is turned on.
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // Option-wrapped in wgpu 30, unlike every example in
                // circulation where these are plain values.
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
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

        let camera = Camera {
            aspect: config.width as f32 / config.height.max(1) as f32,
            ..Camera::default()
        };

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_view,
            pipeline,
            camera_buffer,
            camera_bind_group,
            camera,
            chunks: Vec::new(),
            stats: FrameStats::default(),
            report,
            needs_reconfigure: false,
        })
    }

    /// Mesh a world and upload it, replacing whatever was loaded before.
    ///
    /// One vertex and index buffer per chunk, uploaded once. Meshing is a
    /// startup cost, not a per-frame one — see `0002` `issues.md` §6.
    pub fn load_world(&mut self, map: &TileMap) {
        self.chunks.clear();

        for chunk in map.chunks().collect::<Vec<ChunkPos>>() {
            let mesh = mesh_chunk(map, chunk);
            if mesh.is_empty() {
                continue;
            }
            let Some((min, max)) = map.chunk_bounds(chunk) else {
                continue;
            };

            let vertex_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("chunk vertices"),
                    contents: bytemuck::cast_slice(mesh.vertices()),
                    usage: wgpu::BufferUsages::VERTEX,
                });
            let index_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("chunk indices"),
                    contents: bytemuck::cast_slice(mesh.indices()),
                    usage: wgpu::BufferUsages::INDEX,
                });

            self.chunks.push(GpuChunk {
                vertex_buffer,
                index_buffer,
                index_count: mesh.indices().len() as u32,
                min,
                max,
            });
        }

        // Frame the world by default, so a freshly loaded world is on screen
        // without the caller having to know where it is.
        self.camera.focus = Vec3::new(map.width() as f32 * 0.5, 0.0, map.depth() as f32 * 0.5);

        log::info!(
            "uploaded {} chunks ({} triangles)",
            self.chunks.len(),
            self.chunks
                .iter()
                .map(|c| c.index_count as usize / 3)
                .sum::<usize>()
        );
    }

    pub fn report(&self) -> &HelloReport {
        &self.report
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn stats(&self) -> FrameStats {
        self.stats
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Move the camera's focus on the ground plane.
    pub fn set_camera_focus(&mut self, x: f32, z: f32) {
        self.camera.focus.x = x;
        self.camera.focus.z = z;
    }

    /// Set the zoom, as half the visible height in world units.
    pub fn set_camera_half_height(&mut self, half_height: f32) {
        self.camera.half_height = half_height.max(1.0);
    }

    /// Reconfigure for a new surface size.
    ///
    /// The depth texture is a separate resource of a fixed size and **must**
    /// be recreated here. Forgetting it is the classic bug: everything works
    /// until the window changes size, then geometry draws through geometry.
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
        self.depth_view = create_depth_view(&self.device, &self.config);
        self.camera.aspect = width as f32 / height.max(1) as f32;
        log::debug!("surface resized to {width}x{height}");
    }

    /// Draw one frame. `time` is seconds since start.
    pub fn render(&mut self, time: f32) -> FrameStatus {
        let uniform = CameraUniform {
            view_proj: self.camera.view_projection().to_cols_array_2d(),
            light_dir: LIGHT_DIRECTION.normalize().to_array(),
            ambient: AMBIENT,
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        let _ = time;

        // wgpu 30 reports swapchain trouble as enum variants rather than an
        // `Err`, because none of these are failures the caller can act on.
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                log::debug!("surface suboptimal; reconfiguring after this frame");
                self.needs_reconfigure = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                log::debug!("surface outdated or lost; reconfiguring");
                self.surface.configure(&self.device, &self.config);
                self.depth_view = create_depth_view(&self.device, &self.config);
                return FrameStatus::Skipped;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return FrameStatus::Skipped;
            }
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
                label: Some("terrain encoder"),
            });

        let mut drawn = 0usize;
        let mut triangles = 0usize;

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        // Clear to the far plane. wgpu depth is 0..1 with 0
                        // nearest, so "furthest away" is 1.0.
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);

            for chunk in &self.chunks {
                if !self.camera.is_box_visible(chunk.min, chunk.max) {
                    continue;
                }
                pass.set_vertex_buffer(0, chunk.vertex_buffer.slice(..));
                pass.set_index_buffer(chunk.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..chunk.index_count, 0, 0..1);
                drawn += 1;
                triangles += chunk.index_count as usize / 3;
            }
        }

        self.stats = FrameStats {
            chunks_total: self.chunks.len(),
            chunks_drawn: drawn,
            triangles_drawn: triangles,
        };

        self.queue.submit(std::iter::once(encoder.finish()));
        // wgpu 30 moved `present` from `SurfaceTexture` onto `Queue`.
        self.queue.present(frame);

        if std::mem::take(&mut self.needs_reconfigure) {
            self.surface.configure(&self.device, &self.config);
            self.depth_view = create_depth_view(&self.device, &self.config);
        }

        FrameStatus::Presented
    }
}

fn create_depth_view(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth buffer"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

#[cfg(test)]
mod tests {
    /// Parse and validate the shader without needing a GPU.
    ///
    /// `create_shader_module` only compiles WGSL at runtime, which on the web
    /// means a typo costs a full rebuild-and-reload to discover.
    #[test]
    fn terrain_wgsl_is_valid() {
        let source = include_str!("terrain.wgsl");

        let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|err| {
            panic!(
                "terrain.wgsl failed to parse:\n{}",
                err.emit_to_string(source)
            )
        });

        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|err| panic!("terrain.wgsl failed validation: {err:?}"));
    }

    /// The uniform struct is shared with the shader by memory layout alone, so
    /// a size change on either side silently corrupts every field. WGSL
    /// requires uniform structs to be a multiple of 16 bytes.
    #[test]
    fn camera_uniform_matches_shader_layout() {
        assert_eq!(std::mem::size_of::<super::CameraUniform>(), 80);
        assert_eq!(std::mem::size_of::<super::CameraUniform>() % 16, 0);
    }
}

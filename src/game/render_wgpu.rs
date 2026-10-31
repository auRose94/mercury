//! The wgpu render backend: an offscreen renderer — no window, no surface,
//! no display needed. Frames render to a texture, get read back into RAM,
//! and can be saved as PNGs (gold's screenshot debug aid). Tests assert
//! actual pixels; a window-presenting backend is a later rung.

#[cfg(feature = "winit")]
use std::sync::Arc;

const WGSL: &str = r#"
struct Quad {
    pos: vec2f,
    size: vec2f,
    color: vec4f,
};

struct Scene {
    count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    quads: array<Quad, 64>,
};

@group(0) @binding(0) var<uniform> scene: Scene;

struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec3f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let quad = i / 6u;              // which quad
    let corner = i % 6u;            // which of the 6 triangle corners
    var offsets = array(
        vec2f(-0.5, -0.5), vec2f(0.5, -0.5), vec2f(-0.5, 0.5),
        vec2f(-0.5, 0.5), vec2f(0.5, -0.5), vec2f(0.5, 0.5),
    );
    var out: VsOut;
    out.pos = vec4f(scene.quads[quad].pos + offsets[corner] * scene.quads[quad].size, 0.0, 1.0);
    out.color = scene.quads[quad].color.rgb;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    return vec4f(in.color, 1.0);
}
"#;

/// The per-frame uniform: up to 64 solid quads. CPU layout must match the
/// WGSL: `count` + 3 pad words, then 32-byte quads.
const MAX_QUADS: usize = 64;
const SCENE_BYTES: usize = 16 + MAX_QUADS * 32;

fn scene_bytes(quads: &[crate::game::render::Quad]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SCENE_BYTES);
    out.extend_from_slice(&(quads.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for quad in quads.iter().take(MAX_QUADS) {
        for f in quad.pos.iter().chain(quad.size.iter()).chain(quad.color.iter()) {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    out.resize(SCENE_BYTES, 0);
    out
}

/// An offscreen wgpu renderer: clears to the set color and draws a solid
/// test triangle. `screenshot()` returns the last frame as RGBA8 rows.
pub struct WgpuRenderer {
    // kept alive for the winit presentation path (surface creation)
    #[allow(dead_code)]
    instance: wgpu::Instance,
    #[allow(dead_code)]
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    scene: Vec<crate::game::render::Quad>,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
    size: (u32, u32),
    clear_color: [f64; 4],
    frame_sink: Option<crate::game::render::FrameSink>,
    last_frame: Vec<u8>,
    /// When a window is attached, frames present to its surface instead of
    /// the offscreen target (no readback — the compositor is the consumer).
    #[cfg(feature = "winit")]
    window: Option<(Arc<winit::window::Window>, wgpu::Surface<'static>)>,
    #[cfg(feature = "winit")]
    window_config: Option<wgpu::SurfaceConfiguration>,
}

impl WgpuRenderer {
    /// Create the renderer bound to a winit window: the adapter is chosen
    /// against the window's surface, so frames present to the window.
    /// Still `async` (adapter/device requests); call under `pollster`.
    #[cfg(feature = "winit")]
    pub async fn new_for_window(window: Arc<winit::window::Window>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("window surface: {e}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no wgpu adapter for the window surface: {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let (width, height) = (
            window.inner_size().width.max(1),
            window.inner_size().height.max(1),
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("wgpu device: {e}"))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mercury-quad"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(SCENE_BYTES as u64),
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mercury-quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mercury-scene"),
            size: SCENE_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Srgb,
        };
        surface.configure(&device, &config);
        let target = Self::offscreen_target(&device, 64, 64);
        let readback = Self::offscreen_readback(&device, 64, 64);

        Ok(WgpuRenderer {
            instance,
            adapter,
            device,
            queue,
            pipeline,
            uniforms,
            bind_group,
            target,
            readback,
            size: (64, 64),
            clear_color: [0.1, 0.2, 0.3, 1.0],
            scene: Vec::new(),
            frame_sink: None,
            last_frame: Vec::new(),
            window: Some((window, surface)),
            window_config: Some(config),
        })
    }

    /// Create the offscreen renderer: device + pipeline + target. Tries the
    /// platform's real backends (Vulkan/Metal/DX12/GL).
    ///
    /// # Errors
    /// No adapter, device creation failure, or pipeline creation failure —
    /// e.g. a machine with no GPU stack at all.
    pub async fn new_offscreen(width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| format!("no wgpu adapter: {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("wgpu device: {e}"))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mercury-quad"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(SCENE_BYTES as u64),
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mercury-quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mercury-scene"),
            size: SCENE_BYTES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });

        let target = Self::offscreen_target(&device, width, height);
        let readback = Self::offscreen_readback(&device, width, height);

        Ok(WgpuRenderer {
            instance,
            adapter,
            device,
            queue,
            pipeline,
            uniforms,
            bind_group,
            target,
            readback,
            size: (width, height),
            clear_color: [0.1, 0.2, 0.3, 1.0],
            scene: Vec::new(),
            frame_sink: None,
            last_frame: Vec::new(),
            #[cfg(feature = "winit")]
            window: None,
            #[cfg(feature = "winit")]
            window_config: None,
        })
    }

    pub fn set_clear_color(&mut self, color: [f64; 4]) {
        self.clear_color = color;
    }

    /// The scene: solid quads in clip space (center, half-extents, color),
    /// drawn back-to-front in list order — the v0 "mesh", enough to see
    /// entities move (max 64).
    pub fn set_scene(&mut self, quads: &[crate::game::render::Quad]) {
        self.scene = quads.to_vec();
    }

    /// Convenience: a single-quad scene.
    pub fn set_quad(&mut self, pos: [f32; 2], size: [f32; 2], color: [f32; 4]) {
        self.scene = vec![crate::game::render::Quad { pos, size, color }];
    }

    fn offscreen_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mercury-offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    fn offscreen_readback(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mercury-readback"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
    }

    /// Publish every rendered frame into this sink (a streaming window
    /// backend reads from it).
    pub fn attach_sink(&mut self, sink: crate::game::render::FrameSink) {
        self.frame_sink = Some(sink);
    }

    /// Present frames to a winit window's surface instead of the offscreen
    /// target. The adapter must be the same one the surface's display
    /// belongs to — build the renderer with
    /// [`new_for_window`](Self::new_for_window) to guarantee that.
    #[cfg(feature = "winit")]
    pub fn attach_window(&mut self, window: Arc<winit::window::Window>) -> Result<(), String> {
        let surface = self
            .instance
            .create_surface(window.clone())
            .map_err(|e| format!("window surface: {e}"))?;
        let caps = surface.get_capabilities(&self.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: window.inner_size().width.max(1),
            height: window.inner_size().height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Srgb,
        };
        surface.configure(&self.device, &config);
        self.window_config = Some(config);
        self.window = Some((window, surface));
        Ok(())
    }

    /// The last rendered frame: RGBA8 rows, de-padded, top to bottom.
    /// Clear + the scene quad, into any RGBA view (offscreen or surface).
    fn draw_pass(&mut self, view: &wgpu::TextureView) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.queue
            .write_buffer(&self.uniforms, 0, &scene_bytes(&self.scene));
        let clear = wgpu::Color {
            r: self.clear_color[0],
            g: self.clear_color[1],
            b: self.clear_color[2],
            a: self.clear_color[3],
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mercury-frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            let quads = self.scene.len().max(1) as u32;
            pass.draw(0..6 * quads, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }

    pub fn screenshot(&self) -> &[u8] { &self.last_frame }

    pub fn size(&self) -> (u32, u32) { self.size }
}

impl super::render::RenderBackend for WgpuRenderer {
    fn name(&self) -> &str { "wgpu-offscreen" }

    fn pre_frame(&mut self) {}

    fn render_frame(&mut self) {
        // window mode: present straight to the surface (no readback)
        #[cfg(feature = "winit")]
        if let Some((window, surface)) = &mut self.window {
            let config = self.window_config.as_mut().unwrap();
            let new_size = window.inner_size();
            if new_size.width.max(1) != config.width || new_size.height.max(1) != config.height {
                config.width = new_size.width.max(1);
                config.height = new_size.height.max(1);
                surface.configure(&self.device, config);
            }
            let frame = match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame)
                | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                _ => return, // skip the frame: timeout/occluded/outdated/lost
            };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.draw_pass(&view);
            self.queue.present(frame);
            return;
        }

        let (width, height) = self.size;
        let view = self.target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.draw_pass(&view);
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        // read the frame back
        let total = (bytes_per_row * height) as u64;
        self.readback.map_async(wgpu::MapMode::Read, ..total, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let Ok(data) = self.readback.get_mapped_range(..total) else { return };
        let mut frame = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = (row * bytes_per_row) as usize;
            frame.extend_from_slice(&data[start..start + (width * 4) as usize]);
        }
        drop(data);
        self.readback.unmap();
        if let Some(sink) = &self.frame_sink {
            sink.publish(width, height, frame.clone());
        }
        self.last_frame = frame;
    }
}

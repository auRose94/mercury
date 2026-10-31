//! The wgpu render backend: an offscreen renderer — no window, no surface,
//! no display needed. Frames render to a texture, get read back into RAM,
//! and can be saved as PNGs (gold's screenshot debug aid). Tests assert
//! actual pixels; a window-presenting backend is a later rung.

const WGSL: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4f,
    @location(0) color: vec3f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    var p = array(vec2f(-0.5, -0.5), vec2f(0.5, -0.5), vec2f(0.0, 0.5));
    var out: VsOut;
    out.pos = vec4f(p[i], 0.0, 1.0);
    out.color = vec3f(1.0, 0.4, 0.1);
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4f {
    return vec4f(in.color, 1.0);
}
"#;

/// An offscreen wgpu renderer: clears to the set color and draws a solid
/// test triangle. `screenshot()` returns the last frame as RGBA8 rows.
pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    target: wgpu::Texture,
    readback: wgpu::Buffer,
    size: (u32, u32),
    clear_color: [f64; 4],
    last_frame: Vec<u8>,
}

impl WgpuRenderer {
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
            label: Some("mercury-test-triangle"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mercury-test-triangle"),
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

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mercury-offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // readback rows are 256-byte aligned
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mercury-readback"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Ok(WgpuRenderer {
            device,
            queue,
            pipeline,
            target,
            readback,
            size: (width, height),
            clear_color: [0.1, 0.2, 0.3, 1.0],
            last_frame: Vec::new(),
        })
    }

    pub fn set_clear_color(&mut self, color: [f64; 4]) {
        self.clear_color = color;
    }

    /// The last rendered frame: RGBA8 rows, de-padded, top to bottom.
    pub fn screenshot(&self) -> &[u8] { &self.last_frame }

    pub fn size(&self) -> (u32, u32) { self.size }
}

impl super::render::RenderBackend for WgpuRenderer {
    fn name(&self) -> &str { "wgpu-offscreen" }

    fn pre_frame(&mut self) {}

    fn render_frame(&mut self) {
        let (width, height) = self.size;
        let view = self.target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let clear = wgpu::Color {
                r: self.clear_color[0],
                g: self.clear_color[1],
                b: self.clear_color[2],
                a: self.clear_color[3],
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mercury-frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            pass.draw(0..3, 0..1);
        }
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
        self.last_frame = frame;
    }
}

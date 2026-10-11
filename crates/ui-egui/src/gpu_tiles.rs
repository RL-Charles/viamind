//! Native persistent tile buffers. Immutable CPU meshes are uploaded once; camera uniforms
//! change each frame. Tile-owned handles release buffers when the cache replaces their geometry.
use eframe::{
    egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor},
    wgpu,
};
use egui::{ColorImage, Mesh, PaintCallbackInfo, Rect, Shape};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

/// Shared pipeline and geometry atlas for one tessellation key and native target.
pub(crate) struct Pipeline {
    format: wgpu::TextureFormat,
    atlas: Arc<ColorImage>,
    ready: Mutex<Option<ReadyPipeline>>,
}
struct ReadyPipeline {
    pipeline: wgpu::RenderPipeline,
    uniform_layout: wgpu::BindGroupLayout,
    atlas: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    camera: wgpu::BindGroup,
    capacity: usize,
    stride: usize,
}
pub(crate) struct Tile {
    pub(crate) mesh: Arc<Mesh>,
    ready: Mutex<Option<ReadyTile>>,
}
struct ReadyTile {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}
impl Pipeline {
    pub fn new(format: wgpu::TextureFormat, atlas: ColorImage) -> Arc<Self> {
        Arc::new(Self { format, atlas: Arc::new(atlas), ready: Mutex::new(None) })
    }
}
impl Tile {
    pub fn new(mesh: Arc<Mesh>) -> Arc<Self> {
        Arc::new(Self { mesh, ready: Mutex::new(None) })
    }
}
/// One ordered draw list avoids a callback and pipeline switch for every tile.
pub(crate) struct Draw {
    pub tile: Arc<Tile>,
    pub transform: [f32; 3],
}
struct Frame {
    pipeline: Arc<Pipeline>,
    draws: Vec<Draw>,
    viewport: Rect,
}
pub(crate) fn shape(pipeline: Arc<Pipeline>, draws: Vec<Draw>, viewport: Rect) -> Shape {
    Callback::new_paint_callback(viewport, Frame { pipeline, draws, viewport }).into()
}
impl CallbackTrait for Frame {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &ScreenDescriptor,
        _: &mut wgpu::CommandEncoder,
        _: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let mut pipeline = self.pipeline.ready.lock().unwrap();
        let pipeline = pipeline.get_or_insert_with(|| build_pipeline(device, queue, &self.pipeline));
        if pipeline.capacity < self.draws.len() {
            let capacity = self.draws.len().next_power_of_two();
            let (uniform, camera) = camera_buffer(device, &pipeline.uniform_layout, capacity, pipeline.stride);
            pipeline.uniform = uniform;
            pipeline.camera = camera;
            pipeline.capacity = capacity;
        }
        let mut uniforms = vec![0u8; self.draws.len() * pipeline.stride];
        for (index, draw) in self.draws.iter().enumerate() {
            let mut ready = draw.tile.ready.lock().unwrap();
            ready.get_or_insert_with(|| build_tile(device, &draw.tile.mesh));
            let [s, x, y] = draw.transform;
            let camera = [s, x, y, 0.0, self.viewport.width(), self.viewport.height(), self.viewport.min.x, self.viewport.min.y];
            for (i, value) in camera.into_iter().enumerate() {
                uniforms[index * pipeline.stride + i * 4..index * pipeline.stride + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        if !uniforms.is_empty() {
            queue.write_buffer(&pipeline.uniform, 0, &uniforms);
        }
        Vec::new()
    }
    fn paint(&self, _: PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, _: &CallbackResources) {
        let pipeline = self.pipeline.ready.lock().unwrap();
        let Some(pipeline) = pipeline.as_ref() else { return };
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(1, &pipeline.atlas, &[]);
        for (index, draw) in self.draws.iter().enumerate() {
            let ready = draw.tile.ready.lock().unwrap();
            let Some(ready) = ready.as_ref() else { continue };
            pass.set_bind_group(0, &pipeline.camera, &[(index * pipeline.stride) as u32]);
            pass.set_vertex_buffer(0, ready.vertices.slice(..));
            pass.set_index_buffer(ready.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..draw.tile.mesh.indices.len() as u32, 0, 0..1);
        }
    }
}
fn build_tile(device: &wgpu::Device, mesh: &Mesh) -> ReadyTile {
    let mut vertices = Vec::with_capacity(mesh.vertices.len() * 20);
    for v in &mesh.vertices {
        for n in [v.pos.x, v.pos.y, v.uv.x, v.uv.y] {
            vertices.extend_from_slice(&n.to_le_bytes());
        }
        vertices.extend_from_slice(&v.color.to_array());
    }
    let indices: Vec<_> = mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("board tile vertices"),
        contents: &vertices,
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("board tile indices"),
        contents: &indices,
        usage: wgpu::BufferUsages::INDEX,
    });
    ReadyTile { vertices, indices }
}
fn camera_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize, stride: usize) -> (wgpu::Buffer, wgpu::BindGroup) {
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("board cameras"),
        size: (capacity * stride) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("board cameras"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &uniform, offset: 0, size: std::num::NonZeroU64::new(32) }),
        }],
    });
    (uniform, camera)
}

fn build_pipeline(device: &wgpu::Device, queue: &wgpu::Queue, config: &Pipeline) -> ReadyPipeline {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("board camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
            count: None,
        }],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("board atlas layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let size = wgpu::Extent3d { width: config.atlas.size[0] as u32, height: config.atlas.size[1] as u32, depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("board geometric atlas"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: Vec<_> = config.atlas.pixels.iter().flat_map(egui::Color32::to_array).collect();
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(size.width * 4), rows_per_image: None },
        size,
    );
    let view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("board atlas sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let atlas = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("board atlas"),
        layout: &texture_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });
    let source = format!("const LINEAR_TARGET: bool = {};\n{}", config.format.is_srgb(), include_str!("gpu_tiles.wgsl"));
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("board persistent tiles"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("board tile pipeline"),
        bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("board tile pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Unorm8x4],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragment"),
            targets: &[Some(wgpu::ColorTargetState {
                format: config.format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let stride = (device.limits().min_uniform_buffer_offset_alignment as usize).max(32);
    let (uniform, camera) = camera_buffer(device, &uniform_layout, 1, stride);
    ReadyPipeline { pipeline, uniform_layout, atlas, uniform, camera, capacity: 1, stride }
}

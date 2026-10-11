//! Native persistent layer buffers. Tile geometry is immutable; packed camera transforms change
//! each frame. Layer-owned handles release buffers when the cache replaces their geometry.
use eframe::{
    egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor},
    wgpu,
};
use egui::{ColorImage, Mesh, PaintCallbackInfo, Rect, Shape};
use std::{
    collections::BTreeMap,
    ops::Range,
    sync::{Arc, Mutex},
};
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
    viewport: wgpu::Buffer,
    transforms: wgpu::Buffer,
    camera: wgpu::BindGroup,
    capacity: usize,
}
pub(crate) struct Tile {
    pub(crate) mesh: Arc<Mesh>,
}
/// One immutable generation of every resident tile in a layer.
pub(crate) struct Layer {
    tiles: Vec<(u32, Arc<Tile>)>,
    ranges: BTreeMap<u32, Range<u32>>,
    ready: Mutex<Option<ReadyLayer>>,
}
struct ReadyLayer {
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
        Arc::new(Self { mesh })
    }
}
impl Layer {
    pub fn new(tiles: Vec<(u32, Arc<Tile>)>) -> Arc<Self> {
        let mut end = 0;
        let ranges = tiles
            .iter()
            .map(|(slot, tile)| {
                let start = end;
                end += tile.mesh.indices.len() as u32;
                (*slot, start..end)
            })
            .collect();
        Arc::new(Self { tiles, ranges, ready: Mutex::new(None) })
    }

    /// Preserve visible tile order and gaps: hidden indices are never included.
    pub fn visible_ranges(&self, slots: &[u32]) -> Vec<Range<u32>> {
        let mut result: Vec<Range<u32>> = Vec::new();
        for slot in slots {
            let Some(range) = self.ranges.get(slot).filter(|r| !r.is_empty()) else { continue };
            if let Some(last) = result.last_mut().filter(|last| last.end == range.start) {
                last.end = range.end;
            } else {
                result.push(range.clone());
            }
        }
        result
    }
}
/// One ordered draw list avoids a callback and pipeline switch for every layer.
pub(crate) struct Draw {
    pub layer: Arc<Layer>,
    pub ranges: Vec<Range<u32>>,
}
struct Frame {
    pipeline: Arc<Pipeline>,
    draws: Vec<Draw>,
    transforms: Vec<[f32; 4]>,
    viewport: Rect,
    probe: Option<Arc<crate::gpu_timing::GpuTiming>>,
    timing: Mutex<Option<crate::gpu_timing::DrawTiming>>,
    tag: crate::gpu_timing::FrameTag,
}
pub(crate) fn shape(
    pipeline: Arc<Pipeline>,
    draws: Vec<Draw>,
    transforms: Vec<[f32; 4]>,
    viewport: Rect,
    probe: Option<Arc<crate::gpu_timing::GpuTiming>>,
) -> Shape {
    let tag = probe.as_ref().map(|p| p.frame_tag()).unwrap_or_default();
    Callback::new_paint_callback(viewport, Frame { pipeline, draws, transforms, viewport, probe, timing: Mutex::new(None), tag }).into()
}
impl CallbackTrait for Frame {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        _: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(probe) = &self.probe {
            *self.timing.lock().unwrap() = probe.begin(encoder, self.tag);
        }
        let mut pipeline = self.pipeline.ready.lock().unwrap();
        let pipeline = pipeline.get_or_insert_with(|| build_pipeline(device, queue, &self.pipeline));
        if pipeline.capacity < self.transforms.len() {
            let capacity = self.transforms.len().next_power_of_two();
            let (viewport, transforms, camera) = camera_buffer(device, &pipeline.uniform_layout, capacity);
            pipeline.viewport = viewport;
            pipeline.transforms = transforms;
            pipeline.camera = camera;
            pipeline.capacity = capacity;
        }
        for draw in &self.draws {
            let mut ready = draw.layer.ready.lock().unwrap();
            ready.get_or_insert_with(|| build_layer(device, &draw.layer));
        }
        let transforms: Vec<_> = self.transforms.iter().flatten().flat_map(|value| value.to_le_bytes()).collect();
        if !transforms.is_empty() {
            queue.write_buffer(&pipeline.transforms, 0, &transforms);
        }
        let viewport: Vec<_> = [self.viewport.width(), self.viewport.height(), self.viewport.min.x, self.viewport.min.y]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        queue.write_buffer(&pipeline.viewport, 0, &viewport);
        Vec::new()
    }
    fn paint(&self, _: PaintCallbackInfo, pass: &mut wgpu::RenderPass<'static>, _: &CallbackResources) {
        let pipeline = self.pipeline.ready.lock().unwrap();
        let Some(pipeline) = pipeline.as_ref() else { return };
        let timing = self.timing.lock().unwrap();
        let started = timing.as_ref().is_some_and(|t| t.start(pass));
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(1, &pipeline.atlas, &[]);
        pass.set_bind_group(0, &pipeline.camera, &[]);
        for draw in &self.draws {
            let ready = draw.layer.ready.lock().unwrap();
            let Some(ready) = ready.as_ref() else { continue };
            pass.set_vertex_buffer(0, ready.vertices.slice(..));
            pass.set_index_buffer(ready.indices.slice(..), wgpu::IndexFormat::Uint32);
            for range in &draw.ranges {
                pass.draw_indexed(range.clone(), 0, 0..1);
            }
        }
        if started {
            timing.as_ref().unwrap().end(pass);
        }
    }
}
/// Serialize the exact egui attributes plus the tile's transform slot, rebasing indices.
fn pack_layer(layer: &Layer) -> (Vec<u8>, Vec<u8>) {
    let vertex_count: usize = layer.tiles.iter().map(|(_, t)| t.mesh.vertices.len()).sum();
    let index_count: usize = layer.tiles.iter().map(|(_, t)| t.mesh.indices.len()).sum();
    let mut vertices = Vec::with_capacity(vertex_count * 24);
    let mut indices = Vec::with_capacity(index_count * 4);
    let mut base = 0;
    for (slot, tile) in &layer.tiles {
        for v in &tile.mesh.vertices {
            for n in [v.pos.x, v.pos.y, v.uv.x, v.uv.y] {
                vertices.extend_from_slice(&n.to_le_bytes());
            }
            vertices.extend_from_slice(&v.color.to_array());
            vertices.extend_from_slice(&slot.to_le_bytes());
        }
        for index in &tile.mesh.indices {
            indices.extend_from_slice(&(index + base).to_le_bytes());
        }
        base += tile.mesh.vertices.len() as u32;
    }
    (vertices, indices)
}
fn build_layer(device: &wgpu::Device, layer: &Layer) -> ReadyLayer {
    let (vertices, indices) = pack_layer(layer);
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("board layer vertices"),
        contents: &vertices,
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("board layer indices"),
        contents: &indices,
        usage: wgpu::BufferUsages::INDEX,
    });
    ReadyLayer { vertices, indices }
}
fn camera_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: usize) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup) {
    let viewport = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("board viewport"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let transforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("board tile transforms"),
        size: (capacity * 16) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let camera = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("board cameras"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: viewport.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: transforms.as_entire_binding() },
        ],
    });
    (viewport, transforms, camera)
}

fn build_pipeline(device: &wgpu::Device, queue: &wgpu::Queue, config: &Pipeline) -> ReadyPipeline {
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("board camera layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
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
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Unorm8x4,3=>Uint32],
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
    let (viewport, transforms, camera) = camera_buffer(device, &uniform_layout, 1);
    ReadyPipeline { pipeline, uniform_layout, atlas, viewport, transforms, camera, capacity: 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(x: f32) -> Arc<Tile> {
        let vertices = (0..3)
            .map(|i| egui::epaint::Vertex {
                pos: egui::pos2(x + i as f32, 9.5),
                uv: egui::pos2(0.25, 0.75),
                color: egui::Color32::from_rgba_premultiplied(12, 34, 56, 78),
            })
            .collect();
        Tile::new(Arc::new(Mesh { vertices, indices: vec![2, 0, 1], ..Default::default() }))
    }

    #[test]
    fn packed_layers_keep_every_attribute_slot_and_triangle_order() {
        let a = triangle(-12.25);
        let b = triangle(3.0);
        let layer = Layer::new(vec![(7, Arc::clone(&a)), (19, Arc::clone(&b))]);
        let (vertices, indices) = pack_layer(&layer);
        assert_eq!(vertices.len(), 6 * 24);
        for (bytes, (vertex, slot)) in
            vertices.as_chunks::<24>().0.iter().zip(a.mesh.vertices.iter().map(|v| (v, 7u32)).chain(b.mesh.vertices.iter().map(|v| (v, 19u32))))
        {
            let floats: Vec<_> = bytes[..16].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
            assert_eq!(floats, [vertex.pos.x, vertex.pos.y, vertex.uv.x, vertex.uv.y]);
            assert_eq!(&bytes[16..20], &vertex.color.to_array());
            assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), slot);
        }
        let indices: Vec<_> = indices.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect();
        assert_eq!(indices, [2, 0, 1, 5, 3, 4]);
    }

    #[test]
    fn visible_ranges_merge_adjacent_tiles_without_bridging_hidden_indices() {
        let tile = triangle(0.0);
        let layer = Layer::new((0..5).map(|i| (i, Arc::clone(&tile))).collect());
        assert_eq!(layer.visible_ranges(&[0, 1, 2, 3, 4]), vec![0..15]);
        assert_eq!(layer.visible_ranges(&[0, 2, 3]), vec![0..3, 6..12]);
        assert_eq!(layer.visible_ranges(&[4, 0]), vec![12..15, 0..3], "caller order is retained");
        assert!(layer.visible_ranges(&[]).is_empty());
        assert!(layer.visible_ranges(&[99]).is_empty());
    }
}

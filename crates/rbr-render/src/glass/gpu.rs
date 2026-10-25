//! The glass pass (`glass.wgsl`): the windscreen's drops as instanced quads, drawn after the frame over the kept
//! multisampled colour and depth (depth tested, no depth write), reading the resolved frame for the drops' lenses. When
//! the renderer has no resolved frame of its own (the screen-space reflections' one), the pass resolves the frame
//! into [`Pass::scene`] first.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::atmosphere::Atmosphere;

/// One drop, flake or bug on the glass: its centre and half axes in stage space, its kind (0 water, 1 snow, 2 bug,
/// 3 mud, 4 a cell of the wet film), a seed for its ragged edge (the film's streak length), its opacity and its colour
/// (snow, bugs and mud; the film's corners).
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Drop {
    pub centre: [f32; 3],
    pub kind: f32,
    pub axis_u: [f32; 3],
    pub seed: f32,
    pub axis_v: [f32; 3],
    pub alpha: f32,
    pub color: [f32; 4],
}

pub(crate) const SHADER: &str = include_str!("glass.wgsl");
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 7] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 44, shader_location: 5 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
];

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct FrameUniform {
    view_projection: [[f32; 4]; 4],
    screen: [f32; 4],
    light: [f32; 4],
    /// At night (renderer addition, `Atmosphere::night`): rgb the headlights' glow on the glass, w 1.
    night: [f32; 4],
}

/// Ours: the glass takes the stage's ambient light and this much of its diffuse.
const DIFFUSE_SHARE: f32 = 0.5;
/// At night: the light the headlights' beams, scattered back off the rain and the road, throw on the glass.
const NIGHT_GLOW: [f32; 3] = [0.16, 0.155, 0.14];
const FIRST_CAPACITY: usize = 1024;

/// The glass pipeline, the frame's drops and the resolved frame it reads when the renderer has none.
pub struct Pass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
    scene: Option<((u32, u32), wgpu::TextureView)>,
}

impl Pass {
    /// The pass for colour targets of `format` with `samples` samples and a `depth` attachment.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, samples: u32, depth: wgpu::TextureFormat) -> Pass {
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass"),
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::VERTEX_FRAGMENT,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                entry(
                    1,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(2, wgpu::ShaderStages::FRAGMENT, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glass scene"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glass frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glass"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glass"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("glass"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Drop>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &INSTANCE_ATTRIBUTES,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: crate::post::scene_options(format),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
            multiview_mask: None,
            cache: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glass drops"),
            size: (FIRST_CAPACITY * std::mem::size_of::<Drop>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Pass { pipeline, layout, sampler, uniform, instances, capacity: FIRST_CAPACITY, count: 0, scene: None }
    }

    /// The drops to draw.
    pub fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, drops: &[Drop]) {
        if drops.len() > self.capacity {
            self.capacity = drops.len().next_power_of_two();
            let mut all = drops.to_vec();
            all.resize(self.capacity, Drop::zeroed());
            self.instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glass drops"),
                contents: bytemuck::cast_slice(&all),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        } else if !drops.is_empty() {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(drops));
        }
        self.count = drops.len() as u32;
    }

    /// Whether there is anything to draw.
    pub fn active(&self) -> bool {
        self.count > 0
    }

    /// The frame's camera, size and light.
    pub fn prepare(&self, queue: &wgpu::Queue, view_projection: Mat4, (width, height): (u32, u32), a: &Atmosphere) {
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let light = (a.ambient + a.diffuse * DIFFUSE_SHARE).min(glam::Vec3::splat(1.2));
        let uniform = FrameUniform {
            view_projection: view_projection.to_cols_array_2d(),
            screen: [w, h, 1.0 / w, 1.0 / h],
            light: light.extend(1.0).to_array(),
            night: if a.night { [NIGHT_GLOW[0], NIGHT_GLOW[1], NIGHT_GLOW[2], 1.0] } else { [0.0; 4] },
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    /// The pass's own resolved frame of `size` (made or remade as needed), for a renderer without one.
    pub fn scene(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> &wgpu::TextureView {
        if self.scene.as_ref().is_none_or(|(s, _)| *s != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("glass resolved frame"),
                size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                // Copied from when the frame has one sample (`Renderer::over_attachment`).
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.scene = Some((size, texture.create_view(&Default::default())));
        }
        &self.scene.as_ref().expect("made above").1
    }

    /// The pass's own resolved frame, after [`Pass::scene`].
    pub fn own_scene(&self) -> Option<&wgpu::TextureView> {
        self.scene.as_ref().map(|(_, view)| view)
    }

    /// Draws the drops into `pass`, reading the resolved frame `scene`.
    pub fn draw(&self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'_>, scene: &wgpu::TextureView) {
        if self.count == 0 {
            return;
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glass"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(scene) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..self.count);
    }
}

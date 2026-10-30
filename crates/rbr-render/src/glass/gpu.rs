//! The glass pass (`glass.wgsl`): the windscreen's drops as instanced quads, drawn after the frame over the kept
//! colour and depth (depth tested, no depth write), reading the frame for the drops' lenses. When the renderer has no
//! frame of its own to read (the screen-space reflections' one), the pass copies the frame into [`Pass::scene`] first.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::atmosphere::Atmosphere;

/// One drop, flake or bug on the glass: its centre and half axes in stage space, its kind (0 water, 1 snow, 2 bug,
/// 3 mud, 4 a cell of the wet film, 5 mud seen from inside, 6 a cell of the pane itself seen from inside), a seed for its ragged edge (the film's streak length),
/// its opacity and its colour (snow, bugs and mud; the film's corners), and the pane's outward normal (kind 6). A pane
/// cell's `centre` is its low corner, `axis_u`, `axis_v` and `far` the other corners' offsets from it, and `tint` what
/// each corner's glass lets through (low, right, top, far).
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
    pub normal: [f32; 4],
    pub far: [f32; 3],
    pub tint: [u8; 4],
}

pub(crate) const SHADER: &str = include_str!("glass.wgsl");
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 10] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 44, shader_location: 5 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 7 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 80, shader_location: 8 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 92, shader_location: 9 },
];

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct FrameUniform {
    view_projection: [[f32; 4]; 4],
    screen: [f32; 4],
    light: [f32; 4],
    /// At night (renderer addition, `Atmosphere::night`): rgb the headlights' glow on the glass, w 1.
    night: [f32; 4],
    /// Seen from outside (ours): x 1, the wet layer of the film (`glass.wgsl`); 0 inside.
    outside: [f32; 4],
    /// The eye, stage space.
    eye: [f32; 4],
    /// The way to the sun, stage space, w 1 by day; and its light on the glass, rgb.
    sun: [f32; 4],
    sun_light: [f32; 4],
    /// The windscreen's wipers for the pane (kind 6): [`Pass::set_wipers`].
    wipers: [[f32; 4]; 4],
    /// The sun's first cascade (`sun::Cascades`), and x 1 when there are cascades.
    sun_matrix: [[f32; 4]; 4],
    shadow: [f32; 4],
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
    /// The sun's cascades (`sun::SunShadow::view`, or a placeholder) and their comparison sampler.
    sun_map: wgpu::TextureView,
    sun_sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
    scene: Option<((u32, u32), wgpu::TextureView)>,
    wipers: [[f32; 4]; 4],
}

impl Pass {
    /// The pass for colour targets of `format` and a `depth` attachment, reading the sun's cascades `sun_map`.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        sun_map: &wgpu::TextureView,
    ) -> Pass {
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
                entry(
                    3,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
                entry(
                    4,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                ),
                entry(
                    5,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                ),
                entry(
                    6,
                    wgpu::ShaderStages::FRAGMENT,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glass scene"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sun_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glass sun"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
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
                compilation_options: Default::default(),
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
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glass drops"),
            size: (FIRST_CAPACITY * std::mem::size_of::<Drop>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Pass {
            pipeline,
            layout,
            sampler,
            sun_map: sun_map.clone(),
            sun_sampler,
            uniform,
            instances,
            capacity: FIRST_CAPACITY,
            count: 0,
            scene: None,
            wipers: [[0.0; 4]; 4],
        }
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

    /// The windscreen's wipers for the pane seen from inside (`glass.wgsl` kind 6), at most two, each two rows: its
    /// pivot in the chart (metres) and its reach, then its park angle, its signed sweep and 1 (0: no wiper).
    pub fn set_wipers(&mut self, wipers: [[f32; 4]; 4]) {
        self.wipers = wipers;
    }

    /// Whether there is anything to draw.
    pub fn active(&self) -> bool {
        self.count > 0
    }

    /// The frame's camera, size and light.
    pub(crate) fn prepare(
        &self,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        (width, height): (u32, u32),
        a: &Atmosphere,
        outside: bool,
        cascades: Option<&crate::sun::Cascades>,
    ) {
        let (w, h) = (width.max(1) as f32, height.max(1) as f32);
        let light = (a.ambient + a.diffuse * DIFFUSE_SHARE).min(glam::Vec3::splat(1.2));
        let uniform = FrameUniform {
            view_projection: view_projection.to_cols_array_2d(),
            screen: [w, h, 1.0 / w, 1.0 / h],
            light: light.extend(1.0).to_array(),
            night: if a.night { [NIGHT_GLOW[0], NIGHT_GLOW[1], NIGHT_GLOW[2], 1.0] } else { [0.0; 4] },
            outside: [f32::from(u8::from(outside)), 0.0, 0.0, 0.0],
            eye: crate::camera::eye(view_projection).extend(1.0).to_array(),
            sun: (-a.sun_direction.normalize_or_zero()).extend(if a.night { 0.0 } else { 1.0 }).to_array(),
            sun_light: a.diffuse.extend(1.0).to_array(),
            wipers: self.wipers,
            sun_matrix: cascades.map_or(Mat4::IDENTITY, |c| c.matrices[0]).to_cols_array_2d(),
            shadow: [f32::from(u8::from(cascades.is_some())), 0.0, 0.0, 0.0],
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

    /// Draws the drops into `pass`, reading the resolved frame `scene` and the mirrors' images and views `mirrors`
    /// (the windscreen's reflection, `renderer::mirrors`).
    pub fn draw(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &wgpu::TextureView,
        (images, views): (&wgpu::TextureView, &wgpu::Buffer),
    ) {
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
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&self.sun_map) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.sun_sampler) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(images) },
                wgpu::BindGroupEntry { binding: 6, resource: views.as_entire_binding() },
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..self.count);
    }
}

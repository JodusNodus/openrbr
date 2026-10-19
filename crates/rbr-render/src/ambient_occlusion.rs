//! Ground-truth ambient occlusion over the resolved frame (`ao.wgsl`): a renderer addition behind
//! [`crate::renderer::Enhance`], not the game's look. It reads the frame's depth (`depth_copy.rs`).

use crate::renderer::texture_entry;
use bytemuck::{bytes_of, Pod, Zeroable};
use glam::{DMat4, Mat4, Vec3, Vec4Swizzles};

pub(crate) const SHADER: &str = include_str!("ao.wgsl");
/// The occlusion map's and the distance map's formats.
const MAP_FORMATS: [wgpu::TextureFormat; 2] = [wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::R32Float];
/// Sample radius, metres.
const RADIUS: f32 = 1.2;
/// How dark a fully covered pixel gets (1: black).
const STRENGTH: f32 = 0.9;
/// Distances over which the occlusion fades out, metres; beyond, the fog and the baked light carry the shading.
const FADE: [f32; 2] = [60.0, 150.0];

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Uniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    params: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    forward: [f32; 4],
}

/// How far the slices turn each frame (of a half turn), so the temporal anti-aliasing's history gathers new ones: the
/// golden ratio's steps spread them evenly.
const TURN: f32 = 0.618_034;

pub(crate) struct AmbientOcclusion {
    uniform: wgpu::Buffer,
    depth_layout: wgpu::BindGroupLayout,
    map_layout: wgpu::BindGroupLayout,
    occlusion: wgpu::RenderPipeline,
    apply: wgpu::RenderPipeline,
    /// The occlusion and each pixel's distance from the eye.
    maps: [wgpu::TextureView; 2],
    /// The frame's size, pixels.
    size: (u32, u32),
    depth_group: wgpu::BindGroup,
    map_group: wgpu::BindGroup,
}

impl AmbientOcclusion {
    /// The passes for colour targets of `format`, reading `depth` (`size` pixels).
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        depth: &wgpu::TextureView,
    ) -> AmbientOcclusion {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ambient occlusion"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ambient occlusion depth"),
            entries: &[
                texture_entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
                texture_entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
            ],
        });
        let map_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ambient occlusion map"),
            entries: &[0, 1].map(|binding| {
                texture_entry(
                    binding,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                )
            }),
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ambient occlusion"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = |entry: &str, layouts: &[&wgpu::BindGroupLayout], targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(entry),
                    bind_group_layouts: layouts,
                    push_constant_ranges: &[],
                })),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets,
                }),
                multiview: None,
                cache: None,
            })
        };
        let occlusion = pipeline("fs_occlusion", &[&depth_layout], &MAP_FORMATS.map(|format| Some(format.into())));
        // The frame times the occlusion.
        let multiply = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::Src,
            operation: wgpu::BlendOperation::Add,
        };
        let apply = pipeline(
            "fs_apply",
            &[&depth_layout, &map_layout],
            &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState { color: multiply, alpha: wgpu::BlendComponent::REPLACE }),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        );
        let (maps, depth_group, map_group) = targets(device, &uniform, [&depth_layout, &map_layout], size, depth);
        AmbientOcclusion { uniform, depth_layout, map_layout, occlusion, apply, maps, size, depth_group, map_group }
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, size: (u32, u32), depth: &wgpu::TextureView) {
        self.size = size;
        (self.maps, self.depth_group, self.map_group) =
            targets(device, &self.uniform, [&self.depth_layout, &self.map_layout], size, depth);
    }

    /// Darkens `target` by the occlusion of the frame drawn with `view_projection`, the `frame`th of the temporal
    /// anti-aliasing (0 without it, so a still frame stays the same).
    pub(crate) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        view_projection: Mat4,
        eye: Vec3,
        frame: u32,
    ) {
        // Positions are rebuilt about the eye: in stage coordinates, thousands of metres out, f32 left the cockpit's
        // centimetre radii a millimetre's grain, which showed as noise.
        let about_eye = view_projection.as_dmat4() * DMat4::from_translation(eye.as_dvec3());
        let height = self.size.1 as f32;
        let [right, up, forward] = view_axes(view_projection);
        let uniform = Uniform {
            view_projection: about_eye.as_mat4().to_cols_array_2d(),
            inverse_view_projection: about_eye.inverse().as_mat4().to_cols_array_2d(),
            eye: [0.0, 0.0, 0.0, 1.0],
            params: [RADIUS, STRENGTH, FADE[0], FADE[1]],
            right: right.truncate().extend(0.5 * height * up.w).to_array(),
            up: up.truncate().extend((frame as f32 * TURN).fract()).to_array(),
            forward: forward.to_array(),
        };
        queue.write_buffer(&self.uniform, 0, bytes_of(&uniform));
        let attachment = |view, load| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
            })
        };
        let mut occlusion = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ambient occlusion"),
            color_attachments: &self
                .maps
                .each_ref()
                .map(|map| attachment(map, wgpu::LoadOp::Clear(wgpu::Color::WHITE))),
            ..Default::default()
        });
        occlusion.set_pipeline(&self.occlusion);
        occlusion.set_bind_group(0, &self.depth_group, &[]);
        occlusion.draw(0..3, 0..1);
        drop(occlusion);
        let mut apply = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ambient occlusion apply"),
            color_attachments: &[attachment(target, wgpu::LoadOp::Load)],
            ..Default::default()
        });
        apply.set_pipeline(&self.apply);
        apply.set_bind_group(0, &self.depth_group, &[]);
        apply.set_bind_group(1, &self.map_group, &[]);
        apply.draw(0..3, 0..1);
    }
}

/// The view's right, up and forward axes in stage space, from the rows of `view_projection` (left-handed, the clip w
/// the distance along the view); `w` of the right and up axes their projection's scale (half the clip's size over a
/// unit at a unit's distance). The jitter only adds a little of the forward axis to the right and up rows.
fn view_axes(view_projection: Mat4) -> [glam::Vec4; 3] {
    let forward = view_projection.row(3).xyz().normalize_or_zero();
    let axis = |row: Vec3| {
        let side = row - row.dot(forward) * forward;
        side.normalize_or_zero().extend(side.length())
    };
    [axis(view_projection.row(0).xyz()), axis(view_projection.row(1).xyz()), forward.extend(0.0)]
}

/// The occlusion and distance maps for frames of `size` (half the size) and the bind groups that read `depth` and the
/// maps.
fn targets(
    device: &wgpu::Device,
    uniform: &wgpu::Buffer,
    [depth_layout, map_layout]: [&wgpu::BindGroupLayout; 2],
    (width, height): (u32, u32),
    depth: &wgpu::TextureView,
) -> ([wgpu::TextureView; 2], wgpu::BindGroup, wgpu::BindGroup) {
    let maps = MAP_FORMATS.map(|format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("ambient occlusion"),
                size: wgpu::Extent3d {
                    width: width.div_ceil(2).max(1),
                    height: height.div_ceil(2).max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
    });
    let depth_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ambient occlusion depth"),
        layout: depth_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(depth) },
        ],
    });
    let map_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ambient occlusion map"),
        layout: map_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&maps[0]) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&maps[1]) },
        ],
    });
    (maps, depth_group, map_group)
}

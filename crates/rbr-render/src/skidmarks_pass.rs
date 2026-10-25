//! The skid marks' draw pass: the [`Quad`]s of [`crate::skidmarks`] as alpha-blended triangles on the ground,
//! textured from `Textures/particles.dat`'s `skidmarks.dds`. The pass tests against the stage's depth without writing
//! it, and biases the depth towards the camera so the marks sit on the surface they were laid on (ours: the original's
//! bias is not traced). It is drawn inside the stage pass (`renderer::Renderer::draw_scene`).
//!
//! The car's shadow decal ([`crate::car_shadow`]) draws the same way, with its texture clamped instead of tiled
//! ([`SkidPass::decal`]), and so do our tyre marks ([`crate::tyre_marks`], [`SkidPass::tyres`]), shaded in
//! `tyre_marks.wgsl` with no texture.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use rbr_formats::dds::Dds;

use crate::renderer::{LampUniform, DEPTH_FORMAT, MAX_LAMPS};
use crate::skidmarks::Quad;
use crate::tyre_marks;

/// The texture's cells, by the texture index of the skid record: `skidmarks.dds` is 256 by 64 texels, eight 32-texel
/// strips side by side, each a trail running down the texture (the retail records use indices 0 to 7). An index past
/// the grid wraps.
pub(crate) const SHADER: &str = concat!(include_str!("skidmarks.wgsl"), include_str!("lamps.wgsl"));
/// The shader of [`SkidPass::tyres`].
pub(crate) const TYRE_SHADER: &str =
    concat!(include_str!("skidmarks.wgsl"), include_str!("tyre_marks.wgsl"), include_str!("lamps.wgsl"));
const COLUMNS: u32 = 8;
const ROWS: u32 = 1;

/// One triangle corner: its stage position, the texture coordinates along and across the trail, the texture cell
/// (`x`, `y`, width, height in texture coordinates) and the colour with its alpha. A tyre mark's corner
/// ([`tyre_vertices`]) carries its look in the cell and the colour instead.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    cell: [f32; 4],
    rgba: [f32; 4],
}

const VERTEX_FLOATS: u64 = 13;

/// The marks' view and light (`View` in `skidmarks.wgsl`).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ViewUniform {
    view_projection: [[f32; 4]; 4],
    light: [f32; 4],
    lamps: [LampUniform; MAX_LAMPS],
    eye: [f32; 4],
}

/// The draw state: the pipeline, the view-projection uniform, the texture, and the vertex buffer for the marks.
pub struct SkidPass {
    pipeline: wgpu::RenderPipeline,
    view: wgpu::Buffer,
    frame: wgpu::BindGroup,
    texture: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    /// The vertex buffer's capacity, in vertices.
    capacity: u32,
    count: u32,
}

impl SkidPass {
    /// The pass for `skidmarks.dds`, for colour targets of `format`.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        samples: u32,
        texture: &Dds,
    ) -> SkidPass {
        SkidPass::with_fragment(
            device,
            queue,
            (format, samples),
            texture,
            (SHADER, "fs_skid"),
            wgpu::BlendState::ALPHA_BLENDING,
        )
    }

    /// The pass for the car's shadow decal (`car_dpshad2.tga`), its texture clamped at the edges.
    pub fn decal(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        samples: u32,
        texture: &Dds,
    ) -> SkidPass {
        SkidPass::with_fragment(
            device,
            queue,
            (format, samples),
            texture,
            (SHADER, "fs_decal"),
            wgpu::BlendState::ALPHA_BLENDING,
        )
    }

    /// The pass for our tyre marks ([`crate::tyre_marks`]): each fragment darkens the stage under it by its alpha and
    /// adds its colour (`fs_tyre`), so the marks keep the ground's own colour and light.
    pub fn tyres(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, samples: u32) -> SkidPass {
        let white = Dds { width: 1, height: 1, format: rbr_formats::dds::Format::Rgba8, mip_count: 1, data: &[255; 4] };
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::SrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        SkidPass::with_fragment(device, queue, (format, samples), &white, (TYRE_SHADER, "fs_tyre"), blend)
    }

    fn with_fragment(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        (format, samples): (wgpu::TextureFormat, u32),
        texture: &Dds,
        (source, fragment): (&str, &str),
        blend: wgpu::BlendState,
    ) -> SkidPass {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skid marks"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skid marks view"),
            size: size_of::<ViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skid marks frame"),
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
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skid marks texture"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skid marks"),
            bind_group_layouts: &[Some(&frame_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skid marks"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_skid"),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout())],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 8, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment),
                compilation_options: crate::post::scene_options(format),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let frame = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skid marks frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: view.as_entire_binding() }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("skid marks"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let image = crate::renderer::dds_texture(device, queue, texture).create_view(&Default::default());
        let texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("skid marks texture"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&image) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let capacity = 1024;
        let vertices = vertex_buffer(device, capacity);
        SkidPass { pipeline, view, frame, texture, vertices, capacity, count: 0 }
    }

    /// Replaces the marks drawn: each quad is two triangles, in stage coordinates (the physics z-up axes swapped).
    pub fn set_quads(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, quads: &[Quad]) {
        let vertices: Vec<Vertex> = quads.iter().flat_map(quad_vertices).collect();
        self.set_vertices(device, queue, &vertices);
    }

    /// Replaces the tyre marks drawn ([`SkidPass::tyres`]).
    pub fn set_tyre_quads(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, quads: &[tyre_marks::Quad]) {
        let vertices: Vec<Vertex> = quads.iter().flat_map(tyre_vertices).collect();
        self.set_vertices(device, queue, &vertices);
    }

    /// Replaces the shadow triangles drawn ([`SkidPass::decal`]): three corners each, in stage coordinates.
    pub fn set_corners(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, corners: &[crate::car_shadow::Corner]) {
        let cell = [0.0, 0.0, 1.0, 1.0];
        let vertices: Vec<Vertex> = corners
            .iter()
            .map(|c| Vertex { pos: c.position, uv: c.uv, cell, rgba: [1.0, 1.0, 1.0, c.alpha] })
            .collect();
        self.set_vertices(device, queue, &vertices);
    }

    fn set_vertices(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[Vertex]) {
        let count = vertices.len() as u32;
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.vertices = vertex_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(vertices));
        self.count = count;
    }

    /// Draws the marks with `view_projection` (stage coordinates), lit by `light` and the `lamps`.
    pub(crate) fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        queue: &wgpu::Queue,
        view_projection: Mat4,
        light: Vec3,
        lamps: [LampUniform; MAX_LAMPS],
    ) {
        if self.count == 0 {
            return;
        }
        let view = ViewUniform {
            view_projection: view_projection.to_cols_array_2d(),
            light: light.extend(0.0).to_array(),
            lamps,
            eye: crate::camera::eye(view_projection).extend(1.0).to_array(),
        };
        queue.write_buffer(&self.view, 0, bytemuck::bytes_of(&view));
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame, &[]);
        pass.set_bind_group(1, &self.texture, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

fn vertex_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("skid marks"),
        size: capacity as u64 * VERTEX_FLOATS * 4,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
    ];
    wgpu::VertexBufferLayout {
        array_stride: VERTEX_FLOATS * 4,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

/// The texture cell of a record's texture index: `(x, y, width, height)` in texture coordinates.
fn cell(texture: u32) -> [f32; 4] {
    let index = texture % (COLUMNS * ROWS);
    let (column, row) = (index % COLUMNS, index / COLUMNS);
    let (width, height) = (1.0 / COLUMNS as f32, 1.0 / ROWS as f32);
    [column as f32 * width, row as f32 * height, width, height]
}

/// The stage position of a physics point: y and z swapped, as [`crate::coords::physics_pose_to_stage`] does.
fn stage(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], p[1]]
}

/// The two triangles of one quad. `u` runs from edge `a` (0) to edge `b` (1), `v` along the trail.
fn quad_vertices(q: &Quad) -> [Vertex; 6] {
    let cell = cell(q.texture);
    let corner = |p: [f32; 3], u: f32, v: f32, rgba: [f32; 4]| Vertex { pos: stage(p), uv: [u, v], cell, rgba };
    let a0 = corner(q.a0, 0.0, q.v0, q.rgba0);
    let b0 = corner(q.b0, 1.0, q.v0, q.rgba0);
    let a1 = corner(q.a1, 0.0, q.v1, q.rgba1);
    let b1 = corner(q.b1, 1.0, q.v1, q.rgba1);
    [a0, b0, a1, b0, b1, a1]
}

/// The two triangles of one tyre mark quad: `uv` across the quad in half tread widths ([`tyre_marks::Kind::spread`]
/// at the edges) and along it in metres, the cell the kind, wetness, slide and swept width, the colour the strength,
/// fade and looseness (`fs_tyre` in `tyre_marks.wgsl`).
fn tyre_vertices(q: &tyre_marks::Quad) -> [Vertex; 6] {
    let spread = q.look0.kind.spread();
    let corner = |p: [f32; 3], u: f32, v: f32, l: &tyre_marks::Look| Vertex {
        pos: stage(p),
        uv: [u * spread, v],
        cell: [l.kind.id(), l.wet, l.slide, l.width],
        rgba: [l.strength, l.fade, l.loose, 0.0],
    };
    let a0 = corner(q.a0, -1.0, q.v0, &q.look0);
    let b0 = corner(q.b0, 1.0, q.v0, &q.look0);
    let a1 = corner(q.a1, -1.0, q.v1, &q.look1);
    let b1 = corner(q.b1, 1.0, q.v1, &q.look1);
    [a0, b0, a1, b0, b1, a1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_tile_the_texture_and_wrap() {
        assert_eq!(cell(0), [0.0, 0.0, 0.125, 1.0]);
        assert_eq!(cell(5), [0.625, 0.0, 0.125, 1.0]);
        assert_eq!(cell(COLUMNS * ROWS), cell(0));
    }

    #[test]
    fn a_quad_is_two_triangles_in_stage_axes() {
        let q = Quad {
            a0: [1.0, 2.0, 3.0],
            b0: [1.0, 4.0, 3.0],
            a1: [5.0, 2.0, 3.0],
            b1: [5.0, 4.0, 3.0],
            rgba0: [1.0; 4],
            rgba1: [0.5; 4],
            v0: 0.0,
            v1: 1.0,
            texture: 1,
        };
        let v = quad_vertices(&q);
        assert_eq!(v.len(), 6);
        assert_eq!(v[0].pos, [1.0, 3.0, 2.0], "y and z swapped");
        assert_eq!(v[4].uv, [1.0, 1.0]);
        assert_eq!(v[2].rgba, [0.5; 4]);
    }
}

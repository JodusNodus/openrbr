//! The motion of what moves of itself on the screen (ours), for the temporal anti-aliasing (`taa.wgsl`): after the
//! main view's stage pass the meshes that move are drawn again where the frame's depth is theirs (`motion.wgsl`), each
//! pixel writing its motion since the previous frame and whether it is the car's. The rest moves with the eye, which
//! the resolve finds from the depth.
//!
//! The car's parts kept in its model space ([`Renderer::set_car_vertices`]) were where its previous body placed them.
//! The meshes given in stage space ([`Renderer::set_vertices`]: the wheels, the steering wheel and the wipers, the
//! ghost, the characters, the fences; the props' poses) keep their positions as last drawn in a second vertex buffer,
//! per buffer of [`MeshBuffers`] at their vertices' index, made with the first of its meshes that moves.

use super::frame::View;
use super::meshes::{GpuMesh, MeshBuffers};
use super::{Renderer, DEPTH_FORMAT};
use crate::camera::Frustum;
use crate::coords::FRONT_FACE;
use crate::stage::{Anchor, Vertex};
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use std::collections::HashMap;

/// The motion's target: x and y the uv the point was at less the uv it is at (y down), z 0 where nothing moving was
/// drawn, 1 where something was and 2 where the car's meshes were.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    view_projection: [f32; 16],
    previous_view_projection: [f32; 16],
    previous_body: [f32; 16],
}

/// A mesh given in stage space: its positions as last drawn and as given since (`None`: as drawn), and whether its
/// previous positions are those last drawn.
struct Track {
    shown: Vec<[f32; 3]>,
    given: Option<Vec<[f32; 3]>>,
    settled: bool,
}

pub(super) struct Motion {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    target: wgpu::TextureView,
    /// Per buffer of [`MeshBuffers`], its vertices' previous positions, made for the first mesh in it drawn here.
    previous: Vec<Option<wgpu::Buffer>>,
    /// The meshes given in stage space, by their index in [`Renderer::meshes`].
    tracks: HashMap<usize, Track>,
    /// The view-projection (unjittered) and the car's body the last frame was drawn with.
    last: Option<(Mat4, Mat4)>,
}

impl Motion {
    /// The pass over `buffers` vertex buffers, with the stage's `shader` and the `frame` layout (its uniform alone).
    pub(super) fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        frame: &wgpu::BindGroupLayout,
        buffers: usize,
    ) -> Motion {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("motion"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("motion"),
            size: size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("motion"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 7, resource: uniform.as_entire_binding() }],
        });
        const PREVIOUS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![11 => Float32x3];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("motion"),
            layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("motion"),
                bind_group_layouts: &[Some(frame), Some(&layout)],
                immediate_size: 0,
            })),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_motion"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(super::vertex_layout()),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<[f32; 3]>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &PREVIOUS,
                    }),
                ],
            },
            // Both faces: only those at the frame's depth pass.
            primitive: wgpu::PrimitiveState { front_face: FRONT_FACE, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Equal),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_motion"),
                compilation_options: Default::default(),
                targets: &[Some(FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        Motion {
            pipeline,
            uniform,
            group,
            target: target(device, (1, 1)),
            previous: (0..buffers).map(|_| None).collect(),
            tracks: HashMap::new(),
            last: None,
        }
    }

    /// The motion's target ([`FORMAT`]).
    pub(super) fn view(&self) -> &wgpu::TextureView {
        &self.target
    }

    /// For frames of `size`.
    pub(super) fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) {
        self.target = target(device, size);
    }

    /// Mesh `index` was given `vertices` in stage space.
    pub(super) fn given(&mut self, index: usize, vertices: &[Vertex]) {
        let positions = vertices.iter().map(|v| v.position).collect();
        let track = self.tracks.entry(index).or_insert(Track { shown: Vec::new(), given: None, settled: false });
        track.given = Some(positions);
    }

    /// Mesh `index` is placed by the car's body from now on.
    pub(super) fn placed(&mut self, index: usize) {
        self.tracks.remove(&index);
    }

    /// Whether the pass draws `mesh`, the mesh at `index`.
    fn moves(&self, index: usize, mesh: &GpuMesh) -> bool {
        mesh.anchor == Anchor::World && (mesh.in_car.is_some() || self.tracks.contains_key(&index))
    }

    /// Writes the previous positions of the meshes given since the last frame, and of those given before it, which
    /// now stay where they were; and the uniform for the frame of `view_projection` (unjittered) with the car's `body`.
    fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        (meshes, buffers): (&[GpuMesh], &[MeshBuffers]),
        view_projection: Mat4,
        body: Mat4,
    ) {
        for (index, mesh) in meshes.iter().enumerate() {
            if !self.moves(index, mesh) {
                continue;
            }
            let pack = mesh.slot.buffer();
            let previous = self.previous[pack].get_or_insert_with(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("previous positions"),
                    size: u64::from(buffers[pack].vertex_count) * size_of::<[f32; 3]>() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            });
            let Some(track) = self.tracks.get_mut(&index) else { continue };
            let offset = mesh.slot.base_vertex() as u64 * size_of::<[f32; 3]>() as u64;
            match track.given.take() {
                Some(given) => {
                    let shown = std::mem::replace(&mut track.shown, given);
                    // A mesh given for the first time was where it is.
                    let was = if shown.len() == track.shown.len() { &shown } else { &track.shown };
                    queue.write_buffer(previous, offset, bytemuck::cast_slice(was));
                    track.settled = false;
                }
                None if !track.settled => {
                    queue.write_buffer(previous, offset, bytemuck::cast_slice(&track.shown));
                    track.settled = true;
                }
                None => {}
            }
        }
        let (previous_view_projection, previous_body) = self.last.unwrap_or((view_projection, body));
        let uniform = Uniform {
            view_projection: view_projection.to_cols_array(),
            previous_view_projection: previous_view_projection.to_cols_array(),
            previous_body: previous_body.to_cols_array(),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        self.last = Some((view_projection, body));
    }
}

impl Renderer {
    /// Prepares the motion of the main view's frame of `view_projection` (unjittered), with the temporal
    /// anti-aliasing.
    pub(super) fn prepare_motion(&mut self, view_projection: Mat4) {
        let body = self.car_body.unwrap_or_default();
        if let Some(motion) = &mut self.motion {
            motion.prepare(&self.device, &self.queue, (&self.meshes, &self.buffers), view_projection, body);
        }
    }

    /// Draws the motion of the main view's frame of `view_projection` (jittered, as drawn) into its target, over the
    /// frame's depth.
    pub(super) fn draw_motion(&self, encoder: &mut wgpu::CommandEncoder, view_projection: Mat4) {
        let Some(motion) = &self.motion else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("motion"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &motion.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: None,
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&motion.pipeline);
        pass.set_bind_group(0, &self.shadow_frame, &[]);
        pass.set_bind_group(1, &motion.group, &[]);
        let eye = crate::camera::eye(view_projection);
        let frustum = Frustum::from_view_projection(view_projection);
        let mut bound = None;
        for (index, mesh) in self.meshes.iter().enumerate() {
            if !mesh.shown_in(View::Main) || !motion.moves(index, mesh) {
                continue;
            }
            let Some((center, half_extent)) = mesh.bounds else { continue };
            if !frustum.intersects_box(center, half_extent) || mesh.cell_hidden(view_projection) {
                continue;
            }
            let distance = center.distance(eye);
            // Every draw that writes depth, blended too (the gauges' markings): the frame's depth is one of theirs.
            let draws = mesh.draws.iter().filter(|d| d.pipeline.depth_write && d.lod.shows(distance));
            for draw in draws {
                let pack = mesh.slot.buffer();
                if bound != Some(pack) {
                    let (buffers, previous) = (&self.buffers[pack], motion.previous[pack].as_ref());
                    pass.set_vertex_buffer(0, buffers.vertices.slice(..));
                    pass.set_vertex_buffer(1, previous.expect("made by prepare").slice(..));
                    pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
                    bound = Some(pack);
                }
                let instance = u32::from(mesh.in_car.is_some()) | u32::from(mesh.car) << 1;
                pass.draw_indexed(draw.indices.clone(), mesh.slot.base_vertex(), instance..instance + 1);
            }
        }
    }
}

fn target(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("motion"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

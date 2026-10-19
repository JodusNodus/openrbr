//! The stage meshes on the GPU: their shared vertex and index buffers, their draws, and the updates of the car's
//! and the interactive props' vertices.

use super::pipelines::PipelineKey;
use super::textures::BindKey;
use super::Renderer;
use crate::coords::{interactive_pose_matrix, physics_pose_to_stage};
use crate::stage::{Anchor, Bounds, InteractivePose, Layer, Lod, Mesh, Vertex};
use crate::visibility;
use bytemuck::cast_slice;
use glam::{Mat4, Quat, Vec3};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// A mesh's GPU buffers and draws.
pub(super) struct GpuMesh {
    pub(super) bounds: Option<(glam::Vec3, glam::Vec3)>,
    /// Its visibility cell's box ([`Mesh::cell`]).
    pub(super) cell: Option<(glam::Vec3, glam::Vec3)>,
    /// Drawn; [`Renderer::set_visible`] hides the car's parts that a camera view leaves out.
    pub(super) visible: bool,
    /// For the car's parts, whether the outside views would draw it ([`Renderer::set_outside_visible`]): the door
    /// mirrors' passes draw those (`View::Mirror`).
    pub(super) outside: Option<bool>,
    pub(super) anchor: Anchor,
    pub(super) layer: Layer,
    /// Where its vertices and indices are ([`Packing`]); its draws' index ranges are in the shared index buffer.
    pub(super) slot: MeshSlot,
    pub(super) draws: Vec<GpuDraw>,
    pub(super) interactive: Option<InteractiveGpu>,
}

impl GpuMesh {
    /// Whether `view` draws the mesh.
    pub(super) fn shown_in(&self, view: super::frame::View) -> bool {
        match view {
            super::frame::View::Mirror { side: true } => self.outside.unwrap_or(self.visible),
            _ => self.visible,
        }
    }

    /// Whether the view of `view_projection` leaves the mesh out with its visibility cell (`visibility`).
    pub(super) fn cell_hidden(&self, view_projection: Mat4) -> bool {
        self.cell.is_some_and(|(center, half_extent)| visibility::beyond_reach(view_projection, center, half_extent))
    }

    /// Whether the reflection cube centred at `point` leaves the mesh out with its visibility cell
    /// (`visibility::near_cube`).
    pub(super) fn cell_far_from(&self, point: Vec3) -> bool {
        self.cell.is_some_and(|(center, half_extent)| !visibility::near_cube(point, center, half_extent))
    }
}

/// Where a mesh lies in the shared buffers ([`Packing`]): which pair, and its first vertex there.
#[derive(Debug, Clone, Copy)]
pub(super) struct MeshSlot {
    buffer: usize,
    base_vertex: i32,
}

/// One pair of shared vertex and index buffers.
pub(super) struct MeshBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
}

/// The most bytes of vertices or indices in one pair of [`MeshBuffers`], within wgpu's default buffer size limit.
const PACK_BYTES: usize = 1 << 27;

/// Packs the meshes' vertices and indices into a few shared buffers, so that draws of different meshes need no buffer
/// switch.
#[derive(Default)]
pub(super) struct Packing {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    buffers: Vec<MeshBuffers>,
}

impl Packing {
    /// Adds a mesh: its slot, and where its indices start in the shared index buffer.
    pub(super) fn add(&mut self, device: &wgpu::Device, vertices: &[Vertex], indices: &[u32]) -> (MeshSlot, u32) {
        let full = |used: usize, more: usize, size: usize| used > 0 && (used + more) * size > PACK_BYTES;
        if full(self.vertices.len(), vertices.len(), size_of::<Vertex>())
            || full(self.indices.len(), indices.len(), size_of::<u32>())
        {
            self.flush(device);
        }
        let slot = MeshSlot { buffer: self.buffers.len(), base_vertex: self.vertices.len() as i32 };
        let first_index = self.indices.len() as u32;
        self.vertices.extend_from_slice(vertices);
        self.indices.extend_from_slice(indices);
        (slot, first_index)
    }

    /// Uploads the meshes added since the last flush into a new pair of buffers.
    fn flush(&mut self, device: &wgpu::Device) {
        if self.vertices.is_empty() {
            return;
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertices"),
            contents: cast_slice(&std::mem::take(&mut self.vertices)),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh indices"),
            contents: cast_slice(&std::mem::take(&mut self.indices)),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.buffers.push(MeshBuffers { vertices, indices });
    }

    pub(super) fn finish(mut self, device: &wgpu::Device) -> Vec<MeshBuffers> {
        self.flush(device);
        self.buffers
    }
}

/// Binds the shared buffers of each mesh drawn in a pass, when they are not the ones bound already.
pub(super) struct Bound<'a> {
    buffers: &'a [MeshBuffers],
    buffer: Option<usize>,
}

impl<'a> Bound<'a> {
    pub(super) fn new(buffers: &'a [MeshBuffers]) -> Bound<'a> {
        Bound { buffers, buffer: None }
    }

    /// Binds `mesh`'s buffers and draws `draw` of it.
    pub(super) fn draw(&mut self, pass: &mut wgpu::RenderPass<'_>, mesh: &GpuMesh, draw: &GpuDraw) {
        if self.buffer != Some(mesh.slot.buffer) {
            let buffers = &self.buffers[mesh.slot.buffer];
            pass.set_vertex_buffer(0, buffers.vertices.slice(..));
            pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
            self.buffer = Some(mesh.slot.buffer);
        }
        pass.draw_indexed(draw.indices.clone(), mesh.slot.base_vertex, 0..1);
    }
}

pub(super) struct InteractiveGpu {
    pub(super) placement: [f32; 16],
    pub(super) local_mesh: Arc<Mesh>,
    pub(super) current_pose: Option<InteractivePose>,
}

pub(super) struct GpuDraw {
    pub(super) pipeline: PipelineKey,
    /// The pipeline of [`DrawPass::Main`](super::pipelines::DrawPass::Main), and for a draw whose depth is laid first
    /// in the main view ([`prepassed`](super::pipelines::prepassed)) those of the prepass and after it.
    pub(super) main: wgpu::RenderPipeline,
    pub(super) prepassed: Option<[wgpu::RenderPipeline; 2]>,
    pub(super) group: wgpu::BindGroup,
    /// Drawn into the car's shadow map: the opaque lit draws (the car's body, wheels and plastic).
    pub(super) casts_shadow: bool,
    pub(super) bind: BindKey,
    pub(super) lod: Lod,
    /// Index range within the shared index buffer ([`MeshSlot`]).
    pub(super) indices: Range<u32>,
}

impl Renderer {
    /// Replaces the positions of stage mesh `index` (its index in the `Stage` given to [`Renderer::new`]) and its
    /// culling bounds, for a car that moves. The vertex count must not change. Meshes with no draws are ignored.
    pub fn set_vertices(&mut self, index: usize, vertices: &[Vertex], bounds: Option<Bounds>) {
        let Some(Some(slot)) = self.slots.get(index) else {
            return;
        };
        let mesh = &mut self.meshes[*slot];
        self.queue.write_buffer(
            &self.buffers[mesh.slot.buffer].vertices,
            vertex_offset(mesh.slot),
            cast_slice(vertices),
        );
        mesh.bounds = bounds.map(|b| (b.center, b.half_extent));
    }

    /// Updates supported interactive instances from the live physics pose map. The live layer keeps
    /// the last pose after a solid leaves the awake list, so a sleeping prop remains where it stopped.
    pub fn set_interactive_poses(&mut self, poses: &HashMap<u32, InteractivePose>) {
        if poses.is_empty() {
            for mesh in &mut self.meshes {
                let Some(base) = mesh.interactive.as_mut() else {
                    continue;
                };
                if base.current_pose.take().is_none() {
                    continue;
                }
                let vertices = transformed_vertices(&base.local_mesh.vertices, Mat4::from_cols_array(&base.placement));
                let buffer = &self.buffers[mesh.slot.buffer].vertices;
                self.queue.write_buffer(buffer, vertex_offset(mesh.slot), cast_slice(&vertices));
                mesh.bounds = mesh_bounds(&vertices).map(|b| (b.center, b.half_extent));
            }
            return;
        }
        for (&handle, &pose) in poses {
            let Some(&slot) = self.interactive_slots.get(&handle) else {
                continue;
            };
            let mesh = &mut self.meshes[slot];
            let Some(base) = mesh.interactive.as_mut() else {
                continue;
            };
            if base.current_pose == Some(pose) {
                continue;
            }
            let physics_position = Vec3::from(pose.position);
            let physics_orientation =
                Quat::from_xyzw(pose.orientation[0], pose.orientation[1], pose.orientation[2], pose.orientation[3]);
            if !physics_position.is_finite()
                || !physics_orientation.is_finite()
                || physics_orientation.length_squared() < 1e-8
            {
                continue;
            }
            let (position, orientation) = physics_pose_to_stage(physics_position, physics_orientation);
            let matrix = interactive_pose_matrix(base.placement, position, orientation);
            let vertices = transformed_vertices(&base.local_mesh.vertices, matrix);
            self.queue.write_buffer(
                &self.buffers[mesh.slot.buffer].vertices,
                vertex_offset(mesh.slot),
                cast_slice(&vertices),
            );
            mesh.bounds = mesh_bounds(&vertices).map(|b| (b.center, b.half_extent));
            base.current_pose = Some(pose);
        }
    }

    /// Marks the stage meshes `car` (indices in the `Stage` given to [`Renderer::new`]) as the car's: only their opaque
    /// lit draws cast the car's shadow, so its shadow maps fit the car, not every lit mesh (the characters along the
    /// stage, the ghost car). Until this is called every opaque lit draw casts.
    pub fn set_car(&mut self, car: &[usize]) {
        let slots: HashSet<usize> = car.iter().filter_map(|&index| self.slots.get(index).copied().flatten()).collect();
        for (slot, mesh) in self.meshes.iter_mut().enumerate() {
            for draw in &mut mesh.draws {
                draw.casts_shadow = slots.contains(&slot) && draw.bind.shade.is_some() && draw.pipeline.depth_write;
            }
        }
    }

    /// Shows or hides stage mesh `index` (the car's interior or exterior set, by camera view).
    pub fn set_visible(&mut self, index: usize, visible: bool) {
        if let Some(Some(slot)) = self.slots.get(index) {
            self.meshes[*slot].visible = visible;
        }
    }

    /// Whether the outside views would show stage mesh `index`, a part of the car (the door mirrors draw what they
    /// would, [`crate::mirrors`]).
    pub fn set_outside_visible(&mut self, index: usize, visible: bool) {
        if let Some(Some(slot)) = self.slots.get(index) {
            self.meshes[*slot].outside = Some(visible);
        }
    }
}

/// The byte offset of a mesh's first vertex in its shared vertex buffer.
fn vertex_offset(slot: MeshSlot) -> u64 {
    slot.base_vertex as u64 * size_of::<Vertex>() as u64
}

pub(super) fn transformed_vertices(vertices: &[Vertex], transform: Mat4) -> Vec<Vertex> {
    vertices
        .iter()
        .map(|vertex| {
            let mut world = *vertex;
            world.position = transform.transform_point3(Vec3::from(vertex.position)).to_array();
            world
        })
        .collect()
}

/// Axis-aligned bounds of the vertices, or `None` for none.
pub(crate) fn mesh_bounds(vertices: &[Vertex]) -> Option<Bounds> {
    let first = Vec3::from(vertices.first()?.position);
    let (low, high) = vertices.iter().skip(1).fold((first, first), |(low, high), vertex| {
        let position = Vec3::from(vertex.position);
        (low.min(position), high.max(position))
    });
    Some(Bounds { center: (low + high) * 0.5, half_extent: (high - low) * 0.5 })
}

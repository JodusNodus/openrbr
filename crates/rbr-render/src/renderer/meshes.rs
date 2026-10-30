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
    /// Drawn only in the in-car mirrors' passes (the windscreen's reflection, the centre mirror), casting no shadow
    /// ([`Renderer::set_reflected_only`]).
    pub(super) reflected: bool,
    pub(super) anchor: Anchor,
    pub(super) layer: Layer,
    /// Where its vertices and indices are ([`Packing`]); its draws' index ranges are in the shared index buffer.
    pub(super) slot: MeshSlot,
    pub(super) draws: Vec<GpuDraw>,
    pub(super) interactive: Option<InteractiveGpu>,
    /// For the car's parts kept in its model space ([`Renderer::set_car_vertices`]), their bounds there: drawn as
    /// instance 1, which the vertex shader places by the car's body (`placed` in `shader.wgsl`).
    pub(super) in_car: Option<(Vec3, Vec3)>,
    /// One of the car's or its crew's meshes ([`Renderer::set_car`]): the car's own shadow map covers them, so the
    /// sun's cascades leave them out.
    pub(super) car: bool,
    /// A sky dome over the stage ([`crate::sky_dome`]): drawn, but no caster (and left out under `Enhance::sky`).
    pub(super) sky_dome: bool,
    /// For the backdrop's meshes (world-anchored sky faces, which have no `bounds`: the views draw them at the far
    /// plane), the centre and half extents of their vertices, by which the sun's maps cull them.
    pub(super) backdrop_bounds: Option<(Vec3, Vec3)>,
}

impl GpuMesh {
    /// Whether `view` draws the mesh.
    pub(super) fn shown_in(&self, view: super::frame::View) -> bool {
        match view {
            super::frame::View::Mirror { side: true } => !self.reflected && self.outside.unwrap_or(self.visible),
            super::frame::View::Mirror { side: false } => self.visible,
            _ => self.visible && !self.reflected,
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
    pub(super) vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    /// How many vertices `vertices` holds.
    pub(super) vertex_count: u32,
    /// The vertex ranges of the backdrop's meshes (world-anchored sky faces), which the sky's occlusion weighs against
    /// the backdrop alone.
    pub(super) backdrop: Vec<Range<u32>>,
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
    /// The ranges of `vertices` that are the backdrop's.
    backdrop: Vec<Range<u32>>,
}

impl Packing {
    /// Adds a mesh (`backdrop`: one of the backdrop's): its slot, and where its indices start in the shared index
    /// buffer.
    pub(super) fn add(
        &mut self,
        device: &wgpu::Device,
        vertices: &[Vertex],
        indices: &[u32],
        backdrop: bool,
    ) -> (MeshSlot, u32) {
        let full = |used: usize, more: usize, size: usize| used > 0 && (used + more) * size > PACK_BYTES;
        if full(self.vertices.len(), vertices.len(), size_of::<Vertex>())
            || full(self.indices.len(), indices.len(), size_of::<u32>())
        {
            self.flush(device);
        }
        let slot = MeshSlot { buffer: self.buffers.len(), base_vertex: self.vertices.len() as i32 };
        let first_index = self.indices.len() as u32;
        if backdrop {
            let first = self.vertices.len() as u32;
            self.backdrop.push(first..first + vertices.len() as u32);
        }
        self.vertices.extend_from_slice(vertices);
        self.indices.extend_from_slice(indices);
        (slot, first_index)
    }

    /// Uploads the meshes added since the last flush into a new pair of buffers.
    fn flush(&mut self, device: &wgpu::Device) {
        if self.vertices.is_empty() {
            return;
        }
        let vertex_count = self.vertices.len() as u32;
        // Storage too, for the sky's occlusion to write into (`sky_occlusion.wgsl`), and read back for the grass's
        // light (`Renderer::set_grass`).
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertices"),
            contents: cast_slice(&std::mem::take(&mut self.vertices)),
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::STORAGE,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh indices"),
            contents: cast_slice(&std::mem::take(&mut self.indices)),
            usage: wgpu::BufferUsages::INDEX,
        });
        self.buffers.push(MeshBuffers {
            vertices,
            indices,
            vertex_count,
            backdrop: std::mem::take(&mut self.backdrop),
        });
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
        self.draw_instance(pass, mesh, draw, u32::from(mesh.in_car.is_some()));
    }

    /// [`Bound::draw`] as instance `instance`.
    pub(super) fn draw_instance(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        mesh: &GpuMesh,
        draw: &GpuDraw,
        instance: u32,
    ) {
        if self.buffer != Some(mesh.slot.buffer) {
            let buffers = &self.buffers[mesh.slot.buffer];
            pass.set_vertex_buffer(0, buffers.vertices.slice(..));
            pass.set_index_buffer(buffers.indices.slice(..), wgpu::IndexFormat::Uint32);
            self.buffer = Some(mesh.slot.buffer);
        }
        pass.draw_indexed(draw.indices.clone(), mesh.slot.base_vertex, instance..instance + 1);
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
    /// For the stage's unlit draws, `main` and the pipeline after the prepass with the lamps' code
    /// ([`PipelineKey::lamps`](super::pipelines::PipelineKey)), for the frames the lamps light.
    pub(super) lamps: Option<[wgpu::RenderPipeline; 2]>,
    /// For the lit draws, `main` for a view outside the car (`main` itself) and in it
    /// ([`PipelineKey::inside`](super::pipelines::PipelineKey)).
    pub(super) views: Option<[wgpu::RenderPipeline; 2]>,
    pub(super) group: wgpu::BindGroup,
    /// Drawn into the car's shadow map: the opaque lit draws (the car's body, wheels and plastic).
    pub(super) casts_shadow: bool,
    pub(super) bind: BindKey,
    /// Ground that takes the wet film by its surface (`puddles::mark_roads`), with
    /// [`Enhance::wet`](super::Enhance::wet): gets the screen-space reflections in wet weather.
    pub(super) wet_road: bool,
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
        mesh.in_car = None;
    }

    /// Replaces the vertices of stage mesh `index`, a car part that moves only with the car's body, in the body's model
    /// space with their `bounds` there: the GPU places them by [`Renderer::set_car_body`], so a part is given again only
    /// when it changes. [`Renderer::set_vertices`] gives it back in stage space.
    pub fn set_car_vertices(&mut self, index: usize, vertices: &[Vertex], bounds: Option<Bounds>) {
        let Some(Some(slot)) = self.slots.get(index) else {
            return;
        };
        let mesh = &mut self.meshes[*slot];
        self.queue.write_buffer(
            &self.buffers[mesh.slot.buffer].vertices,
            vertex_offset(mesh.slot),
            cast_slice(vertices),
        );
        // An empty part draws nothing wherever it is.
        mesh.in_car = bounds.map(|b| (b.center, b.half_extent));
        mesh.bounds = mesh.in_car.map(|model| placed_box(self.car_body.unwrap_or_default(), model));
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
    /// stage, the ghost car). Until this is called every opaque lit draw casts. Neither they nor the `crew` riding in it
    /// cast into the sun's cascades.
    pub fn set_car(&mut self, car: &[usize], crew: &[usize]) {
        let slot_of = |index: &usize| self.slots.get(*index).copied().flatten();
        let slots: HashSet<usize> = car.iter().filter_map(slot_of).collect();
        let riders: HashSet<usize> = crew.iter().filter_map(slot_of).collect();
        for (slot, mesh) in self.meshes.iter_mut().enumerate() {
            mesh.car = slots.contains(&slot) || riders.contains(&slot);
            for draw in &mut mesh.draws {
                draw.casts_shadow = slots.contains(&slot) && draw.bind.shade.is_some() && draw.pipeline.depth_write;
            }
        }
    }

    /// Streams `field`'s grass into the stage's grass tile meshes ([`crate::grass::add`]), about the eye each frame, in
    /// the light computed at load for the ground under it.
    pub fn set_grass(&mut self, field: Arc<crate::grass::Field>) {
        let tiles = self.meshes.iter().enumerate().filter_map(|(i, m)| {
            let draw = m.draws.iter().find(|d| d.lod == Lod::Close)?;
            Some((i, draw.indices.len() / 6))
        });
        let tiles = tiles.collect();
        let light = if self.ground_light.is_some() { self.grass_light(&field) } else { Vec::new() };
        self.grass = Some(crate::grass::Ring::new(field, tiles, light));
    }

    /// The light of the corners of `field`'s ground triangles ([`crate::grass::Light`]), read back from the vertex
    /// buffers: one copy of each mesh's span of the vertices wanted. Open sky where the read fails.
    fn grass_light(&self, field: &crate::grass::Field) -> Vec<[crate::grass::Light; 3]> {
        let gpu_mesh = |mesh: usize| self.slots.get(mesh).copied().flatten();
        let mut spans: HashMap<usize, Range<u32>> = HashMap::new();
        for (mesh, corners) in field.corners() {
            let Some(gpu) = gpu_mesh(mesh) else { continue };
            let (low, high) = (corners.iter().min().unwrap_or(&0), corners.iter().max().unwrap_or(&0));
            let span = spans.entry(gpu).or_insert(*low..*high + 1);
            *span = span.start.min(*low)..span.end.max(*high + 1);
        }
        let size = size_of::<Vertex>() as u64;
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("grass light") });
        // Each mesh's span and its first vertex in the read.
        let mut read = HashMap::new();
        let mut at = 0;
        for (&gpu, span) in &spans {
            read.insert(gpu, (span.start, at));
            at += span.len() as u64;
        }
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grass light"),
            size: at.max(1) * size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        for (gpu, span) in &spans {
            let slot = self.meshes[*gpu].slot;
            let source = vertex_offset(slot) + u64::from(span.start) * size;
            encoder.copy_buffer_to_buffer(
                &self.buffers[slot.buffer].vertices,
                source,
                &readback,
                read[gpu].1 * size,
                span.len() as u64 * size,
            );
        }
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let mapped =
            self.device.poll(wgpu::PollType::wait_indefinitely()).ok().and_then(|_| receiver.recv().ok()?.ok());
        let vertices: Vec<Vertex> = match mapped {
            Some(()) => bytemuck::pod_collect_to_vec(&readback.slice(..).get_mapped_range().expect("mapped above")),
            None => return Vec::new(),
        };
        readback.unmap();
        field
            .corners()
            .map(|(mesh, corners)| {
                let Some(&(start, first)) = gpu_mesh(mesh).and_then(|gpu| read.get(&gpu)) else {
                    return Default::default();
                };
                corners.map(|c| crate::grass::Light::of(&vertices[(first + u64::from(c - start)) as usize]))
            })
            .collect()
    }

    /// Fills the grass tile meshes the eye at `eye` now wants ([`crate::grass::Ring::update`]).
    pub(super) fn stream_grass(&mut self, eye: Vec3) {
        let Some(mut ring) = self.grass.take() else { return };
        for (index, vertices, bounds) in ring.update(eye) {
            let mesh = &mut self.meshes[index];
            if !vertices.is_empty() {
                let buffer = &self.buffers[mesh.slot.buffer].vertices;
                self.queue.write_buffer(buffer, vertex_offset(mesh.slot), cast_slice(&vertices));
            }
            mesh.bounds = Some((bounds.center, bounds.half_extent));
            // Only the tile's cards are drawn: past them the mesh holds an earlier tile's.
            for draw in &mut mesh.draws {
                draw.indices.end = draw.indices.start + (vertices.len() / 4 * 6) as u32;
            }
        }
        self.grass = Some(ring);
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

    /// Draws stage mesh `index` only in the in-car mirrors' passes, with no shadow (`glass::bulkhead`; after
    /// [`Renderer::set_car`]).
    pub fn set_reflected_only(&mut self, index: usize) {
        if let Some(Some(slot)) = self.slots.get(index) {
            let mesh = &mut self.meshes[*slot];
            mesh.reflected = true;
            mesh.draws.iter_mut().for_each(|draw| draw.casts_shadow = false);
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

/// The stage-space box around the model-space box `(centre, half extent)` placed by `model_to_stage`.
pub(super) fn placed_box(model_to_stage: Mat4, (centre, half): (Vec3, Vec3)) -> (Vec3, Vec3) {
    let m = glam::Mat3::from_mat4(model_to_stage);
    let reach = glam::Mat3::from_cols(m.x_axis.abs(), m.y_axis.abs(), m.z_axis.abs()) * half;
    (model_to_stage.transform_point3(centre), reach)
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

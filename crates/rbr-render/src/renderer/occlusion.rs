//! The sky's occlusion over the stage's meshes ([`crate::sky_occlusion`]), run once when the renderer is made: the
//! tiles' depth maps along each direction drawn as the sun's cascades are ([`Casting::draw_layer`]), and the compute
//! passes of `sky_occlusion.wgsl` over every vertex buffer.

use super::meshes::{GpuMesh, MeshBuffers};
use super::shadows::Casting;
use crate::sky_occlusion::{self, Tiles, BATCH, SIZE, TILE};
use crate::stage::{Anchor, Layer};
use crate::sun::SunShadow;
use bytemuck::{cast_slice, Pod, Zeroable};
use glam::Mat4;

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Params {
    toward: [f32; 4],
    grid: [f32; 4],
    counts: [u32; 4],
}

/// Writes each world-anchored stage vertex's [`Vertex::occlusion`](crate::stage::Vertex::occlusion) into `buffers`
/// (`meshes`' vertices); the stage textures' bind groups are laid out by `texture_layout`. Returns the tiles drawn.
pub(super) fn bake(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_layout: &wgpu::BindGroupLayout,
    meshes: &[GpuMesh],
    buffers: &[MeshBuffers],
) -> usize {
    let receivers = meshes
        .iter()
        .filter(|m| m.anchor == Anchor::World && m.layer != Layer::Sky && !m.car && m.in_car.is_none())
        .filter_map(|m| m.bounds);
    let Some(tiles) = Tiles::of(receivers) else { return 0 };
    let maps = SunShadow::with_layers(device, texture_layout, SIZE, BATCH);
    let casting = Casting { meshes, buffers, sun: &maps, queue };

    let uniform = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let params = uniform("sky occlusion", size_of::<Params>() as u64);
    let matrices = uniform("sky occlusion maps", (BATCH * size_of::<Mat4>()) as u64);
    let cells = (tiles.columns * tiles.rows) as usize;
    let map_of = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sky occlusion tiles"),
        size: (cells * size_of::<i32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sky occlusion"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sky occlusion"),
        source: wgpu::ShaderSource::Wgsl(sky_occlusion::SHADER.into()),
    });
    let pipeline = |entry| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sky occlusion"),
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let (accumulate, finish) = (pipeline("accumulate"), pipeline("finish"));
    let storage = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    };
    let sums: Vec<(wgpu::Buffer, wgpu::Buffer)> = buffers
        .iter()
        .map(|b| {
            let count = u64::from(b.vertex_count.max(1));
            (storage("sky occlusion sums", count * 16), storage("sky occlusion totals", count * 4))
        })
        .collect();
    let groups = |pipeline: &wgpu::ComputePipeline, all: bool| -> Vec<wgpu::BindGroup> {
        let layout = pipeline.get_bind_group_layout(0);
        buffers
            .iter()
            .zip(&sums)
            .map(|(b, (sums, totals))| {
                let mut entries = vec![
                    wgpu::BindGroupEntry { binding: 5, resource: b.vertices.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 6, resource: sums.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 7, resource: totals.as_entire_binding() },
                ];
                if all {
                    entries.extend([
                        wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: matrices.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: map_of.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&maps.view) },
                        wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&sampler) },
                    ]);
                }
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sky occlusion"),
                    layout: &layout,
                    entries: &entries,
                })
            })
            .collect()
    };
    let (accumulate_groups, finish_groups) = (groups(&accumulate, true), groups(&finish, false));
    let dispatch = |pass: &mut wgpu::ComputePass, pipeline, groups: &[wgpu::BindGroup]| {
        pass.set_pipeline(pipeline);
        for (b, group) in buffers.iter().zip(groups) {
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(b.vertex_count.div_ceil(64), 1, 1);
        }
    };

    for toward in sky_occlusion::directions() {
        for batch in tiles.touched.chunks(BATCH) {
            let mut map_cells = vec![-1i32; cells];
            let mut batch_matrices = [Mat4::IDENTITY; BATCH];
            let mut texel = 0.0f32;
            for (k, &(index, low, high)) in batch.iter().enumerate() {
                let (matrix, t) = tiles.matrix(index, low, high, toward);
                (batch_matrices[k], map_cells[index as usize], texel) = (matrix, k as i32, texel.max(t));
            }
            let p = Params {
                toward: toward.extend(1.5 * texel).to_array(),
                grid: [tiles.origin.x, tiles.origin.y, TILE, tiles.columns as f32],
                counts: [tiles.rows, 0, 0, 0],
            };
            queue.write_buffer(&params, 0, bytemuck::bytes_of(&p));
            queue.write_buffer(&matrices, 0, cast_slice(&batch_matrices.map(|m| m.to_cols_array_2d())));
            queue.write_buffer(&map_of, 0, cast_slice(&map_cells));
            let mut encoder = device.create_command_encoder(&Default::default());
            for (k, &matrix) in batch_matrices.iter().enumerate().take(batch.len()) {
                casting.draw_layer(&mut encoder, k, matrix, None);
            }
            dispatch(&mut encoder.begin_compute_pass(&Default::default()), &accumulate, &accumulate_groups);
            queue.submit([encoder.finish()]);
        }
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    dispatch(&mut encoder.begin_compute_pass(&Default::default()), &finish, &finish_groups);
    queue.submit([encoder.finish()]);
    tiles.touched.len()
}

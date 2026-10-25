//! The light the stage bounces onto itself ([`crate::bounce`]), drawn with the sky's occlusion when the renderer is
//! made: the sun's tile maps as the sky's occlusion draws its own
//! ([`Casting::draw_layer`]), the tiles' lit surfaces from below along each direction (`bounce_surface.wgsl`), and the
//! compute passes of `bounce.wgsl` over every vertex buffer; and the ground's light seen from above ([`Ground`]).

use super::meshes::{Bound, GpuDraw, GpuMesh, MeshBuffers};
use super::shadows::Casting;
use super::DEPTH_FORMAT;
use crate::bounce::{self, DIRECTIONS, GROUND, SIZE};
use crate::camera::Frustum;
use crate::sky_occlusion::{self, Tiles, BATCH, TILE};
use crate::stage::{Anchor, Layer, Stage};
use crate::sun::SunShadow;
use bytemuck::{cast_slice, Pod, Zeroable};
use glam::{Mat4, Vec3};

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Params {
    toward: [f32; 4],
    grid: [f32; 4],
    counts: [u32; 4],
    weight: [f32; 4],
    tiles: [[i32; 4]; BATCH],
}

const LIGHT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The ground's light seen from above, for the draws that move (`ground_bounce` in `shader.wgsl`): a pair of layers per
/// tile of the stage (the sun's, the sky's; [`GROUND`] texels across), each grid cell's tile (its pair's index, or -1),
/// and the grid for the frame (its corner's x and z, columns and rows; all 0 without the maps).
pub(super) struct Ground {
    tiles: Option<Tiles>,
    pub(super) view: wgpu::TextureView,
    pub(super) cells: wgpu::Buffer,
    pub(super) grid: [f32; 4],
}

impl Ground {
    /// The maps over the tiles of `stage`'s world meshes when `on` (the sky's occlusion's, [`Tiles`]) and the device holds
    /// their layers, else a placeholder that the shader skips.
    pub(super) fn new(device: &wgpu::Device, stage: &Stage, on: bool) -> Ground {
        let receivers = stage.meshes.iter().filter(|m| m.anchor == Anchor::World && m.layer != Layer::Sky);
        let tiles = on
            .then(|| Tiles::of(receivers.filter_map(|m| m.bounds).map(|b| (b.center, b.half_extent))))
            .flatten()
            .filter(|t| 2 * t.touched.len() as u32 <= device.limits().max_texture_array_layers);
        let (size, layers) = tiles.as_ref().map_or((1, 1), |t| (GROUND, 2 * t.touched.len() as u32));
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ground light"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let mut cells = vec![-1i32; tiles.as_ref().map_or(1, |t| (t.columns * t.rows) as usize)];
        for (k, &(index, ..)) in tiles.iter().flat_map(|t| t.touched.iter()).enumerate() {
            cells[index as usize] = k as i32;
        }
        let cells = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("ground tiles"),
                contents: cast_slice(&cells),
                usage: wgpu::BufferUsages::STORAGE,
            },
        );
        let grid = tiles.as_ref().map_or([0.0; 4], |t| [t.origin.x, t.origin.y, t.columns as f32, t.rows as f32]);
        Ground { tiles, view, cells, grid }
    }
}

/// Writes each vertex's [`Vertex::bounce`](crate::stage::Vertex::bounce) into `buffers` (`meshes`' vertices) and the
/// ground's maps into `ground` for the sun `toward` (toward it), over `ground`'s tiles; the stage textures' bind groups
/// are laid out by `texture_layout`.
pub(super) fn bake(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_layout: &wgpu::BindGroupLayout,
    meshes: &[GpuMesh],
    buffers: &[MeshBuffers],
    ground: &Ground,
    toward: Vec3,
) {
    let start = std::time::Instant::now();
    let Some(tiles) = &ground.tiles else { return };

    let uniform = |label, size| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    };
    let params = uniform("bounce", size_of::<Params>() as u64);
    let matrices = uniform("bounce maps", (BATCH * size_of::<Mat4>()) as u64);
    let cells = (tiles.columns * tiles.rows) as usize;
    let map_of = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bounce tiles"),
        size: (cells * size_of::<i32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let compare = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bounce depth"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    });
    let linear = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("bounce light"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bounce"),
        source: wgpu::ShaderSource::Wgsl(bounce::SHADER.into()),
    });
    let pipeline = |entry| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("bounce"),
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let [clear, sun, gather, finish, down] = ["clear", "sun", "gather", "finish", "ground"].map(pipeline);
    let sums: Vec<[wgpu::Buffer; 2]> = buffers
        .iter()
        .map(|b| {
            let size = u64::from(b.vertex_count.max(1)) * 16;
            ["bounce sun sums", "bounce sky sums"].map(|label| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage: wgpu::BufferUsages::STORAGE,
                    mapped_at_creation: false,
                })
            })
        })
        .collect();

    let sun_maps = SunShadow::with_layers(device, texture_layout, sky_occlusion::SIZE, BATCH);
    let surfaces = Surfaces::new(device, texture_layout);
    // The bind groups of `pipeline` over each vertex buffer: of `bindings` (`bounce.wgsl`'s), with the depth maps
    // `depth`.
    let groups = |pipeline: &wgpu::ComputePipeline, bindings: &[u32], depth: &wgpu::TextureView| {
        let layout = pipeline.get_bind_group_layout(0);
        buffers
            .iter()
            .zip(&sums)
            .map(|(b, [sun_sums, sky_sums])| {
                let entries: Vec<wgpu::BindGroupEntry> = bindings
                    .iter()
                    .map(|&binding| {
                        let resource = match binding {
                            0 => params.as_entire_binding(),
                            1 => matrices.as_entire_binding(),
                            2 => map_of.as_entire_binding(),
                            3 => wgpu::BindingResource::TextureView(depth),
                            4 => wgpu::BindingResource::Sampler(&compare),
                            5 => b.vertices.as_entire_binding(),
                            6 => sun_sums.as_entire_binding(),
                            7 => sky_sums.as_entire_binding(),
                            8 => wgpu::BindingResource::TextureView(&surfaces.sun_view),
                            9 => wgpu::BindingResource::TextureView(&surfaces.sky_view),
                            10 => wgpu::BindingResource::Sampler(&linear),
                            _ => wgpu::BindingResource::TextureView(&ground.view),
                        };
                        wgpu::BindGroupEntry { binding, resource }
                    })
                    .collect();
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bounce"),
                    layout: &layout,
                    entries: &entries,
                })
            })
            .collect::<Vec<_>>()
    };
    let clear_groups = groups(&clear, &[5, 6], &sun_maps.view);
    let sun_groups = groups(&sun, &[0, 1, 2, 3, 4, 5, 6], &sun_maps.view);
    let gather_groups = groups(&gather, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10], &surfaces.depth_view);
    let finish_groups = groups(&finish, &[5, 6, 7], &sun_maps.view);
    let down_group = groups(&down, &[0, 1, 8, 9, 10, 11], &sun_maps.view).swap_remove(0);
    let dispatch = |encoder: &mut wgpu::CommandEncoder, pipeline, groups: &[wgpu::BindGroup]| {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(pipeline);
        for (b, group) in buffers.iter().zip(groups) {
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(b.vertex_count.div_ceil(64), 1, 1);
        }
    };
    // Each tile of `batch` along `toward` into the map slots: their matrices, the grid's map of each tile, and the
    // parameters with the largest texel of maps `size` texels across.
    let batch_maps = |batch: &[(u32, f32, f32)], toward: Vec3, size: u32, weight: f32| {
        let mut map_cells = vec![-1i32; cells];
        let mut batch_matrices = [Mat4::IDENTITY; BATCH];
        let mut texel = 0.0f32;
        for (k, &(index, low, high)) in batch.iter().enumerate() {
            let (matrix, t) = tiles.matrix(index, low, high, toward);
            (batch_matrices[k], map_cells[index as usize], texel) = (matrix, k as i32, texel.max(t));
        }
        let texel = texel * sky_occlusion::SIZE as f32 / size as f32;
        let p = Params {
            toward: toward.extend(1.5 * texel).to_array(),
            grid: [tiles.origin.x, tiles.origin.y, TILE, tiles.columns as f32],
            counts: [tiles.rows, 0, 0, 0],
            weight: [weight, 0.0, 0.0, 0.0],
            tiles: std::array::from_fn(|k| {
                batch.get(k).map_or([0; 4], |&(index, ..)| {
                    let pair = tiles.touched.iter().position(|t| t.0 == index).unwrap_or(0) as i32;
                    [pair, (index % tiles.columns) as i32, (index / tiles.columns) as i32, 0]
                })
            }),
        };
        queue.write_buffer(&params, 0, bytemuck::bytes_of(&p));
        queue.write_buffer(&matrices, 0, cast_slice(&batch_matrices.map(|m| m.to_cols_array_2d())));
        queue.write_buffer(&map_of, 0, cast_slice(&map_cells));
        batch_matrices
    };

    let mut encoder = device.create_command_encoder(&Default::default());
    dispatch(&mut encoder, &clear, &clear_groups);
    queue.submit([encoder.finish()]);
    // Every vertex's share of the sun first: the surfaces drawn next reach into the tiles around theirs.
    let casting = Casting { meshes, buffers, sun: &sun_maps, queue };
    for batch in tiles.touched.chunks(BATCH) {
        let batch_matrices = batch_maps(batch, toward, sky_occlusion::SIZE, 0.0);
        let mut encoder = device.create_command_encoder(&Default::default());
        for (k, &matrix) in batch_matrices.iter().enumerate().take(batch.len()) {
            casting.draw_layer(&mut encoder, k, matrix, None);
        }
        dispatch(&mut encoder, &sun, &sun_groups);
        queue.submit([encoder.finish()]);
    }
    // The ground's maps from above, without the cut-out draws, by the shares just found.
    for batch in tiles.touched.chunks(BATCH) {
        let batch_matrices = batch_maps(batch, Vec3::Y, SIZE, 0.0);
        let mut encoder = device.create_command_encoder(&Default::default());
        for (k, &matrix) in batch_matrices.iter().enumerate().take(batch.len()) {
            surfaces.draw(queue, &mut encoder, k, matrix, (meshes, buffers), false);
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&down);
            pass.set_bind_group(0, &down_group, &[]);
            pass.dispatch_workgroups(GROUND / 8, GROUND / 8, batch.len() as u32);
        }
        queue.submit([encoder.finish()]);
    }
    let weight = 2.0 / DIRECTIONS as f32;
    for look in bounce::directions() {
        for batch in tiles.touched.chunks(BATCH) {
            let batch_matrices = batch_maps(batch, look, SIZE, weight);
            let mut encoder = device.create_command_encoder(&Default::default());
            for (k, &matrix) in batch_matrices.iter().enumerate().take(batch.len()) {
                surfaces.draw(queue, &mut encoder, k, matrix, (meshes, buffers), true);
            }
            dispatch(&mut encoder, &gather, &gather_groups);
            queue.submit([encoder.finish()]);
        }
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    dispatch(&mut encoder, &finish, &finish_groups);
    queue.submit([encoder.finish()]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    eprintln!("bounce: {} tiles in {:.1} s", tiles.touched.len(), start.elapsed().as_secs_f32());
}

/// The tiles' maps of the lit surfaces: two colour layers each (per unit of the sun's and of the sky's light) and
/// their depth, the uniform and bind group of each layer's matrix, and the pipeline that draws stage draws into them.
struct Surfaces {
    sun_view: wgpu::TextureView,
    sky_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    /// Each layer's views: the sun's, the sky's and the depth.
    layers: Vec<[wgpu::TextureView; 3]>,
    uniforms: Vec<wgpu::Buffer>,
    groups: Vec<wgpu::BindGroup>,
    pipeline: wgpu::RenderPipeline,
}

impl Surfaces {
    fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Surfaces {
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: BATCH as u32 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures = [
            texture("bounce sun light", LIGHT_FORMAT),
            texture("bounce sky light", LIGHT_FORMAT),
            texture("bounce depth", DEPTH_FORMAT),
        ];
        let [sun_view, sky_view, depth_view] = textures.each_ref().map(|t| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        });
        let layers = (0..BATCH as u32)
            .map(|layer| {
                textures.each_ref().map(|t| {
                    t.create_view(&wgpu::TextureViewDescriptor {
                        dimension: Some(wgpu::TextureViewDimension::D2),
                        base_array_layer: layer,
                        array_layer_count: Some(1),
                        ..Default::default()
                    })
                })
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bounce surfaces"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniforms: Vec<wgpu::Buffer> = (0..BATCH)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bounce surfaces"),
                    size: size_of::<Mat4>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let groups = uniforms
            .iter()
            .map(|uniform| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bounce surfaces"),
                    layout: &layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
                })
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bounce surfaces"),
            source: wgpu::ShaderSource::Wgsl(bounce::SURFACE_SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bounce surfaces"),
            bind_group_layouts: &[Some(&layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let target =
            Some(wgpu::ColorTargetState { format: LIGHT_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bounce surfaces"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_surface"),
                compilation_options: Default::default(),
                buffers: &[Some(super::vertex_layout())],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_surface"),
                compilation_options: Default::default(),
                targets: &[target.clone(), target],
            }),
            multiview_mask: None,
            cache: None,
        });
        Surfaces { sun_view, sky_view, depth_view, layers, uniforms, groups, pipeline }
    }

    /// Clears layer `index` and draws into it, through `matrix`, the draws the sun's tile maps draw
    /// ([`Casting::draw_layer`]), at their nearest level of detail, the cut-out ones only with `cut`.
    fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        index: usize,
        matrix: Mat4,
        (meshes, buffers): (&[GpuMesh], &[MeshBuffers]),
        cut: bool,
    ) {
        let frustum = Frustum::from_view_projection(matrix);
        let mut drawn: Vec<(&GpuMesh, &GpuDraw)> = meshes
            .iter()
            .filter(|mesh| mesh.visible && mesh.anchor == Anchor::World && mesh.in_car.is_none())
            .filter(|mesh| mesh.layer != Layer::Sky && !mesh.car)
            .filter(|mesh| mesh.bounds.is_none_or(|(centre, half)| frustum.intersects_box(centre, half)))
            .flat_map(|mesh| mesh.draws.iter().map(move |draw| (mesh, draw)))
            .filter(|(_, draw)| {
                draw.lod.shows(0.0)
                    && draw.pipeline.depth_write
                    && !draw.pipeline.blend
                    && !draw.bind.water
                    && (cut || (draw.bind.alpha_ref.is_none() && !draw.bind.coverage))
            })
            .collect();
        drawn.sort_by(|(_, a), (_, b)| a.group.cmp(&b.group));
        queue.write_buffer(&self.uniforms[index], 0, bytemuck::bytes_of(&matrix.to_cols_array_2d()));
        let [sun, sky, depth] = &self.layers[index];
        let clear = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bounce surfaces"),
            color_attachments: &[clear(sun), clear(sky)],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.groups[index], &[]);
        let mut bound = Bound::new(buffers);
        let mut group = None;
        for (mesh, draw) in drawn {
            if group != Some(&draw.group) {
                pass.set_bind_group(1, &draw.group, &[]);
                group = Some(&draw.group);
            }
            bound.draw(&mut pass, mesh, draw);
        }
    }
}

//! The dials' drawing on the GPU. Each dial texture is an `RGBA8` render target with the mip levels of its base (the
//! config's `remapped` copy, kept as a texture of its own). A draw restores its rects from the base and draws the
//! sprites over them, alpha blended, in every mip level, with the sprites scaled to the level.

use super::Rgba;
use crate::stage::TextureRef;
use bytemuck::{Pod, Zeroable};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use wgpu::util::DeviceExt;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(crate) const SHADER: &str = include_str!("dials.wgsl");

/// An atlas sprite placed on the texture: its corners in texels of the texture's top level and in the atlas
/// (normalised), in the order top left, top right, bottom left, bottom right of the sprite, and its RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub corners: [[f32; 2]; 4],
    pub uvs: [[f32; 2]; 4],
    pub colour: [f32; 4],
}

/// One face's redraw: the textures it draws on, its atlas, the texel rects (x0, y0, x1, y1) restored and redrawn, and
/// all its sprites.
pub struct Draw {
    pub keys: Vec<TextureRef>,
    pub atlas: Arc<Rgba>,
    pub rects: Vec<[u32; 4]>,
    pub sprites: Vec<Sprite>,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    colour: [f32; 4],
    level: u32,
    _pad: u32,
}

/// A dial texture and the base it is restored from.
pub(crate) struct Target {
    base: wgpu::TextureView,
    texture: wgpu::Texture,
}

impl Target {
    /// The target for `base` (the remapped texture), with its size and mip levels.
    pub(crate) fn new(device: &wgpu::Device, base: wgpu::Texture) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dial"),
            size: base.size(),
            mip_level_count: base.mip_level_count(),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Target { base: base.create_view(&Default::default()), texture }
    }

    pub(crate) fn view(&self) -> wgpu::TextureView {
        self.texture.create_view(&Default::default())
    }
}

/// The pipelines, the atlases uploaded, and the bind group per target and atlas.
pub(crate) struct Pass {
    layout: wgpu::BindGroupLayout,
    restore: wgpu::RenderPipeline,
    sprite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    /// By the atlas's address, which the `Arc` kept with it holds.
    atlases: HashMap<usize, (Arc<Rgba>, wgpu::TextureView)>,
    groups: HashMap<(TextureRef, usize), wgpu::BindGroup>,
    /// The vertices, grown as needed.
    vertices: Option<wgpu::Buffer>,
}

impl Pass {
    pub(crate) fn new(device: &wgpu::Device) -> Pass {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("dials"),
            entries: &[
                texture(0),
                texture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dials"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("dials"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dials"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Uint32],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        // Straight alpha over: the colour by the sprite's alpha, and the alpha accumulated (the transparent needle
        // layers, BMW_M3_E30's `int5`, take the sprite's).
        let over = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        Pass {
            restore: pipeline("fs_restore", None),
            sprite: pipeline("fs_sprite", Some(over)),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("dials atlas"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
            atlases: HashMap::new(),
            groups: HashMap::new(),
            vertices: None,
        }
    }

    /// Records `draws` on `targets` into `encoder`.
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        targets: &BTreeMap<TextureRef, Target>,
        draws: &[Draw],
    ) {
        // Per target and level: the first vertex of its restore triangle and of its sprites.
        let mut vertices = Vec::new();
        let mut passes = Vec::new();
        for draw in draws {
            let key = Arc::as_ptr(&draw.atlas) as usize;
            let (_, atlas) = self.atlases.entry(key).or_insert_with(|| {
                let view = atlas_texture(device, queue, &draw.atlas).create_view(&Default::default());
                (Arc::clone(&draw.atlas), view)
            });
            for (target_key, target) in draw.keys.iter().filter_map(|k| Some((*k, targets.get(k)?))) {
                let group = self.groups.entry((target_key, key)).or_insert_with(|| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("dials"),
                        layout: &self.layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&target.base),
                            },
                            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(atlas) },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                        ],
                    })
                });
                let size = target.texture.size();
                for level in 0..target.texture.mip_level_count() {
                    let (w, h) = ((size.width >> level).max(1), (size.height >> level).max(1));
                    let ndc =
                        |[x, y]: [f32; 2]| [2.0 * x / size.width as f32 - 1.0, 1.0 - 2.0 * y / size.height as f32];
                    let first = vertices.len() as u32;
                    // The restore: a triangle over the whole level, cut to the rects by the scissor.
                    for position in [[-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0]] {
                        vertices.push(Vertex { position, uv: [0.0; 2], colour: [0.0; 4], level, _pad: 0 });
                    }
                    for sprite in &draw.sprites {
                        for corner in [0, 1, 2, 2, 1, 3] {
                            vertices.push(Vertex {
                                position: ndc(sprite.corners[corner]),
                                uv: sprite.uvs[corner],
                                colour: sprite.colour,
                                level,
                                _pad: 0,
                            });
                        }
                    }
                    let scissors: Vec<[u32; 4]> = (draw.rects.iter())
                        .map(|&[x0, y0, x1, y1]| {
                            let up = |v: u32, max: u32| v.div_ceil(1 << level).min(max);
                            [x0 >> level, y0 >> level, up(x1, w), up(y1, h)]
                        })
                        .filter(|[x0, y0, x1, y1]| x0 < x1 && y0 < y1)
                        .collect();
                    passes.push((target, level, group.clone(), first, draw.sprites.len() as u32 * 6, scissors));
                }
            }
        }
        if vertices.is_empty() {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&vertices);
        if self.vertices.as_ref().is_none_or(|b| b.size() < bytes.len() as u64) {
            self.vertices = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dials"),
                size: (bytes.len() as u64).next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let buffer = self.vertices.as_ref().expect("made above");
        queue.write_buffer(buffer, 0, bytes);
        for (target, level, group, first, sprites, scissors) in passes {
            if scissors.is_empty() {
                continue;
            }
            let view = target.texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dials"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            for [x0, y0, x1, y1] in scissors {
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.set_pipeline(&self.restore);
                pass.draw(first..first + 3, 0..1);
                pass.set_pipeline(&self.sprite);
                pass.draw(first + 3..first + 3 + sprites, 0..1);
            }
        }
    }
}

/// The atlas as an `RGBA8` texture.
fn atlas_texture(device: &wgpu::Device, queue: &wgpu::Queue, atlas: &Rgba) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("dials atlas"),
            size: wgpu::Extent3d { width: atlas.width, height: atlas.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &atlas.pixels,
    )
}

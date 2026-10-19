//! The particle pass: the sprites in one texture array, the water waves as triangles, and the billboards as instanced
//! quads, drawn after the stage and the car, depth tested against them without writing depth. The waves come first,
//! then the billboards in the order given (back to front, sorted by [`super::sim::Simulation::billboards`]). Blending
//! is premultiplied: alpha-blended sprites cover what is behind, additive ones (`BLEND_ADD`) add to it. Lit by the
//! stage's ambient and diffuse light and hazed by its fog (ours).

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use rbr_formats::dds;
use wgpu::util::DeviceExt;

use super::sim::Billboard;
use super::wave::WaveVertex;
use crate::atmosphere::Atmosphere;

/// Every sprite is resampled to this square, the size most of them have.
pub const SPRITE_SIZE: u32 = 64;
/// The particles' shader.
pub(crate) fn source() -> String {
    format!("{}\n{}", include_str!("particles.wgsl"), crate::effects::HEADER)
}

const MIPS: u32 = 7;

/// Decoded sprites: RGBA layers of [`SPRITE_SIZE`] and the layer of each sprite id.
pub struct Sprites {
    layers: Vec<Vec<u8>>,
    of_sprite: HashMap<u32, u32>,
}

impl Sprites {
    /// The sprites of `files` (by sprite id: each one's DDS bytes, `None` when missing or undecodable).
    pub fn decode(files: &[Option<Vec<u8>>]) -> Sprites {
        let mut layers = Vec::new();
        let mut of_sprite = HashMap::new();
        for (id, bytes) in files.iter().enumerate() {
            let Some(rgba) = bytes.as_deref().and_then(decode_dds) else { continue };
            of_sprite.insert(id as u32, layers.len() as u32);
            layers.push(rgba);
        }
        Sprites { layers, of_sprite }
    }

    /// Adds a layer for sprite `id` from a DDS file's colour and an 8-bit grey TGA's alpha (`width` by `height`,
    /// rows bottom up; the water wave's `SplashColor.dds` and `splashalpha.tga`). Nothing when either does not decode.
    pub fn add(&mut self, id: u32, dds: &[u8], tga: &[u8]) {
        let Some(colour) = dds::parse(dds).ok().and_then(|file| {
            let level = *file.levels().first()?;
            let rgba = decode_blocks(level.width, level.height, file.format, level.data)?;
            Some(resample(&rgba, level.width, level.height, SPRITE_SIZE))
        }) else {
            return;
        };
        let Some((grey, width, height)) = grey_tga(tga) else { return };
        let mut alpha = Vec::with_capacity(grey.len() * 4);
        for &g in &grey {
            alpha.extend_from_slice(&[255, 255, 255, g]);
        }
        let alpha = resample(&alpha, width, height, SPRITE_SIZE);
        let mut layer = colour;
        for (texel, a) in layer.chunks_exact_mut(4).zip(alpha.chunks_exact(4)) {
            texel[3] = a[3];
        }
        self.of_sprite.insert(id, self.layers.len() as u32);
        self.layers.push(layer);
    }

    /// The texture layer of sprite `id`.
    pub fn layer(&self, id: u32) -> Option<u32> {
        self.of_sprite.get(&id).copied()
    }

    /// Layer `i`'s RGBA texels, [`SPRITE_SIZE`] square.
    pub fn rgba(&self, i: usize) -> Option<&[u8]> {
        self.layers.get(i).map(Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// An uncompressed 8-bit grey TGA (type 3): its texels top row first, width and height.
fn grey_tga(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let header = bytes.get(..18)?;
    if header[2] != 3 || header[16] != 8 {
        return None;
    }
    let (width, height) = (u16::from_le_bytes([header[12], header[13]]), u16::from_le_bytes([header[14], header[15]]));
    let start = 18 + usize::from(header[0]);
    let texels = bytes.get(start..start + usize::from(width) * usize::from(height))?;
    let mut rows: Vec<&[u8]> = texels.chunks_exact(usize::from(width)).collect();
    if header[17] & 0x20 == 0 {
        rows.reverse();
    }
    Some((rows.concat(), u32::from(width), u32::from(height)))
}

/// A DXT1 or DXT3 file's largest level, decoded and resampled to [`SPRITE_SIZE`] square RGBA.
fn decode_dds(bytes: &[u8]) -> Option<Vec<u8>> {
    let file = dds::parse(bytes).ok()?;
    let level = *file.levels().first()?;
    let rgba = decode_blocks(level.width, level.height, file.format, level.data)?;
    Some(resample(&rgba, level.width, level.height, SPRITE_SIZE))
}

/// Decodes BC1 or BC2 blocks into RGBA rows (BC3 through `texpresso`; plain RGBA as it is).
fn decode_blocks(width: u32, height: u32, format: dds::Format, data: &[u8]) -> Option<Vec<u8>> {
    let (w, h) = (width as usize, height as usize);
    if format == dds::Format::Rgba8 {
        return Some(data.to_vec());
    }
    if format == dds::Format::Dxt5 {
        let mut rgba = vec![0u8; w * h * 4];
        texpresso::Format::Bc3.decompress(data, w, h, &mut rgba);
        return Some(rgba);
    }
    let blocks_x = w.div_ceil(4).max(1);
    let block_len = format.block_len();
    let mut out = vec![0u8; w * h * 4];
    for (b, block) in data.chunks_exact(block_len).enumerate() {
        let (bx, by) = (b % blocks_x, b / blocks_x);
        if by * 4 >= h.max(1) {
            break;
        }
        let (alpha, colour) = match format {
            dds::Format::Dxt3 => (Some(&block[..8]), &block[8..]),
            dds::Format::Dxt1 | dds::Format::Dxt5 | dds::Format::Rgba8 => (None, block),
        };
        let c0 = u16::from_le_bytes([colour[0], colour[1]]);
        let c1 = u16::from_le_bytes([colour[2], colour[3]]);
        let rgb = |c: u16| {
            let r = ((c >> 11) & 31) as f32 * 255.0 / 31.0;
            let g = ((c >> 5) & 63) as f32 * 255.0 / 63.0;
            let b = (c & 31) as f32 * 255.0 / 31.0;
            [r, g, b]
        };
        let (a, z) = (rgb(c0), rgb(c1));
        let mix = |p: f32| -> [f32; 3] { std::array::from_fn(|i| a[i] + (z[i] - a[i]) * p) };
        let four = c0 > c1 || alpha.is_some();
        let palette: [[f32; 4]; 4] = if four {
            let [m, n] = [mix(1.0 / 3.0), mix(2.0 / 3.0)];
            [[a[0], a[1], a[2], 255.0], [z[0], z[1], z[2], 255.0], [m[0], m[1], m[2], 255.0], [n[0], n[1], n[2], 255.0]]
        } else {
            let m = mix(0.5);
            [[a[0], a[1], a[2], 255.0], [z[0], z[1], z[2], 255.0], [m[0], m[1], m[2], 255.0], [0.0; 4]]
        };
        let bits = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]);
        for py in 0..4 {
            for px in 0..4 {
                let (x, y) = (bx * 4 + px, by * 4 + py);
                if x >= w || y >= h {
                    continue;
                }
                let i = py * 4 + px;
                let mut texel = palette[((bits >> (2 * i)) & 3) as usize];
                if let Some(alpha) = alpha {
                    let nibble = (alpha[i / 2] >> (4 * (i % 2))) & 0xf;
                    texel[3] = f32::from(nibble) * 17.0;
                }
                let at = (y * w + x) * 4;
                for k in 0..4 {
                    out[at + k] = texel[k].round() as u8;
                }
            }
        }
    }
    Some(out)
}

/// Bilinear resampling of RGBA rows to a square of `size`, with alpha-weighted colour so that the transparent
/// texels' colour does not bleed into the edges.
fn resample(rgba: &[u8], width: u32, height: u32, size: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let texel = |x: usize, y: usize| {
        let at = (y.min(h - 1) * w + x.min(w - 1)) * 4;
        let a = f32::from(rgba[at + 3]) / 255.0;
        [f32::from(rgba[at]) * a, f32::from(rgba[at + 1]) * a, f32::from(rgba[at + 2]) * a, a * 255.0]
    };
    // Box-average when shrinking: sample the source over the target texel's footprint.
    let taps = (w / size as usize).max(1);
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size as usize {
        for x in 0..size as usize {
            let mut sum = [0.0f32; 4];
            for ty in 0..taps {
                for tx in 0..taps {
                    let u = ((x * taps + tx) as f32 + 0.5) / (size as usize * taps) as f32 * w as f32 - 0.5;
                    let v = ((y * taps + ty) as f32 + 0.5) / (size as usize * taps) as f32 * h as f32 - 0.5;
                    let (u0, v0) = (u.floor().max(0.0), v.floor().max(0.0));
                    let (fu, fv) = ((u - u0).clamp(0.0, 1.0), (v - v0).clamp(0.0, 1.0));
                    let (u0, v0) = (u0 as usize, v0 as usize);
                    let corners = [texel(u0, v0), texel(u0 + 1, v0), texel(u0, v0 + 1), texel(u0 + 1, v0 + 1)];
                    for k in 0..4 {
                        let top = corners[0][k] + (corners[1][k] - corners[0][k]) * fu;
                        let bottom = corners[2][k] + (corners[3][k] - corners[2][k]) * fu;
                        sum[k] += top + (bottom - top) * fv;
                    }
                }
            }
            let n = (taps * taps) as f32;
            let a = sum[3] / n;
            let at = (y * size as usize + x) * 4;
            let unweight = if a > 0.0 { 255.0 / a } else { 0.0 };
            for k in 0..3 {
                out[at + k] = (sum[k] / n * unweight).round().clamp(0.0, 255.0) as u8;
            }
            out[at + 3] = a.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// The next mip level of a square RGBA layer (2x2 box, alpha-weighted colour).
fn halve(rgba: &[u8], size: usize) -> Vec<u8> {
    let half = (size / 2).max(1);
    let mut out = vec![0u8; half * half * 4];
    for y in 0..half {
        for x in 0..half {
            let mut sum = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let at = (((2 * y + dy).min(size - 1)) * size + (2 * x + dx).min(size - 1)) * 4;
                let a = f32::from(rgba[at + 3]);
                for k in 0..3 {
                    sum[k] += f32::from(rgba[at + k]) * a;
                }
                sum[3] += a;
            }
            let at = (y * half + x) * 4;
            for k in 0..3 {
                out[at + k] = if sum[3] > 0.0 { (sum[k] / sum[3]).round() as u8 } else { 0 };
            }
            out[at + 3] = (sum[3] / 4.0).round() as u8;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct FrameUniform {
    view_projection: [[f32; 4]; 4],
    eye: [f32; 4],
    fog_color: [f32; 4],
    fog_range: [f32; 4],
    light: [f32; 4],
}

/// The [`Billboard`] fields, by offset.
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 16, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 32, shader_location: 4 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 44, shader_location: 5 },
];

/// The [`WaveVertex`] fields, by offset.
const WAVE_ATTRIBUTES: [wgpu::VertexAttribute; 5] = [
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 20, shader_location: 2 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 28, shader_location: 3 },
    wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 44, shader_location: 4 },
];

/// Ours: a particle takes the stage's ambient light and this much of its diffuse (sprites have no normal).
const DIFFUSE_SHARE: f32 = 0.2;
/// Instances the buffer starts with; it grows to the largest frame.
const FIRST_CAPACITY: usize = 4096;

/// Writes `items` into `buffer`, which is replaced by a larger one (of `capacity` items) when they do not fit.
fn upload<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    capacity: &mut usize,
    items: &[T],
) {
    if items.len() > *capacity {
        *capacity = items.len().next_power_of_two();
        let mut all = items.to_vec();
        all.resize(*capacity, T::zeroed());
        *buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("particle buffer"),
            contents: bytemuck::cast_slice(&all),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
    } else if !items.is_empty() {
        queue.write_buffer(buffer, 0, bytemuck::cast_slice(items));
    }
}

/// The particle pipelines, their sprites and the frame's billboards and waves.
pub struct Pass {
    pipeline: wgpu::RenderPipeline,
    /// Lit and soft (`fs_soft`), with the effects at group 1 (`Enhance::particles`); `None` without them.
    soft: Option<wgpu::RenderPipeline>,
    wave_pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
    waves: wgpu::Buffer,
    wave_capacity: usize,
    wave_count: u32,
}

impl Pass {
    /// The pass for colour targets of `format` with a `depth` attachment, and with
    /// `effects` (their bind group layout) the lit and soft pipeline too.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
        sprites: &Sprites,
        effects: Option<&wgpu::BindGroupLayout>,
    ) -> Pass {
        let layers = sprites.layers.len().max(1) as u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("particle sprites"),
            size: wgpu::Extent3d { width: SPRITE_SIZE, height: SPRITE_SIZE, depth_or_array_layers: layers },
            mip_level_count: MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let blank = vec![0u8; (SPRITE_SIZE * SPRITE_SIZE * 4) as usize];
        for layer in 0..layers {
            let mut rgba = sprites.layers.get(layer as usize).unwrap_or(&blank).clone();
            let mut size = SPRITE_SIZE as usize;
            for mip in 0..MIPS {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: layer },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size as u32 * 4),
                        rows_per_image: Some(size as u32),
                    },
                    wgpu::Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
                );
                rgba = halve(&rgba, size);
                size = (size / 2).max(1);
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle sprites"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particles"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particles"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("particles"),
            source: wgpu::ShaderSource::Wgsl(source().into()),
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let build = |layouts: &[&wgpu::BindGroupLayout],
                     vertex: (&str, wgpu::VertexBufferLayout),
                     fragment: &str,
                     topology: wgpu::PrimitiveTopology| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("particles"),
                layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("particles"),
                    bind_group_layouts: layouts,
                    push_constant_ranges: &[],
                })),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex.0),
                    compilation_options: Default::default(),
                    buffers: &[vertex.1],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: crate::post::scene_options(format),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState { topology, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: crate::renderer::SAMPLE_COUNT, ..Default::default() },
                multiview: None,
                cache: None,
            })
        };
        let instance_layout = || wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Billboard>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        };
        let soft = effects.map(|effects| {
            build(
                &[&layout, effects],
                ("vs_main", instance_layout()),
                "fs_soft",
                wgpu::PrimitiveTopology::TriangleStrip,
            )
        });
        let pipeline =
            build(&[&layout], ("vs_main", instance_layout()), "fs_main", wgpu::PrimitiveTopology::TriangleStrip);
        let wave_pipeline = build(
            &[&layout],
            (
                "vs_wave",
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WaveVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &WAVE_ATTRIBUTES,
                },
            ),
            "fs_wave",
            wgpu::PrimitiveTopology::TriangleList,
        );
        let buffer = |label: &str, size: usize| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let instances = buffer("particle instances", FIRST_CAPACITY * std::mem::size_of::<Billboard>());
        let waves = buffer("particle waves", FIRST_CAPACITY * std::mem::size_of::<WaveVertex>());
        Pass {
            pipeline,
            soft,
            wave_pipeline,
            group,
            uniform,
            instances,
            capacity: FIRST_CAPACITY,
            count: 0,
            waves,
            wave_capacity: FIRST_CAPACITY,
            wave_count: 0,
        }
    }

    /// Whether there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.count == 0 && self.wave_count == 0
    }

    /// The billboards to draw, back to front.
    pub fn set(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, billboards: &[Billboard]) {
        upload(device, queue, &mut self.instances, &mut self.capacity, billboards);
        self.count = billboards.len() as u32;
    }

    /// The water waves' triangles to draw first.
    pub fn set_waves(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, waves: &[WaveVertex]) {
        upload(device, queue, &mut self.waves, &mut self.wave_capacity, waves);
        self.wave_count = waves.len() as u32;
    }

    /// The frame's camera and light.
    pub fn prepare(&self, queue: &wgpu::Queue, view_projection: Mat4, atmosphere: &Atmosphere) {
        let light = (atmosphere.ambient + atmosphere.diffuse * DIFFUSE_SHARE).min(Vec3::splat(1.2));
        let uniform = FrameUniform {
            view_projection: view_projection.to_cols_array_2d(),
            eye: crate::camera::eye(view_projection).extend(1.0).to_array(),
            fog_color: atmosphere.fog_color.extend(atmosphere.haze()).to_array(),
            fog_range: [atmosphere.fog_start, atmosphere.fog_end, 0.0, 0.0],
            light: light.extend(1.0).to_array(),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    /// Draws the waves, then the billboards, into `pass`, whose targets match the ones [`Pass::new`] was made for.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.draw_waves(pass);
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..self.count);
    }

    /// Draws the waves and the billboards lit and soft with the effects' bind group `effects`, in a pass that reads
    /// the depth (as [`Pass::draw`] without the soft pipeline).
    pub fn draw_soft(&self, pass: &mut wgpu::RenderPass<'_>, effects: &wgpu::BindGroup) {
        let Some(soft) = &self.soft else { return self.draw(pass) };
        self.draw_waves(pass);
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(soft);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_bind_group(1, effects, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..4, 0..self.count);
    }

    fn draw_waves(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.wave_count == 0 {
            return;
        }
        pass.set_pipeline(&self.wave_pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_vertex_buffer(0, self.waves.slice(..));
        pass.draw(0..self.wave_count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt3_blocks_decode_alpha_and_endpoints() {
        // One 4x4 block: alpha nibbles 0..15, colour 0 white, colour 1 black, every index 0 (white).
        let mut block = Vec::new();
        block.extend((0..8u8).map(|i| (2 * i) | ((2 * i + 1) << 4)));
        block.extend([0xff, 0xff, 0x00, 0x00, 0, 0, 0, 0]);
        let rgba = decode_blocks(4, 4, dds::Format::Dxt3, &block).unwrap();
        assert_eq!(&rgba[..4], &[255, 255, 255, 0]);
        assert_eq!(&rgba[60..], &[255, 255, 255, 255]);
    }

    #[test]
    fn resampling_keeps_a_flat_sprite_flat() {
        let rgba: Vec<u8> = (0..16 * 16).flat_map(|_| [200, 100, 50, 128]).collect();
        let out = resample(&rgba, 16, 16, 64);
        assert!(out.chunks_exact(4).all(|p| p == [200, 100, 50, 128]));
        let small = halve(&out, 64);
        assert_eq!(small.len(), 32 * 32 * 4);
        assert!(small.chunks_exact(4).all(|p| p == [200, 100, 50, 128]));
    }
}

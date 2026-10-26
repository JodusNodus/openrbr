//! The draws' textures, samplers and bind groups, and the colour and depth targets.

use super::pipelines::cutout;
use super::uniforms::ModeUniform;
use super::DEPTH_FORMAT;
use crate::stage::{Address, DrawState, Shade, TextureRef, TextureSet, MIP_MAP_BIAS};
use bytemuck::bytes_of;
use rbr_formats::dds::{self, Format};
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;

/// Texel used for a draw with no texture (vertex colour only).
pub(super) const WHITE: [u8; 4] = [255, 255, 255, 255];
/// Texel used for a draw whose texture is missing. Magenta stands out.
pub(super) const MISSING: [u8; 4] = [255, 0, 255, 255];

/// Which view a texture slot samples: a loaded texture, the white placeholder (no texture), or the
/// magenta placeholder (missing texture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Source {
    White,
    Missing,
    Texture(TextureRef),
}

/// The texture bindings and shader mode of a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct BindKey {
    textures: [Source; 2],
    /// The shadow light map (`White` when the draw has none or it is missing: no shadow).
    shadow: Source,
    address: [Address; 2],
    second_texture: bool,
    pub(super) alpha_ref: Option<u8>,
    mip_bias: bool,
    pub(super) cutout: bool,
    pub(super) shade: Option<Shade>,
    /// The water stand-in's draws, which the shader draws as water.
    pub(super) water: bool,
    /// The specular map (`White` when the draw has none or it is missing: no highlight), or a lit draw's dirt map
    /// ([`Shade::dirt`]) in the same binding.
    pub(super) specular: Source,
    /// The first texture is a dial's with an emissive mask ([`Views::glows`]).
    glow: bool,
}

impl BindKey {
    /// What the draw's mode uniform says it is ([`ModeUniform`]): the water stand-in, a draw with a specular map (the
    /// roads' shine and wet film), a second texture, the near ground's mip bias (its fine detail).
    pub(super) fn features(&self) -> DrawFeatures {
        DrawFeatures {
            water: self.water,
            specular: self.specular != Source::White && self.shade.is_none(),
            second_texture: self.second_texture,
            detail: self.mip_bias,
        }
    }
}

/// The parts of the stage shader a draw runs, by its mode ([`BindKey::features`]; `KNOWN` and `DRAW_*` in
/// `shader.wgsl`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct DrawFeatures {
    pub(super) water: bool,
    pub(super) specular: bool,
    pub(super) second_texture: bool,
    pub(super) detail: bool,
}

/// The nine samplers: one per (U mode, V mode) pair.
pub(super) struct Samplers([[wgpu::Sampler; 3]; 3]);

impl Samplers {
    pub(super) fn new(device: &wgpu::Device, anisotropy: u16) -> Samplers {
        let sampler = |u: Address, v: Address| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("stage"),
                address_mode_u: address_mode(u),
                address_mode_v: address_mode(v),
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: anisotropy,
                ..Default::default()
            })
        };
        let row = |u| [sampler(u, Address::Wrap), sampler(u, Address::Mirror), sampler(u, Address::Clamp)];
        Samplers([row(Address::Wrap), row(Address::Mirror), row(Address::Clamp)])
    }

    fn get(&self, u: Address, v: Address) -> &wgpu::Sampler {
        &self.0[address_index(u)][address_index(v)]
    }
}

fn address_index(address: Address) -> usize {
    match address {
        Address::Wrap => 0,
        Address::Mirror => 1,
        Address::Clamp => 2,
    }
}

fn address_mode(address: Address) -> wgpu::AddressMode {
    match address {
        Address::Wrap => wgpu::AddressMode::Repeat,
        Address::Mirror => wgpu::AddressMode::MirrorRepeat,
        Address::Clamp => wgpu::AddressMode::ClampToEdge,
    }
}

/// The texture bindings of a draw. A texture that did not load is the missing placeholder.
pub(super) fn bind_key(state: &DrawState, views: &Views) -> BindKey {
    let loaded = &views.loaded;
    let source = |texture: Option<TextureRef>| match texture {
        None => Source::White,
        Some(texture) if loaded.contains_key(&texture) => Source::Texture(texture),
        Some(_) => Source::Missing,
    };
    // A missing light map means no shadow, not the magenta placeholder.
    let shadow = match state.shadow {
        Some(texture) if loaded.contains_key(&texture) => Source::Texture(texture),
        _ => Source::White,
    };
    BindKey {
        textures: [source(state.textures[0]), source(state.textures[1])],
        shadow,
        address: state.address,
        second_texture: state.second_texture,
        alpha_ref: state.alpha_ref,
        mip_bias: state.mip_bias,
        cutout: cutout(state),
        shade: state.shade,
        water: state.textures[0].is_some_and(|t| t.set == TextureSet::WaterStandIn),
        specular: match state.specular.or(state.shade.and_then(|s| s.dirt)) {
            Some(texture) if loaded.contains_key(&texture) => Source::Texture(texture),
            _ => Source::White,
        },
        glow: state.textures[0].is_some_and(|t| views.glows.contains_key(&t)),
    }
}

/// The texture views a draw can sample: the loaded stage textures and the two placeholders.
pub(super) struct Views {
    pub(super) loaded: BTreeMap<TextureRef, wgpu::TextureView>,
    pub(super) white: wgpu::TextureView,
    pub(super) missing: wgpu::TextureView,
    /// The dials' emissive masks, by the dial texture they light (`Image::Dial`), and whether their alpha counts.
    pub(super) glows: BTreeMap<TextureRef, (wgpu::TextureView, bool)>,
}

impl Views {
    fn get(&self, source: Source) -> &wgpu::TextureView {
        match source {
            Source::White => &self.white,
            Source::Missing => &self.missing,
            Source::Texture(texture) => self.loaded.get(&texture).unwrap_or(&self.missing),
        }
    }
}

/// A draw's lit class (`mode.lit`, `LIT` in `shader.wgsl`): 0 unlit, 1 lit, 2 glass, 3 a lit lamp's part, 4 a crack.
pub(super) fn lit_class(shade: Option<&Shade>) -> u32 {
    shade.map_or(0, |s| match (s.glow, s.crack) {
        (true, _) => 3,
        (_, true) => 4,
        _ => 1 + u32::from(s.glass),
    })
}

/// A bind group for one set of texture bindings and shader mode.
pub(super) fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    key: BindKey,
    views: &Views,
    samplers: &Samplers,
) -> wgpu::BindGroup {
    let glow = match key.textures[0] {
        Source::Texture(texture) if key.glow => views.glows.get(&texture),
        _ => None,
    };
    let features = key.features();
    let mode = ModeUniform {
        second_texture: u32::from(features.second_texture),
        alpha_test: u32::from(key.alpha_ref.is_some()),
        alpha_ref: key.alpha_ref.map_or(0.0, |r| f32::from(r) / 255.0),
        mip_bias: if key.mip_bias { MIP_MAP_BIAS } else { 0.0 },
        cutout: u32::from(key.cutout),
        lit: lit_class(key.shade.as_ref()),
        water: u32::from(features.water),
        specular: u32::from(features.specular),
        light: key.shade.map_or([0.0; 4], |s| [s.ka, s.kd, s.ks, s.m]),
        // `w` 1: the dirt map is in the specular binding.
        fresnel: key.shade.map_or([0.0; 4], |s| {
            [s.fresnel_min, s.fresnel_max, s.kf, f32::from(u8::from(key.specular != Source::White))]
        }),
        glow: [f32::from(u8::from(glow.is_some())), f32::from(u8::from(glow.is_some_and(|(_, a)| *a))), 0.0, 0.0],
        grime: key.shade.map_or([0.0; 4], |s| [f32::from(s.grime) / 10.0, f32::from(u8::from(s.tint)), 0.0, 0.0]),
    };
    let glow = glow.map_or(&views.white, |(view, _)| view);
    let mode_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("draw mode"),
        contents: bytes_of(&mode),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("draw textures"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(views.get(key.textures[0])),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(views.get(key.textures[1])),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(samplers.get(key.address[0], key.address[1])),
            },
            wgpu::BindGroupEntry { binding: 3, resource: mode_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(views.get(key.shadow)) },
            wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(views.get(key.specular)) },
            wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(glow) },
        ],
    })
}

/// A 1x1 texture of one colour.
pub(super) fn solid_texture(device: &wgpu::Device, queue: &wgpu::Queue, texel: [u8; 4]) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("solid"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &texel,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    texture
}

/// A single-channel light map (`R8Unorm`): one level of `width * height` texels.
pub(super) fn luminance_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width), rows_per_image: Some(height) },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    texture
}

/// A texture with its whole mip chain, uploaded level by level. A block-compressed texture whose size is not whole
/// blocks (a 2x2 one on a community stage), which the GPU cannot hold compressed, is decoded to one RGBA level.
pub(crate) fn dds_texture(device: &wgpu::Device, queue: &wgpu::Queue, dds: &dds::Dds) -> wgpu::Texture {
    let edge = dds.format.block_edge();
    if !dds.width.is_multiple_of(edge) || !dds.height.is_multiple_of(edge) {
        let (width, height) = (dds.width as usize, dds.height as usize);
        let bc = match dds.format {
            Format::Dxt1 => texpresso::Format::Bc1,
            Format::Dxt3 => texpresso::Format::Bc2,
            Format::Dxt5 => texpresso::Format::Bc3,
            Format::Rgba8 => unreachable!("plain texels are whole blocks"),
        };
        let mut rgba = vec![0u8; width * height * 4];
        bc.decompress(dds.levels()[0].data, width, height, &mut rgba);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dds rgba"),
            size: wgpu::Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dds.width),
                rows_per_image: Some(dds.height),
            },
            wgpu::Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 },
        );
        return texture;
    }
    let format = match dds.format {
        Format::Dxt1 => wgpu::TextureFormat::Bc1RgbaUnorm,
        Format::Dxt3 => wgpu::TextureFormat::Bc2RgbaUnorm,
        Format::Dxt5 => wgpu::TextureFormat::Bc3RgbaUnorm,
        Format::Rgba8 => wgpu::TextureFormat::Rgba8Unorm,
    };
    let levels = dds.levels();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dds"),
        size: wgpu::Extent3d { width: dds.width, height: dds.height, depth_or_array_layers: 1 },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip_level, level) in levels.iter().enumerate() {
        // Copies cover whole blocks: a level under 4 texels is still one block across.
        let blocks_x = level.width.div_ceil(edge);
        let blocks_y = level.height.div_ceil(edge);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            level.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(blocks_x * dds.format.block_len() as u32),
                rows_per_image: Some(blocks_y),
            },
            // Block formats copy whole blocks: a mip smaller than 4 texels still takes one block.
            wgpu::Extent3d { width: blocks_x * edge, height: blocks_y * edge, depth_or_array_layers: 1 },
        );
    }
    texture
}

pub(super) fn depth_view(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            // Read by the reflections, the sun's mask, the effects and the post pass.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

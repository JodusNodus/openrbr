//! The stage shader and the render pipelines of the stage draws, the sky and the car's shadow map.

use super::enhance::Enhance;
use super::sun_mask::NORMALS;
use super::textures::{lit_class, DrawFeatures};
use super::DEPTH_FORMAT;
use crate::coords::FRONT_FACE;
use crate::stage::{DrawState, Vertex};

pub(crate) const SHADER: &str = concat!(
    include_str!("../shader.wgsl"),
    include_str!("../lamps.wgsl"),
    include_str!("../night.wgsl"),
    include_str!("../clouds.wgsl"),
    include_str!("../water.wgsl"),
    include_str!("../cabin.wgsl"),
    include_str!("../sky_lut.wgsl"),
    include_str!("../mirrors.wgsl"),
    include_str!("../sun_mask.wgsl"),
    include_str!("../cloud_volume.wgsl")
);

/// The light flares' blend (`DrawState::additive`): the colour times the texel's alpha, added to the frame.
const ADDITIVE_BLENDING: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::SrcAlpha,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// The pipeline state of a draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PipelineKey {
    cull_back: bool,
    pub(super) blend: bool,
    additive: bool,
    depth_test: bool,
    pub(super) depth_write: bool,
    /// The alpha test (`ALPHA_TEST` in `shader.wgsl`); without it the shader is built with no `discard`.
    alpha_test: bool,
    /// A mirror's glass is among its faces (`MIRRORS` in `mirrors.wgsl`): only those draws carry the mirrors' code,
    /// which slows the rest of the stage's shading by a millisecond at 1440p on an M-series Mac.
    mirror: bool,
    /// The draw's lit class ([`lit_class`], `LIT` in `shader.wgsl`), 0 for the stage's: each pipeline carries only its
    /// class's code, and only lit draws (the car's) the cabin light's (`CABIN` in `cabin.wgsl`). The shader's code for
    /// the classes a pipeline never draws still held registers on an M1 Pro, where it slowed every pixel.
    lit: u32,
    /// For a lit draw, whether the frame's view is in the car (`INSIDE` in `shader.wgsl`): the cockpit's pipelines
    /// carry no outside view's light, the others no cabin's. `None` decides at run time (`frame.reflection.y`).
    inside: Option<bool>,
    /// The draw's features (`KNOWN` in `shader.wgsl`): each pipeline carries only the code of the ones it draws,
    /// which its mode uniform says as well. `None` (the prepass) decides at run time.
    features: Option<DrawFeatures>,
    /// For a lit draw, whether its faces are all the cabin's interior set (`CABIN_SET` in `shader.wgsl`), or none are:
    /// the interior's pipelines carry no fog, the others no cabin light. `None` decides per fragment.
    cabin_set: Option<bool>,
    /// The draw's frame uniform ([`frame_index`](super::uniforms::frame_index), `FRAME_KIND` in `shader.wgsl`): the
    /// stage's, the clouds' or the sky faces'. `None` (the prepass) decides at run time.
    frame: Option<usize>,
    /// A sky draw, drawn behind the rest at depth 0 where the depth is still the clear one (`Renderer::draw_scene`).
    sky: bool,
    pass: DrawPass,
    /// The car's lamps light the stage's draws (`LAMPS` in `shader.wgsl`): only the frames where they are lit take the
    /// pipelines with the lamps' code, which slows the rest of the stage's shading by a millisecond at 1080p on an
    /// M-series Mac.
    lamps: bool,
}

/// Where a pipeline draws in the main view's pass. The alpha-tested opaque draws ([`prepassed`]) first lay their depth
/// with a cheap shader that only cuts the texture's alpha out, then shade where their depth is still the one laid: so
/// only the nearest of the layers of foliage over a pixel runs the full shader. The frame is the same as drawing them
/// once in order (to the pixel on 66_E and 33_N).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum DrawPass {
    /// Drawn once, in order (the reflection cube draws every draw so).
    Main,
    /// The depth only, before every other draw: `fs_prepass`, no colour written.
    DepthPrepass,
    /// The full shader where the depth equals the prepass's, writing no depth. The alpha test, since a cut-out texel
    /// lying at the depth laid by another draw (coincident cards) passes Equal too: without its `discard`, which of
    /// the tied fragments a pixel kept changed from run to run on Metal. Both pipelines give the same depth through
    /// `vs_main`'s `@invariant` position.
    AfterPrepass,
}

impl PipelineKey {
    /// A draw's, with `mirror` when it holds a mirror's glass and `sky` for the sky's.
    pub(super) fn of(state: &DrawState, mirror: bool, sky: bool) -> PipelineKey {
        PipelineKey {
            cull_back: state.cull_back,
            blend: state.blend,
            additive: state.additive,
            depth_test: state.depth_test,
            depth_write: state.depth_write,
            alpha_test: state.alpha_ref.is_some(),
            mirror,
            lit: lit_class(state.shade.as_ref()),
            inside: None,
            features: None,
            cabin_set: None,
            frame: None,
            sky,
            pass: DrawPass::Main,
            lamps: false,
        }
    }

    pub(super) fn in_pass(self, pass: DrawPass) -> PipelineKey {
        // The prepass only cuts the texture's alpha out.
        let laying = pass == DrawPass::DepthPrepass;
        let (features, frame) = (self.features.filter(|_| !laying), self.frame.filter(|_| !laying));
        PipelineKey { pass, features, frame, ..self }
    }

    /// The lit draw's pipeline for faces all of the cabin's interior set or none of it ([`PipelineKey::cabin_set`]).
    pub(super) fn in_cabin(self, cabin_set: Option<bool>) -> PipelineKey {
        PipelineKey { cabin_set: cabin_set.filter(|_| self.lit != 0), ..self }
    }

    /// The pipeline for a draw with frame uniform `frame`.
    pub(super) fn framed(self, frame: usize) -> PipelineKey {
        PipelineKey { frame: Some(frame), ..self }
    }

    /// The pipeline for a draw with `features`.
    pub(super) fn drawing(self, features: DrawFeatures) -> PipelineKey {
        PipelineKey { features: Some(features), ..self }
    }

    /// The lit draw's pipeline for a view in the car or outside it.
    pub(super) fn viewed(self, inside: bool) -> PipelineKey {
        PipelineKey { inside: Some(inside), ..self }
    }

    /// Whether the draw has a pipeline per view ([`PipelineKey::inside`]): the lit draws.
    pub(super) fn takes_views(&self) -> bool {
        self.lit != 0
    }

    /// The pipeline with the lamps' code.
    pub(super) fn with_lamps(self) -> PipelineKey {
        PipelineKey { lamps: true, ..self }
    }

    /// Whether the draw has a pipeline with the lamps' code ([`PipelineKey::lamps`]): the stage's unlit draws.
    pub(super) fn takes_lamps(&self) -> bool {
        self.lit == 0 && !self.sky
    }
}

/// An opaque alpha-tested stage draw that writes depth: the main view lays its depth first ([`DrawPass`]); with the
/// sun's mask (`sun_mask`) every opaque one, whose depth the mask is worked out from. The lit draws (the car's) are
/// left out, since their alpha is not the texture's alone.
pub(super) fn prepassed(state: &DrawState, sun_mask: bool) -> bool {
    (cutout(state) || (sun_mask && !state.blend)) && state.depth_test && state.depth_write && state.shade.is_none()
}

/// An opaque alpha-tested draw (`cutout` in `shader.wgsl`): the object lanes' foliage, fences and spectators.
pub(super) fn cutout(state: &DrawState) -> bool {
    !state.blend && state.alpha_ref.is_some()
}

/// What the stage's pipelines are built from: the shader, its layout and the colour format drawn in.
pub(super) struct Context<'a> {
    pub(super) device: &'a wgpu::Device,
    pub(super) format: wgpu::TextureFormat,
    pub(super) shader: &'a wgpu::ShaderModule,
    pub(super) layout: &'a wgpu::PipelineLayout,
    /// The stage draws' vertex entry point ([`vertex_entry`]).
    pub(super) vertex: &'static str,
    /// [`Enhance::water`] or [`Enhance::wet`] (`WATER_PLUS` in `shader.wgsl`).
    pub(super) water_plus: bool,
    /// [`Enhance::cabin`] (`CABIN` in `shader.wgsl`).
    pub(super) cabin: bool,
    /// [`Enhance::sky`] (`SKY_LUT` in `sky_lut.wgsl`).
    pub(super) sky_lut: bool,
    /// [`Enhance::sun`] or [`Enhance::occlusion`]: the draws after the prepass read the sun's shadow and the contact
    /// shading from their mask (`SUN_MASK` in `shader.wgsl`, `renderer::sun_mask`).
    pub(super) sun_mask: bool,
    /// [`Enhance::occlusion`]: with the mask, the contact shading (`CONTACT`).
    pub(super) contact: bool,
    /// [`Enhance::occlusion`]: the reflection probes (`PROBES`).
    pub(super) probes: bool,
    /// The surroundings cube's mip levels past [`REFLECTION_SIZE`](super::reflection::REFLECTION_SIZE)'s: 1 at
    /// [`Detail::Ultra`](super::Detail::Ultra) (`CUBE_SHIFT`).
    pub(super) cube_shift: u32,
    /// The colour targets' alpha the draws write ([`alpha_writes`]).
    pub(super) taa: bool,
}

/// The colour a draw writes into the frame: all of it, but under the temporal anti-aliasing only the car's blended
/// draws (its glass) write the alpha, which the resolve takes as its mask of what moves with the car over what moves
/// with the stage behind it (`taa.wgsl`); the frame's alpha is otherwise read by nothing.
pub(super) fn alpha_writes(taa: bool, car_blend: bool) -> wgpu::ColorWrites {
    match taa && !car_blend {
        true => wgpu::ColorWrites::COLOR,
        false => wgpu::ColorWrites::ALL,
    }
}

/// The stage draws' vertex entry point: with the moving clouds ([`Enhance::clouds`]) the one that also gives the
/// clouds' shadow. Every pipeline over the stage's vertices takes the same one, so their depths agree.
pub(super) fn vertex_entry(enhance: Enhance) -> &'static str {
    if enhance.clouds {
        "vs_clouded"
    } else {
        "vs_main"
    }
}

/// The screen-space reflections' fragment entry point: `fs_reflect_plus` (`water.wgsl`) with the flowing water or the
/// puddles, which keeps `fs_reflect` as it was without them.
pub(super) fn reflect_entry(enhance: Enhance) -> &'static str {
    if enhance.water || enhance.wet {
        "fs_reflect_plus"
    } else {
        "fs_reflect"
    }
}

impl Context<'_> {
    /// The fragment entry point that shades the draws, taking the clouds' shadow when the vertex gives it.
    fn shading(&self) -> &'static str {
        if self.vertex == "vs_clouded" {
            "fs_clouded"
        } else {
            "fs_main"
        }
    }

    pub(super) fn pipeline(&self, key: PipelineKey) -> wgpu::RenderPipeline {
        let (entry, depth_write, depth_compare, alpha_test, write_mask) = match key.pass {
            DrawPass::Main => (
                self.shading(),
                key.depth_write,
                wgpu::CompareFunction::Greater,
                key.alpha_test,
                alpha_writes(self.taa, key.lit != 0 && key.blend),
            ),
            DrawPass::DepthPrepass => {
                let normals = if self.sun_mask { wgpu::ColorWrites::COLOR } else { wgpu::ColorWrites::empty() };
                ("fs_prepass", true, wgpu::CompareFunction::Greater, key.alpha_test, normals)
            }
            DrawPass::AfterPrepass => {
                let write_mask = alpha_writes(self.taa, key.lit != 0 && key.blend);
                (self.shading(), false, wgpu::CompareFunction::Equal, key.alpha_test, write_mask)
            }
        };
        let depth_compare = match key {
            PipelineKey { sky: true, .. } => wgpu::CompareFunction::Equal,
            PipelineKey { depth_test: true, .. } => depth_compare,
            _ => wgpu::CompareFunction::Always,
        };
        let features = key.features.unwrap_or_default();
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stage"),
            layout: Some(self.layout),
            vertex: wgpu::VertexState {
                module: self.shader,
                entry_point: Some(self.vertex),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: FRONT_FACE,
                cull_mode: key.cull_back.then_some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(depth_compare),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: self.shader,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[
                        ("ALPHA_TEST", f64::from(u8::from(alpha_test))),
                        ("WATER_PLUS", f64::from(u8::from(self.water_plus))),
                        ("CABIN", f64::from(u8::from(self.cabin && key.lit != 0))),
                        ("SKY_LUT", f64::from(u8::from(self.sky_lut))),
                        ("MIRRORS", f64::from(u8::from(key.mirror))),
                        ("LAMPS", f64::from(u8::from(key.lamps))),
                        ("LIT", f64::from(key.lit)),
                        ("INSIDE", key.inside.map_or(2.0, |inside| f64::from(u8::from(inside)))),
                        ("CABIN_SET", key.cabin_set.map_or(2.0, |set| f64::from(u8::from(set)))),
                        ("FRAME_KIND", key.frame.map_or(3.0, |frame| frame as f64)),
                        ("KNOWN", f64::from(u8::from(key.features.is_some()))),
                        ("DRAW_WATER", f64::from(u8::from(features.water))),
                        ("DRAW_SPECULAR", f64::from(u8::from(features.specular))),
                        ("DRAW_SECOND", f64::from(u8::from(features.second_texture))),
                        ("DRAW_DETAIL", f64::from(u8::from(features.detail))),
                        (
                            "SUN_MASK",
                            f64::from(u8::from(self.sun_mask && key.pass == DrawPass::AfterPrepass && !key.sky)),
                        ),
                        ("CONTACT", f64::from(u8::from(self.contact))),
                        ("PROBES", f64::from(u8::from(self.probes))),
                        ("CUBE_SHIFT", f64::from(self.cube_shift)),
                    ],
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    // The mask's prepass draws the normals into their own, smaller target.
                    format: if self.sun_mask && key.pass == DrawPass::DepthPrepass { NORMALS } else { self.format },
                    blend: key.blend.then_some(if key.additive {
                        ADDITIVE_BLENDING
                    } else {
                        wgpu::BlendState::ALPHA_BLENDING
                    }),
                    write_mask,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// The sky gradient: one triangle over the screen, behind everything (where the depth is still the clear one, no
    /// depth write).
    pub(super) fn sky_pipeline(&self, layout: &wgpu::PipelineLayout) -> wgpu::RenderPipeline {
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: self.shader,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Equal),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: self.shader,
                entry_point: Some("fs_sky"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &[("SKY_LUT", f64::from(u8::from(self.sky_lut)))],
                    ..Default::default()
                },
                targets: &[Some(wgpu::ColorTargetState {
                    write_mask: alpha_writes(self.taa, false),
                    ..self.format.into()
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// The car shadow map's draws into the layer of vertex `entry`: depth only, both faces (thin parts cast from
    /// either side), with a slope bias against the car shading itself.
    pub(super) fn shadow_pipeline(&self, layout: &wgpu::PipelineLayout, entry: &str) -> wgpu::RenderPipeline {
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("car shadow"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: self.shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout())],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 0, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        })
    }
}

/// The stage's bind group layouts: the frame's (its uniform, the car's shadow map and its sampler, the reflection cube
/// and its sampler, the sun's cascades), the car shadow map draws' (the frame uniform alone), and the draws' own
/// ([`bind_group`](super::textures::bind_group)).
pub(super) fn layouts(device: &wgpu::Device) -> [wgpu::BindGroupLayout; 3] {
    let frame_entry = wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("frame"),
        entries: &[
            frame_entry,
            texture_entry(
                1,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
            ),
            texture_entry(2, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)),
            texture_entry(
                3,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
            ),
            texture_entry(4, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            texture_entry(
                5,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
            ),
            // The physically based sky's tables (`sky_lut.wgsl`): the sky view, the aerial table, the sky cube and the
            // sky's light.
            texture_entry(6, TEXTURE),
            texture_entry(
                8,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D3,
                    multisampled: false,
                },
            ),
            texture_entry(
                9,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
            ),
            texture_entry(
                10,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ), // The surroundings cube (`Reflection::surroundings`).
            texture_entry(
                11,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::Cube,
                    multisampled: false,
                },
            ),
            // The mirrors' images and views (`mirrors.wgsl`).
            texture_entry(
                12,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
            ),
            texture_entry(
                13,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            // The ground's light seen from above and each grid cell's tile (`renderer::bounce::Ground`).
            texture_entry(
                14,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
            ),
            texture_entry(
                15,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            // The reflection probes and their grid (`renderer::probes`).
            texture_entry(
                16,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::CubeArray,
                    multisampled: false,
                },
            ),
            texture_entry(
                17,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            // The car's shadow map's depth range per block (`renderer::shadows::ShadowRange`).
            texture_entry(
                18,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
            ),
            // The volumetric clouds' shadow map and its sampler (`renderer::cloud_volume`, `cloud_shadow` in
            // `clouds.wgsl`): read by the vertex stage (`vs_clouded`), so both stages may see them.
            wgpu::BindGroupLayoutEntry {
                binding: 19,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: TEXTURE,
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 20,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow frame"),
        entries: &[frame_entry],
    });
    let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("texture"),
        entries: &[
            texture_entry(0, TEXTURE),
            texture_entry(1, TEXTURE),
            texture_entry(2, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            // The mode is read by the vertex stage too (the sun's cascades leave out the grass, `vs_sun`).
            wgpu::BindGroupLayoutEntry {
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ..texture_entry(
                    3,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                )
            },
            texture_entry(4, TEXTURE),
            texture_entry(5, TEXTURE),
            texture_entry(6, TEXTURE),
        ],
    });
    [frame_layout, shadow_layout, texture_layout]
}

/// A filterable float 2D texture binding.
pub(crate) const TEXTURE: wgpu::BindingType = wgpu::BindingType::Texture {
    sample_type: wgpu::TextureSampleType::Float { filterable: true },
    view_dimension: wgpu::TextureViewDimension::D2,
    multisampled: false,
};

/// A binding of `ty` that the fragment stage reads.
pub(crate) fn texture_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty, count: None }
}

/// Vertex buffer layout of [`Vertex`]: position, colour, texture 0, 1 and 2 coordinates, normal.
pub(crate) fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Unorm8x4,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x2,
        5 => Float32x3,
        6 => Float32x3,
        7 => Float32,
        8 => Uint32,
        9 => Uint32x2,
        10 => Float32,
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTRIBUTES,
    }
}

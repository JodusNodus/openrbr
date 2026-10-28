//! The stage's draw data (meshes, vertices, draws and their render state) and how [`super::load`] builds it from the
//! `.lbs` geometry blocks, object items, interactive objects, sky faces and water.

use super::{Anchor, LoadReport, Pose, TextureRef, OBJECT_ALPHA_REF};
use crate::coords::rgba_from_d3dcolor;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rbr_formats::lbs;
use std::ops::Range;

/// Which pass a mesh is drawn in. Sky first, then solid, then blended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Sky,
    Solid,
    Blended,
}

/// A group of triangles that share one vertex buffer: a geometry block, an object item or a sky face.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    pub layer: Layer,
    pub anchor: Anchor,
    /// Bounds for culling. `None` for the sky, which is always drawn.
    pub bounds: Option<Bounds>,
    /// The `.lbs` visibility cell of a geometry block or object item (`crate::visibility`): drawn only while the
    /// cell is. `None` for the rest, and on a stage without cells.
    pub cell: Option<Bounds>,
    pub vertices: Vec<Vertex>,
    /// Triangle list indices into `vertices`.
    pub indices: Vec<u32>,
    pub draws: Vec<Draw>,
}

/// Axis-aligned bounds of a mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub center: Vec3,
    pub half_extent: Vec3,
}

/// One vertex: position, D3DCOLOR colour as RGBA, and the texture 0, 1 and 2 coordinates (2 is the shadow uv).
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [u8; 4],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub uv2: [f32; 2],
    /// The surface normal, for the lit draws ([`DrawState::shade`]) and the stage's specular draws (slots 6 and 8,
    /// as stored, not normalised). Zero on the other stage draws, whose light is baked into the vertex colour.
    pub normal: [f32; 3],
    /// The specular map's uv and the vertex's specular strength (slots 6 and 8, TEXCOORD2 and TEXCOORD3); zero
    /// elsewhere.
    pub specular: [f32; 3],
    /// How little of the sky the vertex sees, 0 (open) to 1, computed for the stage's world draws
    /// ([`crate::sky_occlusion`]; ours); 0 elsewhere.
    pub occlusion: f32,
    /// The mean direction the vertex sees the sky in (its bent normal; octahedral, two 16-bit snorms,
    /// `sky_occlusion.wgsl`), computed with [`Vertex::occlusion`]; 0 for none (the normal stands in).
    pub sky_dir: u32,
    /// The light the vertex gets off the stage it sees below the horizon, per unit of the sun's and of the sky's light
    /// on that ground (ours, [`crate::bounce`]): two colours of three 10-bit shares, computed with
    /// [`Vertex::occlusion`]; 0 elsewhere.
    pub bounce: [u32; 2],
    /// How much of the wet film the ground takes in wet and damp weather (ours, [`crate::puddles::mark_roads`]): by the
    /// collision surface under it, tarmac 1, gravel and dirt 0.5; 0 elsewhere. Dry too with the gravel scan, which takes
    /// the loose roads by it ([`Stage::mark_dry_roads`](super::Stage::mark_dry_roads)).
    pub wet: f32,
    /// How far the wet road's edge is from the vertex, metres, up to 8 (ours, [`crate::puddles::mark_roads`]); 0
    /// elsewhere. Interpolated across a road triangle it is the distance to the road's edge there.
    pub edge: f32,
}

/// Triangles drawn with one render state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draw {
    /// Range of the mesh's `indices`.
    pub indices: Range<usize>,
    pub state: DrawState,
    /// When the draw shows, by the camera's distance to its mesh.
    pub lod: Lod,
}

/// Level of detail of a geometry chunk, its draw list (render chunk byte 0x6c, `HH21_Block10`). Each block
/// holds its ground twice: a near version (list 1) and a far one (list 3) with fewer, larger triangles, and
/// list 2 is drawn at every distance. The render set's draw (0x10027590, `HH_H12D_7590_Render`) measures the
/// camera's distance to the block's box centre and draws list 1 below [`LOD_DISTANCE`], list 3 above
/// `LOD_DISTANCE - LOD_BAND` (both in the band between), and list 2 always. Drawing both versions everywhere
/// stacks the far ground's stretched texture over the near one. The capture's draws follow this rule
/// (`notes/formats.md` "Stage geometry").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Lod {
    #[default]
    Always,
    Near,
    Far,
}

/// The near-to-far switch distance, metres: `g_renderQuality.m_unk10`, which each quality preset sets
/// (`HH_Renderer.cpp`: low 200, medium 300, custom 500, high 600). Always high's (ours, the highest quality; the
/// reference installs and the capture ran `RenderQuality = low`).
pub const LOD_DISTANCE: f32 = 600.0;
/// Width of the band below [`LOD_DISTANCE`] where both versions draw, metres (0x10027590).
pub const LOD_BAND: f32 = 50.0;

impl Lod {
    /// The level of a chunk's draw list (1 near, 3 far, anything else always).
    pub fn of_list(list: u8) -> Lod {
        match list {
            1 => Lod::Near,
            3 => Lod::Far,
            _ => Lod::Always,
        }
    }

    /// Whether a draw of this level shows with the camera `distance` metres from its block's box centre.
    pub fn shows(self, distance: f32) -> bool {
        match self {
            Lod::Always => true,
            Lod::Near => distance < LOD_DISTANCE,
            Lod::Far => distance > LOD_DISTANCE - LOD_BAND,
        }
    }
}

/// The render state of one draw. Each field's source is noted where it is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrawState {
    /// Texture 0 and texture 1. `None` means no texture (vertex colour only).
    pub textures: [Option<TextureRef>; 2],
    /// Shadow light map, multiplied into the colour (slots 7 and 8; `notes/formats.md` "Shaders and vertex layouts").
    pub shadow: Option<TextureRef>,
    /// Address modes for U and V.
    pub address: [Address; 2],
    /// The second texture blends in by its alpha and the vertex alpha (`double` shader types).
    pub second_texture: bool,
    /// Alpha test threshold in 1/255: texels with `alpha <= ref` are dropped.
    pub alpha_ref: Option<u8>,
    /// Textures 0 and 1 are sampled with the mip LOD bias [`MIP_MAP_BIAS`] (the shadow light map never is).
    pub mip_bias: bool,
    /// Cull back faces (`D3DCULL_CCW`).
    pub cull_back: bool,
    /// Alpha blended over the frame.
    pub blend: bool,
    /// With `blend`, the colour is added to the frame, scaled by the texel's alpha, instead of alpha blended over it
    /// (the car's light flares, `lights`).
    pub additive: bool,
    /// Depth test against the depth buffer.
    pub depth_test: bool,
    /// Write depth.
    pub depth_write: bool,
    /// Lit with the stage's sun, ambient and sky reflection (the car; `None`: unlit).
    pub shade: Option<Shade>,
    /// Specular map, its highlight added to the colour before the shadow light map (slots 6 and 8).
    pub specular: Option<TextureRef>,
}

/// A car class's lighting constants from `<car>_shader_settings_clear.ini` (`Ka`, `Kd`, `CookTorranceKs`,
/// `CookTorranceM`, `FresnelMin`, `FresnelMax`, `Kf`; `notes/formats.md` "Shaders and vertex layouts", Car pixel constants and vertex
/// constants). The game's own use of them goes through constants with unproven sources (`c41`, `c67`, `c68`, `c90`,
/// the dirt ratio), so the lighting model that uses them is the renderer's (`shader.wgsl`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Shade {
    pub ka: f32,
    pub kd: f32,
    pub ks: f32,
    pub m: f32,
    pub fresnel_min: f32,
    pub fresnel_max: f32,
    pub kf: f32,
    /// `CarGlass`: the Fresnel term adds to the alpha (`alpha = t0.a + 0.5 * F * Kf`).
    pub glass: bool,
    /// A lit lamp's part (`lights::Switch`): glows (renderer addition).
    pub glow: bool,
    /// A window's crack overlay (ours, [`crate::crack_decal`]): its normal comes from the crack texture's alpha, so
    /// the cracks catch the light.
    pub crack: bool,
    /// `CarPaint`'s dirt map (`Dirt`, `<car>_external_mud.dds`), blended in by the car's dirt level (`crate::dirt`).
    pub dirt: Option<TextureRef>,
    /// An exterior pane's mud film by the car's dirt level (ours, [`crate::dirt::grime`]): 0 none, else its share in
    /// tenths.
    pub grime: u8,
    /// A wheel's or mud flap's dirt spatter by the car's dirt level (ours, [`crate::dirt::spatter`]).
    pub spatter: bool,
    /// `CarIntGlass`: in the in-car view with the day's light a tint over what is seen through it, not a lit layer
    /// (ours).
    pub tint: bool,
}

impl Shade {
    fn bits(&self) -> [u32; 8] {
        [
            self.ka.to_bits(),
            self.kd.to_bits(),
            self.ks.to_bits(),
            self.m.to_bits(),
            self.fresnel_min.to_bits(),
            self.fresnel_max.to_bits(),
            self.kf.to_bits(),
            u32::from(self.glass)
                | u32::from(self.glow) << 1
                | u32::from(self.crack) << 2
                | u32::from(self.grime) << 3
                | u32::from(self.tint) << 11
                | u32::from(self.spatter) << 12,
        ]
    }
}

impl PartialEq for Shade {
    fn eq(&self, other: &Shade) -> bool {
        self.bits() == other.bits() && self.dirt == other.dirt
    }
}

impl Eq for Shade {}

impl std::hash::Hash for Shade {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits().hash(state);
        self.dirt.hash(state);
    }
}

/// The mip LOD bias of the sharpened draws ([`DrawState::mip_bias`]). Geometry chunks of draw list 1
/// (render chunk byte 0x6c) get the world setting `MipMapBias` from `Maps/TrackSettings.ini` in their texture 0
/// and 1 samplers (`CHH_World::Unk60` through `HH_H12D_7500_FillFloat`); the car's diffuse gets -3 in
/// `CShaderCarBase::Load`. `MipMapBias` is -3 in 360 of the 387 `TrackSettings.ini` sections and -2.88 in the
/// rest; which section a run uses is not decoded, so -3 stands for all (`notes/formats.md` "Shaders and vertex layouts", Mip LOD bias).
pub const MIP_MAP_BIAS: f32 = -3.0;

/// A texture address mode. The numbers are the kind byte's slot modes (`HH_TM2_0D10_FlagDecode`):
/// 1 wrap, 2 mirror, 3 clamp, as the D3D9 `D3DTADDRESS` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Address {
    #[default]
    Wrap,
    Mirror,
    Clamp,
}

/// A texture the draws use. `image` holds the decoded file when it was found and reads; `None` means the
/// texture is missing and the renderer substitutes a placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageTexture {
    /// The archive path that was requested (after the filename map), for reports.
    pub request: String,
    pub image: Option<Image>,
}

/// The bytes of a texture file, as the renderer uploads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// A block-compressed `.dds`, parsed by `rbr_formats::dds`.
    Dds(Vec<u8>),
    /// A flat RGBA texel used by a renderer stand-in, never read from the game files.
    Solid([u8; 4]),
    /// A shadow light map: `width * height` `L8` texels.
    Shadow { width: u32, height: u32, pixels: Vec<u8> },
    /// A block-compressed `.dds` the dials draw on ([`crate::dials`]): the renderer keeps it as their base and samples
    /// a render target made from it ([`crate::renderer::Renderer::draw_dials`]). `glow` is the gauge's emissive mask, a
    /// `.dds` whose alpha is how much of each texel glows by night (`shader.wgsl`, `DIAL_GLOW`).
    Dial { base: Vec<u8>, glow: Option<Vec<u8>> },
}

/// Byte offsets, within a geometry vertex, of what the renderer reads for one render slot
/// (`notes/formats.md` "Stage geometry", geometry table). Position is at 0 in every slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SlotLayout {
    color: usize,
    texcoord0: Option<usize>,
    texcoord1: Option<usize>,
    texcoord2: Option<usize>,
    /// The stored normal, and the specular uv followed by the specular strength (slots 6 and 8).
    normal: Option<usize>,
    specular: Option<usize>,
}

fn slot_layout(slot: usize) -> Option<SlotLayout> {
    let layout = |color, texcoord0, texcoord1, texcoord2| SlotLayout {
        color,
        texcoord0,
        texcoord1,
        texcoord2,
        normal: None,
        specular: None,
    };
    // Slots 6 and 8 store NORMAL at 12 and the specular uv and strength (TEXCOORD2, TEXCOORD3) at 44 and 52.
    let specular = |layout: SlotLayout| SlotLayout { normal: Some(12), specular: Some(44), ..layout };
    match slot {
        0 => Some(layout(12, None, None, None)),
        5 => Some(layout(12, Some(16), Some(24), None)),
        7 => Some(layout(12, Some(16), Some(24), Some(32))),
        // Slot 8's shadow uv is TEXCOORD4 (`RLDoubleTextureSpecularShadow.vso`, `dcl_texcoord4 v13`); slot 6 has
        // no shadow.
        6 => Some(specular(layout(24, Some(28), Some(36), None))),
        8 => Some(specular(layout(24, Some(28), Some(36), Some(56)))),
        _ => None,
    }
}

/// The car start pose from the `(2,11)` chunk: its position, and the heading from its rotation (x pitch, y yaw), the
/// two vectors the loader reads (`HH_H23_Block2b` skips the matrix; community exporters leave it zero or stale). On
/// retail stages the heading equals the matrix's third row.
pub(super) fn start_pose(location: &lbs::StartLocation) -> Option<Pose> {
    let [pitch, yaw, _] = location.rotation;
    let forward = Vec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()).normalize_or_zero();
    let position = Vec3::from_array(location.position);
    (forward != Vec3::ZERO).then_some(Pose { position, forward })
}

/// One geometry block: every render slot's vertices, and the triangles of its render chunks.
pub(super) fn block_mesh(block: &lbs::GeometryBlock, report: &mut LoadReport) -> Mesh {
    let mut vertices = Vec::new();
    // Where each slot's vertices start in `vertices`, for slots with a known layout.
    let mut bases: [Option<usize>; 9] = [None; 9];
    for (slot, view) in block.views.iter().enumerate() {
        let (Some(view), Some(layout)) = (view, slot_layout(slot)) else {
            continue;
        };
        bases[slot] = Some(vertices.len());
        vertices.extend((0..view.vertex_count).map(|v| repack(view, v, layout)));
    }

    let mut indices = Vec::new();
    let mut draws = Vec::new();
    for chunk in &block.chunks {
        let (Some(view), Some(base)) = (block.views[chunk.view], bases[chunk.view]) else {
            report.skipped_draws += 1;
            continue;
        };
        let first = indices.len();
        // Indices are absolute in the view's buffer. The chunk's draw names the range they lie in, but some RSF
        // stages (141, 176, 177) index outside it; the game's hardware vertex processing draws them as given, so
        // only indices past the buffer are dropped.
        let in_range = |i: usize| i < view.vertex_count;
        for triangle in 0..chunk.prim_count as usize {
            let at = chunk.start_index as usize + 3 * triangle;
            match [at, at + 1, at + 2].map(|i| view.index(i).map(usize::from)) {
                [Some(a), Some(b), Some(c)] if [a, b, c].into_iter().all(in_range) => {
                    indices.extend([a, b, c].map(|i| (base + i) as u32));
                }
                _ => report.bad_triangles += 1,
            }
        }
        if indices.len() > first {
            // Texture 0 with the vertex colour, and texture 1 blended in when the chunk has two. The shadow
            // light map needs the shadow uv (`slot_layout`, slots 7 and 8), so it is only read from those views.
            let textures = [0, 1].map(|i| chunk.textures.get(i).copied().map(TextureRef::stage));
            let shadow =
                (matches!(chunk.view, 7 | 8) && chunk.shadow).then(|| TextureRef::shadow(chunk.shadow_texture));
            let specular =
                (matches!(chunk.view, 6 | 8) && chunk.specular).then(|| TextureRef::specular(chunk.specular_texture));
            let lod = Lod::of_list(chunk.list);
            draws.push(Draw {
                indices: first..indices.len(),
                state: DrawState {
                    textures,
                    shadow,
                    address: [Address::Wrap; 2],
                    second_texture: textures[1].is_some(),
                    alpha_ref: None,
                    // List 1 (the render set's list at `+0x94`) is the one `HH_H12D_7500_FillFloat` biases.
                    mip_bias: lod == Lod::Near,
                    cull_back: true,
                    blend: false,
                    additive: false,
                    depth_test: true,
                    depth_write: true,
                    shade: None,
                    specular,
                },
                lod,
            });
        }
    }

    Mesh {
        layer: Layer::Solid,
        anchor: Anchor::World,
        bounds: Some(Bounds {
            center: Vec3::from(block.bounds.center),
            half_extent: Vec3::from(block.bounds.half_extent),
        }),
        cell: None,
        vertices,
        indices,
        draws,
    }
}

/// One object item: its own buffers, drawn with the alpha test and the item's address modes
/// (`kind` byte, `notes/formats.md` "Shaders and vertex layouts"). Items are double-sided.
pub(super) fn object_mesh(item: &lbs::ObjectItem, report: &mut LoadReport) -> Mesh {
    let kind = lbs::KindState::decode(item.kind);
    let fields = match lbs::ItemVertex::for_flags(item.flags) {
        Some(layout) => SlotLayout {
            color: layout.color,
            texcoord0: Some(layout.texcoord0),
            texcoord1: None,
            texcoord2: None,
            normal: None,
            specular: None,
        },
        None => {
            report.skipped_draws += 1;
            return Mesh {
                layer: Layer::Solid,
                anchor: Anchor::World,
                bounds: Some(bounds_of(item.bounds)),
                cell: None,
                vertices: Vec::new(),
                indices: Vec::new(),
                draws: Vec::new(),
            };
        }
    };
    let state = DrawState {
        textures: [item.texture_index().map(TextureRef::stage), None],
        shadow: None,
        address: [address(kind.slot0), address(kind.slot1)],
        second_texture: false,
        alpha_ref: Some(OBJECT_ALPHA_REF),
        mip_bias: false,
        cull_back: false,
        blend: false,
        additive: false,
        depth_test: true,
        depth_write: true,
        shade: None,
        specular: None,
    };
    let mesh = buffer_mesh(&item.buffers, fields, Layer::Solid, state, report);
    Mesh { bounds: Some(bounds_of(item.bounds)), ..mesh }
}

/// One interactive object's shared local draw buffers. Retail records with this layout use the
/// same texture-0, alpha-test, depth and address defaults as stage object draws; this is the
/// data-backed stand-in for the runtime render-state factory.
pub(super) fn interactive_mesh(object: &lbs::InteractiveObject<'_>, report: &mut LoadReport) -> Mesh {
    let mut mesh = Mesh {
        layer: Layer::Solid,
        anchor: Anchor::World,
        bounds: None,
        cell: None,
        vertices: Vec::new(),
        indices: Vec::new(),
        draws: Vec::new(),
    };
    let fields =
        SlotLayout { color: 12, texcoord0: Some(16), texcoord1: None, texcoord2: None, normal: None, specular: None };
    for item in &object.draws {
        let buffers = lbs::Buffers {
            indices: item.indices,
            vertices: item.vertices,
            vertex_count: item.vertex_count,
            stride: item.vertex_size,
        };
        let state = DrawState {
            textures: [item.texture0.map(TextureRef::stage), None],
            shadow: None,
            address: [Address::Wrap; 2],
            second_texture: false,
            alpha_ref: Some(OBJECT_ALPHA_REF),
            mip_bias: false,
            cull_back: false,
            blend: false,
            additive: false,
            depth_test: true,
            depth_write: true,
            shade: None,
            specular: None,
        };
        let part = buffer_mesh(&buffers, fields, Layer::Solid, state, report);
        let vertex_base = mesh.vertices.len() as u32;
        let index_base = mesh.indices.len();
        mesh.vertices.extend(part.vertices);
        mesh.indices.extend(part.indices.into_iter().map(|index| index + vertex_base));
        mesh.draws.extend(part.draws.into_iter().map(|mut draw| {
            draw.indices = draw.indices.start + index_base..draw.indices.end + index_base;
            draw
        }));
    }
    mesh
}

/// One stage sky face: texture 0 (and texture 1 with flag 0x20) with the vertex colour, no culling, depth
/// test on and depth write off, as the capture's `SkyDome` draws (`notes/formats.md` "Shaders and vertex layouts"). Whether
/// these faces use the `SkyDome` shader, which ignores vertex colour, is not verified. The tree-line strips'
/// textures have an alpha cut-out (`OpacityMap=true`, for example `MB_SUPERBOWL_8` and `_9` in track-41_O); the
/// face's render state is not decoded, so the object lanes' alpha test cuts them (ours). The faces are terrain on
/// some stages and upright photo panoramas on others (462 Odry); the shading must suit both (`notes/renderer.md`
/// "Decisions").
pub(super) fn face_mesh(item: &lbs::SkyItem, report: &mut LoadReport) -> Mesh {
    let state = DrawState {
        textures: [item.texture0.map(TextureRef::stage), item.texture1.map(TextureRef::stage)],
        shadow: None,
        address: [Address::Wrap; 2],
        second_texture: item.texture1.is_some(),
        alpha_ref: Some(OBJECT_ALPHA_REF),
        mip_bias: false,
        cull_back: false,
        blend: false,
        additive: false,
        depth_test: true,
        depth_write: false,
        shade: None,
        specular: None,
    };
    buffer_mesh(&item.buffers, sky_fields(item.flags), Layer::Sky, state, report)
}

/// Water vertex declarations 29/30 (`ShaderManager.cpp`, `g_vsDecls[35]` / `[89]`) put position at
/// byte 0, D3DCOLOR at 24, and TEXCOORD0 at 28. The flat texture supplies the stand-in colour and alpha.
/// The declaration strides are 48 and 56 bytes. Only `Enhance::water` reads the rest (`water_flowing` in
/// `shader.wgsl`; the default water shader uses the position only): the normal at 12 (up in retail), the colour
/// (grey shade, alpha 0 at the banks in retail), the second layer's coordinates at 36 in `uv1`, and each layer's
/// scroll (texture coordinates per second) in `uv2` and `specular`. The game scrolls the first layer by `key10`'s
/// float pair and, in the 56-byte `WaterDoubleTexture.vso` only, the second by `key20`'s (`HH_KWaterRenderItems`
/// 0x10030170 uploads the object's +0x48 and +0x50 pairs as `c85.yz` and `c86.yz`, the shaders add `c8x.x * c8x.yz`
/// to `TEXCOORD0` and `TEXCOORD1`); `key40`'s pair (+0x58) is never uploaded.
pub(super) fn water_mesh(item: &lbs::SurfaceItem<'_>, report: &mut LoadReport) -> Mesh {
    let scroll = |key: lbs::SurfaceKey| key.and_then(|(_, pair)| pair).unwrap_or([0.0; 2]);
    let buffers = &item.buffers;
    let stride = buffers.vertex_size;
    let first = scroll(item.key10);
    let second = if stride == 56 { scroll(item.key20) } else { [0.0; 2] };
    let vertex_count = if matches!(stride, 48 | 56) && buffers.vertices.len().is_multiple_of(stride) {
        buffers.vertices.len() / stride
    } else {
        report.skipped_draws += 1;
        0
    };
    let vertices: Vec<Vertex> = (0..vertex_count)
        .map(|vertex| {
            let at = vertex * stride;
            let raw = &buffers.vertices[at..at + stride];
            Vertex {
                position: [f32_at(raw, 0), f32_at(raw, 4), f32_at(raw, 8)],
                color: rgba_from_d3dcolor([raw[24], raw[25], raw[26], raw[27]]),
                uv0: [f32_at(raw, 28), f32_at(raw, 32)],
                uv1: [f32_at(raw, 36), f32_at(raw, 40)],
                uv2: first,
                normal: [f32_at(raw, 12), f32_at(raw, 16), f32_at(raw, 20)],
                specular: [second[0], second[1], 0.0],
                occlusion: 0.0,
                sky_dir: 0,
                bounce: [0; 2],
                wet: 0.0,
                edge: 0.0,
            }
        })
        .collect();

    let mut indices = Vec::new();
    let index_count = buffers.indices.len() / 2;
    if !index_count.is_multiple_of(3) {
        report.bad_triangles += 1;
    }
    for triangle in 0..index_count / 3 {
        let at = triangle * 6;
        let index =
            |at: usize| buffers.indices.get(at..at + 2).map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as usize);
        match [index(at), index(at + 2), index(at + 4)] {
            [Some(a), Some(b), Some(c)] if [a, b, c].into_iter().all(|i| i < vertices.len()) => {
                indices.extend([a, b, c].map(|i| i as u32));
            }
            _ => report.bad_triangles += 1,
        }
    }

    let draws = if indices.is_empty() {
        Vec::new()
    } else {
        vec![Draw {
            indices: 0..indices.len(),
            state: DrawState {
                textures: [Some(TextureRef::water_stand_in()), None],
                shadow: None,
                address: [Address::Wrap; 2],
                second_texture: false,
                alpha_ref: Some(0),
                mip_bias: false,
                cull_back: true,
                blend: true,
                additive: false,
                depth_test: true,
                depth_write: true,
                shade: None,
                specular: None,
            },
            lod: Lod::Always,
        }]
    };
    Mesh { layer: Layer::Blended, anchor: Anchor::World, bounds: None, cell: None, vertices, indices, draws }
}

/// Vertex fields of a sky item: 24-byte vertices (flags 0x15) have texcoord0 at 16; 32-byte vertices
/// (0x35) add texcoord1 at 24. Colour is at 12 in both.
pub(super) fn sky_fields(flags: u32) -> SlotLayout {
    SlotLayout {
        color: 12,
        texcoord0: Some(16),
        texcoord1: (flags == 0x35).then_some(24),
        texcoord2: None,
        normal: None,
        specular: None,
    }
}

/// A mesh of one buffer pair: every vertex, and the triangles whose indices are in range.
fn buffer_mesh(
    buffers: &lbs::Buffers,
    fields: SlotLayout,
    layer: Layer,
    state: DrawState,
    report: &mut LoadReport,
) -> Mesh {
    let vertices: Vec<Vertex> = (0..buffers.vertex_count).map(|v| repack(buffers, v, fields)).collect();
    let mut indices = Vec::new();
    for triangle in 0..buffers.index_count() / 3 {
        let at = 3 * triangle;
        match [at, at + 1, at + 2].map(|i| buffers.index(i).map(usize::from)) {
            [Some(a), Some(b), Some(c)] if [a, b, c].into_iter().all(|i| i < vertices.len()) => {
                indices.extend([a, b, c].map(|i| i as u32));
            }
            _ => report.bad_triangles += 1,
        }
    }
    let draws =
        if indices.is_empty() { Vec::new() } else { vec![Draw { indices: 0..indices.len(), state, lod: Lod::Always }] };
    Mesh { layer, anchor: Anchor::World, bounds: None, cell: None, vertices, indices, draws }
}

/// A camera-anchored mesh: drawn at infinity, so it is never culled.
pub(super) fn camera_mesh(
    buffers: &lbs::Buffers,
    fields: SlotLayout,
    layer: Layer,
    state: DrawState,
    report: &mut LoadReport,
) -> Mesh {
    Mesh { anchor: Anchor::Camera, ..buffer_mesh(buffers, fields, layer, state, report) }
}

/// A `(2,10)` visibility cell's box: its first vector is the centre, its second the half extent (the game's tests,
/// `HH_H24_VisSearch406` 0x100628e0 and `HH_H24_VisFill5` 0x10062bc0).
pub(super) fn cell_bounds(entry: &lbs::VisibilityEntry) -> Bounds {
    Bounds { center: Vec3::from(entry.a), half_extent: Vec3::from(entry.b) }
}

fn bounds_of(bounds: lbs::Bounds) -> Bounds {
    Bounds { center: Vec3::from(bounds.center), half_extent: Vec3::from(bounds.half_extent) }
}

/// The address mode of one slot of the kind byte: 1 wrap, 2 mirror, 3 clamp.
pub(super) fn address(mode: u8) -> Address {
    match mode {
        2 => Address::Mirror,
        3 => Address::Clamp,
        _ => Address::Wrap,
    }
}

/// One vertex of `buffers`, with the colour reordered to RGBA. Positions and attributes were checked by
/// the parser, so a read outside the buffer gives zero rather than a panic.
fn repack(buffers: &lbs::Buffers, v: usize, fields: SlotLayout) -> Vertex {
    let position = buffers.position(v).unwrap_or([0.0; 3]);
    let color = vertex_bytes(buffers, v, fields.color, 4)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map_or([0; 4], rgba_from_d3dcolor);
    Vertex {
        position,
        color,
        uv0: texcoord(buffers, v, fields.texcoord0),
        uv1: texcoord(buffers, v, fields.texcoord1),
        uv2: texcoord(buffers, v, fields.texcoord2),
        normal: fields
            .normal
            .and_then(|at| vertex_bytes(buffers, v, at, 12))
            .map_or([0.0; 3], |b| [f32_at(b, 0), f32_at(b, 4), f32_at(b, 8)]),
        specular: fields
            .specular
            .and_then(|at| vertex_bytes(buffers, v, at, 12))
            .map_or([0.0; 3], |b| [f32_at(b, 0), f32_at(b, 4), f32_at(b, 8)]),
        occlusion: 0.0,
        sky_dir: 0,
        bounce: [0; 2],
        wet: 0.0,
        edge: 0.0,
    }
}

fn texcoord(buffers: &lbs::Buffers, v: usize, at: Option<usize>) -> [f32; 2] {
    at.and_then(|at| vertex_bytes(buffers, v, at, 8)).map_or([0.0; 2], |b| [f32_at(b, 0), f32_at(b, 4)])
}

/// `len` bytes of vertex `v` at `offset`, if they lie inside the buffer.
fn vertex_bytes<'a>(buffers: &lbs::Buffers<'a>, v: usize, offset: usize, len: usize) -> Option<&'a [u8]> {
    let start = v.checked_mul(buffers.stride)?.checked_add(offset)?;
    buffers.vertices.get(start..start.checked_add(len)?)
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_pose_reads_position_and_rotation() {
        // tanner track-54_E: rotation (-0.04, -0.736, -0.05), matrix row 2 (-0.671, 0.04, 0.74).
        let location = lbs::StartLocation {
            matrix: [0.0; 16],
            position: [1850.7, 683.6, 1116.5],
            rotation: [-0.04, -0.736, -0.05],
        };
        let pose = start_pose(&location).unwrap();
        assert_eq!(pose.position, Vec3::new(1850.7, 683.6, 1116.5));
        assert!((pose.forward - Vec3::new(-0.671, 0.04, 0.74)).length() < 0.002, "{:?}", pose.forward);
    }

    #[test]
    fn address_modes_follow_the_slot_values() {
        assert_eq!([address(1), address(2), address(3)], [Address::Wrap, Address::Mirror, Address::Clamp]);
    }
}

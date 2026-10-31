//! Reader for stage geometry `Maps/*.lbs` (render meshes and object lanes).
//!
//! Container: the `.trk` framing (see [`crate::trk`]), with type 1 and type 2
//! chunks. Each chunk is `[u32 header_len = 8][u32 type][u32 subtype]
//! [u32 payload_len][payload]`, and the chunks tile the file exactly.
//!
//! Layouts come from HedgeHog3D's track loader `CHH_TrackLoader::LoadTrack`
//! (0x10052220) and the block handlers it dispatches to. See
//! `notes/formats.md` "Stage geometry" for the evidence and the unparsed chunk kinds.
//!
//! Parsed here:
//! - `(1, 0)` geometry: a sky word, then blocks. Each block has nine
//!   vertex/index views (one per render slot), render chunks that draw
//!   ranges of those views, and the block bounds.
//! - `(1, 1)` object lanes: each item carries its own index and vertex
//!   buffers and its bounds.
//! - `(1, 6)` interactive objects: named draw records and visibility matrices.
//! - `(1, 3)` sky dome faces.
//! - `(2, 10)` visibility entries: two vectors plus the lane index.
//! - `(2, 11)` car start location: a 4x4 matrix and two vectors.
//! - `(2, 12)` driveline points: a count and that many vectors.
//! - `(2, 14)` animation objects: a name, three words, two material
//!   (script) names, five vectors and a packed color per item.
//! - `(2, 15)` container objects: a name and three words per item.
//! - `(2, 16)` track vectors: two vectors.
//! - `(2, 18)` visible-object vectors: four vectors and three words per
//!   record (no count word; the loader iterates its own visibility
//!   count, which equals the `(2, 10)` entry count in every retail file).
//!
//! Every other chunk is kept as [`Chunk::Other`] with its raw payload.
//!
//! All reads are bounds-checked against the chunk they belong to, and every
//! count is checked against the bytes that remain before anything is
//! allocated. [`parse`] requires each payload to be consumed exactly.

use std::fmt::{self, Display, Formatter};

mod read;

pub use read::parse;

/// Vertex stride in bytes of each geometry view, indexed by render slot. The
/// loader fixes these per slot (the nine `HH_H24_ReadBlock*` readers), so the
/// file does not store them. No retail chunk draws from slots 1-4; candidate
/// declarations by stride from `g_vsDecls` (`ShaderManager.cpp`) are: 0x18 is
/// `RLSingleTexture.vso` (vs 10) or the 0x18 car decls, 0x30 is
/// `RLSingleTextureSpecular.vso` (vs 14), 0x24 is `RLSingleTextureSway.vso`
/// (vs 19), 0x3c is `RLSingleTextureSpecularShadow.vso` (vs 15). Which
/// declaration (and shader) each slot uses is unverified: it only shows in
/// retail `(vs, ps, slot)` triples, and none use slots 1-4.
pub const VIEW_STRIDES: [usize; 9] = [0x10, 0x18, 0x30, 0x24, 0x3c, 0x20, 0x38, 0x2c, 0x44];

/// Bytes per render chunk record.
pub const RENDER_CHUNK_LEN: usize = 0x70;

const GEOMETRY: (u32, u32) = (1, 0);
const OBJECTS: (u32, u32) = (1, 1);
const CLOUDS: (u32, u32) = (1, 2);
const SKY: (u32, u32) = (1, 3);
const CLIPPING: (u32, u32) = (1, 4);
const INTERACTIVE: (u32, u32) = (1, 6);
const REFLECTION: (u32, u32) = (1, 7);
const WATER: (u32, u32) = (1, 8);
const TEXTURE_TABLE: (u32, u32) = (0, 23);
const VISIBILITY: (u32, u32) = (2, 10);
const START_LOCATION: (u32, u32) = (2, 11);
const DRIVELINE_POINTS: (u32, u32) = (2, 12);
const ANIMATION_OBJECTS: (u32, u32) = (2, 14);
const CONTAINER_OBJECTS: (u32, u32) = (2, 15);
const TRACK_VECTORS: (u32, u32) = (2, 16);
const VISIBLE_OBJECTS: (u32, u32) = (2, 18);

/// A parsed `.lbs` file: its chunks in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Lbs<'a> {
    pub chunks: Vec<Chunk<'a>>,
}

/// One framed chunk. Payload slices borrow from the input buffer.
#[derive(Debug, Clone, PartialEq)]
pub enum Chunk<'a> {
    /// Type 1, subtype 0: static geometry.
    Geometry(Geometry<'a>),
    /// Type 1, subtype 1: object lanes.
    Objects(Objects<'a>),
    /// Type 1, subtype 6: interactive faces (`HH21_Block16`, `0x10057fd0`).
    Interactive(InteractiveObjects<'a>),
    /// Type 1, subtype 2: cloud faces. The items use the sky dome layout (`HH21_Block12` fills them
    /// with `HH22_FillSkyItem`, kind 1).
    Clouds(Sky<'a>),
    /// Type 1, subtype 3: the sky dome (`SuperBowl`) face.
    Sky(Sky<'a>),
    /// Type 1, subtype 4: clipping planes (`FUN_100590b0`).
    ClippingPlanes(ClippingPlanes),
    /// Type 1, subtype 7: reflection faces (`HH21_Block17`, `0x100578d0`).
    /// Same item layout as water; the handler skips the float pairs instead
    /// of storing them.
    Reflection(Reflection<'a>),
    /// Type 1, subtype 8: water faces (`HH21_Block18`, `0x10057080`).
    Water(Water<'a>),
    /// Type 0, subtype 23: per-texture words (`FUN_10059a50`). Each record
    /// repeats the stage `.ini` `[TextureN]` section of the same name.
    TextureTable(TextureTable<'a>),
    /// Type 2, subtype 10: visibility entries (`HH21_Block2a`).
    Visibility(Visibility),
    /// Type 2, subtype 11: car start location (`HH_H23_Block2b`).
    StartLocation(StartLocation),
    /// Type 2, subtype 12: driveline points (`HH21_Block2c`).
    DrivelinePoints(DrivelinePoints),
    /// Type 2, subtype 14: animation objects (`HH_H23_Block2e`).
    AnimationObjects(AnimationObjects<'a>),
    /// Type 2, subtype 15: container objects (`HH_H23_Block2f`).
    ContainerObjects(ContainerObjects<'a>),
    /// Type 2, subtype 16: track vectors (`HH_H23_Block210`).
    TrackVectors(TrackVectors),
    /// Type 2, subtype 18: visible-object vectors (`HH_H23_Block212`).
    VisibleObjects(VisibleObjects),
    /// Any other `(type, subtype)`, payload verbatim.
    Other { kind: (u32, u32), data: &'a [u8] },
}

/// Geometry payload: a sky word, then the blocks in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry<'a> {
    /// Float bits stored into the sky holder (`CHH_SkyHolder` impl, `+0x18`).
    /// The holder never reads the word back, so it has no render effect;
    /// retail values are 0, 3, 4, 8, 12, 15 and 20 (as floats).
    pub sky_value: u32,
    pub blocks: Vec<GeometryBlock<'a>>,
}

/// One geometry block: nine views, then render chunks, then bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryBlock<'a> {
    /// View `i` is the buffer pair that render chunks with `view == i` draw
    /// from. `None` for an empty view.
    pub views: [Option<Buffers<'a>>; 9],
    pub chunks: Vec<RenderChunk>,
    pub bounds: Bounds,
}

/// A render chunk: one `DrawIndexedPrimitive(TRIANGLELIST, base 0,
/// min_index, vertex_count, start_index, prim_count)` on its view.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderChunk {
    /// Render slot (0..=8), which view it draws from.
    pub view: usize,
    /// Vertex shader id. Its declaration sets the view's vertex layout.
    pub vertex_shader: u32,
    /// Pixel shader id.
    pub pixel_shader: u32,
    /// First index in the view's index buffer.
    pub start_index: u32,
    /// Triangles to draw.
    pub prim_count: u32,
    /// Vertices spanned by the draw, from `min_index`.
    pub vertex_count: u32,
    pub min_index: u32,
    /// Bounds of the chunk's geometry.
    pub bounds: Bounds,
    /// Shadow texture flag (byte at record offset 0x35). `shadow_texture`
    /// is only used when this is set.
    pub shadow: bool,
    /// Index into the stage's shadow texture list.
    pub shadow_texture: u32,
    /// Specular flag (byte at 0x3c). `specular_texture` is only used when
    /// this is set.
    pub specular: bool,
    /// Index into the stage's specular texture list.
    pub specular_texture: u32,
    /// Diffuse texture indices into `[TextureInfo]` of the stage `.ini`
    /// (`Texture0`, `Texture1`, ...). Empty or two entries in retail.
    pub textures: Vec<u32>,
    /// Vertex-shader-constant flag (byte at 0x50). Set, the chunk replaces the shader constants with
    /// `vertex_constants`. Always 0 in retail, so no retail chunk uses them.
    pub vertex_constants_flag: bool,
    /// Four floats at 0x54 (the vertex-shader constants used when `vertex_constants_flag` is set).
    /// Not decoded further.
    pub vertex_constants: [f32; 4],
    /// The word at 0x64, next to the constants. Not decoded; always 0 in retail.
    pub vertex_constants_word: u32,
    /// Draw-list group (byte at 0x6c): 1, 2 or 3 in retail. The loader maps
    /// these to three per-view lists of the render set.
    pub list: u8,
}

/// Axis-aligned bounds: `center` and `half_extent`, so min = center -
/// half_extent and max = center + half_extent (the loader's arithmetic in
/// the block box merge).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub center: [f32; 3],
    pub half_extent: [f32; 3],
}

/// A vertex buffer and its index buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Buffers<'a> {
    /// Little-endian `u16` indices, `index_count() * 2` bytes.
    pub indices: &'a [u8],
    /// `vertex_count * stride` bytes. Position is the first float3 of each
    /// vertex (see [`Buffers::position`]).
    pub vertices: &'a [u8],
    pub vertex_count: usize,
    pub stride: usize,
}

impl Buffers<'_> {
    /// Number of `u16` indices.
    pub fn index_count(&self) -> usize {
        self.indices.len() / 2
    }

    /// The index at `i`, if it exists.
    pub fn index(&self, i: usize) -> Option<u16> {
        let at = i.checked_mul(2)?;
        let b = self.indices.get(at..at.checked_add(2)?)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    /// The position (first float3) of vertex `v`, if it exists and the
    /// stride holds it.
    pub fn position(&self, v: usize) -> Option<[f32; 3]> {
        if v >= self.vertex_count || self.stride < 12 {
            return None;
        }
        let at = v.checked_mul(self.stride)?;
        let b = self.vertices.get(at..at.checked_add(12)?)?;
        Some(crate::le::floats_of(b))
    }
}

/// The sky dome payload: one named face and its items.
#[derive(Debug, Clone, PartialEq)]
pub struct Sky<'a> {
    /// The face name, without its NUL (for example `MAPS/ARCTIC/10 KAIHUAVAARA/10 SUPERBOWL.KSH`).
    pub name: &'a [u8],
    pub items: Vec<SkyItem<'a>>,
}

/// One sky dome item. Layout from `HH22_FillSkyItem` (`0x10058a90`, kind 0).
#[derive(Debug, Clone, PartialEq)]
pub struct SkyItem<'a> {
    /// Position floats stored before each sky dome item (kind 0, `HH22_FillSkyItem`
    /// skips them). Absent for cloud items (kind 1). Looks like a position, not verified.
    pub centre: Option<[f32; 3]>,
    /// Flags: 0x15 (24-byte vertices) or 0x35 (32-byte vertices) in retail.
    pub flags: u32,
    /// Second word (render-state key, not resolved).
    pub kind: u32,
    /// Texture index into `[TextureInfo]`, present with flag 0x10.
    pub texture0: Option<u32>,
    /// Second texture index, present with flag 0x20.
    pub texture1: Option<u32>,
    /// Word between the vertex size and the index count. Not decoded.
    pub unknown: u32,
    /// Index buffer and vertex buffer (`vertex size` bytes per vertex).
    pub buffers: Buffers<'a>,
}

/// The object lanes payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Objects<'a> {
    pub lanes: Vec<Lane<'a>>,
}

/// One lane (one `Block11` outer iteration; the loader keeps four bytes of
/// lane state per iteration).
#[derive(Debug, Clone, PartialEq)]
pub struct Lane<'a> {
    /// Lane flag. Items follow only when it is non-zero.
    pub flag: u32,
    /// The two counted item runs (`n` items, then `m` items). Both are
    /// empty when `flag` is zero.
    pub runs: [Vec<ObjectItem<'a>>; 2],
}

/// One KObjectBlock item.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectItem<'a> {
    /// Item flags. Bit 0x10 and bit 0x20 add one extra word each. Retail
    /// uses 0x05 and 0x15 (and 0x35 is handled by the loader).
    pub flags: u32,
    /// Render-state flag byte (`kind2` in the loader). See [`KindState`].
    pub kind: u32,
    /// Present when `flags & 0x10`. For flags `0x15` (almost every item) this
    /// is the item's texture index into the stage's `[TextureInfo]` list; see
    /// [`ObjectItem::texture_index`].
    pub extra10: Option<u32>,
    /// Present when `flags & 0x20`.
    pub extra20: Option<u32>,
    /// Six floats, present when the has-box word is non-zero. Never set in
    /// retail.
    pub box_words: Option<[f32; 6]>,
    /// Four words, present with `box_words` when `flags & 0x10`.
    pub box_extra: Option<[u32; 4]>,
    /// Stored word the loader advances past without reading
    /// (`HH21_Block11`, `0x100569f0`). Values seen: 322 with 36-byte
    /// vertices, 66 with 28-byte vertices. Meaning unknown.
    pub unknown: u32,
    /// The first index buffer and the vertex buffer. Triangle list.
    pub buffers: Buffers<'a>,
    /// The second index buffer, if any.
    pub second_indices: SecondIndices<'a>,
    /// Bounds of the item's vertices (checked against every retail item).
    pub bounds: Bounds,
}

/// Type 1, subtype 6: interactive face objects from `HH21_Block16`
/// (`HedgeHog3D`, `0x10057fd0`).
#[derive(Debug, Clone, PartialEq)]
pub struct InteractiveObjects<'a> {
    pub objects: Vec<InteractiveObject<'a>>,
}

/// One named interactive object. Its draw records have a counted list of
/// words and matrices used by runtime visibility; their mapping is unresolved.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractiveObject<'a> {
    pub name: &'a [u8],
    /// Stored at byte offset `+0x49`; meaning not established.
    pub flag: u8,
    pub draws: Vec<InteractiveDraw<'a>>,
    pub visibility: Vec<InteractiveVisibility>,
}

/// One interactive vertex/index buffer record. Texture keys are manager keys,
/// not the stage texture indices used by `(1,0)` and `(1,1)`.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractiveDraw<'a> {
    pub flags: u32,
    /// Unconsumed loader word following the draw flags (13 or 49 in retail).
    pub unknown: u32,
    pub texture0: Option<u32>,
    pub texture1: Option<u32>,
    pub vertex_size: usize,
    /// The other pool word; retail stores 322 for every `(1,6)` draw.
    pub stride: u32,
    pub indices: &'a [u8],
    pub vertex_count: usize,
    pub vertices: &'a [u8],
}

/// Visibility record paired with an interactive draw object.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractiveVisibility {
    /// Unresolved word preceding the matrix in the loader's record.
    pub unknown: u32,
    pub matrix: [f32; 16],
}

impl ObjectItem<'_> {
    /// The texture index into the stage's `[TextureInfo]` list (`Texture<N>`).
    /// Set for flags `0x15`, where the loader's factory looks it up
    /// (`HH_TM2_0DA0_Factory`). `None` for other flags.
    pub fn texture_index(&self) -> Option<u32> {
        if self.flags == 0x15 {
            self.extra10
        } else {
            None
        }
    }

    /// The vertex attributes for this item's flags. See [`ItemVertex`].
    pub fn vertex_layout(&self) -> Option<ItemVertex> {
        ItemVertex::for_flags(self.flags)
    }
}

/// Type 2, subtype 10: visibility entries, from `HH21_Block2a`
/// (`HedgeHog3D`, `0x10056d90`). The loader reads a count and, per entry,
/// two vectors, storing them plus the entry index (the lane) into a
/// visibility block filed in the world's `+0x3a0` holder vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Visibility {
    /// One entry per record; the lane index is the position in this vector.
    pub entries: Vec<VisibilityEntry>,
}

/// One `(2, 10)` record: two vectors of six stream words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibilityEntry {
    pub a: [f32; 3],
    pub b: [f32; 3],
}

/// Type 2, subtype 11: car start location, from `HH_H23_Block2b`
/// (`HedgeHog3D`, `0x10059990`). Always 88 bytes: the loader skips 0x40
/// bytes (a 4x4 matrix), then reads two vectors into the world bounds at
/// `+0x654`/`+0x660`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StartLocation {
    /// The 16 skipped floats (a 4x4 matrix; its translation equals
    /// `position` in every retail file).
    pub matrix: [f32; 16],
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

/// Type 2, subtype 12: driveline points, from `HH21_Block2c`
/// (`HedgeHog3D`, `0x10056b20`). Each sampled point equals a `.trk`
/// `(3, 0x14)` driveline position with y and z swapped (`.lbs` is y-up,
/// `.trk` z-up) in all 86 retail files. The loader also sums the arc
/// lengths between consecutive points.
#[derive(Debug, Clone, PartialEq)]
pub struct DrivelinePoints {
    pub points: Vec<[f32; 3]>,
}

/// Type 2, subtype 14: animation objects, from `HH_H23_Block2e`
/// (`HedgeHog3D`, `0x10059230`). Per item the loader reads a name, three
/// words, two material names, four vectors, one vector and a packed color,
/// then files the item under its name.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationObjects<'a> {
    pub items: Vec<AnimationObject<'a>>,
}

/// One `(2, 14)` item. Retail names are animation objects (marshals,
/// birds, `... AnimObject`); the two material names are `.LUA` script
/// paths. The handler files each item opaquely under its name, so the vector
/// roles below are read off retail values, not loader names: `vectors[0]` is
/// a track-scale position, `vectors[1]` euler angles in degrees (yaw-only in
/// about half the items, pitch/yaw/roll in camera items), `vectors[2]` is a
/// unit scale in all but one of 7,041 items, `vectors[3]` is a second point
/// shared within object groups (up to ~4 km from the first), and
/// `vectors[4]` is a group-shared extent triple. The name words read as
/// (object id, flags, group id): the third word is shared within groups.
/// They stay a plain array here since the loader does not name them.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationObject<'a> {
    pub name: &'a [u8],
    pub dwords: [u32; 3],
    pub material_a: &'a [u8],
    pub material_b: &'a [u8],
    pub vectors: [[f32; 3]; 5],
    /// Packed color word, split by the loader into four `/255` floats.
    pub color: u32,
}

/// Type 2, subtype 15: container objects, from `HH_H23_Block2f`
/// (`HedgeHog3D`, `0x10059690`). Per item the loader reads a name and
/// three words, then files the item.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerObjects<'a> {
    pub items: Vec<ContainerObject<'a>>,
}

/// One `(2, 15)` item: a name and three words, meanings not established.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerObject<'a> {
    pub name: &'a [u8],
    pub dwords: [u32; 3],
}

/// Type 2, subtype 16: track vectors, from `HH_H23_Block210`
/// (`HedgeHog3D`, `0x100597c0`). Always 24 bytes: the loader reads two
/// vectors into the loader head words and no traced consumer reads them
/// back. In retail the first vector is a high-altitude track-scale point and
/// the second a unit vector, and the unit vector is shared across stages of
/// the same rally (consistent with a per-rally sun direction, unproven).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackVectors {
    pub vectors: [[f32; 3]; 2],
}

/// Type 2, subtype 18: visible-object vectors, from `HH_H23_Block212`
/// (`HedgeHog3D`, `0x10059850`). Records of four vectors plus three words,
/// copied (15 words) into the `+0x3a0` holder's per-slot store at `+0x40`.
/// The payload carries no count; the loader iterates its own visibility
/// count, which equals the `(2, 10)` entry count in all 86 retail files.
#[derive(Debug, Clone, PartialEq)]
pub struct VisibleObjects {
    pub records: Vec<VisibleRecord>,
}

/// One `(2, 18)` record: four vectors and three words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibleRecord {
    pub vectors: [[f32; 3]; 4],
    pub dwords: [u32; 3],
}

/// Type 1, subtype 4: clipping planes, from `FUN_100590b0`
/// (`HedgeHog3D`, `0x100590b0`). Each of the three arrays is prefixed by its
/// own count: `n` six-byte pairs, `m` six-byte pairs, then `k` float triples.
/// The handler copies all three into heap buffers, builds vertex buffers
/// from the counts via `FUN_10028c80`, then frees all three buffers, so the
/// triples never reach a stored object. The six-byte pairs read as u16
/// triples (triangle-soup patterns like `0, 1, 2, 3, 2, 1, ...`) whose
/// maximum is below the triple count in every retail file; the triples read
/// as track-scale positions. Neither use is proven, so both stay raw here.
#[derive(Debug, Clone, PartialEq)]
pub struct ClippingPlanes {
    /// First `n` six-byte entries (u16 triples by value).
    pub a: Vec<[u8; 6]>,
    /// Next `m` six-byte entries (u16 triples by value).
    pub b: Vec<[u8; 6]>,
    /// Final `k` float triples (positions by value).
    pub positions: Vec<[f32; 3]>,
}

/// Type 1, subtypes 7 and 8: reflection and water faces. One shared file
/// layout, two handlers: `HH21_Block17` (`0x100578d0`, reflection) and
/// `HH21_Block18` (`0x10057080`, water). Both read a face count, then per
/// face a name, one skipped byte, and an item count. Reflection advances past
/// the sign-gated float pairs without storing them; water stores them into
/// the item at `+0x48`..`+0x5c`.
#[derive(Debug, Clone, PartialEq)]
pub struct Water<'a> {
    pub faces: Vec<SurfaceFace<'a>>,
}

/// One `(1, 7)` payload: reflection faces, same layout as [`Water`].
#[derive(Debug, Clone, PartialEq)]
pub struct Reflection<'a> {
    pub faces: Vec<SurfaceFace<'a>>,
}

/// One named water/reflection face.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceFace<'a> {
    /// Face name without its NUL (`RIVER*`, `WATER*`, `STREAM*`, `LAKE WATER`,
    /// `WATERBOWL`, `WATER_REFLECTION`, `REFLEXSKY`, `REFLECTION`).
    pub name: &'a [u8],
    pub items: Vec<SurfaceItem<'a>>,
}

/// One water/reflection item. The flag bits select words exactly as the
/// handlers read them: an unconditional word, then per set bit of
/// 0x10/0x20 a key word, per set bit of 0x40 a key word, and each key group
/// is followed by a float pair when the sign bit (0x80) is set. Retail flags
/// are 0x15, 0x35, 0x57, 0xd7 and 0xf7. The keys are small integers resolved
/// through the texture manager's maps (roles unproven); the sign-gated
/// pairs are small floats (0.1/-0.1 and 0.04/0.015 style). Stride and the
/// trailing unknown word follow the flags exactly in retail: 0x15 gives 24
/// bytes and 322, 0x35 gives 32 and 578, 0x57 and 0xd7 give 48 and 338,
/// 0xf7 gives 56 and 850.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceItem<'a> {
    pub flags: u32,
    /// Unconditional word after the flags (13 or 49 in retail: a texture
    /// key candidate). The loader advances past it; its use is unproven.
    pub key0: u32,
    /// Key word for flag 0x10, with its sign-gated float pair.
    pub key10: SurfaceKey,
    /// Key word for flag 0x20, with its sign-gated float pair.
    pub key20: SurfaceKey,
    /// Key word for flag 0x40 (0 or 1 in retail: a specular key candidate),
    /// with its sign-gated float pair.
    pub key40: SurfaceKey,
    /// Vertex size, trailing unknown word, index buffer (u16) and vertices.
    pub buffers: SurfaceBuffers<'a>,
}

/// A flag-gated key word with its sign-gated float pair.
pub type SurfaceKey = Option<(u32, Option<[f32; 2]>)>;

/// The trailing buffer triple of a [`SurfaceItem`]: vertex size, the unknown
/// word, u16 indices and raw vertices.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceBuffers<'a> {
    pub vertex_size: usize,
    /// Word between the vertex size and the index count. Flag-determined in
    /// retail (322/578/338/850); meaning unknown.
    pub unknown: u32,
    pub indices: &'a [u8],
    pub vertices: &'a [u8],
}

/// Type 0, subtype 23: per-texture words, from `FUN_10059a50`
/// (`HedgeHog3D`, `0x10059a50`). Each record repeats four words and the
/// texture name. The words equal the stage `.ini` section of the same name
/// in all 2,932 retail records: `MipLevels`, `Dynamic`, `OneBitOpacity`,
/// `OpacityMap` (seven `FENCE01.tga` records carry a `MipLevels` the ini
/// disagrees with). The texture loader ignores these words and reads the
/// ini instead, so they only take effect through the ini.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureTable<'a> {
    pub entries: Vec<TextureTableEntry<'a>>,
}

/// One `(0, 23)` record.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureTableEntry<'a> {
    /// Equals the ini `MipLevels` except for seven retail `FENCE01.tga`
    /// records.
    pub mip_levels: u32,
    /// Equals the ini `Dynamic` flag in all retail records.
    pub dynamic: bool,
    /// Equals the ini `OneBitOpacity` flag in all retail records.
    pub one_bit_opacity: bool,
    /// Equals the ini `OpacityMap` flag in all retail records.
    pub opacity_map: bool,
    /// Texture name, without its NUL; matches an ini section name.
    pub name: &'a [u8],
}

/// Render-state decode of an object item's `kind` byte, as
/// `HH_TM2_0D10_FlagDecode` (`HedgeHog3D`, `0x10050d10`) reads it. The two
/// mode values are the decoded numbers, not named; the names of the modes are
/// unresolved. Retail uses kind bytes 49, 13, 37 and 25 (bits 2 and 7 are
/// always set; bit 0 is always set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindState {
    /// Bit 0. Sets the stage flag the loader's factory stores (`st[15] = 1`).
    pub flag: bool,
    /// Mode of state slot 0: 3 if bit 0x10, else 2 if bit 0x40, else 1.
    pub slot0: u8,
    /// Mode of state slot 1: 3 if bit 0x20, else 2 if bit 0x80, else 1.
    pub slot1: u8,
}

impl KindState {
    /// Decodes a `kind` byte. Only the low byte is read.
    pub fn decode(kind: u32) -> KindState {
        let kind = kind as u8;
        KindState {
            flag: kind & 0x01 != 0,
            slot0: if kind & 0x10 != 0 {
                3
            } else if kind & 0x40 != 0 {
                2
            } else {
                1
            },
            slot1: if kind & 0x20 != 0 {
                3
            } else if kind & 0x80 != 0 {
                2
            } else {
                1
            },
        }
    }
}

/// Vertex attributes of an object item, as byte offsets within a vertex.
/// Positions are float3 at 0 in both layouts (checked against the item bounds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemVertex {
    /// Float3 position.
    pub position: usize,
    /// D3DCOLOR diffuse colour, four bytes.
    pub color: usize,
    /// First texture coordinate. Float2 for flags `0x15`, and a float3 for
    /// flags `0x05` whose first two components are the UV (inferred).
    pub texcoord0: usize,
    /// Float3 second texture coordinate, flags `0x15` only. Unresolved: across
    /// 24,436 retail items 13,573 hold x = 0 with garbage-magnitude y/z
    /// (uninitialised exporter output), 702 varying items hold sane values
    /// (x in [0, 1], y in [0.8, 1.2], z in [0.003, 3.12]), and the rest mix
    /// garbage into one component. No height correlation; the consuming
    /// shader use is untraced.
    pub texcoord1: Option<usize>,
}

impl ItemVertex {
    /// The layout the loader's vertex shader for these flags uses:
    /// flags `0x15` shader 19 (36-byte vertices), flags `0x05` shader 18
    /// (28-byte vertices). `None` for other flags.
    pub fn for_flags(flags: u32) -> Option<ItemVertex> {
        match flags {
            0x15 => Some(ItemVertex { position: 0, color: 12, texcoord0: 16, texcoord1: Some(24) }),
            0x05 => Some(ItemVertex { position: 0, color: 12, texcoord0: 16, texcoord1: None }),
            _ => None,
        }
    }
}

/// The second index buffer of an object item (its flag byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecondIndices<'a> {
    /// Flag byte 0: no second buffer.
    None,
    /// Flag byte 1: the second buffer repeats the first buffer's indices.
    /// No bytes are stored for it (inferred from the loader's copy).
    SameAsFirst,
    /// Flag byte 2: its own `u16` indices.
    Own(&'a [u8]),
}

/// Parse failure with the byte offset it was found at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A read needed more bytes than its chunk or the file holds.
    Truncated { offset: usize, needed: usize, len: usize },
    /// A chunk header is not the 8-byte `(type, subtype)` pair the loader
    /// reads.
    Header { offset: usize, len: usize },
    /// A value the loader would not accept, or a payload with bytes left
    /// over.
    Malformed { offset: usize, what: &'static str },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { offset, needed, len } => {
                write!(f, "truncated at offset {offset}: need {needed} bytes but only {len} are available")
            }
            Error::Header { offset, len } => {
                write!(f, "chunk at offset {offset} has a {len}-byte header, expected 8")
            }
            Error::Malformed { offset, what } => write!(f, "malformed at offset {offset}: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// Builds [`Error::Truncated`]. The reader reports the bytes left in scope
/// as `len` (see `le::Reader::within`), never the file length.
impl From<crate::le::Truncated> for Error {
    fn from(t: crate::le::Truncated) -> Self {
        Error::Truncated { offset: t.offset, needed: t.needed, len: t.len }
    }
}

#[cfg(test)]
mod tests;

//! The `.lbs` chunk walk and the payload readers.

use super::*;
use crate::le::Reader;

/// A read position inside `bytes`, valid up to `end`. Errors report the
/// bytes left in scope, not the file length. Offsets are absolute file offsets.
type Cursor<'a> = Reader<'a, Error>;

fn malformed(c: &Cursor<'_>, what: &'static str) -> Error {
    Error::Malformed { offset: c.pos(), what }
}

fn bounds(c: &mut Cursor<'_>) -> Result<Bounds, Error> {
    Ok(Bounds { center: c.f32s::<3>()?, half_extent: c.f32s::<3>()? })
}

/// Parses a whole `.lbs` buffer. Chunks must tile it exactly.
pub fn parse(bytes: &[u8]) -> Result<Lbs<'_>, Error> {
    let mut chunks = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let mut head = Cursor::within(bytes, pos, bytes.len());
        let header_len = head.u32()?;
        if header_len != 8 {
            return Err(Error::Header { offset: pos, len: header_len as usize });
        }
        let kind = (head.u32()?, head.u32()?);
        let payload_len = head.u32()? as usize;
        let payload_start = head.pos();
        let payload = head.take(payload_len)?;
        let payload_end = payload_start + payload.len();
        chunks.push(chunk(kind, Cursor::within(bytes, payload_start, payload_end))?);
        pos = payload_end;
    }
    Ok(Lbs { chunks })
}

fn chunk<'a>(kind: (u32, u32), mut c: Cursor<'a>) -> Result<Chunk<'a>, Error> {
    let parsed = match kind {
        GEOMETRY => Chunk::Geometry(geometry(&mut c)?),
        OBJECTS => Chunk::Objects(objects(&mut c)?),
        INTERACTIVE => Chunk::Interactive(interactive_objects(&mut c)?),
        CLOUDS => Chunk::Clouds(sky(&mut c, false)?),
        SKY => Chunk::Sky(sky(&mut c, true)?),
        CLIPPING => Chunk::ClippingPlanes(clipping_planes(&mut c)?),
        REFLECTION => Chunk::Reflection(Reflection { faces: surface_faces(&mut c)? }),
        WATER => Chunk::Water(Water { faces: surface_faces(&mut c)? }),
        TEXTURE_TABLE => Chunk::TextureTable(texture_table(&mut c)?),
        VISIBILITY => Chunk::Visibility(visibility(&mut c)?),
        START_LOCATION => Chunk::StartLocation(start_location(&mut c)?),
        DRIVELINE_POINTS => Chunk::DrivelinePoints(driveline_points(&mut c)?),
        ANIMATION_OBJECTS => Chunk::AnimationObjects(animation_objects(&mut c)?),
        CONTAINER_OBJECTS => Chunk::ContainerObjects(container_objects(&mut c)?),
        TRACK_VECTORS => Chunk::TrackVectors(track_vectors(&mut c)?),
        VISIBLE_OBJECTS => Chunk::VisibleObjects(visible_objects(&mut c)?),
        _ => {
            let data = c.take(c.remaining())?;
            return Ok(Chunk::Other { kind, data });
        }
    };
    if !c.is_empty() {
        return Err(malformed(&c, "payload bytes after the last record"));
    }
    Ok(parsed)
}

fn geometry<'a>(c: &mut Cursor<'a>) -> Result<Geometry<'a>, Error> {
    let block_count = c.u32()?;
    let sky_value = c.u32()?;
    let mut blocks = Vec::new();
    for _ in 0..block_count {
        blocks.push(geometry_block(c)?);
    }
    Ok(Geometry { sky_value, blocks })
}

fn geometry_block<'a>(c: &mut Cursor<'a>) -> Result<GeometryBlock<'a>, Error> {
    let mut views = [None; 9];
    for (slot, view) in views.iter_mut().enumerate() {
        *view = geometry_view(c, VIEW_STRIDES[slot])?;
    }
    let chunk_count = c.u32()?;
    let mut chunks = Vec::new();
    for _ in 0..chunk_count {
        let at = c.pos();
        let record: &[u8; RENDER_CHUNK_LEN] =
            c.take(RENDER_CHUNK_LEN)?.try_into().map_err(|_| c.truncated(RENDER_CHUNK_LEN))?;
        chunks.push(render_chunk(record, at)?);
    }
    let bounds = bounds(c)?;
    Ok(GeometryBlock { views, chunks, bounds })
}

/// One view: an index count, its `u16` indices, a vertex count, and the
/// vertices. A zero index count is an empty view and is followed by one more
/// word (always 0 in retail, ignored by the loader).
fn geometry_view<'a>(c: &mut Cursor<'a>, stride: usize) -> Result<Option<Buffers<'a>>, Error> {
    let index_count = c.u32()?;
    if index_count == 0 {
        c.u32()?;
        return Ok(None);
    }
    if index_count & 0x8000_0000 != 0 {
        return Err(malformed(c, "view index count has bit 31 set"));
    }
    let indices = c.take((index_count as usize).saturating_mul(2))?;
    let vertex_count = c.u32()?;
    if vertex_count & 0xf000_0000 != 0 {
        return Err(malformed(c, "view vertex count has flag bits set"));
    }
    let vertex_count = vertex_count as usize;
    let vertices = c.take(vertex_count.saturating_mul(stride))?;
    Ok(Some(Buffers { indices, vertices, vertex_count, stride }))
}

fn render_chunk(rec: &[u8; RENDER_CHUNK_LEN], offset: usize) -> Result<RenderChunk, Error> {
    // Record byte 0x34 is never read back (`HH21_Block10`, `0x100553b0`, has
    // no `+0x34` read) and is always 1 in retail. Record byte 0x6d varies
    // (0xff most often) and is read twice there: once for a debug `sprintf`
    // and once gating a texture-manager map find.
    let view = word(rec, 0x00) as usize;
    if view >= VIEW_STRIDES.len() {
        return Err(Error::Malformed { offset, what: "render chunk view is not a slot 0..=8" });
    }
    let texture_count = word(rec, 0x44);
    if texture_count > 2 {
        return Err(Error::Malformed { offset, what: "render chunk has more than two textures" });
    }
    let textures = (0..texture_count as usize).map(|i| word(rec, 0x48 + 4 * i)).collect();
    Ok(RenderChunk {
        view,
        vertex_shader: word(rec, 0x04),
        pixel_shader: word(rec, 0x08),
        start_index: word(rec, 0x0c),
        prim_count: word(rec, 0x10),
        vertex_count: word(rec, 0x14),
        min_index: word(rec, 0x18),
        bounds: Bounds {
            center: [float(rec, 0x1c), float(rec, 0x20), float(rec, 0x24)],
            half_extent: [float(rec, 0x28), float(rec, 0x2c), float(rec, 0x30)],
        },
        shadow: rec[0x35] != 0,
        shadow_texture: word(rec, 0x38),
        specular: rec[0x3c] != 0,
        specular_texture: word(rec, 0x40),
        textures,
        vertex_constants_flag: rec[0x50] != 0,
        vertex_constants: [float(rec, 0x54), float(rec, 0x58), float(rec, 0x5c), float(rec, 0x60)],
        vertex_constants_word: word(rec, 0x64),
        list: rec[0x6c],
    })
}

/// Little-endian word at a constant offset of a fixed-size record.
fn word(rec: &[u8; RENDER_CHUNK_LEN], at: usize) -> u32 {
    u32::from_le_bytes([rec[at], rec[at + 1], rec[at + 2], rec[at + 3]])
}

fn float(rec: &[u8; RENDER_CHUNK_LEN], at: usize) -> f32 {
    f32::from_bits(word(rec, at))
}

fn objects<'a>(c: &mut Cursor<'a>) -> Result<Objects<'a>, Error> {
    let lane_count = c.u32()?;
    let mut lanes = Vec::new();
    for _ in 0..lane_count {
        let flag = c.u32()?;
        let mut runs = [Vec::new(), Vec::new()];
        if flag != 0 {
            for run in runs.iter_mut() {
                let count = c.u32()?;
                for _ in 0..count {
                    run.push(object_item(c)?);
                }
            }
        }
        lanes.push(Lane { flag, runs });
    }
    Ok(Objects { lanes })
}

fn interactive_objects<'a>(c: &mut Cursor<'a>) -> Result<InteractiveObjects<'a>, Error> {
    let object_count = c.u32()?;
    let mut objects = Vec::new();
    for _ in 0..object_count {
        let name = c.take_cstr()?;
        let flag = c.u8()?;
        let draw_count = c.u32()?;
        let mut draws = Vec::new();
        for _ in 0..draw_count {
            let flags = c.u32()?;
            let unknown = c.u32()?;
            let texture0 = if flags & 0x10 != 0 { Some(c.u32()?) } else { None };
            let texture1 = if flags & 0x20 != 0 { Some(c.u32()?) } else { None };
            let vertex_size = c.u32()? as usize;
            let stride = c.u32()?;
            let index_count = c.u32()? as usize;
            let index_bytes =
                index_count.checked_mul(2).ok_or_else(|| malformed(c, "interactive index byte count overflow"))?;
            let indices = c.take(index_bytes)?;
            let vertex_count = c.u32()? as usize;
            let vertex_bytes = vertex_count
                .checked_mul(vertex_size)
                .ok_or_else(|| malformed(c, "interactive vertex byte count overflow"))?;
            let vertices = c.take(vertex_bytes)?;
            draws.push(InteractiveDraw {
                flags,
                unknown,
                texture0,
                texture1,
                vertex_size,
                stride,
                indices,
                vertex_count,
                vertices,
            });
        }
        let visibility_count = c.u32()?;
        let mut visibility = Vec::new();
        for _ in 0..visibility_count {
            visibility.push(InteractiveVisibility { unknown: c.u32()?, matrix: c.f32s::<16>()? });
        }
        objects.push(InteractiveObject { name, flag, draws, visibility });
    }
    Ok(InteractiveObjects { objects })
}

/// A named face and its items. `with_centre` is set for the sky dome (kind 0), whose items start
/// with a position.
fn sky<'a>(c: &mut Cursor<'a>, with_centre: bool) -> Result<Sky<'a>, Error> {
    let name = c.take_cstr()?;
    let count = c.u32()?;
    let mut items = Vec::new();
    for _ in 0..count {
        items.push(sky_item(c, with_centre)?);
    }
    Ok(Sky { name, items })
}

fn sky_item<'a>(c: &mut Cursor<'a>, with_centre: bool) -> Result<SkyItem<'a>, Error> {
    let centre = if with_centre { Some(c.f32s::<3>()?) } else { None };
    let flags = c.u32()?;
    let kind = c.u32()?;
    let texture0 = if flags & 0x10 != 0 { Some(c.u32()?) } else { None };
    let texture1 = if flags & 0x20 != 0 { Some(c.u32()?) } else { None };
    let vertex_size = c.u32()? as usize;
    let unknown = c.u32()?;
    let index_count = c.u32()? as usize;
    let indices = c.take(index_count.saturating_mul(2))?;
    let vertex_count = c.u32()? as usize;
    let vertices = c.take(vertex_count.saturating_mul(vertex_size))?;
    Ok(SkyItem {
        centre,
        flags,
        kind,
        texture0,
        texture1,
        unknown,
        buffers: Buffers { indices, vertices, vertex_count, stride: vertex_size },
    })
}

/// `(2, 10)`: a count and that many six-word (two-vector) entries.
fn visibility(c: &mut Cursor<'_>) -> Result<Visibility, Error> {
    let count = c.u32()? as usize;
    let mut entries = Vec::new();
    for _ in 0..count {
        entries.push(VisibilityEntry { a: c.f32s::<3>()?, b: c.f32s::<3>()? });
    }
    Ok(Visibility { entries })
}

/// `(2, 11)`: 0x40 skipped bytes (a 4x4 matrix) and two vectors.
fn start_location(c: &mut Cursor<'_>) -> Result<StartLocation, Error> {
    let skipped = c.take(0x40)?;
    let mut matrix = [0.0f32; 16];
    for (i, v) in matrix.iter_mut().enumerate() {
        let at = i * 4;
        *v = f32::from_bits(u32::from_le_bytes([skipped[at], skipped[at + 1], skipped[at + 2], skipped[at + 3]]));
    }
    Ok(StartLocation { matrix, position: c.f32s::<3>()?, rotation: c.f32s::<3>()? })
}

/// `(2, 12)`: a count and that many vectors.
fn driveline_points(c: &mut Cursor<'_>) -> Result<DrivelinePoints, Error> {
    let count = c.u32()? as usize;
    let mut points = Vec::new();
    for _ in 0..count {
        points.push(c.f32s::<3>()?);
    }
    Ok(DrivelinePoints { points })
}

/// `(2, 14)`: a count and that many variable-length items.
fn animation_objects<'a>(c: &mut Cursor<'a>) -> Result<AnimationObjects<'a>, Error> {
    let count = c.u32()? as usize;
    let mut items = Vec::new();
    for _ in 0..count {
        let name = c.take_cstr()?;
        let dwords = [c.u32()?, c.u32()?, c.u32()?];
        let material_a = c.take_cstr()?;
        let material_b = c.take_cstr()?;
        let mut vectors = [[0.0f32; 3]; 5];
        for v in vectors.iter_mut() {
            *v = c.f32s::<3>()?;
        }
        let color = c.u32()?;
        items.push(AnimationObject { name, dwords, material_a, material_b, vectors, color });
    }
    Ok(AnimationObjects { items })
}

/// `(2, 15)`: a count and that many name-plus-three-words items.
fn container_objects<'a>(c: &mut Cursor<'a>) -> Result<ContainerObjects<'a>, Error> {
    let count = c.u32()? as usize;
    let mut items = Vec::new();
    for _ in 0..count {
        items.push(ContainerObject { name: c.take_cstr()?, dwords: [c.u32()?, c.u32()?, c.u32()?] });
    }
    Ok(ContainerObjects { items })
}

/// `(2, 16)`: exactly two vectors.
fn track_vectors(c: &mut Cursor<'_>) -> Result<TrackVectors, Error> {
    Ok(TrackVectors { vectors: [c.f32s::<3>()?, c.f32s::<3>()?] })
}

/// `(1, 4)`: two counted six-byte runs, then a counted float-triple run.
fn clipping_planes(c: &mut Cursor<'_>) -> Result<ClippingPlanes, Error> {
    fn run(c: &mut Cursor<'_>) -> Result<Vec<[u8; 6]>, Error> {
        let count = c.u32()? as usize;
        let mut items = Vec::new();
        for _ in 0..count {
            items.push(c.take(6)?.try_into().map_err(|_| c.truncated(6))?);
        }
        Ok(items)
    }
    let a = run(c)?;
    let b = run(c)?;
    let triples = c.u32()? as usize;
    let mut positions = Vec::new();
    for _ in 0..triples {
        positions.push(c.f32s::<3>()?);
    }
    Ok(ClippingPlanes { a, b, positions })
}

/// `(1, 7)` and `(1, 8)`: a face count, then named faces.
fn surface_faces<'a>(c: &mut Cursor<'a>) -> Result<Vec<SurfaceFace<'a>>, Error> {
    let face_count = c.u32()? as usize;
    let mut faces = Vec::new();
    for _ in 0..face_count {
        let name = c.take_cstr()?;
        c.take(1)?;
        let item_count = c.u32()? as usize;
        let mut items = Vec::new();
        for _ in 0..item_count {
            items.push(surface_item(c)?);
        }
        faces.push(SurfaceFace { name, items });
    }
    Ok(faces)
}

/// One water/reflection item: flags, an unconditional word, flag-gated key
/// words each with a sign-gated float pair, then the buffer triple.
fn surface_item<'a>(c: &mut Cursor<'a>) -> Result<SurfaceItem<'a>, Error> {
    let flags = c.u32()?;
    let key0 = c.u32()?;
    let keyed = |c: &mut Cursor<'a>, bit: u32| -> Result<SurfaceKey, Error> {
        if flags & bit == 0 {
            return Ok(None);
        }
        let key = c.u32()?;
        let pair = if flags & 0x80 != 0 { Some(c.f32s::<2>()?) } else { None };
        Ok(Some((key, pair)))
    };
    let key10 = keyed(c, 0x10)?;
    let key20 = keyed(c, 0x20)?;
    let key40 = keyed(c, 0x40)?;
    let vertex_size = c.u32()? as usize;
    let unknown = c.u32()?;
    let index_count = c.u32()? as usize;
    let indices = c.take(index_count.saturating_mul(2))?;
    let vertex_count = c.u32()? as usize;
    let vertices = c.take(vertex_count.saturating_mul(vertex_size))?;
    Ok(SurfaceItem {
        flags,
        key0,
        key10,
        key20,
        key40,
        buffers: SurfaceBuffers { vertex_size, unknown, indices, vertices },
    })
}

/// `(0, 23)`: a count and that many four-word-plus-name records.
fn texture_table<'a>(c: &mut Cursor<'a>) -> Result<TextureTable<'a>, Error> {
    let count = c.u32()? as usize;
    let mut entries = Vec::new();
    for _ in 0..count {
        let mip_levels = c.u32()?;
        let dynamic = c.u32()? != 0;
        let one_bit_opacity = c.u32()? != 0;
        let opacity_map = c.u32()? != 0;
        let name = c.take_cstr()?;
        entries.push(TextureTableEntry { mip_levels, dynamic, one_bit_opacity, opacity_map, name });
    }
    Ok(TextureTable { entries })
}

/// `(2, 18)`: records of four vectors plus three words tile the payload.
fn visible_objects(c: &mut Cursor<'_>) -> Result<VisibleObjects, Error> {
    const RECORD: usize = 4 * 12 + 3 * 4;
    let mut records = Vec::new();
    while !c.is_empty() {
        if c.remaining() < RECORD {
            return Err(c.truncated(RECORD));
        }
        let mut vectors = [[0.0f32; 3]; 4];
        for v in vectors.iter_mut() {
            *v = c.f32s::<3>()?;
        }
        records.push(VisibleRecord { vectors, dwords: [c.u32()?, c.u32()?, c.u32()?] });
    }
    Ok(VisibleObjects { records })
}

fn object_item<'a>(c: &mut Cursor<'a>) -> Result<ObjectItem<'a>, Error> {
    let flags = c.u32()?;
    let kind = c.u32()?;
    let extra10 = if flags & 0x10 != 0 { Some(c.u32()?) } else { None };
    let extra20 = if flags & 0x20 != 0 { Some(c.u32()?) } else { None };
    let has_box = c.u32()? != 0;
    let stride = c.u32()? as usize;
    let unknown = c.u32()?;
    let index_count = c.u32()? as usize;
    let indices = c.take(index_count.saturating_mul(2))?;
    let second_indices = match c.u8()? {
        0 => SecondIndices::None,
        1 => SecondIndices::SameAsFirst,
        2 => {
            let count = c.u32()? as usize;
            SecondIndices::Own(c.take(count.saturating_mul(2))?)
        }
        _ => return Err(malformed(c, "object item index flag is not 0, 1 or 2")),
    };
    let vertex_count = c.u32()? as usize;
    let (box_words, box_extra) = if has_box {
        let words = c.f32s::<6>()?;
        let extra = if flags & 0x10 != 0 { Some([c.u32()?, c.u32()?, c.u32()?, c.u32()?]) } else { None };
        (Some(words), extra)
    } else {
        (None, None)
    };
    let vertices = c.take(vertex_count.saturating_mul(stride))?;
    let bounds = bounds(c)?;
    Ok(ObjectItem {
        flags,
        kind,
        extra10,
        extra20,
        box_words,
        box_extra,
        unknown,
        buffers: Buffers { indices, vertices, vertex_count, stride },
        second_indices,
        bounds,
    })
}

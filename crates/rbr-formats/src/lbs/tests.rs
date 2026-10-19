use super::*;

#[test]
fn kind_byte_decodes_like_the_flag_decoder() {
    assert_eq!(KindState::decode(49), KindState { flag: true, slot0: 3, slot1: 3 });
    assert_eq!(KindState::decode(13), KindState { flag: true, slot0: 1, slot1: 1 });
    assert_eq!(KindState::decode(0x40 | 0x80), KindState { flag: false, slot0: 2, slot1: 2 });
    assert_eq!(KindState::decode(0x100 | 0x25).slot1, 3);
}

#[test]
fn item_layouts_match_their_declarations() {
    let l = ItemVertex::for_flags(0x15).unwrap();
    assert_eq!((l.position, l.color, l.texcoord0, l.texcoord1), (0, 12, 16, Some(24)));
    assert_eq!(ItemVertex::for_flags(0x05).unwrap().texcoord1, None);
    assert_eq!(ItemVertex::for_flags(0x35), None);
}

fn framed(kind: (u32, u32), payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&kind.0.to_le_bytes());
    b.extend_from_slice(&kind.1.to_le_bytes());
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    b.extend_from_slice(payload);
    b
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn put(rec: &mut [u8], at: usize, v: u32) {
    rec[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// A render chunk record drawing one triangle from `start` on `view`.
fn render_record(view: u32, start: u32, textures: &[u32]) -> Vec<u8> {
    let mut rec = vec![0u8; RENDER_CHUNK_LEN];
    put(&mut rec, 0x00, view);
    put(&mut rec, 0x04, 12);
    put(&mut rec, 0x08, 5);
    put(&mut rec, 0x0c, start);
    put(&mut rec, 0x10, 1);
    put(&mut rec, 0x14, 3);
    rec[0x1c..0x28].copy_from_slice(&floats(&[0.5, 0.5, 0.0]));
    rec[0x28..0x34].copy_from_slice(&floats(&[0.5, 0.5, 0.0]));
    rec[0x35] = 1;
    put(&mut rec, 0x38, 2);
    put(&mut rec, 0x44, textures.len() as u32);
    for (i, t) in textures.iter().enumerate() {
        put(&mut rec, 0x48 + 4 * i, *t);
    }
    rec[0x6c] = 2;
    rec
}

/// Geometry payload with one block. View 0 holds a triangle (three
/// 16-byte vertices at (0,0,0), (1,0,0), (0,1,0)); views 1..=8 are empty.
fn geometry_payload(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut p = words(&[1, 0x4100_0000]);
    p.extend(words(&[3]));
    p.extend([0u8, 0, 1, 0, 2, 0]);
    p.extend(words(&[3]));
    p.extend(floats(&[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]));
    for _ in 1..9 {
        p.extend(words(&[0, 0]));
    }
    p.extend(words(&[chunks.len() as u32]));
    for c in chunks {
        p.extend(c);
    }
    p.extend(floats(&[0.5, 0.5, 0.0, 0.5, 0.5, 0.0]));
    p
}

/// Object payload: lane 0 is empty; lane 1 has one item in its first run
/// and nothing in its second.
fn object_payload() -> Vec<u8> {
    let mut p = words(&[2, 0, 1, 1]);
    p.extend(words(&[0x20, 5, 77, 1, 12, 66, 3]));
    p.extend([0u8, 0, 1, 0, 2, 0]);
    p.push(2);
    p.extend(words(&[2]));
    p.extend([1u8, 0, 2, 0]);
    p.extend(words(&[3]));
    p.extend(floats(&[0.0, 0.0, 0.0, 1.0, 1.0, 0.0]));
    p.extend(floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
    p.extend(floats(&[0.5, 0.5, 0.0, 0.5, 0.5, 0.0]));
    p.extend(words(&[0]));
    p
}

/// Clipping payload: no `a` pairs, one `b` pair, one triple.
fn clipping_payload() -> Vec<u8> {
    let mut p = words(&[0, 1]);
    p.extend([0u8, 0, 1, 0, 2, 0]);
    p.extend(words(&[1]));
    p.extend(floats(&[1.0, 2.0, 3.0]));
    p
}

/// Water payload: one face with one 0xd7 item (24-byte vertices).
fn water_payload() -> Vec<u8> {
    let mut p = words(&[1]);
    p.extend(b"RIVER\0");
    p.push(0);
    p.extend(words(&[1]));
    p.extend(words(&[0xd7, 13, 29]));
    p.extend(floats(&[0.04, 0.015]));
    p.extend(words(&[0]));
    p.extend(floats(&[0.05, 0.015]));
    p.extend(words(&[24, 338, 3]));
    p.extend([0u8, 0, 1, 0, 2, 0]);
    p.extend(words(&[3]));
    p.extend(vec![0u8; 3 * 24]);
    p
}

/// Texture-table payload: one record.
fn texture_table_payload() -> Vec<u8> {
    let mut p = words(&[1, 2, 1, 0, 1]);
    p.extend(b"FENCE01.tga\0");
    p
}

#[test]
fn clipping_planes_parse() {
    let file = framed(CLIPPING, &clipping_payload());
    let lbs = parse(&file).unwrap();
    let Chunk::ClippingPlanes(planes) = &lbs.chunks[0] else {
        panic!("expected clipping planes");
    };
    assert!(planes.a.is_empty());
    assert_eq!(planes.b.len(), 1);
    assert_eq!(planes.positions, [[1.0, 2.0, 3.0]]);
}

#[test]
fn water_item_parses() {
    let file = framed(WATER, &water_payload());
    let lbs = parse(&file).unwrap();
    let Chunk::Water(water) = &lbs.chunks[0] else {
        panic!("expected water");
    };
    assert_eq!(water.faces.len(), 1);
    assert_eq!(water.faces[0].name, b"RIVER");
    assert_eq!(water.faces[0].items.len(), 1);
    let item = &water.faces[0].items[0];
    assert_eq!(item.flags, 0xd7);
    assert_eq!(item.key0, 13);
    assert_eq!(item.key10, Some((29, Some([0.04, 0.015]))));
    assert_eq!(item.key20, None);
    assert_eq!(item.key40, Some((0, Some([0.05, 0.015]))));
    assert_eq!(item.buffers.vertex_size, 24);
    assert_eq!(item.buffers.unknown, 338);
    assert_eq!(item.buffers.indices.len(), 6);
    assert_eq!(item.buffers.vertices.len(), 3 * 24);
}

#[test]
fn texture_table_parses() {
    let file = framed(TEXTURE_TABLE, &texture_table_payload());
    let lbs = parse(&file).unwrap();
    let Chunk::TextureTable(table) = &lbs.chunks[0] else {
        panic!("expected texture table");
    };
    assert_eq!(table.entries.len(), 1);
    let entry = &table.entries[0];
    assert_eq!(entry.mip_levels, 2);
    assert!(entry.dynamic);
    assert!(!entry.one_bit_opacity);
    assert!(entry.opacity_map);
    assert_eq!(entry.name, b"FENCE01.tga");
}

#[test]
fn empty_file_has_no_chunks() {
    assert!(parse(&[]).unwrap().chunks.is_empty());
}

#[test]
fn other_chunks_keep_their_payload() {
    let file = framed((2, 13), &[1, 2, 3]);
    let lbs = parse(&file).unwrap();
    assert_eq!(lbs.chunks, vec![Chunk::Other { kind: (2, 13), data: &[1, 2, 3] }]);
}

#[test]
fn geometry_block_parses() {
    let file = framed(GEOMETRY, &geometry_payload(&[render_record(0, 0, &[7, 9])]));
    let lbs = parse(&file).unwrap();
    let Chunk::Geometry(geo) = &lbs.chunks[0] else {
        panic!("expected geometry");
    };
    assert_eq!(geo.sky_value, 0x4100_0000);
    assert_eq!(geo.blocks.len(), 1);
    let block = &geo.blocks[0];
    let view = block.views[0].expect("view 0");
    assert_eq!(view.index_count(), 3);
    assert_eq!(view.index(2), Some(2));
    assert_eq!(view.index(3), None);
    assert_eq!(view.vertex_count, 3);
    assert_eq!(view.position(1), Some([1.0, 0.0, 0.0]));
    assert_eq!(view.position(3), None);
    assert!(block.views[1..].iter().all(Option::is_none));
    let rc = &block.chunks[0];
    assert_eq!((rc.view, rc.start_index, rc.prim_count, rc.vertex_count, rc.min_index), (0, 0, 1, 3, 0));
    assert_eq!(rc.textures, vec![7, 9]);
    assert!(rc.shadow);
    assert_eq!(rc.shadow_texture, 2);
    assert!(!rc.specular);
    assert_eq!(rc.list, 2);
    assert_eq!(block.bounds.half_extent, [0.5, 0.5, 0.0]);
}

#[test]
fn object_item_parses() {
    let file = framed(OBJECTS, &object_payload());
    let lbs = parse(&file).unwrap();
    let Chunk::Objects(objects) = &lbs.chunks[0] else {
        panic!("expected objects");
    };
    assert_eq!(objects.lanes.len(), 2);
    assert_eq!(objects.lanes[0].flag, 0);
    assert!(objects.lanes[0].runs.iter().all(Vec::is_empty));
    assert_eq!(objects.lanes[1].flag, 1);
    assert!(objects.lanes[1].runs[1].is_empty());
    let item = &objects.lanes[1].runs[0][0];
    assert_eq!((item.flags, item.kind, item.extra10, item.extra20, item.unknown), (0x20, 5, None, Some(77), 66));
    assert_eq!(item.box_words, Some([0.0, 0.0, 0.0, 1.0, 1.0, 0.0]));
    assert_eq!(item.box_extra, None);
    assert_eq!(item.buffers.index_count(), 3);
    assert_eq!(item.buffers.vertex_count, 3);
    assert_eq!(item.buffers.position(2), Some([0.0, 1.0, 0.0]));
    assert_eq!(item.second_indices, SecondIndices::Own(&[1, 0, 2, 0]));
    assert_eq!(item.bounds.center, [0.5, 0.5, 0.0]);
}

#[test]
fn interactive_draw_and_visibility_records_parse() {
    let mut payload = words(&[1]);
    payload.extend(b"banner\0");
    payload.push(3);
    payload.extend(words(&[1])); // one draw record
    payload.extend(words(&[0x15, 49, 25, 24, 322, 3]));
    payload.extend([0u8, 0, 1, 0, 2, 0]);
    payload.extend(words(&[3]));
    payload.extend([0u8; 72]);
    payload.extend(words(&[1, 7]));
    payload.extend(floats(&[1.0; 16]));

    let raw = framed(INTERACTIVE, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::Interactive(interactive) = &file.chunks[0] else {
        panic!("expected interactive chunk");
    };
    assert_eq!(interactive.objects.len(), 1);
    let object = &interactive.objects[0];
    assert_eq!(object.name, b"banner");
    assert_eq!(object.flag, 3);
    assert_eq!(object.draws.len(), 1);
    let draw = &object.draws[0];
    assert_eq!(draw.flags, 0x15);
    assert_eq!(draw.unknown, 49);
    assert_eq!((draw.texture0, draw.texture1), (Some(25), None));
    assert_eq!((draw.vertex_size, draw.stride), (24, 322));
    assert_eq!(draw.indices, &[0, 0, 1, 0, 2, 0]);
    assert_eq!(draw.vertex_count, 3);
    assert_eq!(draw.vertices.len(), 72);
    assert_eq!(object.visibility.len(), 1);
    assert_eq!(object.visibility[0].unknown, 7);
    assert_eq!(object.visibility[0].matrix, [1.0; 16]);

    let truncated = framed(INTERACTIVE, &payload[..payload.len() - 1]);
    assert!(parse(&truncated).is_err());
}

#[test]
fn sky_item_parses() {
    let mut payload = b"SKY.KSH\0".to_vec();
    payload.extend(words(&[1]));
    payload.extend(floats(&[1.0, 2.0, 3.0]));
    payload.extend(words(&[0x15, 49, 7, 24, 0, 3]));
    payload.extend([0u8, 0, 1, 0, 2, 0]);
    payload.extend(words(&[2]));
    payload.extend(floats(&[0.0; 12]));
    let file = framed(SKY, &payload);
    let lbs = parse(&file).unwrap();
    let Chunk::Sky(sky) = &lbs.chunks[0] else {
        panic!("expected sky");
    };
    assert_eq!(sky.name, b"SKY.KSH");
    let item = &sky.items[0];
    assert_eq!((item.flags, item.kind, item.centre), (0x15, 49, Some([1.0, 2.0, 3.0])));
    assert_eq!((item.texture0, item.texture1), (Some(7), None));
    assert_eq!(item.unknown, 0);
    assert_eq!(item.buffers.index_count(), 3);
    assert_eq!(item.buffers.vertex_count, 2);
    assert_eq!(item.buffers.stride, 24);
}

#[test]
fn header_must_be_eight_bytes() {
    let mut file = framed((1, 0), &[]);
    file[0] = 4;
    assert!(matches!(parse(&file), Err(Error::Header { offset: 0, len: 4 })));
}

#[test]
fn truncated_payload_is_an_error() {
    let mut file = framed((2, 11), &[1, 2, 3, 4]);
    file.truncate(file.len() - 1);
    assert!(matches!(parse(&file), Err(Error::Truncated { .. })));
}

#[test]
fn huge_counts_fail_without_allocating() {
    let file = framed(GEOMETRY, &words(&[u32::MAX, 0]));
    assert!(matches!(parse(&file), Err(Error::Truncated { .. })));
    let file = framed(OBJECTS, &words(&[u32::MAX]));
    assert!(matches!(parse(&file), Err(Error::Truncated { .. })));
}

#[test]
fn leftover_payload_is_malformed() {
    let mut payload = geometry_payload(&[]);
    payload.extend([0u8; 4]);
    let file = framed(GEOMETRY, &payload);
    assert!(matches!(parse(&file), Err(Error::Malformed { .. })));
}

#[test]
fn render_chunk_fields_are_checked() {
    let bad_view = framed(GEOMETRY, &geometry_payload(&[render_record(9, 0, &[])]));
    assert!(matches!(parse(&bad_view), Err(Error::Malformed { .. })));
    let mut rec = render_record(0, 0, &[1, 2]);
    put(&mut rec, 0x44, 3);
    let too_many = framed(GEOMETRY, &geometry_payload(&[rec]));
    assert!(matches!(parse(&too_many), Err(Error::Malformed { .. })));
}

#[test]
fn view_index_flag_is_rejected() {
    let mut payload = words(&[1, 0]);
    payload.extend(words(&[0x8000_0001]));
    let file = framed(GEOMETRY, &payload);
    assert!(matches!(parse(&file), Err(Error::Malformed { .. })));
}

#[test]
fn object_index_flag_is_rejected() {
    let mut payload = words(&[1, 1, 1, 0, 0, 0, 0, 0, 0]);
    payload.push(3);
    let file = framed(OBJECTS, &payload);
    assert!(matches!(parse(&file), Err(Error::Malformed { .. })));
}

#[test]
fn buffer_accessors_are_bounds_checked() {
    let buffers = Buffers { indices: &[0, 0], vertices: &[0; 12], vertex_count: 1, stride: 12 };
    assert_eq!(buffers.index(usize::MAX), None);
    assert_eq!(buffers.position(usize::MAX), None);
    assert_eq!(buffers.position(1), None);
}

#[test]
fn visibility_entries_are_two_vectors() {
    let mut payload = words(&[2]);
    payload.extend(floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    payload.extend(floats(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0]));
    let raw = framed(VISIBILITY, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::Visibility(v) = &file.chunks[0] else { panic!("expected visibility chunk") };
    assert_eq!(
        v.entries,
        [
            VisibilityEntry { a: [1.0, 2.0, 3.0], b: [4.0, 5.0, 6.0] },
            VisibilityEntry { a: [7.0, 8.0, 9.0], b: [10.0, 11.0, 12.0] }
        ]
    );
    let short = framed(VISIBILITY, &words(&[1]));
    assert!(matches!(parse(&short), Err(Error::Truncated { .. })));
}

#[test]
fn start_location_is_matrix_plus_two_vectors() {
    let mut payload = floats(&[1.0; 16]);
    payload.extend(floats(&[2.0, 3.0, 4.0, 5.0, 6.0, 7.0]));
    assert_eq!(payload.len(), 88);
    let raw = framed(START_LOCATION, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::StartLocation(s) = &file.chunks[0] else { panic!("expected start location chunk") };
    assert_eq!(s.matrix, [1.0; 16]);
    assert_eq!(s.position, [2.0, 3.0, 4.0]);
    assert_eq!(s.rotation, [5.0, 6.0, 7.0]);
}

#[test]
fn driveline_points_are_counted_vectors() {
    let mut payload = words(&[2]);
    payload.extend(floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
    let raw = framed(DRIVELINE_POINTS, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::DrivelinePoints(d) = &file.chunks[0] else { panic!("expected driveline points chunk") };
    assert_eq!(d.points, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
}

#[test]
fn animation_and_container_objects_parse() {
    let mut payload = words(&[1]);
    payload.extend(b"bird\0");
    payload.extend(words(&[7, 0, 0]));
    payload.extend(b"LOAD_BIRD.LUA\0");
    payload.extend(b"RUN_BIRD.LUA\0");
    payload.extend(floats(&[1.0; 15]));
    payload.extend(words(&[0xffc8c8c8]));
    let raw = framed(ANIMATION_OBJECTS, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::AnimationObjects(a) = &file.chunks[0] else { panic!("expected animation objects chunk") };
    assert_eq!(a.items.len(), 1);
    assert_eq!(a.items[0].name, b"bird");
    assert_eq!(a.items[0].dwords, [7, 0, 0]);
    assert_eq!(a.items[0].material_a, b"LOAD_BIRD.LUA");
    assert_eq!(a.items[0].material_b, b"RUN_BIRD.LUA");
    assert_eq!(a.items[0].vectors, [[1.0; 3]; 5]);
    assert_eq!(a.items[0].color, 0xffc8c8c8);

    let mut payload = words(&[1]);
    payload.extend(b"marshal_Right_Flat\0");
    payload.extend(words(&[34462, 0, 0]));
    let raw = framed(CONTAINER_OBJECTS, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::ContainerObjects(c) = &file.chunks[0] else { panic!("expected container objects chunk") };
    assert_eq!(c.items.len(), 1);
    assert_eq!(c.items[0].name, b"marshal_Right_Flat");
    assert_eq!(c.items[0].dwords, [34462, 0, 0]);
}

#[test]
fn track_vectors_are_two_vectors() {
    let payload = floats(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    let raw = framed(TRACK_VECTORS, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::TrackVectors(t) = &file.chunks[0] else { panic!("expected track vectors chunk") };
    assert_eq!(t.vectors, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    let short = framed(TRACK_VECTORS, &floats(&[1.0]));
    assert!(matches!(parse(&short), Err(Error::Truncated { .. })));
}

#[test]
fn visible_objects_tile_the_payload() {
    let mut payload = floats(&[1.0; 12]);
    payload.extend(words(&[2, 3, 4]));
    assert_eq!(payload.len(), 60);
    let raw = framed(VISIBLE_OBJECTS, &payload);
    let file = parse(&raw).unwrap();
    let Chunk::VisibleObjects(v) = &file.chunks[0] else { panic!("expected visible objects chunk") };
    assert_eq!(v.records.len(), 1);
    assert_eq!(v.records[0].vectors, [[1.0; 3]; 4]);
    assert_eq!(v.records[0].dwords, [2, 3, 4]);
    let partial = framed(VISIBLE_OBJECTS, &floats(&[1.0; 13]));
    assert!(matches!(parse(&partial), Err(Error::Truncated { .. })));
}

use super::*;

fn put(buf: &mut Vec<u8>, off: usize, bytes: &[u8]) {
    if buf.len() < off + bytes.len() {
        buf.resize(off + bytes.len(), 0);
    }
    buf[off..off + bytes.len()].copy_from_slice(bytes);
}

fn put_u32(buf: &mut Vec<u8>, off: usize, v: u32) {
    put(buf, off, &v.to_le_bytes());
}

fn node(buf: &mut Vec<u8>, off: usize, flags: u32, pointer: u32) {
    put(buf, off, &[0u8; 0x18]);
    put_u32(buf, off + 0x18, flags);
    put_u32(buf, off + 0x1c, pointer);
}

/// One cell grafted onto the root's first child, with a single triangle.
///
/// Offsets: file header 0x00, root header 0x10, root tree 0x38 (pair at
/// 0x58), table 0x98, cell 0x9c (vertices 0xb0, tree 0xd4, pair 0xf4,
/// triangle 0x134). Total 0x148 bytes.
fn minimal() -> Vec<u8> {
    let mut b = vec![0u8; 0x148];
    put_u32(&mut b, 0, MAGIC);
    put_u32(&mut b, 4, 0x10);
    put_u32(&mut b, 8, 1);
    put_u32(&mut b, 12, 0x98);
    // Root header: type 4/3, tree at +0x28, no meshes.
    b[0x10] = ROOT_SUB;
    b[0x11] = ROOT_MODE;
    put_u32(&mut b, 0x10 + 0x10, 0x28);
    put_u32(&mut b, 0x10 + 0x14, 0x28);
    // Root tree: inner node with the pair at +0x20, two placeholders.
    node(&mut b, 0x38, 0, 0x20);
    node(&mut b, 0x58, PLACEHOLDER_MASK, 0);
    node(&mut b, 0x78, PLACEHOLDER_MASK, 0);
    put_u32(&mut b, 0x98, 0x9c);
    // Cell: flags depth 1, path bit 0; vertices; tree at +0x38.
    b[0x9c] = 4;
    b[0x9d] = CELL_MODE;
    put_u32(&mut b, 0x9c + 4, 0x0100_0000);
    put_u32(&mut b, 0x9c + 8, 3);
    put_u32(&mut b, 0x9c + 12, 0x14);
    put_u32(&mut b, 0x9c + 16, 0x38);
    for (i, v) in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]].iter().enumerate() {
        for (j, f) in v.iter().enumerate() {
            put(&mut b, 0xb0 + i * 12 + j * 4, &f.to_le_bytes());
        }
    }
    node(&mut b, 0xd4, 0, 0x20);
    node(&mut b, 0xf4, 1, 0x60);
    node(&mut b, 0x114, 0, 0);
    let tri = [0u8, 0, 1, 0, 2, 0, 0, 0, 0, 0, 7, 8, 1, 2, 3, 4, 5, 6];
    put(&mut b, 0x134, &tri);
    b
}

#[test]
fn minimal_file_parses_and_grafts() {
    let file = parse(&minimal()).unwrap();
    assert_eq!(file.root.tree.nodes.len(), 3);
    assert_eq!(file.root.tree.nodes[1].kind, NodeKind::Placeholder);
    assert_eq!(file.cells.len(), 1);
    let cell = &file.cells[0];
    assert_eq!(cell.graft, 1);
    assert_eq!(cell.vertices.len(), 3);
    assert_eq!(cell.vertices[1], [1.0, 0.0, 0.0]);
    assert_eq!(cell.tree.nodes[1].kind, NodeKind::Leaf { triangles: 0..1 });
    assert_eq!(cell.tree.nodes[2].kind, NodeKind::Empty);
    let t = cell.tree.triangles[0];
    assert_eq!(t.vertices, [0, 1, 2]);
    assert_eq!(t.materials, [7, 8]);
    assert_eq!(t.grid, [1, 2, 3, 4, 5, 6]);
}

#[test]
fn attr_parts_split_values_and_flag() {
    // Bits 0-4, 5-9, 10-14 are the per-vertex values, first vertex first.
    assert_eq!(Triangle::attr_parts(0x0000), ([0, 0, 0], false));
    assert_eq!(Triangle::attr_parts(0x8413), ([19, 0, 1], true));
    assert_eq!(Triangle::attr_parts(0x8000), ([0, 0, 0], true));
    assert_eq!(Triangle::attr_parts(0xffff), ([31, 31, 31], true));
    let tri = Triangle { vertices: [0, 1, 2], attrs: [0x03e0, 0x8001], materials: [0, 0], grid: [0; 6] };
    assert_eq!(tri.attr_a(), ([0, 31, 0], false));
    assert_eq!(tri.attr_b(), ([1, 0, 0], true));
}

/// `relocate` on [`minimal`] at `0x10000000`: five words change (the
/// two tree pointers, the cell's tree pointer and leaf pointer, and
/// the 32 graft bytes copied over the placeholder).
#[test]
fn relocate_minimal_exact() {
    const BASE: u32 = 0x1000_0000;
    let mut b = minimal();
    let done = relocate(&mut b, BASE).unwrap();
    assert_eq!(done, Relocated { slotted: 1, grafted: 1, skipped: 0 });
    let mut want = minimal();
    put_u32(&mut want, 0x54, BASE + 0x58);
    want[0x9d] = 2;
    put_u32(&mut want, 0xf0, BASE + 0xf4);
    put_u32(&mut want, 0x110, BASE + 0x134);
    // The graft copies the cell tree root after its own relocation,
    // so it carries the absolute pointer.
    let head: Vec<u8> = want[0xd4..0xf4].to_vec();
    want[0x58..0x78].copy_from_slice(&head);
    assert_eq!(b, want);
    // The placeholder is gone: the grafted node is the cell's inner
    // root with bit 21 clear.
    assert_eq!(u32_at(&b, 0x58 + 0x18).unwrap(), 0);
    assert_eq!(u32_at(&b, 0x58 + 0x1c).unwrap(), BASE + 0xf4);
}

/// Relocating at two bases shifts exactly the absolute words.
#[test]
fn relocate_delta_is_base_difference() {
    const B1: u32 = 0x1000_0000;
    const B2: u32 = 0x2000_0000;
    let mut b1 = minimal();
    let mut b2 = minimal();
    relocate(&mut b1, B1).unwrap();
    relocate(&mut b2, B2).unwrap();
    // Absolute words live at 0x54, 0x74, 0xf0 and 0x110.
    let abs = |i: usize| [0x54, 0x74, 0xf0, 0x110].iter().any(|o| (*o..*o + 4).contains(&i));
    for (i, (x, y)) in b1.iter().zip(b2.iter()).enumerate() {
        if abs(i) {
            continue; // checked word-wise below
        }
        assert_eq!(x, y, "byte {i:#x} differs across bases");
    }
    for off in [0x54, 0x74, 0xf0, 0x110] {
        let (a, c) = (u32_at(&b1, off).unwrap(), u32_at(&b2, off).unwrap());
        assert_eq!(c.wrapping_sub(a), B2.wrapping_sub(B1), "word {off:#x}");
    }
}

#[test]
fn relocate_rejects_mode2_and_bad_magic() {
    let mut b = minimal();
    b[0x9d] = 2;
    assert!(matches!(relocate(&mut b, 0x1000_0000), Err(Error::AlreadyRelocated { offset: 0x9c })));
    let mut bad = vec![0u8; 16];
    assert!(matches!(relocate(&mut bad, 0x1000_0000), Err(Error::BadMagic { .. })));
}

#[test]
fn bad_magic_rejected() {
    let mut b = minimal();
    b[0] = b'X';
    assert!(matches!(parse(&b), Err(Error::BadMagic { .. })));
}

#[test]
fn trailing_byte_is_a_gap() {
    let mut b = minimal();
    b.push(0);
    assert_eq!(parse(&b), Err(Error::Gap { offset: 0x148 }));
}

#[test]
fn vertex_index_out_of_range_rejected() {
    let mut b = minimal();
    put(&mut b, 0x134, &[3u8, 0]);
    assert!(matches!(parse(&b), Err(Error::VertexIndex { cell: 0, index: 3, vertex_count: 3 })));
}

#[test]
fn graft_path_must_reach_placeholder() {
    let mut b = minimal();
    put_u32(&mut b, 0x9c + 4, 0x0200_0000);
    assert!(matches!(parse(&b), Err(Error::BadGraft { cell: 0 })));
}

#[test]
fn truncations_and_flips_never_panic() {
    let good = minimal();
    crate::damage::never_panics(&good, &[0, 0x20, 0x80, 0xff], |b| {
        let _ = parse(b);
    });
}

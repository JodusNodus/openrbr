//! Opens and fully reads every retail `.rbz` archive, which verifies each entry's size and CRC-32.
//!
//! Runs only when `RBR_DATA` points at the extracted install tree (`original/retail-1.0/install`),
//! so `cargo test` works without game data.

use rbr_formats::{
    car_parts, cars, col, dat, dds, dls, fnc, ini, lbs, lsp, mat, rbz, repair, sga, sgc, sgs, stage_textures, times,
    trk,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::Path;

mod common;

#[test]
fn all_retail_archives_read_with_valid_crc() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let mut files = Vec::new();
    common::collect_rbz(&data_dir, &mut files);
    assert_eq!(files.len(), 93, "retail 1.0 ships 93 .rbz archives");

    let mut entries = 0;
    for path in &files {
        let mut archive = rbz::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).expect("entry");
            let name = format!("{}!{}", path.display(), entry.name().expect("ascii name"));
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(bytes.len() as u64, entry.size(), "{name}");
            entries += 1;
        }
    }
    assert_eq!(entries, 5289, "retail 1.0 archives hold 5,289 entries");
}

/// Parses every retail `.lsp` physics/setup script into a plain tree.
///
/// Runs only when `RBR_DATA` is set (see above). Retail 1.0 ships 87 `.lsp`
/// files in `Root/physics.rbz` plus 2 in `Root/misc.rbz`; each must yield
/// exactly one top-level list (see `notes/formats.md` "Physics scripts").
#[test]
fn all_retail_lsp_parse_to_one_list() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let mut files = 0;
    for archive_name in ["Root/physics.rbz", "Root/misc.rbz"] {
        let path = Path::new(&data_dir).join(archive_name);
        common::each_rbz_entry(&path, |entry_name, bytes| {
            if !entry_name.ends_with(".lsp") {
                return;
            }
            let name = format!("{}!{entry_name}", path.display());
            let text = std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            let doc = lsp::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(doc.len(), 1, "{name}: expected one top-level list");
            assert!(matches!(doc[0], lsp::Node::List(_)), "{name}: top level is a list");
            files += 1;
        });
    }
    assert_eq!(files, 89, "retail 1.0 ships 89 .lsp files");
}

/// Parses every retail `Maps/*.mat` material grid.
///
/// Runs only when `RBR_DATA` is set (see above). Every real stage ships
/// one `.mat`; the `Clouds`/`skyboxes` skybox sets do not.
#[test]
fn all_retail_mat_parse() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "mat");
    assert_eq!(files.len(), 86, "retail ships 86 .mat files");
    let mut sparse_files = 0;
    assert!(!files.is_empty(), "expected Maps/*.mat in {}", dir.display());
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = mat::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!file.entries.is_empty(), "{}: no entries", path.display());
        assert!(file.tail.is_empty(), "{}: bytes after the entries", path.display());
        let has_empty_maps = file.entries.iter().any(|entry| entry.runs.is_empty());
        if has_empty_maps {
            sparse_files += 1;
            // FUN_0046a290 accepts a zero run count and emits the selected
            // material entry with an empty run array. Retail has six such
            // all-empty USA files; keep their complete, exact layout checked.
            assert!(
                file.entries.iter().all(|entry| entry.runs.is_empty()),
                "{}: mixed empty and populated maps are not observed in retail",
                path.display()
            );
            assert_eq!(bytes.len(), 289, "{}: sparse material file size", path.display());
            assert_eq!(file.entries.len(), 9, "{}: sparse material entries", path.display());
            assert!(
                file.entries.iter().all(|entry| {
                    entry.name.starts_with("USA 2002 standard ")
                        && ["dry", "damp", "wet"].iter().any(|part| entry.name.contains(part))
                        && ["new", "normal", "worn"].iter().any(|part| entry.name.ends_with(part))
                }),
                "{}: sparse material names",
                path.display()
            );
        }
        for entry in &file.entries {
            assert!(!entry.name.is_empty(), "{}: empty name", path.display());
            for run in &entry.runs {
                assert_eq!(run.width, mat::GRID_WIDTH, "{}: {:?}", path.display(), entry.name);
                assert_eq!(run.height, mat::GRID_HEIGHT, "{}: {:?}", path.display(), entry.name);
            }
        }
    }
    assert_eq!(sparse_files, 6, "six USA material files have no runs");
    eprintln!("parsed {} .mat files", files.len());
}

/// Selects a `.mat` entry for every wear/condition combination.
///
/// Runs only when `RBR_DATA` is set (see above). All nine
/// wear (`new`/`normal`/`worn`) by wetness (`dry`/`damp`/`wet`) pairs from
/// `E11_TyreCond` (`0x46f6c0`) resolve in every retail file, and the
/// winner is always the first double match in file order (the
/// `E09_LoadMaterials` first-match rule).
#[test]
fn all_retail_mat_select() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "mat");
    assert_eq!(files.len(), 86, "retail ships 86 .mat files");
    let wear = ["new", "normal", "worn"];
    let wetness = ["dry", "damp", "wet"];
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = mat::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for w in wear {
            for m in wetness {
                let winner = mat::select(&file.entries, w, m)
                    .unwrap_or_else(|| panic!("{}: no entry for wear {w:?} wetness {m:?}", path.display()));
                let name = &file.entries[winner].name;
                assert!(name.contains(w) && name.contains(m), "{}: {name:?} selected for {w:?}/{m:?}", path.display());
                assert!(
                    !file.entries[..winner].iter().any(|e| e.name.contains(w) && e.name.contains(m)),
                    "{}: earlier entry matches {w:?}/{m:?}",
                    path.display()
                );
            }
        }
    }
    eprintln!("selected .mat entries for 9 conditions in {} files", files.len());
}

/// Parses every retail `Maps/*.col` collision file.
///
/// Runs only when `RBR_DATA` is set (see above). Every real stage ships one
/// `.col`; `col::parse` checks that header, table, root, mesh lists, cells
/// and triangle arrays tile each file exactly, that each cell grafts onto a
/// distinct root placeholder, and that every vertex index is in range.
#[test]
fn all_retail_col_parse() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "col");
    assert_eq!(files.len(), 86, "retail 1.0 ships 86 Maps/*.col");
    let (mut cells, mut triangles, mut walls) = (0, 0, 0);
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = col::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!file.cells.is_empty(), "{}: no cells", path.display());
        let grafts: std::collections::HashSet<usize> = file.cells.iter().map(|c| c.graft).collect();
        assert_eq!(grafts.len(), file.cells.len(), "{}: two cells share a placeholder", path.display());
        cells += file.cells.len();
        triangles += file.cells.iter().map(|c| c.tree.triangles.len()).sum::<usize>();
        if let Some(wall) = &file.root.brake_wall {
            walls += 1;
            // The root box is centre and half extent; its edges match the points.
            let [cx, cy, hx, hy] = wall.root_box;
            let min_x = wall.points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
            let max_x = wall.points.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
            let min_y = wall.points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
            let max_y = wall.points.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
            for (got, want) in [(cx - hx, min_x), (cx + hx, max_x), (cy - hy, min_y), (cy + hy, max_y)] {
                assert!((got - want).abs() < 0.01, "{}: box edge {got} vs points {want}", path.display());
            }
        }
    }
    assert_eq!(walls, 76, "76 retail stages define brake walls");
    eprintln!("parsed {} .col files: {cells} cells, {triangles} triangles, {walls} brake walls", files.len());
}

/// Relocates every retail `Maps/*.col` image the way `FUN_004fe570` does.
///
/// Runs only when `RBR_DATA` is set (see above). Each file is relocated
/// at two bases; the two images must differ only in the tree pointer
/// words (each shifted by the base delta), and an absolute-address walk
/// of every tree checks the loader invariants: all pointers land inside
/// the image, no root placeholder bit survives, every cell takes its own
/// slot, and every leaf carries its owner's slot.
#[test]
fn all_retail_col_relocate() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    const B1: u32 = 0x0a00_0000;
    const B2: u32 = 0x0b00_0000;
    fn word(bytes: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
    }
    /// Walks one relocated tree from its file offset, collecting the
    /// pointer-word offsets. `slot` is the owner's slot: every leaf must
    /// carry it (`None` for the root tree, whose grafted copies keep
    /// their cell's slot).
    fn walk(
        bytes: &[u8],
        base: u32,
        tree_off: usize,
        slot: Option<u32>,
        ptr_words: &mut std::collections::HashSet<usize>,
        path: &std::path::Path,
    ) {
        let mut pending = vec![tree_off];
        while let Some(off) = pending.pop() {
            assert!(off + 0x20 <= bytes.len(), "{}: node {off:#x}", path.display());
            let flags = word(bytes, off + 0x18);
            let ptr = word(bytes, off + 0x1c);
            if flags & 0x001f_ffff == 0 {
                if ptr == 0 {
                    continue;
                }
                let target = ptr.wrapping_sub(base) as usize;
                assert!(
                    ptr >= base && target + 0x40 <= bytes.len(),
                    "{}: inner pointer {ptr:#x} out of image",
                    path.display()
                );
                ptr_words.insert(off + 0x1c);
                pending.push(target);
                pending.push(target + 0x20);
            } else {
                if let Some(slot) = slot {
                    assert_eq!(flags >> 22, slot, "{}: leaf at {off:#x} carries wrong slot", path.display());
                }
                let count = (flags & 0x001f_ffff) as usize;
                let target = ptr.wrapping_sub(base) as usize;
                let end = (target + (count * 0x12 + 3)) & !3;
                assert!(ptr >= base && end <= bytes.len(), "{}: leaf pointer {ptr:#x} out of image", path.display());
                ptr_words.insert(off + 0x1c);
            }
        }
    }
    let files = common::collect_ext(&dir, "col");
    assert_eq!(files.len(), 86, "retail 1.0 ships 86 Maps/*.col");
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let parsed = col::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut img1 = bytes.clone();
        let mut img2 = bytes.clone();
        for (img, base) in [(&mut img1, B1), (&mut img2, B2)] {
            let done = col::relocate(img, base).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(done.slotted, parsed.cells.len(), "{}: slots", path.display());
            assert_eq!(done.grafted, parsed.cells.len(), "{}: grafts", path.display());
            assert_eq!(done.skipped, 0, "{}: all retail cells have vertices", path.display());
        }
        // Header words: root offset, cell count, table offset, cell headers.
        let root_off = word(&bytes, 4) as usize;
        let cell_count = word(&bytes, 8) as usize;
        let table_off = word(&bytes, 12) as usize;
        let root_tree_off = root_off + word(&bytes, root_off + 0x10) as usize;
        let mut ptr_words = std::collections::HashSet::new();
        for (img, base) in [(&img1, B1), (&img2, B2)] {
            let mut words = std::collections::HashSet::new();
            walk(img, base, root_tree_off, None, &mut words, path);
            for i in 0..cell_count {
                let cell_off = word(&bytes, table_off + 4 * i) as usize;
                assert_eq!(img[cell_off + 1], 2, "{}: cell {i} marked", path.display());
                assert_eq!(
                    u16::from_le_bytes(img[cell_off + 2..cell_off + 4].try_into().unwrap()),
                    i as u16,
                    "{}: cell {i} slot",
                    path.display()
                );
                walk(img, base, cell_off + word(&bytes, cell_off + 16) as usize, Some(i as u32), &mut words, path);
            }
            if base == B1 {
                ptr_words = words;
            } else {
                assert_eq!(words, ptr_words, "{}: same pointer words", path.display());
            }
        }
        // Every cell's graft target holds its tree root with bit 21 clear.
        for i in 0..cell_count {
            let cell_off = word(&bytes, table_off + 4 * i) as usize;
            let flags = word(&bytes, cell_off + 4);
            let cell_tree_off = cell_off + word(&bytes, cell_off + 16) as usize;
            let mut node = B1 + root_tree_off as u32;
            let mut bits = flags;
            for _ in 0..(flags >> 24) - 1 {
                let off = node.wrapping_sub(B1) as usize;
                node = word(&img1, off + 0x1c).wrapping_add((bits & 1) * 0x20);
                bits >>= 1;
            }
            let off = node.wrapping_sub(B1) as usize;
            let target = word(&img1, off + 0x1c).wrapping_add((bits & 1) * 0x20).wrapping_sub(B1) as usize;
            assert_eq!(
                img1[target..target + 0x18],
                img1[cell_tree_off..cell_tree_off + 0x18],
                "{}: cell {i} graft body",
                path.display()
            );
            assert_eq!(
                word(&img1, target + 0x18),
                word(&img1, cell_tree_off + 0x18) & !0x0020_0000,
                "{}: cell {i} graft clears bit 21",
                path.display()
            );
            assert_eq!(
                word(&img1, target + 0x1c),
                word(&img1, cell_tree_off + 0x1c),
                "{}: cell {i} graft pointer",
                path.display()
            );
        }
        // The two bases differ in exactly the pointer words, by the delta.
        for i in 0..img1.len() {
            if ptr_words.contains(&(i & !3)) && (i & !3) + 4 <= img1.len() {
                continue;
            }
            assert_eq!(img1[i], img2[i], "{}: byte {i:#x} differs across bases", path.display());
        }
        for off in &ptr_words {
            assert_eq!(
                word(&img2, *off).wrapping_sub(word(&img1, *off)),
                B2.wrapping_sub(B1),
                "{}: pointer {off:#x} not shifted by the delta",
                path.display()
            );
        }
    }
    eprintln!("relocated {} .col files at two bases", files.len());
}

/// Parses every retail `Maps/*.fnc` fence file.
///
/// Runs only when `RBR_DATA` is set (see above). Every real stage ships
/// one `.fnc`; the `Clouds`/`skyboxes` skybox sets do not.
#[test]
fn all_retail_fnc_parse() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "fnc");
    assert_eq!(files.len(), 86, "retail ships 86 .fnc files");
    let mut zero_group_files = 0;
    let mut texture_counts = [0usize; 7];
    assert!(!files.is_empty(), "expected Maps/*.fnc in {}", dir.display());
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = fnc::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let stored_group_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(file.groups.len(), stored_group_count, "{}: group count", path.display());
        if stored_group_count == 0 {
            zero_group_files += 1;
            // FUN_006038c0 skips FUN_006058c0 and leaves this retail trailer
            // unread when there are no fence groups.
            assert_eq!(file.ignored_tail, bytes[8..], "{}: ignored zero-group tail", path.display());
        } else {
            assert!(file.ignored_tail.is_empty(), "{}: unexpected unread tail", path.display());
            assert!(!file.groups.is_empty(), "{}: nonzero count has no groups", path.display());
        }
        if stored_group_count != 0 {
            assert!(matches!(file.textures.len(), 2 | 4 | 6), "{}: texture count", path.display());
            assert!(
                file.textures.iter().all(|name| name.len() == 32 && name.contains(&0)),
                "{}: fixed-width NUL-terminated texture names",
                path.display()
            );
            texture_counts[file.textures.len()] += 1;
        }
    }
    assert_eq!(zero_group_files, 24, "retail zero-group fence files");
    assert_eq!(texture_counts[2], 28, "two-texture fence tables");
    assert_eq!(texture_counts[4], 30, "four-texture fence tables");
    assert_eq!(texture_counts[6], 4, "six-texture fence tables");
    eprintln!("parsed {} .fnc files", files.len());
}

/// Parses every retail AnimData header, count table, and loader-read record.
#[test]
fn all_retail_dls_parse() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "dls");
    assert_eq!(files.len(), 86, "retail ships 86 .dls files");
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = dls::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(file.version, dls::RETAIL_VERSION, "{}: version", path.display());
        assert_eq!(file.minor_version, 0, "{}: minor version", path.display());
        assert_eq!(file.raw, bytes, "{}: raw bytes retained", path.display());
        for entry in &file.animation_entries {
            assert_eq!(entry.fields.len(), 17, "{}: animation words", path.display());
            assert_eq!(entry.replay.offset, entry.fields[16], "{}: replay offset", path.display());
        }
        // Table-1 spline keys the table-2 records match against.
        let splines: std::collections::HashSet<(u32, u32)> =
            file.track_groups.iter().map(|g| (g.header[0], g.header[1])).collect();
        // Table-2 word 0 is the trigger id `BSearch50` resolves.
        let trigger_ids: std::collections::HashSet<u32> = file.table_2.iter().map(|r| r.ids[0]).collect();
        for r in &file.table_2 {
            // `(0, 0)` is the loader's no-match default (index 0); 7 more
            // refs dangle the same way (the loader stores index 0).
            if (r.spline_group, r.spline_id) != (0, 0) && !splines.contains(&(r.spline_group, r.spline_id)) {
                eprintln!("{}: dangling trigger spline ref ({}, {})", path.display(), r.spline_group, r.spline_id);
            }
            assert!(r.rotation_deg.iter().all(|v| v.abs() <= 180.0), "{}: trigger rotation degrees", path.display());
        }
        // Words the loader resolves through `BSearch50`, plus the raw
        // sentinels it keeps (`-16`/`-15`/`-14`/`-1`): fixed, car, chase
        // and driver looks. `0` (fixed) misses the search by design and
        // 26 other refs dangle; the loader stores 0 for any miss.
        let sentinels = [0xffff_fff0u32, 0xffff_fff1, 0xffff_fff2, 0xffff_ffff];
        let mut dangling = 0;
        for c in &file.table_3 {
            assert!(trigger_ids.contains(&c.id), "{}: camera id", path.display());
            if !trigger_ids.contains(&c.look_at) && !sentinels.contains(&c.look_at) && c.look_at != 0 {
                dangling += 1;
            }
            // The loader reads only bits 0 and 1 of the mode word.
            assert_eq!(c.mode & !3, 0, "{}: camera mode", path.display());
        }
        // Stages 56 and 62 carry table-5 rows; every trigger resolves
        // (a miss trips the loader's debug break) and every name offset
        // lands in the string block.
        for r in &file.table_5 {
            assert!(trigger_ids.contains(&r.trigger), "{}: table-5 trigger", path.display());
            assert!((r.name_offset as usize) < file.string_bytes.len(), "{}: table-5 name", path.display());
        }
        if dangling != 0 {
            eprintln!("{}: {dangling} dangling camera looks", path.display());
        }
    }
    eprintln!("parsed {} .dls files", files.len());
}

/// Parses framing and recognized payloads for every retail `Maps/*.trk` physics container.
///
/// Runs only when `RBR_DATA` is set (see above). Empty files (the
/// `Clouds`/`skyboxes` skybox sets) must yield zero chunks; every nonempty
/// file has exactly the type-3 kind-0x14 driveline and kind-0x16 collision
/// mesh chunks dispatched by `FUN_0046ed20`.
#[test]
fn all_retail_trk_frame() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "trk");
    assert_eq!(files.len(), 88, "retail ships 88 .trk files");
    let mut empty_files = 0;
    assert!(!files.is_empty(), "expected Maps/*.trk in {}", dir.display());
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = trk::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        if bytes.is_empty() {
            empty_files += 1;
            assert!(file.chunks.is_empty(), "{}: empty file has chunks", path.display());
            continue;
        }
        assert_eq!(file.chunks.len(), 2, "{}: two physics chunks", path.display());
        // Chunks must tile the file exactly.
        let mut tiled = 0usize;
        for (chunk, expected_kind) in file.chunks.iter().zip([0x14, 0x16]) {
            tiled += 4 + chunk.header.len() + 4 + chunk.data.len();
            assert_eq!(chunk.header.len(), 8, "{}: header_len", path.display());
            let (t, kind) = chunk.type_kind().unwrap_or_else(|| panic!("{}: 8-byte header", path.display()));
            assert_eq!(t, 3, "{}: chunk type", path.display());
            assert_eq!(kind, expected_kind, "{}: chunk kind", path.display());
            match (&chunk.payload, kind) {
                (trk::ChunkPayload::DriveLine(data), 0x14) => {
                    assert!(data.ignored_tail.is_empty(), "{}: driveline tail", path.display());
                }
                (trk::ChunkPayload::CollisionMeshes(data), 0x16) => {
                    assert!(data.ignored_tail.is_empty(), "{}: collision tail", path.display());
                }
                _ => panic!("{}: expected parsed chunk {kind:#x}", path.display()),
            }
        }
        assert_eq!(tiled, bytes.len(), "{}: chunks tile file", path.display());
    }
    assert_eq!(empty_files, 2, "retail empty skybox physics files");
    eprintln!("parsed {} .trk files", files.len());
}

/// Checks the loader-stored `.trk` fields against every retail file.
///
/// Runs only when `RBR_DATA` is set (see above). `E09_LoadDriveLine`
/// (`0x46a530`) stores two vectors, one float and two words per record;
/// `LoadCollisionMeshes` (`0x46e300`) truncates the mesh count words to
/// u16 and keeps only each face index word's low byte. Retail exercises
/// none of the truncations (all count words fit u16, all indices fit u8)
/// and every driveline word pair is zero, so the typed reader exposes
/// every stored field exactly.
#[test]
fn all_retail_trk_stored_fields() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "trk");
    assert_eq!(files.len(), 88, "retail ships 88 .trk files");
    let (mut records, mut meshes, mut faces) = (0, 0, 0);
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = trk::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for chunk in &file.chunks {
            match &chunk.payload {
                trk::ChunkPayload::DriveLine(data) => {
                    for record in &data.records {
                        assert_eq!(record.unknown_u16, [0, 0], "{}: driveline words", path.display());
                        records += 1;
                    }
                }
                trk::ChunkPayload::CollisionMeshes(data) => {
                    for mesh in &data.meshes {
                        assert_eq!(
                            mesh.vertex_count_word,
                            u32::from(mesh.vertices.len() as u16),
                            "{}: {:?} vertex count",
                            path.display(),
                            mesh.name
                        );
                        assert_eq!(
                            mesh.face_count_word,
                            u32::from(mesh.faces.len() as u16),
                            "{}: {:?} face count",
                            path.display(),
                            mesh.name
                        );
                        for face in &mesh.faces {
                            assert!(
                                face.indices.iter().all(|&v| v <= 0xff),
                                "{}: face index truncates",
                                path.display()
                            );
                            faces += 1;
                        }
                        meshes += 1;
                    }
                }
                trk::ChunkPayload::Opaque => {}
            }
        }
    }
    assert_eq!(records, 27458, "retail driveline records");
    assert_eq!(meshes, 6295, "retail collision meshes");
    assert_eq!(faces, 68886, "retail collision faces");
    eprintln!("checked stored fields of {records} driveline records, {meshes} meshes, {faces} faces");
}

/// Every car listed in `Cars/Cars.ini` resolves to its files (matching case-insensitively, as the
/// game does), and each car folder's `.ini` files parse with the generic reader.
///
/// Retail 1.0 lists 8 cars with indexes 0..=7 (see `notes/formats.md` "Cars"). Settings files are
/// `<car>_settings.ini`; shader settings are `<ShaderSettings>_clear.ini` and `_overcast.ini`.
#[test]
fn all_retail_cars_resolve_and_ini_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);
    let text = std::fs::read(root.join("Cars/Cars.ini")).expect("Cars.ini");
    let cars = cars::parse(std::str::from_utf8(&text).expect("ascii")).expect("Cars.ini");
    assert_eq!(cars.iter().map(|c| c.index).collect::<Vec<_>>(), (0..8).collect::<Vec<_>>());

    let parse_file = |dos_path: &str| -> usize {
        let path = cars::resolve(root, dos_path).unwrap_or_else(|| panic!("missing {dos_path}"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let text = std::str::from_utf8(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let sections = ini::parse(text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!sections.is_empty(), "{}: no sections", path.display());
        sections.len()
    };

    for car in &cars {
        assert!(cars::resolve(root, &car.file_name).is_some(), "{}", car.file_name);
        assert!(cars::resolve(root, &car.texture_path).is_some(), "{}", car.texture_path);
        parse_file(&car.ini_file);
        parse_file(&car.shader_file);
        parse_file(&format!("{}_clear.ini", car.shader_settings));
        parse_file(&format!("{}_overcast.ini", car.shader_settings));

        // The car folder also holds `<car>_settings.ini` (light definitions); find it by suffix.
        let folder =
            cars::resolve(root, &car.ini_file).and_then(|p| p.parent().map(Path::to_path_buf)).expect("car folder");
        let settings: Vec<_> = std::fs::read_dir(&folder)
            .expect("read car folder")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().to_ascii_lowercase().ends_with("_settings.ini"))
            .collect();
        assert_eq!(settings.len(), 1, "{}: settings files", folder.display());
        let bytes = std::fs::read(&settings[0]).expect("settings");
        let sections = ini::parse(std::str::from_utf8(&bytes).expect("ascii")).expect("settings");
        assert!(sections.iter().any(|s| s.name == "Light0"), "{}", settings[0].display());
    }
}

/// Every retail mesh stores its bounds as a centre and a half-extent: the vertex box is centred on
/// `centre` and is `half_extent` either side (1,005 meshes, rbr-recomp `notes/formats/sgc.md`, section 5.2).
#[test]
fn all_retail_sgc_bounds_are_centre_and_half_extent() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);
    let mut meshes = 0;
    for car in retail_cars(root) {
        let path = cars::resolve(root, &car.file_name).expect("model");
        for element in sgc::parse(&std::fs::read(&path).expect("model")).expect("model") {
            let sgc::Element::Mesh(mesh) = element else {
                continue;
            };
            if mesh.vertices.is_empty() {
                continue;
            }
            meshes += 1;
            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for vertex in &mesh.vertices {
                for axis in 0..3 {
                    lo[axis] = lo[axis].min(vertex.position[axis]);
                    hi[axis] = hi[axis].max(vertex.position[axis]);
                }
            }
            for axis in 0..3 {
                let centre = (lo[axis] + hi[axis]) * 0.5;
                let half = (hi[axis] - lo[axis]) * 0.5;
                assert!(
                    (centre - mesh.centre[axis]).abs() < 2e-3 && (half - mesh.half_extent[axis]).abs() < 2e-3,
                    "{}: {} axis {axis}",
                    car.file_name,
                    mesh.node.name
                );
            }
        }
    }
    assert_eq!(meshes, 1005);
}

/// The rear door's interior twin (`i_door_b`, a child of `e_door_b`) is placed by its chained world,
/// and so is the exterior door: their offset-moved centres coincide in all eight cars. This holds
/// only with the parent chain of `sgc::chained_worlds` (the root contributes nothing). The side doors
/// are rotated and do not coincide (rbr-recomp `notes/formats/sgc.md`, section 12).
#[test]
fn all_retail_cars_rear_doors_match_their_interior_twins() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);
    for car in retail_cars(root) {
        let path = cars::resolve(root, &car.file_name).expect("model");
        let elements = sgc::parse(&std::fs::read(&path).expect("model")).expect("model");
        let worlds = sgc::chained_worlds(&elements);
        let mesh = |name: &str| {
            elements
                .iter()
                .find_map(|e| match e {
                    sgc::Element::Mesh(m) if m.node.name == name => Some(m),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{}: {name}", car.file_name))
        };
        let centre = |name: &str| {
            let m = mesh(name);
            let offset = m.node.transform.scale;
            let local = [0, 1, 2].map(|i| m.centre[i] + offset[i]);
            apply(&worlds[&m.node.id], local)
        };
        let (outside, inside) = (centre("e_door_b"), centre("i_door_b"));
        let gap = (0..3).map(|i| (outside[i] - inside[i]).powi(2)).sum::<f32>().sqrt();
        assert!(gap < 0.01, "{}: rear door twins {gap} m apart", car.file_name);
    }
}

/// `m * [p; 1]` for a 3x4 world matrix.
fn apply(m: &[[f32; 4]; 3], p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|row| (0..3).map(|k| m[row][k] * p[k]).sum::<f32>() + m[row][3])
}

/// Every car's part sections map to shaders and textures that exist. Each `FaceBlock<N>` pair is
/// contiguous from 0 and is read in full; every shader class of a mesh part is a section of the
/// car's shader settings, except `GhostCar`, which only the ghost parts (`ghost_*`) use; every
/// diffuse material of a mesh part resolves through `TextureSet0` to a DDS file that parses.
#[test]
fn all_retail_car_parts_and_diffuse_textures_resolve() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);
    let mut unknown_shaders = BTreeSet::new();
    let mut diffuse = 0;
    for car in retail_cars(root) {
        let read_ini = |dos_path: &str| {
            let path = cars::resolve(root, dos_path).unwrap_or_else(|| panic!("missing {dos_path}"));
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            ini::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        };
        let parts = read_ini(&car.ini_file);
        let shaders = read_ini(&car.shader_file);
        let classes: BTreeSet<String> =
            read_ini(&format!("{}_clear.ini", car.shader_settings)).into_iter().map(|s| s.name).collect();

        let model_path = cars::resolve(root, &car.file_name).expect("model");
        let model = sgc::parse(&std::fs::read(model_path).expect("model")).expect("model");
        let mesh_names: BTreeSet<&str> = model
            .iter()
            .filter_map(|element| match element {
                sgc::Element::Mesh(mesh) => Some(mesh.node.name.as_str()),
                _ => None,
            })
            .collect();
        for part in &parts {
            let blocks = car_parts::face_blocks(part);
            let keys = part.entries.iter().filter(|(k, _)| k.starts_with("FaceBlock")).count();
            assert_eq!(keys, 2 * blocks.len(), "{}: [{}] FaceBlock keys", car.file_name, part.name);
            // `texture_dirtmap` and `texture_scrapemap` are dirt and scrape overlay layers, not base
            // paint (milestone 5). Parts with no mesh of their own are not drawn at all.
            if !mesh_names.contains(part.name.as_str()) || part.name.starts_with("texture_") {
                continue;
            }
            for block in blocks {
                if !classes.contains(&block.shader) {
                    assert!(
                        part.name.starts_with("ghost_"),
                        "{}: {} shader {}",
                        car.file_name,
                        part.name,
                        block.shader
                    );
                    unknown_shaders.insert(block.shader.clone());
                }
                let Some(dos_path) = car_parts::diffuse_path(&shaders, &block.material) else {
                    continue;
                };
                let path = cars::resolve(root, dos_path).unwrap_or_else(|| panic!("missing {dos_path}"));
                let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                dds::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                diffuse += 1;
            }
        }
    }
    assert_eq!(unknown_shaders, BTreeSet::from(["GhostCar".to_owned()]));
    eprintln!("{diffuse} diffuse face blocks resolve to DDS files");
}

/// Parses the `CHELALIC` archives: `Physics/physics.dat` inside `physics.rbz`, and the three loose
/// `Textures/*.dat` archives that the game opens (`E09_TexBlock::CheckAll`).
///
/// Every file in `physics.dat` is compared with the `physics.rbz` copy under the same relative
/// path. 91 match. Four differ (for example `MaxTimeStep` 0.0100 in the `.dat` against 0.0070 in
/// `physics.rbz`), and the test pins that set; see `notes/formats.md` "Cars".
#[test]
fn all_retail_chelalic_archives_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);

    let mut archive = rbz::open(&root.join("Root/physics.rbz")).expect("physics.rbz");
    let dat_bytes = {
        let mut entry = archive.by_name("Physics/physics.dat").expect("physics.dat");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read physics.dat");
        bytes
    };
    let entries = dat::parse(&dat_bytes).expect("physics.dat");
    assert_eq!(entries.len(), 121, "physics.dat record count");

    let prefix = "L:/Rally_7/TargetMedia_Develop/Pc/Debug/World/";
    let mut differing = Vec::new();
    let mut files = 0;
    for entry in entries.iter().filter(|e| !e.is_dir) {
        let rel = entry.path.strip_prefix(prefix).expect("dev path prefix");
        let data = dat::file_data(&dat_bytes, entry).expect("file data");
        let mut rbz_entry = archive.by_name(rel).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let mut rbz_bytes = Vec::new();
        rbz_entry.read_to_end(&mut rbz_bytes).expect("read rbz entry");
        files += 1;
        if rbz_bytes != data {
            differing.push(rel.to_owned());
        }
    }
    assert_eq!(files, 95, "physics.dat files");
    differing.sort();
    assert_eq!(
        differing,
        ["Physics/michelin/tyres.lsp", "Physics/p_206/common.lsp", "Physics/physics.lsp", "Physics/pirelli/tyres.lsp",],
        "physics.dat files that differ from physics.rbz"
    );

    for name in ["particles", "splash", "fence"] {
        let path = root.join("Textures").join(format!("{name}.dat"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        dat::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
}

/// Parses every car's `repair.ini` from `physics.rbz` the way the game reads it. Seven cars have
/// 76 parts against the header 74; all 76 are read, because the last top-level block is read whole.
/// `mg_zr` has 69 parts and is a short-file error. See `notes/formats.md` "Cars".
#[test]
fn all_retail_repair_ini_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let path = Path::new(&data_dir).join("Root/physics.rbz");
    let mut archive = rbz::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i)?.ok().map(|n| n.into_owned()))
        .filter(|name| name.starts_with("Physics/") && name.ends_with("/repair.ini"))
        .collect();
    assert_eq!(names.len(), 8, "one repair.ini per car");
    for name in &names {
        let mut bytes = Vec::new();
        archive.by_name(name).expect("repair.ini entry").read_to_end(&mut bytes).expect("read repair.ini");
        let text = std::str::from_utf8(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The loader's first pass counts every `{` outside `;` comments and
        // warns when it differs from the header (seven files take the
        // warning path with 76; `mg_zr` takes the debug-break path with 69).
        let braces = text
            .lines()
            .map(|line| line.split(';').next().unwrap_or(""))
            .flat_map(str::split_whitespace)
            .take_while(|t| *t != "$")
            .filter(|t| *t == "{")
            .count();
        match repair::parse(text) {
            Ok(parts) => {
                assert_eq!(repair::node_count(&parts), 76, "{name}");
                assert_eq!(braces, 76, "{name}: brace count");
                // Every field the loader stores is exposed: ids, names and
                // all six timing values on every part.
                let mut stack: Vec<&repair::Part> = parts.iter().collect();
                while let Some(part) = stack.pop() {
                    assert!(!part.name.is_empty(), "{name}: empty part name");
                    for timing in [part.unmount, part.mount] {
                        for value in [timing.time, timing.min_variance, timing.max_variance] {
                            assert!(value.is_finite(), "{name}: {}", part.name);
                        }
                    }
                    stack.extend(part.children.iter());
                }
            }
            Err(e) => {
                assert_eq!(braces, 69, "{name}: brace count");
                assert!(
                    name.contains("mg_zr") && e == repair::Error::PartsShort { declared: 74, found: 69 },
                    "{name}: {e}"
                );
            }
        }
    }
}

/// Parses every retail `Maps/*.lbs` (86 stages plus the `Clouds` and `skyboxes` sets) and
/// checks the references their render chunks and object items make.
///
/// Runs only when `RBR_DATA` is set (see above). Each render chunk must draw inside its view's
/// index and vertex buffers, and its indices must fall in the range it claims. Its texture
/// indices must be inside the stage `.ini` counts. Layouts: `notes/formats.md` "Stage geometry".
///
/// Water and reflection faces share one item layout, checked here: the flag
/// word selects the stride and trailing unknown word exactly, key words
/// track bits 0x10/0x20/0x40, float pairs track the sign bit, and u16
/// indices stay inside the vertex buffer.
fn check_surface_faces(name: &std::path::Display<'_>, faces: &[lbs::SurfaceFace<'_>], water_items: &mut usize) {
    for face in faces {
        for item in &face.items {
            *water_items += 1;
            let (stride, unknown) = match item.flags {
                0x15 => (24, 322),
                0x35 => (32, 578),
                0x57 | 0xd7 => (48, 338),
                0xf7 => (56, 850),
                f => panic!("{name}: water flags {f:#x}"),
            };
            assert_eq!(item.buffers.vertex_size, stride, "{name}: water stride");
            assert_eq!(item.buffers.unknown, unknown, "{name}: water unknown");
            for (key, bit) in [(&item.key10, 0x10), (&item.key20, 0x20), (&item.key40, 0x40)] {
                assert_eq!(key.is_some(), item.flags & bit != 0, "{name}: water key bit {bit:#x}");
                assert_eq!(
                    key.is_some_and(|(_, p)| p.is_some()),
                    item.flags & (bit | 0x80) == bit | 0x80,
                    "{name}: water pair bit {bit:#x}"
                );
            }
            let vertex_count = item.buffers.vertices.len() / stride;
            assert!(item.buffers.vertices.len() % stride == 0, "{name}: water vertices");
            assert!(
                item.buffers
                    .indices
                    .chunks_exact(2)
                    .all(|e| { (u16::from_le_bytes([e[0], e[1]]) as usize) < vertex_count }),
                "{name}: water index range"
            );
        }
    }
}

#[test]
fn all_retail_lbs_parse() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let files = common::collect_ext(&dir, "lbs");
    assert_eq!(files.len(), 88, "retail 1.0 ships 88 Maps/*.lbs");
    let (mut geometry, mut blocks, mut draws, mut objects, mut items) = (0, 0, 0, 0, 0);
    let (mut clipping, mut water_items, mut texture_words) = (0, 0, 0);
    let (mut sky_items, mut cloud_items) = (0, 0);
    let (mut vis_entries, mut vis_records) = (0usize, 0usize);
    let (mut interactive_chunks, mut interactive_objects, mut interactive_draw_items) = (0usize, 0usize, 0usize);
    let mut interactive_visibility_records = 0usize;
    let mut interactive_stage_counts = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let lbs = lbs::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let text = std::fs::read(path.with_extension("ini")).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let sections =
            ini::parse(&String::from_utf8_lossy(&text)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let count = |key: &str| -> u32 {
            sections
                .iter()
                .find(|s| s.name == "TextureInfo")
                .and_then(|s| s.get(key))
                .map_or(0, |v| v.parse().expect("texture count"))
        };
        let (textures, shadows, speculars) =
            (count("NumTextures"), count("NumShadowTextures"), count("NumSpecularTextures"));
        let name = path.display();
        let (mut stage_interactive_objects, mut stage_interactive_draw_items) = (0, 0);
        for chunk in &lbs.chunks {
            match chunk {
                lbs::Chunk::Geometry(geo) => {
                    geometry += 1;
                    for block in &geo.blocks {
                        blocks += 1;
                        for rc in &block.chunks {
                            draws += 1;
                            let view = block.views[rc.view].unwrap_or_else(|| panic!("{name}: draw on an empty view"));
                            let first = rc.start_index as usize;
                            let last = first + 3 * rc.prim_count as usize;
                            assert!(last <= view.index_count(), "{name}: draw indices");
                            let (min, end) = (rc.min_index, rc.min_index + rc.vertex_count);
                            assert!(end as usize <= view.vertex_count, "{name}: draw vertices");
                            for i in first..last {
                                let v = view.index(i).expect("index in range");
                                assert!(min <= v as u32 && (v as u32) < end, "{name}: index {v}");
                            }
                            assert!(rc.textures.iter().all(|&t| t < textures), "{name}: texture");
                            assert!(!rc.vertex_constants_flag, "{name}: vertex constants are unused");
                            if rc.shadow {
                                assert!(rc.shadow_texture < shadows, "{name}: shadow texture");
                            }
                            if rc.specular {
                                assert!(rc.specular_texture < speculars, "{name}: specular");
                            }
                        }
                    }
                }
                lbs::Chunk::Objects(objs) => {
                    objects += 1;
                    for lane in &objs.lanes {
                        for run in &lane.runs {
                            for item in run {
                                items += 1;
                                let buffers = item.buffers;
                                let layout = item.vertex_layout().expect("known item flags");
                                assert_eq!(
                                    buffers.stride,
                                    if item.flags == 0x15 { 36 } else { 28 },
                                    "{name}: item stride"
                                );
                                assert!(layout.position + 12 <= buffers.stride, "{name}: layout");
                                if let Some(t) = item.texture_index() {
                                    assert!(t < textures, "{name}: item texture {t}");
                                }
                                assert!(
                                    (0..buffers.index_count())
                                        .all(|i| buffers.index(i).is_some_and(|v| (v as usize) < buffers.vertex_count)),
                                    "{name}: object index range"
                                );
                            }
                        }
                    }
                }
                lbs::Chunk::Interactive(interactive) => {
                    interactive_chunks += 1;
                    for object in &interactive.objects {
                        interactive_objects += 1;
                        stage_interactive_objects += 1;
                        assert!(!object.name.is_empty(), "{name}: interactive name");
                        assert!(
                            (1..=4).contains(&object.draws.len()),
                            "{name}: interactive draw count for {:?}",
                            String::from_utf8_lossy(object.name)
                        );
                        assert!(!object.visibility.is_empty(), "{name}: interactive object has no visibility records");
                        for draw in &object.draws {
                            interactive_draw_items += 1;
                            stage_interactive_draw_items += 1;
                            assert_eq!(draw.flags, 0x15, "{name}: interactive flags");
                            assert!(matches!(draw.unknown, 13 | 49), "{name}: interactive unknown {}", draw.unknown);
                            assert!(draw.texture0.is_some(), "{name}: interactive texture 0");
                            assert_eq!(draw.texture1, None, "{name}: interactive texture 1");
                            assert_eq!(draw.vertex_size, 24, "{name}: interactive vertex size");
                            assert_eq!(draw.stride, 322, "{name}: interactive stride");
                            assert_eq!(draw.indices.len() % 2, 0, "{name}: index bytes");
                            assert_eq!(draw.indices.len() % 6, 0, "{name}: triangle indices");
                            assert_eq!(
                                draw.vertices.len(),
                                draw.vertex_count * draw.vertex_size,
                                "{name}: interactive vertex bytes"
                            );
                            assert!(
                                draw.indices.chunks_exact(2).all(|bytes| {
                                    (u16::from_le_bytes([bytes[0], bytes[1]]) as usize) < draw.vertex_count
                                }),
                                "{name}: interactive index range"
                            );
                        }
                        for visibility in &object.visibility {
                            interactive_visibility_records += 1;
                            assert!(
                                visibility.matrix.iter().all(|value| value.is_finite()),
                                "{name}: interactive visibility matrix"
                            );
                        }
                    }
                }
                lbs::Chunk::Sky(sky) | lbs::Chunk::Clouds(sky) => {
                    let clouds = matches!(chunk, lbs::Chunk::Clouds(_));
                    for item in &sky.items {
                        sky_items += 1;
                        let buffers = item.buffers;
                        if clouds {
                            cloud_items += 1;
                        } else {
                            assert_eq!(
                                buffers.stride,
                                if item.flags == 0x15 { 24 } else { 32 },
                                "{name}: sky item stride"
                            );
                        }
                        assert!(
                            (0..buffers.index_count())
                                .all(|i| buffers.index(i).is_some_and(|v| (v as usize) < buffers.vertex_count)),
                            "{name}: sky index range"
                        );
                        for t in [item.texture0, item.texture1].into_iter().flatten() {
                            assert!(t < textures, "{name}: sky texture {t}");
                        }
                    }
                }
                lbs::Chunk::Visibility(v) => {
                    vis_entries += v.entries.len();
                }
                lbs::Chunk::StartLocation(s) => {
                    // The matrix translation is the start position.
                    assert!(
                        (s.matrix[12] - s.position[0]).abs() < 0.01
                            && (s.matrix[13] - s.position[1]).abs() < 0.01
                            && (s.matrix[14] - s.position[2]).abs() < 0.01,
                        "{name}: start matrix translation"
                    );
                }
                lbs::Chunk::DrivelinePoints(_) => {}
                lbs::Chunk::AnimationObjects(_) => {}
                lbs::Chunk::ContainerObjects(_) => {}
                lbs::Chunk::TrackVectors(t) => {
                    // The second vector is a unit direction.
                    let len = t.vectors[1].iter().map(|v| v * v).sum::<f32>().sqrt();
                    assert!((len - 1.0).abs() < 0.01, "{name}: track vector length");
                }
                lbs::Chunk::VisibleObjects(v) => {
                    vis_records += v.records.len();
                }
                lbs::Chunk::ClippingPlanes(planes) => {
                    clipping += 1;
                    // The six-byte pairs read as u16 triples below the
                    // triple count; the triples are finite positions.
                    let max_index = planes
                        .a
                        .iter()
                        .chain(&planes.b)
                        .flat_map(|e| [0, 2, 4].map(|i| u16::from_le_bytes([e[i], e[i + 1]]) as usize))
                        .max();
                    if let Some(max_index) = max_index {
                        assert!(max_index < planes.positions.len(), "{name}: clipping index range");
                    }
                    assert!(
                        planes.positions.iter().all(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e9)),
                        "{name}: clipping positions"
                    );
                }
                lbs::Chunk::Reflection(r) => {
                    check_surface_faces(&name, &r.faces, &mut water_items);
                }
                lbs::Chunk::Water(w) => {
                    check_surface_faces(&name, &w.faces, &mut water_items);
                }
                lbs::Chunk::TextureTable(table) => {
                    for entry in &table.entries {
                        texture_words += 1;
                        let section = sections
                            .iter()
                            .find(|s| s.name == String::from_utf8_lossy(entry.name).into_owned())
                            .unwrap_or_else(|| panic!("{name}: texture word without ini section"));
                        let flag =
                            |key: &str| -> bool { section.get(key).is_some_and(|v| v.eq_ignore_ascii_case("true")) };
                        // The words repeat the ini section exactly, except
                        // seven FENCE01.tga MipLevels records.
                        assert_eq!(entry.dynamic, flag("Dynamic"), "{name}: Dynamic");
                        assert_eq!(entry.one_bit_opacity, flag("OneBitOpacity"), "{name}: OneBitOpacity");
                        assert_eq!(entry.opacity_map, flag("OpacityMap"), "{name}: OpacityMap");
                        let mip_levels: u32 = section.get("MipLevels").expect("MipLevels").parse().expect("MipLevels");
                        if entry.mip_levels != mip_levels {
                            assert_eq!(entry.name, b"FENCE01.tga", "{name}: MipLevels mismatch");
                            assert!(
                                (entry.mip_levels, mip_levels) == (2, 1) || (entry.mip_levels, mip_levels) == (1, 2),
                                "{name}: MipLevels mismatch"
                            );
                        }
                    }
                }
                lbs::Chunk::Other { .. } => {}
            }
        }
        // The visibility cells: one per geometry block and object lane, each box holding its block
        // (`rbr_render::visibility`; the lane's objects may stick out).
        let cells = lbs.chunks.iter().find_map(|chunk| match chunk {
            lbs::Chunk::Visibility(v) => Some(&v.entries),
            _ => None,
        });
        let block_bounds = lbs.chunks.iter().find_map(|chunk| match chunk {
            lbs::Chunk::Geometry(geo) => Some(geo.blocks.iter().map(|block| block.bounds).collect::<Vec<_>>()),
            _ => None,
        });
        let lanes = lbs.chunks.iter().find_map(|chunk| match chunk {
            lbs::Chunk::Objects(objects) => Some(&objects.lanes),
            _ => None,
        });
        if let (Some(cells), Some(block_bounds), Some(lanes)) = (cells, block_bounds, lanes) {
            assert_eq!(cells.len(), block_bounds.len(), "{name}: a cell per geometry block");
            assert_eq!(cells.len(), lanes.len(), "{name}: a cell per object lane");
            for (i, (cell, block)) in cells.iter().zip(&block_bounds).enumerate() {
                let holds =
                    (0..3).all(|k| (block.center[k] - cell.a[k]).abs() + block.half_extent[k] <= cell.b[k] + 0.01);
                assert!(holds, "{name}: cell {i} holds its block");
            }
        }
        let stage_name = path.file_stem().and_then(|stem| stem.to_str()).expect("stage name").to_owned();
        interactive_stage_counts.push((stage_name, stage_interactive_objects, stage_interactive_draw_items));
    }
    // The `(2, 18)` record count comes from the loader's visibility count:
    // in every stage it equals the `(2, 10)` entry count.
    assert_eq!(vis_entries, vis_records, "visibility entries match visible-object records");
    assert_eq!(interactive_chunks, 76, "stages with an (1,6) chunk");
    assert_eq!(interactive_objects, 1127, "interactive objects");
    assert_eq!(interactive_draw_items, 1992, "interactive draw records");
    assert_eq!(interactive_visibility_records, 13_703, "interactive visibility records");
    assert_eq!(
        interactive_stage_counts.iter().filter(|(_, objects, _)| *objects != 0).count(),
        76,
        "all retail stages with (1,6) parse at least one interactive object"
    );
    eprintln!(
        "interactive `(1,6)`: {interactive_chunks} stages, {interactive_objects} objects, \
         {interactive_draw_items} draw records, {interactive_visibility_records} visibility records"
    );
    for (stage, objects, draws) in interactive_stage_counts.iter().filter(|(_, n, _)| *n != 0) {
        eprintln!("interactive {stage}: {objects} objects, {draws} draw records");
    }
    eprintln!(
        "parsed {} .lbs files: {geometry} geometry chunks, {blocks} blocks, {draws} render chunks, \
         {objects} object chunks, {items} object items, {sky_items} sky items ({cloud_items} cloud items), \
         {clipping} clipping chunks, {water_items} water/reflection items, {texture_words} texture words",
        files.len()
    );
}

/// Writes the meshes of `track-10_O.lbs` to `/tmp/lbs-track-10_O.obj` for a visual check.
///
/// Each geometry view of each block is one OBJ group, and each object item is one group. Faces
/// are triangle lists with indices shifted to the group's vertices. Runs only with `RBR_DATA`.
#[test]
fn lbs_track_10_o_writes_obj() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let bytes = std::fs::read(dir.join("track-10_O.lbs")).expect("track-10_O.lbs");
    let lbs = lbs::parse(&bytes).expect("track-10_O.lbs parses");
    let mut out = String::new();
    // OBJ indices are 1-based; `base` is the OBJ index of the group's first vertex.
    let mut base = 1usize;
    let mut group = |out: &mut String, name: &str, buffers: &lbs::Buffers, faces: &[usize]| {
        writeln!(out, "o {name}").unwrap();
        for v in 0..buffers.vertex_count {
            let p = buffers.position(v).expect("vertex position");
            writeln!(out, "v {} {} {}", p[0], p[1], p[2]).unwrap();
        }
        for tri in faces.chunks_exact(3) {
            let i = tri.iter().map(|&k| base + k).collect::<Vec<_>>();
            writeln!(out, "f {} {} {}", i[0], i[1], i[2]).unwrap();
        }
        base += buffers.vertex_count;
    };
    for chunk in &lbs.chunks {
        match chunk {
            lbs::Chunk::Geometry(geo) => {
                for (b, block) in geo.blocks.iter().enumerate() {
                    for (slot, view) in block.views.iter().enumerate() {
                        let Some(view) = view else { continue };
                        let mut faces = Vec::new();
                        for rc in block.chunks.iter().filter(|rc| rc.view == slot) {
                            let first = rc.start_index as usize;
                            for i in first..first + 3 * rc.prim_count as usize {
                                faces.push(view.index(i).expect("index") as usize);
                            }
                        }
                        group(&mut out, &format!("block{b}_view{slot}"), view, &faces);
                    }
                }
            }
            lbs::Chunk::Objects(objs) => {
                for (l, lane) in objs.lanes.iter().enumerate() {
                    for (r, run) in lane.runs.iter().enumerate() {
                        for (i, item) in run.iter().enumerate() {
                            let faces: Vec<usize> = (0..item.buffers.index_count())
                                .map(|k| item.buffers.index(k).expect("index") as usize)
                                .collect();
                            group(&mut out, &format!("lane{l}_run{r}_item{i}"), &item.buffers, &faces);
                        }
                    }
                }
            }
            lbs::Chunk::Sky(sky) | lbs::Chunk::Clouds(sky) => {
                for (i, item) in sky.items.iter().enumerate() {
                    let faces: Vec<usize> = (0..item.buffers.index_count())
                        .map(|k| item.buffers.index(k).expect("index") as usize)
                        .collect();
                    group(&mut out, &format!("sky_item{i}"), &item.buffers, &faces);
                }
            }
            _ => {}
        }
    }
    std::fs::write("/tmp/lbs-track-10_O.obj", out).expect("write obj");
    eprintln!("wrote /tmp/lbs-track-10_O.obj ({} vertices)", base - 1);
}

/// Resolves every texture the stage's chunks and object items reference (diffuse, shadow and
/// specular) through `TextureFilenameMap.dat` to an entry in a `*_textures.rbz` archive, with the
/// default weather and wear. The 86 `track-*` stages are checked; `Clouds` and `skyboxes` are not.
///
/// Runs only when `RBR_DATA` is set (see above). Each resolved path must exist in one of the
/// archives in `Maps`. Most resolve in the stage's own archive. Some resolve through the map to a
/// sibling variant's archive (for example `track-20_O` to `track-20_N`), and those are counted
/// separately. Layout: `notes/formats.md` "Stage geometry".
#[test]
fn all_retail_stage_textures_resolve() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let map_bytes = std::fs::read(dir.join("TextureFilenameMap.dat")).expect("TextureFilenameMap.dat");
    let map = stage_textures::FilenameMap::parse(&map_bytes).expect("texture filename map parses");
    assert_eq!(map.len(), 6970, "TextureFilenameMap.dat holds 6,970 pairs");

    // Every texture archive in Maps: lower-case file name to lower-case entry paths.
    let mut archives: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for path in common::collect_ext(&dir, "rbz") {
        let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with("_textures.rbz") {
            continue;
        }
        let mut zip = rbz::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let entries = (0..zip.len())
            .map(|i| {
                let entry = zip.by_index(i).expect("entry");
                entry.name().expect("ascii name").replace('/', "\\").to_ascii_lowercase()
            })
            .collect();
        archives.insert(name, entries);
    }
    // Entries are stored as full archive paths (`track-10_o_textures\\dry\\chair 01.dds`).
    let exists = |path: &str| -> bool {
        stage_textures::split_archive_path(path)
            .and_then(|(archive, _)| archives.get(&archive))
            .is_some_and(|entries| entries.contains(path))
    };
    let archive_of = |path: &str| stage_textures::split_archive_path(path).map(|(a, _)| a);

    let (mut own, mut sibling, mut shadows, mut specular) = (0, 0, 0, 0);
    let mut stages = 0;
    for lbs_path in common::collect_ext(&dir, "lbs") {
        let base = lbs_path.file_stem().unwrap().to_string_lossy().into_owned();
        // The sky sets (`Clouds`, `skyboxes`) load their textures through the sky loader, not here.
        if !base.starts_with("track-") {
            continue;
        }
        let own_archive = format!("{}_textures.rbz", base.to_ascii_lowercase());
        let text = std::fs::read(lbs_path.with_extension("ini")).expect("stage ini");
        let sections = ini::parse(&String::from_utf8_lossy(&text)).expect("stage ini parses");
        let find = |name: &str| sections.iter().find(|s| s.name.eq_ignore_ascii_case(name));
        stages += 1;
        let lbs_bytes = std::fs::read(&lbs_path).expect("lbs");
        let geometry = lbs::parse(&lbs_bytes).expect("lbs parses");
        // Texture indices the stage's chunks and object items reference.
        let (mut diffuse, mut shadow_ids, mut spec_ids) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for chunk in &geometry.chunks {
            match chunk {
                lbs::Chunk::Geometry(geo) => {
                    for rc in geo.blocks.iter().flat_map(|b| &b.chunks) {
                        diffuse.extend(rc.textures.iter().copied());
                        if rc.shadow {
                            shadow_ids.insert(rc.shadow_texture);
                        }
                        if rc.specular {
                            spec_ids.insert(rc.specular_texture);
                        }
                    }
                }
                lbs::Chunk::Objects(objs) => {
                    for item in objs.lanes.iter().flat_map(|l| l.runs.iter().flatten()) {
                        diffuse.extend(item.texture_index());
                    }
                }
                lbs::Chunk::Sky(sky) | lbs::Chunk::Clouds(sky) => {
                    for item in &sky.items {
                        diffuse.extend(item.texture0.into_iter().chain(item.texture1));
                    }
                }
                _ => {}
            }
        }
        for &i in &diffuse {
            let tex = find("TextureInfo").and_then(|s| s.get(&format!("Texture{i}"))).expect("texture name");
            let ground =
                find(tex).and_then(|s| s.get("IsGroundTexture")).is_some_and(|v| v.eq_ignore_ascii_case("true"));
            let request = stage_textures::texture_request(
                &base,
                tex,
                ground,
                stage_textures::DEFAULT_WEATHER,
                stage_textures::DEFAULT_WEAR,
            );
            let resolved = map.resolve(&request);
            assert!(exists(&resolved), "{base}: texture {tex} -> {resolved}");
            if archive_of(&resolved).as_deref() == Some(own_archive.as_str()) {
                own += 1;
            } else {
                sibling += 1;
            }
        }
        for &i in &shadow_ids {
            let name = find("TextureInfo").and_then(|s| s.get(&format!("ShadowTexture{i}"))).expect("shadow name");
            let resolved = map.resolve(&stage_textures::shadow_request(&base, name));
            assert!(exists(&resolved), "{base}: shadow {name} -> {resolved}");
            shadows += 1;
        }
        for &i in &spec_ids {
            let name = find("TextureInfo").and_then(|s| s.get(&format!("SpecularTexture{i}"))).expect("specular name");
            let found = stage_textures::specular_requests(
                &base,
                name,
                stage_textures::DEFAULT_WEATHER,
                stage_textures::DEFAULT_WEAR,
            )
            .iter()
            .map(|r| map.resolve(r))
            .any(|r| exists(&r));
            assert!(found, "{base}: specular {name} has no file");
            specular += 1;
        }
    }
    eprintln!(
        "{stages} stages: {own} diffuse textures in the stage archive, {sibling} in a sibling archive, \
         {shadows} shadow, {specular} specular"
    );
}

/// Runs only when `RBR_DATA` is set (see above). Every DDS in every `Maps/*_textures.rbz` must
/// parse as DXT1 or DXT3 with its mip chain filling the entry exactly. Layout: `notes/formats.md` "Textures and sprites".
#[test]
fn all_retail_dds_parse() {
    let Some(dir) = common::maps_dir() else {
        return;
    };
    let (mut files, mut dxt1, mut dxt3, mut levels) = (0, 0, 0, 0);
    for path in common::collect_ext(&dir, "rbz") {
        let name = path.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
        if !name.ends_with("_textures.rbz") {
            continue;
        }
        common::each_rbz_entry(&path, |name, bytes| {
            if !name.to_ascii_lowercase().ends_with(".dds") {
                return;
            }
            let entry_name = format!("{}!{name}", path.display());
            let dds = dds::parse(bytes).unwrap_or_else(|e| panic!("{entry_name}: {e}"));
            let chain = dds.levels();
            assert_eq!(chain.len(), dds.mip_count as usize, "{entry_name}: mip levels");
            assert_eq!(chain.iter().map(|l| l.data.len()).sum::<usize>(), dds.data.len(), "{entry_name}: mip chain");
            match dds.format {
                dds::Format::Dxt1 => dxt1 += 1,
                dds::Format::Dxt3 => dxt3 += 1,
                dds::Format::Dxt5 | dds::Format::Rgba8 => panic!("retail has only DXT1 and DXT3"),
            }
            files += 1;
            levels += chain.len();
        });
    }
    assert_eq!(files, 3485, "stage texture archives hold 3,485 DDS files");
    eprintln!("{files} DDS files: {dxt1} DXT1, {dxt3} DXT3, {levels} mip levels");
}

/// Reads `Cars/Cars.ini` and returns the eight car entries.
fn retail_cars(root: &Path) -> Vec<cars::CarEntry> {
    let text = std::fs::read_to_string(root.join("Cars/Cars.ini")).expect("Cars.ini");
    cars::parse(&text).expect("Cars.ini")
}

/// Mesh and node checks that hold for every retail car. Returns the count of nodes and roots.
fn check_car_model(name: &str, elements: &[sgc::Element], sections: &[ini::Section]) -> (usize, usize) {
    let has_section = |wanted: &str| sections.iter().any(|s| s.name == wanted);
    let mut ids = HashSet::new();
    let mut parents = Vec::new();
    let mut roots = 0;
    let mut nodes = 0;
    for element in elements {
        let (id, parent) = match element {
            sgc::Element::Mesh(mesh) => (mesh.node.id, mesh.node.parent),
            sgc::Element::Node(node) => (node.id, node.parent),
            sgc::Element::Collision(c) => (c.node.id, c.node.parent),
            sgc::Element::Unparsed { id, parent, .. } => (*id, *parent),
            sgc::Element::Ignored { .. } => continue,
        };
        nodes += 1;
        assert!(ids.insert(id), "{name}: duplicate node id {id}");
        if parent == -1 {
            roots += 1;
        } else {
            parents.push((id, parent));
        }
    }
    for (id, parent) in parents {
        assert!(ids.contains(&parent), "{name}: node {id} has unknown parent {parent}");
    }

    for element in elements {
        match element {
            sgc::Element::Mesh(mesh) => check_mesh(name, mesh, sections),
            sgc::Element::Node(node) => assert!(
                has_section(&node.name) || node.name.starts_with("Point_") || !node.name.is_empty(),
                "{name}: node {}",
                node.name
            ),
            sgc::Element::Collision(c) => {
                let part = &c.node.name;
                assert!(has_section(&part[2..]), "{name}: collision {part}")
            }
            sgc::Element::Unparsed { kind: sgc::KIND_SHADOW, name: part, .. } => {
                assert!(has_section(&part[2..]) || has_section(&format!("e_{}", &part[2..])), "{name}: shadow {part}")
            }
            _ => {}
        }
    }
    (nodes, roots)
}

/// Checks one mesh: faces partition the triangle list, indices stay in range, normals are unit
/// length, morph data matches the vertex count, and every named face is the car's `FaceBlock<N>`
/// material. Also checks that the mesh name is a section of the car's `.ini`.
fn check_mesh(car: &str, mesh: &sgc::Mesh, sections: &[ini::Section]) {
    let name = &mesh.node.name;
    let section = sections.iter().find(|s| s.name == *name);
    assert!(section.is_some(), "{car}: mesh {name} has no ini section");
    let vertex_count = mesh.vertices.len();
    assert!(mesh.uvs.is_empty() || mesh.uvs.len() == vertex_count, "{car}: {name} uvs");
    assert!(mesh.aux.iter().all(|c| *c == [255; 4]), "{car}: {name} aux stream");

    let mut next_start = 0;
    let mut triangles = 0;
    for (i, face) in mesh.faces.iter().enumerate() {
        assert_eq!(face.start_index as usize, next_start, "{car}: {name} face {i} start");
        next_start += face.prim_count as usize * 3;
        triangles += face.prim_count as usize;
        let first = face.start_index as usize / 3;
        assert_eq!(face.start_index % 3, 0, "{car}: {name} face {i} start not a triangle");
        for tri in &mesh.triangles[first..first + face.prim_count as usize] {
            for index in tri {
                let index = *index as u32;
                assert!(
                    index >= face.min_vertex && index < face.min_vertex + face.num_vertices,
                    "{car}: {name} face {i} index {index}"
                );
            }
        }
        if !face.name.is_empty() {
            let slot = section.and_then(|s| s.get(&format!("FaceBlock{i}_Material")));
            assert_eq!(slot, Some(face.name.as_str()), "{car}: {name} face {i}");
        }
    }
    assert_eq!(triangles, mesh.triangles.len(), "{car}: {name} triangle count");

    for tri in &mesh.triangles {
        assert!(tri.iter().all(|i| (*i as usize) < vertex_count), "{car}: {name} index range");
    }
    for v in &mesh.vertices {
        let len = v.normal.iter().map(|c| c * c).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-3, "{car}: {name} normal length {len}");
    }
    if mesh.damageable {
        // Zero targets in 8 retail panels; one in the rest.
        assert!(mesh.morph_targets.len() <= 1, "{car}: {name} morph targets");
        assert!(mesh.morph_targets.iter().all(|t| t.flag == 1 && t.deltas.len() == vertex_count));
    } else {
        assert!(mesh.morph_targets.is_empty(), "{car}: {name} unexpected morph");
    }
    for list in &mesh.morph_lists {
        for (index, weight) in list {
            assert!((*index as usize) < vertex_count, "{car}: {name} morph index");
            assert!(*weight > 0.0 && *weight <= 1.0, "{car}: {name} morph weight {weight}");
        }
    }
}

/// Decodes the eight retail car models and checks the structure proven by `CHH_CarLoader`
/// (`notes/formats.md` "Car models"). Runs only when `RBR_DATA` is set.
#[test]
fn all_retail_sgc_models_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);
    let cars = retail_cars(root);
    assert_eq!(cars.len(), 8);

    let mut records = 0;
    let mut by_kind: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut total_roots = 0;
    for car in &cars {
        let sgc_path = cars::resolve(root, &car.file_name).expect("model path");
        let bytes = std::fs::read(&sgc_path).expect("model bytes");
        for rec in sgc::records(&bytes).expect("records tile the file") {
            records += 1;
            *by_kind.entry((rec.kind, rec.subtype)).or_default() += 1;
        }
        let elements = sgc::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", sgc_path.display()));
        let ini_path = cars::resolve(root, &car.ini_file).expect("car ini path");
        let text = std::fs::read_to_string(&ini_path).expect("car ini");
        let sections = ini::parse(&text).expect("car ini parse");
        let (_, roots) = check_car_model(&car.file_name, &elements, &sections);
        total_roots += roots;
    }

    assert_eq!(records, 5199, "records over the eight models");
    let expected = [
        ((sgc::KIND_GEOMETRY, sgc::SUBTYPE_STATIC), 718),
        ((sgc::KIND_GEOMETRY, sgc::SUBTYPE_DAMAGEABLE), 287),
        ((sgc::KIND_CAMERA, 11), 42),
        ((sgc::KIND_HELPER, 13), 367),
        ((sgc::KIND_COLLISION, 0), 65),
        ((sgc::KIND_SHADOW, 0), 152),
        ((sgc::KIND_POINT, 0), 247),
    ];
    for (key, count) in expected {
        assert_eq!(by_kind.get(&key), Some(&count), "records of kind {key:?}");
    }
    // 33 roots among meshes, helpers and cameras/collisions/shadows, plus `Point_drivercambase`
    // in ACCENT, which is a root point.
    assert_eq!(total_roots, 34, "root nodes (parent -1) over the eight models");
}

/// Writes the static and damageable meshes of XSARA to an OBJ file for a visual check, with each
/// mesh placed by its world matrix. Runs only when `RBR_DATA` is set. Output:
/// `/tmp/rbr-sgc/xsara-meshes.obj`.
#[test]
fn xsara_meshes_export_to_obj() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let root = Path::new(&data_dir);
    let car = retail_cars(root)
        .into_iter()
        .find(|c| c.file_name.to_ascii_lowercase().ends_with("xsara.sgc"))
        .expect("XSARA entry");
    let path = cars::resolve(root, &car.file_name).expect("model path");
    let bytes = std::fs::read(&path).expect("model bytes");
    let elements = sgc::parse(&bytes).expect("parse");

    let out_dir = Path::new("/tmp/rbr-sgc");
    std::fs::create_dir_all(out_dir).expect("create /tmp/rbr-sgc");
    let out_path = out_dir.join("xsara-meshes.obj");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&out_path).expect("create obj"));
    writeln!(out, "# XSARA static and damageable meshes, placed by their world matrix").unwrap();

    let mut base = 1;
    let mut faces_written = 0;
    for element in &elements {
        let sgc::Element::Mesh(mesh) = element else {
            continue;
        };
        let w = mesh.node.world;
        let apply = |p: [f32; 3]| -> [f32; 3] {
            std::array::from_fn(|i| w[i][0] * p[0] + w[i][1] * p[1] + w[i][2] * p[2] + w[i][3])
        };
        let rotate =
            |n: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| w[i][0] * n[0] + w[i][1] * n[1] + w[i][2] * n[2]) };
        writeln!(out, "o {}", mesh.node.name).unwrap();
        for v in &mesh.vertices {
            let p = apply(v.position);
            writeln!(out, "v {} {} {}", p[0], p[1], p[2]).unwrap();
        }
        for v in &mesh.vertices {
            let n = rotate(v.normal);
            writeln!(out, "vn {} {} {}", n[0], n[1], n[2]).unwrap();
        }
        for face in &mesh.faces {
            let group = face.name.replace(' ', "_");
            writeln!(out, "usemtl {}", if group.is_empty() { "none" } else { &group }).unwrap();
            let first = face.start_index as usize / 3;
            for tri in &mesh.triangles[first..first + face.prim_count as usize] {
                let [a, b, c] = tri.map(|i| i as usize + base);
                writeln!(out, "f {a}//{a} {b}//{b} {c}//{c}").unwrap();
                faces_written += 1;
            }
        }
        base += mesh.vertices.len();
    }
    out.flush().unwrap();
    assert!(faces_written > 0, "no triangles written");
    eprintln!("wrote {faces_written} triangles to {}", out_path.display());
}

/// Parses every retail `SGS/*.sgs` scenery model.
///
/// Runs only when `RBR_DATA` is set (see above). Retail 1.0 ships 153 files,
/// all accepted: 151 tile exactly, and `REDBULL_prize_01.sgs` (7,635 trailing
/// bytes) plus `moose_01.sgs` (24 trailing bytes) keep their non-record tails
/// in `Sgs::trailing` (`notes/formats.md` "Characters").
#[test]
fn all_retail_sgs_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let dir = Path::new(&data_dir).join("SGS");
    let mut files = common::collect_ext(&dir, "sgs");
    files.sort();
    assert_eq!(files.len(), 153, "retail 1.0 ships 153 SGS/*.sgs");
    let (mut static_meshes, mut skinned) = (0, 0);
    let (mut helpers, mut bones, mut iks, mut lights) = (0, 0, 0, 0);
    let (mut hierarchies, mut hierarchy_nodes) = (0, 0);
    let (mut cameras, mut points) = (0, 0);
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.display();
        let file = path.file_name().expect("file name").to_str().expect("utf-8");
        let parsed = sgs::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Only the two known files carry trailing bytes, with exact lengths.
        let trailing = !parsed.trailing.is_empty();
        assert_eq!(
            trailing,
            file == "REDBULL_prize_01.sgs" || file == "moose_01.sgs",
            "{name}: unexpected trailing bytes"
        );
        if trailing {
            assert_eq!(
                parsed.trailing.len(),
                if file == "REDBULL_prize_01.sgs" { 7635 } else { 24 },
                "{name}: trailing length"
            );
        }
        for element in &parsed.elements {
            match element {
                sgs::Element::StaticMesh(mesh) => {
                    static_meshes += 1;
                    // Bounds are the vertex-box centre and half-extent, as in the car files.
                    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
                    for v in &mesh.vertices {
                        for i in 0..3 {
                            lo[i] = lo[i].min(v.position[i]);
                            hi[i] = hi[i].max(v.position[i]);
                        }
                    }
                    for i in 0..3 {
                        assert!((mesh.centre[i] - (lo[i] + hi[i]) / 2.0).abs() < 0.01, "{name}: static mesh centre");
                        assert!(
                            (mesh.half_extent[i] - (hi[i] - lo[i]) / 2.0).abs() < 0.01,
                            "{name}: static mesh half-extent"
                        );
                    }
                    // Faces partition the triangle list in order.
                    let total: u32 = mesh.faces.iter().map(|f| f.prim_count).sum();
                    assert_eq!(total as usize, mesh.triangles.len(), "{name}: face partition");
                    for v in mesh.triangles.iter().flatten() {
                        assert!((*v as usize) < mesh.vertices.len(), "{name}: index range");
                    }
                }
                sgs::Element::SkinnedMesh(mesh) => {
                    skinned += 1;
                    assert_eq!(mesh.stream_flags, 23, "{name}: skinned stream flags");
                    assert_eq!(mesh.skin.weights.len(), mesh.vertices.len(), "{name}: one skin record per vertex");
                    // Each record names skin bones by matrix offset and its weights sum to one, but for a few
                    // `richard.sgs` and `robert.sgs` vertices on the identity matrix with weight 0.
                    let mut unskinned = 0;
                    for record in &mesh.skin.weights {
                        let bones: Vec<(Option<usize>, f32)> = record.bones().collect();
                        if bones == [(None, 0.0)] {
                            unskinned += 1;
                            continue;
                        }
                        assert!(
                            bones.iter().all(|&(b, _)| b.is_some_and(|b| b < mesh.skin.bones.len())),
                            "{name}: skin bones {bones:?} of {}",
                            mesh.skin.bones.len()
                        );
                        let sum: f32 = bones.iter().map(|&(_, w)| w).sum();
                        assert!((sum - 1.0).abs() < 0.01, "{name}: skin weights sum to {sum}");
                    }
                    let crew = ["richard.sgs", "robert.sgs"].iter().any(|n| name.to_string().ends_with(n));
                    assert_eq!(unskinned, if crew { 4 } else { 0 }, "{name}: unskinned vertices");
                    assert_eq!(mesh.uvs.len(), mesh.vertices.len(), "{name}: one uv per vertex");
                    assert!(mesh.uvs.iter().flatten().all(|v| v.is_finite()), "{name}: finite uvs");
                    // The two bounds words match neither the stored-vertex
                    // box as min/max nor as centre/half-extent
                    // (`notes/formats.md` "Characters"): only finiteness is checked.
                    assert!(
                        mesh.min.iter().chain(mesh.max.iter()).all(|v| v.is_finite()),
                        "{name}: skinned bounds finite"
                    );
                    let total: u32 = mesh.faces.iter().map(|f| f.prim_count).sum();
                    assert_eq!(total as usize, mesh.triangles.len(), "{name}: face partition");
                    for v in mesh.triangles.iter().flatten() {
                        assert!((*v as usize) < mesh.vertices.len(), "{name}: index range");
                    }
                    assert!(!mesh.skin.bones.is_empty(), "{name}: skin bones");
                }
                sgs::Element::Helper(_) => helpers += 1,
                sgs::Element::Bone(_) => bones += 1,
                sgs::Element::IkNode(_) => iks += 1,
                sgs::Element::Light(light) => {
                    lights += 1;
                    assert_eq!(light.enabled, 0, "{name}: light flag");
                }
                sgs::Element::Hierarchy(h) => {
                    hierarchies += 1;
                    hierarchy_nodes += h.nodes.len();
                    // 47 (0x2f, PRS node) everywhere; 3 nodes of type 48
                    // (0x30, PS node) in anim2_REDBULL_can1b_01.sgs.
                    assert!(
                        h.nodes.iter().all(|n| n.node_type == 47 || n.node_type == 48),
                        "{name}: hierarchy node type"
                    );
                }
                sgs::Element::Unparsed { kind, .. } => {
                    if *kind == 2 {
                        cameras += 1;
                    } else {
                        points += 1;
                    }
                }
                sgs::Element::Ignored { .. } => {}
            }
        }
    }
    assert_eq!(static_meshes, 8, "retail .sgs holds 8 static meshes");
    assert_eq!(skinned, 148, "retail .sgs holds 148 skinned meshes");
    assert_eq!(helpers, 1444, "retail .sgs holds 1,444 helpers");
    assert_eq!(bones, 3800, "retail .sgs holds 3,800 bones");
    assert_eq!(iks, 18, "retail .sgs holds 18 IK nodes");
    assert_eq!(lights, 18, "retail .sgs holds 18 lights");
    assert_eq!(hierarchies, 153, "every .sgs holds one hierarchy table");
    assert_eq!(cameras, 3, "retail .sgs holds 3 cameras");
    assert_eq!(points, 100, "retail .sgs holds 100 points");
    eprintln!(
        "sgs: {static_meshes} static, {skinned} skinned, {helpers} helpers, {bones} bones, \
         {hierarchy_nodes} hierarchy nodes"
    );
}

/// Parses every retail `SGA/*.sga` scenery animation.
///
/// Runs only when `RBR_DATA` is set (see above). Retail 1.0 ships 137 files:
/// one kind-20 range record each and 4,069 kind-6/21 controller bindings
/// (3,859 block-order sampled-PRS, 41 interleaved, 19 single-matrix, 60
/// float/position/scale camera tracks, 90 skipped IK chains). Every record
/// is consumed exactly.
#[test]
fn all_retail_sga_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let dir = Path::new(&data_dir).join("SGA");
    let mut files = common::collect_ext(&dir, "sga");
    files.sort();
    assert_eq!(files.len(), 137, "retail 1.0 ships 137 SGA/*.sga");
    let (mut ranges, mut sampled, mut interleaved, mut single, mut position, mut scale, mut float, mut ik) =
        (0, 0, 0, 0, 0, 0, 0, 0);
    let mut samples = 0usize;
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.display();
        let elements = sga::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        for element in &elements {
            match element {
                sga::Element::Range(_) => ranges += 1,
                sga::Element::Animation(anim) => match &anim.controller {
                    sga::Controller::SampledPrs(s) => {
                        sampled += 1;
                        samples += s.samples.len();
                        assert_eq!(s.times.len(), s.samples.len(), "{name}: sample times");
                        assert!(!s.samples.is_empty(), "{name}: empty samples");
                        // Times run forward.
                        assert!(s.times.windows(2).all(|w| w[1] >= w[0]), "{name}: sample times ordered");
                    }
                    sga::Controller::InterleavedPrs(s) => {
                        interleaved += 1;
                        samples += s.samples.len();
                        assert_eq!(s.times.len(), s.samples.len(), "{name}: sample times");
                        assert!(s.times.windows(2).all(|w| w[1] >= w[0]), "{name}: sample times ordered");
                    }
                    sga::Controller::SingleMatrix { count, head, matrix } => {
                        // The head runs i * 293 * 52 in file order. Most
                        // matrices are unit rotation rows (Redbull01 is
                        // zero, spinerot2 a uniform 0.457 scale), so only
                        // finiteness is checked.
                        assert_eq!(*count, 293, "{name}: single-matrix count word");
                        assert_eq!(*head, single as u32 * 293 * 52, "{name}: single-matrix head");
                        assert!(matrix.iter().flatten().all(|v| v.is_finite()), "{name}: single-matrix finite");
                        single += 1;
                    }
                    sga::Controller::PositionXyz(xyz) => {
                        position += 1;
                        assert!(
                            [xyz.x.keys.len(), xyz.y.keys.len(), xyz.z.keys.len()].iter().all(|&n| n > 0),
                            "{name}: empty axis"
                        );
                    }
                    sga::Controller::Scale(t) => {
                        scale += 1;
                        assert!(!t.keys.is_empty(), "{name}: empty scale");
                    }
                    sga::Controller::Float(t) => {
                        float += 1;
                        assert!(!t.keys.is_empty(), "{name}: empty float");
                    }
                    sga::Controller::SkippedIk => ik += 1,
                },
            }
        }
    }
    assert_eq!(ranges, 137, "every .sga holds one range record");
    assert_eq!(sampled, 3859, "retail .sga holds 3,859 block-order bindings");
    assert_eq!(interleaved, 41, "retail .sga holds 41 interleaved bindings");
    assert_eq!(single, 19, "retail .sga holds 19 single-matrix bindings");
    assert_eq!(position, 20, "retail .sga holds 20 position bindings");
    assert_eq!(scale, 20, "retail .sga holds 20 scale bindings");
    assert_eq!(float, 20, "retail .sga holds 20 float bindings");
    assert_eq!(ik, 90, "retail .sga holds 90 IK bindings");
    eprintln!("sga: {ranges} ranges, {samples} PRS samples");
}

/// Checks how stages reach the scenery files: the `(2, 14)` animation
/// objects of every retail `Maps/*.lbs` name load scripts, not models —
/// both material names of every item end in `.LUA` (resolved from
/// `Root/Scripts.rbz`), and those scripts call `app:LoadCharacter` on
/// `SGS/*.sgs` and `app:LoadAnimation` on `SGA/*.sga`
/// (`notes/formats.md` "Characters"). Neither the `.lbs` object lanes (`(1, 1)`,
/// static geometry) nor the `.dls` reference the scenery files directly.
///
/// Runs only when `RBR_DATA` is set (see above).
#[test]
fn stage_objects_reference_sgs_and_sga() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let data = Path::new(&data_dir);
    let mut animation = 0;
    for path in common::collect_ext(&data.join("Maps"), "lbs") {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let lbs = lbs::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for chunk in &lbs.chunks {
            if let lbs::Chunk::AnimationObjects(objs) = chunk {
                for item in &objs.items {
                    animation += 1;
                    for material in [item.material_a, item.material_b] {
                        let material = String::from_utf8_lossy(material);
                        assert!(
                            material.to_lowercase().ends_with(".lua"),
                            "{}: material {material} is not a load script",
                            path.display()
                        );
                    }
                }
            }
        }
    }
    assert_eq!(animation, 7041, "retail stages hold 7,041 animation objects");
}

/// Every script in the install's and the 1.02 patch's `Scripts.rbz` parses with the `lua` subset, and each stage
/// character load or run script defines the functions the exe calls (`Load`, `Update`, `Trigger`).
///
/// Runs only when `RBR_DATA` is set (see above); the patch archive also when `RBR_PATCH` is.
#[test]
fn all_retail_scripts_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let mut archives = vec![Path::new(&data_dir).join("Root/Scripts.rbz")];
    if let Ok(patch) = std::env::var("RBR_PATCH") {
        archives.push(Path::new(&patch).join("Scripts.rbz"));
    }
    let defines = |chunk: &rbr_formats::lua::Chunk, name: &str| {
        chunk.body.iter().any(|s| matches!(s, rbr_formats::lua::Stmt::Function { name: n, .. } if n == name))
    };
    // Scripts Lua 4.0 rejects too: placeholder text (`load_crash_*`, `blah blah blah`), a stray `<`, statements after
    // a `return` (`*_site0_test`) and an extra `end` (`*test_redbull_*`). No retail stage item loads them.
    let broken = |file: &str| {
        ["load_crash_car.lua", "load_crash_codriver.lua", "load_crash_driver.lua", "load_cow_walk_03.lua"]
            .contains(&file)
            || file.ends_with("_site0_test.lua")
            || file.contains("test_redbull_")
    };
    for path in &archives {
        let mut scripts = 0;
        common::each_rbz_entry(path, |entry_name, bytes| {
            let name = entry_name.to_owned();
            if !name.to_lowercase().ends_with(".lua") {
                return;
            }
            let file = name.rsplit('/').next().unwrap_or(&name).to_lowercase();
            let parsed = rbr_formats::lua::parse(bytes);
            if broken(&file) {
                assert!(parsed.is_err(), "{name} parses");
                return;
            }
            let chunk = parsed.unwrap_or_else(|e| panic!("{name}: {e}"));
            if file.starts_with("load_") {
                assert!(defines(&chunk, "Load"), "{name}: no Load");
            }
            if file.starts_with("run_") {
                assert!(defines(&chunk, "Update") && defines(&chunk, "Trigger"), "{name}: no Update/Trigger");
            }
            scripts += 1;
        });
        assert!(scripts > 400, "{}: {scripts} scripts", path.display());
    }
}

/// Every `Driveline` animation channel of the 86 stages triggers an `.lbs` `(2, 14)` animation object of its stage.
///
/// Runs only when `RBR_DATA` is set (see above).
#[test]
fn stage_animation_triggers_name_animation_objects() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let maps = Path::new(&data_dir).join("Maps");
    let (mut stages, mut triggers) = (0, 0);
    for path in common::collect_ext(&maps, "dls") {
        let bytes = std::fs::read(&path).expect("read dls");
        let file = dls::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let found =
            rbr_formats::markers::animation_triggers(&file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let lbs_bytes = std::fs::read(path.with_extension("lbs")).expect("read lbs");
        let lbs = lbs::parse(&lbs_bytes).expect("parse lbs");
        let ids: HashSet<u32> = lbs
            .chunks
            .iter()
            .filter_map(|c| match c {
                lbs::Chunk::AnimationObjects(objs) => Some(objs.items.iter().map(|i| i.dwords[0])),
                _ => None,
            })
            .flatten()
            .collect();
        // The three track-54 stages (Tanner) keep 14 of their 31 channels for objects their `.lbs` no longer has;
        // the exe's lookup misses them and nothing reacts.
        let dangling = found.iter().filter(|t| !ids.contains(&t.object)).count();
        let stale = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("track-54_"));
        assert_eq!(dangling, if stale { 14 } else { 0 }, "{}: triggers for missing objects", path.display());
        for trigger in &found {
            assert!(trigger.distance.is_finite() && trigger.lead >= 0.0, "{}: {trigger:?}", path.display());
        }
        stages += 1;
        triggers += found.len();
    }
    assert_eq!(stages, 86);
    eprintln!("{triggers} animation triggers");
}

/// `Misc/times.ini` in `Root/misc.rbz`: 37 stages, each with two splits and a finish in increasing order.
#[test]
fn all_retail_times_ini_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let stages = times::read(&Path::new(&data_dir).join("Root/misc.rbz")).expect("times.ini");
    assert_eq!(stages.len(), 37, "the file's stage count");
    let ids: BTreeSet<u32> = stages.iter().map(|s| s.stage).collect();
    assert_eq!(ids.len(), 37, "stage numbers are distinct");
    for s in &stages {
        let [first, second] = s.splits;
        assert!(0.0 < first && first < second && second < s.finish, "stage {}: {:?}", s.stage, s);
    }
    let stage_54 = stages.iter().find(|s| s.stage == 54).expect("stage 54");
    assert_eq!(*stage_54, times::StageTimes { stage: 54, splits: [57.0, 119.0], finish: 185.0 });
}

/// The `Driveline` plain trigger keys of the 86 stages parse sorted, and every `run_birdstrike` item has a forward key
/// (no animation channel names a bird strike).
///
/// Runs only when `RBR_DATA` is set (see above).
#[test]
fn stage_plain_triggers_reach_the_bird_strikes() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let maps = Path::new(&data_dir).join("Maps");
    let (mut stages, mut keys, mut strikes) = (0, 0, 0);
    for path in common::collect_ext(&maps, "dls") {
        let bytes = std::fs::read(&path).expect("read dls");
        let file = dls::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let found = rbr_formats::markers::plain_triggers(&file).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(found.windows(2).all(|w| w[0].distance <= w[1].distance), "{}: unsorted", path.display());
        let forward: HashSet<u32> = found.iter().filter(|k| k.forward()).map(|k| k.object).collect();
        let lbs_bytes = std::fs::read(path.with_extension("lbs")).expect("read lbs");
        let lbs = lbs::parse(&lbs_bytes).expect("parse lbs");
        for chunk in &lbs.chunks {
            let lbs::Chunk::AnimationObjects(objs) = chunk else { continue };
            for item in &objs.items {
                if String::from_utf8_lossy(item.material_b).to_ascii_lowercase().ends_with("run_birdstrike.lua") {
                    assert!(forward.contains(&item.dwords[0]), "{}: bird strike {}", path.display(), item.dwords[0]);
                    strikes += 1;
                }
            }
        }
        stages += 1;
        keys += found.len();
    }
    assert_eq!(stages, 86);
    eprintln!("{keys} plain trigger keys, {strikes} bird strikes");
}

#[test]
fn all_retail_tga_parse() {
    let Some(data_dir) = common::data_dir() else {
        return;
    };
    let files = common::collect_ext(&Path::new(&data_dir).join("Textures"), "tga");
    assert_eq!(files.len(), 5, "retail 1.0 ships 5 loose .tga files, all in Textures");
    for path in &files {
        let bytes = std::fs::read(path).expect("read tga");
        let image = rbr_formats::tga::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(image.rgba.len() as u32, image.width * image.height * 4, "{}", path.display());
    }
    // The car's shadow decal: black, its alpha clear at the border and about 0.6 inside.
    let shadow =
        rbr_formats::tga::parse(&std::fs::read(Path::new(&data_dir).join("Textures/car_dpshad2.tga")).unwrap())
            .expect("car_dpshad2.tga");
    assert_eq!((shadow.width, shadow.height), (64, 128));
    assert_eq!(shadow.rgba[3], 0);
    let middle = ((64 * 64 + 32) * 4) as usize;
    assert_eq!(&shadow.rgba[middle..middle + 3], [0, 0, 0]);
    assert!(shadow.rgba[middle + 3] > 140, "{}", shadow.rgba[middle + 3]);
}

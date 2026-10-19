//! `stage-info`: print summaries of stage metadata files.
//!
//! ```text
//! stage-info <file.mat|file.fnc|file.trk|file.col|file.dls> [...]
//! ```
//!
//! Read-only inspection: each file is read, framed/parsed in memory and a
//! short summary is printed. Nothing is extracted, written or mutated.

use rbr_formats::{col, dls, fnc, mat, trk};
use std::path::Path;
use std::process::ExitCode;

fn summarize_mat(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = mat::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("{}: mat entries={}", path.display(), file.entries.len());
    let mut total_runs = 0usize;
    for (i, entry) in file.entries.iter().enumerate() {
        total_runs += entry.runs.len();
        let dims = entry.runs.iter().all(|r| r.width == mat::GRID_WIDTH && r.height == mat::GRID_HEIGHT);
        let dims_note = if dims { "16x16" } else { "mixed" };
        println!("  entry {i}: {:?} runs={} dims={dims_note}", entry.name, entry.runs.len());
    }
    println!("  total runs={total_runs}");
    Ok(())
}

fn summarize_fnc(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = fnc::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let total_posts: usize = file.groups.iter().map(|g| g.posts.len()).sum();
    println!(
        "{}: fnc groups={} posts={total_posts} textures={} ignored_tail={} bytes",
        path.display(),
        file.groups.len(),
        file.textures.len(),
        file.ignored_tail.len()
    );
    for (i, group) in file.groups.iter().enumerate() {
        println!(
            "  group {i}: posts={} fence_type={} post_type={} material={} rail_material={} \
             center={:?} half_extent={:?}",
            group.posts.len(),
            group.fence_type,
            group.post_type,
            group.material,
            group.rail_material,
            group.center,
            group.half_extent
        );
        if let Some(first) = group.posts.first() {
            println!(
                "    first post: position={:?} center={:?} half_extent={:?} color={:#x}",
                first.position, first.center, first.half_extent, first.color
            );
        }
    }
    Ok(())
}

fn summarize_trk(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = trk::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("{}: trk chunks={} size={} bytes", path.display(), file.chunks.len(), bytes.len());
    let mut offset = 0usize;
    for (i, chunk) in file.chunks.iter().enumerate() {
        match chunk.type_kind() {
            Some((t, k)) => {
                println!("  chunk {i}: offset={offset} header=[{t}, 0x{k:x}] payload={} bytes", chunk.data.len())
            }
            None => println!(
                "  chunk {i}: offset={offset} header_len={} payload={} bytes",
                chunk.header.len(),
                chunk.data.len()
            ),
        }
        match &chunk.payload {
            trk::ChunkPayload::DriveLine(data) => {
                println!("    driveline records={} ignored_tail={} bytes", data.records.len(), data.ignored_tail.len())
            }
            trk::ChunkPayload::CollisionMeshes(data) => {
                let vertices: usize = data.meshes.iter().map(|m| m.vertices.len()).sum();
                let faces: usize = data.meshes.iter().map(|m| m.faces.len()).sum();
                let objects: usize = data.meshes.iter().map(|m| m.objects.len()).sum();
                println!(
                    "    collision meshes={} vertices={vertices} faces={faces} placements={objects} ignored_tail={} bytes",
                    data.meshes.len(),
                    data.ignored_tail.len()
                );
            }
            trk::ChunkPayload::Opaque => println!("    payload opaque"),
        }
        offset += 4 + chunk.header.len() + 4 + chunk.data.len();
    }
    Ok(())
}

fn summarize_col(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = col::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let vertices: usize = file.cells.iter().map(|cell| cell.vertices.len()).sum();
    let triangles: usize = file.cells.iter().map(|cell| cell.tree.triangles.len()).sum();
    println!(
        "{}: col root_nodes={} cells={} vertices={} triangles={} water_meshes={}",
        path.display(),
        file.root.tree.nodes.len(),
        file.cells.len(),
        vertices,
        triangles,
        file.root.mesh_a.len() + file.root.mesh_b.len()
    );
    Ok(())
}

fn summarize_dls(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file = dls::parse(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let replay_v6 = file.animation_entries.iter().filter(|entry| entry.replay.header[0] == 6).count();
    println!(
        "{}: dls version={}.{} animations={} groups={} table2={} table3={} emitters={} table5={} spheres={} strings={} bytes replay_v6={}",
        path.display(),
        file.version,
        file.minor_version,
        file.animation_entries.len(),
        file.track_groups.len(),
        file.table_2.len(),
        file.table_3.len(),
        file.emitters.len(),
        file.table_5.len(),
        file.spheres.len(),
        file.string_bytes.len(),
        replay_v6
    );
    Ok(())
}

fn summarize(path: &Path) -> Result<(), String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    match ext.as_str() {
        "mat" => summarize_mat(path, &bytes),
        "fnc" => summarize_fnc(path, &bytes),
        "trk" => summarize_trk(path, &bytes),
        "col" => summarize_col(path, &bytes),
        "dls" => summarize_dls(path, &bytes),
        _ => Err(format!("{}: unknown extension {ext:?}, expected .mat/.fnc/.trk/.col/.dls", path.display())),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage:\n  stage-info <file.mat|file.fnc|file.trk|file.col|file.dls> [...]");
        return ExitCode::from(2);
    }
    let mut failed = false;
    for arg in &args[1..] {
        if let Err(e) = summarize(Path::new(arg)) {
            eprintln!("stage-info: {e}");
            failed = true;
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

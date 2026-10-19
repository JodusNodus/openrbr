//! The water items of every retail stage (visuals-5, ours): each face's name, flags, keys and float pairs, its
//! vertices' bounds and colours, and how far it lies from the start.
//!
//! RBR_DATA=<install> cargo run --release -p rbr-render --example water_survey [STAGE_DIR...]

use rbr_formats::data::Data;
use rbr_formats::lbs::{self, Chunk};
use std::error::Error;
use std::path::PathBuf;

fn f32_at(raw: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(raw[at..at + 4].try_into().unwrap())
}

fn main() -> Result<(), Box<dyn Error>> {
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&maps)?
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "lbs" || e == "lb2"))
        .collect();
    files.extend(std::env::args().skip(1).map(PathBuf::from));
    files.sort();
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(parsed) = lbs::parse(&bytes) else {
            println!("{}: unreadable", path.display());
            continue;
        };
        let start = parsed.chunks.iter().find_map(|c| match c {
            Chunk::StartLocation(s) => Some(*s),
            _ => None,
        });
        for chunk in &parsed.chunks {
            let Chunk::Water(water) = chunk else { continue };
            for face in &water.faces {
                for item in &face.items {
                    let b = &item.buffers;
                    let stride = b.vertex_size;
                    let n = b.vertices.len().checked_div(stride).unwrap_or(0);
                    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    let (mut alo, mut ahi) = ([255u8; 4], [0u8; 4]);
                    for v in 0..n {
                        let raw = &b.vertices[v * stride..(v + 1) * stride];
                        for k in 0..3 {
                            lo[k] = lo[k].min(f32_at(raw, 4 * k));
                            hi[k] = hi[k].max(f32_at(raw, 4 * k));
                        }
                        if stride >= 28 {
                            for k in 0..4 {
                                alo[k] = alo[k].min(raw[24 + k]);
                                ahi[k] = ahi[k].max(raw[24 + k]);
                            }
                        }
                    }
                    // The middle of the deep part (alpha over half), for a camera.
                    let deep: Vec<[f32; 3]> = (0..n)
                        .map(|v| &b.vertices[v * stride..(v + 1) * stride])
                        .filter(|raw| stride >= 28 && raw[27] > 100)
                        .map(|raw| [f32_at(raw, 0), f32_at(raw, 4), f32_at(raw, 8)])
                        .collect();
                    if let Some(&mid) = deep.get(deep.len() / 2) {
                        println!("  deep vertex {:?}", mid.map(|x| x.round()));
                    }
                    if std::env::var_os("WATER_RAW").is_some() {
                        for v in 0..n.min(3) {
                            let raw = &b.vertices[v * stride..(v + 1) * stride];
                            let floats: Vec<f32> = (0..stride / 4).map(|k| f32_at(raw, 4 * k)).collect();
                            println!("  vertex {v}: {floats:?} bytes24 {:?}", &raw[24..28]);
                        }
                    }
                    println!(
                        "{} {:?} flags {:#x} key0 {} k10 {:?} k20 {:?} k40 {:?} stride {} verts {} tris {} \
                         lo {:?} hi {:?} colour bgra {:?}..{:?} start {:?}",
                        path.file_name().unwrap().to_string_lossy(),
                        String::from_utf8_lossy(face.name),
                        item.flags,
                        item.key0,
                        item.key10,
                        item.key20,
                        item.key40,
                        stride,
                        n,
                        b.indices.len() / 6,
                        lo.map(|x| x.round()),
                        hi.map(|x| x.round()),
                        alo,
                        ahi,
                        start.map(|s| format!("{s:?}")).unwrap_or_default().get(..80).map(str::to_owned),
                    );
                }
            }
        }
    }
    Ok(())
}

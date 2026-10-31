//! Shared helpers for the retail-data integration tests: locating the
//! extracted install tree, listing stage files, and reading archive entries.
//!
//! Every helper skips (printing `skipping: ...`) when its input is missing,
//! so `cargo test` works without game data. Each test target compiles this
//! module separately and uses only a subset of it.

// Allow the unused subset: every test target compiles this whole module but
// uses only the helpers its tests need.
#![allow(dead_code)]

use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The extracted retail install (`RBR_DATA`), or `None` after printing the skip note.
pub fn data_dir() -> Option<PathBuf> {
    match std::env::var("RBR_DATA") {
        Ok(dir) => Some(PathBuf::from(dir)),
        Err(_) => {
            eprintln!("skipping: RBR_DATA not set");
            None
        }
    }
}

/// The install's `Maps` folder, or `None` after printing the skip note.
pub fn maps_dir() -> Option<PathBuf> {
    let dir = data_dir()?.join("Maps");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: {} not found", dir.display());
        None
    }
}

/// The files with extension `ext` directly in `dir`, in sorted order.
pub fn collect_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Every `.rbz` archive under `dir`, recursively.
pub fn collect_rbz(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rbz(&path, out);
        } else if path.extension().is_some_and(|e| e == "rbz") {
            out.push(path);
        }
    }
}

/// Calls `f` with the name and bytes of every entry of the archive at
/// `path`, in archive order. Failures panic with the archive path and entry
/// name, like the loops this replaces.
pub fn each_rbz_entry(path: &Path, mut f: impl FnMut(&str, &[u8])) {
    let mut archive = rbr_formats::rbz::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        let name = entry.name().expect("ascii name").into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap_or_else(|e| panic!("{}!{name}: {e}", path.display()));
        f(&name, &bytes);
    }
}

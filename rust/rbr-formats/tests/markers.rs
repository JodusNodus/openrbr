//! Reads the stage marker keys (`rbr_formats::markers`) from every retail `Maps/*.dls`.
//!
//! Runs only when `RBR_DATA` points at the extracted install tree
//! (`original/retail-1.0/install`), so `cargo test` works without game data.

use rbr_formats::markers::{self, MAX_SPLITS};
use rbr_formats::trk::{self, ChunkPayload};
use std::path::{Path, PathBuf};

fn maps_dir() -> Option<PathBuf> {
    let data_dir = std::env::var("RBR_DATA").ok()?;
    let dir = Path::new(&data_dir).join("Maps");
    if dir.is_dir() {
        Some(dir)
    } else {
        eprintln!("skipping: {} not found", dir.display());
        None
    }
}

fn collect_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
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

/// First and last driveline `value` of a stage's `.trk` kind-`0x14` chunk.
fn driveline_range(trk_path: &Path) -> (f32, f32) {
    let bytes = std::fs::read(trk_path).unwrap_or_else(|e| panic!("{}: {e}", trk_path.display()));
    let file = trk::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", trk_path.display()));
    for chunk in &file.chunks {
        if let ChunkPayload::DriveLine(data) = &chunk.payload {
            if let (Some(first), Some(last)) = (data.records.first(), data.records.last()) {
                return (first.value, last.value);
            }
        }
    }
    panic!("{}: no driveline records", trk_path.display());
}

/// Parses the marker keys of every retail stage and checks them against the
/// driveline: start/split/finish/end keys lie on the driveline and split
/// keys are ordered between start and finish.
#[test]
fn all_retail_stage_markers_parse() {
    let Some(dir) = maps_dir() else {
        eprintln!("skipping: RBR_DATA not set");
        return;
    };
    let files = collect_ext(&dir, "dls");
    assert_eq!(files.len(), 86, "retail ships 86 .dls files");
    let mut with_start = 0;
    let mut with_splits = 0;
    let mut max_pace_overshoot = 0.0f32;
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let markers = markers::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (lo, hi) = driveline_range(&path.with_extension("trk"));
        let name = path.display();

        assert!(markers.keys.windows(2).all(|w| w[0].distance <= w[1].distance), "{name}: keys not sorted");
        for key in &markers.keys {
            if (markers::TAG_START..=markers::TAG_END).contains(&key.tag) {
                assert!(
                    key.distance >= lo && key.distance <= hi,
                    "{name}: tag {:#x} at {} outside driveline [{lo}, {hi}]",
                    key.tag,
                    key.distance
                );
            } else {
                max_pace_overshoot = max_pace_overshoot.max(key.distance - hi);
            }
        }

        let splits: Vec<f32> = markers.splits().collect();
        assert!(splits.len() <= MAX_SPLITS, "{name}: too many splits");
        if let Some(start) = markers.start() {
            with_start += 1;
            if let Some(finish) = markers.finish() {
                assert!(start < finish, "{name}: start {start} >= finish {finish}");
                for split in &splits {
                    assert!(start < *split && *split < finish, "{name}: split {split} outside ({start}, {finish})");
                }
            }
        }
        if !splits.is_empty() {
            with_splits += 1;
            assert_eq!(splits.len(), 2, "{name}: retail stages with splits have two");
        }
    }
    eprintln!(
        "parsed markers for {} .dls files: {with_start} with a start key, {with_splits} with splits; \
         largest pace-note distance past the last driveline value: {max_pace_overshoot:.1} m",
        files.len()
    );
}

/// Every retail `.dls`'s named triggers (table 9) resolve to names in its string table; the 57 stages with the start
/// set pieces name `start_animation2`, and its object is no `Driveline` trigger's: only the HUD's countdown fires it.
#[test]
fn all_retail_named_triggers_resolve() {
    let Some(dir) = maps_dir() else { return };
    let mut with_start = 0;
    for path in collect_ext(&dir, "dls") {
        let bytes = std::fs::read(&path).unwrap();
        let file = rbr_formats::dls::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let named = markers::named_triggers(&file);
        assert_eq!(named.len(), file.table_9.len(), "{}: a name outside the string table", path.display());
        assert!(named.iter().all(|t| !t.name.is_empty() && t.object != 0), "{}: {named:?}", path.display());
        let Some(object) = markers::find_named(&named, markers::START_ANIMATION) else { continue };
        with_start += 1;
        let channels = markers::animation_triggers(&file).unwrap();
        let keys = markers::plain_triggers(&file).unwrap();
        assert!(
            channels.iter().all(|t| t.object != object) && keys.iter().all(|t| t.object != object),
            "{}: the driveline also triggers {object:#x}",
            path.display()
        );
    }
    assert_eq!(with_start, 57);
}

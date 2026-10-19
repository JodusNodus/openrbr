//! The game's particle records of a recorded drive, for its playback: what the recording lacks (the lamps' reverse
//! switch and damage, PEV1 kind 6, and the game's own tyre payloads). `rbr-standalone --boot --recording X
//! --events-out FILE` makes them by running the recording's controls through the live boot, whose physics equals the
//! recording's. It runs once per recording, stage and `rbr-standalone` build (its size and modification time); the file
//! is kept in the cache directory (`RBR_EVENTS_CACHE`, else `drive-events/` next to the viewer, with the build's other
//! outputs), never next to the recording.
//!
//! `RBR_PARTICLE_EVENTS=<file>` gives such a file instead; set to an empty
//! value it turns them off. A recording without its `.auto` (the car and the race options) gets none.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{load_events, Step};

/// The recorded events of `recording` on `stage` (`track-54_E`), by step; empty (reported on stderr) when there are
/// none.
pub fn events(recording: &Path, stage: &str) -> HashMap<u32, Step> {
    let path = match std::env::var_os("RBR_PARTICLE_EVENTS") {
        Some(path) if path.is_empty() => return HashMap::new(),
        Some(path) => Ok(PathBuf::from(path)),
        None => cached(recording, stage),
    };
    path.and_then(|path| load_events(&path)).unwrap_or_else(|e| {
        eprintln!("particles: recorded events: {e}");
        HashMap::new()
    })
}

/// The cache file of `recording` on `stage`, made first if missing.
fn cached(recording: &Path, stage: &str) -> Result<PathBuf, String> {
    if !recording.with_extension("auto").is_file() {
        return Err(format!("{}: no .auto beside it, so no replay", recording.display()));
    }
    let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    let (bytes, auto) = (read(recording)?, read(&recording.with_extension("auto"))?);
    let here = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
    let standalone = std::env::var_os("RBR_STANDALONE")
        .map_or_else(|| here.join(format!("rbr-standalone{}", std::env::consts::EXE_SUFFIX)), PathBuf::from);
    // A rebuilt `rbr-standalone` makes its events again.
    let build = std::fs::metadata(&standalone).ok().map(|m| (m.len(), m.modified().ok()));
    let dir = std::env::var_os("RBR_EVENTS_CACHE").map_or_else(|| here.join("drive-events"), PathBuf::from);
    let path = dir.join(cache_name(recording, stage, &(stage, &bytes, &auto, build)));
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = path.with_extension("part");
    eprintln!("particles: replaying {} for its lamps and tyre payloads (once)", recording.display());
    let status = Command::new(&standalone)
        .args(["--boot", "--stage", stage, "--recording"])
        .arg(recording)
        .arg("--events-out")
        .arg(&part)
        .status()
        .map_err(|e| format!("{}: {e}", standalone.display()))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} --events-out: {status}", standalone.display()));
    }
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// The cache file's name: the recording's stem, the stage and a hash of what makes the events (`inputs`: the stage,
/// the recording's and its `.auto`'s bytes, and the `rbr-standalone` build).
fn cache_name(recording: &Path, stage: &str, inputs: &impl Hash) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    inputs.hash(&mut hasher);
    let stem = recording.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let stage = Path::new(stage).file_name().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    format!("{stem}-{stage}-{:016x}.events.bin", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_name_follows_the_recording_the_stage_and_the_build() {
        let recording = Path::new("/r/drive-tanner-corolla.bin");
        let built = std::time::SystemTime::UNIX_EPOCH;
        let name = |stage, bytes: &[u8], len| cache_name(recording, stage, &(stage, bytes, Some((len, Some(built)))));
        let first = name("track-54_E", b"frames", 100);
        assert!(first.starts_with("drive-tanner-corolla-track-54_E-") && first.ends_with(".events.bin"));
        assert_ne!(first, name("track-54_E", b"other", 100));
        assert_ne!(first, name("track-54_O", b"frames", 100));
        // Another `rbr-standalone` build.
        assert_ne!(first, name("track-54_E", b"frames", 101));
    }
}

//! What a stage gives the loop: its markers, driveline and reference times, read from the data (`stage_files`).

use rbr_formats::data::Data;
use rbr_formats::markers::{self, StageMarkers};
use rbr_formats::times::{self, StageTimes};
use rbr_formats::trk::{self, ChunkPayload, DriveLineRecord};
use rbr_formats::{lbs, stage_files};
use std::path::Path;

use crate::saves::Saves;
use crate::{autopilot, boot};

/// What the stage gives the loop.
pub struct StageData {
    pub markers: StageMarkers,
    pub track: Vec<DriveLineRecord>,
    pub number: u32,
    /// The stage's reference times from `misc/times.ini`, when the file has the stage.
    pub reference: Option<StageTimes>,
    pub car: u32,
    /// End-exclusive range for the `.lbs` `(1,6)` solid handles.
    pub interactive_handle_end: u32,
    /// The heavy props the autopilot steers around.
    pub obstacles: Vec<autopilot::Obstacle>,
}

/// The stage `stage` (a name or path, `stage_files`) of `data`, with `car` the race's car.
pub fn stage_data(data: &Data, stage: &str, car: u32) -> Result<StageData, String> {
    let read = |ext: &str| stage_files::read(data, stage, ext);
    let lbs_bytes = read("lbs")?;
    let stage_lbs = lbs::parse(&lbs_bytes).map_err(|e| format!("{stage}.lbs: {e}"))?;
    let interactive_count: usize = stage_lbs
        .chunks
        .iter()
        .filter_map(|chunk| match chunk {
            lbs::Chunk::Interactive(objects) => {
                Some(objects.objects.iter().map(|object| object.visibility.len()).sum::<usize>())
            }
            _ => None,
        })
        .sum();
    let markers = markers::parse(&read("dls")?).map_err(|e| format!("{stage}.dls: {e}"))?;
    let trk = trk::parse(&read("trk")?).map_err(|e| format!("{stage}.trk: {e}"))?;
    let track = trk
        .chunks
        .iter()
        .find_map(|chunk| match &chunk.payload {
            ChunkPayload::DriveLine(d) => Some(d.records.clone()),
            _ => None,
        })
        .ok_or_else(|| format!("{stage}.trk: no driveline"))?;
    let obstacles = autopilot::obstacles(&trk);
    let (number, _) = stage_files::parts(stage).ok_or_else(|| format!("{stage}: not track-<number>_<variant>"))?;
    let misc = data.resolve("Root/misc.rbz").ok_or("Root/misc.rbz: not found")?;
    let reference = times::read(&misc).map_err(|e| e.to_string())?.into_iter().find(|t| t.stage == number);
    let interactive_handle_end = boot::INTERACTIVE_HANDLE_BASE + interactive_count as u32;
    Ok(StageData { markers, track, number, reference, car, interactive_handle_end, obstacles })
}

impl StageData {
    /// The player's stage record and personal best for this stage and car in the save files in `dir` (`saves.rs`).
    pub fn saves(&self, dir: &Path, profile: Option<&str>) -> Result<Saves, String> {
        let times: Vec<StageTimes> = self.reference.into_iter().collect();
        Saves::open(dir, profile, &times, self.number, self.car)
    }
}

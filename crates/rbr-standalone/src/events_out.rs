//! `--events-out FILE` (with `--boot --recording X`): the recording's controls through the live boot (`live::Live`,
//! paced a step a payload as the viewer's live play), whose physics equals the recording's
//! (`live::tests::a_paced_live_boot_with_the_recorded_controls_is_the_recording`), and per driving step the PTC1 and
//! PEV1 records it answers: `u32 step` (the step after it), `u32 length`, then the records. The viewer's playback of
//! the recording reads it for what the recording lacks, the lamps' reverse switch and damage above all
//! (`rbr_render::particles::sidecar`). The car's pose of each step is compared with the recording's, bit for bit, and
//! the first step that differs is reported (the replay's records from there are not the recording's).

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use rbr_gameplay::stage::State;
use rbr_recording::{pose, Frame, Frames};

use crate::autopilot::Pilot;
use crate::live::Live;
use crate::stage::StageData;

/// What a run wrote: the steps, and the first whose car pose differs from the recording's.
pub struct Written {
    pub steps: usize,
    pub diverged: Option<u32>,
}

/// The car's position and orientation bits of a frame.
fn pose_bits(frame: &Frame) -> Option<[u32; 7]> {
    let pose = pose::car_pose(frame.car)?;
    let mut out = [0; 7];
    for (o, v) in out.iter_mut().zip(pose.position.into_iter().chain(pose.orientation)) {
        *o = v.to_bits();
    }
    Some(out)
}

/// Writes `out` from `recording`'s controls.
pub fn write(mut live: Live, stage: &StageData, recording: &Path, out: &Path) -> Result<Written, String> {
    live.pace(f64::from(rbr_recording::STEP_SECONDS));
    let what = recording.to_str().ok_or("the recording's path is not UTF-8")?;
    let mut pilot = Pilot::new(what, stage, &live)?;
    let bytes = std::fs::read(recording).map_err(|e| format!("{}: {e}", recording.display()))?;
    let recorded: HashMap<u32, [u32; 7]> =
        Frames::new(&bytes).map_while(Result::ok).filter_map(|f| Some((f.post, pose_bits(&f)?))).collect();
    let mut diverged = None;
    let mut file = std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?);
    let mut written = 0;
    while live.game().stage().state() != State::Ended {
        let pre = live.next_step();
        // Past the recording's last step the drive is over.
        let Some(payload) = pilot.payload(&live, None) else { break };
        let answer = live.answer(&payload)?;
        let (records, stopped) = live.particle_records();
        if stopped {
            return Err(format!("step {pre}: the live boot stopped"));
        }
        if live.next_step() > pre {
            if diverged.is_none() {
                let ours = Frames::new(&answer).next().and_then(Result::ok).and_then(|f| pose_bits(&f));
                if ours.is_none() || ours != recorded.get(&live.next_step()).copied() {
                    diverged = Some(live.next_step());
                }
            }
            let length = records.iter().map(|r| r.len()).sum::<usize>() as u32;
            [&live.next_step().to_le_bytes()[..], &length.to_le_bytes(), records[0], records[1]]
                .iter()
                .try_for_each(|bytes| file.write_all(bytes))
                .map_err(|e| format!("{}: {e}", out.display()))?;
            written += 1;
        }
    }
    file.flush().map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(Written { steps: written, diverged })
}

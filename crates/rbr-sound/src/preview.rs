//! The co-driver preview: the corner ladder and one distance call as one wave, which the launcher's menu sound plays
//! like the race's speech voice ([`crate::menu::MenuPlayer::preview`]), so players hear a voice, style and range set
//! before driving. Only the takes heard are decoded.

use std::path::Path;

use rbr_formats::audio::{self, AudioDat};
use rbr_formats::data::Data;
use rbr_formats::rsf::codriver;

use crate::data::Wave;
use crate::mixer::voices::interpolate;

/// The left-corner ladder, tightest to fastest: the plugin ids `0, 102, 1-5`, then flat (`26`).
pub const LADDER: [u32; 8] = [0, 102, 1, 2, 3, 4, 5, 26];
/// 1.02's left corners, tightest to fastest, as `CoDriver.ini` calls.
const ORIGINAL_LADDER: [&str; 7] =
    ["HAIRPINLEFT", "90_LEFT", "KLEFT", "MEDIUMLEFT", "FASTLEFT", "EASYLEFT", "FLATLEFT"];
/// The distance call at the end, metres.
pub const DISTANCE: u32 = 100;
/// Silence between calls, seconds.
pub const GAP: f32 = 0.3;

/// The calls said, in order, and the wave saying them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub calls: Vec<String>,
    pub wave: Wave,
}

/// The takes joined at the first one's rate, [`GAP`] apart.
fn join(takes: Vec<(String, Wave)>) -> Preview {
    let rate = takes.first().map_or(44100, |(_, w)| w.rate);
    let gap = vec![0.0; (GAP * rate as f32) as usize];
    let mut preview = Preview { calls: Vec::new(), wave: Wave { rate, samples: Vec::new() } };
    for (k, (call, wave)) in takes.into_iter().enumerate() {
        if k > 0 {
            preview.wave.samples.extend(&gap);
        }
        let step = f64::from(wave.rate) / f64::from(rate);
        let n = (wave.samples.len() as f64 / step) as usize;
        preview.wave.samples.extend((0..n).map(|i| interpolate(&wave.samples, false, i as f64 * step, step)));
        preview.calls.push(call);
    }
    preview
}

/// A Pacenote plugin voice saying the style's ladder (skipping the ids it lacks) and the range set's [`DISTANCE`],
/// first takes only.
pub fn custom_preview(plugin: &Path, voice: &str, style: &str, ranges: &str) -> Result<Preview, String> {
    let calls = codriver::load(plugin, style, ranges)?;
    let folder = codriver::voice_dir(plugin, voice).ok_or_else(|| format!("{}: no voice {voice}", plugin.display()))?;
    let take = |call: &codriver::Call| {
        let path = rbr_formats::stage_files::in_dir(&folder, call.sounds.first()?)?;
        Some((call.name.clone(), Wave::decode_ogg(&std::fs::read(path).ok()?).ok()?))
    };
    let ladder = LADDER.iter().filter_map(|id| calls.notes.get(id));
    Ok(join(ladder.chain(calls.ranges.get(&DISTANCE)).filter_map(take).collect()))
}

/// 1.02's co-driver saying its ladder and [`DISTANCE`] from the retail bank (`CoDriver.ini`, `Numbers.ini`), first
/// takes only.
pub fn original_preview(data: &Data) -> Result<Preview, String> {
    let dat = AudioDat::open(&data.audio_dat().ok_or("no Root/audio.dat")?)?;
    let (notes, numbers) = (dat.ini("Audio\\CoDriver.ini")?, dat.ini("Audio\\Numbers.ini")?);
    let take = |ini: &[rbr_formats::ini::Section], section: String, dir: &str, call: String| {
        let name = audio::text(ini, &section, "Snd0")?;
        let file = dat.file(&format!("Audio\\Speech\\{dir}\\{}.ogg", name.trim()))?;
        Some((call, Wave::decode_ogg(file).ok()?))
    };
    let ladder = ORIGINAL_LADDER.iter().map(|call| (&notes, format!("PACENOTE::{call}"), "PaceNote", call.to_string()));
    let distance = (&numbers, format!("RANGE::{DISTANCE}"), "Number", DISTANCE.to_string());
    Ok(join(ladder.chain([distance]).filter_map(|(ini, section, dir, call)| take(ini, section, dir, call)).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_join_at_the_first_rate_with_gaps() {
        let take = |rate, samples: Vec<f32>| ("c".to_owned(), Wave { rate, samples });
        let preview = join(vec![take(10, vec![1.0, 1.0]), take(20, vec![2.0; 4])]);
        assert_eq!(preview.wave.rate, 10);
        assert_eq!(preview.calls.len(), 2);
        let gap = (GAP * 10.0) as usize;
        assert_eq!(preview.wave.samples.len(), 2 + gap + 2, "the 20 Hz take is resampled to 10 Hz");
        assert!(preview.wave.samples[2..2 + gap].iter().all(|&s| s == 0.0));
        assert!(join(Vec::new()).wave.samples.is_empty());
    }
}

//! The co-driver preview: the corner ladder (and one distance call) as one PCM buffer, played from the
//! launcher's menus so players hear what a voice, style and range set sound like before driving.
//!
//! [`Preview`] is built by [`custom_preview`] (a Pacenote plugin voice, loading only the takes it needs) or
//! [`original_preview`] (1.02's retail bank, first takes only), resampled with the mixer's own interpolation
//! ([`crate::mixer::voices`]) and joined with [`GAP`] of silence between calls. The gain is the in-race speech gain
//! times the master's ([`gain`]); the limiter is the menu mixer's, at play time.

use std::path::Path;

use rbr_formats::audio::{self, AudioDat};
use rbr_formats::data::Data;
use rbr_formats::ini::Section;
use rbr_formats::rsf::codriver;

use crate::data::Wave;
use crate::mixer::voices::interpolate;
use crate::mixer::{FULL_SCALE, HEADROOM};

/// The preview's left-corner ladder, tightest to fastest: the plugin ids `0, 102, 1-5`, then flat (`26`).
pub const LADDER: [u32; 8] = [0, 102, 1, 2, 3, 4, 5, 26];
/// Silence between preview calls, seconds.
pub const GAP: f32 = 0.3;
/// The preview's distance call, metres (when the range set has it).
pub const DISTANCE: u32 = 100;
/// 1.02's left corners, tightest to fastest, as `CoDriver.ini` bank names.
const ORIGINAL_LADDER: [&str; 7] =
    ["HAIRPINLEFT", "90_LEFT", "KLEFT", "MEDIUMLEFT", "FASTLEFT", "EASYLEFT", "FLATLEFT"];

/// One preview: the samples to play (mono f32, -1 to 1, at the rate built at) and what they say, in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub samples: Vec<f32>,
    /// What each segment says: a custom call's name, or a retail `CoDriver.ini`/`Numbers.ini` bank name (`100` last).
    pub calls: Vec<String>,
}

/// The preview's linear gain on -1..1 samples: the in-race speech gain (`Volumes::codriver`) times the master's.
pub fn gain(master: f32, codriver: f32) -> f32 {
    codriver * master * HEADROOM
}

/// `wave` resampled to `rate`, mono -1..1, with the mixer's interpolation.
fn resample(wave: &Wave, rate: u32) -> Vec<f32> {
    if wave.samples.is_empty() || rate == 0 {
        return Vec::new();
    }
    let step = f64::from(wave.rate) / f64::from(rate);
    let n = (wave.samples.len() as f64 / step).ceil() as usize;
    (0..n).map(|i| interpolate(&wave.samples, false, i as f64 * step, step) / FULL_SCALE).collect()
}

/// The segments joined with [`GAP`] of silence between calls, scaled by `gain`.
fn join(segments: Vec<Vec<f32>>, rate: u32, gain: f32) -> Vec<f32> {
    let gap = (GAP * rate as f32) as usize;
    let mut out = Vec::new();
    for (k, segment) in segments.into_iter().enumerate() {
        if k > 0 {
            out.resize(out.len() + gap, 0.0);
        }
        out.extend(segment);
    }
    for s in &mut out {
        *s *= gain;
    }
    out
}

/// The first take of the retail `section` (`dir` under `Audio\Speech`), decoded, if it has one.
fn first_take(dat: &AudioDat, ini: &[Section], section: &str, dir: &str) -> Option<(String, Wave)> {
    let count = audio::float(ini, section, "Sounds").unwrap_or(0.0) as usize;
    (0..count)
        .filter_map(|k| audio::text(ini, section, &format!("Snd{k}")))
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .filter_map(|name| {
            let file = format!("Audio\\Speech\\{dir}\\{name}.ogg");
            let wave = Wave::decode_ogg(dat.file(&file)?).ok()?;
            Some((name, wave))
        })
        .next()
}

/// The preview of a Pacenote plugin voice saying the style's ladder (the ids it lacks are skipped) and the range
/// set's [`DISTANCE`] call: the stage notes play no part. Only the takes heard are loaded and decoded.
pub fn custom_preview(
    plugin: &Path,
    voice: &str,
    style: &str,
    ranges: &str,
    rate: u32,
    master: f32,
    codriver_gain: f32,
) -> Result<Preview, String> {
    let calls = codriver::load(plugin, style, ranges)?;
    let folder = codriver::voice_dir(plugin, voice).ok_or_else(|| format!("{}: no voice {voice}", plugin.display()))?;
    let mut segments = Vec::new();
    let mut names = Vec::new();
    let take = |file: &str| -> Option<Wave> {
        let path = rbr_formats::stage_files::in_dir(&folder, file)?;
        Wave::decode_ogg(&std::fs::read(path).ok()?).ok()
    };
    for id in LADDER {
        let Some(call) = calls.notes.get(&id) else { continue };
        let Some(file) = call.sounds.first() else { continue };
        let Some(wave) = take(file) else { continue };
        segments.push(resample(&wave, rate));
        names.push(call.name.clone());
    }
    if let Some(call) = calls.ranges.get(&DISTANCE) {
        if let Some(file) = call.sounds.first() {
            if let Some(wave) = take(file) {
                segments.push(resample(&wave, rate));
                names.push(call.name.clone());
            }
        }
    }
    let samples = join(segments, rate, gain(master, codriver_gain));
    Ok(Preview { samples, calls: names })
}

/// The preview of 1.02's co-driver saying its own ladder ([`ORIGINAL_LADDER`]) and `100`, from the retail bank
/// (`CoDriver.ini`, `Numbers.ini`): first takes only.
pub fn original_preview(data: &Data, rate: u32, master: f32, codriver_gain: f32) -> Result<Preview, String> {
    let path = data.audio_dat().ok_or("no Root/audio.dat")?;
    let dat = AudioDat::open(&path)?;
    let notes = dat.ini("Audio\\CoDriver.ini")?;
    let numbers = dat.ini("Audio\\Numbers.ini")?;
    let mut segments = Vec::new();
    let mut names = Vec::new();
    for name in ORIGINAL_LADDER {
        if let Some((_, wave)) = first_take(&dat, &notes, &format!("PACENOTE::{name}"), "PaceNote") {
            segments.push(resample(&wave, rate));
            names.push(name.to_owned());
        }
    }
    if let Some((_, wave)) = first_take(&dat, &numbers, &format!("RANGE::{DISTANCE}"), "Number") {
        segments.push(resample(&wave, rate));
        names.push(DISTANCE.to_string());
    }
    let samples = join(segments, rate, gain(master, codriver_gain));
    Ok(Preview { samples, calls: names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_a_whole_sample_and_joins_with_silence() {
        let wave = Wave { rate: 44100, samples: vec![FULL_SCALE, -FULL_SCALE] };
        assert_eq!(resample(&wave, 44100), [1.0, -1.0]);
        let joined = join(vec![vec![0.5], vec![0.5]], 10, 1.0);
        assert_eq!(joined.len(), 2 + (GAP * 10.0) as usize);
        assert_eq!(&joined[..1], [0.5]);
        assert!(joined[1..1 + (GAP * 10.0) as usize].iter().all(|&s| s == 0.0));
        assert_eq!(gain(2.0, 1.0), HEADROOM * 2.0);
    }

    #[test]
    fn joining_nothing_is_silence() {
        assert!(join(Vec::new(), 44100, 1.0).is_empty());
    }
}

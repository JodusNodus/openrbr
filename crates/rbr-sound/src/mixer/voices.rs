//! The mixer's voices and filters: the looping and one-shot sample voices, the gains that move toward their target,
//! the plain balance, the band-limited interpolation, the loop seams and the one-shots' edges (ours) and the one-pole
//! low-pass of `E32Flt`.

use std::f32::consts::PI;
use std::sync::Arc;

use crate::data::Wave;
use crate::voice::Material;

/// The interpolation's taps per output sample and its table's phases (fractions of a sample).
const TAPS: usize = 16;
const PHASES: usize = 256;
/// The cutoffs of the tables, as fractions of the source's Nyquist: a voice read `step` samples apart takes the
/// first at or below `1 / step`, so a raised pitch drops what would fold back. Below a quarter (pitch above 4)
/// some aliasing is left.
const BANDS: [f64; 5] = [1.0, 0.75, 0.5, 0.35, 0.25];

/// Each band's polyphase table: for each phase `p` the [`TAPS`] weights of the samples from 7 before to 8 after,
/// read at `p / PHASES` past a sample: a sinc at the band's cutoff under a Blackman window 8 samples wide on each
/// side, each phase scaled to a DC gain of 1.
type Table = [[f32; TAPS]; PHASES + 1];

fn tables() -> &'static [Table; BANDS.len()] {
    static TABLES: std::sync::OnceLock<Box<[Table; BANDS.len()]>> = std::sync::OnceLock::new();
    TABLES.get_or_init(|| {
        let half = (TAPS / 2) as f64;
        Box::new(BANDS.map(|band| {
            std::array::from_fn(|p| {
                let weight = |k: usize| {
                    let x = k as f64 - (half - 1.0) - p as f64 / PHASES as f64;
                    let a = std::f64::consts::PI * x / half;
                    let window = if x.abs() < half { 0.42 + 0.5 * a.cos() + 0.08 * (2.0 * a).cos() } else { 0.0 };
                    let y = std::f64::consts::PI * band * x;
                    let sinc = if y == 0.0 { 1.0 } else { y.sin() / y };
                    sinc * window
                };
                let sum: f64 = (0..TAPS).map(weight).sum();
                std::array::from_fn(|k| (weight(k) / sum) as f32)
            })
        }))
    })
}

/// The sample at `pos` of `samples` (looping, or silent past either end) read `step` samples apart: band-limited
/// interpolation (ours, standing for DirectSound's resampler, which Wine and Windows since Vista run as a FIR), a
/// 16-tap windowed sinc from a precomputed polyphase table whose cutoff is the source's Nyquist, or below the
/// output's when `step` is above 1 (a raised pitch, [`BANDS`]). On a whole sample at `step` 1 or less it is the
/// sample itself.
pub(crate) fn interpolate(samples: &[f32], looping: bool, pos: f64, step: f64) -> f32 {
    let base = pos.floor();
    let phase = ((pos - base) * PHASES as f64).round() as usize;
    let base = base as i64;
    let n = samples.len() as i64;
    if phase == 0 && step <= 1.0 {
        return match base {
            i if looping => samples[i.rem_euclid(n) as usize],
            i => usize::try_from(i).ok().and_then(|i| samples.get(i)).copied().unwrap_or(0.0),
        };
    }
    let band = BANDS.iter().position(|&b| b * step <= 1.0).unwrap_or(BANDS.len() - 1);
    let row = &tables()[band][phase];
    let first = base - (TAPS as i64 / 2 - 1);
    if first >= 0 && first + TAPS as i64 <= n {
        let window = &samples[first as usize..first as usize + TAPS];
        return window.iter().zip(row).map(|(x, h)| x * h).sum();
    }
    let at = |i: i64| match i {
        i if looping => samples[i.rem_euclid(n) as usize],
        i => usize::try_from(i).ok().and_then(|i| samples.get(i)).copied().unwrap_or(0.0),
    };
    row.iter().enumerate().map(|(k, h)| at(first + k as i64) * h).sum()
}

/// The fade over a one-shot's first and last samples, and over a stopped one-shot's rest, seconds (ours: many retail
/// samples start or end far from zero, and a stopped voice would end mid-wave; either clicks).
const EDGE_IN: f32 = 0.001;
const EDGE_OUT: f32 = 0.004;
const RELEASE: f32 = 0.006;
/// The crossfade over a loop's seam, seconds, at most a quarter of the loop (ours).
const SEAM: f32 = 0.02;

/// `wave` as a loop without a step at its seam (ours): its last samples fade into its first, which are then left out,
/// so the loop is shorter by the fade and the samples on either side of the seam are the recording's own neighbours.
pub(crate) fn seamless(wave: &Wave) -> Wave {
    let x = &wave.samples;
    let fade = ((SEAM * wave.rate as f32) as usize).min(x.len() / 4);
    if fade == 0 {
        return wave.clone();
    }
    let start = x.len() - fade;
    let samples = (fade..x.len())
        .map(|i| match i.checked_sub(start) {
            Some(k) => {
                let w = 0.5 - 0.5 * (PI * (k as f32 + 0.5) / fade as f32).cos();
                x[i] * (1.0 - w) + x[k] * w
            }
            None => x[i],
        })
        .collect();
    Wave { rate: wave.rate, samples }
}

/// A gain that moves toward its target.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Gain {
    pub now: f32,
    pub target: f32,
}

impl Gain {
    pub(crate) fn next(&mut self, k: f32) -> f32 {
        self.now += (self.target - self.now) * k;
        self.now
    }
}

/// The left and right gains of a voice on side `pan` (-1 left to 1 right): a plain balance, both 1 in the middle
/// (ours).
pub(crate) fn balance(pan: f32) -> [f32; 2] {
    [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)]
}

/// A looping sample voice.
#[derive(Debug, Clone, Default)]
pub(crate) struct LoopVoice {
    pub wave: Arc<Wave>,
    pub pos: f64,
    pub pitch: f32,
    pub gain: Gain,
    pub pan: f32,
    /// The in-car low-pass, when it takes one.
    pub low_pass: Option<OnePole>,
}

impl LoopVoice {
    /// Whether it sounds (or is fading in or out).
    pub(crate) fn audible(&mut self) -> bool {
        if self.wave.samples.is_empty() || (self.gain.target == 0.0 && self.gain.now.abs() < 1e-5) {
            self.gain.now = 0.0;
            return false;
        }
        true
    }

    pub(crate) fn add(&mut self, out: &mut [[f32; 2]], rate: u32, k: f32) {
        let side = balance(self.pan);
        self.each(out.len(), rate, k, |i, s| {
            out[i][0] += s * side[0];
            out[i][1] += s * side[1];
        });
    }

    /// Adds it to the mono `out` at `scale` (an FMOD bus).
    pub(crate) fn add_mono(&mut self, out: &mut [f32], rate: u32, k: f32, scale: f32) {
        self.each(out.len(), rate, k, |i, s| out[i] += s * scale);
    }

    fn each(&mut self, len: usize, rate: u32, k: f32, mut f: impl FnMut(usize, f32)) {
        if !self.audible() {
            return;
        }
        let n = self.wave.samples.len() as f64;
        let step = f64::from(self.pitch) * f64::from(self.wave.rate) / f64::from(rate);
        for i in 0..len {
            let s = interpolate(&self.wave.samples, true, self.pos, step);
            let s = self.low_pass.as_mut().map_or(s, |f| f.run(s));
            f(i, s * self.gain.next(k));
            self.pos = (self.pos + step) % n;
        }
    }
}

/// A one-shot playing.
#[derive(Debug, Clone)]
pub(crate) struct Playing {
    pub wave: Arc<Wave>,
    pub pos: f64,
    pub gain: f32,
    pub pan: f32,
    /// One of the ranged impacts (they are capped, [`MAX_RANGED`](super::MAX_RANGED)).
    pub ranged: bool,
    /// The playback rate (1 = the recorded rate).
    pub pitch: f32,
    /// The one voice it plays on, for the shots that have one ([`Repeat`](crate::voice::Repeat)).
    pub family: Option<Family>,
    /// Once stopped, the output samples left of its fade ([`Playing::stop`]).
    pub release: Option<f32>,
    /// The in-car low-pass, when it takes one.
    pub low_pass: Option<OnePole>,
}

/// The one-shots that each have one voice (`CCarSound +0x41700`, `+0x41704`, `+0x41708`, `+0x108c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    ExhaustPop,
    Pop,
    Bang,
    Fire,
    /// `+0x1084`.
    Ignition,
    /// `E31Turbo +0x7c`.
    BlowOff,
}

impl Playing {
    pub(crate) fn new(wave: Arc<Wave>, gain: f32) -> Playing {
        Playing {
            wave,
            pos: 0.0,
            gain,
            pan: 0.0,
            ranged: false,
            pitch: 1.0,
            family: None,
            release: None,
            low_pass: None,
        }
    }

    /// Fades it out over [`RELEASE`] instead of ending at once.
    pub(crate) fn stop(&mut self, rate: u32) {
        self.release.get_or_insert(RELEASE * rate as f32);
    }

    /// Adds the sample to `out`; false once it has ended.
    pub(crate) fn add(&mut self, out: &mut [[f32; 2]], rate: u32) -> bool {
        let side = balance(self.pan).map(|g| g * self.gain);
        self.each(out.len(), rate, |i, s| {
            out[i][0] += s * side[0];
            out[i][1] += s * side[1];
        })
    }

    /// Adds the sample to the mono `out` at `scale` (an FMOD bus); false once it has ended.
    pub(crate) fn add_mono(&mut self, out: &mut [f32], rate: u32, scale: f32) -> bool {
        let g = self.gain * scale;
        self.each(out.len(), rate, |i, s| out[i] += s * g)
    }

    fn each(&mut self, len: usize, rate: u32, mut f: impl FnMut(usize, f32)) -> bool {
        let step = f64::from(self.wave.rate) * f64::from(self.pitch) / f64::from(rate);
        let samples = &self.wave.samples;
        let end = samples.len() as f64;
        let (fade_in, fade_out) =
            (f64::from(EDGE_IN * self.wave.rate as f32), f64::from(EDGE_OUT * self.wave.rate as f32));
        let full = RELEASE * rate as f32;
        for i in 0..len {
            if self.pos >= end || self.release.is_some_and(|r| r <= 0.0) {
                return false;
            }
            let edge = (self.pos / fade_in).min((end - self.pos) / fade_out).min(1.0) as f32;
            let release = self.release.as_mut().map_or(1.0, |r| {
                *r -= 1.0;
                *r / full
            });
            let s = interpolate(samples, false, self.pos, step);
            let s = self.low_pass.as_mut().map_or(s, |f| f.run(s));
            f(i, s * edge * release);
            self.pos += step;
        }
        self.pos < end && self.release.is_none_or(|r| r > 0.0)
    }
}

/// A tyre voice: the loop of its material and skid state.
#[derive(Debug, Clone, Default)]
pub(crate) struct TyreVoice {
    pub key: Option<(Material, bool)>,
    pub voice: LoopVoice,
}

/// A one-pole low-pass, the bilinear form of `E32Flt` (`E32Flt::Filter` 0x4da1e0) and `E25Engine::Filter80`
/// (0x4b8780): with `p = 2 pi cutoff`, `y = (2r - p) / (2r + p) * y + p / (2r + p) * (x + x')`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OnePole {
    g1: f32,
    g2: f32,
    y: f32,
    x: f32,
}

impl OnePole {
    pub(crate) fn new(rate: u32, cutoff: f32) -> OnePole {
        let mut f = OnePole::default();
        f.retune(rate, cutoff);
        f
    }

    /// New coefficients, keeping the state.
    pub(crate) fn retune(&mut self, rate: u32, cutoff: f32) {
        let (p, two_rate) = (cutoff * 2.0 * PI, 2.0 * rate as f32);
        (self.g1, self.g2) = ((two_rate - p) / (two_rate + p), p / (two_rate + p));
    }

    pub(crate) fn run(&mut self, x: f32) -> f32 {
        self.y = self.g1 * self.y + self.g2 * self.x + self.g2 * x;
        self.x = x;
        self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The amplitude of the `freq` Hz component of `x` at `rate`.
    fn level(x: &[f32], rate: f32, freq: f32) -> f32 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in x.iter().enumerate() {
            let a = 2.0 * std::f64::consts::PI * f64::from(freq) * i as f64 / f64::from(rate);
            re += f64::from(v) * a.cos();
            im += f64::from(v) * a.sin();
        }
        ((re * re + im * im).sqrt() * 2.0 / x.len() as f64) as f32
    }

    /// `n` output samples of a `freq` Hz sine at `rate` read `step` samples apart.
    fn resampled(rate: f32, freq: f32, step: f64, n: usize) -> Vec<f32> {
        let x: Vec<f32> =
            (0..(n as f64 * step) as usize + 64).map(|i| (2.0 * PI * freq * i as f32 / rate).sin()).collect();
        (0..n).map(|k| interpolate(&x, false, 32.0 + k as f64 * step, step)).collect()
    }

    #[test]
    fn interpolation_keeps_the_tone_and_leaves_no_images() {
        // An 11025 Hz voice's 1 kHz tone at 44100: the tone stays, its images (10025, 12025 Hz) do not.
        let y = resampled(11025.0, 1000.0, 0.25, 44100);
        assert!((level(&y, 44100.0, 1000.0) - 1.0).abs() < 0.01);
        assert!(level(&y, 44100.0, 10025.0) < 1e-3);
        assert!(level(&y, 44100.0, 12025.0) < 1e-3);
    }

    #[test]
    fn a_raised_pitch_drops_what_would_alias() {
        // Played at 3x, 15 kHz becomes 45 kHz, above the output's Nyquist: linear interpolation folds it to 900 Hz.
        let y = resampled(44100.0, 15000.0, 3.0, 44100);
        assert!(level(&y, 44100.0, 900.0) < 1e-3, "{}", level(&y, 44100.0, 900.0));
        // 1.5 kHz at 3x is 4.5 kHz and stays, a little softer (the band's 16 taps).
        let y = resampled(44100.0, 1500.0, 3.0, 44100);
        assert!((0.9..1.0).contains(&level(&y, 44100.0, 4500.0)), "{}", level(&y, 44100.0, 4500.0));
    }

    #[test]
    fn whole_samples_at_the_recorded_rate_are_the_samples() {
        let x = [1.0, -2.0, 3.0];
        assert_eq!(interpolate(&x, true, 4.0, 1.0), -2.0);
        assert_eq!(interpolate(&x, false, 4.0, 1.0), 0.0);
    }

    #[test]
    fn loops_play_at_their_pitch_and_side() {
        // A 200 Hz sample at half pitch into 100 Hz: one sample a frame, wrapping at the end.
        let wave = Wave { rate: 200, samples: (0..100).map(|i| i as f32).collect() };
        let mut voice = LoopVoice {
            wave: Arc::new(wave),
            pitch: 0.5,
            gain: Gain { now: 1.0, target: 1.0 },
            pos: 0.0,
            pan: 0.5,
            low_pass: None,
        };
        let mut out = vec![[0.0; 2]; 110];
        voice.add(&mut out, 100, 1.0);
        assert_eq!(out[..3], [[0.0, 0.0], [0.5, 1.0], [1.0, 2.0]]);
        assert_eq!(out[100], [0.0, 0.0]);
    }

    #[test]
    fn a_seamless_loop_wraps_onto_its_own_neighbours() {
        // A ramp jumps from 999 back to 0 at its seam; the seamless loop ends next to where it starts.
        let wave = Wave { rate: 1000, samples: (0..1000).map(|i| i as f32).collect() };
        let fade = (SEAM * 1000.0) as usize;
        let looped = seamless(&wave);
        assert_eq!(looped.samples.len(), 1000 - fade);
        assert_eq!(looped.samples[0], fade as f32);
        let last = *looped.samples.last().unwrap();
        assert!((last - (fade - 1) as f32).abs() < 10.0, "{last}");
    }

    #[test]
    fn one_shots_fade_at_their_edges_and_when_stopped() {
        let wave = Arc::new(Wave { rate: 1000, samples: vec![1.0; 100] });
        let mut out = vec![[0.0; 2]; 100];
        assert!(!Playing::new(wave.clone(), 1.0).add(&mut out, 1000));
        assert_eq!((out[0][0], out[50][0]), (0.0, 1.0));
        assert!(out[99][0] < 0.5);
        let mut playing = Playing::new(wave, 1.0);
        let mut out = vec![[0.0; 2]; 10];
        assert!(playing.add(&mut out, 1000));
        playing.stop(1000);
        let mut out = vec![[0.0; 2]; 10];
        assert!(!playing.add(&mut out, 1000));
        assert!(out[0][0] > out[3][0] && out[6][0] == 0.0);
    }

    #[test]
    fn the_one_pole_passes_the_lows_and_halves_at_its_cutoff() {
        let mut f = OnePole::new(44100, 300.0);
        assert!((0..2000).map(|_| f.run(1.0)).last().unwrap() > 0.999);
        let mut f = OnePole::new(44100, 300.0);
        let n = 44100;
        let y: Vec<f32> = (0..n).map(|i| f.run((2.0 * PI * 300.0 * i as f32 / 44100.0).sin())).collect();
        let rms = (y[n / 2..].iter().map(|v| v * v).sum::<f32>() / (n / 2) as f32).sqrt() * 2f32.sqrt();
        assert!((rms - 0.707).abs() < 0.02, "{rms}");
    }
}

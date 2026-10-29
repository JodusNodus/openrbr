//! An FMOD bank's effects on one bus of its mixing tree (`super::fmod_mix`; ours, standing for FMOD's DSP), in
//! order, with the bank's parameters and automations ([`rbr_formats::fmod::Effect`]).
//!
//! - `LOWPASS_SIMPLE` ("two single-pole RC modules" in FMOD's documentation): two one-pole low-passes at the cutoff;
//!   `HIGHPASS_SIMPLE` one one-pole high-pass.
//! - `LOWPASS` and `HIGHPASS`: an RBJ biquad at the cutoff, its Q the resonance. `PARAMEQ`: RBJ's peaking EQ (centre,
//!   bandwidth in octaves, gain).
//! - `THREE_EQ`: the low band below the low crossover, the high band above the high one (Butterworth low-passes of
//!   order 2, 4 or 8 by the slope, the bands as differences so they sum back to the input), each at its gain.
//! - `FADER`: its gain. A send: the signal at its level into its return bus.
//! - `COMPRESSOR`: a peak envelope (attack and release one-poles) and a gain of `(env / threshold)^(1/ratio - 1)`
//!   above the threshold, then the make-up gain.
//! - `SFXREVERB`: the dry level plus, unless the wet level is -80 dB, a small reverb ([`Reverb`]): two early taps
//!   at the early delay and a four-line feedback delay network fed after the early plus late delay, its lines
//!   decaying in the decay time, mixed by the early/late mix, through the high cut and the low shelf, at the wet
//!   level. Density, diffusion and the HF decay ratio (100 in the banks) are not modelled.
//!
//! - `CONVOLUTIONREVERB`: its dry level; the wet signal (the impulse response) is left out.
//!
//! Left out: the panner and channel mix (the bank's voices are mono here), and the distortion.
//! The loudness meter does not touch the sound.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

use rbr_formats::fmod::{
    Effect, DSP_COMPRESSOR, DSP_CONVOLUTION_REVERB, DSP_FADER, DSP_HIGHPASS, DSP_HIGHPASS_SIMPLE, DSP_LOWPASS,
    DSP_LOWPASS_SIMPLE, DSP_PARAMEQ, DSP_SEND, DSP_SFXREVERB, DSP_THREE_EQ,
};

/// `SFXREVERB`'s parameters: decay time (ms), early delay (ms), late delay (ms), low shelf frequency and gain (dB),
/// high cut (Hz), early/late mix (%), wet and dry levels (dB).
const SFXREVERB_DECAY: usize = 0;
const SFXREVERB_EARLY: usize = 1;
const SFXREVERB_LATE: usize = 2;
const SFXREVERB_SHELF_FREQUENCY: usize = 7;
const SFXREVERB_SHELF_GAIN: usize = 8;
const SFXREVERB_HIGH_CUT: usize = 9;
const SFXREVERB_MIX: usize = 10;
const SFXREVERB_WET: usize = 11;
const SFXREVERB_DRY: usize = 12;
/// `CONVOLUTIONREVERB`'s dry level parameter.
const CONVOLUTION_DRY: usize = 2;
/// How often the compressor works out its gain, samples (ours).
const COMPRESSOR_EVERY: u32 = 8;
/// A wet level at or below this is off (FMOD's floor).
const SILENT_DB: f32 = -80.0;
/// The most parameters any effect the code models reads (`SFXREVERB`'s 0..=12): parameter indices at or above
/// this are ignored, so a corrupt `PROP` word (effect parameter `p - 1000`, e.g. 0xFFFFFFFF) cannot grow the Vec
/// by gigabytes inside `Mixer::apply`.
const MAX_EFFECT_PARAMS: usize = 13;

/// How many parameters an effect of `kind` reads in `Stage::tune` (indices below this are kept). Kinds the code
/// does not model read none of the ones it keeps, so they take the cap: storing more cannot change the sound.
fn param_limit(kind: u32) -> usize {
    match kind {
        DSP_LOWPASS_SIMPLE | DSP_HIGHPASS_SIMPLE | DSP_FADER | DSP_SEND => 1,
        DSP_LOWPASS | DSP_HIGHPASS => 2,
        DSP_PARAMEQ | DSP_CONVOLUTION_REVERB => 3,
        DSP_COMPRESSOR => 5,
        DSP_THREE_EQ => 6,
        DSP_SFXREVERB => MAX_EFFECT_PARAMS,
        _ => MAX_EFFECT_PARAMS,
    }
}
/// The reverb's delay lines (ms; ours), and the longest early plus late delay it keeps.
const LINES_MS: [f32; 4] = [29.7, 37.1, 41.1, 43.7];
const MAX_DELAY_S: f32 = 0.4;
/// The highest cutoff, a share of the output rate.
const MAX_CUTOFF: f32 = 0.45;

/// A biquad, direct form I.
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    /// RBJ's low-pass (`high` false) or high-pass at `cutoff` Hz.
    fn rbj(rate: u32, cutoff: f32, q: f32, high: bool) -> Biquad {
        let w = 2.0 * PI * cutoff.clamp(10.0, MAX_CUTOFF * rate as f32) / rate as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high { ((1.0 + cos) / 2.0, -(1.0 + cos)) } else { ((1.0 - cos) / 2.0, 1.0 - cos) };
        Biquad { b: [b0 / a0, b1 / a0, b0 / a0], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], ..Biquad::default() }
    }

    /// RBJ's peaking EQ at `centre` Hz, `octaves` wide, `gain` dB.
    fn peak(rate: u32, centre: f32, octaves: f32, gain: f32) -> Biquad {
        let w = 2.0 * PI * centre.clamp(10.0, MAX_CUTOFF * rate as f32) / rate as f32;
        let (sin, cos) = w.sin_cos();
        let alpha = sin * (std::f32::consts::LN_2 / 2.0 * octaves.max(0.05) * w / sin).sinh();
        let a = 10f32.powf(gain / 40.0);
        let a0 = 1.0 + alpha / a;
        let b = [(1.0 + alpha * a) / a0, -2.0 * cos / a0, (1.0 - alpha * a) / a0];
        Biquad { b, a: [-2.0 * cos / a0, (1.0 - alpha / a) / a0], ..Biquad::default() }
    }

    /// The bilinear one-pole low-pass (as [`super::voices::OnePole`]) or high-pass at `cutoff` Hz.
    fn one_pole(rate: u32, cutoff: f32, high: bool) -> Biquad {
        let (p, two_rate) = (2.0 * PI * cutoff.clamp(10.0, MAX_CUTOFF * rate as f32), 2.0 * rate as f32);
        let (g1, g2) = ((two_rate - p) / (two_rate + p), p / (two_rate + p));
        let b = if high { [two_rate / (two_rate + p), -two_rate / (two_rate + p), 0.0] } else { [g2, g2, 0.0] };
        Biquad { b, a: [-g1, 0.0], ..Biquad::default() }
    }

    /// New coefficients, keeping the state.
    fn retune(&mut self, to: Biquad) {
        (self.b, self.a) = (to.b, to.a);
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        (self.x, self.y) = ([x, self.x[0]], [y, self.y[0]]);
        y
    }
}

fn cascade(filters: &mut [Biquad], x: f32) -> f32 {
    filters.iter_mut().fold(x, |s, f| f.run(s))
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// One effect: its parameters, its filters and band gains from them.
#[derive(Debug, Clone)]
pub(crate) struct Stage {
    kind: u32,
    params: Vec<f32>,
    filters: Vec<Biquad>,
    gains: [f32; 3],
    /// A low-pass just switched on from open: its state starts at the next input.
    prime: bool,
    /// A send's return bus and what it sent this block.
    pub(crate) target: Option<usize>,
    pub(crate) sent: Vec<f32>,
    compressor: Compressor,
    reverb: Option<Box<Reverb>>,
}

/// `COMPRESSOR`'s state and settings.
#[derive(Debug, Clone, Copy, Default)]
struct Compressor {
    env: f32,
    /// The gain, worked out every [`COMPRESSOR_EVERY`] samples.
    gain: f32,
    count: u32,
    attack: f32,
    release: f32,
    threshold: f32,
    slope: f32,
    makeup: f32,
}

impl Compressor {
    fn tune(&mut self, rate: u32, p: impl Fn(usize, f32) -> f32) {
        let coef = |ms: f32| (-1000.0 / (ms.max(0.1) * rate as f32)).exp();
        (self.attack, self.release) = (coef(p(2, 20.0)), coef(p(3, 100.0)));
        (self.threshold, self.slope, self.makeup) = (db(p(0, 0.0)), 1.0 / p(1, 2.5).max(1.0) - 1.0, db(p(4, 0.0)));
    }

    fn run(&mut self, x: f32) -> f32 {
        let a = x.abs();
        let c = if a > self.env { self.attack } else { self.release };
        self.env = a + c * (self.env - a);
        if self.count == 0 {
            self.gain = if self.env > self.threshold { (self.env / self.threshold).powf(self.slope) } else { 1.0 };
            self.gain *= self.makeup;
        }
        self.count = (self.count + 1) % COMPRESSOR_EVERY;
        x * self.gain
    }
}

/// `SFXREVERB`'s wet signal (ours, see the module's notes).
#[derive(Debug, Clone)]
struct Reverb {
    input: Vec<f32>,
    at: usize,
    /// The early taps' and the network's delays, samples.
    early: [usize; 2],
    late: usize,
    lines: [Vec<f32>; 4],
    heads: [usize; 4],
    feedback: [f32; 4],
    /// The network's input scale, for a tail of about unit energy.
    scale: f32,
    mix: f32,
    cut: Biquad,
    shelf: Biquad,
    shelf_gain: f32,
    wet: f32,
}

impl Reverb {
    fn new(rate: u32) -> Reverb {
        let len = |ms: f32| (ms * rate as f32 / 1000.0) as usize;
        Reverb {
            input: vec![0.0; (MAX_DELAY_S * rate as f32) as usize + 1],
            at: 0,
            early: [0; 2],
            late: 0,
            lines: LINES_MS.map(|ms| vec![0.0; len(ms).max(1)]),
            heads: [0; 4],
            feedback: [0.0; 4],
            scale: 0.0,
            mix: 0.0,
            cut: Biquad::default(),
            shelf: Biquad::default(),
            shelf_gain: 1.0,
            wet: 0.0,
        }
    }

    fn tune(&mut self, rate: u32, p: impl Fn(usize, f32) -> f32) {
        let samples = |ms: f32| ((ms.max(0.0) * rate as f32 / 1000.0) as usize).min(self.input.len() - 1);
        let early = p(SFXREVERB_EARLY, 20.0);
        self.early = [samples(early), samples(early * 1.7)];
        self.late = samples(early + p(SFXREVERB_LATE, 40.0));
        let decay = p(SFXREVERB_DECAY, 1500.0).max(10.0) / 1000.0;
        self.feedback = std::array::from_fn(|k| 10f32.powf(-3.0 * self.lines[k].len() as f32 / (decay * rate as f32)));
        let mean = self.feedback.iter().map(|g| g * g).sum::<f32>() / 4.0;
        self.scale = (1.0 - mean).max(0.0).sqrt();
        self.mix = (p(SFXREVERB_MIX, 50.0) / 100.0).clamp(0.0, 1.0);
        self.cut.retune(Biquad::one_pole(rate, p(SFXREVERB_HIGH_CUT, 20000.0), false));
        self.shelf.retune(Biquad::one_pole(rate, p(SFXREVERB_SHELF_FREQUENCY, 250.0), false));
        self.shelf_gain = db(p(SFXREVERB_SHELF_GAIN, 0.0));
        self.wet = db(p(SFXREVERB_WET, -6.0));
    }

    fn run(&mut self, x: f32) -> f32 {
        let n = self.input.len();
        self.input[self.at] = x;
        let tap = |d: usize| self.input[if self.at >= d { self.at - d } else { self.at + n - d }];
        let early = 0.5 * (tap(self.early[0]) + tap(self.early[1]));
        let into = tap(self.late) * self.scale;
        self.at = if self.at + 1 == n { 0 } else { self.at + 1 };
        let outs: [f32; 4] = std::array::from_fn(|k| self.lines[k][self.heads[k]]);
        let half = 0.5 * (outs[0] + outs[1] + outs[2] + outs[3]);
        for (k, out) in outs.into_iter().enumerate() {
            let head = self.heads[k];
            self.lines[k][head] = into + self.feedback[k] * (out - half);
            self.heads[k] = if head + 1 == self.lines[k].len() { 0 } else { head + 1 };
        }
        let wet = self.cut.run((1.0 - self.mix) * early + self.mix * half);
        (wet + (self.shelf_gain - 1.0) * self.shelf.run(wet)) * self.wet
    }
}

impl Stage {
    fn param(&self, k: usize, default: f32) -> f32 {
        self.params.get(k).copied().unwrap_or(default)
    }

    /// The filters and gains for the parameters, the filters keeping their state.
    fn tune(&mut self, rate: u32) {
        let params = &self.params;
        let p = |k: usize, d: f32| params.get(k).copied().unwrap_or(d);
        // A low-pass at the top of its range (the banks' 22 kHz, FMOD's default) is open.
        let open = matches!(self.kind, DSP_LOWPASS_SIMPLE | DSP_LOWPASS) && p(0, 22000.0) >= MAX_CUTOFF * rate as f32;
        let (filters, gains) = match self.kind {
            _ if open => (Vec::new(), [1.0; 3]),
            DSP_LOWPASS_SIMPLE => (vec![Biquad::one_pole(rate, p(0, 22000.0), false); 2], [1.0; 3]),
            DSP_HIGHPASS_SIMPLE => (vec![Biquad::one_pole(rate, p(0, 10.0), true)], [1.0; 3]),
            DSP_LOWPASS | DSP_HIGHPASS => {
                (vec![Biquad::rbj(rate, p(0, 5000.0), p(1, 1.0), self.kind == DSP_HIGHPASS)], [1.0; 3])
            }
            DSP_PARAMEQ => (vec![Biquad::peak(rate, p(0, 8000.0), p(1, 1.0), p(2, 0.0))], [1.0; 3]),
            DSP_THREE_EQ => {
                let n = 1 << (p(5, 1.0) as usize).min(2);
                let low = p(3, 400.0);
                let mut f = vec![Biquad::rbj(rate, low, FRAC_1_SQRT_2, false); n];
                f.extend(vec![Biquad::rbj(rate, p(4, 4000.0).max(low), FRAC_1_SQRT_2, false); n]);
                (f, [db(p(0, 0.0)), db(p(1, 0.0)), db(p(2, 0.0))])
            }
            DSP_FADER | DSP_SEND => (Vec::new(), [db(p(0, 0.0)); 3]),
            DSP_SFXREVERB => (Vec::new(), [db(p(SFXREVERB_DRY, 0.0)); 3]),
            DSP_CONVOLUTION_REVERB => (Vec::new(), [db(p(CONVOLUTION_DRY, 0.0)); 3]),
            DSP_COMPRESSOR => {
                self.compressor.tune(rate, p);
                (Vec::new(), [1.0; 3])
            }
            _ => (Vec::new(), [1.0; 3]),
        };
        if self.filters.len() == filters.len() {
            self.filters.iter_mut().zip(filters).for_each(|(f, to)| f.retune(to));
        } else {
            self.prime = self.filters.is_empty();
            self.filters = filters;
        }
        self.gains = gains;
        if self.kind == DSP_SFXREVERB {
            if self.param(SFXREVERB_WET, -6.0) <= SILENT_DB {
                self.reverb = None;
            } else {
                let params = &self.params;
                let p = |k: usize, d: f32| params.get(k).copied().unwrap_or(d);
                self.reverb.get_or_insert_with(|| Box::new(Reverb::new(rate))).tune(rate, p);
            }
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        if self.prime {
            self.prime = false;
            for f in &mut self.filters {
                (f.x, f.y) = ([x; 2], [x; 2]);
            }
        }
        match self.kind {
            DSP_THREE_EQ => {
                let half = self.filters.len() / 2;
                let (low, high) = self.filters.split_at_mut(half);
                let [gl, gm, gh] = self.gains;
                let below = cascade(low, x);
                if gm == gh {
                    gl * below + gm * (x - below)
                } else {
                    let below_high = cascade(high, x);
                    gl * below + gm * (below_high - below) + gh * (x - below_high)
                }
            }
            DSP_FADER | DSP_CONVOLUTION_REVERB => x * self.gains[0],
            DSP_SEND => {
                self.sent.push(x * self.gains[0]);
                x
            }
            DSP_COMPRESSOR => self.compressor.run(x),
            DSP_SFXREVERB => x * self.gains[0] + self.reverb.as_mut().map_or(0.0, |r| r.run(x)),
            _ => cascade(&mut self.filters, x),
        }
    }
}

/// A bus's effects, in their order.
#[derive(Debug, Clone, Default)]
pub(crate) struct Chain {
    pub(crate) stages: Vec<Stage>,
}

impl Chain {
    pub(crate) fn new(effects: &[Effect], rate: u32) -> Chain {
        let stages = effects
            .iter()
            .map(|e| {
                let mut params = e.params.clone();
                params.truncate(param_limit(e.kind));
                let mut s = Stage {
                    params,
                    kind: e.kind,
                    filters: Vec::new(),
                    gains: [1.0; 3],
                    prime: false,
                    target: e.target,
                    sent: Vec::new(),
                    compressor: Compressor::default(),
                    reverb: None,
                };
                s.tune(rate);
                s
            })
            .collect();
        Chain { stages }
    }

    /// Effect `effect`'s parameter `param` to `value` (indices at or above its kind's count are ignored).
    pub(crate) fn set(&mut self, effect: usize, param: usize, value: f32, rate: u32) {
        let Some(s) = self.stages.get_mut(effect) else { return };
        if param >= param_limit(s.kind) {
            return;
        }
        if s.params.len() <= param {
            s.params.resize(param + 1, value);
        }
        if s.params[param] != value {
            s.params[param] = value;
            s.tune(rate);
        }
    }

    pub(crate) fn run(&mut self, x: f32) -> f32 {
        self.stages.iter_mut().fold(x, |s, stage| stage.run(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steady amplitude of a `freq` Hz sine through `chain` at 48 kHz.
    fn response(chain: &mut Chain, freq: f32) -> f32 {
        let w = 2.0 * PI * freq / 48000.0;
        (0..48000).map(|i| chain.run((w * i as f32).sin())).skip(24000).fold(0.0, |m: f32, y| m.max(y.abs()))
    }

    fn effect(kind: u32, params: &[f32]) -> Effect {
        Effect { kind, params: params.to_vec(), automations: Vec::new(), target: None }
    }

    #[test]
    fn three_eq_lifts_its_low_band() {
        // Fiesta_Rally2's `engine_int`: +10 dB below 1750 Hz, 24 dB a octave, the high crossover below the low one.
        let mut chain = Chain::new(&[effect(DSP_THREE_EQ, &[10.0, 0.0, 0.0, 1750.0, 10.0, 1.0])], 48000);
        assert!((response(&mut chain, 100.0) - db(10.0)).abs() < 0.05);
        assert!((response(&mut chain, 12000.0) - 1.0).abs() < 0.05);
    }

    #[test]
    fn a_simple_low_pass_follows_its_automated_cutoff() {
        let mut chain = Chain::new(&[effect(DSP_LOWPASS_SIMPLE, &[22000.0])], 48000);
        assert!(chain.stages[0].filters.is_empty() && response(&mut chain, 4000.0) > 0.99);
        chain.set(0, 0, 500.0, 48000);
        assert!(response(&mut chain, 4000.0) < 0.05);
        // Switched on, it starts from its input (no step).
        let mut chain = Chain::new(&[effect(DSP_LOWPASS_SIMPLE, &[22000.0])], 48000);
        chain.run(0.5);
        chain.set(0, 0, 5000.0, 48000);
        assert!((chain.run(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_huge_parameter_index_is_ignored() {
        // A corrupt PROP word (0xFFFFFFFF - 1000): no 17 GB Vec, and the sound is unchanged.
        let mut chain = Chain::new(&[effect(DSP_LOWPASS_SIMPLE, &[22000.0])], 48000);
        chain.set(0, 0xFFFF_FFFF - 1000, 500.0, 48000);
        assert_eq!(chain.stages[0].params, vec![22000.0]);
        assert!(response(&mut chain, 4000.0) > 0.99);
        chain.set(0, 1, 500.0, 48000);
        assert_eq!(chain.stages[0].params, vec![22000.0]);
    }

    #[test]
    fn faders_and_the_reverbs_dry_level_are_gains() {
        let mut reverb = vec![0.0; 13];
        (reverb[SFXREVERB_WET], reverb[SFXREVERB_DRY]) = (SILENT_DB, 6.0);
        let mut chain = Chain::new(&[effect(DSP_FADER, &[-6.0]), effect(DSP_SFXREVERB, &reverb)], 48000);
        assert!(chain.stages[1].reverb.is_none());
        assert!((chain.run(1.0) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn the_reverb_rings_after_its_delays_and_dies_in_its_decay_time() {
        // Fiesta_Rally2's `engine_int` reverb: 200 ms, early 10 ms, late 45 ms, wet -13.5 dB, dry 0 dB.
        let p = [200.0, 10.0, 45.0, 5000.0, 100.0, 100.0, 100.0, 84.0, -7.0, 5000.0, 0.0, -13.5, 0.0];
        let mut chain = Chain::new(&[effect(DSP_SFXREVERB, &p)], 48000);
        let out: Vec<f32> = (0..48000).map(|i| chain.run(if i == 0 { 1.0 } else { 0.0 })).collect();
        let energy = |from: usize, to: usize| out[from..to].iter().map(|v| v * v).sum::<f32>();
        assert_eq!(out[0], 1.0);
        assert_eq!(energy(1, 470), 0.0, "nothing before the early delay");
        assert!(energy(470, 4800) > 1e-4);
        assert!(energy(28800, 48000) < 1e-6 * energy(470, 4800), "60 dB down well after 200 ms");
    }

    #[test]
    fn the_compressor_brings_a_loud_tone_down_by_its_ratio() {
        // `bus:/internal`: -10 dB, 2.5:1, 20 ms, 200 ms. A full-scale tone is 10 dB over: 6 dB of reduction.
        let mut chain = Chain::new(&[effect(DSP_COMPRESSOR, &[-10.0, 2.5, 20.0, 200.0, 0.0])], 48000);
        let peak = response(&mut chain, 200.0);
        assert!((0.45..0.6).contains(&peak), "{peak}");
        let mut quiet = Chain::new(&[effect(DSP_COMPRESSOR, &[-10.0, 2.5, 20.0, 200.0, 0.0])], 48000);
        let w = 2.0 * PI * 200.0 / 48000.0;
        let y = (0..48000).map(|i| quiet.run(0.1 * (w * i as f32).sin())).skip(24000).fold(0.0, |m: f32, y| m.max(y));
        assert!((y - 0.1).abs() < 1e-3, "below the threshold it is left alone: {y}");
    }
}

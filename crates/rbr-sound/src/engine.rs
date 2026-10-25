//! The granular engine (`CEngine`, vtable 0x732c64; rbr-recomp `notes/sound.md` section 4): the `EngineNoise` inputs, the rpm
//! slew, the bank choice and crossfade, the grain choice and the renderer, which writes one firing period at a time.
//! Pure: no audio device. The random grain restarts take a seeded RNG of their own, not the CRT `rand()`.

use std::sync::Arc;

use rbr_formats::eng;

/// rad/s to rpm (0x7319a4).
const RPM_PER_RAD_S: f32 = 9.549_296;
/// The rpm the slew target is clamped to (0x731cd4, 0x731cd8); below the low one the engine voice stops. NGP reads
/// the low one from NGP.dll (`rbr_recording::sound::NgpSound::min_rpm`; the exe's is 500).
pub const MAX_RPM: f32 = 9000.0;
/// The most the rpm moves per input (`M740` 0x4bc740).
const SLEW: i64 = 0x1f4;
/// rpm values the slew steps over, 2 at a time (`E26_Tab2cc8`, the 0xffff-terminated words at 0x732cc8).
const SKIPPED_RPM: [u32; 80] = [
    1010, 1024, 1028, 1032, 1061, 1073, 1088, 1109, 1126, 1175, 1216, 1233, 1282, 1287, 1292, 1327, 1331, 1357, 1371,
    1403, 1406, 1409, 1415, 1526, 1542, 1551, 1660, 1694, 1734, 1835, 1882, 1923, 1963, 2023, 2048, 2064, 2077, 2176,
    2285, 2289, 2293, 2313, 2317, 2371, 2441, 2446, 2451, 2455, 2473, 2501, 2559, 2564, 2574, 2579, 2580, 2584, 2662,
    2803, 2827, 2833, 2895, 2908, 3084, 3158, 3159, 3160, 3219, 3341, 3547, 3711, 3919, 3973, 3997, 4096, 4759, 4811,
    4882, 5069, 5128, 5168,
];
/// Firings per second per rpm: two per revolution (0x730898, `0.033333335`).
const FIRINGS_PER_RPM: f32 = 0.033_333_335;
/// The gain of a misfiring chunk (`M7b0`).
const MISFIRE_GAIN: f32 = 0.25;

/// What `CCarSound::M6a0` (0x4b06a0) takes from one `EngineNoise` payload for the engine voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineNoise {
    /// The slew target, clamped to the low rpm (NGP's)..[`MAX_RPM`] and truncated.
    pub target_rpm: u32,
    /// Below the low rpm before the clamp: the engine voice stops.
    pub stopped: bool,
    /// `int(throttle * 100)`, `CEngine +0x1018`.
    pub load: u32,
    /// The cylinder flags `p[8..12]` (`CEngine +0x32cf0`); a cylinder whose flag is 0 misfires.
    pub cylinders: [bool; 4],
}

impl EngineNoise {
    /// From the payload words (`rbr_recording::sound`, the engine state from +0x2c) and the low rpm.
    pub fn from_payload(p: &[u32; 12], min_rpm: f32) -> EngineNoise {
        let f = |i: usize| f32::from_bits(p[i]);
        let rpm = f(0) * RPM_PER_RAD_S;
        let clamped = rpm.clamp(min_rpm, MAX_RPM);
        EngineNoise {
            target_rpm: clamped as u32,
            stopped: rpm < min_rpm,
            load: (f(1) * 100.0).max(0.0) as u32,
            cylinders: std::array::from_fn(|i| f(8 + i) != 0.0),
        }
    }
}

/// `CEngine::M740` (0x4bc740): `rpm` moved toward `target` by at most 500, then stepped 2 up past each value of
/// [`SKIPPED_RPM`] it lands on (the walk restarts after each step).
pub fn slew(rpm: u32, target: u32) -> u32 {
    let cur = i64::from(rpm);
    let mut v = i64::from(target).clamp(cur - SLEW, cur + SLEW);
    let mut idx = 0;
    let mut t = i64::from(SKIPPED_RPM[0]);
    loop {
        idx += 1;
        if t == v {
            v += 2;
            idx = 0;
        }
        if t > v {
            break;
        }
        match SKIPPED_RPM.get(idx) {
            Some(&next) => t = i64::from(next),
            None => break,
        }
    }
    v.max(0) as u32
}

/// `CEngine::M580` and `M5d0` (0x4bc580, 0x4bc5d0, from the decompile): the banks below and above `rpm` in `rpms`
/// (ascending). Below the first bank both are the first, from the last bank up both are the last.
pub fn bank_pair(rpms: &[u32], rpm: u32) -> (usize, usize) {
    let lo = rpms.iter().rposition(|&r| r <= rpm).unwrap_or(0);
    let hi = rpms.iter().position(|&r| r > rpm).unwrap_or(rpms.len().saturating_sub(1));
    (lo, hi)
}

/// `CEngine::M7b0` (0x4bc7b0): the banks a chunk mixes with their gains. The bank above takes
/// `(rpm - lo) / (hi - lo)`, the one below the rest; one bank alone takes 1.
pub fn crossfade(rpms: &[u32], rpm: u32) -> Vec<(usize, f32)> {
    let (lo, hi) = bank_pair(rpms, rpm);
    if lo == hi {
        return vec![(lo, 1.0)];
    }
    let (r_lo, r_hi) = (rpms[lo] as f32, rpms[hi] as f32);
    let up = 1.0 - (r_hi - rpm as f32) / (r_hi - r_lo);
    vec![(hi, up), (lo, 1.0 - up)]
}

/// The output samples of one firing period at `rpm` (`M9c0` 0x4bc9c0: `rate / (rpm * 0.033333335)`, truncated).
pub fn chunk_len(rate: u32, rpm: u32) -> usize {
    (f64::from(rate) / (f64::from(rpm) * f64::from(FIRINGS_PER_RPM))) as usize
}

/// A small seeded generator for the grain restarts (ours: the original uses the CRT `rand()`, which other systems
/// share, so its stream cannot be matched).
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed | 1)
    }

    /// 15 bits, as `rand()` gives.
    pub fn rand(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 >> 17
    }
}

/// `CEngine::VoiceBb0` (0x4bbbb0) on one part's counter: the cycle row the next grain starts at. The counter steps
/// on until `(count - 3) & !3`; then the grain is row 0 and the counter restarts at `(rand % (count - 11) & !3) + 5`.
pub fn next_grain(counter: &mut usize, count: usize, rng: &mut Rng) -> usize {
    let u = *counter;
    if u < (count.saturating_sub(3) & !3) {
        *counter = u + 1;
        u
    } else {
        *counter = (((rng.rand() as usize) % count.saturating_sub(11).max(1)) & !3) + 5;
        0
    }
}

/// The Catmull-Rom step of the cubic resampler (`Resample3e0` 0x4bb3e0) between `s1` and `s2` at `fr`.
fn cubic(s0: f32, s1: f32, s2: f32, s3: f32, fr: f32) -> f32 {
    ((((s2 * 2.0 + s0) - (s1 * 5.0 + s3) * 0.5) + ((s1 - s2) * 3.0 - s0 + s3) * 0.5 * fr) * fr + (s2 - s0) * 0.5) * fr
        + s1
}

/// Adds the grain of `part` at cycle row `row` to `out`, its firing at `out[half]`: the recording about the row's
/// firing at its own pitch (resampled to `rate` only), under a Hann window of `half` samples each side, times `gain`.
/// Ours: 1.02 stretches the grain from one firing to the next to the firing period, which shifts the whole recording
/// with the rpm, its resonances too (up to 1.8 times between the retail banks, half at idle), and butts the grains
/// together. Here only the spacing of the grains follows the rpm (pitch-synchronous overlap-add), and the windows
/// one period apart sum to 1.
fn add_grain(out: &mut [f32], half: usize, part: &eng::Part, row: usize, rate: u32, gain: f32) {
    let Some(&mark) = part.cycles.get(row) else {
        return;
    };
    let step = f64::from(part.rate) / f64::from(rate);
    let x = &part.samples;
    let at = |i: i64| x.get(i.clamp(0, x.len() as i64 - 1) as usize).map_or(0.0, |&s| f32::from(s));
    for (k, sample) in out.iter_mut().enumerate().take(half * 2) {
        let pos = mark - 1.0 + step * (k as f64 - half as f64);
        let p = pos.floor() as i64;
        let fr = (pos - p as f64) as f32;
        let w = (std::f32::consts::FRAC_PI_2 * k as f32 / half as f32).sin().powi(2);
        *sample += cubic(at(p - 1), at(p), at(p + 1), at(p + 2), fr) * w * gain;
    }
}

/// The half window of `part`'s grain at `row` for a firing period of `period` output samples at `rate`: the period,
/// or the recording's own firing interval where that is shorter (a grain wider than it would carry the next firing
/// off its time), and the gain that keeps the level of the gaps that leaves (`sqrt(period / half)`).
fn half_window(part: &eng::Part, row: usize, period: usize, rate: u32) -> (usize, f32) {
    let own = match (part.cycles.get(row), part.cycles.get(row + 1)) {
        (Some(&a), Some(&b)) => ((b - a) * f64::from(rate) / f64::from(part.rate)) as usize,
        _ => period,
    };
    let half = period.min(own).max(1);
    (half, (period as f32 / half as f32).sqrt())
}

/// `E32BufferHandler::MixScale`'s (0x4d99f0) gain at `rpm` and `load`: `256 + trunc(min(rpm, 8000) * 0.000125 * 256 +
/// (load << 7) * 0.01)` / 256, 1 to 2.5.
pub fn mix_gain(rpm: u32, load: u32) -> f32 {
    let speed = (rpm as f32).min(8000.0) * 0.000_125 * 256.0;
    ((speed + (load << 7) as f32 * 0.01) as i32 + 0x100).max(0x100) as f32 / 256.0
}

/// `E32BufferHandler::MixScale` (0x4d99f0), which `E30Engine::Vf08` (0x4cebc0) runs on each block of the engine's
/// render before the stream and the reflections take it: the [`mix_gain`], then above half scale (0x4000) a 4:1 knee.
/// This is the "compressor -6 dB 4:1" that `audio.ini` mentions. The samples are in the 16-bit range; the original's
/// integer steps are taken in floats. Ours: the gain moves from `from` to `to` across the block instead of stepping
/// at its start, and the knee is soft, a quadratic bend over [`KNEE_WIDTH`] about it, instead of a corner.
pub fn mix_scale(samples: &mut [f32], from: f32, to: f32) {
    const KNEE: f32 = 16384.0;
    const KNEE_WIDTH: f32 = 8192.0;
    const SLOPE: f32 = 0.25;
    let bend = |a: f32| {
        let over = a - (KNEE - KNEE_WIDTH / 2.0);
        if over <= 0.0 {
            a
        } else if over < KNEE_WIDTH {
            a + (SLOPE - 1.0) * over * over / (2.0 * KNEE_WIDTH)
        } else {
            (a - KNEE) * SLOPE + KNEE
        }
    };
    let n = samples.len().max(1) as f32;
    for (i, s) in samples.iter_mut().enumerate() {
        let x = *s * (from + (to - from) * (i + 1) as f32 / n);
        *s = bend(x.abs()).copysign(x);
    }
}

/// The renderer's state: the bank, the rpm after the slew, the load and the cylinder flags, each part's grain
/// counter, the cylinder counter, the samples left of the last chunk and the grains' overlap past it.
#[derive(Debug, Clone)]
pub struct Engine {
    bank: Arc<eng::Engine>,
    rpms: Vec<u32>,
    /// Output rate, Hz.
    rate: u32,
    pub rpm: u32,
    pub load: u32,
    pub cylinders: [bool; 4],
    pub running: bool,
    counters: Vec<[usize; 2]>,
    cylinder: usize,
    rng: Rng,
    left: Vec<f32>,
    /// The output from the start of the next chunk on that the grains so far have reached.
    overlap: Vec<f32>,
}

impl Engine {
    /// The renderer for `bank` at `rate` Hz, from 1000 rpm (the ctor 0x4bc180's `+0x101c`).
    pub fn new(bank: Arc<eng::Engine>, rate: u32, seed: u32) -> Engine {
        let rpms = bank.banks.iter().map(eng::Bank::rpm).collect();
        let counters = vec![[0; 2]; bank.banks.len()];
        Engine {
            bank,
            rpms,
            rate,
            rpm: 1000,
            load: 0,
            cylinders: [true; 4],
            running: true,
            counters,
            cylinder: 0,
            rng: Rng::new(seed),
            left: Vec::new(),
            overlap: Vec::new(),
        }
    }

    /// One `EngineNoise` input: the slew toward its rpm, its load and cylinder flags.
    pub fn input(&mut self, noise: &EngineNoise) {
        self.rpm = slew(self.rpm, noise.target_rpm);
        self.load = noise.load;
        self.cylinders = noise.cylinders;
        self.running = !noise.stopped;
    }

    /// Samples in the engine's own scale (the recordings' 16-bit range) into `out`, overwriting it. Each firing
    /// period mixes the crossfaded banks' grains; a misfiring cylinder's chunk takes half the load and a quarter
    /// gain (`M9c0`, `M7b0`). Samples of a chunk that does not fit are kept for the next call.
    pub fn render(&mut self, out: &mut [f32]) {
        let mut done = self.left.len().min(out.len());
        out[..done].copy_from_slice(&self.left[..done]);
        self.left.drain(..done);
        while done < out.len() {
            if !self.running || self.rpm == 0 || self.rpms.is_empty() {
                out[done..].fill(0.0);
                self.overlap.clear();
                return;
            }
            let chunk = self.chunk();
            let n = chunk.len().min(out.len() - done);
            out[done..done + n].copy_from_slice(&chunk[..n]);
            self.left.extend_from_slice(&chunk[n..]);
            done += n;
        }
    }

    /// One firing period: the overlap of the last grains, and the start of this period's grains, which fire at its
    /// end.
    fn chunk(&mut self) -> Vec<f32> {
        let period = chunk_len(self.rate, self.rpm).max(1);
        let mut out = std::mem::take(&mut self.overlap);
        out.resize(out.len().max(period * 2), 0.0);
        let (mut load, mut gain) = (self.load, 1.0);
        if !self.cylinders[self.cylinder] {
            load >>= 1;
            gain = MISFIRE_GAIN;
        }
        self.cylinder = (self.cylinder + 1) % 4;
        let mix = (load.min(100) as f32 * 0.01 * 256.0).trunc() / 256.0;
        for (b, weight) in crossfade(&self.rpms, self.rpm) {
            let bank = &self.bank.banks[b];
            let (Some(empty), Some(full)) = (bank.parts.first(), bank.parts.get(1).or(bank.parts.first())) else {
                continue;
            };
            for (p, part, part_gain) in [(0, empty, 1.0 - mix), (1, full, mix)] {
                let row = next_grain(&mut self.counters[b][p], part.cycles.len(), &mut self.rng);
                let (half, level) = half_window(part, row, period, self.rate);
                let gain = weight * gain * part_gain * level;
                add_grain(&mut out[period - half..], half, part, row, self.rate, gain);
            }
        }
        self.overlap = out.split_off(period);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_RPM: f32 = 500.0;

    #[test]
    fn slew_moves_at_most_500_and_steps_over_the_table() {
        assert_eq!(slew(1000, 3000), 1500);
        assert_eq!(slew(3000, 1000), 2500);
        assert_eq!(slew(1000, 1200), 1200);
        // 1024 is skipped to 1026; 1028 to 1030, 1032 to 1034.
        assert_eq!(slew(1000, 1024), 1026);
        assert_eq!(slew(1000, 1028), 1030);
        // 3158, 3159 and 3160 step on to 3162 (3158 + 2 = 3160 is in the table again).
        assert_eq!(slew(3000, 3158), 3162);
        assert_eq!(slew(5000, 5168), 5170);
    }

    #[test]
    fn noise_clamps_and_truncates() {
        let mut p = [0u32; 12];
        p[0] = 200f32.to_bits();
        p[1] = 0.555f32.to_bits();
        p[8] = 1f32.to_bits();
        let noise = EngineNoise::from_payload(&p, MIN_RPM);
        assert_eq!(noise.target_rpm, 1909);
        assert_eq!(noise.load, 55);
        assert_eq!(noise.cylinders, [true, false, false, false]);
        assert!(!noise.stopped);
        p[0] = 10f32.to_bits();
        let idle = EngineNoise::from_payload(&p, MIN_RPM);
        assert_eq!((idle.target_rpm, idle.stopped), (500, true));
        p[0] = 1000f32.to_bits();
        assert_eq!(EngineNoise::from_payload(&p, MIN_RPM).target_rpm, 9000);
    }

    #[test]
    fn banks_are_chosen_and_crossfaded() {
        let rpms = [1700, 3000, 4000, 5000, 6000, 7000];
        assert_eq!(bank_pair(&rpms, 1000), (0, 0));
        assert_eq!(bank_pair(&rpms, 1700), (0, 1));
        assert_eq!(bank_pair(&rpms, 3500), (1, 2));
        assert_eq!(bank_pair(&rpms, 7000), (5, 5));
        assert_eq!(bank_pair(&rpms, 8000), (5, 5));
        assert_eq!(crossfade(&rpms, 1000), [(0, 1.0)]);
        assert_eq!(crossfade(&rpms, 3250), [(2, 0.25), (1, 0.75)]);
        assert_eq!(crossfade(&rpms, 4000), [(3, 0.0), (2, 1.0)]);
    }

    #[test]
    fn a_chunk_is_one_firing_period() {
        // Truncated after the float constant: 44100 / (3000 * 0.033333335) is 440.99998.
        assert_eq!(chunk_len(44100, 3000), 440);
        assert_eq!(chunk_len(44100, 6000), 220);
        assert_eq!(chunk_len(48000, 1000), 1439);
    }

    #[test]
    fn grains_step_on_then_restart_at_random() {
        let mut rng = Rng::new(7);
        let mut counter = 0;
        let rows: Vec<usize> = (0..14).map(|_| next_grain(&mut counter, 32, &mut rng)).collect();
        assert_eq!(rows[..13], (0..13).collect::<Vec<_>>()[..]);
        // (32 - 3) & !3 = 28: row 28 is the last one stepped to, then row 0 and a restart.
        let mut counter = 28;
        assert_eq!(next_grain(&mut counter, 32, &mut rng), 0);
        assert!((5..=25).contains(&counter) && (counter - 5) % 4 == 0, "{counter}");
    }

    /// A bank of one rpm whose grains are a ramp: each chunk is the ramp stretched to the firing period.
    fn ramp_bank() -> Arc<eng::Engine> {
        let samples: Vec<i16> = (0..4000).map(|i| (i % 100) as i16 * 100).collect();
        let cycles = (0..40).map(|k| 1.0 + 100.0 * k as f64).collect();
        let part = eng::Part { rpm: 3000, load: 0, rate: 10000, samples, cycles };
        let full = eng::Part { load: 100, ..part.clone() };
        Arc::new(eng::Engine {
            version: 0x101,
            banks: vec![eng::Bank { parts: vec![part, full] }],
            table_a: vec![],
            table_b: vec![],
        })
    }

    #[test]
    fn the_grains_keep_their_pitch_one_period_apart() {
        let mut engine = Engine::new(ramp_bank(), 44100, 1);
        engine.rpm = 3000;
        let n = chunk_len(44100, 3000);
        let mut out = vec![0.0; n * 4 + 10];
        engine.render(&mut out);
        // A grain fires at the end of each chunk: the 100-sample ramp at 10 kHz is 441 samples at 44.1 kHz, so a
        // period of 440 later the next grain's ramp starts again. Halfway between two firings the windows meet at
        // half each, the end of one ramp and the start of the next.
        for k in 1..4 {
            assert!(out[k * n].abs() < 60.0, "{}", out[k * n]);
            assert!((out[k * n + n / 2] - 5000.0).abs() < 60.0, "{}", out[k * n + n / 2]);
            assert!((out[k * n + 100] - 100.0 / 4.41 * 100.0).abs() < 60.0, "{}", out[k * n + 100]);
        }
        assert_eq!(engine.left.len(), n - 10);
        assert_eq!(engine.overlap.len(), n);
    }

    #[test]
    fn the_mix_scale_lifts_with_rpm_and_load_and_bends_above_half_scale() {
        // 256 + 32 = 288: x1.125.
        let gain = mix_gain(1000, 0);
        assert_eq!(gain, 1.125);
        let mut s = [1000.0, 20000.0, -20000.0];
        mix_scale(&mut s, gain, gain);
        assert_eq!(s[0], 1125.0);
        // Past the soft knee the 4:1 line.
        assert_eq!(s[1], (22500.0 - 16384.0) * 0.25 + 16384.0);
        assert_eq!(s[2], -s[1]);
        // Within the knee it bends smoothly between the two lines.
        let mut s = [16384.0];
        mix_scale(&mut s, 1.0, 1.0);
        assert!(s[0] < 16384.0 && s[0] > 16384.0 - 1024.0, "{}", s[0]);
        // 256 + 256 + 128 = 640.
        assert_eq!(mix_gain(9000, 100), 2.5);
        // The gain moves across the block.
        let mut s = [1000.0; 4];
        mix_scale(&mut s, 1.0, 2.0);
        assert_eq!(s, [1250.0, 1500.0, 1750.0, 2000.0]);
    }

    #[test]
    fn a_misfire_quarters_the_chunk() {
        let mut engine = Engine::new(ramp_bank(), 44100, 1);
        engine.rpm = 3000;
        engine.cylinders = [true, false, true, true];
        let n = chunk_len(44100, 3000);
        let mut out = vec![0.0; n * 3];
        engine.render(&mut out);
        // The first chunk's grain fires at n, the second's (the misfire) at 2n; just after a firing the next grain's
        // window has barely opened.
        assert!((out[2 * n + 10] - out[n + 10] * 0.25).abs() < 15.0, "{} {}", out[2 * n + 10], out[n + 10]);
    }
}

//! The sound outside a stage: the service park's ambience (`CServiceParkSnd`, 0x4b98c0), for the launcher's pages
//! (rbr-recomp `notes/sound.md` section 13). The game's music (`CMusic`) is not played (user decision).
//!
//! - **Service park** (`CSigServicePark` 1 starts, 2 stops): a looping `SFX::BACKGROUND` sample (gain
//!   `Servicepark_BG` amp times the effects volume) and work hits (`Servicepark_FX` amp, the pneumatic wrench 0.8 of
//!   it). Every frame whose timer is out, the hit counter drops by one; at 0 a new hit kind is drawn,
//!   `rand / 65536 * 5`, and `M96b0` draws a place `(rand / 65536 + 0.5) * 32 - 24` in x and y, 1 m behind, a
//!   count `rand / 65536 * a + 1` and a 0.7 s timer, playing nothing that frame. While the counter is above 0 each
//!   frame with the timer out plays the kind again (so a count of 2 plays once, the frame after the 0.7 s). With
//!   the CRT's 15-bit `rand()` the fractions stay below 0.5: only the wrench, the hard and the medium hit play, the
//!   counts are 1 or 2 and the places lie within 8 m. All of that is faithful (read in the reconstruction,
//!   `RBR_E26_Park.cpp`, and the decompile); ours: the random numbers, a 60 Hz frame, the place as the listener's
//!   x (right) and y, and the background at the listener.

use std::sync::{Arc, Mutex};

use rbr_formats::audio::{self, Amps, AudioDat};
use rbr_formats::data::Data;

use crate::data::Wave;
use crate::device::{Output, Render};
use crate::engine::Rng;
use crate::mixer::voices::{Gain, LoopVoice, Playing};
use crate::mixer::{soft_clip, FULL_SCALE, GAIN_TIME};
use crate::world::{distance_gain, DEFAULT_MAX_DISTANCE};

/// The menus' headroom (ours): the drive's until Sound-5 lowered that one.
const HEADROOM: f32 = 0.5;

/// The service park's hit kinds, in `CServiceParkSnd`'s state order (`Vf4c` to `Vf5c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hit {
    Pneumatic,
    Hard,
    Medium,
    Soft,
    Tools,
}

const HITS: [Hit; 5] = [Hit::Pneumatic, Hit::Hard, Hit::Medium, Hit::Soft, Hit::Tools];

impl Hit {
    /// `M96b0`'s count range `a` for the kind.
    fn count_range(self) -> f32 {
        if self == Hit::Tools {
            1.0
        } else {
            4.0
        }
    }

    /// The voice's min distance (`+0x78`, after `+0x74` 17 m: 0x4b9d0e-0x4b9f95). 1.02 keeps it as the min with
    /// DirectSound's default max; NGP's max of 17 m would not change the gain, the hits being within 12 m.
    fn min_distance(self) -> f32 {
        match self {
            Hit::Pneumatic => 1.0,
            Hit::Hard | Hit::Tools => 5.0,
            Hit::Medium | Hit::Soft => 3.0,
        }
    }

    /// The gain factor on the `Servicepark_FX` amp (`Vf4c`'s 0x72f610 for the wrench).
    fn factor(self) -> f32 {
        if self == Hit::Pneumatic {
            0.8
        } else {
            1.0
        }
    }
}

/// The hit timer after a new draw (0x3f333333), seconds.
const HIT_TIMER: f32 = 0.7;
/// The menus' frame (ours: the original's is the render frame).
const FRAME: f32 = 1.0 / 60.0;

/// `rand() & 0xffff` as a fraction (`0x7319f4` = 1/65536).
fn fraction(rand: u32) -> f32 {
    (rand & 0xffff) as f32 / 65536.0
}

/// A sound the service park starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParkSound {
    /// The `SFX::BACKGROUND` sample, by index, looping.
    Background(usize),
    /// A hit kind's sample, by index, at a place relative to the listener (x right, y, z).
    Hit { hit: Hit, index: usize, at: [f32; 3] },
}

/// `CServiceParkSnd`'s state: pure, a frame at a time.
#[derive(Debug, Clone, Default)]
pub struct Park {
    /// The samples in each bank: background, then the hits in [`HITS`] order.
    pub counts: [usize; 6],
    running: bool,
    timer: f32,
    count: i32,
    state: usize,
    at: [f32; 3],
}

impl Park {
    pub fn new(counts: [usize; 6]) -> Park {
        Park { counts, ..Park::default() }
    }

    /// `M9570`: a sample index of a bank of `n`.
    fn pick(n: usize, rand: &mut impl FnMut() -> u32) -> usize {
        (fraction(rand()) * n as f32) as usize
    }

    /// `CSigServicePark` 1: the ambience starts.
    pub fn start(&mut self, mut rand: impl FnMut() -> u32) -> Vec<ParkSound> {
        self.running = true;
        self.timer = 0.0;
        self.count = 1;
        self.state = 0;
        if self.counts[0] == 0 {
            return Vec::new();
        }
        vec![ParkSound::Background(Park::pick(self.counts[0], &mut rand))]
    }

    /// `CSigServicePark` 2: it stops (the caller stops the background).
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// `M96b0`: true while the counter runs; else a new place, count and timer, and false.
    fn burst(&mut self, hit: Hit, rand: &mut impl FnMut() -> u32) -> bool {
        if self.count != 0 {
            return true;
        }
        let y = (fraction(rand()) + 0.5) * 32.0;
        let x = (fraction(rand()) + 0.5) * 32.0;
        self.at = [(x as i32 - 24) as f32, (y as i32 - 24) as f32, -1.0];
        self.count = (fraction(rand()) * hit.count_range()) as i32 + 1;
        self.timer = HIT_TIMER;
        false
    }

    /// `Vf08`: one frame of `dt` seconds.
    pub fn frame(&mut self, dt: f32, mut rand: impl FnMut() -> u32) -> Option<ParkSound> {
        if !self.running {
            return None;
        }
        if self.timer <= 0.0 {
            self.count -= 1;
            if self.count == 0 {
                self.state = (fraction(rand()) * 5.0) as usize;
            }
            if let Some(&hit) = HITS.get(self.state) {
                if self.burst(hit, &mut rand) {
                    let index = Park::pick(self.counts[1 + self.state], &mut rand);
                    self.timer -= dt;
                    return Some(ParkSound::Hit { hit, index, at: self.at });
                }
            }
        }
        self.timer -= dt;
        None
    }
}

/// The service park's samples.
#[derive(Debug, Clone)]
pub struct MenuBank {
    /// `Service.ini`'s banks: background, then the hits in [`HITS`] order.
    pub park: [Vec<Arc<Wave>>; 6],
    /// The `Servicepark_BG` and `Servicepark_FX` amps.
    pub background_gain: f32,
    pub hit_gain: f32,
}

impl MenuBank {
    /// From `Root/audio.dat` and the 1.02 `audio.ini` (the retail amps without it).
    pub fn load(data: &Data) -> Result<MenuBank, String> {
        let path = data.audio_dat().ok_or("no Root/audio.dat")?;
        let dat = AudioDat::open(&path)?;
        let amps = match data.resolve("audio.ini") {
            Some(ini) => {
                let bytes = std::fs::read(&ini).map_err(|e| format!("{}: {e}", ini.display()))?;
                Some(Amps::read(&audio::parse_ini(&bytes).map_err(|e| format!("audio.ini: {e}"))?))
            }
            None => None,
        };
        let service = dat.ini("Audio\\Service.ini")?;
        let bank = |section: &str| -> Result<Vec<Arc<Wave>>, String> {
            let count = audio::float(&service, section, "Sounds").unwrap_or(0.0) as usize;
            (0..count)
                .map(|k| {
                    let name = audio::text(&service, section, &format!("snd{k}"))
                        .ok_or_else(|| format!("Service.ini has no {section} snd{k}"))?;
                    let file = format!("{name}.wav");
                    let bytes = dat.file(&file).ok_or_else(|| format!("audio.dat has no {file}"))?;
                    Ok(Arc::new(Wave::decode(bytes).map_err(|e| format!("{file}: {e}"))?))
                })
                .collect()
        };
        Ok(MenuBank {
            park: [
                bank("SFX::BACKGROUND")?,
                bank("SFX::PNEUMATIC")?,
                bank("SFX::HARDHIT")?,
                bank("SFX::MEDIUMHIT")?,
                bank("SFX::SOFTHIT")?,
                bank("SFX::TOOLS")?,
            ],
            background_gain: amps.map_or(1.0, |a| a.servicepark_bg),
            hit_gain: amps.map_or(1.0, |a| a.servicepark_fx),
        })
    }
}

/// Plays the menus' sounds into stereo frames.
pub struct MenuMixer {
    rate: u32,
    bank: MenuBank,
    /// The effects volume (linear).
    pub volume: f32,
    park: Park,
    background: LoopVoice,
    hits: Vec<Playing>,
    /// Seconds to the next park frame.
    clock: f32,
    rng: Rng,
    k: f32,
}

impl MenuMixer {
    pub fn new(bank: MenuBank, rate: u32, volume: f32) -> MenuMixer {
        let counts = std::array::from_fn(|i| bank.park[i].len());
        MenuMixer {
            rate,
            bank,
            volume,
            park: Park::new(counts),
            background: LoopVoice::default(),
            hits: Vec::new(),
            clock: 0.0,
            rng: Rng::new(0x5e7c),
            k: 1.0 - (-1.0 / (GAIN_TIME * rate as f32)).exp(),
        }
    }

    /// Starts (`CSigServicePark` 1) or stops (2) the service park's ambience.
    pub fn set_park(&mut self, on: bool) {
        if on == self.park.running {
            return;
        }
        if on {
            let rng = &mut self.rng;
            for sound in self.park.start(|| rng.rand()) {
                if let ParkSound::Background(i) = sound {
                    self.background.wave = self.bank.park[0][i].clone();
                    self.background.pitch = 1.0;
                    self.background.gain = Gain::default();
                }
            }
        } else {
            self.park.stop();
        }
    }

    fn hit(&mut self, hit: Hit, index: usize, [x, y, z]: [f32; 3]) {
        let Some(wave) = self.bank.park[1 + hit as usize].get(index) else { return };
        let d = (x * x + y * y + z * z).sqrt();
        let gain = self.bank.hit_gain
            * self.volume
            * hit.factor()
            * distance_gain(d, (hit.min_distance(), DEFAULT_MAX_DISTANCE));
        let mut playing = Playing::new(wave.clone(), gain);
        playing.pan = (x / d.max(2.0)).clamp(-1.0, 1.0);
        self.hits.push(playing);
    }

    /// Stereo frames, -1 to 1, into `out` (overwritten).
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        out.fill([0.0; 2]);
        self.background.gain.target = if self.park.running { self.bank.background_gain * self.volume } else { 0.0 };
        let mut done = 0;
        while done < out.len() {
            // The park's frames fall between the blocks.
            let n = ((self.clock * self.rate as f32).ceil() as usize).clamp(1, out.len() - done);
            let block = &mut out[done..done + n];
            self.background.add(block, self.rate, self.k);
            let rate = self.rate;
            self.hits.retain_mut(|p| p.add(block, rate));
            done += n;
            self.clock -= n as f32 / self.rate as f32;
            if self.clock <= 0.0 {
                self.clock += FRAME;
                let rng = &mut self.rng;
                if let Some(ParkSound::Hit { hit, index, at }) = self.park.frame(FRAME, || rng.rand()) {
                    self.hit(hit, index, at);
                }
            }
        }
        let scale = HEADROOM / FULL_SCALE;
        for frame in out.iter_mut() {
            *frame = frame.map(|s| soft_clip(s * scale));
        }
    }
}

impl Render for MenuMixer {
    fn render(&mut self, out: &mut [[f32; 2]]) {
        MenuMixer::render(self, out);
    }
}

/// The menus' sound on the audio device.
pub struct MenuPlayer {
    mixer: Arc<Mutex<MenuMixer>>,
    _output: Output,
}

impl MenuPlayer {
    pub fn start(data: &Data, volume: f32) -> Result<MenuPlayer, String> {
        let bank = MenuBank::load(data)?;
        let (output, mixer) = Output::start(|rate| MenuMixer::new(bank, rate, volume))?;
        Ok(MenuPlayer { mixer, _output: output })
    }

    pub fn set_park(&self, on: bool) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.set_park(on);
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if let Ok(mut mixer) = self.mixer.lock() {
            mixer.volume = volume;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `rand` that returns `values` in turn.
    fn seq(values: &[u32]) -> impl FnMut() -> u32 + '_ {
        let mut i = 0;
        move || {
            i += 1;
            values[(i - 1) % values.len()]
        }
    }

    #[test]
    fn the_park_draws_a_burst_then_plays_it_after_the_timer() {
        let mut park = Park::new([1; 6]);
        assert_eq!(park.start(seq(&[0])), [ParkSound::Background(0)]);
        // The first frame draws: kind 16384 / 65536 * 5 = 1 (hard), place (0.25 + 0.5) * 32 - 24 = 0, count 2.
        assert_eq!(park.frame(FRAME, seq(&[16384])), None);
        // 0.7 s on: the count drops to 1 and the hit plays; the next frame draws again.
        let frames = (1..60).find(|_| park.frame(FRAME, seq(&[0])).is_some()).unwrap();
        assert!((42..=43).contains(&frames), "{frames}");
        assert_eq!(park.at, [0.0, 0.0, -1.0]);
        assert_eq!((park.state, park.count), (1, 1));
        assert_eq!(park.frame(FRAME, seq(&[0])), None);
        assert_eq!((park.state, park.count), (0, 1));
        park.stop();
        assert_eq!(park.frame(1.0, seq(&[0])), None);
    }

    #[test]
    fn fifteen_bit_rand_never_draws_the_soft_hit_or_the_tools() {
        let mut park = Park::new([1; 6]);
        park.start(seq(&[0]));
        let mut rng = Rng::new(7);
        let mut kinds = std::collections::HashSet::new();
        for _ in 0..100_000 {
            if let Some(ParkSound::Hit { hit, at, .. }) = park.frame(FRAME, || rng.rand()) {
                kinds.insert(hit);
                assert!(at[0] >= -8.0 && at[0] < 8.0 && at[1] >= -8.0 && at[1] < 8.0);
            }
        }
        assert_eq!(kinds, [Hit::Pneumatic, Hit::Hard, Hit::Medium].into_iter().collect());
    }
}

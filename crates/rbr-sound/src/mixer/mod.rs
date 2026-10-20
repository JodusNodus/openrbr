//! The backend's mixer: plays the voice commands into stereo frames at the output rate, without a device (the
//! device output is `crate::device`, an offline render calls [`Mixer::render`] itself). It streams the granular
//! engine through `MixScale` ([`crate::engine::mix_scale`]) and `E30Engine::Vf08`'s filter, plays its
//! two reflections ([`crate::reflections`]), loops the whine, turbo, tyre, wind and siren samples at
//! their pitch, synthesises the rain, plays the one-shots, and plays the speech voice's queue.
//!
//! The car's voices take the distance gain of the distances the original sets on each (DirectSound's default rolloff,
//! [`distance_gain`]; the tyre loops the software 3D path's [`software_gain`]; the 2D whine and wetness none) and
//! their side from where the car is from the camera ([`Command::Listener`]). The engine takes `E30Engine::Vf08`'s
//! one-pole filter ([`engine_filter`]). The speech queue is `SOUND::CSpeech`'s: 10 entries, placed by priority
//! as `SpInsert` does ([`speech_slot`]), with the pause [`Command::SpeechGap`] gives after each word.
//!
//! Ours, not traced (rbr-recomp `notes/sound.md` 4.3): the sample voices' band-limited interpolation ([`voices::interpolate`],
//! standing for DirectSound's resampler), gains move to a new value over about 10 ms instead of at once, the sides
//! are a plain balance (not DirectSound's 3D panning), and at most [`MAX_RANGED`] ranged impacts play at once,
//! [`RANGED_SPACING`] apart. Speech plays at gain 1. The mix takes a fixed master gain ([`HEADROOM`]) and goes
//! through a look-ahead limiter ([`limiter`]) instead of a hard clamp. The one-shots ask for a place under the
//! original's limit of 16 sounds ([`SOUND_LIMIT`]), as on current systems. For a cleaner sound (ours, not the
//! original's): loops cross their seam with a fade ([`voices::seamless`]), one-shots fade in and out at their edges
//! and when stopped, the gravel hits, squeaks, rattles and impacts vary a little in pitch and gain
//! ([`VARIATION`]), and in the car the voices 1.02 means to low-pass take its filter ([`CABIN_CUTOFF`]).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use rbr_formats::fmod::Bus;

use crate::data::{CarBank, Distances, Wave};
use crate::engine::{mix_gain, mix_scale, Engine, Rng};
use crate::reflections::Reflections;
use crate::voice::{Command, Loop, Material, Repeat, Shot, Speech, WheelLoop};
use crate::world::{distance_gain, DEFAULT_MAX_DISTANCE};

mod dsp;
mod fmod_mix;
mod limiter;
mod rain;
pub(crate) mod voices;

pub(crate) use fmod_mix::fmod_loudness;
use fmod_mix::Tree;
use limiter::Limiter;
use rain::Rain;
use voices::{balance, seamless, Family, Gain, LoopVoice, OnePole, Playing, TyreVoice};

/// The 16-bit range the samples are in.
pub(crate) const FULL_SCALE: f32 = 32768.0;
/// The engine's filter in the car (`E30Engine::Vf08` 0x4ced95: `E32Flt::SetCoef(400, 44100)`, then `Run` at 2).
const INSIDE_CUTOFF: f32 = 400.0;
const INSIDE_GAIN: f32 = 2.0;
/// Outside, above this rpm the engine gets its low-passed self (at `Car%d_LowShelfFrequency`) mixed in at
/// `(rpm - 2500) * 0.0004 * 2` (0x4cee35-0x4cee61: 0x734d7c, 0x731d08, 0x72ad94).
const OUTSIDE_LOW_RPM: f32 = 2500.0;
const OUTSIDE_LOW_PER_RPM: f32 = 0.0004 * 2.0;
/// The in-car low-pass of the gravel hits, squeaks, bottoming, driveshaft clunks and the tyres' rolling loops
/// (`E31Mgr::M4870` 0x4d4870: `SetRate(1000)` with the in-car flag set and the skid flag clear). 1.02 sets it but runs
/// it only on software-mixed voices, so on DirectSound nothing is filtered (`notes/sound.md`); ours runs it, so in
/// the car the road comes through the floor instead of as bright as outside.
const CABIN_CUTOFF: f32 = 1000.0;
/// Time constant of the gain moves, seconds (ours).
pub(crate) const GAIN_TIME: f32 = 0.01;
/// The sounds the original's mixer plays at once (`E30DXMixer +0x440`: `Vf04` 0x4cfdca sets 16 when the option
/// `Set440` 0x4cf360 gives is 0). `Vf48` (0x4cf940) and `Vf4c` (0x4cf840) hand a new sound the silent stand-in
/// (`+0x40`, which `E32Factory::Make3` 0x4db570 deletes) once `[+0x280]+0x106f8` reaches it; `E31Master::Vf08`
/// (0x4d4980) recounts the sounds playing once a frame, so every request in a frame sees the same count. On XP with
/// hardware voices every sound played; on current systems the original refuses most one-shots (`notes/sound.md`).
pub const SOUND_LIMIT: usize = 16;
/// The race's sounds that always count. The harness's live list (`notes/verification.md` "Plugin", `RBR_POPS`) has 11 (the base
/// object, the mono and stereo Ogg streams, the turbo, the four tyres, the transmission, the two reflections and the
/// exhaust, less the one the count leaves out), but 9 is what gives each caller the original's made and refused
/// counts at 60 frames a second (rbr-recomp `notes/sound.md` 2.5); the two are open.
const RACE_SOUNDS: usize = 9;
/// The speech queue's entries (`SOUND::CSpeech::SpInsert` 0x4dd4d0).
const SPEECH_QUEUE: usize = 10;

/// The mix's master gain (ours): every retail sample is normalised to full scale and the original sums its voices
/// past it. 0.8 matched the original's level on the tanner drive (rbr-recomp `notes/sound.md` 2.6), clipping 2-3% of
/// its samples; at 0.5 (about 4 dB down) the [`limiter`] leaves about four in five samples untouched there and takes
/// more than 3 dB off under 3% of them, where at 0.8 it worked nearly all the time and pumped.
pub(crate) const HEADROOM: f32 = 0.5;
/// Above this the output bends smoothly toward 1 instead of clipping (ours).
const KNEE: f32 = 0.8;
/// The most a repeated hit's pitch and gain stray from what the model asks, as a share (ours: the same sample at the
/// same pitch many times a second sounds mechanical).
const VARIATION: [f32; 2] = [0.04, 0.15];

/// The last stage after the [`limiter`] (ours): linear up to [`KNEE`], then a tanh curve that never passes 1.
pub(crate) fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * ((a - KNEE) / (1.0 - KNEE)).tanh())
    }
}

/// `E30Engine::Vf08`'s (0x4cebc0) filter on one engine sample after `MixScale`, the filter tuned as the camera
/// asks: in the car the low-passed sample times [`INSIDE_GAIN`]; outside the sample plus its low-passed self at
/// `low` ([`OUTSIDE_LOW_PER_RPM`] above [`OUTSIDE_LOW_RPM`]), untouched below (`E32Flt::Mix` is not called, so the
/// filter's state stands still). The replay's and the TV cameras' branch (a cutoff of `22050 - d * 21500` from a
/// direction product `d`, else 450 Hz mixed in at 2) is not modelled: those play as outside.
fn engine_filter(filter: &mut OnePole, s: f32, inside: bool, low: f32) -> f32 {
    if inside {
        filter.run(s) * INSIDE_GAIN
    } else if low > 0.0 {
        s + filter.run(s) * low
    } else {
        s
    }
}

/// The sample banks the one-shots and the speech pick from.
#[derive(Debug, Clone, Default)]
struct Banks {
    blow_off: Vec<Arc<Wave>>,
    gear_changes: Vec<Arc<Wave>>,
    offroad: Vec<Arc<Wave>>,
    ranged: Vec<Arc<Wave>>,
    screams: Vec<Arc<Wave>>,
    go: Vec<Arc<Wave>>,
    birds: Vec<Arc<Wave>>,
    bird_hit: Vec<Arc<Wave>>,
    grus: Vec<Arc<Wave>>,
    squeaks: Vec<Arc<Wave>>,
    bottomings: Vec<Arc<Wave>>,
    drive_shafts: Vec<Arc<Wave>>,
    exhaust_pops: Vec<Arc<Wave>>,
    pops: Vec<Arc<Wave>>,
    bangs: Vec<Arc<Wave>>,
    fire: Vec<Arc<Wave>>,
    ignition: Vec<Arc<Wave>>,
    dog_rings: Vec<Arc<Wave>>,
    wiper: Vec<Arc<Wave>>,
    surfaces: HashMap<(Material, bool), Arc<Wave>>,
    words: HashMap<u32, Vec<Arc<Wave>>>,
    numbers: Vec<Arc<Wave>>,
    damage: Vec<Arc<Wave>>,
}

fn shared(waves: &[Wave]) -> Vec<Arc<Wave>> {
    waves.iter().cloned().map(Arc::new).collect()
}

/// Which distances a car voice takes from the camera. Each is what the original sets on the voice after creating it
/// (`E30DXSound::Vf48` 0x4d21e0 creates the DirectSound buffer after the voice reset 0x4dcbf0, whose `+0x74` 10 and
/// `+0x78` 1 reach no buffer, so a voice nobody sets keeps DirectSound's defaults: [`DEFAULT_DISTANCES`]).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Place {
    /// The engine stream (`E30Engine::Vf04` 0x4cf220: `+0x78` `SOUNDDEF::EXHAUST` min, then `+0x74` its max).
    Exhaust,
    /// The turbo stream (`CDXTurbo` 0x4d3bf4: flags 0x98, 3D, no distance call): DirectSound's defaults.
    Turbo,
    /// The blow-off valve (`E31Turbo::M3cb0` 0x4d3d2c: `+0x78` 2, then `+0x74` 40).
    BlowOff,
    /// A wheel's surface loops on the software path ([`software_gain`]).
    Tire,
    Ranged,
    /// The off-road rattles' own distances (`Mce10`: 10 to 20 m as called).
    Offroad,
    ExhaustPop,
    /// The gravel hits' (`M7330`: 1 to 10 m as called), the fire's (`Md1c0`: min 20 m) and DirectSound's defaults (1 m
    /// and 1e9 m: the squeaks, the bottoming and the driveshaft clunks).
    Grus,
    Fire,
    /// The ignition's (`Vf54`: `+0x78` 1, then `+0x78` 10) and the broken cooler's (`Md2b0`: `+0x78` 1, then 5): the
    /// min both, under NGP too.
    Ignition,
    Cooler,
    Default,
    /// A 2D voice, no distance gain: the gear and diff whine (`CDXTransmission` 0x4d2fe0, 0x4d3260: flags 0x200 and
    /// 0x90, no `CTRL3D`; they play in the car only) and the wetness on the software path.
    Flat,
}

/// The blow-off valve's distances: 1.02 keeps the last call's 40 m as the min, NGP 2 to 40 m.
const BLOW_OFF_DISTANCES: (f32, f32) = (40.0, DEFAULT_MAX_DISTANCE);
const NGP_BLOW_OFF_DISTANCES: (f32, f32) = (2.0, 40.0);
/// The tyre loops' distances on the software path (`CTireSnd::M6cb0` 0x4d6cb0: `+0x78` 10, then `+0x74` 40 on each
/// wheel's two surface voices), which `E30MixerBase::Mce250` reads from the voice's own fields (`+0x80` min, `+0x7c`
/// max), so 1.02 and NGP play them alike. The hardware path (`M69a0` 0x4d69a0, 0x21 or more free hardware channels)
/// sets 2 and 10 m instead and plays them through DirectSound 3D: not modelled.
const TIRE_DISTANCES: (f32, f32) = (10.0, 40.0);
/// `M7ea0` (0x4d7f28) renders a wheel's loops at `Mce250`'s gain times 0.7 (0x72f674).
const SOFTWARE_TIRE_SCALE: f32 = 0.7;

/// `E30MixerBase::Mce250`'s (0x4ce250) gain of a voice at `distance`: its own volume up to `min`, falling linearly to
/// its outside volume (0 on every retail voice, `SOUNDDEF OutsideVolume`) at `max`, the outside volume past it.
pub fn software_gain(distance: f32, (min, max): (f32, f32)) -> f32 {
    if distance > max {
        0.0
    } else if min > distance {
        1.0
    } else {
        1.0 - (distance - min) / (max - min)
    }
}

/// The off-road rattles' distances under NGP (`CCarSound::Mce10` 0x4ace10: `+0x78` 10, then `+0x74` 20 outside the
/// car, 11 in it, at 0x4acf08-0x4acf1e). 1.02 keeps the last as the min: 20 m (in the car 11 m, the same gain there,
/// where the camera is within 10 m of the car).
const NGP_OFFROAD_DISTANCES: (f32, f32) = (10.0, 20.0);
const OFFROAD_DISTANCES: (f32, f32) = (20.0, DEFAULT_MAX_DISTANCE);
/// The gravel hits' (`CTireSnd::M7330` 0x4d7330: `+0x74` 10 at 0x4d7530, then `+0x78` 1 at 0x4d753c): 1.02 keeps the
/// min of 1 m and the default max, NGP 1 to 10 m.
const GRUS_DISTANCES: (f32, f32) = (1.0, DEFAULT_MAX_DISTANCE);
const NGP_GRUS_DISTANCES: (f32, f32) = (1.0, 10.0);
const FIRE_DISTANCES: (f32, f32) = (20.0, DEFAULT_MAX_DISTANCE);
/// Under NGP the fire's (`Md1c0`'s second `+0x78` call patched to `+0x74`, which NGP's patch of `E30DXSound` 0x4d295f
/// makes `SetMaxDistance`; 1.02's `+0x74` and `+0x78` both set the min distance): 1 to 20 m.
const NGP_FIRE_DISTANCES: (f32, f32) = (1.0, 20.0);
const DEFAULT_DISTANCES: (f32, f32) = (1.0, DEFAULT_MAX_DISTANCE);
const IGNITION_MIN: f32 = 10.0;
const COOLER_MIN: f32 = 5.0;
/// At most this many ranged impacts play at once, and a new one needs this many seconds since the last (ours:
/// scraping and a crash change the damage payload nearly every step).
pub const MAX_RANGED: usize = 3;
pub const RANGED_SPACING: f32 = 0.08;

/// Whether a new ranged impact plays, `since` seconds after the last with `playing` still playing.
pub fn ranged_plays(since: f32, playing: usize) -> bool {
    since >= RANGED_SPACING && playing < MAX_RANGED
}

/// `SpInsert`'s (0x4dd4d0) place for an entry of priority `pri` in a queue of priorities `queue`: after the last
/// entry whose priority, read back as the signed top two bits, is at least `pri`; else first. So a damage call
/// (priority 2, read back as -2) goes before everything, and a pace note (0) after the last queued note.
pub fn speech_slot(queue: impl IntoIterator<Item = u32>, pri: u32) -> usize {
    queue.into_iter().enumerate().fold(0, |at, (i, p)| if pri as i32 <= ((p << 30) as i32) >> 30 { i + 1 } else { at })
}

/// The priorities the game queues with: pace notes and countdown numbers 0 (`E25Handler::M6510`, `Vf48`), damage
/// calls 2 (`CCarSound::OnSpeech`).
const NOTE_PRIORITY: u32 = 0;
const DAMAGE_PRIORITY: u32 = 2;

/// The mixer's layers, for [`Mixer::solo`]: the engine stream, its reflections, the named loops (whine, turbo,
/// wind, siren, helicopter, cooler, grind), the tyre surface loops, the rumble and wetness, an FMOD bank's mix, the
/// rain, the one-shots and the speech.
pub const LAYERS: [&str; 9] = ["engine", "reflections", "loops", "tyres", "wheel", "fmod", "rain", "shots", "speech"];

/// The player's sound volume options (`0x01660e08` + 4 k, linear; the test install's profile has 0.9 for each), and
/// which voices each scales (their gain call sites in the original): the engine (`CalcVol`'s engine and reflections,
/// `E31Transmission`'s whines), the co-driver (`E31Speech::Vf08`, the `GO` at 0x4ba690), the effects (the turbo, the
/// blow-off, the wind, the rain, the impacts, squeaks, bottomings, pops and the other one-shots) and the tyres
/// (`E31Tire`'s surface loops, wetness, rumble and gravel hits). An FMOD bank's voices take none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volumes {
    pub engine: f32,
    pub codriver: f32,
    pub effects: f32,
    pub tyres: f32,
}

impl Default for Volumes {
    fn default() -> Volumes {
        Volumes { engine: 1.0, codriver: 1.0, effects: 1.0, tyres: 1.0 }
    }
}

impl Volumes {
    /// A one-shot's option.
    fn shot(&self, shot: Shot) -> f32 {
        match shot {
            Shot::Grus(_) => self.tyres,
            Shot::Go => self.codriver,
            _ => self.effects,
        }
    }
}

/// The mixer.
pub struct Mixer {
    rate: u32,
    /// The user's volume (linear).
    pub volume: f32,
    pub volumes: Volumes,
    /// Plays one of [`LAYERS`] alone (an index; `None` plays all), for level checks (ours).
    pub solo: Option<usize>,
    engine: Engine,
    engine_gain: Gain,
    /// `MixScale`'s gain at the end of the last block.
    mix_gain: f32,
    reflections: Reflections,
    reflections_gain: Gain,
    doors_off: [bool; 2],
    /// The engine's `E32Flt` (`E30Engine +0xf0`) and the outside cutoff (`CCarSound +0x41934`).
    filter: OnePole,
    low_shelf_frequency: f32,
    inside: bool,
    /// By key, so the mix sums them in the same order on every run.
    loops: BTreeMap<Loop, LoopVoice>,
    tyres: [TyreVoice; 8],
    /// Each wheel's rumble (0-3) and wetness (4-7) loops.
    wheel_loops: [LoopVoice; 8],
    rain: Rain,
    banks: Banks,
    playing: Vec<Playing>,
    /// One-shots and speech stopped early, fading out ([`Playing::stop`]).
    fading: Vec<Playing>,
    speech: VecDeque<(u32, Arc<Wave>)>,
    speaking: Option<Playing>,
    /// The pause after each word, and what is left of it.
    speech_gap: f32,
    speech_wait: f32,
    /// The car voices' distances, and where the car is from the camera.
    distances: Distances,
    car_distance: f32,
    car_pan: f32,
    /// The physics is NGP's ([`Command::Ngp`]).
    ngp: bool,
    /// Seconds since the last ranged impact started.
    since_ranged: f32,
    /// The sounds playing at the last frame's count ([`Command::Frame`], [`SOUND_LIMIT`]), and each one-shot kind's
    /// made and refused requests.
    sounds: usize,
    tally: BTreeMap<String, [u32; 2]>,
    /// An FMOD bank's loops, one per slot, with their buses, its recordings, the one-shots playing and their buses,
    /// and its mixing tree (`crate::fmod`; empty without one).
    fmod_loops: Vec<(LoopVoice, usize)>,
    fmod_waves: Vec<Arc<Wave>>,
    fmod_playing: Vec<(Playing, usize, Option<u16>)>,
    fmod_tree: Tree,
    rng: Rng,
    /// The hits' variation ([`VARIATION`]), apart from `rng` so the picks stay as they were.
    variation: Rng,
    limiter: Limiter,
    /// Gain move per sample.
    k: f32,
    scratch: Vec<f32>,
    /// One layer's frames before they join the mix.
    layer: Vec<[f32; 2]>,
}

impl Mixer {
    pub fn new(bank: &CarBank, rate: u32, volume: f32) -> Mixer {
        let voice = |wave: &Wave| LoopVoice { wave: Arc::new(seamless(wave)), pitch: 1.0, ..LoopVoice::default() };
        let loops = BTreeMap::from([
            (Loop::Gearbox, voice(&bank.transmission)),
            (Loop::Diff, voice(&bank.transmission)),
            (Loop::Turbo, voice(&bank.turbo)),
            (Loop::Wind, voice(&bank.wind)),
            (Loop::Siren, voice(&bank.siren)),
            (Loop::Helicopter, voice(&bank.helicopter)),
            (Loop::Cooler, voice(&bank.cooler)),
        ]);
        let materials = [Material::Gravel, Material::Tarmac, Material::Snow];
        let banks = Banks {
            blow_off: shared(std::slice::from_ref(&bank.blow_off)),
            gear_changes: shared(&bank.gear_changes),
            offroad: shared(&bank.offroad),
            ranged: shared(&bank.ranged),
            screams: shared(&bank.screams),
            go: shared(std::slice::from_ref(&bank.go)),
            birds: shared(&bank.birds),
            bird_hit: shared(std::slice::from_ref(&bank.bird_hit)),
            grus: shared(&bank.grus),
            squeaks: shared(&bank.squeaks),
            bottomings: shared(&bank.bottomings),
            drive_shafts: shared(&bank.drive_shafts),
            exhaust_pops: shared(&bank.exhaust_pops),
            pops: shared(&bank.pops),
            bangs: shared(&bank.bangs),
            fire: shared(std::slice::from_ref(&bank.fire)),
            ignition: shared(std::slice::from_ref(&bank.ignition)),
            dog_rings: shared(&bank.dog_rings),
            wiper: shared(&bank.wiper),
            surfaces: materials
                .iter()
                .zip(&bank.surfaces)
                .flat_map(|(&m, pair)| {
                    [((m, false), Arc::new(seamless(&pair[0]))), ((m, true), Arc::new(seamless(&pair[1])))]
                })
                .collect(),
            words: bank.codriver.words.iter().map(|(&id, takes)| (id, shared(takes))).collect(),
            numbers: shared(&bank.codriver.numbers),
            damage: shared(&bank.codriver.damage),
        };
        Mixer {
            rate,
            volume,
            volumes: Volumes::default(),
            solo: None,
            engine: Engine::new(bank.engine.clone(), rate, 0x5eed),
            engine_gain: Gain::default(),
            mix_gain: 1.0,
            reflections: Reflections::default(),
            reflections_gain: Gain::default(),
            doors_off: [false; 2],
            filter: OnePole::new(rate, bank.car.low_shelf_frequency),
            low_shelf_frequency: bank.car.low_shelf_frequency,
            inside: false,
            loops,
            tyres: Default::default(),
            wheel_loops: std::array::from_fn(|k| voice(if k < 4 { &bank.rumble } else { &bank.wetness })),
            rain: Rain::new(rate),
            banks,
            playing: Vec::new(),
            fading: Vec::new(),
            speech: VecDeque::new(),
            speaking: None,
            speech_gap: 0.0,
            speech_wait: 0.0,
            distances: bank.distances,
            car_distance: 0.0,
            car_pan: 0.0,
            ngp: false,
            since_ranged: f32::INFINITY,
            sounds: RACE_SOUNDS,
            tally: BTreeMap::new(),
            fmod_loops: Vec::new(),
            fmod_waves: Vec::new(),
            fmod_playing: Vec::new(),
            fmod_tree: Tree::default(),
            rng: Rng::new(0x9e37),
            variation: Rng::new(0x51ed),
            limiter: Limiter::new(rate),
            k: 1.0 - (-1.0 / (GAIN_TIME * rate as f32)).exp(),
            scratch: Vec::new(),
            layer: Vec::new(),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The engine's rpm after the slew.
    pub fn rpm(&self) -> u32 {
        self.engine.rpm
    }

    /// The in-car low-pass for a new voice that takes it ([`CABIN_CUTOFF`]).
    fn cabin(&self) -> Option<OnePole> {
        self.inside.then(|| OnePole::new(self.rate, CABIN_CUTOFF))
    }

    /// A car voice's gain at the car's distance from the camera.
    fn placed(&self, place: Place) -> f32 {
        self.placed_at(place, self.car_distance)
    }

    /// A car voice's gain `distance` m from the camera.
    fn placed_at(&self, place: Place, distance: f32) -> f32 {
        let d = &self.distances;
        let range = match place {
            Place::Exhaust if self.ngp => d.exhaust,
            // 1.02's last call (`+0x74`, the max) sets the min.
            Place::Exhaust => (d.exhaust.1, DEFAULT_MAX_DISTANCE),
            Place::Flat => return 1.0,
            Place::Tire => return SOFTWARE_TIRE_SCALE * software_gain(distance, TIRE_DISTANCES),
            Place::BlowOff if self.ngp => NGP_BLOW_OFF_DISTANCES,
            Place::BlowOff => BLOW_OFF_DISTANCES,
            Place::Ranged if self.ngp => d.ranged,
            // `CCollisionSnd::Vf64` (0x4b2ed0) calls `+0x78` with the min (0x4b2f9c), then `+0x74` with the max
            // (0x4b2fa7): 1.02's min is the `SOUNDDEF` max.
            Place::Ranged => (d.ranged.1, DEFAULT_MAX_DISTANCE),
            Place::Offroad if self.ngp => NGP_OFFROAD_DISTANCES,
            Place::Offroad => OFFROAD_DISTANCES,
            Place::ExhaustPop if self.ngp => d.exhaust_pop,
            // The pops and bangs (`Mf180` 0x4af22f, `Mf280` 0x4af396, `Mf3f0` 0x4af4a7) call `+0x74` with the max,
            // then `+0x78` with the min: 1.02 keeps the min.
            Place::ExhaustPop => (d.exhaust_pop.0, DEFAULT_MAX_DISTANCE),
            Place::Grus if self.ngp => NGP_GRUS_DISTANCES,
            Place::Grus => GRUS_DISTANCES,
            Place::Fire if self.ngp => NGP_FIRE_DISTANCES,
            Place::Fire => FIRE_DISTANCES,
            Place::Ignition => (IGNITION_MIN, DEFAULT_MAX_DISTANCE),
            Place::Cooler => (COOLER_MIN, DEFAULT_MAX_DISTANCE),
            Place::Turbo | Place::Default => DEFAULT_DISTANCES,
        };
        distance_gain(distance, range)
    }

    pub fn apply(&mut self, commands: &[Command]) {
        for command in commands {
            match *command {
                Command::Engine(noise) => self.engine.input(&noise),
                Command::Inside(inside) if inside != self.inside => {
                    self.inside = inside;
                    self.rain.set_inside(inside);
                    let cutoff = if inside { INSIDE_CUTOFF } else { self.low_shelf_frequency };
                    self.filter.retune(self.rate, cutoff);
                }
                Command::Inside(_) => {}
                Command::Listener { car_distance, car_pan } => {
                    self.car_distance = car_distance;
                    self.car_pan = car_pan;
                }
                Command::Gain { voice: Loop::Engine, gain } | Command::Loop { voice: Loop::Engine, gain, .. } => {
                    self.engine_gain.target = gain * self.placed(Place::Exhaust) * self.volumes.engine;
                }
                Command::Gain { voice, gain } => self.set_loop(voice, gain, None),
                Command::Loop { voice, gain, pitch } => self.set_loop(voice, gain, Some(pitch)),
                Command::Pan { voice, pan } => {
                    if let Some(v) = self.loops.get_mut(&voice) {
                        v.pan = pan;
                    }
                }
                Command::FmodLoop { slot, gain, pitch, outside } => {
                    let (placed, pan) = if outside { (self.placed(Place::Exhaust), self.car_pan) } else { (1.0, 0.0) };
                    self.fmod_tree.pan = pan;
                    if let Some((v, _)) = self.fmod_loops.get_mut(usize::from(slot)) {
                        v.gain.target = gain * placed;
                        v.pitch = pitch;
                    }
                }
                Command::FmodBus { bus, gain } => self.fmod_tree.set_gain(usize::from(bus), gain),
                Command::FmodEffect { bus, effect, param, value } => {
                    let (bus, effect, param) = (usize::from(bus), usize::from(effect), usize::from(param));
                    self.fmod_tree.set_param(bus, effect, param, value, self.rate);
                }
                Command::FmodShot { wave, gain, pitch, outside, bus, slot } => {
                    let (placed, pan) = if outside { (self.placed(Place::Exhaust), self.car_pan) } else { (1.0, 0.0) };
                    self.fmod_tree.pan = pan;
                    if let Some(wave) = self.fmod_waves.get(usize::from(wave)) {
                        let bus = bus.map_or(self.fmod_tree.master, usize::from);
                        let playing = Playing { pitch, ..Playing::new(wave.clone(), gain * placed) };
                        self.fmod_playing.push((playing, bus, slot));
                    }
                }
                Command::FmodTrack { slot, gain, pitch, outside } => {
                    let placed = if outside { self.placed(Place::Exhaust) } else { 1.0 };
                    for (p, _, _) in self.fmod_playing.iter_mut().filter(|(_, _, s)| *s == Some(slot)) {
                        (p.gain, p.pitch) = (gain * placed, pitch);
                    }
                }
                Command::Shot { shot, gain } => self.shot(shot, gain, None),
                Command::ShotAt { shot, gain, pan } => self.shot(shot, gain, Some(pan)),
                Command::Effect { shot, gain, pitch, repeat } => self.effect(shot, gain, pitch, repeat),
                Command::Hit { shot, gain, pitch, distance } => {
                    let (bank, i, place) = match shot {
                        Shot::Grus(i) => (&self.banks.grus, i, Place::Grus),
                        Shot::Squeak(i) => (&self.banks.squeaks, i, Place::Default),
                        Shot::Bottoming(i) => (&self.banks.bottomings, i, Place::Default),
                        Shot::DriveShaft(i) => (&self.banks.drive_shafts, i, Place::Default),
                        _ => continue,
                    };
                    let wave = (!bank.is_empty()).then(|| bank[i % bank.len()].clone());
                    if let Some(wave) = wave.filter(|_| self.make(shot)) {
                        let [vary_pitch, vary_gain] = self.vary();
                        let gain = gain * vary_gain * self.placed_at(place, distance) * self.volumes.shot(shot);
                        let (pitch, low_pass) = (pitch * vary_pitch, self.cabin());
                        self.playing.push(Playing { pan: self.car_pan, pitch, low_pass, ..Playing::new(wave, gain) });
                    }
                }
                Command::WheelLoop { wheel, voice, gain, pitch } => {
                    // The rumble plays in the car; the wetness is a 2D voice on the software path (`M6cb0`: flags
                    // 0x200), so neither takes a distance gain.
                    let (k, placed) = match voice {
                        WheelLoop::Rumble => (usize::from(wheel) % 4, 1.0),
                        WheelLoop::Wetness => (4 + usize::from(wheel) % 4, self.placed(Place::Flat)),
                    };
                    let v = &mut self.wheel_loops[k];
                    v.gain.target = gain * placed * self.volumes.tyres;
                    v.pitch = pitch;
                    v.pan = self.car_pan;
                }
                Command::Tyre { wheel, slot, material, skid, gain, pitch } => {
                    let placed = self.placed(Place::Tire);
                    let tyre = &mut self.tyres[(usize::from(wheel) * 2 + usize::from(slot)) % 8];
                    if tyre.key != Some((material, skid)) {
                        if let Some(wave) = self.banks.surfaces.get(&(material, skid)) {
                            tyre.key = Some((material, skid));
                            // WaveParse and SetScaled: the new loop starts a quarter of its length per wheel in.
                            tyre.voice.pos = wave.samples.len() as f64 * 0.25 * f64::from(wheel);
                            tyre.voice.wave = wave.clone();
                        }
                    }
                    tyre.voice.gain.target = gain * placed * self.volumes.tyres;
                    if !self.inside || skid {
                        tyre.voice.low_pass = None;
                    } else if tyre.voice.low_pass.is_none() {
                        tyre.voice.low_pass = Some(OnePole::new(self.rate, CABIN_CUTOFF));
                    }
                    tyre.voice.pitch = pitch;
                    tyre.voice.pan = self.car_pan;
                }
                Command::Speech(speech) => {
                    let pick = self.rng.rand() as usize;
                    let (pri, wave) = match speech {
                        Speech::Word(id) => (
                            NOTE_PRIORITY,
                            self.banks.words.get(&id).filter(|t| !t.is_empty()).map(|t| t[pick % t.len()].clone()),
                        ),
                        Speech::Number(n) => {
                            (NOTE_PRIORITY, self.banks.numbers.get(n.wrapping_sub(1) as usize).cloned())
                        }
                        Speech::Damage(code) => (DAMAGE_PRIORITY, self.banks.damage.get(code as usize).cloned()),
                    };
                    // SpInsert drops an entry when the queue is full.
                    if let Some(wave) = wave.filter(|w| !w.samples.is_empty() && self.speech.len() < SPEECH_QUEUE) {
                        let at = speech_slot(self.speech.iter().map(|(p, _)| *p), pri);
                        self.speech.insert(at, (pri, wave));
                    }
                }
                Command::Frame => self.sounds = RACE_SOUNDS + self.playing.len(),
                Command::StopSpeech => {
                    self.speech.clear();
                    if let Some(mut speaking) = self.speaking.take() {
                        speaking.stop(self.rate);
                        self.fading.push(speaking);
                    }
                }
                Command::SpeechGap(gap) => self.speech_gap = gap,
                Command::Ngp(ngp) => self.ngp = ngp,
                Command::Reflections { gain, doors_off } => {
                    self.reflections_gain.target = gain * self.volumes.engine;
                    self.doors_off = doors_off;
                }
                Command::Rain { gain, amount } => {
                    self.rain.gain.target = gain * self.volumes.effects;
                    self.rain.amount = amount;
                }
            }
        }
    }

    /// Plays a one-shot: a car voice at the car's distance and side, or at side `at` with its distance gain in
    /// `gain`.
    fn shot(&mut self, shot: Shot, gain: f32, at: Option<f32>) {
        let pick = self.rng.rand() as usize;
        let (bank, index, place) = match shot {
            Shot::GearChange => (&self.banks.gear_changes, pick, None),
            Shot::Offroad(i) => (&self.banks.offroad, i, Some(Place::Offroad)),
            Shot::Ranged(i) => (&self.banks.ranged, i, Some(Place::Ranged)),
            Shot::Scream(i) => (&self.banks.screams, i, None),
            Shot::Go => (&self.banks.go, 0, None),
            Shot::Bird(i) => (&self.banks.birds, i, None),
            Shot::BirdHit => (&self.banks.bird_hit, 0, None),
            // 2D voices (`E31Transmission::M3510`, `Md490`: the master's `+0x50` with no position).
            Shot::DogRing(i) => (&self.banks.dog_rings, i, None),
            Shot::WiperEngine => (&self.banks.wiper, 0, None),
            Shot::WiperUp => (&self.banks.wiper, 1, None),
            Shot::WiperDown => (&self.banks.wiper, 2, None),
            Shot::BlowOff
            | Shot::Grus(_)
            | Shot::Squeak(_)
            | Shot::Bottoming(_)
            | Shot::DriveShaft(_)
            | Shot::ExhaustPop(_)
            | Shot::Pop(_)
            | Shot::Bang(_)
            | Shot::Fire
            | Shot::Ignition => return self.effect(shot, gain, 1.0, Repeat::Overlap),
        };
        let gain = gain * self.volumes.shot(shot);
        let ranged = matches!(shot, Shot::Ranged(_));
        let capped = ranged && !ranged_plays(self.since_ranged, self.playing.iter().filter(|p| p.ranged).count());
        let wave = (!bank.is_empty() && !capped).then(|| bank[index % bank.len()].clone());
        if let Some(wave) = wave.filter(|_| self.make(shot)) {
            let (gain, pan) = match (at, place) {
                (Some(pan), _) => (gain, pan),
                (None, Some(place)) => (gain * self.placed(place), self.car_pan),
                (None, None) => (gain, 0.0),
            };
            let [pitch, vary_gain] =
                if matches!(shot, Shot::Offroad(_) | Shot::Ranged(_)) { self.vary() } else { [1.0; 2] };
            self.playing.push(Playing { pan, ranged, pitch, ..Playing::new(wave, gain * vary_gain) });
            if ranged {
                self.since_ranged = 0.0;
            }
        }
    }

    /// Plays an effect at the car at `pitch`; a [`Repeat::Restart`] one stops the last of its kind, a
    /// [`Repeat::IfIdle`] one is skipped while the last plays.
    fn effect(&mut self, shot: Shot, gain: f32, pitch: f32, repeat: Repeat) {
        let (bank, index, place, family) = match shot {
            Shot::Grus(i) => (&self.banks.grus, i, Place::Grus, None),
            Shot::Squeak(i) => (&self.banks.squeaks, i, Place::Default, None),
            Shot::Bottoming(i) => (&self.banks.bottomings, i, Place::Default, None),
            Shot::DriveShaft(i) => (&self.banks.drive_shafts, i, Place::Default, None),
            Shot::ExhaustPop(i) => (&self.banks.exhaust_pops, i, Place::ExhaustPop, Some(Family::ExhaustPop)),
            Shot::Pop(i) => (&self.banks.pops, i, Place::ExhaustPop, Some(Family::Pop)),
            Shot::Bang(i) => (&self.banks.bangs, i, Place::ExhaustPop, Some(Family::Bang)),
            Shot::Fire => (&self.banks.fire, 0, Place::Fire, Some(Family::Fire)),
            Shot::BlowOff => (&self.banks.blow_off, 0, Place::BlowOff, Some(Family::BlowOff)),
            Shot::Ignition => (&self.banks.ignition, 0, Place::Ignition, Some(Family::Ignition)),
            _ => return self.shot(shot, gain, None),
        };
        if bank.is_empty() {
            return;
        }
        let wave = bank[index % bank.len()].clone();
        let last = self.playing.iter().rfind(|p| family.is_some() && p.family == family);
        let wait = match repeat {
            Repeat::IfIdle => last.is_some(),
            Repeat::After(n) => last.is_some_and(|p| p.pos <= f64::from(n)),
            _ => false,
        };
        if wait || !self.make(shot) {
            return;
        }
        if repeat == Repeat::Restart && family.is_some() {
            let (rate, fading) = (self.rate, &mut self.fading);
            self.playing.retain_mut(|p| {
                let keep = p.family != family;
                if !keep {
                    p.stop(rate);
                    fading.push(p.clone());
                }
                keep
            });
        }
        let [vary_pitch, vary_gain] =
            if matches!(shot, Shot::Grus(_) | Shot::Squeak(_)) { self.vary() } else { [1.0; 2] };
        let gain = gain * vary_gain * self.placed(place) * self.volumes.shot(shot);
        let pitch = pitch * vary_pitch;
        let low_pass = if family.is_none() { self.cabin() } else { None };
        self.playing.push(Playing { pan: self.car_pan, pitch, family, low_pass, ..Playing::new(wave, gain) });
    }

    /// Whether the original's mixer makes a one-shot now ([`SOUND_LIMIT`]), tallied by kind. A sound it makes takes
    /// a place even when silent. The ranged impacts and the `GO` (flag 0x400: 0x4b2f5f, 0x4ba5ea) are always made:
    /// `Vf48` (0x4cf98f) stops the newest sound without flag 0x80 or 0x10, here the newest one-shot.
    fn make(&mut self, shot: Shot) -> bool {
        let full = self.sounds >= SOUND_LIMIT;
        let steals = matches!(shot, Shot::Ranged(_) | Shot::Go);
        if full && steals {
            if let Some(mut stolen) = self.playing.pop() {
                stolen.stop(self.rate);
                self.fading.push(stolen);
            }
        }
        let made = !full || steals;
        let kind = format!("{shot:?}");
        let kind = kind.split('(').next().unwrap_or_default();
        self.tally.entry(kind.to_string()).or_default()[usize::from(!made)] += 1;
        made
    }

    /// A hit's pitch and gain factors, each within [`VARIATION`] of 1.
    fn vary(&mut self) -> [f32; 2] {
        VARIATION.map(|v| 1.0 + v * (self.variation.rand() as f32 / 16384.0 - 1.0))
    }

    /// Each one-shot kind's made and refused requests ([`SOUND_LIMIT`]).
    pub fn tally(&self) -> &BTreeMap<String, [u32; 2]> {
        &self.tally
    }

    /// Gives an FMOD bank's slots their loops and buses, the mixer its recordings and the bank's mixing tree
    /// ([`Command::FmodLoop`], [`Command::FmodBus`], [`Command::FmodEffect`], [`Command::FmodShot`]).
    pub fn set_fmod(&mut self, loops: Vec<(Arc<Wave>, Option<usize>)>, waves: Vec<Arc<Wave>>, buses: &[Bus]) {
        self.fmod_tree = Tree::new(buses, self.rate);
        let master = self.fmod_tree.master;
        self.fmod_loops = loops
            .into_iter()
            .map(|(wave, bus)| (LoopVoice { wave, pitch: 1.0, ..LoopVoice::default() }, bus.unwrap_or(master)))
            .collect();
        self.fmod_waves = waves;
    }

    /// Gives the grind loop ([`Loop::Grind`]) its sample.
    pub fn set_grind(&mut self, wave: Wave) {
        let wave = Arc::new(seamless(&wave));
        self.loops.insert(Loop::Grind, LoopVoice { wave, pitch: 1.0, ..LoopVoice::default() });
    }

    /// A loop's gain and pitch; the car's loops take their distance gain and side.
    fn set_loop(&mut self, voice: Loop, gain: f32, pitch: Option<f32>) {
        let place = match voice {
            Loop::Gearbox | Loop::Diff => Some(Place::Flat),
            Loop::Turbo => Some(Place::Turbo),
            Loop::Cooler => Some(Place::Cooler),
            Loop::Engine | Loop::Wind | Loop::Siren | Loop::Helicopter | Loop::Grind => None,
        };
        let placed = place.map(|p| (self.placed(p), self.car_pan));
        let option = match voice {
            Loop::Engine | Loop::Gearbox | Loop::Diff => self.volumes.engine,
            _ => self.volumes.effects,
        };
        if let Some(v) = self.loops.get_mut(&voice) {
            v.gain.target = gain * placed.map_or(1.0, |(g, _)| g) * option;
            if let Some((_, pan)) = placed {
                v.pan = pan;
            }
            if let Some(pitch) = pitch {
                v.pitch = pitch;
            }
        }
    }

    /// Stereo frames (left, right), -1 to 1, into `out` (overwritten).
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        let play = |layer: usize| self.solo.is_none_or(|s| s == layer);
        let play: [bool; LAYERS.len()] = std::array::from_fn(play);
        let mut layer = std::mem::take(&mut self.layer);
        layer.clear();
        layer.resize(out.len(), [0.0; 2]);
        self.scratch.resize(out.len(), 0.0);
        self.engine.render(&mut self.scratch);
        let gain = mix_gain(self.engine.rpm, self.engine.load);
        mix_scale(&mut self.scratch, self.mix_gain, gain);
        self.mix_gain = gain;
        let side = balance(self.car_pan);
        let low = (self.engine.rpm as f32 - OUTSIDE_LOW_RPM) * OUTSIDE_LOW_PER_RPM;
        for (frame, &s) in out.iter_mut().zip(&self.scratch) {
            let s = engine_filter(&mut self.filter, s, self.inside, low) * self.engine_gain.next(self.k);
            *frame = if play[0] { [s * side[0], s * side[1]] } else { [0.0; 2] };
        }
        let keep = |layer: &mut Vec<[f32; 2]>, out: &mut [[f32; 2]], on: bool| {
            if on {
                for (o, l) in out.iter_mut().zip(layer.iter()) {
                    o[0] += l[0];
                    o[1] += l[1];
                }
            }
            layer.fill([0.0; 2]);
        };
        let (gain, k) = (&mut self.reflections_gain, self.k);
        self.reflections.add(
            &self.scratch,
            self.rate,
            self.inside,
            self.car_distance,
            self.engine.rpm,
            self.doors_off,
            &mut layer,
            || gain.next(k),
        );
        keep(&mut layer, out, play[1]);
        for voice in self.loops.values_mut() {
            voice.add(&mut layer, self.rate, self.k);
        }
        keep(&mut layer, out, play[2]);
        for voice in self.tyres.iter_mut().map(|t| &mut t.voice) {
            voice.add(&mut layer, self.rate, self.k);
        }
        keep(&mut layer, out, play[3]);
        for voice in self.wheel_loops.iter_mut() {
            voice.add(&mut layer, self.rate, self.k);
        }
        keep(&mut layer, out, play[4]);
        let (len, rate, k) = (out.len(), self.rate, self.k);
        for (voice, bus) in self.fmod_loops.iter_mut() {
            if voice.audible() {
                if let Some(input) = self.fmod_tree.input(*bus, len) {
                    voice.add_mono(input, rate, k, 1.0 / FULL_SCALE);
                }
            }
        }
        let tree = &mut self.fmod_tree;
        self.fmod_playing
            .retain_mut(|(p, bus, _)| tree.input(*bus, len).is_some_and(|i| p.add_mono(i, rate, 1.0 / FULL_SCALE)));
        self.fmod_tree.render(&mut layer, k, FULL_SCALE);
        keep(&mut layer, out, play[5]);
        self.rain.add(&mut layer, self.k);
        keep(&mut layer, out, play[6]);
        let rate = self.rate;
        self.since_ranged += out.len() as f32 / rate as f32;
        self.playing.retain_mut(|p| p.add(&mut layer, rate));
        self.fading.retain_mut(|p| p.add(&mut layer, rate));
        keep(&mut layer, out, play[7]);
        // The speech voice: the next queued sample once the last has ended and the pause after it has passed (a
        // sample that ends mid-block lets the next start at a following block).
        if self.speaking.is_none() {
            if self.speech_wait > 0.0 {
                self.speech_wait -= out.len() as f32 / rate as f32;
            } else if let Some((_, wave)) = self.speech.pop_front() {
                self.speaking = Some(Playing::new(wave, self.volumes.codriver));
                self.speech_wait = self.speech_gap;
            }
        }
        if let Some(speaking) = &mut self.speaking {
            if !speaking.add(&mut layer, rate) {
                self.speaking = None;
            }
        }
        keep(&mut layer, out, play[8]);
        self.layer = layer;
        let scale = self.volume * HEADROOM / FULL_SCALE;
        for frame in out.iter_mut() {
            *frame = frame.map(|s| s * scale);
        }
        self.limiter.run(out);
        for frame in out.iter_mut() {
            *frame = frame.map(soft_clip);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limiter_is_linear_below_its_knee_and_never_passes_one() {
        assert_eq!(soft_clip(0.5), 0.5);
        assert_eq!(soft_clip(-0.8), -0.8);
        assert!(soft_clip(3.0) <= 1.0 && soft_clip(1.2) > 0.99);
        assert!(soft_clip(-0.9) > -0.9 && soft_clip(-0.9) < -0.8);
    }

    #[test]
    fn ranged_impacts_are_spaced_and_capped() {
        assert!(ranged_plays(f32::INFINITY, 0));
        assert!(!ranged_plays(0.05, 0));
        assert!(ranged_plays(0.1, 2));
        assert!(!ranged_plays(0.1, MAX_RANGED));
    }

    #[test]
    fn damage_calls_go_first_and_notes_after_the_last_note() {
        assert_eq!(speech_slot([], 0), 0);
        assert_eq!(speech_slot([0, 0], 0), 2);
        assert_eq!(speech_slot([2, 0, 0], 0), 3);
        assert_eq!(speech_slot([0, 0], 2), 0);
        assert_eq!(speech_slot([2, 0], 2), 0);
        // A note behind only damage calls goes first (the signed read of priority 2 is -2).
        assert_eq!(speech_slot([2], 0), 0);
    }

    #[test]
    fn the_software_path_falls_linearly_from_min_to_max() {
        assert_eq!(software_gain(5.0, TIRE_DISTANCES), 1.0);
        assert_eq!(software_gain(25.0, TIRE_DISTANCES), 0.5);
        assert_eq!(software_gain(41.0, TIRE_DISTANCES), 0.0);
    }
}

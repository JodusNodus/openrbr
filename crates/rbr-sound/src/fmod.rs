//! An RSF car's own sound from its FMOD Studio bank (`rbr_formats::fmod`; ours, rbr-recomp `notes/sound.md` section 14): the
//! car's recordings, laid out as the bank's events lay them out, in place of the 1.02 engine, turbo, blow-off and
//! transmission voices, with the bank's gear, backfire, ignition, wind and wheel events on top. NGP plays these
//! banks through FMOD (`NgpFmodDaemon.exe`); FMOD is not used here: the bank's mixing tree (tracks, the mixer's
//! groups, returns and sends, their faders, effects and automations) runs in the mixer (`mixer::fmod_mix`,
//! `mixer::dsp`), and the rest is left out: the convolution reverb, distortion and 3D panner, modulators, the curve
//! shapes (straight lines here), and the events' timelines.
//!
//! What is the bank's: which recordings play over which rpm, throttle, boost and wheel speed ranges, their volume
//! crossfades and the tracks' automations over those parameters, the instruments' volume and pitch, and their
//! auto-pitch (the recording's rpm). What is ours: some parameter values (below), a blow-off playing one of its
//! event's recordings at random on the 1.02 valve's trigger, the outside events taking the 1.02 engine voice's
//! distance gain, and [`MIX`].
//!
//! The parameters, as NGP's FMOD daemon sets them (`NgpFmodDaemon.exe` 7.4, `FUN_004092f0`, from the block `NGP.dll`
//! `FUN_100b3470` fills from each `EngineNoise` emit in app states 1 and 8; rbr-recomp `notes/sound.md` 14): `rpms` the engine
//! rpm, `throttle` the engine's throttle; `exhaustpop` the misfire word `p[4]` from 0.9 up, else 0; `startup` 1
//! while the engine is stopped (`CCarSound +0x41924`) and the starter word (drivetrain `p[9]`, `+0x418f0`) is above
//! 0, with `battery` that word (it keeps its last value otherwise); `car_speed` the car's speed in km/h, 0 under 1;
//! the transmission's `wheel_speed` the drive's speed (FMB1, block +0x30) and each of the four `wheel` events'
//! `wheel_speed` its wheel's (block +0x38..+0x44), with `brake` the car's +0x860 (block +0x10); `gear` the gear - 1
//! and `state` a random 0.6-1.0 on the update of a shift up, 0.1-0.5 down, else 0; `fluctuation` on the engine
//! events and the transmission, 1 on a shift, falling to 0 over the event's `fluctuation_length` (a shift waits,
//! halving them, while one still runs). Ours: `boost` the 1.02 turbo level (`car::turbo_level`), `valve` the 1.02
//! blow-off volume, `Speed` and the other events' `wheel_speed` the car's speed, `Distance` the camera's distance
//! from the car, `Event Cone Angle` the angle between the car's back (against its velocity) and the camera (the
//! banks' `rear` recordings are loud near 0 and their `front` ones near 180).
//!
//! The daemon pauses the events (`FUN_00408ef0`): the `_int` ones outside the car and the `_ext` ones in it, the wind
//! outside the car, and the engine, turbo, blow-off and backfire while the engine is stopped; all of them 0.2 s after
//! NGP last sent its block (the start orbit). The orbit's first 0.2 s plays every event unpaused with `rpms` the
//! car's `IdleRPM_NGP` and nothing else set (command 2). A gear, backfire or ignition instrument is a one-shot that
//! starts as its parameters enter its regions (FMOD's parameter sheets; a `gear` region holds its start, not its
//! end) and plays to its end (ours: the instrument's async flag is not read), following its automations while it
//! plays; its recording is one of its playlist at random. The wind and the wheels are loops; the wheels' tracks are
//! one per wheel, as FMOD gives each event instance its own.
//!
//! NGP turns the 1.02 voices an event stands in for down to 0.003 when the bank has the event (`NGP.dll`'s app
//! state handler, 0x100b32cc-0x100b32fd, on app state 10; the daemon's load flags, `FUN_00409ab0`, say whether the
//! event exists, even without instruments): the pops (`Pop`, `ExhaustPop`) with `backfire_int`, the bangs with
//! `backfire_ext`, the ignition, the blow-off valve with `blowoff_int`, the two whines with `transmission`, and the
//! wind; with `gear_int` it raises the 1.02 gear-change click's threshold to 1 and its volume to 0.003
//! ([`FmodCar::gear_click`]). [`FmodCar::mute`] does the rest, so those voices still take their places under the
//! sound limit.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use lewton::audio::{read_audio_packet, PreviousWindowRight};
use lewton::header::{read_header_ident, read_header_setup};
use rbr_formats::fmod::{self, interpolate, Bank, Bus, Layer, Property, Sample};
use rbr_recording::sound::{FmodBlock, Inputs, Sound};

use crate::car::{blow_off_volume, turbo_level, BLOW_OFF_CAP, BLOW_OFF_RISE};
use crate::data::{mono, Wave};
use crate::engine::Rng;
use crate::voice::Command;
use crate::world::Scene;

/// The bank's level against the 1.02 voices' (ours: the engine's RMS on a drive, chase camera, about the 1.02
/// engine's).
pub const MIX: f32 = 1.0;
/// The engine loudness every bank is brought to, in the car and outside ([`crate::mixer::fmod_loudness`]; ours), and
/// the most a bank is raised or lowered (30 dB). The 57 RSF banks measure -36 to -1 dB in the car and -31 to +1 dB
/// outside (the Fiesta Rally2's -28 dB, the 208 R2's +1 dB), so one car could be all engine and the next all tyres.
/// The targets put a bank's engine where the retail engine is on the same drive (tanner under NGP, the Fiesta Rally3
/// with and without its bank: -16.4 dB in the car, -17.9 dB on the chase camera), as against the tyres and effects.
const LOUDNESS: [f32; 2] = [0.255, 0.293];
const MAX_TRIM: f32 = 32.0;
/// rad/s to rpm.
const RPM_PER_RAD_S: f32 = 9.549_296;
const KMH_PER_MS: f32 = 3.6;
/// Below this speed (m/s) the camera is taken as behind the car (`Event Cone Angle` 0).
const STILL: f32 = 1.0;

/// The Vorbis setup headers FMOD's encoder writes, by the CRC32 the FSB5 keeps in their place: the ones the RSF
/// banks use (`data/fsb-vorbis/README.md`).
const SETUP_HEADERS: [(u32, &[u8]); 7] = [
    (0xc4c3_0a29, include_bytes!("../data/fsb-vorbis/c4c30a29.bin")),
    (0x3552_95ca, include_bytes!("../data/fsb-vorbis/355295ca.bin")),
    (0x38aa_59ce, include_bytes!("../data/fsb-vorbis/38aa59ce.bin")),
    (0x6d39_bf3e, include_bytes!("../data/fsb-vorbis/6d39bf3e.bin")),
    (0xf278_1a42, include_bytes!("../data/fsb-vorbis/f2781a42.bin")),
    (0x84d3_ac87, include_bytes!("../data/fsb-vorbis/84d3ac87.bin")),
    (0x2ba1_a444, include_bytes!("../data/fsb-vorbis/2ba1a444.bin")),
];

/// What an event stands in for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Loops, the in-car (`false`) or outside events.
    Engine(bool),
    Turbo,
    Transmission,
    /// One-shots on their parameters' regions.
    Gear(bool),
    /// One-shots on the 1.02 valve's trigger.
    BlowOff(bool),
    /// One-shots on their parameters' regions.
    Backfire(bool),
    Ignition,
    /// A loop in the car only.
    Wind,
    /// A loop per wheel: the daemon starts the event four times (+0x238, +0x264, +0x290, +0x2bc).
    Wheel(u8),
}

const EVENTS: [(&str, Role); 13] = [
    ("event:/engine_int", Role::Engine(false)),
    ("event:/engine_ext", Role::Engine(true)),
    ("event:/turbo", Role::Turbo),
    ("event:/transmission", Role::Transmission),
    ("event:/gear_int", Role::Gear(false)),
    ("event:/gear_ext", Role::Gear(true)),
    ("event:/blowoff_int", Role::BlowOff(false)),
    ("event:/blowoff_ext", Role::BlowOff(true)),
    ("event:/backfire_int", Role::Backfire(false)),
    ("event:/backfire_ext", Role::Backfire(true)),
    ("event:/ignition", Role::Ignition),
    ("event:/wind", Role::Wind),
    ("event:/wheel", Role::Wheel(0)),
];
const WHEELS: u8 = 4;

/// NGP's level for a 1.02 voice the bank stands in for (`0x3b449ba6`, written at 0x100b32f5).
pub const MUTED: f32 = 0.003;
/// NGP's words for the 1.02 gear-change click when the bank has `gear_int` (0x100b32a0-0x100b32c5): its threshold
/// (`0x10173210`, which NGP patches in at 0x4ae62d for 0x795e94's 0.001) and its volume (`0x10173214`, patched in at
/// 0x4d34e7 for 0.9), turned down with the other muted voices.
const GEAR_CLICK: (f32, f32) = (1.0, MUTED);
/// The misfire word's threshold for `exhaustpop` (daemon 0x4098d6).
const EXHAUST_POP: f32 = 0.9;
/// NGP's speeds under this (km/h) are 0 (`0x10147118`).
const STILL_KMH: f32 = 1.0;
/// The daemon pauses every event when no command has come for this long (s): its loop waits 200 ms
/// (`WaitForMultipleObjects` at 0x40888d and 0x408e8b) and on the timeout calls `FUN_00408ef0` with the pause set
/// (0x408e46-0x408e6a).
const WATCHDOG: f32 = 0.2;
/// The events whose `fluctuation` the daemon drives (`+0x2c`, `+0x58`, `+0x1b8`), in that order.
const FLUCTUATING: [Role; 3] = [Role::Engine(false), Role::Engine(true), Role::Transmission];
/// A shift's `state`: `rand() % 100 / 100 * 0.4` above 0.6 up or 0.1 down (0x409547).
const STATE_UP: f32 = 0.6;
const STATE_DOWN: f32 = 0.1;
const STATE_SPAN: f32 = 0.4;
/// The daemon's last gear after command 2 (`+0x2cc`, 0x4089b5).
const START_GEAR: i32 = 1;

/// One sample mixed down to mono, at the 16-bit scale of the retail waves.
fn decode(bank: &Bank, sample: &Sample) -> Result<Wave, String> {
    if sample.rate == 0 {
        return Err(format!("{}: sample rate 0", sample.name));
    }
    let data = bank.sample_data(sample);
    let channels = usize::from(sample.channels.max(1));
    let mut samples: Vec<f32> = match sample.codec {
        fmod::PCM16 => data
            .chunks_exact(2 * channels)
            .map(|f| mono(channels, f.chunks_exact(2).map(|s| f32::from(i16::from_le_bytes([s[0], s[1]])))))
            .collect(),
        fmod::VORBIS => {
            let crc = sample.vorbis_crc.ok_or("no Vorbis CRC")?;
            let setup = SETUP_HEADERS.iter().find(|h| h.0 == crc).ok_or(format!("Vorbis setup {crc:08x}: unknown"))?;
            let mut ident = vec![1];
            ident.extend_from_slice(b"vorbis");
            ident.extend_from_slice(&[0; 4]);
            ident.push(sample.channels);
            ident.extend_from_slice(&sample.rate.to_le_bytes());
            ident.extend_from_slice(&[0; 12]);
            // Block sizes 256 and 2048, FMOD's, and the framing bit.
            ident.extend_from_slice(&[0xb8, 1]);
            let ident = read_header_ident(&ident).map_err(|e| format!("{e:?}"))?;
            let setup = read_header_setup(setup.1, sample.channels, (8, 11)).map_err(|e| format!("{e:?}"))?;
            let mut window = PreviousWindowRight::new();
            let mut out = Vec::with_capacity(sample.frames as usize);
            let mut at = 0;
            while at + 2 <= data.len() {
                let size = usize::from(u16::from_le_bytes([data[at], data[at + 1]]));
                let Some(packet) = data.get(at + 2..at + 2 + size).filter(|_| size != 0 && size != 0xffff) else {
                    break;
                };
                let block = read_audio_packet(&ident, &setup, packet, &mut window).map_err(|e| format!("{e:?}"))?;
                let len = block.first().map_or(0, Vec::len);
                out.extend((0..len).map(|i| mono(channels, block.iter().map(|c| f32::from(c[i])))));
                at += 2 + size;
            }
            out
        }
        codec => return Err(format!("FSB5 codec {codec}")),
    };
    samples.truncate(sample.frames as usize);
    Ok(Wave { rate: sample.rate, samples })
}

/// An instrument of one of the played events.
#[derive(Debug, Clone)]
pub struct Slot {
    pub role: Role,
    pub layer: Layer,
}

/// The gain and pitch `layer` plays at with parameter `k` at `value(k)` (`None`: not known), or `None` when a known
/// parameter is outside its regions.
pub fn play(layer: &Layer, value: impl Fn(usize) -> Option<f32>) -> Option<(f32, f32)> {
    for r in &layer.regions {
        if value(r.param).is_some_and(|v| v < r.start || v > r.end) {
            return None;
        }
    }
    Some(level(layer, value))
}

/// The gain and pitch `layer`'s volume, pitch and automations give it at `value(k)`, wherever its regions are.
pub fn level(layer: &Layer, value: impl Fn(usize) -> Option<f32>) -> (f32, f32) {
    let (mut db, mut gain, mut semitones) = (layer.volume_db, 1.0, layer.semitones);
    for a in &layer.automations {
        let Some(v) = value(a.param) else { continue };
        let y = interpolate(&a.points, v);
        match a.property {
            Property::VolumeDb => db += y,
            Property::Gain => gain *= y.max(0.0),
            Property::Semitones => semitones += y,
            Property::Effect(_) => {}
        }
    }
    let auto = layer.auto_pitch.and_then(|p| {
        let v = value(p.param)?;
        Some(p.min + (1.0 - p.min) * v / p.reference.max(1.0))
    });
    let pitch = 2f32.powf(semitones / 12.0) * auto.unwrap_or(1.0);
    (gain * 10f32.powf(db / 20.0), pitch.clamp(0.05, 8.0))
}

/// A speed (m/s) as NGP's block has it: its magnitude in km/h, 0 under 1.
fn kmh(speed: f32) -> f32 {
    let v = speed.abs() * KMH_PER_MS;
    if v < STILL_KMH {
        0.0
    } else {
        v
    }
}

/// What the bank's parameters are given this step, by name.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Params {
    pub rpm: f32,
    pub throttle: f32,
    pub boost: f32,
    pub speed_kmh: f32,
    pub distance: f32,
    pub cone_angle: f32,
    pub valve: f32,
    pub exhaust_pop: f32,
    pub startup: f32,
    pub battery: f32,
    /// The gear events' `gear` (`None`: not known, without FMB1) and `state`.
    pub gear: Option<f32>,
    pub state: f32,
    /// The wheels' `brake`.
    pub brake: f32,
    /// The transmission's and each wheel's `wheel_speed`, km/h (`None`: not known, the car's speed instead).
    pub transmission_kmh: Option<f32>,
    pub wheels_kmh: Option<[f32; 4]>,
    /// The `fluctuation` of [`FLUCTUATING`].
    pub fluctuation: [f32; 3],
}

impl Params {
    /// Parameter `name` of event `role` (`None`: a bus no event owns).
    fn by_name(&self, name: &str, role: Option<Role>) -> Option<f32> {
        Some(match (name, role) {
            ("rpms", _) => self.rpm,
            ("throttle", _) => self.throttle,
            ("boost", _) => self.boost,
            ("wheel_speed", Some(Role::Transmission)) => self.transmission_kmh.unwrap_or(self.speed_kmh),
            ("wheel_speed", Some(Role::Wheel(k))) => self.wheels_kmh.map_or(self.speed_kmh, |w| w[usize::from(k)]),
            ("wheel_speed" | "car_speed" | "Speed", _) => self.speed_kmh,
            ("Distance", _) => self.distance,
            ("Event Cone Angle", _) => self.cone_angle,
            ("valve", _) => self.valve,
            ("exhaustpop", _) => self.exhaust_pop,
            ("startup", _) => self.startup,
            ("battery", _) => self.battery,
            ("brake", Some(Role::Wheel(_))) => self.brake,
            ("brake", _) => 0.0,
            ("fluctuation", _) => {
                role.and_then(|r| FLUCTUATING.iter().position(|&f| f == r)).map_or(0.0, |i| self.fluctuation[i])
            }
            ("gear", _) => return self.gear,
            ("state", _) => self.state,
            _ => return None,
        })
    }

    /// From one step's sound inputs, NGP's block values (FMB1) and the camera.
    pub fn new(sound: &Sound, block: Option<&FmodBlock>, scene: &Scene) -> Params {
        let e = |i: usize| sound.engine_f32(i);
        let to_camera: [f32; 3] = std::array::from_fn(|i| scene.listener[i] - scene.car[i]);
        let distance = to_camera.iter().map(|v| v * v).sum::<f32>().sqrt();
        let speed = scene.velocity.iter().map(|v| v * v).sum::<f32>().sqrt();
        let cone_angle = if speed < STILL || distance < 1e-3 {
            0.0
        } else {
            let cos = -(0..3).map(|i| scene.velocity[i] * to_camera[i]).sum::<f32>() / (speed * distance);
            cos.clamp(-1.0, 1.0).acos().to_degrees()
        };
        Params {
            rpm: e(0).max(0.0) * RPM_PER_RAD_S,
            throttle: e(1).clamp(0.0, 1.0),
            boost: turbo_level(sound),
            speed_kmh: kmh(sound.speed),
            distance,
            cone_angle,
            exhaust_pop: if e(4) >= EXHAUST_POP { e(4) } else { 0.0 },
            brake: block.map_or(0.0, |b| b.brake),
            transmission_kmh: block.map(|b| kmh(b.transmission_speed)),
            wheels_kmh: block.map(|b| b.wheel_speeds.map(kmh)),
            ..Params::default()
        }
    }
}

/// Bus `b` as event instance `role`'s: the event's own buses (its tracks, master and input, those without a name)
/// copied for an instance after the first (`copy`, recorded in `copies`), the mixer's groups shared. `roles` takes
/// each bus's event.
fn instance_bus(
    buses: &mut Vec<Bus>,
    roles: &mut Vec<Option<Role>>,
    copies: &mut HashMap<usize, usize>,
    b: usize,
    role: Role,
    copy: bool,
) -> usize {
    if !buses[b].name.is_empty() {
        return b;
    }
    if let Some(&c) = copies.get(&b) {
        return c;
    }
    copies.insert(b, b);
    let parent = buses[b].parent.map(|p| instance_bus(buses, roles, copies, p, role, copy));
    let at = if copy {
        buses.push(Bus { parent, ..buses[b].clone() });
        roles.push(None);
        buses.len() - 1
    } else {
        b
    };
    roles[at].get_or_insert(role);
    copies.insert(b, at);
    at
}

/// A car's bank, decoded, and what its sound keeps between steps.
pub struct FmodCar {
    pub slots: Vec<Slot>,
    /// The events the bank has, with instruments or not.
    present: Vec<Role>,
    /// Each slot's recordings, indices into [`FmodCar::waves`].
    pub samples: Vec<Vec<usize>>,
    pub waves: Vec<Arc<Wave>>,
    /// The bank's parameter names.
    params: Vec<String>,
    /// The bank's buses, then the wheel events' own buses for the second to fourth wheel.
    pub buses: Vec<Bus>,
    /// Each bus's event, whose parameters its automations take (`None`: the mixer's groups and returns).
    bus_roles: Vec<Option<Role>>,
    /// The bus gains and effect parameters last sent, by (bus, effect, parameter) (`u16::MAX`: the gain).
    sent: HashMap<(u16, u16, u16), f32>,
    /// The one-shot slots' gains and pitches last sent ([`Command::FmodTrack`]).
    tracked: HashMap<u16, (f32, f32)>,
    turbo_flow: f32,
    /// Each slot's parameters were in its regions at the last step it was not paused (the one-shots on regions).
    in_regions: Vec<bool>,
    /// `battery` as last set.
    battery: f32,
    /// The daemon's `fluctuation` of [`FLUCTUATING`] (`+0x2c`, `+0x58`, `+0x1b8`) and their events'
    /// `fluctuation_length` (0 without the event or the property).
    fluctuation: [f32; 3],
    fluctuation_length: [f32; 3],
    /// The daemon's last gear (`+0x2cc`).
    last_gear: i32,
    /// Seconds since NGP last sent the daemon its block.
    quiet: f32,
    /// The gains that bring the bank's engine to [`LOUDNESS`], in the car and outside (ours).
    trims: [f32; 2],
    rng: Rng,
}

impl FmodCar {
    /// The bank at `path` (`AudioFMOD/<name>.bank`, its strings bank beside it), its recordings decoded.
    pub fn load(path: &Path) -> Result<FmodCar, String> {
        let read = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
        let strings = path.with_extension("strings.bank");
        let bank = fmod::parse(read(path)?, &read(&strings)?).map_err(|e| format!("{}: {e}", path.display()))?;
        FmodCar::new(&bank)
    }

    pub fn new(bank: &Bank) -> Result<FmodCar, String> {
        let mut buses = bank.buses.clone();
        let mut bus_roles = vec![None; buses.len()];
        let mut slots = Vec::new();
        for &(name, role) in &EVENTS {
            let Some(event) = bank.event(name) else { continue };
            let instances = if role == Role::Wheel(0) { WHEELS } else { 1 };
            for k in 0..instances {
                let role = if instances > 1 { Role::Wheel(k) } else { role };
                let mut copies = HashMap::new();
                for l in &event.layers {
                    let bus = l.bus.map(|b| instance_bus(&mut buses, &mut bus_roles, &mut copies, b, role, k > 0));
                    slots.push(Slot { role, layer: Layer { bus, ..l.clone() } });
                }
            }
        }
        if !slots.iter().any(|s| matches!(s.role, Role::Engine(_))) {
            return Err("no engine event".to_owned());
        }
        // Each recording once, decoded on a few threads.
        let mut used: Vec<usize> = slots.iter().flat_map(|s| s.layer.samples.iter().copied()).collect();
        used.sort_unstable();
        used.dedup();
        let decoded: Vec<Result<Wave, String>> = std::thread::scope(|scope| {
            let jobs: Vec<_> = used
                .chunks(used.len().div_ceil(4).max(1))
                .map(|chunk| {
                    scope.spawn(move || chunk.iter().map(|&k| decode(bank, &bank.samples[k])).collect::<Vec<_>>())
                })
                .collect();
            jobs.into_iter().flat_map(|j| j.join().expect("a decode thread panicked")).collect()
        });
        let mut index = HashMap::new();
        let mut waves = Vec::new();
        for (&k, wave) in used.iter().zip(decoded) {
            let wave = wave.map_err(|e| format!("{}: {e}", bank.samples[k].name))?;
            index.insert(k, waves.len());
            waves.push(Arc::new(wave));
        }
        let samples = slots.iter().map(|s| s.layer.samples.iter().map(|k| index[k]).collect()).collect();
        let event = |role| EVENTS.iter().find(|e| e.1 == role).and_then(|e| bank.event(e.0));
        let mut car = FmodCar {
            present: EVENTS.iter().filter(|e| bank.event(e.0).is_some()).map(|e| e.1).collect(),
            in_regions: vec![false; slots.len()],
            battery: 0.0,
            fluctuation: [0.0; 3],
            fluctuation_length: FLUCTUATING.map(|r| event(r).and_then(|e| e.fluctuation_length).unwrap_or(0.0)),
            last_gear: START_GEAR,
            quiet: 0.0,
            params: bank.params.iter().map(|p| p.name.clone()).collect(),
            buses,
            bus_roles,
            sent: HashMap::new(),
            tracked: HashMap::new(),
            slots,
            samples,
            waves,
            turbo_flow: 0.0,
            trims: [1.0; 2],
            rng: Rng::new(0xf30d),
        };
        // Measured three times, each at the last trim, as the bank's compressors then work.
        car.trims = [false, true].map(|outside| {
            let target = LOUDNESS[usize::from(outside)];
            let mut trim = 1.0f32;
            for _ in 0..3 {
                let loudness = crate::mixer::fmod_loudness(&car, outside, trim);
                if loudness <= 0.0 {
                    return 1.0;
                }
                trim = (trim * target / loudness).clamp(1.0 / MAX_TRIM, MAX_TRIM);
            }
            trim
        });
        Ok(car)
    }

    /// The commands that set the engine event in the car or outside going at `params` with every bus as they give
    /// it, untrimmed: what [`crate::mixer::fmod_loudness`] measures the bank's level with.
    pub(crate) fn engine_at(&self, params: &Params, outside: bool) -> Vec<Command> {
        let value = |k: usize, role: Option<Role>| self.params.get(k).and_then(|n| params.by_name(n, role));
        let role = Role::Engine(outside);
        let loops = self.slots.iter().enumerate().filter(|(_, s)| s.role == role).map(|(k, s)| {
            let (gain, pitch) = play(&s.layer, |p| value(p, Some(role))).unwrap_or((0.0, 1.0));
            Command::FmodLoop { slot: k as u16, gain, pitch, outside }
        });
        loops.chain(self.bus_settings(value).into_iter().map(|(_, _, c)| c)).collect()
    }

    /// Each bus's gain and automated effect parameters at `value`, keyed by (bus, effect, parameter) (`u16::MAX`:
    /// the gain).
    fn bus_settings(&self, value: impl Fn(usize, Option<Role>) -> Option<f32>) -> Vec<((u16, u16, u16), f32, Command)> {
        let mut out = Vec::new();
        for (b, bus) in self.buses.iter().enumerate() {
            let role = self.bus_roles[b];
            let mut db = bus.volume_db;
            for a in &bus.automations {
                db += value(a.param, role).map_or(0.0, |v| interpolate(&a.points, v));
            }
            let b = b as u16;
            let gain = 10f32.powf(db / 20.0);
            out.push(((b, u16::MAX, 0), gain, Command::FmodBus { bus: b, gain }));
            for (e, effect) in bus.pre.iter().chain(&bus.post).enumerate() {
                for a in &effect.automations {
                    let (Property::Effect(param), Some(v)) = (a.property, value(a.param, role)) else { continue };
                    let value = interpolate(&a.points, v);
                    let (effect, param) = (e as u8, param as u8);
                    out.push((
                        (b, e as u16, param.into()),
                        value,
                        Command::FmodEffect { bus: b, effect, param, value },
                    ));
                }
            }
        }
        out
    }

    /// The in-car and outside gains toward [`LOUDNESS`].
    pub fn trims(&self) -> [f32; 2] {
        self.trims
    }

    /// An event's gain toward [`LOUDNESS`]: the in-car or outside trim, the events both play the two's mean (ours).
    fn trim(&self, role: Role) -> f32 {
        match role {
            Role::Engine(o) | Role::Gear(o) | Role::BlowOff(o) | Role::Backfire(o) => self.trims[usize::from(o)],
            _ => (self.trims[0] * self.trims[1]).sqrt(),
        }
    }

    /// Each slot's loop wave (its first recording) and bus, for the mixer.
    pub fn loop_waves(&self) -> Vec<(Arc<Wave>, Option<usize>)> {
        self.samples.iter().zip(&self.slots).map(|(s, slot)| (self.waves[s[0]].clone(), slot.layer.bus)).collect()
    }

    /// Whether `command`, a 1.02 car voice, is one the bank plays instead (the engine always; the turbo when the bank
    /// has its instruments; the voices NGP turns down are left to [`FmodCar::mute`] and [`FmodCar::gear_click`]).
    pub fn replaces(&self, command: &Command) -> bool {
        use crate::voice::Loop;
        match command {
            Command::Gain { voice: Loop::Engine, .. } | Command::Reflections { .. } => true,
            Command::Loop { voice: Loop::Turbo, .. } => self.slots.iter().any(|s| s.role == Role::Turbo),
            _ => false,
        }
    }

    /// The 1.02 gear-change click's threshold and volume NGP gives it, when the bank has `gear_int` ([`GEAR_CLICK`]).
    pub fn gear_click(&self) -> Option<(f32, f32)> {
        self.present.contains(&Role::Gear(false)).then_some(GEAR_CLICK)
    }

    /// Turns `command`, a 1.02 car voice, down to [`MUTED`] when NGP does: the bank has the event it stands in for.
    pub fn mute(&self, command: &mut Command) {
        use crate::voice::{Loop, Shot};
        let has = |role| self.present.contains(&role);
        let gain = match command {
            Command::Effect { shot: Shot::ExhaustPop(_) | Shot::Pop(_), gain, .. } if has(Role::Backfire(false)) => {
                gain
            }
            Command::Effect { shot: Shot::Bang(_), gain, .. } if has(Role::Backfire(true)) => gain,
            Command::Effect { shot: Shot::Ignition, gain, .. } if has(Role::Ignition) => gain,
            Command::Effect { shot: Shot::BlowOff, gain, .. } if has(Role::BlowOff(false)) => gain,
            Command::Loop { voice: Loop::Gearbox | Loop::Diff, gain, .. } if has(Role::Transmission) => gain,
            Command::Gain { voice: Loop::Wind, gain } | Command::Loop { voice: Loop::Wind, gain, .. }
                if has(Role::Wind) =>
            {
                gain
            }
            _ => return,
        };
        *gain *= MUTED;
    }

    /// The daemon's gear and `fluctuation` on an update with the block's gear `gear` (0x40950d-0x409864): the gear it
    /// sends and the `state`.
    fn shift(&mut self, gear: i32, dt: f32) -> (i32, f32) {
        let mut out = (gear, 0.0);
        let mut shifted = false;
        if gear != self.last_gear {
            let base = if gear > self.last_gear { STATE_UP } else { STATE_DOWN };
            if self.fluctuation.iter().any(|&f| f > 0.0) {
                // A fluctuation still runs: the shift waits, with them halved, and the old gear is sent.
                self.fluctuation = self.fluctuation.map(|f| f * 0.5);
                out.0 = self.last_gear;
            } else {
                self.last_gear = gear;
                out.1 = (self.rng.rand() % 100) as f32 / 100.0 * STATE_SPAN + base;
                shifted = true;
            }
        }
        for (f, &length) in self.fluctuation.iter_mut().zip(&self.fluctuation_length) {
            *f = if length <= 0.0 {
                0.0
            } else if shifted {
                1.0
            } else {
                (*f - dt / length).max(0.0)
            };
        }
        out
    }

    /// The commands for one step's inputs, the engine stopped (`CCarSound +0x41924`) or not, NGP sending the daemon
    /// its block this step (`sent`: app states 1 and 8) or not: every loop slot's gain and pitch, the one-shots that
    /// start and those that follow their parameters.
    pub fn step(&mut self, inputs: &Inputs, scene: &Scene, stopped: bool, sent: bool) -> Vec<Command> {
        let sound = &inputs.sound;
        self.quiet = if sent { 0.0 } else { self.quiet + rbr_recording::STEP_SECONDS };
        let held = self.quiet > WATCHDOG;
        // The start orbit's first 0.2 s: command 2 started every event unpaused with `rpms` the car's idle, `gear`
        // and `state` 0, and set nothing else (0x408927-0x4089fb); no 3D attributes.
        let orbit = !sent && !held;
        let mut params = if orbit {
            let rpm = inputs.ngp.idle_rpm;
            Params { rpm, gear: Some(0.0), ..Params::default() }
        } else {
            Params::new(sound, inputs.block.as_ref(), scene)
        };
        let starter = sound.drivetrain_f32(9);
        let startup = stopped && starter > 0.0;
        if startup {
            self.battery = starter;
        }
        params.battery = self.battery;
        params.startup = f32::from(u8::from(startup && !orbit));
        let flow = sound.engine_f32(6);
        // As the 1.02 valve (`crate::car`): a rise past twice the last step's flow.
        let blow_off = self.turbo_flow * BLOW_OFF_RISE < flow;
        self.turbo_flow = if blow_off { flow.min(BLOW_OFF_CAP) } else { flow };
        if !orbit {
            params.valve = blow_off_volume(flow);
        }
        if let Some(block) = inputs.block.filter(|_| sent) {
            let (gear, state) = self.shift(block.gear, rbr_recording::STEP_SECONDS);
            (params.gear, params.state) = (Some((gear - 1) as f32), state);
        }
        params.fluctuation = self.fluctuation;
        let outside = !orbit && !scene.inside;
        // The daemon's pauses (`FUN_00408ef0`).
        let paused = |role| {
            !orbit
                && (held
                    || match role {
                        Role::Engine(o) | Role::BlowOff(o) | Role::Backfire(o) => stopped || o != outside,
                        Role::Turbo => stopped,
                        Role::Gear(o) => o != outside,
                        Role::Wind => outside,
                        Role::Transmission | Role::Ignition | Role::Wheel(_) => false,
                    })
        };
        let names = &self.params;
        let value = |k: usize, role: Option<Role>| names.get(k).and_then(|n| params.by_name(n, role));
        let mut out = Vec::new();
        let mut valves = Vec::new();
        for (k, slot) in self.slots.iter().enumerate() {
            let role = slot.role;
            let mix = MIX * self.trim(role);
            let at = |p| value(p, Some(role));
            let state = play(&slot.layer, at);
            let slot_id = k as u16;
            let bus = slot.layer.bus.map(|b| b as u16);
            match role {
                Role::Engine(_) | Role::Turbo | Role::Wind | Role::Transmission | Role::Wheel(_) => {
                    let (gain, pitch) = state.filter(|_| !paused(role)).unwrap_or((0.0, 1.0));
                    out.push(Command::FmodLoop { slot: slot_id, gain: gain * mix, pitch, outside });
                    continue;
                }
                Role::Backfire(_) | Role::Ignition | Role::Gear(_) if !paused(role) => {
                    // A gear region holds its start, not its end: the daemon's gears are whole numbers, and the
                    // banks' gear regions follow on from each other (`-1..0`, `0..1`, ...).
                    let gear_end = |r: &fmod::Region| {
                        names.get(r.param).is_some_and(|n| n == "gear") && at(r.param).is_some_and(|v| v >= r.end)
                    };
                    let state = state.filter(|_| !slot.layer.regions.iter().any(gear_end));
                    if let Some((gain, pitch)) = state.filter(|&(g, _)| g > 0.0 && !self.in_regions[k]) {
                        let list = &self.samples[k];
                        let wave = list[self.rng.rand() as usize % list.len()] as u16;
                        out.push(Command::FmodShot {
                            wave,
                            gain: gain * mix,
                            pitch,
                            outside,
                            bus,
                            slot: Some(slot_id),
                        });
                    }
                    self.in_regions[k] = state.is_some();
                }
                Role::BlowOff(_) if !paused(role) => {
                    if blow_off {
                        valves.extend(state.map(|(g, p)| (k, g, p)));
                    }
                }
                _ => continue,
            }
            // A playing one-shot follows its automations.
            let (gain, pitch) = level(&slot.layer, at);
            if self.tracked.insert(slot_id, (gain, pitch)) != Some((gain, pitch)) {
                out.push(Command::FmodTrack { slot: slot_id, gain: gain * mix, pitch, outside });
            }
        }
        if !valves.is_empty() {
            let (k, gain, pitch) = valves[self.rng.rand() as usize % valves.len()];
            let list = &self.samples[k];
            let wave = list[self.rng.rand() as usize % list.len()] as u16;
            let bus = self.slots[k].layer.bus.map(|b| b as u16);
            let gain = gain * MIX * self.trim(self.slots[k].role);
            out.push(Command::FmodShot { wave, gain, pitch, outside, bus, slot: Some(k as u16) });
        }
        // The buses' gains and effect parameters, where they changed.
        for (key, v, command) in self.bus_settings(value) {
            if self.sent.insert(key, v) != Some(v) {
                out.push(command);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Scene;
    use rbr_formats::fmod::{AutoPitch, Automation, Region};
    use rbr_recording::sound::NgpSound;

    fn layer() -> Layer {
        Layer {
            samples: vec![0],
            regions: vec![Region { param: 0, start: 1500.0, end: 6000.0 }],
            auto_pitch: Some(AutoPitch { param: 0, reference: 4000.0, min: 0.0 }),
            volume_db: -6.0,
            semitones: 0.0,
            automations: vec![
                Automation { param: 0, property: Property::Gain, points: vec![[1500.0, 0.0], [3000.0, 1.0]] },
                Automation { param: 1, property: Property::VolumeDb, points: vec![[0.0, -20.0], [1.0, 0.0]] },
            ],
            bus: None,
        }
    }

    #[test]
    fn stereo_pcm_is_mixed_down_to_mono() {
        let mut bytes = Vec::new();
        for s in [1000i16, 3000, -1000, -3000] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let sample = Sample {
            name: "s".into(),
            rate: 44100,
            channels: 2,
            frames: 2,
            vorbis_crc: None,
            codec: fmod::PCM16,
            data: 0..8,
        };
        let bank =
            Bank { bytes, samples: vec![sample.clone()], params: Vec::new(), events: Vec::new(), buses: Vec::new() };
        assert_eq!(decode(&bank, &sample).unwrap(), Wave { rate: 44100, samples: vec![2000.0, -2000.0] });
    }

    #[test]
    fn a_layer_plays_in_its_region_with_its_fades_and_auto_pitch() {
        let at = |rpm: f32, throttle: Option<f32>| play(&layer(), |k| if k == 0 { Some(rpm) } else { throttle });
        assert_eq!(at(1000.0, Some(1.0)), None);
        assert_eq!(at(7000.0, Some(1.0)), None);
        let (gain, pitch) = at(4000.0, Some(1.0)).unwrap();
        assert!((gain - 10f32.powf(-6.0 / 20.0)).abs() < 1e-6);
        assert!((pitch - 1.0).abs() < 1e-6);
        let (gain, pitch) = at(2250.0, Some(0.0)).unwrap();
        assert!((gain - 0.5 * 10f32.powf(-26.0 / 20.0)).abs() < 1e-6);
        assert!((pitch - 2250.0 / 4000.0).abs() < 1e-6);
        // An unknown parameter is left out.
        assert!((at(4000.0, None).unwrap().0 - 10f32.powf(-6.0 / 20.0)).abs() < 1e-6);
    }

    /// A car of one-sample slots over parameters `params`, with the events `present`.
    fn car(slots: Vec<Slot>, params: &[&str], present: Vec<Role>) -> FmodCar {
        let n = slots.len();
        FmodCar {
            samples: (0..n).map(|k| vec![k]).collect(),
            waves: (0..n).map(|_| Arc::new(Wave { rate: 48000, samples: vec![0.0; 16] })).collect(),
            slots,
            present,
            params: params.iter().map(|p| p.to_string()).collect(),
            buses: Vec::new(),
            bus_roles: Vec::new(),
            sent: HashMap::new(),
            tracked: HashMap::new(),
            turbo_flow: 0.0,
            in_regions: vec![false; n],
            battery: 0.0,
            fluctuation: [0.0; 3],
            fluctuation_length: [0.0; 3],
            last_gear: START_GEAR,
            quiet: 0.0,
            trims: [1.0; 2],
            rng: Rng::new(1),
        }
    }

    fn slot(role: Role, regions: Vec<Region>) -> Slot {
        let layer = Layer { regions, auto_pitch: None, volume_db: 0.0, automations: Vec::new(), ..layer() };
        Slot { role, layer }
    }

    fn shots(commands: &[Command]) -> usize {
        commands.iter().filter(|c| matches!(c, Command::FmodShot { .. })).count()
    }

    fn sound(misfire: f32, starter: f32, speed: f32) -> Inputs {
        let mut s = Sound { speed, ..Sound::default() };
        s.engine[4] = misfire.to_bits();
        s.drivetrain[9] = starter.to_bits();
        Inputs { sound: s, ..Inputs::default() }
    }

    #[test]
    fn the_ignition_plays_once_as_the_starter_turns_a_stopped_engine() {
        let regions = vec![Region { param: 0, start: 1.0, end: 1.0 }, Region { param: 1, start: 0.009, end: 1.0 }];
        let mut car = car(vec![slot(Role::Ignition, regions)], &["startup", "battery"], vec![Role::Ignition]);
        let scene = Scene::default();
        let crank = sound(0.0, 0.9, 0.0);
        assert_eq!(shots(&car.step(&crank, &scene, true, true)), 1);
        assert_eq!(shots(&car.step(&crank, &scene, true, true)), 0, "once per crank");
        // Running: `startup` 0 leaves the region; a stall and a new crank play it again.
        assert_eq!(shots(&car.step(&crank, &scene, false, true)), 0);
        assert_eq!(shots(&car.step(&sound(0.0, 0.0, 0.0), &scene, true, true)), 0, "no starter, no ignition");
        assert_eq!(shots(&car.step(&crank, &scene, true, true)), 1);
    }

    #[test]
    fn a_backfire_plays_on_a_misfire_past_0_9_in_its_view_with_the_engine_running() {
        let regions = vec![Region { param: 0, start: 0.9, end: 1.0 }];
        let slots = vec![slot(Role::Backfire(false), regions.clone()), slot(Role::Backfire(true), regions)];
        let mut car = car(slots, &["exhaustpop"], vec![Role::Backfire(false), Role::Backfire(true)]);
        let inside = Scene::default();
        let (pop, calm) = (sound(0.95, 0.0, 20.0), sound(0.85, 0.0, 20.0));
        let step = |car: &mut FmodCar, s: &Inputs, stopped| car.step(s, &inside, stopped, true);
        let played = step(&mut car, &pop, false);
        assert!(matches!(played[..], [Command::FmodShot { wave: 0, outside: false, .. }, ..]), "{played:?}");
        assert_eq!(shots(&played), 1, "only the in-car event");
        assert_eq!(shots(&step(&mut car, &pop, false)), 0);
        // Under 0.9 the daemon sends 0.
        assert_eq!(shots(&step(&mut car, &calm, false)), 0);
        assert_eq!(shots(&step(&mut car, &calm, true)), 0);
        assert_eq!(shots(&step(&mut car, &pop, true)), 0, "paused while the engine is stopped");
        assert_eq!(shots(&step(&mut car, &pop, false)), 1);
    }

    #[test]
    fn the_wind_plays_in_the_car_only() {
        let slots = vec![slot(Role::Wind, vec![Region { param: 0, start: 0.0, end: 400.0 }])];
        let mut car = car(slots, &["car_speed"], vec![Role::Wind]);
        let gain = |commands: Vec<Command>| match commands[..] {
            [Command::FmodLoop { gain, .. }] => gain,
            _ => panic!("{commands:?}"),
        };
        let fast = sound(0.0, 0.0, 30.0);
        assert!(gain(car.step(&fast, &Scene::default(), false, true)) > 0.0);
        assert_eq!(gain(car.step(&fast, &Scene { inside: false, ..Scene::default() }, false, true)), 0.0);
    }

    #[test]
    fn the_daemon_pauses_every_event_after_200_ms_without_ngp_s_block() {
        let slots = vec![slot(Role::Transmission, vec![Region { param: 0, start: 0.0, end: 400.0 }])];
        let mut car = car(slots, &["wheel_speed"], vec![Role::Transmission]);
        let mut gain = |sent| match car.step(&sound(0.0, 0.0, 30.0), &Scene::default(), false, sent)[..] {
            [Command::FmodLoop { gain, .. }] => gain,
            ref other => panic!("{other:?}"),
        };
        // 7 ms steps: 28 are 196 ms.
        assert!((0..28).all(|_| gain(false) > 0.0));
        assert_eq!(gain(false), 0.0);
        assert!(gain(true) > 0.0);
    }

    #[test]
    fn ngp_turns_down_the_1_02_voices_of_the_events_the_bank_has() {
        use crate::voice::{Loop, Repeat, Shot};
        let car = car(Vec::new(), &[], vec![Role::Backfire(false), Role::Wind, Role::BlowOff(false)]);
        let effect = |shot| Command::Effect { shot, gain: 1.0, pitch: 1.0, repeat: Repeat::Restart };
        let gain = |mut c: Command| {
            car.mute(&mut c);
            match c {
                Command::Effect { gain, .. } | Command::Loop { gain, .. } => gain,
                _ => unreachable!(),
            }
        };
        assert_eq!(gain(effect(Shot::Pop(0))), MUTED);
        assert_eq!(gain(effect(Shot::ExhaustPop(3))), MUTED);
        assert_eq!(gain(effect(Shot::Bang(0))), 1.0, "no backfire_ext");
        assert_eq!(gain(effect(Shot::Ignition)), 1.0);
        assert_eq!(gain(Command::Loop { voice: Loop::Wind, gain: 1.0, pitch: 1.0 }), MUTED);
        // An event with no instruments (Fiesta_Rally2's blowoff_int) still turns its voice down, and stays a voice.
        let valve = effect(Shot::BlowOff);
        assert!(!car.replaces(&valve));
        assert_eq!(gain(valve), MUTED);
        assert_eq!(gain(Command::Loop { voice: Loop::Gearbox, gain: 1.0, pitch: 1.0 }), 1.0, "no transmission");
        let car = self::car(Vec::new(), &[], vec![Role::BlowOff(true), Role::Transmission]);
        let mut valve = effect(Shot::BlowOff);
        car.mute(&mut valve);
        assert!(matches!(valve, Command::Effect { gain: 1.0, .. }), "blowoff_ext alone leaves the valve");
        for voice in [Loop::Gearbox, Loop::Diff] {
            let mut whine = Command::Loop { voice, gain: 1.0, pitch: 1.0 };
            assert!(!car.replaces(&whine));
            car.mute(&mut whine);
            assert!(matches!(whine, Command::Loop { gain: MUTED, .. }));
        }
    }

    fn region(param: usize, start: f32, end: f32) -> Region {
        Region { param, start, end }
    }

    /// Inputs whose FMB1 block has the drive state's gear `gear`.
    fn in_gear(gear: i32) -> Inputs {
        Inputs { block: Some(FmodBlock { gear, ..FmodBlock::default() }), ..Inputs::default() }
    }

    #[test]
    fn a_shift_plays_the_gear_event_s_instrument_for_its_gear_and_direction() {
        // Shifting up into first (`gear` 1): "1..2" plays, the "0..1" before it does not; down is `state` 0.1-0.5.
        let slots = vec![
            slot(Role::Gear(false), vec![region(0, 1.0, 2.0), region(1, 0.5, 1.0)]),
            slot(Role::Gear(false), vec![region(0, 0.0, 1.0), region(1, 0.5, 1.0)]),
            slot(Role::Gear(false), vec![region(0, 0.0, 1.0), region(1, 0.1, 0.5)]),
        ];
        let mut car = car(slots, &["gear", "state"], vec![Role::Gear(false)]);
        let started = |commands: &[Command]| -> Vec<u16> {
            commands
                .iter()
                .filter_map(|c| match c {
                    Command::FmodShot { slot, .. } => *slot,
                    _ => None,
                })
                .collect()
        };
        let scene = Scene::default();
        assert_eq!(started(&car.step(&in_gear(1), &scene, false, true)), [] as [u16; 0], "neutral: no shift");
        assert_eq!(started(&car.step(&in_gear(2), &scene, false, true)), [0]);
        assert_eq!(started(&car.step(&in_gear(2), &scene, false, true)), [] as [u16; 0], "state is 0 again");
        assert_eq!(started(&car.step(&in_gear(1), &scene, false, true)), [2]);
        // Outside the car the in-car event is paused.
        let outside = Scene { inside: false, ..Scene::default() };
        assert_eq!(started(&car.step(&in_gear(2), &outside, false, true)), [] as [u16; 0]);
    }

    #[test]
    fn a_shift_sets_the_fluctuation_which_falls_over_its_length_and_holds_the_next_shift() {
        let slots = vec![
            slot(Role::Engine(false), vec![region(0, 0.5, 1.0)]),
            slot(Role::Gear(false), vec![region(1, 0.5, 1.0)]),
        ];
        let mut car = car(slots, &["fluctuation", "state"], vec![Role::Engine(false), Role::Gear(false)]);
        car.fluctuation_length = [0.5, 0.0, 0.8];
        let scene = Scene::default();
        let mut step = |gear| {
            let out = car.step(&in_gear(gear), &scene, false, true);
            let engine = out.iter().find_map(|c| match *c {
                Command::FmodLoop { slot: 0, gain, .. } => Some(gain),
                _ => None,
            });
            (engine.unwrap() > 0.0, shots(&out))
        };
        assert_eq!(step(2), (true, 1), "the shift: fluctuation 1");
        // 0.5 s from 1 to 0: under 0.5 after 0.25 s (36 steps of 7 ms).
        assert!((0..34).all(|_| step(2) == (true, 0)));
        assert!((0..4).any(|_| step(2) == (false, 0)));
        // A shift while one still runs waits, the old gear sent and the fluctuations halved each update.
        let mut car = self::car(Vec::new(), &[], Vec::new());
        car.fluctuation_length = [0.5, 0.0, 0.8];
        let (gear, state) = car.shift(2, 0.007);
        assert!(gear == 2 && (0.6..=1.0).contains(&state), "{gear} {state}");
        assert_eq!(car.fluctuation, [1.0, 0.0, 1.0]);
        assert_eq!(car.shift(1, 0.007), (2, 0.0));
        assert!(car.fluctuation[0] < 0.5 && car.fluctuation[2] < 0.5);
        let waited = (0..20).position(|_| car.shift(1, 0.007).1 > 0.0).unwrap();
        assert!(waited < 8 && car.last_gear == 1, "{waited}");
    }

    #[test]
    fn each_wheel_event_has_its_own_tracks_and_wheel_speed() {
        let bus = |name: &str, parent| Bus {
            name: name.to_owned(),
            parent,
            volume_db: 0.0,
            automations: Vec::new(),
            pre: Vec::new(),
            post: Vec::new(),
        };
        let mut buses = vec![bus("bus:/", None), bus("bus:/common/wheel", Some(0)), bus("", Some(1)), bus("", Some(2))];
        let mut roles = vec![None; 4];
        let first = instance_bus(&mut buses, &mut roles, &mut HashMap::new(), 3, Role::Wheel(0), false);
        let second = instance_bus(&mut buses, &mut roles, &mut HashMap::new(), 3, Role::Wheel(1), true);
        assert_eq!((first, second), (3, 5));
        assert_eq!(
            buses.iter().map(|b| b.parent).collect::<Vec<_>>(),
            [None, Some(0), Some(1), Some(2), Some(1), Some(4)]
        );
        assert_eq!(
            roles,
            [None, None, Some(Role::Wheel(0)), Some(Role::Wheel(0)), Some(Role::Wheel(1)), Some(Role::Wheel(1))]
        );
        let block = FmodBlock {
            brake: 0.5,
            transmission_speed: -10.0,
            wheel_speeds: [10.0, 0.2, 0.0, 0.0],
            ..FmodBlock::default()
        };
        let sound = Sound { speed: 20.0, ..Sound::default() };
        let params = Params::new(&sound, Some(&block), &Scene::default());
        let at = |name, role| params.by_name(name, role);
        assert_eq!(at("wheel_speed", Some(Role::Wheel(0))), Some(36.0));
        assert_eq!(at("wheel_speed", Some(Role::Wheel(1))), Some(0.0), "under 1 km/h");
        assert_eq!(at("wheel_speed", Some(Role::Transmission)), Some(36.0));
        assert_eq!(at("wheel_speed", Some(Role::Engine(false))), Some(72.0));
        assert_eq!((at("brake", Some(Role::Wheel(2))), at("brake", Some(Role::Transmission))), (Some(0.5), Some(0.0)));
    }

    #[test]
    fn with_gear_int_ngp_moves_the_1_02_gear_click_out_of_reach() {
        let car = car(Vec::new(), &[], vec![Role::Gear(false)]);
        assert_eq!(car.gear_click(), Some(GEAR_CLICK));
        assert_eq!(self::car(Vec::new(), &[], vec![Role::Gear(true)]).gear_click(), None);
        let mut sound = Sound::default();
        sound.drivetrain[8] = 0.5f32.to_bits();
        let clicks = |gear_click| {
            let mut car = crate::car::Car::default();
            car.gear_click = gear_click;
            let out = car.step(&sound, true, &NgpSound::default(), false);
            out.iter().filter(|c| matches!(c, Command::Shot { shot: crate::voice::Shot::GearChange, .. })).count()
        };
        assert_eq!((clicks(None), clicks(Some(GEAR_CLICK))), (1, 0));
    }

    #[test]
    fn the_orbit_s_first_200_ms_play_every_event_at_the_idle_rpm() {
        let idle = vec![region(0, 900.0, 1100.0)];
        let slots = vec![slot(Role::Engine(false), idle.clone()), slot(Role::Engine(true), idle)];
        let mut car = car(slots, &["rpms"], vec![Role::Engine(false), Role::Engine(true)]);
        let mut inputs = sound(0.0, 0.0, 0.0);
        inputs.sound.engine[0] = (5000.0 / RPM_PER_RAD_S).to_bits();
        inputs.ngp = NgpSound { gear_load_cap: 600.0, diff_load_cap: 6000.0, min_rpm: 750.0, idle_rpm: 1000.0 };
        let mut gains = |sent| -> Vec<f32> {
            car.step(&inputs, &Scene::default(), false, sent)
                .iter()
                .filter_map(|c| match *c {
                    Command::FmodLoop { gain, .. } => Some(gain),
                    _ => None,
                })
                .collect()
        };
        assert!(gains(false).iter().all(|&g| g > 0.0), "both, at the idle");
        assert!((0..40).any(|_| gains(false).iter().all(|&g| g == 0.0)), "then paused");
        assert!(gains(true).iter().all(|&g| g == 0.0), "the block's 5000 rpm");
    }

    #[test]
    fn a_playing_one_shot_follows_its_automations() {
        let mut backfire = slot(Role::Backfire(false), vec![region(0, 0.9, 1.0)]);
        backfire.layer.automations =
            vec![Automation { param: 1, property: Property::Gain, points: vec![[0.0, 0.0], [1.0, 1.0]] }];
        let mut car = car(vec![backfire], &["exhaustpop", "throttle"], vec![Role::Backfire(false)]);
        let mut pop = sound(0.95, 0.0, 20.0);
        let tracked = |commands: &[Command]| {
            commands.iter().find_map(|c| match *c {
                Command::FmodTrack { slot: 0, gain, .. } => Some(gain),
                _ => None,
            })
        };
        pop.sound.engine[1] = 0.5f32.to_bits();
        let out = car.step(&pop, &Scene::default(), false, true);
        assert!(matches!(out[..], [Command::FmodShot { slot: Some(0), gain: 0.5, .. }, ..]), "{out:?}");
        assert_eq!(tracked(&car.step(&pop, &Scene::default(), false, true)), None, "unchanged");
        pop.sound.engine[1] = 0.25f32.to_bits();
        assert_eq!(tracked(&car.step(&pop, &Scene::default(), false, true)), Some(0.25));
    }

    /// A bank of `RBR_RSF` decodes, and its engine plays something at idle and at 6000 rpm on and off throttle.
    #[test]
    fn an_rsf_bank_decodes_and_its_engine_plays_over_the_rev_range() {
        let Some(root) = std::env::var_os("RBR_RSF") else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let car = FmodCar::load(&Path::new(&root).join("AudioFMOD").join("Lancer_Evo_IX_X.bank")).unwrap();
        assert!(car.waves.iter().all(|w| w.rate == 48000 && !w.samples.is_empty()));
        let peak = car.waves.iter().flat_map(|w| w.samples.iter()).fold(0f32, |m, s| m.max(s.abs()));
        assert!((1000.0..=32768.0).contains(&peak), "peak {peak}");
        // Engine recordings, not noise: few zero crossings (a decode with the wrong codebooks is noise, about 0.5).
        for w in &car.waves {
            let crossings = w.samples.windows(2).filter(|p| (p[0] < 0.0) != (p[1] < 0.0)).count();
            assert!((crossings as f32) < 0.3 * w.samples.len() as f32, "{crossings} of {}", w.samples.len());
        }
        for outside in [false, true] {
            for (rpm, throttle) in [(900.0, 0.0), (6000.0, 1.0), (6000.0, 0.0)] {
                let params = Params { rpm, throttle, distance: 6.0, cone_angle: 0.0, ..Params::default() };
                let loud = car
                    .slots
                    .iter()
                    .filter(|s| s.role == Role::Engine(outside))
                    .filter_map(|s| play(&s.layer, |k| params.by_name(&car.params[k], Some(s.role))))
                    .map(|(g, _)| g)
                    .fold(0f32, f32::max);
                assert!(loud > 0.05, "outside {outside} at {rpm} rpm, throttle {throttle}: loudest {loud}");
            }
            // Its trim brings the engine to the target.
            let (trim, target) = (car.trims[usize::from(outside)], LOUDNESS[usize::from(outside)]);
            let db = 20.0 * (crate::mixer::fmod_loudness(&car, outside, trim) / target).log10();
            assert!(db.abs() < 1.0, "outside {outside}: trim {trim}, {db} dB off the target");
        }
    }

    #[test]
    fn a_zero_sample_rate_bank_sample_is_rejected() {
        let bank =
            Bank { bytes: Vec::new(), samples: Vec::new(), params: Vec::new(), events: Vec::new(), buses: Vec::new() };
        let sample = rbr_formats::fmod::Sample {
            name: "x".to_owned(),
            rate: 0,
            channels: 1,
            frames: 0,
            vorbis_crc: None,
            codec: fmod::PCM16,
            data: 0..0,
        };
        assert!(decode(&bank, &sample).is_err());
    }
}

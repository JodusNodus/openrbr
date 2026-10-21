//! The car's sound data from the retail files (`notes/formats.md` "Audio"): the engine bank named in `Cars.ini`, the
//! effect samples of `Effects.ini`, the impact samples of `Impact/Sound.ini`, the co-driver's English words and
//! numbers (`CoDriver.ini`, `Numbers.ini`) and damage calls, the filter settings and the gains from `audio.ini` and
//! `Ingame.ini`.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use rbr_formats::audio::{self, Amps, AudioDat, CarSound, SoundDef};
use rbr_formats::data::Data;
use rbr_formats::eng;

use crate::car::Gains;
use crate::effects::EffectGains;
use crate::scripted::ScriptedGains;
use crate::world::WorldGains;

/// A mono sample as floats in the 16-bit range, with its rate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wave {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Wave {
    /// A 16-bit PCM WAV (every retail one is mono; a stereo one is mixed down).
    pub fn decode(bytes: &[u8]) -> Result<Wave, String> {
        let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
            return Err(format!("{} bits, {:?}", spec.bits_per_sample, spec.sample_format));
        }
        let raw: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>().map_err(|e| e.to_string())?;
        let channels = usize::from(spec.channels.max(1));
        let samples = raw
            .chunks(channels)
            .map(|frame| frame.iter().map(|&s| f32::from(s)).sum::<f32>() / channels as f32)
            .collect();
        Ok(Wave { rate: spec.sample_rate, samples })
    }

    /// An Ogg Vorbis file, from its first `OggS` page (some retail files have bytes before it); stereo is mixed
    /// down.
    pub fn decode_ogg(bytes: &[u8]) -> Result<Wave, String> {
        let start = bytes.windows(4).position(|w| w == b"OggS").ok_or("no OggS page")?;
        let mut reader =
            lewton::inside_ogg::OggStreamReader::new(Cursor::new(&bytes[start..])).map_err(|e| e.to_string())?;
        let channels = usize::from(reader.ident_hdr.audio_channels.max(1));
        let mut samples = Vec::new();
        while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| e.to_string())? {
            samples.extend(
                packet.chunks(channels).map(|frame| frame.iter().map(|&s| f32::from(s)).sum::<f32>() / channels as f32),
            );
        }
        Ok(Wave { rate: reader.ident_hdr.audio_sample_rate, samples })
    }
}

/// The `PACENOTE` bank of each word id (`Word5880` 0x4b5880; ids 0 to 0xd are `RANGE` banks, 0x2b has none).
const WORDS: [(u32, &str); 50] = [
    (0x0e, "90_LEFT"),
    (0x0f, "90_RIGHT"),
    (0x10, "AND"),
    (0x11, "BRIDGE"),
    (0x12, "BUMP"),
    (0x13, "CAUTION"),
    (0x14, "CAUTION_WATER"),
    (0x15, "CUT"),
    (0x16, "DONTCUT"),
    (0x17, "EASYLEFT"),
    (0x18, "EASYRIGHT"),
    (0x19, "FASTLEFT"),
    (0x1a, "FASTRIGHT"),
    (0x1b, "FINISH"),
    (0x1c, "FORD"),
    (0x1d, "HAIRPINLEFT"),
    (0x1e, "HAIRPINRIGHT"),
    (0x1f, "INTO"),
    (0x20, "JUMP"),
    (0x21, "KLEFT"),
    (0x22, "KRIGHT"),
    (0x23, "LONG"),
    (0x24, "MAYBE"),
    (0x25, "MEDIUMLEFT"),
    (0x26, "MEDIUMRIGHT"),
    (0x27, "NARROWS"),
    (0x28, "OVERCREST"),
    (0x29, "TIGHTENSBAD"),
    (0x2a, "TIGHTENS"),
    (0x2c, "TWISTY"),
    (0x2d, "WIDEOUT"),
    (0x2e, "PLUS"),
    (0x2f, "MINUS"),
    (0x30, "ONTO"),
    (0x31, "FLATRIGHT"),
    (0x32, "FLATLEFT"),
    (0x33, "GOSTRAIGHT"),
    (0x34, "KEEPRIGHT"),
    (0x35, "KEEPLEFT"),
    (0x36, "KEEPMIDDLE"),
    (0, "400"),
    (1, "500"),
    (2, "600"),
    (3, "700"),
    (4, "800"),
    (5, "900"),
    (6, "1000"),
    (7, "100"),
    (8, "150"),
    (9, "200"),
];
const MORE_RANGES: [(u32, &str); 4] = [(0xa, "30"), (0xb, "300"), (0xc, "50"), (0xd, "70")];

/// The 1.02 word of a pace-note or range bank name (`EASYLEFT`, `30`), for a co-driver mod's call without a sound
/// file (`crate::custom_codriver`).
pub fn legacy_word(name: &str) -> Option<u32> {
    WORDS.iter().chain(&MORE_RANGES).find(|(_, n)| n.eq_ignore_ascii_case(name)).map(|&(id, _)| id)
}

/// The damage calls by code (the table at 0x7c37e0).
pub const DAMAGE_CALLS: [&str; 9] = [
    "",
    "batterys_flat",
    "steering_broke",
    "problem_engine",
    "wv_lost_brakes",
    "water_temp",
    "sus_broke",
    "weve_got_fire",
    "gearbox_broke",
];

/// The co-driver's English samples (`CPaceNoteHandler::M7170` 0x4b7170, language 0).
#[derive(Debug, Clone, Default)]
pub struct CoDriverBank {
    /// The takes of each word id.
    pub words: HashMap<u32, Vec<Wave>>,
    /// `NUMBER::NUMBERS`.
    pub numbers: Vec<Wave>,
    /// The damage calls by code (index 0 empty).
    pub damage: Vec<Wave>,
}

impl CoDriverBank {
    pub fn from_dat(dat: &AudioDat) -> Result<CoDriverBank, String> {
        let ogg = |path: String| -> Result<Wave, String> {
            let file = format!("{path}.ogg");
            Wave::decode_ogg(dat.file(&file).ok_or_else(|| format!("audio.dat has no {file}"))?)
                .map_err(|e| format!("{file}: {e}"))
        };
        let takes = |ini: &[rbr_formats::ini::Section], section: &str, dir: &str| -> Result<Vec<Wave>, String> {
            let count = audio::float(ini, section, "Sounds").unwrap_or(0.0) as usize;
            Ok((0..count)
                .filter_map(|k| audio::text(ini, section, &format!("Snd{k}")))
                .map(|name| ogg(format!("Audio\\Speech\\{dir}\\{}", name.trim())))
                // A take the archive lacks (`plus`, `minus` in retail) is left out; its word is then silent.
                .filter_map(Result::ok)
                .collect())
        };
        let notes = dat.ini("Audio\\CoDriver.ini")?;
        let numbers = dat.ini("Audio\\Numbers.ini")?;
        let mut words = HashMap::new();
        for (id, name) in WORDS.into_iter().chain(MORE_RANGES) {
            let bank = if id <= 0xd {
                takes(&numbers, &format!("RANGE::{name}"), "Number")?
            } else {
                takes(&notes, &format!("PACENOTE::{name}"), "PaceNote")?
            };
            words.insert(id, bank);
        }
        let damage =
            DAMAGE_CALLS
                .iter()
                .map(|name| {
                    if name.is_empty() {
                        Ok(Wave::default())
                    } else {
                        ogg(format!("Audio\\Speech\\Broken\\Eng\\{name}"))
                    }
                })
                .collect::<Result<_, _>>()?;
        Ok(CoDriverBank { words, numbers: takes(&numbers, "NUMBER::NUMBERS", "Number")?, damage })
    }
}

/// Everything one car's sound plays.
#[derive(Debug, Clone)]
pub struct CarBank {
    pub engine: Arc<eng::Engine>,
    pub transmission: Wave,
    pub turbo: Wave,
    pub blow_off: Wave,
    pub gear_changes: Vec<Wave>,
    /// The tyre loops by material (gravel, tarmac, snow), normal then skid (`TIRE::SURFACES`).
    pub surfaces: [[Wave; 2]; 3],
    /// `CAR::OTHER Offroad1-7`.
    pub offroad: Vec<Wave>,
    /// `CAR::RUMBLE Rumble` and `TIRE::SURFACES SkidWater` (the wetness), each wheel's third and fourth voice.
    pub rumble: Wave,
    pub wetness: Wave,
    /// `IMPACT::GRUS Snd1-20` (the gravel hits; `LoadWaves` starts at 1, so `Snd0` is unused), `CAR::SQUEEKS Sqe1-18`,
    /// `CAR::SUSPENSION Sus1-4` and `CAR::DRIVESHAFT DS1-13`.
    pub grus: Vec<Wave>,
    pub squeaks: Vec<Wave>,
    pub bottomings: Vec<Wave>,
    pub drive_shafts: Vec<Wave>,
    /// `CAR::EXHAUST Pop1-11` (the misfire's pops), `CARn::EXHAUSTNEW Pop1-11` (the runs) and `CAR::BANGS bang1-6`.
    pub exhaust_pops: Vec<Wave>,
    pub pops: Vec<Wave>,
    pub bangs: Vec<Wave>,
    /// `Audio\Cars\Effects\fire.wav`.
    pub fire: Wave,
    /// `CAR::IGNITION Ignition1`, `CAR::DOGRINGS DR1-8`, `Audio\Cars\Effects\cooler.wav` and `CAR::WIPER`
    /// `WiperEngine`, `WiperUp`, `WiperDown` (`crate::effects`).
    pub ignition: Wave,
    pub dog_rings: Vec<Wave>,
    pub cooler: Wave,
    pub wiper: [Wave; 3],
    /// `IMPACT::RANGED` and `IMPACT::SPEECH`.
    pub ranged: Vec<Wave>,
    pub screams: Vec<Wave>,
    /// `SFX::INGAME GO`.
    pub go: Wave,
    pub codriver: CoDriverBank,
    /// `Ambient/europe.ini`'s `AMBIENTFX` wind and birds (`CAmbientSounds::LoadBirds` 0x4aae10), and the siren.
    pub wind: Wave,
    pub birds: Vec<Wave>,
    pub siren: Wave,
    /// The rescue helicopter's loop (`Audio\Ambient\heli_g.wav`) and the bird hit (`IMPACT::VARIOUS` Snd0).
    pub helicopter: Wave,
    pub bird_hit: Wave,
    pub car: CarSound,
    pub gains: Gains,
    pub world_gains: WorldGains,
    pub scripted_gains: ScriptedGains,
    pub effect_gains: EffectGains,
    pub distances: Distances,
}

/// The `SOUNDDEF` min and max distances the car voices read, metres (rbr-recomp `notes/sound.md` 2.2: the tyre, whine, turbo and
/// blow-off voices set their own or none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Distances {
    pub exhaust: (f32, f32),
    pub ranged: (f32, f32),
    pub exhaust_pop: (f32, f32),
}

impl Distances {
    fn read(ingame: &[rbr_formats::ini::Section]) -> Distances {
        let def = |name| {
            let d = sound_def(ingame, name);
            (d.min_distance, d.max_distance)
        };
        Distances { exhaust: def("EXHAUST"), ranged: def("RANGED"), exhaust_pop: def("EXHAUSTPOP") }
    }
}

/// The retail `Audio/Ingame.ini` sets the car voices read: volume, min and max distance.
const RETAIL_SOUND_DEFS: [(&str, [f32; 3]); 6] = [
    ("EXHAUST", [1.0, 40.0, 100.0]),
    ("EXHAUSTPOP", [1.0, 20.0, 50.0]),
    ("TRANSMISSION", [1.0, 5.0, 20.0]),
    ("TIRE", [1.0, 10.0, 40.0]),
    ("BLOWOFFVALVE", [1.0, 10.0, 50.0]),
    ("RANGED", [1.0, 3.0, 50.0]),
];

/// `SOUNDDEF::<name>`, or with no such section the retail set (ours: the game reads a missing key as 0, which mutes
/// the voice).
fn sound_def(ingame: &[rbr_formats::ini::Section], name: &str) -> SoundDef {
    let section = format!("SOUNDDEF::{name}");
    match RETAIL_SOUND_DEFS.iter().find(|(n, _)| *n == name) {
        Some((_, [volume, min_distance, max_distance]))
            if !ingame.iter().any(|s| s.name.eq_ignore_ascii_case(&section)) =>
        {
            SoundDef { volume: *volume, outside_volume: 0.0, min_distance: *min_distance, max_distance: *max_distance }
        }
        _ => SoundDef::read(ingame, name),
    }
}

impl CarBank {
    /// Car `car`'s (the `Cars.ini` index, 0-7) sounds from `Root/audio.dat` and the 1.02 `audio.ini`. Without an
    /// `audio.ini` the retail amps are used (the game would read them as 0 and be silent).
    pub fn load(data: &Data, car: u32) -> Result<CarBank, String> {
        let path = data.audio_dat().ok_or("no Root/audio.dat")?;
        let dat = AudioDat::open(&path)?;
        let amps = match data.resolve("audio.ini") {
            Some(ini) => {
                let bytes = std::fs::read(&ini).map_err(|e| format!("{}: {e}", ini.display()))?;
                Some(Amps::read(&audio::parse_ini(&bytes).map_err(|e| format!("audio.ini: {e}"))?))
            }
            None => None,
        };
        CarBank::from_dat(&dat, car, amps)
    }

    pub fn from_dat(dat: &AudioDat, car: u32, amps: Option<Amps>) -> Result<CarBank, String> {
        let cars = dat.ini("Audio\\Cars\\Cars.ini")?;
        // A car slot the audio `Cars.ini` does not list (a modded `Cars/Cars.ini` with more than eight) takes car 0's
        // sound (ours).
        let car = if CarSound::read(&cars, car).is_some() { car } else { 0 };
        let sound = CarSound::read(&cars, car).ok_or_else(|| format!("Cars.ini has no Car{car}"))?;
        let name = format!("Audio\\Cars\\{}.eng", sound.engine);
        let bytes = dat.file(&name).ok_or_else(|| format!("audio.dat has no {name}"))?;
        let engine = Arc::new(eng::parse(bytes).map_err(|e| format!("{name}: {e}"))?);
        let effects = dat.ini("Audio\\Cars\\Effects.ini")?;
        let wave = |section: &str, key: &str| -> Result<Wave, String> {
            let path =
                audio::text(&effects, section, key).ok_or_else(|| format!("Effects.ini has no {section} {key}"))?;
            let file = format!("{}.wav", path.trim());
            Wave::decode(dat.file(&file).ok_or_else(|| format!("audio.dat has no {file}"))?)
                .map_err(|e| format!("{file}: {e}"))
        };
        let impacts = dat.ini("Audio\\Impact\\Sound.ini")?;
        let sample = |sections: &[rbr_formats::ini::Section], section: &str, key: &str| -> Result<Wave, String> {
            let path = audio::text(sections, section, key).ok_or_else(|| format!("no {section} {key}"))?;
            let file = format!("{}.wav", path.trim());
            Wave::decode(dat.file(&file).ok_or_else(|| format!("audio.dat has no {file}"))?)
                .map_err(|e| format!("{file}: {e}"))
        };
        let numbered = |section: &str| -> Result<Vec<Wave>, String> {
            (0..)
                .map_while(|k| audio::text(&impacts, section, &format!("Snd{k}")).map(|_| k))
                .map(|k| sample(&impacts, section, &format!("Snd{k}")))
                .collect()
        };
        // E31Wave::LoadWaves (0x4d83e0) and its like: `<prefix>1`, `<prefix>2`, ... while the key is there.
        let from_one =
            |sections: &[rbr_formats::ini::Section], section: &str, prefix: &str| -> Result<Vec<Wave>, String> {
                (1..)
                    .map_while(|k| audio::text(sections, section, &format!("{prefix}{k}")).map(|_| k))
                    .map(|k| sample(sections, section, &format!("{prefix}{k}")))
                    .collect()
            };
        let surface = |name: &str| -> Result<[Wave; 2], String> {
            Ok([wave("TIRE::SURFACES", &format!("Normal{name}"))?, wave("TIRE::SURFACES", &format!("Skid{name}"))?])
        };
        let frontend = dat.ini("Audio\\Frontend.ini")?;
        let ingame = dat.ini("Audio\\Ingame.ini")?;
        let volume = |name| sound_def(&ingame, name).volume;
        let gains = match amps {
            Some(a) => Gains {
                engine: volume("EXHAUST") * a.engine,
                gearbox: volume("TRANSMISSION") * a.engine_transmission,
                diff: volume("TRANSMISSION") * a.transmission_diff,
                turbo: volume("BLOWOFFVALVE") * a.turbo,
                blow_off: volume("BLOWOFFVALVE") * a.blow_off_valve,
                tyre: volume("TIRE") * a.gravel_particles,
                offroad: a.offtrack,
                ranged: volume("RANGED") * a.impacts,
                scream: a.impact_screams,
                squeak: a.squeeks,
                suspension: a.suspension,
                drive_shaft: a.drive_shaft_broken,
                pop: volume("EXHAUSTPOP") * a.pop,
                exhaust_pop: volume("EXHAUSTPOP") * a.exhaust_pop,
                bang: volume("EXHAUSTPOP") * a.bang,
                fire: a.enginefire,
                reflections: volume("EXHAUST") * a.engine * a.enviormental_reflections,
            },
            None => Gains::default(),
        };
        let world_gains = match amps {
            Some(a) => WorldGains { wind: a.wind, bird: a.bird, horn: a.horn, rain: a.rain },
            None => WorldGains::default(),
        };
        let scripted_gains = match amps {
            Some(a) => ScriptedGains { helicopter: a.helicopter, bird_hit: a.impacts },
            None => ScriptedGains::default(),
        };
        let effect_gains = match amps {
            Some(a) => EffectGains {
                ignition: a.ignition,
                cooler: a.broken_cooler,
                wiper_engine: a.wiper_engine,
                wiper: a.wiper,
            },
            None => EffectGains::default(),
        };
        let ambient = dat.ini("Audio\\Ambient\\europe.ini")?;
        let ambient_wave = |key: &str| sample(&ambient, "AMBIENTFX", key);
        let named = |file: &str| -> Result<Wave, String> {
            Wave::decode(dat.file(file).ok_or_else(|| format!("audio.dat has no {file}"))?)
                .map_err(|e| format!("{file}: {e}"))
        };
        Ok(CarBank {
            engine,
            transmission: wave("CAR::TRANSMISSION", "Transmission")?,
            turbo: wave("CAR::OTHER", "Turbo")?,
            blow_off: wave("CAR::OTHER", "Valve")?,
            gear_changes: (1..=3)
                .map(|k| wave("CAR::TRANSMISSION", &format!("GearChange{k}")))
                .collect::<Result<_, _>>()?,
            surfaces: [surface("Gravel")?, surface("Tarmac")?, surface("Snow")?],
            offroad: (1..=7).map(|k| wave("CAR::OTHER", &format!("Offroad{k}"))).collect::<Result<_, _>>()?,
            rumble: wave("CAR::RUMBLE", "Rumble")?,
            wetness: wave("TIRE::SURFACES", "SkidWater")?,
            grus: from_one(&impacts, "IMPACT::GRUS", "Snd")?,
            squeaks: from_one(&effects, "CAR::SQUEEKS", "Sqe")?,
            bottomings: from_one(&effects, "CAR::SUSPENSION", "Sus")?,
            drive_shafts: from_one(&effects, "CAR::DRIVESHAFT", "DS")?,
            exhaust_pops: from_one(&effects, "CAR::EXHAUST", "Pop")?,
            pops: from_one(&effects, &format!("CAR{car}::EXHAUSTNEW"), "Pop")?,
            bangs: from_one(&effects, "CAR::BANGS", "bang")?,
            fire: named("Audio\\Cars\\Effects\\fire.wav")?,
            ignition: wave("CAR::IGNITION", "Ignition1")?,
            dog_rings: from_one(&effects, "CAR::DOGRINGS", "DR")?,
            cooler: named("Audio\\Cars\\Effects\\cooler.wav")?,
            wiper: [
                wave("CAR::WIPER", "WiperEngine")?,
                wave("CAR::WIPER", "WiperUp")?,
                wave("CAR::WIPER", "WiperDown")?,
            ],
            ranged: numbered("IMPACT::RANGED")?,
            screams: numbered("IMPACT::SPEECH")?,
            go: sample(&frontend, "SFX::INGAME", "GO")?,
            codriver: CoDriverBank::from_dat(dat)?,
            wind: ambient_wave("wind")?,
            birds: (1..=crate::world::BIRDS).map(|k| ambient_wave(&format!("bird{k}"))).collect::<Result<_, _>>()?,
            siren: named("Audio\\Ambient\\siren.wav")?,
            helicopter: named("Audio\\Ambient\\heli_g.wav")?,
            bird_hit: sample(&impacts, "IMPACT::VARIOUS", "Snd0")?,
            car: sound,
            gains,
            world_gains,
            scripted_gains,
            effect_gains,
            distances: Distances::read(&ingame),
        })
    }
}

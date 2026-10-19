//! The game's sound data (`notes/formats.md` "Audio"): the `Root/audio.dat` archive, whose files the game opens as
//! `Audio\...`, and the values the gameplay sound reads from its inis: the `audio.ini` amps (patch 1.02, next to the
//! exe), the `Audio/Ingame.ini` `SOUNDDEF` sets and `Audio/Cars/Cars.ini`. The inis use the [`crate::ini`] layout. A
//! key the game finds missing reads as 0 there; here it is `None`, and the callers pick the code default.

use std::path::Path;

use crate::dat::{self, Entry};
use crate::ini::{self, Section};

/// The archive's root directory, which the game's `Audio\` paths replace.
const ROOT: &str = "C:/Projects/Rally_7/TargetMedia_Develop/Pc/Debug/World/Audio/";

/// `Root/audio.dat`, read whole.
pub struct AudioDat {
    bytes: Vec<u8>,
    entries: Vec<Entry>,
}

impl AudioDat {
    pub fn open(path: &Path) -> Result<AudioDat, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        AudioDat::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<AudioDat, String> {
        let entries = dat::parse(&bytes).map_err(|e| e.to_string())?;
        Ok(AudioDat { bytes, entries })
    }

    /// The files, by the path under `Audio/` with `/` separators (`Cars/subaru.eng`).
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.entries.iter().filter(|e| !e.is_dir).filter_map(|e| {
            let path = e.path.get(ROOT.len()..).filter(|_| e.path.starts_with(ROOT))?;
            Some((path, dat::file_data(&self.bytes, e)?))
        })
    }

    /// The file the game opens as `name` (`Audio\Cars\subaru.eng` or `Cars/subaru.eng`), ignoring case.
    pub fn file(&self, name: &str) -> Option<&[u8]> {
        let name = name.replace('\\', "/");
        let name = strip_prefix_ignore_case(&name, "audio/").unwrap_or(&name);
        self.files().find(|(path, _)| path.eq_ignore_ascii_case(name)).map(|(_, data)| data)
    }

    /// The ini the game opens as `name`, parsed.
    pub fn ini(&self, name: &str) -> Result<Vec<Section>, String> {
        let bytes = self.file(name).ok_or_else(|| format!("audio.dat has no {name}"))?;
        parse_ini(bytes).map_err(|e| format!("{name}: {e}"))
    }
}

fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len()).filter(|p| p.eq_ignore_ascii_case(prefix)).map(|_| &s[prefix.len()..])
}

/// Parses an audio ini: [`crate::ini`] over the bytes as Latin-1.
pub fn parse_ini(bytes: &[u8]) -> Result<Vec<Section>, ini::ParseError> {
    let text: String = bytes.iter().map(|&b| b as char).collect();
    ini::parse(&text)
}

/// The value of `key` in section `section`, as a float (the game's `atof`).
pub fn float(sections: &[Section], section: &str, key: &str) -> Option<f32> {
    let value = text(sections, section, key)?;
    value.trim().trim_end_matches('f').parse().ok()
}

/// The value of `key` in section `section`, as text, without an inline `;` comment (`audio.ini` has them).
pub fn text<'a>(sections: &'a [Section], section: &str, key: &str) -> Option<&'a str> {
    let value = sections.iter().find(|s| s.name.eq_ignore_ascii_case(section))?.get(key)?;
    Some(value.split(';').next().unwrap_or("").trim())
}

/// `audio.ini` `[AMP::SOUNDS]` (read by `E31Master::M4a70` 0x4d4a70): the volume multipliers, in the master's order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amps {
    pub engine: f32,
    pub engine_transmission: f32,
    pub transmission_diff: f32,
    pub turbo: f32,
    pub blow_off_valve: f32,
    pub drive_shaft_broken: f32,
    pub gravel_particles: f32,
    pub offtrack: f32,
    pub impacts: f32,
    pub impact_screams: f32,
    pub horn: f32,
    pub helicopter: f32,
    pub bird: f32,
    pub wind: f32,
    pub servicepark_bg: f32,
    pub servicepark_fx: f32,
    pub rain: f32,
    pub squeeks: f32,
    pub suspension: f32,
    pub pop: f32,
    pub exhaust_pop: f32,
    pub bang: f32,
    pub enginefire: f32,
    pub enviormental_reflections: f32,
    pub ignition: f32,
    pub broken_cooler: f32,
    pub wiper_engine: f32,
    pub wiper: f32,
}

impl Amps {
    /// The amps of `audio.ini`; a missing key is 0, as in the game.
    pub fn read(sections: &[Section]) -> Amps {
        let amp = |key| float(sections, "AMP::SOUNDS", key).unwrap_or(0.0);
        Amps {
            engine: amp("Engine"),
            engine_transmission: amp("EngineTransmission"),
            transmission_diff: amp("TransmissionDiff"),
            turbo: amp("Turbo"),
            blow_off_valve: amp("BlowOffValve"),
            drive_shaft_broken: amp("DriveShaftBroken"),
            gravel_particles: amp("GravelParticles"),
            offtrack: amp("Offtrack"),
            impacts: amp("Impacts"),
            impact_screams: amp("ImpactScreams"),
            horn: amp("Horn"),
            helicopter: amp("Helicopter"),
            bird: amp("Bird"),
            wind: amp("Wind"),
            servicepark_bg: amp("Servicepark_BG"),
            servicepark_fx: amp("Servicepark_FX"),
            rain: amp("Rain"),
            squeeks: amp("Squeeks"),
            suspension: amp("Suspension"),
            pop: amp("Pop"),
            exhaust_pop: amp("ExhaustPop"),
            bang: amp("Bang"),
            enginefire: amp("Enginefire"),
            enviormental_reflections: amp("EnviormentalReflections"),
            ignition: amp("Ignition"),
            broken_cooler: amp("BrokenCooler"),
            wiper_engine: amp("WiperEngine"),
            wiper: amp("Wiper"),
        }
    }
}

/// One `Audio/Ingame.ini` `SOUNDDEF::*` set (`E31Master::M48e0` 0x4d48e0).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SoundDef {
    pub volume: f32,
    pub outside_volume: f32,
    pub min_distance: f32,
    pub max_distance: f32,
}

impl SoundDef {
    /// The set `SOUNDDEF::<name>` (`EXHAUST`, `TRANSMISSION`, `TIRE`, `BLOWOFFVALVE`, `RANGED`, ...).
    pub fn read(sections: &[Section], name: &str) -> SoundDef {
        let section = format!("SOUNDDEF::{name}");
        let value = |key| float(sections, &section, key).unwrap_or(0.0);
        SoundDef {
            volume: value("Volume"),
            outside_volume: value("OutsideVolume"),
            min_distance: value("MinDistance"),
            max_distance: value("MaxDistance"),
        }
    }
}

/// `Audio/Cars/Cars.ini` (`CCarSound::OnX` 0x4b1810) for one car index (0-7).
#[derive(Debug, Clone, PartialEq)]
pub struct CarSound {
    /// The `.eng` bank name (`subaru` for every retail car).
    pub engine: String,
    /// The low-pass cutoff on the engine with the camera in the car, Hz (code default 300).
    pub inside_cut_frequency: f32,
    pub low_shelf_amp: f32,
    pub low_shelf_frequency: f32,
}

impl CarSound {
    pub fn read(sections: &[Section], car: u32) -> Option<CarSound> {
        let key = format!("Car{car}");
        Some(CarSound {
            engine: text(sections, "CARS", &key)?.trim().to_owned(),
            inside_cut_frequency: float(sections, "CARS", "InsideCutFrequency").unwrap_or(300.0),
            low_shelf_amp: float(sections, "CARS", &format!("{key}_LowShelfAmp")).unwrap_or(1.0),
            low_shelf_frequency: float(sections, "CARS", &format!("{key}_LowShelfFrequency")).unwrap_or(300.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_amps_sounddefs_and_cars() {
        let audio = parse_ini(b"[AMP::SOUNDS]\r\nEngine = 1.0\t\t; engine\r\nTurbo=0.2\r\n").unwrap();
        let amps = Amps::read(&audio);
        assert_eq!((amps.engine, amps.turbo, amps.impacts), (1.0, 0.2, 0.0));
        let ingame = parse_ini(b"[SOUNDDEF::EXHAUST]\nVolume=1.0\nMinDistance=40\nMaxDistance=100\n").unwrap();
        let exhaust = SoundDef::read(&ingame, "EXHAUST");
        assert_eq!((exhaust.volume, exhaust.min_distance, exhaust.max_distance), (1.0, 40.0, 100.0));
        let cars = parse_ini(b"[CARS]\nInsideCutFrequency=750.0\nCar3=subaru\nCar3_LowShelfAmp=2.0\n").unwrap();
        let car = CarSound::read(&cars, 3).unwrap();
        assert_eq!((car.engine.as_str(), car.inside_cut_frequency, car.low_shelf_amp), ("subaru", 750.0, 2.0));
        assert_eq!(car.low_shelf_frequency, 300.0);
        assert_eq!(CarSound::read(&cars, 4), None);
    }
}

//! Retail sound data (`notes/formats/audio.md`, `notes/formats/eng.md`). Runs only when `RBR_DATA` points at the
//! extracted retail install; `RBR_PATCH` adds the 1.02 `audio.ini`.

use std::path::{Path, PathBuf};

use rbr_formats::audio::{self, Amps, AudioDat, CarSound, SoundDef};
use rbr_formats::eng;

fn audio_dat() -> Option<AudioDat> {
    let Some(data) = std::env::var_os("RBR_DATA") else {
        eprintln!("skipping: RBR_DATA not set");
        return None;
    };
    Some(AudioDat::open(&Path::new(&data).join("Root/audio.dat")).expect("audio.dat"))
}

/// All 8 `.eng` files: the four 1.02 can load parse to their exact size with firing periods that match the bank's
/// rpm (2 firings per revolution; within 1% in `subaru.eng` except its 1700 rpm load-0 part, which measures 1562 (as
/// in its near-copy `test.eng`), and within 2% in the other files); the four two-wave files are refused.
#[test]
fn every_retail_eng_parses_or_is_refused() {
    let Some(dat) = audio_dat() else { return };
    let mut loaded = Vec::new();
    let mut refused = Vec::new();
    for (path, bytes) in dat.files().filter(|(p, _)| p.to_ascii_lowercase().ends_with(".eng")) {
        let name = path.rsplit('/').next().unwrap().to_owned();
        match eng::parse(bytes) {
            Ok(engine) => {
                for bank in &engine.banks {
                    assert_eq!(bank.parts.len(), 2, "{name}: parts per bank");
                    for part in &bank.parts {
                        assert!(part.cycles.len() >= 30, "{name}: {} cycles", part.cycles.len());
                        assert!(part.cycles.last().unwrap() < &(part.samples.len() as f64), "{name}");
                        let span = part.cycles.last().unwrap() - part.cycles[0];
                        let period = span / (part.cycles.len() - 1) as f64;
                        let rpm = f64::from(part.rate) / period * 30.0;
                        let odd = (name == "subaru.eng" || name == "test.eng") && part.rpm == 1700 && part.load == 0;
                        let tolerance = match name.as_str() {
                            _ if odd => 0.1,
                            "subaru.eng" => 0.01,
                            _ => 0.02,
                        };
                        assert!(
                            (rpm / f64::from(part.rpm) - 1.0).abs() < tolerance,
                            "{name}: {} rpm load {} measures {rpm}",
                            part.rpm,
                            part.load
                        );
                    }
                }
                loaded.push((name, engine.banks.len()));
            }
            Err(e) => {
                assert_eq!(e, eng::Error::TwoWaveParts, "{name}");
                refused.push(name);
            }
        }
    }
    loaded.sort();
    refused.sort();
    assert_eq!(
        loaded,
        [
            ("mitsu.eng".to_owned(), 5),
            ("subaru.eng".to_owned(), 6),
            ("subaru2.eng".to_owned(), 5),
            ("test.eng".to_owned(), 6)
        ]
    );
    assert_eq!(refused, ["Saxo.eng", "bubbla.eng", "fiat.eng", "vaux.eng"]);
    let subaru = eng::parse(dat.file("Audio\\Cars\\subaru.eng").unwrap()).unwrap();
    let rpms: Vec<u32> = subaru.banks.iter().map(|b| b.rpm()).collect();
    assert_eq!(rpms, [1700, 3000, 4000, 5000, 6000, 7000]);
}

/// The 13 inis parse, and the values the gameplay sound reads are the retail ones.
#[test]
fn every_retail_audio_ini_parses() {
    let Some(dat) = audio_dat() else { return };
    let inis: Vec<&str> = dat.files().map(|(p, _)| p).filter(|p| p.to_ascii_lowercase().ends_with(".ini")).collect();
    assert_eq!(inis.len(), 13, "{inis:?}");
    for name in &inis {
        let sections = dat.ini(name).unwrap();
        assert!(!sections.is_empty(), "{name}");
    }
    let cars = dat.ini("Audio\\Cars\\Cars.ini").unwrap();
    for car in 0..8 {
        let sound = CarSound::read(&cars, car).unwrap();
        assert_eq!(sound.engine, "subaru");
        assert_eq!(sound.inside_cut_frequency, 750.0);
    }
    let ingame = dat.ini("Audio\\Ingame.ini").unwrap();
    let exhaust = SoundDef::read(&ingame, "EXHAUST");
    assert_eq!((exhaust.volume, exhaust.min_distance, exhaust.max_distance), (1.0, 40.0, 100.0));
    assert_eq!(SoundDef::read(&ingame, "TRANSMISSION").volume, 1.0);
    assert_eq!(SoundDef::read(&ingame, "SHATTER").volume, 0.0);
    let effects = dat.ini("Audio\\Cars\\Effects.ini").unwrap();
    for (section, key) in [("CAR::TRANSMISSION", "Transmission"), ("CAR::OTHER", "Valve"), ("CAR::OTHER", "Turbo")] {
        let path = audio::text(&effects, section, key).unwrap();
        assert!(dat.file(&format!("{}.wav", path.trim())).is_some(), "{section} {key}: {path}");
    }
}

/// The 1.02 `audio.ini` amps.
#[test]
fn patch_audio_ini_amps() {
    let Some(patch) = std::env::var_os("RBR_PATCH").map(PathBuf::from) else {
        eprintln!("skipping: RBR_PATCH not set");
        return;
    };
    let sections = audio::parse_ini(&std::fs::read(patch.join("audio.ini")).unwrap()).unwrap();
    let amps = Amps::read(&sections);
    assert_eq!((amps.engine, amps.engine_transmission, amps.transmission_diff), (1.0, 0.35, 0.35));
    assert_eq!((amps.turbo, amps.blow_off_valve, amps.drive_shaft_broken), (0.2, 1.0, 1.0));
    assert_eq!(amps.gravel_particles, 1.0);
    assert_eq!((amps.helicopter, amps.servicepark_bg, amps.servicepark_fx), (0.3, 1.0, 1.0));
    assert_eq!((amps.squeeks, amps.suspension, amps.enginefire), (1.0, 1.0, 1.0));
    assert_eq!((amps.pop, amps.exhaust_pop, amps.bang), (0.5, 0.5, 0.5));
}

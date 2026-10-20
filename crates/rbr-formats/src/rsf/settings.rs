//! The settings of an RSF drive that change its force feedback, the wheel or the driver aids (`notes/ngp.md` "RSF
//! settings"), resolved
//! for one car: from an RSF install's files ([`Sources::install`]) or from a recorded session's `meta.txt`
//! ([`Sources::session`]). Reading is pure (text in, typed values out) but for [`Sources::install`]'s file reads.
//!
//! - `RichardBurnsRally.ini` `[NGP]`: `ForceFeedbackSensitivity<Surface>` (NGP `0x1007a500`, default 100) and
//!   `ForceFeedbackSensitivity<Surface>_Car<slot>` (`0x1007a630`, default 0); car init (`0x1003f5b0`) takes the slot's
//!   when above 0, else the global one when above 0, and scales `S` by it unless it is 100. `ffbDeadZone`,
//!   `ffbDeadZoneThreshold` (`0x10064620`), `ffbRate`, `inputRate`, and `headGasketDurability` (`0x100e71f0`: from 2 on
//!   it patches the engine's `0x50d9c4`).
//! - `rallysimfans_personal.ini` `[car<id>]`: `forcefeedbacksensitivity<surface>` (RSF's per-car override; taken here
//!   before the slot's key, as RSF's launcher offers it; not yet seen set in a drive) and `steeringrotation`. Only
//!   these keys are read: the file may hold account data.
//! - `Plugins/NGPCarMenu.ini` `AdjustSteeringRotation` (RSF's "Adjust steering wheel range per car") and
//!   `rsfdata/cache/cars_data.json` `steering_wheel`: the car's rotation when the player gave none.
//! - The game's profile (`SavedGames/pf<name>.rbr`, the newest): `FRCFEEDBK`, `STRENGTH`, `DIRECTION`, and the aids
//!   `GEAR_PROT`, `CS_NEUTRALLOCK`, `CLUTCH_HLP` (`SGearSettings`, the globals `0x1660ea0..ea8` the car's control unit
//!   takes, car `+0x894..+0x89c`; the automatic gearbox, `0x1660814`, is per player and not in it).

use std::path::Path;

use super::cars::rows;
use crate::ini;
use crate::saves::profile::ProfileFile;

/// The surfaces of the per-surface values, in NGP's `SetupSpec` order (0 gravel, 1 tarmac, 2 snow).
pub const SURFACES: [&str; 3] = ["Gravel", "Tarmac", "Snow"];

/// Where a value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `rallysimfans_personal.ini` `[car<id>]`.
    Car,
    /// `[NGP]`'s `_Car<slot>` key.
    Slot,
    /// `[NGP]`'s key for every car.
    Ngp,
    /// `cars_data.json`.
    CarsData,
    /// The game's profile, by its file name.
    Profile(String),
    /// Nothing set: NGP's or the game's default.
    Default,
}

impl Source {
    pub fn name(&self) -> String {
        match self {
            Source::Car => "rallysimfans_personal.ini".to_owned(),
            Source::Slot => "[NGP] _Car".to_owned(),
            Source::Ngp => "[NGP]".to_owned(),
            Source::CarsData => "cars_data.json".to_owned(),
            Source::Profile(name) => name.clone(),
            Source::Default => "default".to_owned(),
        }
    }
}

/// The game's options a drive takes: force feedback (`SForceFeedBackSettings`) and driver aids (`SGearSettings`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Game {
    pub enabled: bool,
    pub strength: f32,
    /// `DIRECTION` 0 (`INVERSE`).
    pub inverse: bool,
    pub gear_guard: bool,
    pub neutral_lock: bool,
    pub clutch_help: bool,
}

/// The raw values, as the files hold them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sources {
    /// The `[NGP]` section's entries.
    pub ngp: Vec<(String, String)>,
    /// The car's `forcefeedbacksensitivity<surface>` and `steeringrotation` (empty values left out).
    pub car: Vec<(String, String)>,
    /// `AdjustSteeringRotation`, when known.
    pub adjust_rotation: Option<bool>,
    /// `cars_data.json` `steering_wheel` of the car.
    pub car_rotation: Option<u16>,
    /// The game's options and the profile's file name.
    pub game: Option<(Game, String)>,
}

/// The settings a drive of the car runs with.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// NGP's force-feedback sensitivity percent per surface (100 leaves `S`), [`SURFACES`] order.
    pub sensitivity: [i32; 3],
    pub sensitivity_source: [Source; 3],
    /// `ffbDeadZone` (a steering input) and `ffbDeadZoneThreshold` (a fraction of the full force).
    pub dead_zone: f32,
    pub dead_zone_threshold: f32,
    /// `ffbRate` and `inputRate`, Hz (NGP's threads; 0 off).
    pub ffb_rate: u32,
    pub input_rate: u32,
    /// `headGasketDurability` (1, NGP's default, and below 2 leave the engine alone).
    pub head_gasket_durability: i32,
    /// The game's options; `None` without a profile.
    pub game: Option<(Game, Source)>,
    /// The wheel's lock-to-lock degrees when RSF sets it per car (`AdjustSteeringRotation` on), and from where.
    pub rotation: Option<(u16, Source)>,
}

/// The entries of `section` in `text` (an ini), names as written.
fn section(text: &str, name: &str) -> Vec<(String, String)> {
    ini::parse(text)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.name.eq_ignore_ascii_case(name))
        .flat_map(|s| s.entries)
        .collect()
}

fn get<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
}

/// The personal ini's keys the settings read.
fn car_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("steeringrotation")
        || SURFACES.iter().any(|s| key.eq_ignore_ascii_case(&format!("forcefeedbacksensitivity{s}")))
}

/// The car's keys of `rallysimfans_personal.ini` (only [`car_key`]s, set ones).
pub fn personal(text: &str, car: u32) -> Vec<(String, String)> {
    section(text, &format!("car{car}")).into_iter().filter(|(k, v)| car_key(k) && !v.is_empty()).collect()
}

/// `AdjustSteeringRotation` of `NGPCarMenu.ini` (its `[Default]` section).
pub fn adjust_rotation(text: &str) -> Option<bool> {
    get(&section(text, "Default"), "AdjustSteeringRotation").and_then(|v| v.trim().parse::<i32>().ok()).map(|v| v != 0)
}

/// The car's `steering_wheel` of `cars_data.json`.
pub fn car_rotation(text: &str, car: u32) -> Option<u16> {
    let id = car.to_string();
    rows(text)?.into_iter().find(|r| r.get("id") == Some(&id))?.get("steering_wheel")?.parse().ok()
}

/// The game's options of a profile file (`pf<name>.rbr`).
pub fn game_options(profile: &[u8]) -> Result<Game, String> {
    let menu = ProfileFile::read(profile).map_err(|e| e.to_string())?.menu;
    let (f, g) = (menu.force_feedback, menu.gear);
    Ok(Game {
        enabled: f.enabled != 0,
        strength: f.strength,
        inverse: f.direction == 0,
        gear_guard: g.gear_protection != 0,
        neutral_lock: g.neutral_lock != 0,
        clutch_help: g.clutch_help != 0,
    })
}

impl Sources {
    /// The values a session's `meta.txt` recorded as it began: `before.ini.NGP.<key>` and
    /// `before.rsf.car<id>.<key>` (rec/capture's `scripts/rsf-session.sh`).
    pub fn session(meta: &str, car: u32) -> Sources {
        let lines = meta.lines().filter_map(|l| l.trim_end_matches('\r').split_once('='));
        let car_prefix = format!("before.rsf.car{car}.");
        let (mut ngp, mut keys) = (Vec::new(), Vec::new());
        for (key, value) in lines {
            if let Some(k) = key.strip_prefix("before.ini.NGP.") {
                ngp.push((k.to_owned(), value.trim().to_owned()));
            } else if let Some(k) = key.strip_prefix(&car_prefix).filter(|k| car_key(k)) {
                if !value.trim().is_empty() {
                    keys.push((k.to_owned(), value.trim().to_owned()));
                }
            }
        }
        Sources { ngp, car: keys, ..Sources::default() }
    }

    /// The values of the RSF install at `root` for RSF car `car`; a file that is not there leaves its values unset.
    pub fn install(root: &Path, car: u32) -> Sources {
        let text = |path: &str| std::fs::read(root.join(path)).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
        Sources {
            ngp: text("RichardBurnsRally.ini").map(|t| section(&t, "NGP")).unwrap_or_default(),
            car: text("rallysimfans_personal.ini").map(|t| personal(&t, car)).unwrap_or_default(),
            adjust_rotation: text("Plugins/NGPCarMenu.ini").as_deref().and_then(adjust_rotation),
            car_rotation: text("rsfdata/cache/cars_data.json").as_deref().and_then(|t| car_rotation(t, car)),
            game: newest_profile(&root.join("SavedGames"))
                .and_then(|path| Some((game_options(&std::fs::read(&path).ok()?).ok()?, file_name(&path)))),
        }
    }

    /// The settings for the car in game slot `slot`.
    pub fn resolve(&self, slot: u32) -> Settings {
        let number = |key: &str| get(&self.ngp, key).and_then(|v| v.trim().parse::<f32>().ok());
        let percent = |entries: &[(String, String)], key: &str| {
            get(entries, key).and_then(|v| v.trim().parse::<f32>().ok()).map(|v| v as i32).filter(|v| *v > 0)
        };
        let mut sensitivity = [100; 3];
        let mut sensitivity_source = [Source::Default, Source::Default, Source::Default];
        for (k, surface) in SURFACES.iter().enumerate() {
            let key = format!("ForceFeedbackSensitivity{surface}");
            let found = percent(&self.car, &key)
                .map(|v| (v, Source::Car))
                .or_else(|| percent(&self.ngp, &format!("{key}_Car{slot}")).map(|v| (v, Source::Slot)))
                .or_else(|| percent(&self.ngp, &key).map(|v| (v, Source::Ngp)));
            if let Some((value, source)) = found {
                (sensitivity[k], sensitivity_source[k]) = (value, source);
            }
        }
        let rotation = match self.adjust_rotation {
            Some(true) => get(&self.car, "steeringrotation")
                .and_then(|v| v.trim().parse().ok())
                .map(|d| (d, Source::Car))
                .or(self.car_rotation.map(|d| (d, Source::CarsData))),
            _ => None,
        };
        Settings {
            sensitivity,
            sensitivity_source,
            dead_zone: number("ffbDeadZone").unwrap_or(0.0),
            dead_zone_threshold: number("ffbDeadZoneThreshold").unwrap_or(0.0),
            ffb_rate: number("ffbRate").map_or(1000, |v| v as u32),
            input_rate: number("inputRate").map_or(1000, |v| v as u32),
            head_gasket_durability: number("headGasketDurability").map_or(1, |v| v as i32),
            game: self.game.as_ref().map(|(f, name)| (*f, Source::Profile(name.clone()))),
            rotation,
        }
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The most recently written `pf*.rbr` in `dir`: the profile last played.
fn newest_profile(dir: &Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("pf") && name.ends_with(".rbr")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

impl Settings {
    /// One line per value with its source, for the launcher and `rsf-settings`.
    pub fn describe(&self) -> Vec<String> {
        let mut out: Vec<String> = SURFACES
            .iter()
            .zip(self.sensitivity.iter().zip(&self.sensitivity_source))
            .map(|(s, (v, source))| format!("sensitivity {} {v}% ({})", s.to_ascii_lowercase(), source.name()))
            .collect();
        out.push(format!("dead zone {} threshold {} ([NGP])", self.dead_zone, self.dead_zone_threshold));
        out.push(format!("ffb rate {} Hz, input rate {} Hz ([NGP])", self.ffb_rate, self.input_rate));
        out.push(match &self.game {
            Some((g, source)) => format!(
                "game force feedback {}, strength {}, {}; gear guard {}, neutral lock {}, clutch help {} ({})",
                if g.enabled { "on" } else { "off" },
                g.strength,
                if g.inverse { "inverse" } else { "normal" },
                on_off(g.gear_guard),
                on_off(g.neutral_lock),
                on_off(g.clutch_help),
                source.name()
            ),
            None => "game options: no profile".to_owned(),
        });
        out.push(match &self.rotation {
            Some((d, source)) => format!("rotation {d} degrees ({})", source.name()),
            None => "rotation: not set per car".to_owned(),
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn a_car_value_then_the_slots_then_the_global_one_as_ngp_takes_them() {
        let s = Sources {
            ngp: entries(&[
                ("ForceFeedbackSensitivityGravel", "280"),
                ("ForceFeedbackSensitivityTarmac", "250"),
                ("ForceFeedbackSensitivitySnow", "0"),
                ("ForceFeedbackSensitivityGravel_Car5", "0"),
                ("ForceFeedbackSensitivityTarmac_Car5", "120"),
                ("ForceFeedbackSensitivityTarmac_Car4", "90"),
            ]),
            car: entries(&[("forcefeedbacksensitivitygravel", "352")]),
            ..Sources::default()
        };
        let r = s.resolve(5);
        assert_eq!(r.sensitivity, [352, 120, 100]);
        assert_eq!(r.sensitivity_source, [Source::Car, Source::Slot, Source::Default]);
        assert_eq!(s.resolve(4).sensitivity, [352, 90, 100]);
        assert_eq!((r.ffb_rate, r.input_rate, r.head_gasket_durability, r.dead_zone), (1000, 1000, 1, 0.0));
        assert_eq!((r.game, r.rotation), (None, None));
    }

    #[test]
    fn the_rotation_only_when_rsf_adjusts_it_per_car() {
        let mut s = Sources { car_rotation: Some(565), adjust_rotation: Some(true), ..Sources::default() };
        assert_eq!(s.resolve(5).rotation, Some((565, Source::CarsData)));
        s.car = entries(&[("steeringrotation", "720")]);
        assert_eq!(s.resolve(5).rotation, Some((720, Source::Car)));
        s.adjust_rotation = Some(false);
        assert_eq!(s.resolve(5).rotation, None);
    }

    #[test]
    fn only_the_named_personal_keys_are_read() {
        let text = "[car88]\r\nforcefeedbacksensitivitytarmac=\r\nsteeringrotation=540\r\nlogin=secret\r\n\
                    [car87]\r\nforcefeedbacksensitivitygravel=300\r\n";
        assert_eq!(personal(text, 88), entries(&[("steeringrotation", "540")]));
        assert_eq!(personal(text, 87), entries(&[("forcefeedbacksensitivitygravel", "300")]));
        assert_eq!(adjust_rotation("[Default]\nAdjustSteeringRotation=1\n"), Some(true));
        assert_eq!(car_rotation(r#"[{"steering_wheel":"565","id":"88"},{"id":"1"}]"#, 88), Some(565));
        assert_eq!(car_rotation(r#"[{"steering_wheel":"","id":"88"}]"#, 88), None);
    }

    #[test]
    fn a_sessions_meta_lines() {
        let meta = "session=x\nbefore.ini.NGP.ffbRate=500\nbefore.ini.NGP.ForceFeedbackSensitivityTarmac=250\n\
                    after.ini.NGP.ffbRate=1\nbefore.rsf.car88.steeringrotation=\nbefore.rsf.car88.\
                    forcefeedbacksensitivitysnow=310\nbefore.rsf.car87.forcefeedbacksensitivitysnow=1\n";
        let s = Sources::session(meta, 88);
        assert_eq!(s.car, entries(&[("forcefeedbacksensitivitysnow", "310")]));
        let r = s.resolve(5);
        assert_eq!((r.sensitivity, r.ffb_rate), ([100, 250, 310], 500));
    }

    /// The recorded RSF sessions' meta.txt (`RBR_RECORDINGS/rsf/drive*`): RSF's [NGP] as the user drives it.
    #[test]
    fn the_recorded_sessions() {
        let Some(root) = std::env::var_os("RBR_RECORDINGS").map(std::path::PathBuf::from) else { return };
        let mut seen = 0;
        for session in ["drive1", "drive2"] {
            let Ok(meta) = std::fs::read_to_string(root.join("rsf").join(session).join("meta.txt")) else { continue };
            for car in [55, 67, 80, 87, 88] {
                let r = Sources::session(&meta, car).resolve(5);
                assert_eq!(r.sensitivity, [280, 250, 280], "{session} car {car}");
                assert_eq!(r.sensitivity_source, [Source::Ngp, Source::Ngp, Source::Ngp]);
                assert_eq!((r.dead_zone, r.dead_zone_threshold, r.ffb_rate, r.input_rate), (0.0, 0.0, 1000, 1000));
                assert_eq!(r.head_gasket_durability, 10);
            }
            seen += 1;
        }
        eprintln!("{seen} RSF sessions");
    }

    /// An RSF install (`RBR_RSF`): whatever of its files are there resolve.
    #[test]
    fn the_rsf_install() {
        let Some(root) = std::env::var_os("RBR_RSF").map(std::path::PathBuf::from) else { return };
        let s = Sources::install(&root, 88);
        if let Some(degrees) = s.car_rotation {
            assert!((90..=2520).contains(&degrees));
        }
        eprintln!("{}", s.resolve(5).describe().join("\n"));
    }
}

//! "Use RSF's settings" (the Controls page): a drive of an RSF car takes the force feedback and the wheel rotation
//! RSF gives that car (`rbr_formats::rsf::settings`: NGP's sensitivity per surface, dead zone and rate, the game
//! profile's strength, direction and driver aids, the per-car rotation when RSF adjusts it) and NGP's
//! `headGasketDurability` in place of the page's own.

use std::path::{Path, PathBuf};

use rbr_formats::rsf::settings::{Settings, Sources};
use rbr_formats::rsf::SLOT;
use rbr_gameplay::race_options::DriverAids;
use rbr_render::{ffb, wheel_rotation};

use crate::catalog::Car;
use crate::Form;

/// The drive's bindings and RSF's force feedback, in the player's folder.
const FILE: &str = "launcher-rsf-drive.ini";

/// The switch and the last car's settings.
#[derive(Debug, Clone, Default)]
pub struct RsfSettings {
    /// The player's choice; unset, on when an RSF folder is set.
    pub on: Option<bool>,
    cache: Option<((String, u32), Settings)>,
}

impl RsfSettings {
    pub fn new(on: Option<bool>) -> RsfSettings {
        RsfSettings { on, cache: None }
    }

    pub fn is_on(&self, root: &str) -> bool {
        self.on.unwrap_or(!root.trim().is_empty())
    }

    /// RSF's settings for `car` (an RSF car of the install at `root`, not an added one), read once per car; `fresh`
    /// reads the files again.
    pub fn of(&mut self, root: &str, car: &Car, fresh: bool) -> Option<&Settings> {
        let id = car.rsf.as_ref().filter(|r| !r.added)?.id;
        let root = root.trim();
        if root.is_empty() {
            return None;
        }
        let key = (root.to_owned(), id);
        if fresh || self.cache.as_ref().map(|(k, _)| k) != Some(&key) {
            self.cache = Some((key, Sources::install(Path::new(root), id).resolve(SLOT)));
        }
        self.cache.as_ref().map(|(_, s)| s)
    }
}

/// What the Controls page shows: the switch (whether there is an RSF folder) and, for the selected RSF car, what RSF
/// sets it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Shown {
    pub on: bool,
    pub folder: bool,
    pub car: Option<(String, Vec<String>)>,
}

impl Form {
    pub fn rsf_shown(&mut self) -> Shown {
        let root = self.rsf();
        let on = self.rsf_settings.is_on(&root);
        let car = self.catalog.cars.get(self.car).cloned();
        let car = car.and_then(|car| Some((car.name.clone(), self.rsf_settings.of(&root, &car, false)?.describe())));
        Shown { on, folder: !root.trim().is_empty(), car }
    }

    /// The force feedback a drive of `car` runs with: RSF's for an RSF car when the switch is on, else the page's;
    /// and a note of what RSF set. RSF's profile also sets `aids`, but for the automatic gearbox it does not keep.
    pub fn drive_ffb(&mut self, car: &Car, aids: &mut DriverAids) -> (ffb::Options, Option<String>) {
        let root = self.rsf();
        if !self.rsf_settings.is_on(&root) {
            return (self.ffb.clone(), None);
        }
        match self.rsf_settings.of(&root, car, true) {
            Some(s) => {
                if let Some((game, _)) = &s.game {
                    aids.gear_guard = game.gear_guard;
                    aids.neutral_lock = game.neutral_lock;
                    aids.clutch_help = game.clutch_help;
                }
                (applied(&self.ffb, s), Some(s.describe().join("; ")))
            }
            None => (self.ffb.clone(), None),
        }
    }

    /// NGP's `headGasketDurability` a drive of `car` runs with: RSF's for an RSF car when the switch is on, else
    /// NGP's default 1.
    pub fn drive_head_gasket(&mut self, car: &Car) -> i32 {
        let root = self.rsf();
        if !self.rsf_settings.is_on(&root) {
            return 1;
        }
        self.rsf_settings.of(&root, car, false).map_or(1, |s| s.head_gasket_durability)
    }
}

/// `options` with RSF's values for the car: what RSF does not set stays the page's.
pub fn applied(options: &ffb::Options, s: &Settings) -> ffb::Options {
    let mut out = options.clone();
    out.sensitivity = s.sensitivity;
    out.settings.dead_zone = s.dead_zone;
    out.settings.dead_zone_threshold = s.dead_zone_threshold;
    // 0 stops NGP's FFB thread (the game thread sends then); ours keeps its rate.
    if s.ffb_rate > 0 {
        out.rate = s.ffb_rate.min(1000);
    }
    if let Some((game, _)) = &s.game {
        out.settings.enabled = game.enabled;
        out.settings.strength = game.strength.clamp(0.0, 1.0);
        out.settings.inverse = game.inverse;
    }
    if let Some((degrees, _)) = &s.rotation {
        out.rotation = Some((*degrees).clamp(*wheel_rotation::DEGREES.start(), *wheel_rotation::DEGREES.end()));
    }
    out
}

/// The viewer's `--bindings` file for a drive with `options`, beside the settings file `settings`: `text` (the
/// bindings) and the force feedback, so the settings file keeps the page's own.
pub fn write_bindings(settings: &Path, text: &str, options: &ffb::Options) -> Result<PathBuf, String> {
    let path = settings.with_file_name(FILE);
    std::fs::write(&path, format!("{text}\n{}", options.to_text())).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::rsf::settings::{Game, Source};

    #[test]
    fn rsfs_values_replace_the_pages_where_it_sets_them() {
        let page = ffb::Options { rotation: Some(900), device: Some("G923".into()), ..ffb::Options::default() };
        let mut s = Sources {
            ngp: vec![("ForceFeedbackSensitivityTarmac".into(), "250".into()), ("ffbRate".into(), "0".into())],
            ..Sources::default()
        }
        .resolve(SLOT);
        let o = applied(&page, &s);
        assert_eq!((o.sensitivity, o.rate, o.rotation, o.settings), ([100, 250, 100], 1000, Some(900), page.settings));
        s.game = Some((
            Game {
                enabled: true,
                strength: 0.7,
                inverse: true,
                gear_guard: true,
                neutral_lock: false,
                clutch_help: false,
            },
            Source::Default,
        ));
        s.rotation = Some((565, Source::CarsData));
        let o = applied(&page, &s);
        assert_eq!((o.settings.strength, o.settings.inverse, o.rotation), (0.7, true, Some(565)));
        assert_eq!(o.device.as_deref(), Some("G923"));
    }

    /// `RBR_RSF`'s settings for its first car, as the Controls page shows them.
    #[test]
    fn the_rsf_install() {
        let Some(root) = std::env::var_os("RBR_RSF") else { return };
        let Ok(install) = rbr_formats::rsf::Install::open(Path::new(&root)) else { return };
        let car = Car {
            index: SLOT,
            name: install.cars[0].name.clone(),
            rsf: Some(crate::catalog::RsfCar {
                id: install.cars[0].id,
                added: false,
                group: None,
                pack: Err(String::new()),
                model: Ok(None),
            }),
        };
        let mut rsf = RsfSettings::default();
        let s = rsf.of(&root.to_string_lossy(), &car, false).cloned().expect("an RSF car's settings");
        eprintln!("{}: {}", car.name, s.describe().join("; "));
        assert!(rsf.is_on("/rsf") && !rsf.is_on(""));
    }
}

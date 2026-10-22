//! The quick race's ghost car: the game's best and saved ghosts, and RSF's in an RSF race.

use std::path::{Path, PathBuf};

use rbr_formats::rsf::ghosts::{Condition, Gch, Key as GhostKey, Kind as GhostKind};
use rbr_gameplay::weather::Weather;

use crate::catalog::{self, Car, Map};
use crate::Form;

/// The ghost car to race in a quick race: none, the best one kept per map, a saved ghost of `./GhostCars/`, or in an
/// RSF race (an RSF car or stage) one of RSF's kinds ([`rsf_ghost_args`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GhostChoice {
    #[default]
    Off,
    Best,
    File(String),
    Rsf(GhostKind),
}

/// RSF's ghost kinds, as the settings name them.
const RSF_GHOSTS: [(GhostKind, &str); 4] = [
    (GhostKind::Overall, "rsf-overall"),
    (GhostKind::Car, "rsf-car"),
    (GhostKind::Group, "rsf-class"),
    (GhostKind::Latest, "rsf-latest"),
];

/// Where the ghosts are, in the player's folder (the game's `./GhostCars/`).
pub const GHOST_DIR: &str = "GhostCars";
/// The best ghost's name (ours: the game names a ghost when the player saves it).
pub const BEST_GHOST: &str = "Best";

impl GhostChoice {
    pub fn parse(text: &str) -> GhostChoice {
        match text {
            "" | "off" => GhostChoice::Off,
            "best" => GhostChoice::Best,
            rsf if RSF_GHOSTS.iter().any(|(_, n)| *n == rsf) => {
                GhostChoice::Rsf(RSF_GHOSTS.iter().find(|(_, n)| *n == rsf).unwrap().0)
            }
            file => GhostChoice::File(file.to_owned()),
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            GhostChoice::Off => "off".to_owned(),
            GhostChoice::Best => "best".to_owned(),
            GhostChoice::File(file) => file.clone(),
            GhostChoice::Rsf(kind) => RSF_GHOSTS.iter().find(|(k, _)| k == kind).unwrap().1.to_owned(),
        }
    }

    /// The RSF kind the choice plays in an RSF race: the best one is RSF's overall best for the map and condition
    /// (`<map>/<condition>/<map>.gch`, which a finished drive also writes).
    pub fn rsf_kind(&self) -> Option<GhostKind> {
        match self {
            GhostChoice::Best => Some(GhostKind::Overall),
            GhostChoice::Rsf(kind) => Some(*kind),
            GhostChoice::Off | GhostChoice::File(_) => None,
        }
    }

    /// The viewer's arguments for map `map`: `--best-ghost` with the weather words a new best keeps, or `--ghost` with
    /// the file (none when the file is gone).
    pub fn viewer_args(&self, here: &Path, map: u32, weather: &rbr_formats::ghost::Weather) -> Vec<String> {
        let dir = here.join(GHOST_DIR);
        match self {
            GhostChoice::Off => Vec::new(),
            GhostChoice::Best => vec![
                "--best-ghost".into(),
                dir.join(rbr_formats::ghost::SavedGhost::file_name(BEST_GHOST, map)).display().to_string(),
                "--ghost-weather".into(),
                rbr_render::ghost::weather_arg(weather),
            ],
            GhostChoice::File(file) if dir.join(file).is_file() => {
                vec!["--ghost".into(), dir.join(file).display().to_string()]
            }
            GhostChoice::File(_) | GhostChoice::Rsf(_) => Vec::new(),
        }
    }
}

/// The key RSF keeps a race's ghosts under (`rbr_formats::rsf::ghosts`), when `car` or `map` is RSF's: the map, the
/// condition of `wetness`, and the RSF car's id and group (none for a game car on an RSF stage; ours). An added car
/// is not RSF's: its id is ours.
pub fn rsf_ghost_key(car: &Car, map: &Map, wetness: u32) -> Option<GhostKey> {
    let rsf = car.rsf.as_ref().filter(|r| !r.added);
    (rsf.is_some() || map.rsf.is_some()).then(|| GhostKey {
        map: map.number,
        condition: Condition::from_wetness(wetness),
        car: rsf.map(|r| r.id),
        group: rsf.and_then(|r| r.group.clone()),
    })
}

/// The `GhostCars` folders RSF ghosts are looked up in: ours under `here`, then the RSF install's (read only).
fn rsf_ghost_roots(here: &Path, rsf_root: &Path) -> Vec<PathBuf> {
    let ours = here.join(GHOST_DIR);
    let install = (!rsf_root.as_os_str().is_empty()).then(|| rsf_root.join(GHOST_DIR));
    [ours].into_iter().chain(install).collect()
}

/// The RSF ghosts there are for `key`: each kind's pick and its header.
pub fn rsf_ghosts(here: &Path, rsf_root: &Path, key: &GhostKey) -> Vec<(GhostKind, PathBuf, Gch)> {
    let roots = rsf_ghost_roots(here, rsf_root);
    let roots: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    GhostKind::ALL.into_iter().filter_map(|kind| key.pick(&roots, kind).map(|(p, g)| (kind, p, g))).collect()
}

/// The viewer's ghost arguments in an RSF race: the finished drive is kept in RSF's layout under `here`
/// (`--rsf-ghosts`, with the weather words for its condition), and `choice`, an RSF kind, plays its pick (`--ghost`).
pub fn rsf_ghost_args(
    choice: &GhostChoice,
    here: &Path,
    rsf_root: &Path,
    key: &GhostKey,
    weather: &rbr_formats::ghost::Weather,
) -> Vec<String> {
    let mut args = vec![
        "--rsf-ghosts".into(),
        here.join(GHOST_DIR).display().to_string(),
        "--ghost-weather".into(),
        rbr_render::ghost::weather_arg(weather),
    ];
    if let Some(kind) = choice.rsf_kind() {
        let picked = rsf_ghosts(here, rsf_root, key).into_iter().find(|(k, _, _)| *k == kind);
        if let Some((_, path, _)) = picked {
            args.extend(["--ghost".into(), path.display().to_string()]);
        }
    }
    args
}

/// The weather words of the saved ghost `file` in `./GhostCars/` (its header only), or `None` when it does not read.
pub fn ghost_words(here: &Path, file: &str) -> Option<rbr_formats::ghost::Weather> {
    use std::io::Read;
    let mut header = Vec::new();
    let file = std::fs::File::open(here.join(GHOST_DIR).join(file)).ok()?;
    file.take(rbr_formats::ghost::SAVED_HEADER_LEN as u64).read_to_end(&mut header).ok()?;
    rbr_formats::ghost::SavedGhost::header_weather_words(&header)
}

/// The saved ghosts of map `map` in `./GhostCars/` (`<name>_<map:02>.ghc`, the best one too), sorted.
pub fn ghost_files(here: &Path, map: u32) -> Vec<String> {
    let suffix = format!("_{map:02}.ghc");
    let mut files: Vec<String> = std::fs::read_dir(here.join(GHOST_DIR))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|f| f.ends_with(&suffix))
        .collect();
    files.sort();
    files
}

impl Form {
    /// The stage and weather of the chosen saved ghost: loading a `.ghc` sets the race's weather from its words
    /// (`E18_ConnGhostMemory::Dispatch` 0x4930d0). `None` without a file ghost, or when its words fit no season row of
    /// the map (then the Weather field decides).
    pub fn ghost_weather(&self) -> Option<(catalog::Stage, Weather)> {
        let GhostChoice::File(file) = &self.ghost else {
            return None;
        };
        let words = ghost_words(&self.records(), file)?;
        self.catalog.ghost_stage_weather(&self.season, self.catalog.maps[self.map].number, &words)
    }

    /// The key RSF keeps this race's ghosts under, when the car or the stage is RSF's.
    pub fn rsf_ghost_key(&self, weather: &Weather) -> Option<rbr_formats::rsf::ghosts::Key> {
        rsf_ghost_key(self.car()?, &self.catalog.maps[self.map], weather.wetness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::RsfCar;

    #[test]
    fn a_saved_ghost_lists_its_weather_words() {
        let here = std::env::temp_dir().join(format!("rbr-launcher-ghost-{}", std::process::id()));
        std::fs::create_dir_all(here.join(GHOST_DIR)).unwrap();
        let weather =
            rbr_formats::ghost::Weather { track_letter: 3, cloud: 2, wetness: 2, wear: 1, ..Default::default() };
        let ghost = rbr_formats::ghost::SavedGhost { map: 54, car: 6, weather, stream: Default::default() };
        std::fs::write(here.join(GHOST_DIR).join("me_54.ghc"), ghost.write()).unwrap();
        assert_eq!(ghost_words(&here, "me_54.ghc"), Some(weather));
        assert_eq!(ghost_words(&here, "gone_54.ghc"), None);
        assert_eq!(ghost_files(&here, 54), ["me_54.ghc"]);
        std::fs::remove_dir_all(&here).unwrap();
    }

    /// An RSF race keeps its drive in RSF's layout and races the chosen kind's pick, ours or the install's.
    #[test]
    fn an_rsf_race_picks_rsf_ghosts() {
        use rbr_formats::rsf::ghosts::GchSample;
        let here = std::env::temp_dir().join(format!("rbr-launcher-rsf-ghost-{}", std::process::id()));
        let install = here.join("rsf");
        let rsf_car = RsfCar {
            id: 88,
            added: false,
            group: Some("Rally 3".into()),
            pack: Ok("/pack".into()),
            model: Ok(None),
            drive_train: None,
        };
        let car = Car { index: 5, name: "Ford Fiesta Rally3".into(), rsf: Some(rsf_car) };
        let map = Map { number: 450, stages: Vec::new(), rsf: None };
        let key = rsf_ghost_key(&car, &map, 1).unwrap();
        assert_eq!((key.condition, key.car, key.group.as_deref()), (Condition::Damp, Some(88), Some("Rally 3")));
        let retail = Car { index: 6, name: "COROLLA".into(), rsf: None };
        assert_eq!(rsf_ghost_key(&retail, &map, 0), None, "a game car on a game stage keeps the game's ghosts");
        let drive =
            |car, finish| Gch { map: 450, slot: 5, car, finish, samples: vec![GchSample::default()], ..Gch::default() };
        key.store(&here.join(GHOST_DIR), &drive(88, 330.0)).unwrap();
        GhostKey { car: Some(9), group: Some("WRC 1.6".into()), ..key.clone() }
            .store(&install.join(GHOST_DIR), &drive(9, 311.0))
            .unwrap();
        let kinds: Vec<_> = rsf_ghosts(&here, &install, &key).iter().map(|(k, _, g)| (*k, g.car)).collect();
        // The latest is the newest file of the two folders: the install's here.
        assert_eq!(
            kinds,
            [(GhostKind::Overall, 9), (GhostKind::Car, 88), (GhostKind::Group, 88), (GhostKind::Latest, 9)]
        );
        let choice = GhostChoice::parse("rsf-overall");
        assert_eq!(choice.to_text(), "rsf-overall");
        let args = rsf_ghost_args(&choice, &here, &install, &key, &Default::default());
        assert_eq!(args[..2], ["--rsf-ghosts".to_owned(), here.join(GHOST_DIR).display().to_string()]);
        let played = install.join(GHOST_DIR).join("450").join("Damp").join("450.gch");
        assert_eq!(args[4..], ["--ghost".to_owned(), played.display().to_string()]);
        assert_eq!(rsf_ghost_args(&GhostChoice::Off, &here, &install, &key, &Default::default()).len(), 4);
        // The best ghost of a game race carried over to an RSF race plays RSF's overall best.
        let best = rsf_ghost_args(&GhostChoice::Best, &here, &install, &key, &Default::default());
        assert_eq!(best[4..], ["--ghost".to_owned(), played.display().to_string()]);
        std::fs::remove_dir_all(&here).unwrap();
    }
}

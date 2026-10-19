//! The launcher's cars and maps. A map's good or bad quick-race weather, from the 1.02 exe's season table, chooses
//! its stage variant and the wetness used by the viewer. Maps without a good or bad row cannot start.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use rbr_formats::{cars, rbz, rsf, stage_files};
use rbr_gameplay::weather::{self, Preset, Season, Weather};

/// Each car number's folder in `physics.rbz` (the exe's table at 0x7c18d4).
const PHYSICS_DIRS: [&str; 8] = ["c_xsara", "h_accent", "mg_zr", "m_lancer", "p_206", "s_i2003", "t_coroll", "s_i2000"];

/// The car numbers NGP's `physics.rbz` at `archive` has NGP cars in: the slots whose `common.lsp` sets NGP's keys
/// (`*_NGP`); the others keep the stock data, which NGP's physics does not drive. NGP 7.4.778's own package has none
/// (its stock car files are retail's); RallySimFans' archive and our test archive (`ngp-data/physics.rbz`, NGP cars
/// installed into slots 2, 4, 5 and 6) have the slots an NGP car was installed into.
pub fn ngp_cars(archive: &Path) -> Vec<u32> {
    let Ok(mut zip) = rbz::open(archive) else { return Vec::new() };
    (0..8u32)
        .filter(|&car| {
            let mut text = String::new();
            let name = format!("Physics/{}/common.lsp", PHYSICS_DIRS[car as usize]);
            rbz::by_name(&mut zip, &name).is_ok_and(|mut file| file.read_to_string(&mut text).is_ok())
                && is_ngp_car(&text)
        })
        .collect()
}

/// Whether a car's `common.lsp` is NGP's: it sets NGP's keys.
fn is_ngp_car(common: &str) -> bool {
    common.contains("_NGP")
}

/// The cars NGP drives, of `cars` (the retail ones, the RSF ones and the added ones): the retail slots `ngp_cars` has
/// NGP data for, and the RSF and added cars. With RSF's cars, the retail slot 5 is left out: in RallySimFans' archive
/// it is the RSF car last picked, which the list already has.
pub fn ngp_offered(cars: &[Car], ngp_cars: &[u32]) -> Vec<Car> {
    let rsf = cars.iter().any(|c| c.rsf.as_ref().is_some_and(|r| !r.added));
    let offered = |c: &&Car| c.rsf.is_some() || ngp_cars.contains(&c.index) && !(rsf && c.index == rsf::SLOT);
    cars.iter().filter(offered).cloned().collect()
}

/// A car of `Cars/Cars.ini`, or an RSF or added car raced in RSF's slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Car {
    /// Its game's car number (`rbr-standalone --car`).
    pub index: u32,
    /// Its folder, upper case (`viewer --car`); an RSF car's name.
    pub name: String,
    pub rsf: Option<RsfCar>,
}

/// An RSF car of the cache (`rbr_formats::rsf`), or a car pack the player added (`car_packs`, laid out as RSF's):
/// `rbr-standalone` and the viewer read its pack (`--rsf-car`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsfCar {
    /// RSF's id; an added car's is ours, not RSF's.
    pub id: u32,
    /// Added from a car pack, not RSF's.
    pub added: bool,
    pub group: Option<String>,
    /// Its physics pack, or why there is none.
    pub pack: Result<PathBuf, String>,
    /// The model it is drawn with: its own (`None`), a stand-in (why), or none (why).
    pub model: Result<Option<String>, String>,
}

impl Car {
    /// Why the car can't start (an RSF car needs its pack and a model), or `None`.
    pub fn unavailable(&self) -> Option<String> {
        let rsf = self.rsf.as_ref()?;
        match (&rsf.pack, &rsf.model) {
            (Err(e), _) | (_, Err(e)) => Some(e.clone()),
            _ => None,
        }
    }
}

/// The cars of the RSF install at `root` (its cache's `cars.json`; the added cars' folder, `added`), in RSF's slot,
/// their models checked against the install's `Cars.ini` (`install`) for the retail stand-ins.
pub fn rsf_cars(install: &Path, root: &Path, added: bool) -> Result<Vec<Car>, String> {
    let rsf = rsf::Install::open(root)?;
    let cars_ini = install.join("Cars").join("Cars.ini");
    let text = std::fs::read(&cars_ini).map_err(|e| format!("{}: {e}", cars_ini.display()))?;
    let retail = cars::parse(&String::from_utf8_lossy(&text)).map_err(|e| e.to_string())?;
    Ok(rsf
        .cars
        .iter()
        .map(|car| {
            // The pack's files must be plain: `common.lsp` stands for them (we never decrypt RSF-protected files).
            let pack = match rsf.pack(car) {
                Ok(p) => p.read("common.lsp").map(|_| p.path().to_owned()),
                Err(_) => Err("no physics pack in rsfdata/cars".into()),
            };
            let model = rsf.model_entry(car, rsf::SLOT, &retail).map(|m| m.fallback);
            Car {
                index: rsf::SLOT,
                name: car.name.clone(),
                rsf: Some(RsfCar { id: car.id, added, group: car.group.clone(), pack, model }),
            }
        })
        .collect())
}

/// A stage variant, `track-NN_X` (a community stage's: the path of its files, `rbr_formats::stage_files`), loaded for
/// the weather selected by the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub name: String,
    pub map: u32,
    pub letter: char,
}

/// One selectable map and the stage variants installed for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Map {
    pub number: u32,
    pub stages: Vec<Stage>,
    /// A RallySimFans stage folder's map ([`Catalog::set_rsf`]).
    pub rsf: Option<rsf::Stage>,
}

impl Map {
    /// The season that gives the map's weathers: the game's, or a community stage's own (`Season::of_rsf`).
    pub fn season<'a>(&self, season: &'a Season) -> Cow<'a, Season> {
        match &self.rsf {
            Some(stage) => Cow::Owned(Season::of_rsf(stage, None)),
            None => Cow::Borrowed(season),
        }
    }

    /// The game's surface number of the map (0 gravel, 1 tarmac, 2 snow): [`surface_kind`], or the community stage's.
    pub fn surface(&self) -> u32 {
        match self.rsf.as_ref().map(|s| s.surface) {
            Some(Some(rsf::Surface::Gravel)) => 0,
            Some(Some(rsf::Surface::Tarmac)) => 1,
            Some(Some(rsf::Surface::Snow)) => 2,
            _ => surface_kind(self.number),
        }
    }
}

/// The tyre wear's names, new to worn (the settings file, `rbr-standalone --wear`, the Drive page).
pub const WEARS: [&str; 3] = ["new", "normal", "worn"];

/// Whether to use the game's computed tyre, or a tyre selected by the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TyreChoice {
    Auto,
    Fixed(u32),
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// The cars offered: those of `all_cars` NGP drives ([`Catalog::offer`]).
    pub cars: Vec<Car>,
    /// The retail cars and the RSF ones.
    pub all_cars: Vec<Car>,
    pub maps: Vec<Map>,
}

impl Catalog {
    /// Reads the install's `Cars/Cars.ini` and groups its `Maps/track-NN_X.lbs` files by map number.
    pub fn scan(install: &Path) -> Result<Catalog, String> {
        let cars_ini = install.join("Cars").join("Cars.ini");
        let text = std::fs::read(&cars_ini).map_err(|e| format!("{}: {e}", cars_ini.display()))?;
        let cars = cars::parse(&String::from_utf8_lossy(&text))
            .map_err(|e| e.to_string())?
            .iter()
            .filter_map(|e| {
                Some(Car { index: e.index as u32, name: car_folder(&e.file_name)?.to_uppercase(), rsf: None })
            })
            .collect();

        let maps_dir = install.join("Maps");
        let mut maps: BTreeMap<u32, Vec<Stage>> = BTreeMap::new();
        for file in file_names(&maps_dir) {
            let Some(name) = file.strip_suffix(".lbs") else {
                continue;
            };
            let Some((number, letter)) = stage_parts(name) else {
                continue;
            };
            maps.entry(number).or_default().push(Stage { name: name.to_owned(), map: number, letter });
        }
        let maps = maps
            .into_iter()
            .map(|(number, mut stages)| {
                stages.sort_by_key(|s| s.letter);
                Map { number, stages, rsf: None }
            })
            .collect();
        Ok(Catalog { cars: Vec::new(), all_cars: cars, maps })
    }

    /// Replaces the RSF stages by the RallySimFans stage folders of `root` (`<root>/Maps/<id>-<name>/`,
    /// `rbr_formats::rsf`; none when it is empty), after the retail maps; one that does not load stays listed with its
    /// reason ([`Catalog::availability`]). A folder whose `Tracks<id>.ini` does not read, or whose id a retail map has,
    /// is reported and left out.
    pub fn set_rsf(&mut self, root: &str) {
        self.maps.retain(|m| m.rsf.is_none());
        if root.trim().is_empty() {
            return;
        }
        for stage in rsf::scan(Path::new(root.trim())) {
            let stage = match stage {
                Ok(stage) => stage,
                Err((folder, e)) => {
                    eprintln!("launcher: RSF {}: {e}", folder.display());
                    continue;
                }
            };
            if self.maps.iter().any(|m| m.number == stage.id) {
                eprintln!("launcher: RSF stage {} has a retail map's number; left out", stage.id);
                continue;
            }
            let stages =
                stage.variants.iter().map(|&l| Stage { name: stage.stage(l), map: stage.id, letter: l }).collect();
            self.maps.push(Map { number: stage.id, stages, rsf: Some(stage) });
        }
    }

    /// Replaces the RSF cars by those of the RSF folder `dir` (none, with the reason on stderr, when it doesn't read;
    /// none for an empty `dir`), then the added cars of `added` (`car_packs::DIR`) that RSF has not (by pack folder
    /// or name: RSF's copy is kept).
    pub fn set_rsf_cars(&mut self, install: &Path, dir: &str, added: &Path) {
        self.all_cars.retain(|c| c.rsf.is_none());
        if !dir.trim().is_empty() {
            match rsf_cars(install, Path::new(dir.trim()), false) {
                Ok(cars) => self.all_cars.extend(cars),
                Err(e) => eprintln!("launcher: RSF cars: {e}"),
            }
        }
        if !added.join("rsfdata").is_dir() {
            return;
        }
        let pack_name = |c: &Car| {
            let pack = c.rsf.as_ref()?.pack.as_ref().ok()?;
            Some(pack.file_name()?.to_string_lossy().trim_end_matches(".zip").to_lowercase())
        };
        match rsf_cars(install, added, true) {
            Ok(cars) => {
                let rsf: Vec<(Option<String>, String)> = self
                    .all_cars
                    .iter()
                    .filter(|c| c.rsf.is_some())
                    .map(|c| (pack_name(c), c.name.to_lowercase()))
                    .collect();
                let new = |c: &Car| {
                    let (pack, name) = (pack_name(c), c.name.to_lowercase());
                    !rsf.iter().any(|(p, n)| (p.is_some() && *p == pack) || *n == name)
                };
                self.all_cars.extend(cars.into_iter().filter(new));
            }
            Err(e) => eprintln!("launcher: added cars: {e}"),
        }
    }

    /// Offers the cars NGP drives, with the archive's NGP slots `ngp_cars` ([`ngp_offered`]).
    pub fn offer(&mut self, ngp_cars: &[u32]) {
        self.cars = ngp_offered(&self.all_cars, ngp_cars);
    }

    /// Resolves the game's selected row (the random weather's from `draws`) to the installed stage variant and its
    /// wetness.
    pub fn stage_weather(
        &self,
        season: &Season,
        map: u32,
        preset: Preset,
        draws: weather::Draws,
    ) -> Result<(Stage, Weather), String> {
        let entry = self.maps.iter().find(|m| m.number == map);
        if let Some(problem) = entry.and_then(|m| m.rsf.as_ref()?.problem.as_ref()) {
            return Err(problem.clone());
        }
        let season = entry.map_or(Cow::Borrowed(season), |m| m.season(season));
        let weather = weather::quick_race(&season, map, preset, draws)?;
        let stage = self.stage_of(map, &weather).ok_or_else(|| {
            format!("{} selected by {} weather has no installed stage file", weather.stage(), weather::name(preset))
        })?;
        Ok((stage, weather))
    }

    /// The stage and weather a saved ghost's weather `words` bring on `map` (`E18_ConnGhostMemory::Dispatch` sets the
    /// weather from the file; `Season::ghost_weather`), or `None` when the map's season has no row for them or the
    /// stage is not installed.
    pub fn ghost_stage_weather(
        &self,
        season: &Season,
        map: u32,
        words: &rbr_formats::ghost::Weather,
    ) -> Option<(Stage, Weather)> {
        let entry = self.maps.iter().find(|m| m.number == map)?;
        if entry.rsf.as_ref().is_some_and(|r| r.problem.is_some()) {
            return None;
        }
        let weather = entry.season(season).ghost_weather(map, words)?;
        Some((self.stage_of(map, &weather)?, weather))
    }

    /// The installed stage of `map` that `weather` loads (its letter).
    pub fn stage_of(&self, map: u32, weather: &Weather) -> Option<Stage> {
        let entry = self.maps.iter().find(|m| m.number == map);
        let name = weather.stage();
        entry
            .and_then(|m| {
                m.stages
                    .iter()
                    .find(|s| s.map == map && s.letter == weather.letter_char() && stage_files::name(&s.name) == name)
            })
            .cloned()
    }

    /// The map is startable only if both original presets select installed stage variants.
    pub fn availability(&self, season: &Season, map: &Map) -> Result<(), String> {
        let mut reasons = Vec::new();
        for preset in [Preset::Good, Preset::Bad] {
            if let Err(reason) = self.stage_weather(season, map.number, preset, weather::Draws::default()) {
                reasons.push(reason);
            }
        }
        reasons.dedup();
        if reasons.is_empty() {
            Ok(())
        } else {
            Err(reasons.join("; "))
        }
    }

    pub fn startable_maps<'a>(&'a self, season: &Season) -> Vec<&'a Map> {
        self.maps.iter().filter(|map| self.availability(season, map).is_ok()).collect()
    }
}

/// The file names in `dir`, empty when it does not read.
fn file_names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect())
        .unwrap_or_default()
}

/// The folder of a `Cars.ini` model path (`Cars\\xsara\\xsara.sgc` gives `xsara`).
fn car_folder(dos_path: &str) -> Option<&str> {
    dos_path.split(['\\', '/']).rev().nth(1)
}

/// `track-54_E` gives map 54, variant `E`.
pub fn stage_parts(name: &str) -> Option<(u32, char)> {
    let (number, letter) = name.strip_prefix("track-")?.split_once('_')?;
    let mut letters = letter.chars();
    match (number.parse().ok(), letters.next(), letters.next()) {
        (Some(map), Some(l), None) if l.is_ascii_uppercase() => Some((map, l)),
        _ => None,
    }
}

/// The game's surface number (`E14_4a1330`): 0 gravel, 1 tarmac, 2 snow.
pub fn surface_kind(map: u32) -> u32 {
    match map {
        10..=18 => 2,
        0x29..=0x31 | 0x49 | 0x55 | 0x56 | 0x5e => 1,
        _ => 0,
    }
}

pub fn tyre_label(tyre: u32) -> &'static str {
    match tyre {
        0 => "Tarmac dry",
        1 => "Tarmac damp",
        2 => "Tarmac wet",
        3 => "Gravel dry",
        4 => "Gravel damp",
        5 => "Gravel wet",
        6 => "Snow",
        _ => "Unknown tyre",
    }
}

pub fn map_label(map: &Map) -> String {
    if let Some(stage) = &map.rsf {
        return format!("{} {} (RSF)", map.number, stage.name);
    }
    match title(map.number) {
        Some(name) => format!("{} {name}", map.number),
        None => map.number.to_string(),
    }
}

/// The stage's name in the game: `E52ModeHost::M4a1020` (0x4a1020), queried by map number.
pub fn title(map: u32) -> Option<&'static str> {
    Some(match map {
        10 => "Kaihuavaara",
        11 => "Mustaselka",
        12 => "Sikakama",
        13 => "Autiovaara",
        14 => "Kaihuavaara II",
        15 => "Mustaselka II",
        20 => "Harwood Forest",
        21 => "Falstone",
        22 => "Chirdonhead",
        23 => "Shepherds Shield",
        24 => "Harwood Forest II",
        25 => "Chirdonhead II",
        31 => "NewBobs",
        32 => "Greenhills",
        33 => "Mineshaft",
        34 => "East-West",
        35 => "NewBobs II",
        36 => "East-West II",
        41 => "Cote D'Arbroz",
        42 => "Joux Verte",
        43 => "Bisanne",
        44 => "Joux Plane",
        45 => "Joux Verte II",
        46 => "Cote D'Arbroz II",
        51 => "Noiker",
        52 => "Sipirkakim",
        53 => "Pirka Menoko",
        54 => "Tanner",
        55 => "Noiker II",
        56 => "Tanner II",
        61 => "Fraizer Wells",
        62 => "Prospect Ridge",
        63 => "Diamond Creek",
        64 => "Hualapai Nation",
        65 => "Prospect Ridge II",
        66 => "Diamond Creek II",
        71 | 74..=77 | 79 | 80 | 82 | 87 => "[RSSTAGE]",
        72 | 73 | 78 | 81 | 83..=86 | 88 | 89 => "[RALLYYARD]",
        18 | 28 | 39 | 49 | 59 | 69 => return None,
        _ => "[HOMEBASE]",
    })
}

pub fn car_label(car: &Car) -> String {
    match &car.rsf {
        Some(rsf) if rsf.added => format!("{} ({}, added)", car.name, rsf.group.as_deref().unwrap_or("no group")),
        Some(rsf) => format!("{} ({}, RSF {})", car.name, rsf.group.as_deref().unwrap_or("no group"), rsf.id),
        None => format!("{} ({})", car.name, car.index),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NGP cars: none in the retail archive (`RBR_DATA`), 2, 4, 5 and 6 in our test archive (`RBR_NGP_PHYSICS`, NGP
    /// cars installed into those slots), and slot 5 only in a RallySimFans install, whose loose `Physics/` is its
    /// archive's tree (`RBR_RSF`).
    #[test]
    fn ngp_cars_come_from_the_archive() {
        let Some(data) = rbr_formats::data::Data::from_env() else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        assert_eq!(ngp_cars(&data.resolve("Root/physics.rbz").expect("retail physics.rbz")), Vec::<u32>::new());
        if let Some(ngp) = std::env::var_os("RBR_NGP_PHYSICS") {
            assert_eq!(ngp_cars(Path::new(&ngp)), [2, 4, 5, 6]);
        }
        if let Some(rsf) = std::env::var_os("RBR_RSF") {
            let common = |dir: &str| std::fs::read_to_string(Path::new(&rsf).join(format!("Physics/{dir}/common.lsp")));
            let slots: Vec<u32> =
                (0..8).filter(|&c| common(PHYSICS_DIRS[c as usize]).is_ok_and(|t| is_ngp_car(&t))).collect();
            assert_eq!(slots, [5]);
        }
    }

    /// NGP's retail slots are offered, the others hidden; with RSF cars the retail slot 5 is too.
    #[test]
    fn the_offered_cars_are_the_ones_ngp_drives() {
        let retail = |index| Car { index, name: format!("RETAIL{index}"), rsf: None };
        let rsf = Car {
            index: rsf::SLOT,
            name: "RSF".into(),
            rsf: Some(RsfCar { id: 24, added: false, group: None, pack: Ok("/p".into()), model: Ok(None) }),
        };
        let cars: Vec<Car> = (0..8).map(retail).collect();
        let names = |cars: Vec<Car>| cars.into_iter().map(|c| c.name).collect::<Vec<_>>();
        assert_eq!(names(ngp_offered(&cars, &[2, 4, 5, 6])), ["RETAIL2", "RETAIL4", "RETAIL5", "RETAIL6"]);
        assert!(ngp_offered(&cars, &[]).is_empty());
        let with_rsf: Vec<Car> = cars.into_iter().chain([rsf]).collect();
        assert_eq!(names(ngp_offered(&with_rsf, &[2, 4, 5, 6])), ["RETAIL2", "RETAIL4", "RETAIL6", "RSF"]);
    }

    use rbr_formats::data::Data;

    /// A car both RSF and the added cars have (by pack folder or by name) is listed once, RSF's; the others are listed
    /// with their source. Needs `RBR_DATA` (its `Cars.ini`).
    #[test]
    fn an_added_car_rsf_has_is_listed_once() {
        let Some(install) = std::env::var_os("RBR_DATA").map(PathBuf::from) else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        let dir = std::env::temp_dir().join(format!("rbr-launcher-added-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let car = |id, name: &str, pack: &str| rsf::Car {
            id,
            name: name.into(),
            pack: pack.into(),
            group: None,
            model: 1,
            revision: String::new(),
            audio: None,
        };
        let model = rsf::Model { id: 1, name: "m".into(), path: "m".into(), filename: "m".into(), fallback: 5 };
        let make = |root: &Path, cars: &[rsf::Car]| {
            for c in cars {
                std::fs::create_dir_all(root.join("rsfdata/cars").join(&c.pack)).unwrap();
                std::fs::write(root.join("rsfdata/cars").join(&c.pack).join("common.lsp"), "x").unwrap();
            }
            crate::car_packs::write_list(root, cars, std::slice::from_ref(&model)).unwrap();
        };
        let (rsf_root, added) = (dir.join("rsf"), dir.join("cars"));
        make(
            &rsf_root,
            &[car(24, "Mitsubishi Lancer Evo IX N4", "Evo_IX_N4_ngp6"), car(81, "Ford Fiesta Rally2", "F")],
        );
        make(
            &added,
            &[
                car(1, "Evo IX N4 (gvrc)", "evo_ix_n4_NGP6"),
                car(2, "Ford Fiesta Rally2", "Ford_Fiesta_Rally2_ngp7"),
                car(3, "Alpine A110", "Alpine"),
            ],
        );
        let mut catalog = Catalog::default();
        catalog.set_rsf_cars(&install, &rsf_root.display().to_string(), &added);
        let listed: Vec<(String, bool)> =
            catalog.all_cars.iter().map(|c| (c.name.clone(), c.rsf.as_ref().unwrap().added)).collect();
        let want = [("Mitsubishi Lancer Evo IX N4", false), ("Ford Fiesta Rally2", false), ("Alpine A110", true)];
        assert_eq!(listed, want.map(|(n, a)| (n.to_owned(), a)));
        assert_eq!(car_label(&catalog.all_cars[2]), "Alpine A110 (no group, added)");
        // Without RSF the added ones; the retail slot 5 stays offered (it is not RSF's last pick).
        catalog.set_rsf_cars(&install, " ", &added);
        assert_eq!(catalog.all_cars.len(), 3);
        let retail = Car { index: 5, name: "RETAIL5".into(), rsf: None };
        catalog.all_cars.insert(0, retail);
        catalog.offer(&[5]);
        assert_eq!(catalog.cars.len(), 4);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_parse() {
        assert_eq!(car_folder("Cars\\Lancer_EVO_VII\\lancer_evo_vII.sgc"), Some("Lancer_EVO_VII"));
        assert_eq!(stage_parts("track-54_E"), Some((54, 'E')));
        assert_eq!(stage_parts("track-4_O"), Some((4, 'O')));
        assert_eq!(stage_parts("track-54_E_textures"), None);
        assert_eq!(stage_parts("Clouds"), None);
    }

    #[test]
    fn every_retail_map_starts_with_good_and_bad_or_has_a_reason() {
        let install = std::env::var_os("RBR_DATA").expect("RBR_DATA is required for the retail map test");
        let patch = std::env::var_os("RBR_PATCH").map(PathBuf::from);
        let install = PathBuf::from(install);
        let season = Season::load(&Data::new(&install, patch)).unwrap();
        let catalog = Catalog::scan(&install).unwrap();
        let startable = catalog.startable_maps(&season);
        let unavailable: Vec<(u32, String)> = catalog
            .maps
            .iter()
            .filter_map(|map| catalog.availability(&season, map).err().map(|why| (map.number, why)))
            .collect();
        println!("launcher: {}/{} retail maps startable", startable.len(), catalog.maps.len());
        for (map, reason) in &unavailable {
            println!("launcher: map {map}: {reason}");
        }
        assert_eq!(catalog.maps.len(), 58, "retail maps changed");
        assert_eq!(startable.len(), 57);
        assert_eq!(unavailable.len(), 1);
        assert_eq!(unavailable[0].0, 68);
        assert!(unavailable[0].1.contains("int3"));
    }

    #[test]
    fn every_random_row_of_a_startable_map_has_its_stage_installed() {
        let install = PathBuf::from(std::env::var_os("RBR_DATA").expect("RBR_DATA is required"));
        let patch = std::env::var_os("RBR_PATCH").map(PathBuf::from);
        let season = Season::load(&Data::new(&install, patch)).unwrap();
        let catalog = Catalog::scan(&install).unwrap();
        let missing: Vec<String> = catalog
            .startable_maps(&season)
            .iter()
            .flat_map(|map| season.rows_of(map.number).map(move |row| (map.number, row)))
            .filter_map(|(map, row)| {
                let draws = weather::Draws { row: Some(row), wear: 1 };
                catalog.stage_weather(&season, map, Preset::Random, draws).err()
            })
            .collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    /// Setting the RSF folder again leaves the same stages; clearing it leaves the retail ones.
    #[test]
    fn set_rsf_replaces_the_rsf_stages() {
        let (Some(install), Some(rsf)) = (std::env::var_os("RBR_DATA"), std::env::var("RBR_RSF").ok()) else {
            eprintln!("skipping: RBR_DATA or RBR_RSF not set");
            return;
        };
        let mut catalog = Catalog::scan(Path::new(&install)).unwrap();
        let retail = catalog.maps.len();
        catalog.set_rsf(&rsf);
        let with_rsf = catalog.maps.len();
        assert!(with_rsf > retail, "no RSF stages in {rsf}");
        catalog.set_rsf(&rsf);
        assert_eq!(catalog.maps.len(), with_rsf);
        catalog.set_rsf(" ");
        assert_eq!(catalog.maps.len(), retail);
    }
}

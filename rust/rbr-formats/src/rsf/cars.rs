//! RallySimFans (RSF) car installs: the NGP car packs and the car models the RSF plugin puts into the game's slots.
//!
//! What RSF does when a car is chosen (`notes/formats/rsf-cars.md`): it copies the car's physics pack
//! (`rsfdata/cars/<pack>/`, NGP keys in the retail `.lsp` layout) into one slot's physics folder (`Physics/s_i2003/`
//! for slot 5) and rewrites that slot's `[CarNN]` in `Cars/Cars.ini` to the car's model folder (`Cars/<model>/`, the
//! retail model layout), adding `CarName`, `RSFCarID` and `RSFCarPhysics`. The game then loads the slot as a retail
//! car. We read the pack and the model in place instead ([`Pack`], [`Install::model_entry`]); nothing is written.
//!
//! The car list comes from RSF's cache (`rsfdata/cache/`): `cars.json` (id, name, pack folder, model id, base group),
//! `carmodels.json` (model folder, file name, fallback) and `cargroups.json` (group names).

use crate::cars::{self, CarEntry};
use crate::data::Data;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The slot RSF puts the chosen car into (the sample install's `Cars.ini` and `Physics/s_i2003`): `[Car05]`.
pub const SLOT: u32 = 5;

/// A car of RSF's `cars.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Car {
    pub id: u32,
    pub name: String,
    /// The physics pack's folder name under `rsfdata/cars/` (`Mitsubishi_Lancer_Evo_IX_N4_ngp6`).
    pub pack: String,
    /// The base group's name (`Group N`), when `cargroups.json` has it.
    pub group: Option<String>,
    /// The `carmodels.json` id of its model.
    pub model: u32,
    /// `rev` (`8 / 2019-10-08`).
    pub revision: String,
    /// `audio`: its FMOD bank's name (`Lancer_Evo_IX_X`, `AudioFMOD/<audio>.bank`), when it has one.
    pub audio: Option<String>,
}

/// A model of RSF's `carmodels.json`: `Cars/<path>/<filename>.sgc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub filename: String,
    /// Below 8 a retail slot, else another model's id (ours, from the values: Evo IX falls back to 3, the Lancer
    /// Evo VII; the Focus 2006 to model 16). Used when the model folder is not installed.
    pub fallback: u32,
}

/// An RSF install's car list.
#[derive(Debug, Clone)]
pub struct Install {
    pub root: PathBuf,
    pub cars: Vec<Car>,
    pub models: Vec<Model>,
}

/// The model a car is drawn and collided with: its `Cars.ini` entry in [`SLOT`], and, when its own model folder is
/// not installed, why it falls back to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    pub entry: CarEntry,
    pub fallback: Option<String>,
}

/// A row of RSF's cache: an object of scalars, each kept as its text (`null` left out).
type Row = HashMap<String, String>;

/// RSF's cache file at `path`: an array of flat objects (the only shape its `cars`, `carmodels` and `cargroups` files
/// have).
fn json(path: &Path) -> Result<Vec<Row>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    rows(&text).ok_or(format!("{}: not an array of flat objects", path.display()))
}

/// Parses an array of flat JSON objects whose values are strings, numbers, booleans or `null`.
fn rows(text: &str) -> Option<Vec<Row>> {
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let rows = rows_of(&mut chars)?;
    space(&mut chars);
    chars.next().is_none().then_some(rows)
}

fn space(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.next_if(|c| c.is_whitespace()).is_some() {}
}

fn expect(chars: &mut std::iter::Peekable<std::str::Chars>, want: char) -> Option<()> {
    space(chars);
    (chars.next()? == want).then_some(())
}

fn string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    expect(chars, '"')?;
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'u' => {
                    let hex: String = (0..4).map(|_| chars.next()).collect::<Option<_>>()?;
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or('\u{fffd}'));
                }
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' | 'f' => {}
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
}

fn rows_of(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<Vec<Row>> {
    expect(chars, '[')?;
    let mut rows = Vec::new();
    space(chars);
    if chars.next_if_eq(&']').is_some() {
        return Some(rows);
    }
    loop {
        expect(chars, '{')?;
        let mut row = Row::new();
        space(chars);
        if chars.next_if_eq(&'}').is_none() {
            loop {
                let key = string(chars)?;
                expect(chars, ':')?;
                space(chars);
                if chars.peek() == Some(&'"') {
                    row.insert(key, string(chars)?);
                } else {
                    let word: String =
                        std::iter::from_fn(|| chars.next_if(|c| !matches!(c, ',' | '}') && !c.is_whitespace()))
                            .collect();
                    match word.as_str() {
                        "" | "{" | "[" => return None,
                        "null" => {}
                        _ => {
                            row.insert(key, word);
                        }
                    }
                }
                space(chars);
                match chars.next()? {
                    ',' => continue,
                    '}' => break,
                    _ => return None,
                }
            }
        }
        rows.push(row);
        space(chars);
        match chars.next()? {
            ',' => continue,
            ']' => return Some(rows),
            _ => return None,
        }
    }
}

fn text(row: &Row, key: &str) -> String {
    row.get(key).cloned().unwrap_or_default()
}

fn number(row: &Row, key: &str) -> Option<u32> {
    row.get(key)?.parse().ok()
}

impl Install {
    /// The RSF install at `root` (the game folder holding `Cars/` and `rsfdata/`).
    pub fn open(root: &Path) -> Result<Install, String> {
        let cache = root.join("rsfdata").join("cache");
        let groups: Vec<(u32, String)> = json(&cache.join("cargroups.json"))
            .unwrap_or_default()
            .iter()
            .filter_map(|g| Some((number(g, "id")?, text(g, "name"))))
            .collect();
        let cars = json(&cache.join("cars.json"))?
            .iter()
            .filter_map(|c| {
                let group = number(c, "base_group_id").and_then(|id| groups.iter().find(|g| g.0 == id));
                Some(Car {
                    id: number(c, "id")?,
                    name: text(c, "name"),
                    pack: text(c, "path"),
                    group: group.map(|g| g.1.clone()),
                    model: number(c, "carmodel_id")?,
                    revision: text(c, "rev"),
                    audio: c.get("audio").filter(|a| !a.is_empty()).cloned(),
                })
            })
            .collect();
        let models = json(&cache.join("carmodels.json"))?
            .iter()
            .filter_map(|m| {
                Some(Model {
                    id: number(m, "id")?,
                    name: text(m, "name"),
                    path: text(m, "path"),
                    filename: text(m, "filename"),
                    fallback: number(m, "fallback").unwrap_or(0),
                })
            })
            .collect();
        Ok(Install { root: root.to_owned(), cars, models })
    }

    /// The car `key` names: its id or its pack folder (ignoring case).
    pub fn find(&self, key: &str) -> Option<&Car> {
        let id: Option<u32> = key.parse().ok();
        self.cars.iter().find(|c| Some(c.id) == id || c.pack.eq_ignore_ascii_case(key))
    }

    /// The car whose pack is at `path` (`rsfdata/cars/<pack>` or `<pack>.zip`).
    pub fn find_pack(&self, path: &Path) -> Option<&Car> {
        let name = path.file_name()?.to_str()?;
        self.find(name.strip_suffix(".zip").unwrap_or(name))
    }

    /// Car `car`'s FMOD bank (`AudioFMOD/<audio>.bank`, `crate::fmod`), when it has one and it is installed.
    pub fn audio_bank(&self, car: &Car) -> Option<PathBuf> {
        cars::resolve(&self.root, &format!("AudioFMOD\\{}.bank", car.audio.as_deref()?))
    }

    /// Car `car`'s physics pack: the unpacked folder, else the zip.
    pub fn pack(&self, car: &Car) -> Result<Pack, String> {
        let cars = self.root.join("rsfdata").join("cars");
        cars::resolve(&cars, &car.pack)
            .or_else(|| cars::resolve(&cars, &format!("{}.zip", car.pack)))
            .ok_or(format!("{}: no pack in {}", car.pack, cars.display()))
            .and_then(|p| Pack::open(&p))
    }

    /// The model car `car` is loaded with in `slot`: its own when its folder is installed, else its fallback's (a
    /// retail car of `retail`, the install's `Cars.ini`), as `[CarNN]` paths under [`Install::root`] or the install.
    pub fn model_entry(&self, car: &Car, slot: u32, retail: &[CarEntry]) -> Result<ModelChoice, String> {
        let mut id = car.model;
        let mut why: Vec<String> = Vec::new();
        for _ in 0..8 {
            let model =
                self.models.iter().find(|m| m.id == id).ok_or(format!("car model {id}: not in carmodels.json"))?;
            let entry = model_entry(&model.path, &model.filename, slot);
            if let Some(path) = cars::resolve(&self.root, &entry.file_name) {
                let locked = path.parent().map_or(0, encrypted_files);
                if locked > 0 {
                    return Err(format!("Cars/{}: {locked} RSF-encrypted files", model.path));
                }
                let fallback = (!why.is_empty()).then(|| why.join(", "));
                return Ok(ModelChoice { entry, fallback });
            }
            why.push(format!("Cars/{} not installed", model.path));
            if model.fallback < 8 {
                let retail = retail
                    .iter()
                    .find(|e| e.index == model.fallback as usize)
                    .ok_or(format!("Cars.ini: no [Car{:02}]", model.fallback))?;
                why.push(format!("retail car {}", retail.index));
                return Ok(ModelChoice {
                    entry: CarEntry { index: slot as usize, ..retail.clone() },
                    fallback: Some(why.join(", ")),
                });
            }
            id = model.fallback;
        }
        Err(format!("car model {}: fallback loop", car.model))
    }
}

/// The car `key` names (a pack's path, or with `root` a car id or pack folder), its install and its pack. Without
/// `root` a pack path's install is the folder three up (`<root>/rsfdata/cars/<pack>`).
pub fn locate(key: &str, root: Option<&Path>) -> Result<(Install, Car, Pack), String> {
    let path = Path::new(key);
    if path.exists() {
        let pack = Pack::open(path)?;
        let derived = path.canonicalize().ok().and_then(|p| Some(p.parent()?.parent()?.parent()?.to_owned()));
        let root = root.map(Path::to_owned).or(derived).ok_or(format!("{key}: no RSF install above it"))?;
        let install = Install::open(&root)?;
        let car = install.find_pack(path).ok_or(format!("{key}: not in {}'s cars.json", root.display()))?.clone();
        return Ok((install, car, pack));
    }
    let root = root.ok_or(format!("{key}: not a pack path, and no RSF install (RBR_RSF) to find it in"))?;
    let install = Install::open(root)?;
    let car = install.find(key).ok_or(format!("{key}: not an id or pack in {}'s cars.json", root.display()))?.clone();
    let pack = install.pack(&car)?;
    Ok((install, car, pack))
}

/// An RSF car chosen for a race: the car, its pack, its model, and the data with the model in its slot.
#[derive(Debug, Clone)]
pub struct Selected {
    /// The RSF install's folder.
    pub root: PathBuf,
    pub car: Car,
    pub pack: Pack,
    pub model: ModelChoice,
    /// The skin RSF's launcher selected for the model ([`selected_skin`]), drawn over its own textures.
    pub skin: Option<String>,
    pub data: Data,
    /// Its FMOD bank ([`Install::audio_bank`]).
    pub audio_bank: Option<PathBuf>,
}

impl Selected {
    /// The model's folder (`evo_IX`), the viewer's car name.
    pub fn folder(&self) -> &str {
        self.model.entry.file_name.split(['\\', '/']).rev().nth(1).unwrap_or_default()
    }

    /// One line for the log: the car, its group, the pack and the model (with its fallback).
    pub fn describe(&self) -> String {
        format!(
            "rsf car {} {} ({}), pack {}, model {}{}{}",
            self.car.id,
            self.car.name,
            self.car.group.as_deref().unwrap_or("no group"),
            self.pack.path().display(),
            self.model.entry.file_name,
            self.model.fallback.as_deref().map(|f| format!(" (fallback: {f})")).unwrap_or_default(),
            self.skin.as_deref().map(|s| format!(", skin {s}")).unwrap_or_default()
        )
    }
}

/// Car `key` ([`locate`]) in `slot` of `data`: what RSF would install, read in place.
pub fn select(data: Data, key: &str, slot: u32, root: Option<&Path>) -> Result<Selected, String> {
    let (install, car, pack) = locate(key, root)?;
    let ini = data.resolve("Cars/Cars.ini").ok_or("Cars/Cars.ini: not found")?;
    let text = std::fs::read(&ini).map_err(|e| format!("{}: {e}", ini.display()))?;
    let retail = cars::parse(&String::from_utf8_lossy(&text)).map_err(|e| e.to_string())?;
    let model = install.model_entry(&car, slot, &retail)?;
    let mut data = data.with_car(install.root.clone(), model.entry.clone());
    let folder = model.entry.file_name.split(['\\', '/']).rev().nth(1).unwrap_or_default();
    let skin = match model.fallback {
        None => selected_skin(&install.root, folder),
        Some(_) => None,
    };
    if let Some((_, dir)) = &skin {
        data = data.with_skin(dir.clone());
    }
    let audio_bank = install.audio_bank(&car);
    Ok(Selected { root: install.root.clone(), car, pack, model, skin: skin.map(|(name, _)| name), data, audio_bank })
}

/// The skin RSF's launcher selected for model folder `folder` (`Cars/<folder>/.rsfconfig`, `[skin]`'s `folder`), and
/// its folder (`Cars/<folder>/skins/<skin>/`) when installed. Without a `.rsfconfig` the model's own `Textures\` are
/// drawn. Ours: RSF's plugin is not traced; the selected skin's files replacing the model's by name is inferred from
/// the samples (the Fiesta Rally3's skins hold 6 to 18 of its 18 textures, some its shader settings).
pub fn selected_skin(root: &Path, folder: &str) -> Option<(String, PathBuf)> {
    let config = cars::resolve(root, &format!("Cars\\{folder}\\.rsfconfig"))?;
    let text = std::fs::read(config).ok()?;
    let sections = crate::ini::parse(&String::from_utf8_lossy(&text)).ok()?;
    let skin = crate::car_parts::section(&sections, "skin")?.get("folder")?.trim().to_owned();
    let dir = cars::resolve(root, &format!("Cars\\{folder}\\skins\\{skin}"))?;
    dir.is_dir().then_some((skin, dir))
}

/// The car-parts option of model folder `model` that switches the headlights on: the first by folder name of
/// `carparts/<option>/` whose `metadata.ini` names it `Headlights` in `[rsf_info]` with an `on` word in its `state`
/// (`on`, `on+`, `All on`, `On, inner off`). RSF's picker copies the chosen option's files over the model's; we only
/// read this one's, for the headlights' colour (`notes/formats/rsf-cars.md`).
pub fn headlights_on(model: &Path) -> Option<PathBuf> {
    let mut options: Vec<PathBuf> =
        std::fs::read_dir(cars::resolve(model, "carparts")?).ok()?.filter_map(|e| Some(e.ok()?.path())).collect();
    options.sort();
    options.into_iter().find(|option| {
        option_info(option).is_some_and(|(name, state)| {
            name.eq_ignore_ascii_case("Headlights")
                && state.split(|c: char| !c.is_ascii_alphabetic()).any(|word| word.eq_ignore_ascii_case("on"))
        })
    })
}

/// A car-parts option's `name` and `state` (`metadata.ini`, `[rsf_info]`).
pub fn option_info(option: &Path) -> Option<(String, String)> {
    let text = std::fs::read(cars::resolve(option, "metadata.ini")?).ok()?;
    let sections = crate::ini::parse(&String::from_utf8_lossy(&text)).ok()?;
    let info = crate::car_parts::section(&sections, "rsf_info")?;
    Some((info.get("name")?.trim().to_owned(), info.get("state")?.trim().to_owned()))
}

/// Whether `bytes` start like an RSF-protected file (`01 a0 00 00`, `.ini`s `01 a1 00 00`; lead survey of the RSF
/// stages). We never decrypt them: a car with one is unloadable.
pub fn encrypted(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0x01, 0xa0 | 0xa1, 0, 0]))
}

/// The RSF-encrypted files under `dir`, counted by their first four bytes.
fn encrypted_files(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .map(|p| match p.is_dir() {
            true => encrypted_files(&p),
            false => {
                let mut head = [0; 4];
                let read = std::fs::File::open(&p).and_then(|mut f| f.read_exact(&mut head));
                usize::from(read.is_ok() && encrypted(&head))
            }
        })
        .sum()
}

/// The `[CarNN]` paths RSF writes for model folder `path` with file name `filename` (`Cars.ini`'s `[Car05]` in the
/// sample: `Cars\evo_IX\evo_IX.sgc`, `.ini`, `_shaders.ini`, `_shader_settings`, `Textures\`).
pub fn model_entry(path: &str, filename: &str, slot: u32) -> CarEntry {
    let dir = format!("Cars\\{path}\\");
    CarEntry {
        index: slot as usize,
        file_name: format!("{dir}{filename}.sgc"),
        ini_file: format!("{dir}{filename}.ini"),
        shader_file: format!("{dir}{filename}_shaders.ini"),
        shader_settings: format!("{dir}{filename}_shader_settings"),
        texture_path: format!("{dir}Textures\\"),
    }
}

/// An NGP physics pack (`rsfdata/cars/<pack>/` or `<pack>.zip`, whose entries sit in a `<pack>/` folder, some zips
/// with `\` separators): the files
/// RSF copies into a slot's `Physics/<dir>/`: `common.lsp`, `<surface>.lsp`, `r_<surface>.lsp`,
/// `setups/d_<surface>.lsp`, `repair.ini` and a text file named after the car.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pack {
    Dir(PathBuf),
    Zip(PathBuf),
}

impl Pack {
    pub fn open(path: &Path) -> Result<Pack, String> {
        if path.is_dir() {
            Ok(Pack::Dir(path.to_owned()))
        } else if path.is_file() {
            Ok(Pack::Zip(path.to_owned()))
        } else {
            Err(format!("{}: no pack there", path.display()))
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Pack::Dir(p) | Pack::Zip(p) => p,
        }
    }

    /// The pack's file `name` (`setups/d_gravel.lsp`), matched ignoring case.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let bytes = self.read_raw(name)?;
        match encrypted(&bytes) {
            true => Err(format!("{}/{name}: RSF-encrypted", self.path().display())),
            false => Ok(bytes),
        }
    }

    fn read_raw(&self, name: &str) -> Result<Vec<u8>, String> {
        let missing = || format!("{}: no {name}", self.path().display());
        match self {
            Pack::Dir(dir) => {
                let path = cars::resolve(dir, name).ok_or_else(missing)?;
                std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
            }
            Pack::Zip(path) => {
                let mut zip = crate::rbz::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
                let want = name.replace('\\', "/");
                let entry = (0..zip.len())
                    .find(|&k| {
                        zip.name_for_index(k)
                            .and_then(|n| {
                                Some(n.ok()?.replace('\\', "/").split_once('/')?.1.eq_ignore_ascii_case(&want))
                            })
                            .unwrap_or(false)
                    })
                    .ok_or_else(missing)?;
                let mut bytes = Vec::new();
                zip.by_index(entry)
                    .and_then(|mut f| f.read_to_end(&mut bytes).map_err(Into::into))
                    .map_err(|e| format!("{}/{name}: {e}", path.display()))?;
                Ok(bytes)
            }
        }
    }

    /// The pack's text file (named after the car): its first line, the car's full name.
    pub fn title(&self) -> Option<String> {
        let names: Vec<String> = match self {
            Pack::Dir(dir) => {
                std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()
            }
            Pack::Zip(path) => {
                let zip = crate::rbz::open(path).ok()?;
                zip.file_names()
                    .filter_map(|n| Some(n.ok()?.replace('\\', "/").split_once('/')?.1.to_owned()))
                    .collect()
            }
        };
        let known = |n: &str| [".lsp", ".ini"].iter().any(|e| n.to_ascii_lowercase().ends_with(e));
        let name = names.into_iter().find(|n| !n.is_empty() && !n.contains('/') && !known(n) && n != "setups")?;
        let text = self.read(&name).ok()?;
        Some(String::from_utf8_lossy(&text).lines().next()?.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_entry_is_the_slot_rsf_writes() {
        let e = model_entry("evo_IX", "evo_IX", 5);
        assert_eq!(e.index, 5);
        assert_eq!(e.file_name, "Cars\\evo_IX\\evo_IX.sgc");
        assert_eq!(e.shader_settings, "Cars\\evo_IX\\evo_IX_shader_settings");
        assert_eq!(e.texture_path, "Cars\\evo_IX\\Textures\\");
    }

    #[test]
    fn rsf_protected_files_are_told_by_their_magic() {
        assert!(encrypted(&[0x01, 0xa0, 0, 0, 0x5c]));
        assert!(encrypted(&[0x01, 0xa1, 0, 0]));
        assert!(!encrypted(b"((\"PhysicsEngine\""));
        assert!(!encrypted(&[0x01, 0xa0]));
    }

    #[test]
    fn cache_rows_are_flat_objects_of_scalars() {
        let read = rows(r#" [{"id":"24","name":"Evo \/ IX \u00e9","audio":null,"n":5},{}] "#).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(text(&read[0], "name"), "Evo / IX \u{e9}");
        assert_eq!(number(&read[0], "id"), Some(24));
        assert_eq!(number(&read[0], "n"), Some(5));
        assert!(!read[0].contains_key("audio"));
        assert!(rows(r#"[{"a":{"b":1}}]"#).is_none());
        assert!(rows("[1]").is_none());
    }

    /// Every pack of `RBR_RSF` (an RSF install, or the samples) reads, and each cache car finds its pack or says why.
    #[test]
    fn rsf_install_cars_and_packs() {
        let Some(root) = std::env::var_os("RBR_RSF") else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let install = Install::open(Path::new(&root)).unwrap();
        assert!(!install.cars.is_empty());
        let mut packs = 0;
        for car in &install.cars {
            let Ok(pack) = install.pack(car) else { continue };
            packs += 1;
            for file in ["common.lsp", "gravel.lsp", "tarmac.lsp", "snow.lsp", "setups/d_gravel.lsp", "r_gravel.lsp"] {
                pack.read(file).unwrap_or_else(|e| panic!("{}: {e}", car.pack));
            }
            assert!(pack.title().is_some(), "{}", car.pack);
            let zip = install.root.join("rsfdata").join("cars").join(format!("{}.zip", car.pack));
            if let (Pack::Dir(_), true) = (&pack, zip.is_file()) {
                let zip = Pack::open(&zip).unwrap();
                assert_eq!(zip.read("common.lsp").unwrap(), pack.read("common.lsp").unwrap(), "{}", car.pack);
                assert_eq!(zip.title(), pack.title(), "{}", car.pack);
            }
        }
        assert!(packs > 0, "no pack under rsfdata/cars");
    }

    /// The samples' Fiesta Rally3 has `.rsfconfig` selecting `Pajari_Monte`: its six textures come from that skin, the
    /// rest from the model's `Textures\`. The BMW M3 E30 has no `.rsfconfig` and keeps its own.
    #[test]
    fn the_selected_skin_draws_over_the_model() {
        let (Some(root), Some(data)) = (std::env::var_os("RBR_RSF"), Data::from_env()) else {
            eprintln!("skipping: RBR_RSF or RBR_DATA not set");
            return;
        };
        let root = Path::new(&root);
        let fiesta = select(data.clone(), "Ford_Fiesta_Rally3_ngp6", SLOT, Some(root)).unwrap();
        assert_eq!(fiesta.skin.as_deref(), Some("Pajari_Monte"));
        let dir = |path: &str| fiesta.data.resolve(path).unwrap().parent().unwrap().to_owned();
        let skin = cars::resolve(root, "Cars\\Fiesta_rally3\\skins\\Pajari_Monte\\textures").unwrap();
        assert_eq!(dir("Cars\\Fiesta_rally3\\Textures\\Fiesta_rally3_external.dds"), skin);
        let own = cars::resolve(root, "Cars\\Fiesta_rally3\\Textures").unwrap();
        assert_eq!(dir("Cars\\Fiesta_rally3\\Textures\\Fiesta_rally3_internal_parts.dds"), own);
        let bmw = select(data, "BMW_M3_E30_GrpA_ngp6", SLOT, Some(root)).unwrap();
        assert_eq!(bmw.skin, None);
    }

    /// Every car-parts option of the samples' models has its `[rsf_info]` name and state, and the lights-off models
    /// find their headlights' "on" option.
    #[test]
    fn car_parts_options_and_the_headlights_on() {
        let Some(root) = std::env::var_os("RBR_RSF") else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let cars = cars::resolve(Path::new(&root), "Cars").unwrap();
        let mut options = 0;
        for model in std::fs::read_dir(&cars).unwrap().map(|e| e.unwrap().path()) {
            let Some(parts) = cars::resolve(&model, "carparts") else { continue };
            for option in std::fs::read_dir(parts).unwrap().map(|e| e.unwrap().path()).filter(|p| p.is_dir()) {
                if cars::resolve(&option, "metadata.ini").is_some() {
                    assert!(option_info(&option).is_some(), "{}", option.display());
                    options += 1;
                }
            }
        }
        assert!(options > 0, "no car-parts options under {}", cars.display());
        for (model, on) in [
            ("FORD_FOCUS_2006", "lights_on"),
            ("fiesta_wrc_19", "headlights_on"),
            ("delta_4wd", "lights_on_all"),
            ("ESCORT_COSWORTH_GRA", "lights_on"),
            ("bmw_m3_e30", "Lights_On"),
            ("evo_IX", "light_on"),
        ] {
            let found = headlights_on(&cars::resolve(&cars, model).unwrap()).unwrap();
            assert_eq!(found.file_name().unwrap(), on, "{model}");
        }
    }
}

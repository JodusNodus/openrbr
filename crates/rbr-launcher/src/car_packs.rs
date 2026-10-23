//! Adding gvrc.de's NGP car packs, so that the cars drive without RallySimFans: "Add cars…" (the Drive page's Car row
//! and the Settings page) and `launcher --add-cars DIR` take a folder of downloads, and add every
//! `<Car> - NGP Physics.zip` in it with its `<Model> - Model.7z`.
//!
//! The cars go into `cars/` in the player's folder (`home`), laid out as an RSF install so that `rbr_formats::rsf` reads them:
//! - `cars/rsfdata/cars/<pack>/`: the physics pack, unpacked (the zip's `<pack>/` folder, else the download's name
//!   with `_` for spaces);
//! - `cars/Cars/<model>/`: the model, the `.7z`'s folder holding a `.sgc`;
//! - `cars/rsfdata/cache/cars.json` and `carmodels.json`: the list, written by us from the packs' text files (the
//!   name, the revision; ids of our own, so the launcher does not match them with RSF's ghosts or gearboxes).
//!
//! A pack's model is the download, or one added before, whose name starts the car's download name, the pack's title
//! or its "3D model" credit, the longest such (`Mitsubishi Lancer Evo IX - Model.7z` for the Evo IX N4 and R4). A
//! pack without one is added with RSF's stand-in for a missing model, a retail car's ([`STAND_IN`]). Adding a pack
//! again replaces it; models no car uses any more are removed. Nothing outside `cars/` is written; `gvrc` downloads
//! the packs and adds them through [`add_downloads`].

use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rbr_formats::{rbz, rsf};

use crate::disc_install::{Job, Progress, Temp};
use crate::pages::{GOOD, WARN};
use crate::{install, Form};

/// The folder in the player's folder with the added cars.
pub const DIR: &str = "cars";
/// The download names' ends: the physics pack and the model.
pub(crate) const PHYSICS: &str = " - NGP Physics.zip";
pub(crate) const MODEL: &str = " - Model.7z";
/// The retail car whose model a pack without one is drawn with (ours: RSF names one per model; the slot's own).
pub const STAND_IN: u32 = rsf::SLOT;
/// [`STAND_IN`]'s name, for the player.
const STAND_IN_NAME: &str = "Subaru Impreza 2003";

type Fail = String;

/// A physics pack download and the model download it goes with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub physics: PathBuf,
    pub model: Option<PathBuf>,
}

/// An added pack: its car's name, and its model folder or why it has none; or why it was not added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub download: String,
    pub result: Result<(String, Option<String>), Fail>,
}

/// `name` without the end `suffix`, ignoring case.
pub(crate) fn strip_end<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = name.len().checked_sub(suffix.len())?;
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(suffix)).then(|| &name[..cut])
}

/// `text` lower case, `_` as spaces, the spaces single.
pub(crate) fn words(text: &str) -> String {
    text.to_lowercase().replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The model of `models` (their names) whose name starts one of `names` (the car's download name, title, credit) as
/// whole words: the longest such.
pub fn pick_model(names: &[&str], models: &[&str]) -> Option<usize> {
    let names: Vec<String> = names.iter().map(|n| words(n)).filter(|n| !n.is_empty()).collect();
    let starts = |model: &str| {
        let model = words(model);
        !model.is_empty()
            && names.iter().any(|n| n.strip_prefix(&model).is_some_and(|rest| rest.is_empty() || rest.starts_with(' ')))
    };
    (0..models.len()).filter(|&k| starts(models[k])).max_by_key(|&k| words(models[k]).len())
}

/// The physics packs in `dir` and their models (see the module).
pub fn downloads(dir: &Path) -> Result<Vec<Download>, Fail> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    let models: Vec<&str> = names.iter().filter_map(|n| strip_end(n, MODEL)).collect();
    Ok(names
        .iter()
        .filter_map(|name| {
            let car = strip_end(name, PHYSICS)?;
            let model = pick_model(&[car], &models).map(|k| dir.join(format!("{}{MODEL}", models[k])));
            Some(Download { physics: dir.join(name), model })
        })
        .collect())
}

/// An archive entry's path (`\` or `/` separated) as safe components below the folder it is written to, or `None`.
pub(crate) fn safe_path(name: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = name.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    let bad = |p: &&str| *p == ".." || p.contains(':');
    (!parts.is_empty() && !parts.iter().any(bad)).then_some(parts)
}

/// One folder name of ours, from untrusted text: no separators, not hidden.
fn safe_folder(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && !name.starts_with('.') && safe_path(name)?.len() == 1).then(|| name.to_owned())
}

/// Counts the bytes written of all, reports them, and stops when cancelled.
struct Counter<'a> {
    done: u64,
    total: u64,
    report: &'a (dyn Fn(&Progress) + Sync),
    cancel: &'a AtomicBool,
}

impl Counter<'_> {
    fn add(&mut self, n: u64) -> Result<(), Fail> {
        self.done += n;
        (self.report)(&Progress { step: "Adding cars", done: self.done, total: self.total });
        match self.cancel.load(Ordering::Relaxed) {
            true => Err("cancelled".into()),
            false => Ok(()),
        }
    }
}

/// Writes `reader` to `dest`/`parts`, making its folders.
fn write_file(dest: &Path, parts: &[&str], reader: &mut dyn Read) -> io::Result<u64> {
    let path = parts.iter().fold(dest.to_owned(), |p, part| p.join(part));
    fs::create_dir_all(path.parent().unwrap_or(dest))?;
    let mut out = io::BufWriter::new(File::create(&path)?);
    let n = io::copy(reader, &mut out)?;
    out.flush()?;
    Ok(n)
}

/// A physics pack download: the zip, its pack folder's name and the entries below it (index, path below it).
struct PhysicsZip {
    zip: rbz::ZipArchive<BufReader<File>>,
    folder: String,
    entries: Vec<(usize, Vec<String>)>,
    size: u64,
}

impl PhysicsZip {
    /// The zip at `path`: its files in one `<pack>/` folder (as RSF's `rsfdata/cars/<pack>.zip`), or at its top (the
    /// pack folder is then the download's name, `_` for spaces).
    fn open(path: &Path) -> Result<PhysicsZip, Fail> {
        let error = |e: String| format!("{}: {e}", path.display());
        let zip = rbz::open(path).map_err(|e| error(e.to_string()))?;
        let mut files = Vec::new();
        for k in 0..zip.len() {
            let name = zip.name_for_index(k).and_then(Result::ok).ok_or_else(|| error("a bad entry".into()))?;
            let parts = safe_path(&name).ok_or_else(|| error(format!("a bad entry path {name}")))?;
            if !name.ends_with(['/', '\\']) {
                files.push((k, parts.iter().map(|p| p.to_string()).collect::<Vec<_>>()));
            }
        }
        let first = files.first().map(|(_, p)| p[0].clone()).unwrap_or_default();
        let nested = files.iter().all(|(_, p)| p.len() > 1 && p[0] == first);
        let download = path.file_name().and_then(|n| strip_end(n.to_str()?, PHYSICS)).unwrap_or_default();
        let folder = match nested {
            true => first,
            false => download.replace(' ', "_"),
        };
        let folder = safe_folder(&folder).ok_or_else(|| error(format!("a bad pack folder name {folder}")))?;
        let entries: Vec<_> = files.into_iter().map(|(k, p)| (k, if nested { p[1..].to_vec() } else { p })).collect();
        if !entries.iter().any(|(_, p)| p.len() == 1 && p[0].eq_ignore_ascii_case("common.lsp")) {
            return Err(error("no common.lsp: is it an NGP physics pack?".into()));
        }
        let mut zip = zip;
        let mut size = 0;
        for (k, _) in &entries {
            size += zip.by_index_raw(*k).map_err(|e| error(e.to_string()))?.size();
        }
        Ok(PhysicsZip { zip, folder, entries, size })
    }

    /// Unpacks the pack into `dest`.
    fn extract(&mut self, dest: &Path, count: &mut Counter) -> Result<(), Fail> {
        for (k, parts) in &self.entries {
            let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
            let mut file = self.zip.by_index(*k).map_err(|e| e.to_string())?;
            let n = write_file(dest, &parts, &mut file).map_err(|e| format!("{}: {e}", parts.join("/")))?;
            count.add(n)?;
        }
        Ok(())
    }
}

/// A model download: the folder holding its `.sgc` (the path of it in the `.7z`), and the `.sgc`'s name.
struct ModelArchive {
    path: PathBuf,
    prefix: Vec<String>,
    filename: String,
    size: u64,
}

impl ModelArchive {
    fn open(path: &Path) -> Result<ModelArchive, Fail> {
        let error = |e: String| format!("{}: {e}", path.display());
        let file = File::open(path).map_err(|e| error(e.to_string()))?;
        let archive = sevenz_rust2::ArchiveReader::new(BufReader::new(file), sevenz_rust2::Password::empty())
            .map_err(|e| error(format!("not a .7z: {e}")))?;
        let files = &archive.archive().files;
        // The `.sgc` nearest the top, then by name.
        let mut sgcs: Vec<Vec<&str>> = files
            .iter()
            .filter(|f| !f.is_directory && f.name.to_lowercase().ends_with(".sgc"))
            .filter_map(|f| safe_path(&f.name))
            .filter(|p| p.len() >= 2)
            .collect();
        sgcs.sort_by_key(|p| (p.len(), p.join("/")));
        let sgc = sgcs.first().ok_or_else(|| error("no Cars/<model>/<model>.sgc in it".into()))?;
        let prefix: Vec<String> = sgc[..sgc.len() - 1].iter().map(|p| p.to_string()).collect();
        let name = sgc[sgc.len() - 1];
        let filename = name[..name.len() - 4].to_owned();
        let below = |name: &str| safe_path(name).is_some_and(|p| p.len() > prefix.len() && within(&p, &prefix));
        let size = files.iter().filter(|f| !f.is_directory && below(&f.name)).map(|f| f.size).sum();
        Ok(ModelArchive { path: path.to_owned(), prefix, filename, size })
    }

    /// The model folder's name.
    fn folder(&self) -> Result<String, Fail> {
        let last = self.prefix.last().map(String::as_str).unwrap_or_default();
        safe_folder(last).ok_or(format!("{}: a bad model folder name {last}", self.path.display()))
    }

    /// Unpacks the model folder into `dest`.
    fn extract(&self, dest: &Path, count: &mut Counter) -> Result<(), Fail> {
        let error = |e: String| format!("{}: {e}", self.path.display());
        let file = File::open(&self.path).map_err(|e| error(e.to_string()))?;
        let mut archive = sevenz_rust2::ArchiveReader::new(BufReader::new(file), sevenz_rust2::Password::empty())
            .map_err(|e| error(e.to_string()))?;
        fs::create_dir_all(dest).map_err(|e| error(e.to_string()))?;
        let mut failed = None;
        archive
            .for_each_entries(|entry, reader| {
                let Some(parts) =
                    safe_path(&entry.name).filter(|p| p.len() > self.prefix.len() && within(p, &self.prefix))
                else {
                    return Ok(true);
                };
                let parts = &parts[self.prefix.len()..];
                let written = match entry.is_directory {
                    true => fs::create_dir_all(parts.iter().fold(dest.to_owned(), |p, part| p.join(part))).map(|_| 0),
                    false => write_file(dest, parts, reader),
                };
                match written.map_err(|e| format!("{}: {e}", parts.join("/"))).and_then(|n| count.add(n)) {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        failed = Some(e);
                        Ok(false)
                    }
                }
            })
            .map_err(|e| error(e.to_string()))?;
        failed.map_or(Ok(()), Err)
    }
}

/// Whether `path` lies below the folder `prefix` (its names ignoring case).
fn within(path: &[&str], prefix: &[String]) -> bool {
    path.iter().zip(prefix).all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// The added cars' list in `cars` (none without one).
pub fn list(cars: &Path) -> (Vec<rsf::Car>, Vec<rsf::Model>) {
    match rsf::Install::open(cars) {
        Ok(install) => (install.cars, install.models),
        Err(_) => (Vec::new(), Vec::new()),
    }
}

/// `text` as a JSON string.
fn json(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out + "\""
}

/// Writes the list RSF's reader takes (`rsfdata/cache/cars.json`, `carmodels.json`), dropping the models no car uses
/// and their folders.
pub(crate) fn write_list(cars_dir: &Path, cars: &[rsf::Car], models: &[rsf::Model]) -> Result<(), Fail> {
    let used: Vec<&rsf::Model> = models.iter().filter(|m| cars.iter().any(|c| c.model == m.id)).collect();
    for model in models.iter().filter(|m| !used.iter().any(|u| u.path.eq_ignore_ascii_case(&m.path))) {
        if let Some(folder) = safe_folder(&model.path) {
            let _ = fs::remove_dir_all(cars_dir.join("Cars").join(folder));
        }
    }
    let rows = |rows: Vec<String>| format!("[\n{}\n]\n", rows.join(",\n"));
    let cars_json = rows(
        cars.iter()
            .map(|c| {
                let (id, name, path, rev) = (c.id, json(&c.name), json(&c.pack), json(&c.revision));
                format!("{{\"id\":{id},\"name\":{name},\"path\":{path},\"carmodel_id\":{},\"rev\":{rev}}}", c.model)
            })
            .collect(),
    );
    let models_json = rows(
        used.iter()
            .map(|m| {
                let (id, name, path, file) = (m.id, json(&m.name), json(&m.path), json(&m.filename));
                format!(
                    "{{\"id\":{id},\"name\":{name},\"path\":{path},\"filename\":{file},\"fallback\":{}}}",
                    m.fallback
                )
            })
            .collect(),
    );
    let cache = cars_dir.join("rsfdata").join("cache");
    fs::create_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    for (name, text) in [("carmodels.json", models_json), ("cars.json", cars_json)] {
        let (temp, path) = (cache.join(format!(".{name}.new")), cache.join(name));
        fs::write(&temp, text)
            .and_then(|()| fs::rename(&temp, &path))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Moves the folder `from` to `to`, replacing what is there.
fn replace(from: &Path, to: &Path) -> Result<(), Fail> {
    let error = |e: io::Error| format!("{}: {e}", to.display());
    if to.exists() {
        fs::remove_dir_all(to).map_err(error)?;
    }
    fs::create_dir_all(to.parent().unwrap_or(to)).map_err(error)?;
    fs::rename(from, to).map_err(error)
}

/// The value after `key` on a line of the pack's text file (`revision            3 / 2024-12-10`).
fn about_value<'a>(about: &'a str, keys: &[&str]) -> Option<&'a str> {
    about.lines().find_map(|line| {
        let line = line.trim();
        keys.iter().find_map(|key| {
            let rest = line.get(key.len()..)?;
            (line[..key.len()].eq_ignore_ascii_case(key) && rest.starts_with([' ', '\t'])).then(|| rest.trim())
        })
    })
}

/// Adds `download` to `cars_dir`'s list (`cars`, `models`), unpacking through `temp`: its car's name and model folder.
fn add_one(
    cars_dir: &Path,
    temp: &Path,
    download: &Download,
    list: &mut (Vec<rsf::Car>, Vec<rsf::Model>),
    count: &mut Counter,
) -> Result<(String, Option<String>), Fail> {
    let mut physics = PhysicsZip::open(&download.physics)?;
    let model = download.model.as_deref().map(ModelArchive::open).transpose()?;
    let staged = temp.join("pack");
    physics.extract(&staged, count)?;
    let pack = rsf::Pack::open(&staged)?;
    let about = pack.about().unwrap_or_default();
    let title = pack.title().filter(|t| !t.is_empty()).unwrap_or_else(|| physics.folder.replace('_', " "));
    let revision = about_value(&about, &["revision"]).unwrap_or_default().to_owned();
    let (cars, models) = list;
    let next = |ids: &mut dyn Iterator<Item = u32>| ids.max().map_or(1, |m| m + 1);
    let model_id = match &model {
        Some(archive) => {
            let folder = archive.folder()?;
            archive.extract(&temp.join("model"), count)?;
            replace(&temp.join("model"), &cars_dir.join("Cars").join(&folder))?;
            let name = download.model.as_deref().and_then(|p| strip_end(p.file_name()?.to_str()?, MODEL));
            let row = rsf::Model {
                id: 0,
                name: name.unwrap_or(&folder).to_owned(),
                path: folder.clone(),
                filename: archive.filename.clone(),
                fallback: STAND_IN,
            };
            match models.iter_mut().find(|m| m.path.eq_ignore_ascii_case(&folder)) {
                Some(m) => {
                    *m = rsf::Model { id: m.id, ..row };
                    m.id
                }
                None => {
                    let id = next(&mut models.iter().map(|m| m.id));
                    models.push(rsf::Model { id, ..row });
                    id
                }
            }
        }
        None => {
            // A model added before, else the stand-in's row: a folder that is never there.
            let download_name = download.physics.file_name().and_then(|n| strip_end(n.to_str()?, PHYSICS));
            let credit = about_value(&about, &["3D model", "model"]).unwrap_or_default();
            let real: Vec<&rsf::Model> =
                models.iter().filter(|m| cars_dir.join("Cars").join(&m.path).is_dir()).collect();
            let names: Vec<&str> = real.iter().map(|m| m.name.as_str()).collect();
            match pick_model(&[download_name.unwrap_or_default(), &title, credit], &names) {
                Some(k) => real[k].id,
                None => {
                    let id = next(&mut models.iter().map(|m| m.id));
                    let missing = format!("{} (no model)", physics.folder);
                    let row = rsf::Model {
                        id,
                        name: missing.clone(),
                        path: missing.clone(),
                        filename: missing,
                        fallback: STAND_IN,
                    };
                    models.push(row);
                    id
                }
            }
        }
    };
    replace(&staged, &cars_dir.join("rsfdata").join("cars").join(&physics.folder))?;
    let row = rsf::Car {
        id: 0,
        name: title.clone(),
        pack: physics.folder.clone(),
        group: None,
        model: model_id,
        revision,
        audio: None,
        drive_train: None,
    };
    match cars.iter_mut().find(|c| c.pack.eq_ignore_ascii_case(&physics.folder)) {
        Some(c) => *c = rsf::Car { id: c.id, ..row },
        None => {
            let id = next(&mut cars.iter().map(|c| c.id));
            cars.push(rsf::Car { id, ..row });
        }
    }
    write_list(cars_dir, cars, models)?;
    let folder = models.iter().find(|m| m.id == model_id).map(|m| m.path.clone());
    Ok((title, folder.filter(|f| cars_dir.join("Cars").join(f).is_dir())))
}

/// Adds every physics pack download in `dir` to `cars_dir` with its model ([`downloads`]); `report` gets the
/// progress, and `cancel` stops it. One pack that fails does not stop the others; a cancel keeps those added.
pub fn add(
    cars_dir: &Path,
    dir: &Path,
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Vec<Added>, Fail> {
    let downloads = downloads(dir)?;
    if downloads.is_empty() {
        return Err(format!(
            "No car packs in {}: choose the folder with the \"<Car>{PHYSICS}\" files downloaded from gvrc.de.",
            dir.display()
        ));
    }
    add_downloads(cars_dir, &downloads, report, cancel)
}

/// Adds `downloads` (each physics pack with its model, if any) to `cars_dir`, as [`add`] does.
pub fn add_downloads(
    cars_dir: &Path,
    downloads: &[Download],
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Vec<Added>, Fail> {
    fs::create_dir_all(cars_dir).map_err(|e| format!("{}: {e}", cars_dir.display()))?;
    // The bytes to write (a download that does not open fails again when it is added).
    let physics = |d: &Download| PhysicsZip::open(&d.physics).map_or(0, |z| z.size);
    let model = |d: &Download| d.model.as_deref().and_then(|m| ModelArchive::open(m).ok()).map_or(0, |m| m.size);
    let total = downloads.iter().map(|d| physics(d) + model(d)).sum();
    let mut count = Counter { done: 0, total, report, cancel };
    count.add(0)?;
    let mut list = list(cars_dir);
    let mut added = Vec::new();
    for download in downloads {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        let temp = Temp(cars_dir.join(format!(".adding-{}-{stamp}", std::process::id())));
        let result = add_one(cars_dir, &temp.0, download, &mut list, &mut count);
        drop(temp);
        let name = download.physics.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match result {
            Err(e) if e.ends_with("cancelled") => return Err(e),
            result => added.push(Added { download: name, result }),
        }
    }
    Ok(added)
}

/// Removes the added car whose pack folder is `pack` from `cars_dir`, and its model when no other car uses it.
pub fn remove(cars_dir: &Path, pack: &str) -> Result<(), Fail> {
    let (mut cars, models) = list(cars_dir);
    cars.retain(|c| !c.pack.eq_ignore_ascii_case(pack));
    write_list(cars_dir, &cars, &models)?;
    let folder = safe_folder(pack).ok_or(format!("a bad pack folder name {pack}"))?;
    let path = cars_dir.join("rsfdata").join("cars").join(folder);
    match path.exists() {
        true => fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display())),
        false => Ok(()),
    }
}

/// The result line: the cars added, those drawn with the stand-in model, and those not added.
pub fn summary(added: &[Added]) -> Result<String, String> {
    let ok: Vec<&(String, Option<String>)> = added.iter().filter_map(|a| a.result.as_ref().ok()).collect();
    let mut text = match ok.len() {
        1 => format!("Added {}.", ok[0].0),
        n => format!("Added {n} cars."),
    };
    let bare: Vec<&str> = ok.iter().filter(|(_, m)| m.is_none()).map(|(n, _)| n.as_str()).collect();
    if !bare.is_empty() {
        text += &format!(
            " No model for {}: drawn as the {STAND_IN_NAME}, as RallySimFans does, until its \"<Model>{MODEL}\" is \
             added.",
            bare.join(", ")
        );
    }
    let failed: Vec<String> =
        added.iter().filter_map(|a| Some(format!("{}: {}", a.download, a.result.as_ref().err()?))).collect();
    match (ok.is_empty(), failed.is_empty()) {
        (_, true) => Ok(text),
        (true, false) => Err(format!("Nothing added. {}", failed.join("; "))),
        (false, false) => Err(format!("{text} Not added: {}", failed.join("; "))),
    }
}

/// The command line's `--add-cars DIR`, into `cars_dir`.
pub fn cli(cars_dir: &Path, args: &[String]) -> Result<(), String> {
    let [dir] = args else {
        return Err("usage: rerbr --add-cars DIR".into());
    };
    let added = add(cars_dir, Path::new(dir), &|_| {}, &AtomicBool::new(false))?;
    for a in &added {
        match &a.result {
            Ok((name, Some(model))) => println!("{}: {name}, model Cars/{model}", a.download),
            Ok((name, None)) => println!("{}: {name}, no model (retail car {STAND_IN}'s)", a.download),
            Err(e) => println!("{}: not added: {e}", a.download),
        }
    }
    println!("{}", cars_dir.display());
    summary(&added).map(|line| println!("{line}"))
}

/// "Add cars…"'s job and its last result, and the added cars (name, pack folder) for the Settings page's list.
#[derive(Default)]
pub struct Adding {
    job: Option<Job<Vec<Added>>>,
    status: Option<Result<String, String>>,
    pub cars: Vec<(String, String)>,
}

impl Adding {
    /// The added cars of `cars_dir`.
    pub fn new(cars_dir: &Path) -> Adding {
        let cars = list(cars_dir).0.into_iter().map(|c| (c.name, c.pack)).collect();
        Adding { cars, ..Adding::default() }
    }
}

/// What "Add cars…" says it takes.
pub(crate) const ADD_TIP: &str =
    "Choose a folder with gvrc.de's NGP car packs, as downloaded: each car's \"<Car> - NGP \
                       Physics.zip\" and its \"<Model> - Model.7z\". ReRBR copies them into your files' folder; the \
                       downloads are not changed.";

impl Form {
    /// "Add cars…": asks for the downloads' folder and adds its packs on a thread; its progress and Cancel, or its
    /// result line, follow.
    pub fn add_cars_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(self.can_add_cars(), egui::Button::new("Add cars…")).on_hover_text(ADD_TIP).clicked() {
                self.add_cars();
            }
            self.adding_job_ui(ui);
        });
        self.adding_status_ui(ui);
    }

    /// Whether "Add cars…" can run: not while adding or driving.
    pub fn can_add_cars(&self) -> bool {
        self.adding.job.is_none() && self.child.is_none()
    }

    /// Whether a job runs or a result waits to be shown.
    pub fn has_adding_output(&self) -> bool {
        self.adding.job.is_some() || self.adding.status.is_some()
    }

    /// The running job's progress and Cancel; when it ends, its result is kept and the lists read again.
    pub fn adding_job_ui(&mut self, ui: &mut egui::Ui) {
        let Some(job) = &mut self.adding.job else { return };
        if let Some(result) = job.ui(ui) {
            self.adding.job = None;
            self.adding.status = Some(match result {
                Err(e) if e.ends_with("cancelled") => Err("Cancelled; the cars added before are kept.".into()),
                result => result.and_then(|added| summary(&added)),
            });
            self.rescan_cars();
        }
    }

    /// The last result line.
    pub fn adding_status_ui(&self, ui: &mut egui::Ui) {
        match &self.adding.status {
            Some(Ok(text)) => _ = ui.colored_label(GOOD, format!("✔ {text}")),
            Some(Err(text)) => _ = ui.colored_label(WARN, format!("⚠ {text}")),
            None => {}
        }
    }

    /// Runs `work`, which adds cars, as "Add cars…" does (its progress, Cancel and result line), unless adding or
    /// driving already.
    pub fn start_adding(
        &mut self,
        work: impl FnOnce(&(dyn Fn(&Progress) + Sync), &AtomicBool) -> Result<Vec<Added>, Fail> + Send + 'static,
    ) {
        if self.can_add_cars() {
            self.adding.status = None;
            self.adding.job = Some(Job::start(work));
        }
    }

    /// Asks for the downloads' folder and starts adding its packs, unless adding or driving already.
    pub fn add_cars(&mut self) {
        if self.adding.job.is_some() || self.child.is_some() {
            return;
        }
        if let Some(dir) = install::pick_folder("The folder with the NGP car pack downloads") {
            let cars = self.cars_dir();
            self.adding.status = None;
            self.adding.job = Some(Job::start(move |report, cancel| add(&cars, Path::new(&dir), report, cancel)));
        }
    }

    /// The added cars, each with Remove.
    pub fn added_cars_ui(&mut self, ui: &mut egui::Ui) {
        let mut removed = None;
        let idle = self.adding.job.is_none() && self.child.is_none();
        egui::Grid::new("added cars").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
            for (name, pack) in &self.adding.cars {
                ui.label(name).on_hover_text(pack);
                if ui.add_enabled(idle, egui::Button::new("Remove")).clicked() {
                    removed = Some(pack.clone());
                }
                ui.end_row();
            }
        });
        if let Some(pack) = removed {
            self.adding.status = remove(&self.cars_dir(), &pack).err().map(Err);
            self.rescan_cars();
        }
    }

    /// Reads the added cars again, into the lists.
    fn rescan_cars(&mut self) {
        let root = self.rsf();
        self.catalog.set_rsf_cars(&self.paths.data, &root, &self.cars_dir());
        self.list_cars();
        self.adding =
            Adding { job: self.adding.job.take(), status: self.adding.status.take(), ..Adding::new(&self.cars_dir()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_are_picked_by_their_names_start() {
        let models = ["Mitsubishi Lancer Evo IX", "Mitsubishi Lancer Evo X", "Ford Fiesta", "Ford Fiesta Rally2"];
        let pick = |names: &[&str]| pick_model(names, &models);
        assert_eq!(pick(&["Mitsubishi Lancer Evo IX N4"]), Some(0));
        assert_eq!(pick(&["Mitsubishi Lancer Evo IX R4"]), Some(0));
        assert_eq!(pick(&["Mitsubishi Lancer Evo X R4"]), Some(1));
        assert_eq!(pick(&["Ford Fiesta Rally2"]), Some(3));
        assert_eq!(pick(&["Ford Fiesta R5"]), Some(2));
        assert_eq!(pick(&["Ford Fiesta Rally3", "x", "FORD FIESTA RALLY2 2023 by G. Rodriguez"]), Some(3));
        assert_eq!(pick(&["Mitsubishi_Lancer_Evo_IXR"]), None);
        assert_eq!(pick(&["", "Audi quattro"]), None);
    }

    #[test]
    fn archive_paths_stay_below_their_folder() {
        assert_eq!(safe_path("Pack\\setups/d_gravel.lsp"), Some(vec!["Pack", "setups", "d_gravel.lsp"]));
        assert_eq!(safe_path("/abs/./x"), Some(vec!["abs", "x"]));
        assert_eq!(safe_path("a/../../x"), None);
        assert_eq!(safe_path("C:\\x"), None);
        assert_eq!(safe_path(""), None);
        assert_eq!(safe_folder("evo_IX"), Some("evo_IX".into()));
        assert_eq!(safe_folder(".."), None);
        assert_eq!(safe_folder("a/b"), None);
        assert_eq!(safe_folder(".hidden"), None);
        assert_eq!(about_value("Evo\r\nrevision            8 / 2019-10-08\r\n", &["revision"]), Some("8 / 2019-10-08"));
        assert_eq!(about_value("3D model\t\tKUBRT v1.1\n", &["3D model", "model"]), Some("KUBRT v1.1"));
        assert_eq!(json("a \"b\" \\ \u{1}"), "\"a \\\"b\\\" \\\\ \\u0001\"");
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rbr-car-packs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A physics pack download: `<Car> - NGP Physics.zip` holding `<pack>/` (or its files at the top).
    fn physics_zip(dir: &Path, car: &str, pack: Option<&str>, credit: &str) {
        let file = File::create(dir.join(format!("{car}{PHYSICS}"))).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let path = |name: &str| pack.map_or(name.to_owned(), |p| format!("{p}\\{name}"));
        let about = format!("{car}\r\nrevision            3 / 2024-12-10\r\n3D model            {credit}\r\n");
        for (name, text) in
            [("common.lsp", "((\"PhysicsEngine\"))"), ("setups\\d_gravel.lsp", "x"), (car, about.as_str())]
        {
            zip.start_file(path(name), options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    /// A model download: `<Model> - Model.7z` holding `Cars/<folder>/<file>.sgc` and a texture.
    fn model_7z(dir: &Path, model: &str, folder: &str, file: &str) {
        let mut out = sevenz_rust2::ArchiveWriter::create(dir.join(format!("{model}{MODEL}"))).unwrap();
        for (name, bytes) in [
            (format!("Cars/{folder}/{file}.sgc"), b"sgc".as_slice()),
            (format!("Cars/{folder}/textures/a.dds"), b"dds".as_slice()),
            ("readme.txt".to_owned(), b"read me".as_slice()),
        ] {
            out.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(&name), Some(bytes)).unwrap();
        }
        out.finish().unwrap();
    }

    /// Two packs sharing one model and one without a model are added, listed by RSF's reader, added again (replaced,
    /// same ids) and removed (the shared model stays until its last car goes).
    #[test]
    fn packs_are_added_listed_and_removed() {
        let root = temp("add");
        let (downloads_dir, cars) = (root.join("downloads"), root.join("cars"));
        fs::create_dir_all(&downloads_dir).unwrap();
        physics_zip(&downloads_dir, "Mitsubishi Lancer Evo IX N4", Some("Evo_IX_N4_ngp6"), "KUBRT");
        physics_zip(&downloads_dir, "Mitsubishi Lancer Evo IX R4", None, "KUBRT");
        physics_zip(&downloads_dir, "Ford Fiesta Rally2", Some("Ford_Fiesta_Rally2_ngp7"), "by someone");
        model_7z(&downloads_dir, "Mitsubishi Lancer Evo IX", "evo_IX", "evo_IX");
        fs::write(downloads_dir.join("Broken - NGP Physics.zip"), b"not a zip").unwrap();
        let found = downloads(&downloads_dir).unwrap();
        assert_eq!(found.len(), 4);
        assert_eq!(found.iter().filter(|d| d.model.is_some()).count(), 2, "{found:?}");

        let progress = std::sync::Mutex::new(Vec::new());
        let added =
            add(&cars, &downloads_dir, &|p| progress.lock().unwrap().push(p.done), &AtomicBool::new(false)).unwrap();
        let line = summary(&added).unwrap_err();
        assert!(line.starts_with("Added 3 cars. No model for Ford Fiesta Rally2: drawn as the Subaru"), "{line}");
        assert!(line.contains("Not added: Broken - NGP Physics.zip: "), "{line}");
        assert!(progress.lock().unwrap().windows(2).all(|w| w[0] <= w[1]));

        let install = rsf::Install::open(&cars).unwrap();
        let names: Vec<(&str, &str)> = install.cars.iter().map(|c| (c.name.as_str(), c.pack.as_str())).collect();
        assert_eq!(
            names,
            [
                ("Ford Fiesta Rally2", "Ford_Fiesta_Rally2_ngp7"),
                ("Mitsubishi Lancer Evo IX N4", "Evo_IX_N4_ngp6"),
                ("Mitsubishi Lancer Evo IX R4", "Mitsubishi_Lancer_Evo_IX_R4"),
            ]
        );
        assert_eq!(install.cars[1].revision, "3 / 2024-12-10");
        let evo = install.pack(&install.cars[1]).unwrap();
        assert_eq!(evo.read("setups/d_gravel.lsp").unwrap(), b"x");
        assert_eq!(evo.path(), cars.join("rsfdata/cars/Evo_IX_N4_ngp6"));
        assert_eq!(install.cars[1].model, install.cars[2].model);
        let model = |c: &rsf::Car| install.model_entry(c, rsf::SLOT, &[retail()]).unwrap();
        assert_eq!(model(&install.cars[1]).entry.file_name, "Cars\\evo_IX\\evo_IX.sgc");
        assert_eq!(model(&install.cars[1]).fallback, None);
        assert_eq!(model(&install.cars[0]).entry.file_name, retail().file_name);
        assert!(cars.join("Cars/evo_IX/textures/a.dds").is_file());
        assert!(!cars.join("Cars/readme.txt").exists() && !cars.join("readme.txt").exists());
        let (found, car, _) = rsf::locate(&evo.path().display().to_string(), Some(&root)).unwrap();
        assert_eq!((found.root, car.name.as_str()), (cars.canonicalize().unwrap(), "Mitsubishi Lancer Evo IX N4"));

        // Again: replaced, the same ids; nothing left in the temp folders.
        let ids: Vec<u32> = install.cars.iter().map(|c| c.id).collect();
        add(&cars, &downloads_dir, &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(list(&cars).0.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
        let mut top: Vec<String> =
            fs::read_dir(&cars).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        top.sort();
        assert_eq!(top, ["Cars", "rsfdata"]);

        // The R4 alone again, its model already added: it keeps it.
        let single = root.join("single");
        fs::create_dir_all(&single).unwrap();
        fs::copy(
            downloads_dir.join(format!("Mitsubishi Lancer Evo IX R4{PHYSICS}")),
            single.join(format!("Mitsubishi Lancer Evo IX R4{PHYSICS}")),
        )
        .unwrap();
        let again = add(&cars, &single, &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(again[0].result, Ok(("Mitsubishi Lancer Evo IX R4".into(), Some("evo_IX".into()))));

        remove(&cars, "Evo_IX_N4_ngp6").unwrap();
        assert!(!cars.join("rsfdata/cars/Evo_IX_N4_ngp6").exists());
        assert!(cars.join("Cars/evo_IX").is_dir());
        remove(&cars, "Mitsubishi_Lancer_Evo_IX_R4").unwrap();
        assert!(!cars.join("Cars/evo_IX").exists());
        assert_eq!(list(&cars).0.len(), 1);
        assert!(remove(&cars, "../x").is_err());

        // Cancelled: nothing half-written.
        let err = add(&cars, &downloads_dir, &|_| {}, &AtomicBool::new(true)).unwrap_err();
        assert!(err.ends_with("cancelled"));
        assert_eq!(fs::read_dir(&cars).unwrap().count(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    fn retail() -> rbr_formats::cars::CarEntry {
        rbr_formats::cars::CarEntry {
            index: 5,
            file_name: "Cars\\s_i2003\\s_i2003.sgc".into(),
            ini_file: String::new(),
            shader_file: String::new(),
            shader_settings: String::new(),
            texture_path: String::new(),
        }
    }
}

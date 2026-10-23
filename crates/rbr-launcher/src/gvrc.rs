//! Downloading NGP and cars from gvrc.de, so that a game folder without NGP, or a player without cars, is set up from
//! the launcher: the Settings page's "Download NGP…" and "Download cars…", `launcher --download-ngp GAME` and
//! `launcher --download-cars [NAME…]`.
//!
//! The list is gvrc.de's `carList.ini`, the one RBRCIT (the Windows car installer) reads and WorkerBee maintains: `[Plugins]`
//! names the NGP package (`plugin_physics_url`) and each `[Car_<n>]` a car's `name`, `cat`, `year` and downloads
//! (`link_physics`, `link_model`, `link_banks`). It is fetched live, each time the list is opened.
//! Only `https://gvrc.de/` links are followed, and a car needs its `<Car> - NGP Physics.zip` (a model is a
//! `<Model> - Model.7z`; the original eight cars, which NGP's package covers, and the sound banks are left out).
//!
//! - NGP goes into the game folder (`disc_install::add_ngp`): the package is downloaded to a temp folder in it, checked
//!   to be NGP 7.4.778, and written like NGP's own install. This is the one place the launcher writes the game folder.
//! - A car's downloads go to a temp folder in `cars/` and are added as downloaded packs are (`car_packs::add_downloads`),
//!   each car with the model its list entry names.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use egui::Color32;

use crate::car_packs::{self, Added, Download, MODEL, PHYSICS};
use crate::disc_install::{self, Job, Progress, Temp};
use crate::pages::{GOOD, WARN};
use crate::Form;

/// gvrc.de's car list.
pub const LIST_URL: &str = "https://gvrc.de/ngp/tools/rbrcit/carlist/carList.ini";
/// The only links followed.
const HOST: &str = "https://gvrc.de/";
/// A download's size limit.
const MAX_BYTES: u64 = 2 << 30;

type Fail = String;

/// A car of the list: its name, category and year, its physics pack and its model, as links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub cat: String,
    pub year: String,
    pub physics: String,
    pub model: Option<String>,
}

/// What the list gives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub version: String,
    /// NGP's package.
    pub ngp: Option<String>,
    /// The cars to add, by name.
    pub cars: Vec<Entry>,
}

/// `text` (a `carList.ini`): its version, NGP's package and its cars. A car whose links are not gvrc.de's or not
/// named as downloads are, and the original cars, are left out; so are the commented lines.
pub fn parse(text: &str) -> Catalog {
    let mut catalog = Catalog::default();
    let mut sections: Vec<(&str, HashMap<&str, &str>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if let Some(comment) = line.strip_prefix(';') {
            if let Some(version) = comment.trim().strip_prefix("Version:") {
                catalog.version = version.trim().to_owned();
            }
        } else if line.is_empty() || line.starts_with('#') {
            continue;
        } else if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name, HashMap::new()));
        } else if let (Some((key, value)), Some(last)) = (line.split_once('='), sections.last_mut()) {
            last.1.insert(key.trim(), value.trim());
        }
    }
    let link = |fields: &HashMap<&str, &str>, key: &str, suffix: &str| {
        let url = fields.get(key).copied().filter(|u| is_gvrc(u))?;
        let name = file_name(url)?;
        car_packs::strip_end(&name, suffix).is_some().then(|| url.to_owned())
    };
    for (section, fields) in &sections {
        if section.eq_ignore_ascii_case("Plugins") {
            catalog.ngp = fields
                .get("plugin_physics_url")
                .filter(|u| is_gvrc(u) && file_name(u).is_some())
                .map(|u| u.to_string());
        } else if section.get(..4).is_some_and(|s| s.eq_ignore_ascii_case("Car_")) {
            let field = |key: &str| fields.get(key).copied().unwrap_or_default().to_owned();
            let cat = field("cat");
            let (Some(physics), false) = (link(fields, "link_physics", PHYSICS), cat.eq_ignore_ascii_case("Original"))
            else {
                continue;
            };
            let model = link(fields, "link_model", MODEL);
            if model.is_none() && fields.get("link_model").is_some_and(|m| !m.is_empty()) {
                continue;
            }
            catalog.cars.push(Entry { name: field("name"), cat, year: field("year"), physics, model });
        }
    }
    catalog.cars.sort_by_key(|c| c.name.to_lowercase());
    catalog
}

/// Whether `url` is one of gvrc.de's.
fn is_gvrc(url: &str) -> bool {
    url.starts_with(HOST)
}

/// The download's file name: the link's last part, its `%XX`s decoded, when it is one file's (no folder, not hidden).
fn file_name(url: &str) -> Option<String> {
    let last = url.rsplit('/').next()?;
    let mut bytes = Vec::new();
    let mut raw = last.bytes();
    while let Some(b) = raw.next() {
        if b == b'%' {
            let hex: Vec<u8> = raw.by_ref().take(2).collect();
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    let name = String::from_utf8(bytes).ok()?;
    let bad = |c: char| matches!(c, '/' | '\\' | ':' | '?' | '#') || c.is_control();
    (!name.is_empty() && !name.starts_with('.') && !name.contains(bad)).then_some(name)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("ReRBR/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(900)))
        .build()
        .into()
}

/// The list at `url`, read and parsed.
fn read_list(url: &str) -> Result<Catalog, Fail> {
    let bytes = agent()
        .get(url)
        .call()
        .and_then(|mut r| r.body_mut().read_to_vec())
        .map_err(|e| format!("Could not read the car list at {url}: {e}"))?;
    Ok(parse(&String::from_utf8_lossy(&bytes)))
}

/// The car list as a job for the Settings page.
fn list_job() -> Job<Catalog> {
    Job::start(|report, _| {
        report(&Progress { step: "Reading the car list", ..Progress::default() });
        read_list(LIST_URL)
    })
}

/// The bytes downloaded of all, reported, and stopped when cancelled.
struct Bytes<'a> {
    step: &'static str,
    done: u64,
    total: u64,
    report: &'a (dyn Fn(&Progress) + Sync),
    cancel: &'a AtomicBool,
}

impl Bytes<'_> {
    fn add(&mut self, n: u64) -> Result<(), Fail> {
        self.done += n;
        (self.report)(&Progress { step: self.step, done: self.done, total: self.total });
        match self.cancel.load(Ordering::Relaxed) {
            true => Err("cancelled".into()),
            false => Ok(()),
        }
    }
}

/// The size of the download at `url`, 0 when the server does not say.
fn size_of(agent: &ureq::Agent, url: &str) -> u64 {
    let response = agent.head(url).call().ok();
    let length = response.as_ref().and_then(|r| r.headers().get("content-length")?.to_str().ok());
    length.and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// Downloads `url` to the file `dest`.
fn fetch(agent: &ureq::Agent, url: &str, dest: &Path, count: &mut Bytes) -> Result<(), Fail> {
    let error = |e: String| format!("{}: {e}", file_name(url).unwrap_or_else(|| url.to_owned()));
    let mut response = agent.get(url).call().map_err(|e| error(e.to_string()))?;
    fs::create_dir_all(dest.parent().unwrap_or(dest)).map_err(|e| error(e.to_string()))?;
    let mut out = File::create(dest).map_err(|e| error(e.to_string()))?;
    let mut reader = response.body_mut().as_reader().take(MAX_BYTES);
    let mut buffer = vec![0; 64 << 10];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| error(e.to_string()))?;
        if n == 0 {
            break;
        }
        out.write_all(&buffer[..n]).map_err(|e| error(e.to_string()))?;
        count.add(n as u64)?;
    }
    out.flush().map_err(|e| error(e.to_string()))
}

/// A temp folder in `parent`, made.
pub(crate) fn temp_in(parent: &Path) -> Result<Temp, Fail> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp = Temp(parent.join(format!(".downloading-{}-{stamp}", std::process::id())));
    fs::create_dir_all(&temp.0).map_err(|e| format!("{}: {e}", temp.0.display()))?;
    Ok(temp)
}

/// Downloads NGP's package (named by the list at `list`) into the folder `dir`: its path.
fn download_package_from(
    list: &str,
    dir: &Path,
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<PathBuf, Fail> {
    report(&Progress { step: "Reading the car list", ..Progress::default() });
    let url = read_list(list)?.ngp.ok_or("gvrc.de's list names no NGP package")?;
    let package = dir.join(file_name(&url).ok_or("a bad NGP link")?);
    let agent = agent();
    let mut count = Bytes { step: "Downloading NGP", done: 0, total: size_of(&agent, &url), report, cancel };
    count.add(0)?;
    fetch(&agent, &url, &package, &mut count)?;
    Ok(package)
}

/// Downloads NGP's package into the folder `dir`: its path (for `disc_install`'s setup, which reads it).
pub fn download_ngp_package(
    dir: &Path,
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<PathBuf, Fail> {
    download_package_from(LIST_URL, dir, report, cancel)
}

/// Downloads NGP's package (named by the list at `list`) and installs it into the game folder `game`
/// (`disc_install::add_ngp`); the number of files written.
fn download_ngp_from(
    list: &str,
    game: &Path,
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<usize, Fail> {
    disc_install::check_game_folder(game)?;
    let temp = temp_in(game)?;
    let package = download_package_from(list, &temp.0, report, cancel)?;
    disc_install::add_ngp(&package, game, report, cancel)
}

/// Downloads NGP and installs it into the game folder `game`; the number of files written.
pub fn download_ngp(game: &Path, report: &(dyn Fn(&Progress) + Sync), cancel: &AtomicBool) -> Result<usize, Fail> {
    download_ngp_from(LIST_URL, game, report, cancel)
}

/// Downloads `wanted` into a temp folder in `cars_dir` and adds them there (`car_packs::add_downloads`). One car whose
/// download fails does not stop the others; a cancel keeps the cars added before.
pub fn download_cars(
    cars_dir: &Path,
    wanted: &[Entry],
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Vec<Added>, Fail> {
    fs::create_dir_all(cars_dir).map_err(|e| format!("{}: {e}", cars_dir.display()))?;
    let temp = temp_in(cars_dir)?;
    let agent = agent();
    let mut urls: Vec<&str> = Vec::new();
    for entry in wanted {
        for url in [Some(&entry.physics), entry.model.as_ref()].into_iter().flatten() {
            if !urls.contains(&url.as_str()) {
                urls.push(url);
            }
        }
    }
    let total = urls.iter().map(|u| size_of(&agent, u)).sum();
    let mut count = Bytes { step: "Downloading cars", done: 0, total, report, cancel };
    count.add(0)?;
    let mut files: HashMap<&str, Result<PathBuf, Fail>> = HashMap::new();
    for (k, url) in urls.iter().enumerate() {
        let dest = temp.0.join(k.to_string()).join(file_name(url).unwrap_or_default());
        let result = fetch(&agent, url, &dest, &mut count).map(|()| dest);
        if result.as_ref().is_err_and(|e| e.ends_with("cancelled")) {
            return Err("cancelled".into());
        }
        files.insert(url, result);
    }
    let (mut downloads, mut added) = (Vec::new(), Vec::new());
    for entry in wanted {
        let model = entry.model.as_deref().map(|m| files[m].clone()).transpose();
        match (files[entry.physics.as_str()].clone(), model) {
            (Ok(physics), Ok(model)) => downloads.push(Download { physics, model }),
            (Err(e), _) | (_, Err(e)) => added.push(Added { download: entry.name.clone(), result: Err(e) }),
        }
    }
    if !downloads.is_empty() {
        added.extend(car_packs::add_downloads(cars_dir, &downloads, report, cancel)?);
    }
    Ok(added)
}

/// The cars of `catalog` whose name contains `word` (ignoring case); the exact name wins.
fn find<'a>(catalog: &'a Catalog, word: &str) -> Vec<&'a Entry> {
    let word = word.to_lowercase();
    let exact: Vec<&Entry> = catalog.cars.iter().filter(|c| c.name.to_lowercase() == word).collect();
    match exact.is_empty() {
        true => catalog.cars.iter().filter(|c| c.name.to_lowercase().contains(&word)).collect(),
        false => exact,
    }
}

/// The command line's `--download-ngp GAME`.
pub fn ngp_cli(args: &[String]) -> Result<(), String> {
    let [game] = args else {
        return Err("usage: rerbr --download-ngp GAME_FOLDER".into());
    };
    let step = std::sync::Mutex::new("");
    let report = move |p: &Progress| {
        let mut step = step.lock().unwrap();
        if p.step != *step {
            *step = p.step;
            println!("{}…", p.step);
        }
    };
    let files = download_ngp(Path::new(game), &report, &AtomicBool::new(false))?;
    println!("NGP 7.4.778 installed into {game}: {files} files");
    Ok(())
}

/// The command line's `--download-cars [NAME…]`, into `cars_dir`: without names, the list.
pub fn cars_cli(cars_dir: &Path, args: &[String]) -> Result<(), String> {
    let catalog = read_list(LIST_URL)?;
    if args.is_empty() {
        println!("gvrc.de car list {}", catalog.version);
        for car in &catalog.cars {
            println!("{} ({}, {})", car.name, car.cat, car.year);
        }
        return Ok(());
    }
    let mut wanted = Vec::new();
    for word in args {
        match find(&catalog, word)[..] {
            [car] => wanted.push(car.clone()),
            [] => return Err(format!("no car named {word:?} in gvrc.de's list (run --download-cars alone for it)")),
            ref many => {
                let names: Vec<&str> = many.iter().map(|c| c.name.as_str()).collect();
                return Err(format!("{word:?} names {} cars: {}", many.len(), names.join(", ")));
            }
        }
    }
    let added = download_cars(cars_dir, &wanted, &|_| {}, &AtomicBool::new(false))?;
    for a in &added {
        match &a.result {
            Ok((name, Some(model))) => println!("{}: {name}, model Cars/{model}", a.download),
            Ok((name, None)) => println!("{}: {name}, no model", a.download),
            Err(e) => println!("{}: not added: {e}", a.download),
        }
    }
    println!("{}", cars_dir.display());
    car_packs::summary(&added).map(|line| println!("{line}"))
}

/// The Settings page's downloads: the car list and its choice, and NGP's job.
#[derive(Default)]
pub struct Downloads {
    open: bool,
    list: Option<Result<Catalog, Fail>>,
    list_job: Option<Job<Catalog>>,
    search: String,
    /// The chosen cars' physics links.
    picked: HashSet<String>,
    confirm_ngp: bool,
    ngp_job: Option<Job<usize>>,
    ngp_status: Option<Result<String, String>>,
}

/// What "Download cars…" does.
const CARS_TIP: &str = "Lists the NGP cars gvrc.de has, to choose from. ReRBR downloads each car's physics and model \
                        and adds them as Add cars… does.";

impl Downloads {
    /// For `--screenshot`: the NGP confirmation and the car list open with some cars chosen, from a fixed list.
    pub fn screenshot(&mut self) {
        let car = |name: &str, cat: &str, year: &str| Entry {
            name: name.into(),
            cat: cat.into(),
            year: year.into(),
            physics: format!("https://gvrc.de/ngp/physics/{name} - NGP Physics.zip"),
            model: None,
        };
        let cars = vec![
            car("Audi Sport quattro GrpB", "Group B", "1985"),
            car("Citroen C2 R2 Max", "Group R2", "2011"),
            car("Ford Fiesta R5", "Group R5", "2014"),
            car("Lancia Delta HF 4WD GrpA", "Group A8", "1988"),
            car("Mitsubishi Lancer Evo IX N4", "Group N4", "2011"),
        ];
        self.picked = cars[1..3].iter().map(|c| c.physics.clone()).collect();
        self.list = Some(Ok(Catalog { version: "6.175 2025-12-07".into(), ngp: None, cars }));
        (self.open, self.confirm_ngp) = (true, true);
    }
}

impl Form {
    /// Whether NGP can be downloaded into the game folder: it is a Richard Burns Rally folder without NGP (or without
    /// NGP's plugin), not RallySimFans', which has its own.
    pub fn can_download_ngp(&self) -> bool {
        let game = self.folder.game.as_deref();
        let there = game.is_some_and(|g| disc_install::check_game_folder(g).is_ok());
        let missing = self.ngp_revision.is_none() || self.ngp_dll.resolved(&self.data, &self.rsf()).is_none();
        there && missing && self.folder.rsf_root.is_none() && self.child.is_none() && self.downloads.ngp_job.is_none()
    }

    /// "Download NGP…" asked: the confirmation shows.
    pub fn ask_download_ngp(&mut self) {
        self.downloads.ngp_status = None;
        self.downloads.confirm_ngp = true;
    }

    /// The NGP download's confirmation, progress and result line.
    pub fn ngp_download_ui(&mut self, ui: &mut egui::Ui) {
        let game = self.folder.game.clone().unwrap_or_default();
        if self.downloads.confirm_ngp {
            ui.label(format!(
                "Download NGP 7.4.778 from gvrc.de into {}? It adds Plugins/NGP.dll and AudioFMOD/, and replaces \
                 physics.rbz (your retail one is kept as physics.rbz.bak). This is the one thing ReRBR writes in \
                 your game folder.",
                game.display()
            ));
            ui.horizontal(|ui| {
                if ui.button("Download and install").clicked() {
                    self.downloads.confirm_ngp = false;
                    self.downloads.ngp_job =
                        Some(Job::start(move |report, cancel| download_ngp(&game, report, cancel)));
                }
                if ui.button("Cancel").clicked() {
                    self.downloads.confirm_ngp = false;
                }
            });
        }
        if let Some(job) = &mut self.downloads.ngp_job {
            if let Some(result) = ui.horizontal_wrapped(|ui| job.ui(ui)).inner {
                self.downloads.ngp_job = None;
                self.downloads.ngp_status = Some(match result {
                    Err(e) if e.ends_with("cancelled") => Err("Cancelled; nothing was changed.".into()),
                    Err(e) => Err(e),
                    Ok(files) => {
                        self.reload = true;
                        Ok(format!("NGP 7.4.778 installed ({files} files)."))
                    }
                });
            }
        }
        match &self.downloads.ngp_status {
            Some(Ok(text)) => _ = ui.colored_label(GOOD, format!("✔ {text}")),
            Some(Err(text)) => _ = ui.colored_label(WARN, format!("⚠ {text}")),
            None => {}
        }
    }

    /// "Download cars…", which opens and closes the list ([`Form::download_cars_panel`]).
    pub fn download_cars_button(&mut self, ui: &mut egui::Ui) {
        let label = if self.downloads.open { "Hide the list" } else { "Download cars…" };
        if ui.add_enabled(self.child.is_none(), egui::Button::new(label)).on_hover_text(CARS_TIP).clicked() {
            self.downloads.open = !self.downloads.open;
        }
    }

    /// When open, gvrc.de's list with a search and a choice; Download adds the chosen cars like "Add cars…" does (its
    /// progress and result line are the Settings page's).
    pub fn download_cars_panel(&mut self, ui: &mut egui::Ui) {
        if !self.downloads.open {
            return;
        }
        if self.downloads.list.is_none() && self.downloads.list_job.is_none() {
            self.downloads.list_job = Some(list_job());
        }
        if let Some(job) = &mut self.downloads.list_job {
            if let Some(result) = ui.horizontal_wrapped(|ui| job.ui(ui)).inner {
                self.downloads.list_job = None;
                self.downloads.list = Some(result);
            }
        }
        let added: HashSet<String> =
            self.adding.cars.iter().flat_map(|(name, pack)| [car_packs::words(name), car_packs::words(pack)]).collect();
        let busy = !self.can_add_cars();
        let mut start = None;
        let mut reload = false;
        let Downloads { list, search, picked, .. } = &mut self.downloads;
        match list {
            None => {}
            Some(Err(e)) => {
                ui.colored_label(WARN, format!("⚠ {e}"));
                reload = ui.button("Try again").clicked();
            }
            Some(Ok(catalog)) => {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.add(egui::TextEdit::singleline(search).desired_width(220.0));
                    ui.colored_label(
                        Color32::GRAY,
                        format!("{} cars, gvrc.de list {}", catalog.cars.len(), catalog.version),
                    );
                    reload = ui.small_button("Reload").clicked();
                });
                let word = search.to_lowercase();
                egui::ScrollArea::vertical().id_salt("gvrc cars").max_height(280.0).show(ui, |ui| {
                    for car in &catalog.cars {
                        if !word.is_empty() && !format!("{} {}", car.name, car.cat).to_lowercase().contains(&word) {
                            continue;
                        }
                        let mut on = picked.contains(&car.physics);
                        let there = added.contains(&car_packs::words(&car.name));
                        let text = format!(
                            "{}  ({}, {}){}",
                            car.name,
                            car.cat,
                            car.year,
                            if there { "  ✔ added" } else { "" }
                        );
                        if ui.checkbox(&mut on, text).changed() {
                            match on {
                                true => picked.insert(car.physics.clone()),
                                false => picked.remove(&car.physics),
                            };
                        }
                    }
                });
                let wanted: Vec<Entry> = catalog.cars.iter().filter(|c| picked.contains(&c.physics)).cloned().collect();
                ui.horizontal(|ui| {
                    let text = format!("Download {} car{}", wanted.len(), if wanted.len() == 1 { "" } else { "s" });
                    if ui.add_enabled(!busy && !wanted.is_empty(), egui::Button::new(text)).clicked() {
                        start = Some(wanted);
                    }
                    if !picked.is_empty() && ui.small_button("Clear").clicked() {
                        picked.clear();
                    }
                });
            }
        }
        if reload {
            self.downloads.list = None;
        }
        if let Some(wanted) = start {
            self.downloads.picked.clear();
            let dir = self.cars_dir();
            self.start_adding(move |report, cancel| download_cars(&dir, &wanted, report, cancel));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    const LIST: &str = "\u{feff}; File: carList.ini\n; Version: 6.175 2025-12-07\n\n[Plugins]\n\
        plugin_physics_url=https://gvrc.de/ngp/plugin/RBR_NGP-7.4.778.507.7z\n\n\
        [Car_1]\nname=Citroen Xsara (0)\ncat=Original\nlink_physics=https://gvrc.de/ngp/original/physics/c_xsara.zip\nlink_model=\n\n\
        [Car_10]\nname=Audi Sport quattro GrpB\ncat=Group B\nyear=1985\n\
        link_physics=https://gvrc.de/ngp/physics/Audi%20Sport%20quattro%20GrpB%20-%20NGP%20Physics.zip\n\
        link_model=https://gvrc.de/ngp/models/Audi%20Sport%20quattro%20GrpB%20-%20Model.7z\n\
        link_banks=https://gvrc.de/ngp/sounds/Audi%20Sport%20quattro%20GrpB%20-%20banks.7z\n\n\
        ;[Car_11]\n;name=Commented out\n\n\
        [Car_12]\nname=Another host\ncat=Group B\nlink_physics=https://example.com/Another%20-%20NGP%20Physics.zip\n\n\
        [Car_13]\nname=No model link name\ncat=Group A8\nlink_physics=https://gvrc.de/ngp/physics/X%20-%20NGP%20Physics.zip\n\
        link_model=https://gvrc.de/ngp/models/X.zip\n\n\
        [Car_14]\nname=Ford Fiesta R5\ncat=Group R5\nyear=2014\n\
        link_physics=https://gvrc.de/ngp/physics/Ford%20Fiesta%20R5%20-%20NGP%20Physics.zip\n\n[Misc]\naudio_fmod_url=\n";

    #[test]
    fn the_list_gives_ngp_and_the_downloadable_cars() {
        let catalog = parse(LIST);
        assert_eq!(catalog.version, "6.175 2025-12-07");
        assert_eq!(catalog.ngp.as_deref(), Some("https://gvrc.de/ngp/plugin/RBR_NGP-7.4.778.507.7z"));
        let names: Vec<&str> = catalog.cars.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Audi Sport quattro GrpB", "Ford Fiesta R5"]);
        assert_eq!(catalog.cars[0].year, "1985");
        assert_eq!(
            catalog.cars[0].model.as_deref(),
            Some("https://gvrc.de/ngp/models/Audi%20Sport%20quattro%20GrpB%20-%20Model.7z")
        );
        assert_eq!(catalog.cars[1].model, None);
        assert_eq!(find(&catalog, "fiesta").len(), 1);
        assert_eq!(find(&catalog, "ford fiesta r5").len(), 1);
        assert_eq!(find(&catalog, "s").len(), 2);
    }

    #[test]
    fn links_are_gvrcs_and_one_file() {
        assert!(is_gvrc("https://gvrc.de/ngp/x.zip"));
        assert!(!is_gvrc("https://gvrc.de.evil.com/x.zip") && !is_gvrc("http://gvrc.de/x.zip"));
        assert!(!parse("[Plugins]\nplugin_physics_url=http://gvrc.de/x.7z\n").ngp.is_some());
        let name = |u: &str| file_name(u);
        assert_eq!(
            name("https://gvrc.de/ngp/physics/Honda%20Civic%20Type%20R%28FN2%29%20R3%20-%20NGP%20Physics.zip")
                .as_deref(),
            Some("Honda Civic Type R(FN2) R3 - NGP Physics.zip")
        );
        assert_eq!(name("https://gvrc.de/a/%2e%2e%2fx.zip"), None);
        assert_eq!(name("https://gvrc.de/a/..%5Cx"), None);
        assert_eq!(name("https://gvrc.de/a/%zz"), None);
        assert_eq!(name("https://gvrc.de/a/"), None);
        assert_eq!(name("https://gvrc.de/a/.hidden"), None);
    }

    /// Serves `routes` (path, body) on a local port until the process ends; 404 for others.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = routes.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(&stream);
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).unwrap() <= 2 {
                            break;
                        }
                    }
                    let mut parts = line.split_whitespace();
                    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                    let mut out = &stream;
                    match routes.iter().find(|(p, _)| *p == path) {
                        Some((_, body)) => {
                            let _ = write!(
                                out,
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            if method != "HEAD" {
                                let _ = out.write_all(body);
                            }
                        }
                        None => {
                            _ = write!(out, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        }
                    }
                });
            }
        });
        base
    }

    #[test]
    fn files_are_fetched_with_their_size_and_progress() {
        let body: Vec<u8> = (0..300_000u32).map(|n| n as u8).collect();
        let base = serve(vec![("/a.bin", body.clone())]);
        let dir = std::env::temp_dir().join(format!("rbr-gvrc-fetch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (agent, cancel) = (agent(), AtomicBool::new(false));
        let url = format!("{base}/a.bin");
        assert_eq!(size_of(&agent, &url), 300_000);
        assert_eq!(size_of(&agent, &format!("{base}/none")), 0);
        let seen = std::sync::Mutex::new(Vec::new());
        let report = |p: &Progress| seen.lock().unwrap().push(p.done);
        let mut count = Bytes { step: "x", done: 0, total: 300_000, report: &report, cancel: &cancel };
        fetch(&agent, &url, &dir.join("sub/a.bin"), &mut count).unwrap();
        assert_eq!(fs::read(dir.join("sub/a.bin")).unwrap(), body);
        assert_eq!(*seen.lock().unwrap().last().unwrap(), 300_000);
        let missing = fetch(&agent, &format!("{base}/none"), &dir.join("n"), &mut count).unwrap_err();
        assert_eq!(missing, "none: http status: 404");
        cancel.store(true, Ordering::Relaxed);
        let stopped = fetch(&agent, &url, &dir.join("b.bin"), &mut count).unwrap_err();
        assert!(stopped.ends_with("cancelled"), "{stopped}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_car_download_is_reported_and_leaves_no_temp_folder() {
        let base = serve(vec![]);
        let dir = std::env::temp_dir().join(format!("rbr-gvrc-cars-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let car = Entry {
            name: "Ghost Car".into(),
            cat: "Group B".into(),
            year: "1985".into(),
            physics: format!("{base}/Ghost%20Car%20-%20NGP%20Physics.zip"),
            model: None,
        };
        // The links are not gvrc.de's: `file_name` is empty, the download still fails by its 404.
        let added = download_cars(&dir, std::slice::from_ref(&car), &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(added.len(), 1);
        assert!(added[0].result.as_ref().unwrap_err().contains("404"), "{added:?}");
        assert!(car_packs::summary(&added).unwrap_err().starts_with("Nothing added."));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let stopped = download_cars(&dir, &[car], &|_| {}, &AtomicBool::new(true));
        assert_eq!(stopped.unwrap_err(), "cancelled");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }
}

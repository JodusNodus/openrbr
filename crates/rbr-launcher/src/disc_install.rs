//! Setting up a game folder from the retail disc, the official 1.02 patch and the NGP 7 package, without Windows or
//! the installers (docs/install-from-disc.md): `launcher --install-from-disc DISC PATCH NGP OUT`, and the install
//! folder's "Set up from the disc…" button, which runs it on a thread with a progress line and Cancel.
//!
//! OUT gets what the Windows installers leave: the disc installer's `Root` group at the top and its other content
//! groups under their own names (its `_` groups are the installer's own; of the manuals, the English one only), the
//! patch's `Main group` over them, matching names ignoring case (its `PixelShaders/Fog` and `VertexShaders/Fog` go
//! into the retail `fog` folders, a replaced file keeps the existing name), then NGP's files, and last NGP's
//! `physics.rbz` in place of the retail one. Folder names are unshield's (`Main group` is `Main_group`).
//!
//! The patch and NGP are read first, so a wrong one fails before the disc's 2.7 GB are written. Everything is written
//! into a temp folder next to OUT and renamed to OUT at the end; the temp folder is removed on every exit path but a
//! killed process. The inputs are only read.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufReader, Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use rbr_formats::installshield::Cabinet;
use rbr_formats::{installshield, iso9660, rbz};

use crate::job::{Job, Progress, Temp};
use crate::ngp_install;
use crate::pages::{GOOD, WARN};

/// A few files per input that show it is what we expect (ignoring case). Other disc releases may differ.
const RETAIL: [&str; 13] = [
    "physics.rbz",
    "misc.rbz",
    "audio.dat",
    "RichardBurnsRally.ini",
    "Cars/Cars.ini",
    "Maps/Tracks.ini",
    "Maps/track-54_E.lbs",
    "Textures/particles.dat",
    "SGA",
    "SGS",
    "Generic",
    "PixelShaders/fog",
    "VertexShaders/fog",
];
const PATCH: [&str; 7] = [
    "RichardBurnsRally_SSE.exe",
    "HedgeHog3D.dll",
    "LocoMotion.dll",
    "Scripts.rbz",
    "strings.rbz",
    "PixelShaders/Fog/CarGlass.pso",
    "VertexShaders/Fog/CarGlass.vso",
];

/// The game's 2.7 GB with some room.
const FREE_BYTES: u64 = 3 << 30;
/// The patch's group of game files.
const PATCH_GROUP: &str = "Main group";
/// Where the settings page's target folder defaults to, under the home folder.
const DEFAULT_OUT: &str = "Games/Richard Burns Rally";

/// The three downloads and the new game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inputs {
    /// The disc's `.iso`, or the disc as a folder.
    pub disc: PathBuf,
    /// `RBR_v102.zip`, or the patch `.exe` inside it.
    pub patch: PathBuf,
    /// The NGP package's `.7z`, or its folder; none downloads it from gvrc.de.
    pub ngp: Option<PathBuf>,
    /// A new or empty folder.
    pub out: PathBuf,
}

/// The files written from each input (the patch and NGP replace some retail files).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub retail: usize,
    pub patch: usize,
    pub ngp: usize,
}

/// A failed or cancelled setup's message.
type Fail = String;

trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// `name` as unshield writes it: its folder separators as `/`, and in folder names (`folder`) also spaces and `<>[]`
/// as `_`; bytes that are not printable ASCII as `_` everywhere.
fn unshield_name(name: &str, folder: bool) -> String {
    name.chars()
        .map(|c| match c {
            '\\' if folder => '/',
            ' ' | '<' | '>' | '[' | ']' if folder => '_',
            ' '..='~' => c,
            _ => '_',
        })
        .collect()
}

/// Whether the paths `have` (lowercase, `/`-separated, with their folders) hold each of `wanted`; the missing ones.
fn missing<'a>(have: &HashSet<String>, wanted: &[&'a str]) -> Vec<&'a str> {
    wanted.iter().copied().filter(|w| !have.contains(&w.to_lowercase())).collect()
}

/// Each of `paths` and their folders, lowercase.
pub(crate) fn with_folders<'a>(paths: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    let mut set = HashSet::new();
    for path in paths {
        let path = path.to_lowercase();
        let mut at = 0;
        while let Some(k) = path[at..].find('/') {
            set.insert(path[..at + k].to_owned());
            at += k + 1;
        }
        set.insert(path);
    }
    set
}

pub(crate) fn require(have: &HashSet<String>, wanted: &[&str], what: String) -> Result<(), Fail> {
    match missing(have, wanted) {
        lacks if lacks.is_empty() => Ok(()),
        lacks => Err(format!("{what} lacks {}: is it the right file?", lacks.join(", "))),
    }
}

/// The game folder being written: each path's name as first written, found ignoring case (as the game, and the
/// Windows file system, find them).
struct Tree {
    root: PathBuf,
    /// Lowercase path to written path, with the folders (`true`).
    names: HashMap<String, (String, bool)>,
}

impl Tree {
    fn new(root: PathBuf) -> io::Result<Tree> {
        fs::create_dir(&root)?;
        Ok(Tree { root, names: HashMap::new() })
    }

    /// The path for `rel` below the game folder: the names already written kept, its new folders made. The entry
    /// goes through `archive::safe_path`, so a `.7z` path cannot escape the folder (`C:/x/y`, `..` and absolute
    /// paths stay inside or fail, and `\` is a separator).
    fn path(&mut self, rel: &str, dir: bool) -> io::Result<PathBuf> {
        let parts = crate::archive::safe_path(rel)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("a bad path {rel}")))?;
        let mut written = String::new();
        for (k, part) in parts.iter().enumerate() {
            let folder = dir || k + 1 < parts.len();
            let joined = if written.is_empty() { part.to_string() } else { format!("{written}/{part}") };
            let (name, is_dir) = match self.names.entry(joined.to_lowercase()) {
                Entry::Occupied(entry) => entry.get().clone(),
                Entry::Vacant(entry) => {
                    if folder {
                        fs::create_dir(self.root.join(&joined))?;
                    }
                    entry.insert((joined, folder)).clone()
                }
            };
            if is_dir != folder {
                let what = if folder { "a file" } else { "a folder" };
                return Err(io::Error::other(format!("{rel} would replace {what}, {name}")));
            }
            written = name;
        }
        Ok(self.root.join(written))
    }

    /// The lowercase paths written, with their folders.
    fn have(&self) -> HashSet<String> {
        self.names.keys().cloned().collect()
    }
}

/// Counts the bytes written, reports them, and stops when cancelled.
struct Writer<'a> {
    file: io::BufWriter<File>,
    reporter: &'a Reporter<'a>,
}

impl Writer<'_> {
    fn create<'a>(path: &Path, reporter: &'a Reporter<'a>) -> io::Result<Writer<'a>> {
        Ok(Writer { file: io::BufWriter::with_capacity(1 << 20, File::create(path)?), reporter })
    }
}

impl Write for Writer<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.reporter.stopped() {
            return Err(io::Error::other("cancelled"));
        }
        let n = self.file.write(buf)?;
        self.reporter.add(n as u64);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Passes the progress on, at most every 8 MB, from the writing threads.
pub(crate) struct Reporter<'a> {
    step: &'static str,
    pub(crate) total: u64,
    done: AtomicU64,
    reported: AtomicU64,
    report: &'a (dyn Fn(&Progress) + Sync),
    cancel: &'a AtomicBool,
    /// A writing thread failed: the others stop too.
    failed: AtomicBool,
}

impl<'a> Reporter<'a> {
    pub(crate) fn new(report: &'a (dyn Fn(&Progress) + Sync), cancel: &'a AtomicBool) -> Self {
        Reporter {
            step: "",
            total: 0,
            done: AtomicU64::new(0),
            reported: AtomicU64::new(0),
            report,
            cancel,
            failed: AtomicBool::new(false),
        }
    }

    pub(crate) fn step(&mut self, step: &'static str) -> Result<(), Fail> {
        if self.stopped() {
            return Err("cancelled".into());
        }
        self.step = step;
        (self.report)(&self.progress());
        Ok(())
    }

    fn progress(&self) -> Progress {
        Progress { step: self.step, done: self.done.load(Ordering::Relaxed), total: self.total }
    }

    fn stopped(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || self.failed.load(Ordering::Relaxed)
    }

    fn add(&self, n: u64) {
        let done = self.done.fetch_add(n, Ordering::Relaxed) + n;
        if done - self.reported.load(Ordering::Relaxed) >= 8 << 20 {
            self.reported.store(done, Ordering::Relaxed);
            (self.report)(&self.progress());
        }
    }
}

/// The threads writing files.
const THREADS: usize = 4;

/// One cabinet set: its header and its volumes (`data<n>.cab`).
struct Cabinets<'a> {
    cab: Cabinet,
    open: Box<dyn Fn(u16) -> io::Result<Box<dyn ReadSeek + 'a>> + Sync + 'a>,
}

impl Cabinets<'_> {
    /// Writes the files (index, path) in `tree`, on a few threads; where two have one path, the later one.
    fn extract(&self, tree: &mut Tree, files: &[(usize, String)], reporter: &Reporter) -> Result<(), Fail> {
        let mut seen = HashSet::new();
        let mut jobs = Vec::new();
        for (index, rel) in files.iter().rev() {
            let path = tree.path(rel, false).map_err(|e| format!("{rel}: {e}"))?;
            if seen.insert(path.clone()) {
                jobs.push((*index, rel, path));
            }
        }
        let next = AtomicUsize::new(0);
        let work = || -> Result<(), Fail> {
            while let Some((index, rel, path)) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                let mut out = Writer::create(path, reporter).map_err(|e| format!("{}: {e}", path.display()))?;
                let result = self.cab.extract(*index, &mut |n| (self.open)(n), &mut out).and_then(|_| out.flush());
                result.map_err(|e| {
                    reporter.failed.store(!reporter.cancel.load(Ordering::Relaxed), Ordering::Relaxed);
                    format!("{rel}: {e}")
                })?;
            }
            Ok(())
        };
        std::thread::scope(|s| {
            let threads: Vec<_> = (0..THREADS).map(|_| s.spawn(work)).collect();
            let results: Vec<_> =
                threads.into_iter().map(|t| t.join().unwrap_or(Err("a writing thread failed".into()))).collect();
            // The failure, not the others' stop it caused.
            let mut errors = results.into_iter().filter_map(Result::err);
            let first = errors.next();
            match errors.find(|e| !e.ends_with("cancelled")) {
                Some(e) if first.as_ref().is_some_and(|f| f.ends_with("cancelled")) => Err(e),
                _ => first.map_or(Ok(()), Err),
            }
        })
    }

    /// The valid files of `group`, with where they go below the group's folder.
    fn files(&self, group: &installshield::Group) -> Vec<(usize, String)> {
        group
            .files
            .clone()
            .filter(|&k| self.cab.files[k].is_valid())
            .map(|k| {
                let file = &self.cab.files[k];
                let dir = unshield_name(&file.dir, true);
                let name = unshield_name(&file.name, false);
                (k, if dir.trim_matches('/').is_empty() { name } else { format!("{}/{name}", dir.trim_matches('/')) })
            })
            .collect()
    }
}

/// The retail disc: its image's tree, or its folder.
enum Disc {
    Image(PathBuf, Vec<iso9660::Entry>),
    Folder(PathBuf),
}

impl Disc {
    fn open(path: &Path) -> Result<Disc, Fail> {
        if path.is_dir() {
            return Ok(Disc::Folder(path.to_owned()));
        }
        let mut image = BufReader::new(File::open(path).map_err(|e| format!("{}: {e}", path.display()))?);
        let entries = iso9660::read_tree(&mut image).map_err(|e| {
            format!("{} is not the Richard Burns Rally disc (its .iso or its folder): {e}", path.display())
        })?;
        Ok(Disc::Image(path.to_owned(), entries))
    }

    /// The file `name` at the disc's top, its name's case ignored.
    fn file(&self, name: &str) -> io::Result<Option<Box<dyn ReadSeek>>> {
        Ok(match self {
            Disc::Image(path, entries) => match entries.iter().find(|e| !e.dir && e.path.eq_ignore_ascii_case(name)) {
                Some(entry) => Some(Box::new(iso9660::Extent::new(BufReader::new(File::open(path)?), entry)?)),
                None => None,
            },
            Disc::Folder(dir) => match fs::read_dir(dir)?
                .filter_map(Result::ok)
                .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_file())
            {
                Some(entry) => Some(Box::new(BufReader::new(File::open(entry.path())?))),
                None => None,
            },
        })
    }
}

/// The disc's cabinets.
fn disc_cabinets(disc: &Path) -> Result<Cabinets<'static>, Fail> {
    let source = Disc::open(disc)?;
    let error = |e: io::Error| format!("{}: {e}", disc.display());
    let mut hdr = Vec::new();
    source
        .file("data1.hdr")
        .map_err(error)?
        .ok_or_else(|| format!("{} has no data1.hdr: is it the Richard Burns Rally disc?", disc.display()))?
        .read_to_end(&mut hdr)
        .map_err(error)?;
    let cab = Cabinet::parse(&hdr).map_err(error)?;
    let open = move |n: u16| {
        source
            .file(&format!("data{n}.cab"))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no data{n}.cab")))
    };
    Ok(Cabinets { cab, open: Box::new(open) })
}

/// The patch's `.exe`: from the zip, or the file.
fn patch_exe(path: &Path) -> Result<Vec<u8>, Fail> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let Ok(mut zip) = rbz::ZipArchive::new(BufReader::new(file)) else {
        return fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    };
    let exes: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .filter(|n| n.to_lowercase().ends_with(".exe"))
        .map(|n| n.into_owned())
        .collect();
    let [exe] = &exes[..] else {
        return Err(format!("{} should hold one .exe, the patch; it holds {exes:?}", path.display()));
    };
    let mut bytes = Vec::new();
    zip.by_name(exe)
        .and_then(|mut f| Ok(f.read_to_end(&mut bytes)?))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(bytes)
}

/// The patch's cabinets, inside its `.exe`'s overlay.
fn patch_cabinets<'a>(path: &Path, exe: &'a [u8]) -> Result<Cabinets<'a>, Fail> {
    let not = |e: String| format!("{} is not the InstallShield patch: {e}", path.display());
    let files = installshield::sfx_files(exe).map_err(|e| not(e.to_string()))?;
    let file = |name: &str| {
        files.iter().find(|(p, _)| p.replace('\\', "/").eq_ignore_ascii_case(name)).map(|(_, r)| &exe[r.clone()])
    };
    let hdr = file("Disk1/data1.hdr")
        .ok_or_else(|| format!("{} has no Disk1/data1.hdr: is it the 1.02 patch?", path.display()))?;
    let cab = Cabinet::parse(hdr).map_err(|e| not(e.to_string()))?;
    let volumes: Vec<Option<&'a [u8]>> = (0..10).map(|n| file(&format!("Disk1/data{n}.cab"))).collect();
    let open = move |n: u16| match volumes.get(n as usize).copied().flatten() {
        Some(bytes) => Ok(Box::new(Cursor::new(bytes)) as Box<dyn ReadSeek + 'a>),
        None => Err(io::Error::new(io::ErrorKind::NotFound, format!("no Disk1/data{n}.cab"))),
    };
    Ok(Cabinets { cab, open: Box::new(open) })
}

/// Removes the temp folder however the work ends.
/// The free bytes where `dir` is, where the platform tells.
fn free_bytes(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let stat = rustix::fs::statvfs(dir).ok()?;
        Some(stat.f_bavail.saturating_mul(stat.f_frsize))
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        None
    }
}

/// Sets up `inputs.out`; `report` gets the progress, and `cancel` stops it. On an error OUT is left as it was.
pub fn install(inputs: &Inputs, report: &(dyn Fn(&Progress) + Sync), cancel: &AtomicBool) -> Result<Counts, Fail> {
    // NGP's package, downloaded first when none is given, so that a failure comes before the disc is read.
    let (_download, ngp) = match &inputs.ngp {
        Some(path) => (None, path.clone()),
        None => {
            let out = std::path::absolute(&inputs.out).map_err(|e| format!("{}: {e}", inputs.out.display()))?;
            let parent = out.parent().ok_or_else(|| format!("{} has no parent folder", out.display()))?;
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            let temp = Temp::create(parent, "downloading")?;
            let package = crate::gvrc::download_ngp_package(&temp.0, report, cancel)?;
            (Some(temp), package)
        }
    };
    let paths = [&inputs.disc, &inputs.patch, &ngp];
    into_new_folder(&paths, &inputs.out, report, cancel, |game, reporter| setup(inputs, &ngp, game, reporter))
}

/// Runs `work` on a folder in a temp folder next to `out` (new or empty), checked that each of `inputs` exists, and
/// renames the folder to `out` when it succeeds. On an error OUT is left as it was.
fn into_new_folder<T>(
    inputs: &[&PathBuf],
    out: &Path,
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
    work: impl FnOnce(&Path, &mut Reporter) -> Result<T, Fail>,
) -> Result<T, Fail> {
    for path in inputs {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
    }
    let out = std::path::absolute(out).map_err(|e| format!("{}: {e}", out.display()))?;
    if out.exists() && (!out.is_dir() || fs::read_dir(&out).map_or(true, |mut d| d.next().is_some())) {
        return Err(format!("{} is not empty: choose a new or empty game folder", out.display()));
    }
    let parent = out.parent().ok_or_else(|| format!("{} has no parent folder", out.display()))?;
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    if free_bytes(parent).is_some_and(|free| free < FREE_BYTES) {
        return Err(format!(
            "{} needs {} GB free: make room, or choose a game folder on another disk",
            parent.display(),
            FREE_BYTES >> 30
        ));
    }
    let temp = Temp::create(parent, "rbr-install")?;
    let mut reporter = Reporter::new(report, cancel);
    let done = work(&temp.0.join("game"), &mut reporter)?;
    if out.is_dir() {
        fs::remove_dir(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    fs::rename(temp.0.join("game"), &out).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(done)
}

/// The test data (docs/development.md "Set up") in `out` (new or empty): `retail-install/`, every group of the disc's
/// installer in its own folder as unshield extracts them (`RBR_DATA`), and `Main_group/`, the 1.02 patch's game files
/// (`RBR_PATCH`). The files written from the disc and the patch.
pub fn extract_test_data(
    disc: &Path,
    patch: &Path,
    out: &Path,
    report: &(dyn Fn(&Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Counts, Fail> {
    let (disc, patch) = (disc.to_owned(), patch.to_owned());
    into_new_folder(&[&disc, &patch], out, report, cancel, |root, reporter| {
        reporter.step("Reading the 1.02 patch")?;
        let exe = patch_exe(&patch)?;
        let patch_cab = patch_cabinets(&patch, &exe)?;
        let group =
            patch_cab.cab.group(PATCH_GROUP).ok_or_else(|| format!("{} has no {PATCH_GROUP}", patch.display()))?;
        let patch_files = patch_cab.files(&group.clone());
        require(
            &with_folders(patch_files.iter().map(|(_, p)| p.as_str())),
            &PATCH,
            format!("the 1.02 patch {}", patch.display()),
        )?;

        reporter.step("Reading the retail disc")?;
        let disc_cab = disc_cabinets(&disc)?;
        if !disc_cab.cab.groups.iter().any(|g| g.name.eq_ignore_ascii_case("Root")) {
            return Err(format!(
                "{}: the installer has no Root files: is it the Richard Burns Rally disc?",
                disc.display()
            ));
        }
        let mut retail = Vec::new();
        for group in &disc_cab.cab.groups {
            let name = unshield_name(&group.name, true);
            retail.extend(disc_cab.files(group).into_iter().map(|(index, rel)| (index, format!("{name}/{rel}"))));
        }

        fs::create_dir(root).map_err(|e| format!("{}: {e}", root.display()))?;
        let mut install = Tree::new(root.join("retail-install")).map_err(|e| format!("{}: {e}", root.display()))?;
        let mut main = Tree::new(root.join("Main_group")).map_err(|e| format!("{}: {e}", root.display()))?;
        let sizes =
            |files: &[(usize, String)], cab: &Cabinet| files.iter().map(|&(k, _)| cab.files[k].size).sum::<u64>();
        reporter.total = sizes(&retail, &disc_cab.cab) + sizes(&patch_files, &patch_cab.cab);
        reporter.step("Writing the retail install")?;
        disc_cab.extract(&mut install, &retail, reporter)?;
        reporter.step("Writing the 1.02 patch")?;
        patch_cab.extract(&mut main, &patch_files, reporter)?;
        Ok(Counts { retail: unique(&retail), patch: unique(&patch_files), ngp: 0 })
    })
}

/// The setup's steps, writing the game into `game`.
fn setup(inputs: &Inputs, ngp: &Path, game: &Path, reporter: &mut Reporter) -> Result<Counts, Fail> {
    // The small patch and NGP first, so that a wrong one fails before the disc is read.
    reporter.step("Reading the 1.02 patch")?;
    let exe = patch_exe(&inputs.patch)?;
    let patch = patch_cabinets(&inputs.patch, &exe)?;
    let group =
        patch.cab.group(PATCH_GROUP).ok_or_else(|| format!("{} has no {PATCH_GROUP}", inputs.patch.display()))?;
    let patch_files = patch.files(&group.clone());
    let what = format!("the 1.02 patch {}", inputs.patch.display());
    require(&with_folders(patch_files.iter().map(|(_, p)| p.as_str())), &PATCH, what)?;

    reporter.step("Reading NGP")?;
    let ngp_install::Package { files: ngp, dirs: ngp_dirs, physics: ngp_physics } = ngp_install::read_ngp(ngp)?;
    // An unsupported NGP version fails before the disc is read.
    ngp_install::supported_dll(&ngp)?;

    reporter.step("Reading the retail disc")?;
    let disc = disc_cabinets(&inputs.disc)?;
    let retail = retail_files(&disc, &inputs.disc)?;

    let mut tree = Tree::new(game.to_owned()).map_err(|e| format!("{}: {e}", game.display()))?;
    let sizes = |files: &[(usize, String)], cab: &Cabinet| files.iter().map(|&(k, _)| cab.files[k].size).sum::<u64>();
    reporter.total = sizes(&retail, &disc.cab)
        + sizes(&patch_files, &patch.cab)
        + ngp.iter().map(|(_, b)| b.len() as u64).sum::<u64>()
        + ngp_physics.len() as u64;
    let mut counts = Counts::default();
    reporter.step("Writing the retail game")?;
    disc.extract(&mut tree, &retail, reporter)?;
    counts.retail = unique(&retail);
    require(&tree.have(), &RETAIL, format!("the retail disc {}", inputs.disc.display()))?;
    drop(disc);

    reporter.step("Writing the 1.02 patch")?;
    patch.extract(&mut tree, &patch_files, reporter)?;
    counts.patch = unique(&patch_files);

    reporter.step("Writing NGP")?;
    for dir in &ngp_dirs {
        tree.path(dir, true).map_err(|e| format!("{dir}: {e}"))?;
    }
    for (rel, bytes) in &ngp {
        write_bytes(&mut tree, rel, bytes, reporter)?;
    }
    replace_physics_with_ngps(&mut tree, &ngp_physics, reporter)?;
    counts.ngp = ngp.len() + 1;
    Ok(counts)
}

/// NGP's `physics.rbz` in place of the retail one, as NGP's own install does. A step of its own: the retail archive
/// could be kept under another name here.
fn replace_physics_with_ngps(tree: &mut Tree, physics: &[u8], reporter: &Reporter) -> Result<(), Fail> {
    write_bytes(tree, "physics.rbz", physics, reporter)
}

fn write_bytes(tree: &mut Tree, rel: &str, bytes: &[u8], reporter: &Reporter) -> Result<(), Fail> {
    let path = tree.path(rel, false).map_err(|e| format!("{rel}: {e}"))?;
    write_to(&path, bytes, reporter)
}

/// Writes `bytes` to `path`, making its folder.
pub(crate) fn write_to(path: &Path, bytes: &[u8], reporter: &Reporter) -> Result<(), Fail> {
    let error = |e: io::Error| format!("{}: {e}", path.display());
    fs::create_dir_all(path.parent().unwrap_or(path)).map_err(error)?;
    let mut out = Writer::create(path, reporter).map_err(error)?;
    out.write_all(bytes).and_then(|()| out.flush()).map_err(error)
}

/// The number of distinct paths written.
fn unique(files: &[(usize, String)]) -> usize {
    files.iter().map(|(_, p)| p.to_lowercase()).collect::<HashSet<_>>().len()
}

/// The disc's game files and where they go: the `Root` group at the top, then the other groups but the installer's
/// (`_...`) by name, each under its name; of the manuals, the English one only.
fn retail_files(disc: &Cabinets, path: &Path) -> Result<Vec<(usize, String)>, Fail> {
    let mut groups: Vec<(String, &installshield::Group)> = disc
        .cab
        .groups
        .iter()
        .map(|g| (unshield_name(&g.name, true), g))
        .filter(|(name, _)| !name.starts_with('_'))
        .collect();
    if !groups.iter().any(|(name, _)| name.eq_ignore_ascii_case("Root")) {
        return Err(format!(
            "{}: the installer has no Root files: is it the Richard Burns Rally disc?",
            path.display()
        ));
    }
    // Root first, the others by name.
    groups.sort_by_key(|(name, _)| (!name.eq_ignore_ascii_case("Root"), name.clone()));
    let mut files = Vec::new();
    for (name, group) in groups {
        let root = name.eq_ignore_ascii_case("Root");
        let manuals = name.eq_ignore_ascii_case("Manuals");
        for (index, rel) in disc.files(group) {
            if manuals && !rel.contains('/') && !rel.to_uppercase().ends_with("_ENG.PDF") {
                continue;
            }
            files.push((index, if root { rel } else { format!("{name}/{rel}") }));
        }
    }
    Ok(files)
}

/// The command line's `--extract-test-data DISC PATCH OUT`: [`extract_test_data`], its steps and its counts.
pub fn test_data_cli(args: &[String]) -> Result<(), String> {
    let [disc, patch, out] = args else {
        return Err("usage: rerbr --extract-test-data DISC PATCH OUT".into());
    };
    let counts =
        extract_test_data(disc.as_ref(), patch.as_ref(), out.as_ref(), &print_steps(), &AtomicBool::new(false))?;
    let out = Path::new(out);
    println!("{}: {} files from the disc, {} from the patch", out.display(), counts.retail, counts.patch);
    println!("RBR_DATA={}", out.join("retail-install").display());
    println!("RBR_PATCH={}", out.join("Main_group").display());
    Ok(())
}

/// The command line's `--install-from-disc DISC PATCH NGP OUT`: the setup, its steps and what OUT holds.
pub fn cli(args: &[String]) -> Result<(), String> {
    let [disc, patch, ngp, out] = args else {
        return Err("usage: rerbr --install-from-disc DISC PATCH NGP|- OUT".into());
    };
    let ngp = (ngp != "-").then(|| ngp.into());
    let inputs = Inputs { disc: disc.into(), patch: patch.into(), ngp, out: out.into() };
    let counts = install(&inputs, &print_steps(), &AtomicBool::new(false))?;
    print!("{}", summary(&inputs.out, counts));
    Ok(())
}

/// A progress report for the command line: each step's name once.
fn print_steps() -> impl Fn(&Progress) + Sync {
    let step = Mutex::new("");
    move |p: &Progress| {
        let mut step = step.lock().unwrap();
        if p.step != *step {
            *step = p.step;
            println!("{}…", p.step);
        }
    }
}

/// What OUT holds: the files from each input, and the size and file count of each top-level entry.
pub fn summary(out: &Path, counts: Counts) -> String {
    let mut text = format!(
        "\n{}: {} files from retail, {} from the patch, {} from NGP (the patch and NGP replace some retail files)\n",
        out.display(),
        counts.retail,
        counts.patch,
        counts.ngp
    );
    let mut entries: Vec<_> = fs::read_dir(out).into_iter().flatten().filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for entry in entries {
        let (files, bytes) = size(&entry.path());
        let slash = if entry.path().is_dir() { "/" } else { "" };
        text += &format!("  {:9.1} MB {files:5} {}{slash}\n", bytes as f64 / 1e6, entry.file_name().to_string_lossy());
    }
    text + "Choose it as the launcher's game folder (Settings).\n"
}

/// The files below `path` (or the file) and their bytes.
fn size(path: &Path) -> (usize, u64) {
    match fs::read_dir(path) {
        Ok(entries) => {
            entries.filter_map(Result::ok).map(|e| size(&e.path())).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
        }
        Err(_) => (1, fs::metadata(path).map_or(0, |m| m.len())),
    }
}

/// The disc setup of `inputs` as a [`Job`].
fn setup_job(inputs: Inputs) -> Job<Counts> {
    Job::start(move |report, cancel| install(&inputs, report, cancel))
}

/// The "Set up from the disc…" panel under the install folder's field: the three downloads, the target folder, Set
/// up, and while it runs the progress line and Cancel.
#[derive(Default)]
pub struct DiscSetup {
    pub open: bool,
    disc: String,
    patch: String,
    ngp: String,
    out: String,
    job: Option<Job<Counts>>,
    /// The last setup's outcome: what was set up, or why not.
    status: Option<Result<String, String>>,
}

impl DiscSetup {
    /// The panel, when open; the new game folder once a setup has finished.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        if !self.open {
            return None;
        }
        if self.out.is_empty() {
            let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
            self.out = home.map(|h| h.join(DEFAULT_OUT).display().to_string()).unwrap_or_default();
        }
        let mut done = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Set up the game from the Richard Burns Rally disc and the official 1.02 patch, with NGP 7.");
            ui.colored_label(
                egui::Color32::GRAY,
                "The patch (RBR_v102.zip) is a free download. NGP (RBR_NGP-7.4.778.507.7z) is downloaded from gvrc.de \
                 unless you choose a copy. The disc and the files you choose are only read. The game takes about \
                 2.7 GB.",
            );
            let running = self.job.is_some();
            ui.add_enabled_ui(!running, |ui| {
                crate::pages::fields(ui, "disc_setup", |ui| {
                    let rows = [
                        (
                            "Disc",
                            &mut self.disc,
                            "the disc's .iso, or the disc as a folder",
                            "The Richard Burns Rally disc image",
                        ),
                        ("1.02 patch", &mut self.patch, "RBR_v102.zip, or the patch .exe", "The 1.02 patch"),
                        (
                            "NGP",
                            &mut self.ngp,
                            "empty: downloaded from gvrc.de; or the NGP package's .7z or folder",
                            "The NGP 7 package",
                        ),
                    ];
                    for (label, field, hint, prompt) in rows {
                        ui.label(label);
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(field).hint_text(hint).desired_width(420.0));
                            if ui.button("Choose…").clicked() {
                                if let Some(path) = crate::job::pick_file(prompt) {
                                    *field = path;
                                }
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("Game folder");
                    ui.horizontal(|ui| {
                        let hint = "a new or empty folder";
                        ui.add(egui::TextEdit::singleline(&mut self.out).hint_text(hint).desired_width(420.0));
                        if ui.button("Choose…").clicked() {
                            if let Some(dir) = crate::job::pick_folder("The new game folder") {
                                self.out = dir;
                            }
                        }
                    });
                    ui.end_row();
                });
            });
            ui.horizontal(|ui| match &mut self.job {
                Some(job) => {
                    if let Some(result) = job.ui(ui) {
                        self.job = None;
                        let out = self.out.trim().to_owned();
                        self.status = Some(match result {
                            Ok(_) => {
                                done = Some(out.clone());
                                Ok(format!("The game is set up in {out}. It is now your game folder."))
                            }
                            Err(e) if e.ends_with("cancelled") => Err("Cancelled; nothing was written.".into()),
                            Err(e) => Err(e),
                        });
                    }
                }
                None => {
                    let fields = [&self.disc, &self.patch, &self.out];
                    let ready = fields.iter().all(|f| !f.trim().is_empty());
                    if ui.add_enabled(ready, egui::Button::new("Set up")).clicked() {
                        let path = |f: &String| PathBuf::from(f.trim());
                        let inputs = Inputs {
                            disc: path(&self.disc),
                            patch: path(&self.patch),
                            ngp: Some(self.ngp.trim()).filter(|n| !n.is_empty()).map(PathBuf::from),
                            out: path(&self.out),
                        };
                        self.status = None;
                        self.job = Some(setup_job(inputs));
                    }
                    if ui.button("Close").clicked() {
                        self.open = false;
                    }
                }
            });
            match &self.status {
                Some(Ok(text)) => ui.colored_label(GOOD, format!("✔ {text}")),
                Some(Err(text)) => ui.colored_label(WARN, format!("⚠ {text}")),
                None => ui.label(""),
            };
        });
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ngp_install::NGP;

    #[test]
    fn names_are_unshields() {
        assert_eq!(unshield_name("Main group", true), "Main_group");
        assert_eq!(unshield_name("Maps\\Sub [x]", true), "Maps/Sub__x_");
        assert_eq!(unshield_name("RBR Startup.exe", false), "RBR Startup.exe");
        assert_eq!(unshield_name("caf\u{e9}", false), "caf_");
    }

    #[test]
    fn the_checks_name_what_is_missing() {
        let have = with_folders(["Plugins/NGP.dll", "physics.rbz"]);
        assert!(have.contains("plugins"));
        assert_eq!(missing(&have, &NGP), ["AudioFMOD"]);
        let err = require(&have, &NGP, "the NGP package x".into()).unwrap_err();
        assert_eq!(err, "the NGP package x lacks AudioFMOD: is it the right file?");
    }

    #[test]
    fn the_tree_keeps_the_first_names_case() {
        let dir = crate::test_dir("disc-tree");
        fs::create_dir_all(&dir).unwrap();
        let mut tree = Tree::new(dir.join("game")).unwrap();
        let a = tree.path("PixelShaders/fog/a.pso", false).unwrap();
        let b = tree.path("PixelShaders/Fog/CarGlass.pso", false).unwrap();
        assert_eq!(b, dir.join("game/PixelShaders/fog/CarGlass.pso"));
        assert_eq!(a.parent(), b.parent());
        assert!(tree.path("PixelShaders", false).is_err());
        assert!(tree.path("../x", false).is_err());
        assert_eq!(missing(&tree.have(), &["pixelshaders/FOG", "x"]), ["x"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// `Tree::path` through `archive::safe_path`: drive letters, parent climbs and stray separators fail, and
    /// backslash-separated and absolute-looking entries land below the folder instead of escaping it.
    #[test]
    fn the_tree_keeps_unsafe_entries_inside() {
        let dir = crate::test_dir("disc-unsafe-tree");
        fs::create_dir_all(&dir).unwrap();
        let mut tree = Tree::new(dir.join("game")).unwrap();
        for bad in ["C:/x/y", "C:\\x", "..", "../x", "..\\x", "a/../../x", "\\", "a/C:x/b"] {
            assert!(tree.path(bad, false).is_err(), "{bad}");
        }
        for good in ["a\\b", "/abs/path", "a/./b"] {
            let path = tree.path(good, false).unwrap();
            assert!(path.starts_with(dir.join("game")), "{good}: {}", path.display());
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bad_input_leaves_nothing_behind() {
        let dir = crate::test_dir("disc-bad");
        fs::create_dir_all(dir.join("full")).unwrap();
        fs::write(dir.join("full/x"), b"x").unwrap();
        fs::write(dir.join("patch.exe"), b"MZ").unwrap();
        let inputs = |out: &str| Inputs {
            disc: dir.clone(),
            patch: dir.join("patch.exe"),
            ngp: Some(dir.clone()),
            out: dir.join(out),
        };
        let run = |i: &Inputs| install(i, &|_| {}, &AtomicBool::new(false));
        assert!(run(&inputs("full")).unwrap_err().contains("is not empty"));
        assert!(run(&Inputs { disc: dir.join("none"), ..inputs("out") }).unwrap_err().ends_with("does not exist"));
        assert!(run(&inputs("out")).unwrap_err().contains("is not the InstallShield patch"));
        let left: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left.len(), 2, "{left:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The whole setup from the three downloads (`RBR_DISC`, `RBR_PATCH_ZIP`, `RBR_NGP_7Z`), against the extracted
    /// install (`RBR_DATA`, its groups' folders) and the patch's `Main_group` (`RBR_PATCH`): the same paths (ignoring
    /// case) and the same bytes, and NGP's files. A cancelled setup leaves nothing.
    #[test]
    fn sets_up_the_game_from_the_downloads() {
        let env = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let (Some(disc), Some(patch), Some(ngp), Some(data), Some(main)) =
            (env("RBR_DISC"), env("RBR_PATCH_ZIP"), env("RBR_NGP_7Z"), env("RBR_DATA"), env("RBR_PATCH"))
        else {
            eprintln!("skipping: RBR_DISC, RBR_PATCH_ZIP, RBR_NGP_7Z, RBR_DATA or RBR_PATCH not set");
            return;
        };
        let dir = crate::test_dir("disc-e2e");
        fs::create_dir_all(&dir).unwrap();
        let inputs = Inputs { disc, patch, ngp: Some(ngp), out: dir.join("game") };

        // Cancelled once the retail game is being written: no OUT, no temp folder.
        let cancel = AtomicBool::new(false);
        let err = install(&inputs, &|p| cancel.store(p.done > 0, Ordering::Relaxed), &cancel).unwrap_err();
        assert!(err.ends_with("cancelled"), "{err}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);

        let counts = install(&inputs, &|_| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(counts, Counts { retail: 1821, patch: 43, ngp: 38 });
        fn files(root: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) {
            for entry in fs::read_dir(root.join(rel)).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                let rel = if rel.is_empty() { name } else { format!("{rel}/{name}") };
                if entry.path().is_dir() {
                    files(root, &rel, out);
                } else {
                    out.push((rel, entry.path()));
                }
            }
        }
        // The expected files: the install's groups as laid out, the patch over them, then NGP's.
        let mut expected: HashMap<String, Option<PathBuf>> = HashMap::new();
        let mut groups: Vec<_> =
            fs::read_dir(&data).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        groups.retain(|g| !g.starts_with('_') && data.join(g).is_dir());
        for group in groups {
            let mut found = Vec::new();
            files(&data.join(&group), "", &mut found);
            for (rel, path) in found {
                let manual = group == "Manuals" && !rel.to_uppercase().ends_with("_ENG.PDF");
                if !manual {
                    let rel = if group == "Root" { rel } else { format!("{group}/{rel}") };
                    expected.insert(rel.to_lowercase(), Some(path));
                }
            }
        }
        let mut found = Vec::new();
        files(&main, "", &mut found);
        expected.extend(found.into_iter().map(|(rel, path)| (rel.to_lowercase(), Some(path))));
        expected.insert("physics.rbz".into(), None);
        assert!(crate::ngp_install::is_ngp_archive(&fs::read(inputs.out.join("physics.rbz")).unwrap()));
        let mut out = Vec::new();
        files(&inputs.out, "", &mut out);
        // NGP's other files: in Plugins/ and AudioFMOD/, from the package only.
        for (rel, _) in &out {
            let lower = rel.to_lowercase();
            if lower.starts_with("plugins/") || lower.starts_with("audiofmod/") {
                expected.entry(lower).or_insert(None);
            }
        }
        assert_eq!(out.len(), 1879);
        assert_eq!(out.len(), expected.len());
        for (rel, path) in &out {
            let want = expected.get(&rel.to_lowercase()).unwrap_or_else(|| panic!("{rel} is not expected"));
            if let Some(want) = want {
                assert!(fs::read(path).unwrap() == fs::read(want).unwrap(), "{rel} differs from {}", want.display());
            }
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    /// The panel's states drawn offscreen to `RBR_SCREENSHOTS`, with a real setup from the downloads (`RBR_DISC`,
    /// `RBR_PATCH_ZIP`, `RBR_NGP_7Z`) run through it: open, running, done, refused, cancelled.
    #[test]
    fn screenshots_of_the_setup_panel() {
        let env = |key: &str| std::env::var_os(key).map(|v| v.to_string_lossy().into_owned());
        let (Some(shots), Some(disc), Some(patch), Some(ngp)) =
            (env("RBR_SCREENSHOTS"), env("RBR_DISC"), env("RBR_PATCH_ZIP"), env("RBR_NGP_7Z"))
        else {
            eprintln!("skipping: RBR_SCREENSHOTS, RBR_DISC, RBR_PATCH_ZIP or RBR_NGP_7Z not set");
            return;
        };
        let dir = crate::test_dir("disc-shots");
        fs::create_dir_all(&dir).unwrap();
        let mut folder = String::new();
        let mut setup = DiscSetup::default();
        // The first start's page (`window::App::choose_folder`).
        let page = |folder: &mut String, setup: &mut DiscSetup| {
            let mut set = None;
            let rgba = crate::capture::draw(|ctx| {
                ctx.root_ui(|root| {
                    egui::CentralPanel::default().show(root, |ui| {
                        ui.heading("Welcome to ReRBR");
                        ui.add_space(8.0);
                        ui.label("ReRBR plays Richard Burns Rally with NGP's physics: choose your game folder.");
                        ui.add_space(8.0);
                        if crate::settings_page::folder_row(ui, folder, setup) {
                            set = Some(folder.clone());
                        }
                    })
                });
            })
            .unwrap();
            (rgba, set)
        };
        let shot = |name: &str, rgba: &[u8]| crate::capture::write_png(&Path::new(&shots).join(name), rgba).unwrap();
        shot("setup-closed.png", &page(&mut folder, &mut setup).0);
        setup.open = true;
        (setup.disc, setup.patch, setup.ngp) = (disc, patch, ngp);
        shot("setup-open.png", &page(&mut folder, &mut setup).0);
        setup.out = dir.join("Richard Burns Rally").display().to_string();
        let inputs = |s: &DiscSetup| Inputs {
            disc: s.disc.clone().into(),
            patch: s.patch.clone().into(),
            ngp: Some(s.ngp.clone().into()),
            out: s.out.clone().into(),
        };
        setup.job = Some(setup_job(inputs(&setup)));
        let mut running = None;
        let done = loop {
            let (rgba, set) = page(&mut folder, &mut setup);
            let half = setup.job.as_ref().is_some_and(|j| {
                let p = j.progress.lock().unwrap();
                p.total > 0 && p.done > p.total / 3
            });
            if half && running.is_none() {
                running = Some(rgba.clone());
            }
            if let Some(set) = set {
                break (rgba, set);
            }
        };
        shot("setup-running.png", &running.expect("no frame while it ran"));
        shot("setup-done.png", &done.0);
        assert_eq!(done.1, setup.out);
        assert_eq!(folder, setup.out);
        assert!(Path::new(&folder).join("RichardBurnsRally_SSE.exe").is_file());
        // The same folder again: refused, as it is not empty.
        setup.job = Some(setup_job(inputs(&setup)));
        let refused = loop {
            let (rgba, _) = page(&mut folder, &mut setup);
            if setup.job.is_none() {
                break rgba;
            }
        };
        shot("setup-refused.png", &refused);
        assert!(matches!(&setup.status, Some(Err(e)) if e.contains("is not empty")));
        // Cancelled while it writes: no folder and no temp folder left.
        setup.out = dir.join("cancelled").display().to_string();
        setup.job = Some(setup_job(inputs(&setup)));
        let cancelled = loop {
            if let Some(job) = &setup.job {
                if job.progress.lock().unwrap().done > 0 {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            }
            let (rgba, _) = page(&mut folder, &mut setup);
            if setup.job.is_none() {
                break rgba;
            }
        };
        shot("setup-cancelled.png", &cancelled);
        assert_eq!(setup.status, Some(Err("Cancelled; nothing was written.".into())));
        let left: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["Richard Burns Rally"]);
        fs::remove_dir_all(&dir).unwrap();
    }
}

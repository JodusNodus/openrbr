//! Installing NGP 7.4.778's package into a game folder, as NGP's own install does: `Plugins/` and `AudioFMOD/`,
//! and `physics.rbz` in place of the retail one (the retail archive kept once as `physics.rbz.bak`). The disc setup
//! (`disc_install`) and the gvrc.de download (`gvrc`) install through [`add_ngp`]; a game folder that keeps its own
//! NGP takes only the DLL and the archive through [`add_ngp_to`].

use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use rbr_formats::rbz;

use crate::archive::{open_seven, safe_path};
use crate::disc_install::{require, with_folders, write_to, Reporter};
use crate::job::Report;

/// A failed or cancelled install's message.
type Fail = String;

/// A few files that show the package is what we expect (ignoring case).
pub(crate) const NGP: [&str; 3] = ["physics.rbz", "Plugins/NGP.dll", "AudioFMOD"];

/// Whether a `Physics/physics.lsp` sets NGP's revision (`install::is_ngp_archive` reads one from a path).
pub(crate) fn has_ngp_revision(lsp: &[u8]) -> bool {
    lsp.windows(b"PhysicsRevision_NGP".len()).any(|w| w == b"PhysicsRevision_NGP")
}

/// NGP's files (`/`-separated paths and their bytes) and folders.
type NgpFiles = (Vec<(String, Vec<u8>)>, Vec<String>);

/// NGP's files and folders: from the `.7z`, or the folder.
fn ngp_files(path: &Path) -> Result<NgpFiles, Fail> {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let error = |e: String| format!("{}: {e}", path.display());
    if path.is_dir() {
        let mut stack = vec![(path.to_owned(), String::new())];
        while let Some((dir, rel)) = stack.pop() {
            for entry in fs::read_dir(&dir).map_err(|e| error(e.to_string()))? {
                let entry = entry.map_err(|e| error(e.to_string()))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let rel = if rel.is_empty() { name } else { format!("{rel}/{name}") };
                if entry.path().is_dir() {
                    dirs.push(rel.clone());
                    stack.push((entry.path(), rel));
                } else {
                    files.push((rel, fs::read(entry.path()).map_err(|e| error(e.to_string()))?));
                }
            }
        }
    } else {
        let mut archive = open_seven(path)
            .map_err(|e| format!("{} is not the NGP package (its .7z or its folder): {e}", path.display()))?;
        archive
            .for_each_entries(|entry, reader| {
                let rel = entry.name().replace('\\', "/");
                if entry.is_directory() {
                    dirs.push(rel);
                } else {
                    let mut bytes = Vec::with_capacity(entry.size() as usize);
                    reader.read_to_end(&mut bytes)?;
                    files.push((rel, bytes));
                }
                Ok(true)
            })
            .map_err(|e| error(e.to_string()))?;
    }
    // As a sorted walk of the extracted folder would merge them.
    files.sort_by(|a, b| a.0.cmp(&b.0));
    dirs.sort();
    Ok((files, dirs))
}

/// The NGP package's files below the game folder, its folders, and its `physics.rbz` (checked to be NGP's).
pub(crate) struct Package {
    pub(crate) files: Vec<(String, Vec<u8>)>,
    pub(crate) dirs: Vec<String>,
    pub(crate) physics: Vec<u8>,
}

/// The NGP package at `path` (its `.7z` or its folder), checked to hold what NGP's install does.
pub(crate) fn read_ngp(path: &Path) -> Result<Package, Fail> {
    let (files, dirs) = ngp_files(path)?;
    let names = with_folders(files.iter().map(|(p, _)| p.as_str()).chain(dirs.iter().map(String::as_str)));
    require(&names, &NGP, format!("the NGP package {}", path.display()))?;
    let (physics, files): (Vec<_>, Vec<_>) =
        files.into_iter().partition(|(p, _)| p.eq_ignore_ascii_case("physics.rbz"));
    let physics = physics.into_iter().next().map(|(_, bytes)| bytes).unwrap_or_default();
    if !is_ngp_archive(&physics) {
        let path = path.display();
        return Err(format!("{path}: its physics.rbz is not NGP's (no PhysicsRevision_NGP in Physics/physics.lsp)"));
    }
    Ok(Package { files, dirs, physics })
}

/// Whether `game` is a Richard Burns Rally folder (it has `Cars/` and `Maps/`).
pub fn check_game_folder(game: &Path) -> Result<(), Fail> {
    match game.join("Cars").is_dir() && game.join("Maps").is_dir() {
        true => Ok(()),
        false => Err(format!("{} is not a Richard Burns Rally folder: choose it first (Game folder)", game.display())),
    }
}

/// Installs the NGP package at `package` (a `.7z`) into the existing game folder `game`, as NGP's own install does:
/// `Plugins/` and `AudioFMOD/`, and `physics.rbz` in place of the retail one, which is kept as `physics.rbz.bak`.
/// Nothing is written unless the package is NGP 7.4.778's (its `NGP.dll` is checked), and `physics.rbz` goes last, so
/// that an interrupted install does not look like NGP. The number of files written.
pub fn add_ngp(package: &Path, game: &Path, report: Report<'_>, cancel: &AtomicBool) -> Result<usize, Fail> {
    check_game_folder(game)?;
    let mut reporter = Reporter::new(report, cancel);
    reporter.step("Reading NGP")?;
    let Package { files, dirs, physics } = read_ngp(package)?;
    supported_dll(&files)?;
    reporter.total = files.iter().map(|(_, b)| b.len() as u64).sum::<u64>() + physics.len() as u64;
    reporter.step("Writing NGP")?;
    let below = |rel: &str| {
        let parts = safe_path(rel).ok_or_else(|| format!("a bad path in the package: {rel}"))?;
        Ok::<_, Fail>(parts.iter().fold(game.to_owned(), |p, part| p.join(part)))
    };
    for dir in &dirs {
        let path = below(dir)?;
        fs::create_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    for (rel, bytes) in &files {
        write_to(&below(rel)?, bytes, &reporter)?;
    }
    let archive = game.join("physics.rbz");
    let backup = game.join("physics.rbz.bak");
    let retail = archive.is_file() && !backup.exists() && fs::read(&archive).is_ok_and(|b| !is_ngp_archive(&b));
    if retail {
        fs::rename(&archive, &backup).map_err(|e| format!("{}: {e}", archive.display()))?;
    }
    write_to(&archive, &physics, &reporter)?;
    Ok(files.len() + 1)
}

/// The package's `Plugins/NGP.dll`, when it is NGP 7.4.778's.
pub(crate) fn supported_dll(files: &[(String, Vec<u8>)]) -> Result<&[u8], Fail> {
    let dll = files.iter().find(|(p, _)| p.eq_ignore_ascii_case("Plugins/NGP.dll")).map_or(&[][..], |(_, b)| b);
    rbr_formats::ngp::validate(dll).map_err(|e| format!("the downloaded NGP: {e}"))?;
    Ok(dll)
}

/// Writes only the `NGP.dll` and `physics.rbz` of the NGP package at `package` into the folder `to`, when it is NGP
/// 7.4.778's: for a game folder that keeps its own NGP (RallySimFans', or another version), which is left as it is.
pub fn add_ngp_to(package: &Path, to: &Path, report: Report<'_>, cancel: &AtomicBool) -> Result<(), Fail> {
    let mut reporter = Reporter::new(report, cancel);
    reporter.step("Reading NGP")?;
    let Package { files, physics, .. } = read_ngp(package)?;
    let dll = supported_dll(&files)?;
    reporter.total = (dll.len() + physics.len()) as u64;
    reporter.step("Writing NGP")?;
    write_to(&to.join("NGP.dll"), dll, &reporter)?;
    write_to(&to.join("physics.rbz"), &physics, &reporter)
}

/// Whether the `physics.rbz` bytes are NGP's (`install::is_ngp_archive` reads one from a path).
pub(crate) fn is_ngp_archive(bytes: &[u8]) -> bool {
    let Ok(mut zip) = rbz::ZipArchive::new(Cursor::new(bytes)) else {
        return false;
    };
    let mut text = Vec::new();
    rbz::by_name(&mut zip, "Physics/physics.lsp").is_ok_and(|mut f| f.read_to_end(&mut text).is_ok())
        && has_ngp_revision(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use crate::job::Progress;

    /// A `physics.rbz` (NGP's when `ngp`) holding its `Physics/physics.lsp`.
    fn physics_rbz(ngp: bool) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("Physics/physics.lsp", options).unwrap();
        zip.write_all(if ngp { b"PhysicsRevision_NGP 778" } else { b"retail" }).unwrap();
        zip.finish().unwrap().into_inner()
    }

    /// An NGP package `.7z` at `path`: `physics.rbz`, `Plugins/NGP.dll` and `AudioFMOD/` with a bank.
    fn ngp_package(path: &Path, dll: &[u8]) {
        let mut out = sevenz_rust2::ArchiveWriter::create(path).unwrap();
        let physics = physics_rbz(true);
        for (name, bytes) in
            [("physics.rbz", &physics[..]), ("Plugins/NGP.dll", dll), ("AudioFMOD/a.bank", b"bank".as_slice())]
        {
            out.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(bytes)).unwrap();
        }
        out.finish().unwrap();
    }

    /// A retail game folder with the retail `physics.rbz`.
    fn retail_game(dir: &Path) -> PathBuf {
        let game = dir.join("game");
        fs::create_dir_all(game.join("Cars")).unwrap();
        fs::create_dir_all(game.join("Maps")).unwrap();
        fs::write(game.join("physics.rbz"), physics_rbz(false)).unwrap();
        game
    }

    #[test]
    fn a_package_that_is_not_ngp_778_changes_nothing() {
        let dir = crate::test_dir("ngp-bad");
        fs::create_dir_all(&dir).unwrap();
        let game = retail_game(&dir);
        let package = dir.join("ngp.7z");
        ngp_package(&package, b"another NGP.dll");
        let add = |game: &Path, package: &Path| add_ngp(package, game, &|_| {}, &AtomicBool::new(false));
        let err = add(&game, &package).unwrap_err();
        assert!(err.contains("the downloaded NGP: unsupported NGP.dll"), "{err}");
        assert_eq!(fs::read(game.join("physics.rbz")).unwrap(), physics_rbz(false));
        assert!(
            !game.join("Plugins").exists()
                && !game.join("AudioFMOD").exists()
                && !game.join("physics.rbz.bak").exists()
        );
        assert!(add(&dir, &package).unwrap_err().contains("is not a Richard Burns Rally folder"));
        assert!(add(&game, &dir.join("none.7z")).is_err());
        let to = dir.join("player/NGP 7.4.778");
        let err = add_ngp_to(&package, &to, &|_| {}, &AtomicBool::new(false)).unwrap_err();
        assert!(err.contains("unsupported NGP.dll") && !to.exists(), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// With the real NGP 7.4.778 DLL (`RBR_NGP_DLL`, or the data's): installed over a retail folder, the retail archive
    /// kept once, and installed again over NGP's own.
    #[test]
    fn ngp_is_installed_over_a_retail_folder() {
        let data = rbr_formats::data::Data::from_env().unwrap_or_else(|| rbr_formats::data::Data::new("", None));
        let Some(dll) = rbr_formats::ngp::path(&data).and_then(|p| fs::read(p).ok()) else {
            return;
        };
        let dir = crate::test_dir("ngp-ok");
        fs::create_dir_all(&dir).unwrap();
        let game = retail_game(&dir);
        let package = dir.join("ngp.7z");
        ngp_package(&package, &dll);
        let progress = Mutex::new(Vec::new());
        let report = |p: &Progress| progress.lock().unwrap().push(p.done);
        assert_eq!(add_ngp(&package, &game, &report, &AtomicBool::new(false)), Ok(3));
        assert_eq!(fs::read(game.join("Plugins/NGP.dll")).unwrap(), dll);
        assert_eq!(fs::read(game.join("AudioFMOD/a.bank")).unwrap(), b"bank");
        assert_eq!(fs::read(game.join("physics.rbz")).unwrap(), physics_rbz(true));
        assert_eq!(fs::read(game.join("physics.rbz.bak")).unwrap(), physics_rbz(false));
        assert!(progress.lock().unwrap().windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(add_ngp(&package, &game, &|_| {}, &AtomicBool::new(false)), Ok(3));
        assert_eq!(fs::read(game.join("physics.rbz.bak")).unwrap(), physics_rbz(false));
        assert!(add_ngp(&package, &game, &|_| {}, &AtomicBool::new(true)).unwrap_err().ends_with("cancelled"));
        let to = dir.join("player/NGP 7.4.778");
        assert_eq!(add_ngp_to(&package, &to, &|_| {}, &AtomicBool::new(false)), Ok(()));
        assert_eq!(fs::read(to.join("NGP.dll")).unwrap(), dll);
        assert_eq!(fs::read(to.join("physics.rbz")).unwrap(), physics_rbz(true));
        fs::remove_dir_all(&dir).unwrap();
    }
}

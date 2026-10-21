//! Locally owned co-driver voice packs and stage-note packs, so a complete setup works without RallySimFans:
//! "Import co-driver…" (the Graphics & sound page) and `launcher --import-codriver SRC` copy downloads into ReRBR's
//! own user content directories, where the launcher reads them in place of the RSF install's.
//!
//! The general launcher policy (user decision): ReRBR supports both (1) easy reuse of an existing RBR/RSF
//! installation read-only and (2) a complete setup without RSF, using retail+patch+NGP plus locally installed
//! content. Downloaded packs are COPIED into ReRBR-owned user content directories (`cars/` for car packs, here
//! `codrivers/` and `notes/`). Existing RSF content may be read in place. Player settings, saves and personal edits
//! always belong to ReRBR. This module applies that policy to co-driver voice/style/range and stage-note packs.
//!
//! Layout, each pack isolated so different `Rbr.ini` definitions never collide:
//! - `codrivers/<pack>/`: a Pacenote plugin's folder as `rbr_formats::rsf::codriver` reads it (`PaceNote.ini`,
//!   `sounds/...`, `config/pacenotes/*.ini`, `config/ranges/*.ini` with their package/category trees, relative paths
//!   preserved);
//! - `notes/<pack>/`: stage-note INI files (the plugin's BTB/MyPacenotes format, `rbr_formats::rsf::pacenotes`),
//!   relative paths preserved; looked up recursively, so stage folders may nest.
//!
//! Sources: an extracted folder (a Pacenote plugin folder, a MyPacenotes folder, a folder with both, or a whole
//! existing install to COPY from — never modified), or a downloaded ZIP/7z archive of the same (a common single
//! wrapper directory is stripped, as is a direct `Pacenote` folder). Only relevant files are copied: `PaceNote.ini`,
//! `config/**/*.ini`, voice audio (`sounds/`, `.ogg`/`.wav`/`.mp3`/`.flac`/`.oga`) and parseable note INIs. No
//! DLLs, executables or anything else. A failed import leaves no active partial pack: everything stages in a temp
//! folder beside the destination and is renamed only when validated. Existing packs are never overwritten: a taken
//! name gets a `-2`, `-3` suffix. An audio-only download without the style/range definitions it needs fails with an
//! actionable error (or a pack listed with its missing dependency), and never appears ready.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use rbr_formats::rsf::{codriver, pacenotes};

use crate::disc_install::{Job, Temp};
use crate::pages::{GOOD, WARN};
use crate::{install, Form};

/// The folders in the player's folder with the owned packs.
pub const VOICES_DIR: &str = "codrivers";
pub const NOTES_DIR: &str = "notes";
/// The archive suffixes taken as downloads.
const ZIP: &str = ".zip";
const SEVEN_Z: &str = ".7z";

type Fail = String;

/// A locally owned voice pack: its folder (as the plugin's folder) and what it offers, or why it is not ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoicePack {
    pub name: String,
    pub dir: PathBuf,
    pub voices: Vec<String>,
    pub styles: Vec<String>,
    pub ranges: Vec<String>,
    /// Why the pack is not ready (its missing dependency), if it is not.
    pub problem: Option<String>,
}

/// A locally owned stage-note pack: its folder and the stage folders found in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotePack {
    pub name: String,
    pub dir: PathBuf,
    /// The note files, and the stage-folder names naming them.
    pub files: usize,
    pub stages: Vec<String>,
}

/// An archive entry path (`\` or `/` separated) as safe components below the folder it is written to, or `None`.
fn safe_path(name: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = name.split(['/', '\\']).filter(|p| !p.is_empty() && *p != ".").collect();
    let bad = |p: &&str| *p == ".." || p.contains(':');
    (!parts.is_empty() && !parts.iter().any(bad)).then_some(parts)
}

/// One folder name of ours, from untrusted text: no separators, not hidden.
fn safe_folder(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && !name.starts_with('.') && safe_path(name)?.len() == 1).then(|| name.to_owned())
}

/// Whether the file name has an audio suffix the voices play.
fn is_audio(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".ogg", ".wav", ".mp3", ".flac", ".oga"].iter().any(|e| lower.ends_with(e))
}

/// Whether the relative path (from a plugin's folder) is a co-driver file worth owning: the settings, a config
/// definition, or voice audio. Everything else (DLLs, executables, editors, pictures) stays out.
fn keep_voice(rel: &[&str]) -> bool {
    if rel.is_empty() {
        return false;
    }
    let lower: Vec<String> = rel.iter().map(|p| p.to_ascii_lowercase()).collect();
    if lower.len() == 1 && lower[0] == "pacenote.ini" {
        return true;
    }
    if lower[0] == "config" && lower.last().is_some_and(|n| n.ends_with(".ini")) {
        return true;
    }
    if lower[0] == "sounds" && rel.len() > 2 && is_audio(rel[rel.len() - 1]) {
        return true;
    }
    false
}

/// A pack name from the source's file or folder name: the archive's stem, else the folder's name, spaces kept.
fn pack_name(src: &Path) -> String {
    let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    let stem = [ZIP, SEVEN_Z].iter().find_map(|e| lower.ends_with(e).then(|| name[..name.len() - e.len()].to_owned()));
    safe_folder(stem.as_deref().unwrap_or(&name)).unwrap_or_else(|| "pack".to_owned())
}

/// A free pack folder below `dir` for `name`: `name`, else `name-2`, `name-3`, ... (existing packs are never
/// overwritten).
fn free_dir(dir: &Path, name: &str) -> PathBuf {
    if !dir.join(name).exists() {
        return dir.join(name);
    }
    for n in 2.. {
        let next = dir.join(format!("{name}-{n}"));
        if !next.exists() {
            return next;
        }
    }
    unreachable!()
}

/// The files below `dir` (relative `/`-separated paths and their absolute paths), sorted.
fn walk(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(top) = stack.pop() {
        let Ok(entries) = fs::read_dir(&top) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(dir) {
                out.push((rel.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
    out.sort();
    out
}

/// Whether the folder looks like a Pacenote plugin's folder: it has the settings, the voices, or the definitions.
fn is_plugin_dir(dir: &Path) -> bool {
    dir.join("PaceNote.ini").is_file()
        || dir.join("sounds").is_dir()
        || dir.join("config").join("pacenotes").is_dir()
        || dir.join("config").join("ranges").is_dir()
}

/// The plugin's folder below `dir`: itself, a direct `Pacenote` folder beside note folders, or up to two single
/// wrapper directories down. `None` when there is none (notes-only sources reach here too: their stage folders
/// stay where they are, since the note lookup is recursive).
fn find_voice_root(dir: &Path) -> Option<PathBuf> {
    if is_plugin_dir(dir) {
        return Some(dir.to_owned());
    }
    let children: Vec<PathBuf> = fs::read_dir(dir)
        .map(|e| e.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    // A download with the plugin beside the notes (`Pacenote/`, `MyPacenotes/`).
    if let Some(child) = children.iter().find(|c| is_plugin_dir(c)) {
        return Some(child.clone());
    }
    // Up to two single wrapper directories down.
    let [child] = &children[..] else { return None };
    if is_plugin_dir(child) {
        return Some(child.clone());
    }
    let grandchildren: Vec<PathBuf> = fs::read_dir(child)
        .map(|e| e.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    let [grand] = &grandchildren[..] else { return None };
    is_plugin_dir(grand).then(|| grand.clone())
}

/// Whether the text is a note file (the `[PACENOTES]` section with a count).
fn is_note_text(text: &str) -> bool {
    pacenotes::parse(text).is_ok()
}

/// The note INI files below `dir` (relative paths), validated by parsing.
fn note_files(dir: &Path) -> Vec<String> {
    walk(dir)
        .into_iter()
        .filter(|(rel, _)| rel.to_ascii_lowercase().ends_with(".ini"))
        .filter(|(_, path)| fs::read(path).ok().is_some_and(|b| is_note_text(&String::from_utf8_lossy(&b))))
        .map(|(rel, _)| rel)
        .collect()
}

/// The voice packs owned (each folder of `home/codrivers/`), with their problem when not ready.
pub fn voice_packs(home: &Path) -> Vec<VoicePack> {
    let dir = home.join(VOICES_DIR);
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|e| e.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let dir = home.join(VOICES_DIR).join(&name);
            let (voices, styles, ranges) = (codriver::voices(&dir), codriver::styles(&dir), codriver::range_sets(&dir));
            let problem = voice_problem(&voices, &styles, &ranges);
            VoicePack { name, dir, voices, styles, ranges, problem }
        })
        .collect()
}

/// Why the pack's voices, styles and range sets are not a ready co-driver, if they are not.
fn voice_problem(voices: &[String], styles: &[String], ranges: &[String]) -> Option<String> {
    match (voices.is_empty(), styles.is_empty(), ranges.is_empty()) {
        (false, true, _) => Some(
            "no call styles (config/pacenotes/*.ini): import the style pack this voice needs, or pick another pack"
                .into(),
        ),
        (_, _, true) if !voices.is_empty() || !styles.is_empty() => Some(
            "no distance calls (config/ranges/*.ini): import the range set this voice needs, or pick another pack"
                .into(),
        ),
        (true, true, true) => Some("no voices, call styles or distance calls: is it a co-driver pack?".into()),
        _ => None,
    }
}

/// The stage-note packs owned (each folder of `home/notes/` with at least one note file).
pub fn note_packs(home: &Path) -> Vec<NotePack> {
    let dir = home.join(NOTES_DIR);
    let mut names: Vec<String> = fs::read_dir(&dir)
        .map(|e| e.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let dir = home.join(NOTES_DIR).join(&name);
            let files = note_files(&dir);
            let mut stages: Vec<String> = files
                .iter()
                .filter_map(|rel| Path::new(rel).parent()?.file_name()?.to_str().map(str::to_owned))
                .collect();
            stages.sort();
            stages.dedup();
            NotePack { name, dir, files: files.len(), stages }
        })
        .filter(|p| p.files > 0)
        .collect()
}

/// The note file the pack plays on the stage RSF calls by one of `names`: every note INI below `dir`, matched by
/// its parent folder's name (folded as `pacenotes::my_pacenotes` matches stage folders), the file modified last.
/// Ours on a tie: a file other than NGPCarMenu's `_default` export, then the name.
pub fn find_note(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    let wanted: Vec<String> = names.iter().map(|n| fold(n)).filter(|n| !n.is_empty()).collect();
    if wanted.is_empty() {
        return None;
    }
    walk(dir)
        .into_iter()
        .filter(|(rel, _)| rel.to_ascii_lowercase().ends_with(".ini"))
        .filter(|(rel, _)| {
            Path::new(rel)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .is_some_and(|n| wanted.contains(&fold(n)))
        })
        .filter_map(|(_, path)| {
            if !is_note_file(&path) {
                return None;
            }
            let modified = path.metadata().and_then(|m| m.modified()).ok()?;
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some((modified, !name.to_ascii_lowercase().ends_with("_default.ini"), name, path))
        })
        .max_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then_with(|| b.2.cmp(&a.2)))
        .map(|(.., path)| path)
}

/// Whether the file parses as a note file.
fn is_note_file(path: &Path) -> bool {
    fs::read(path).ok().is_some_and(|b| is_note_text(&String::from_utf8_lossy(&b)))
}

/// A name for matching: lowercase ASCII letters and digits, accented Latin letters as their base letter (as
/// `pacenotes` matches stage folders).
fn fold(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
            'ç' | 'ć' | 'č' => 'c',
            'ď' | 'đ' => 'd',
            'è'..='ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
            'ì'..='ï' | 'ī' | 'į' => 'i',
            'ł' => 'l',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò'..='ö' | 'ø' | 'ō' | 'ő' => 'o',
            'ř' => 'r',
            'ś' | 'š' | 'ß' => 's',
            'ť' => 't',
            'ù'..='ü' | 'ū' | 'ů' | 'ű' | 'ų' => 'u',
            'ý' | 'ÿ' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            c if c.is_ascii_alphanumeric() => c,
            _ => continue,
        };
        out.push(base);
    }
    out
}

/// Copies the voice files (see [`keep_voice`]) from the plugin's folder `src` into `dest`, preserving relative
/// paths; the files copied.
fn copy_voice(src: &Path, dest: &Path) -> Result<usize, Fail> {
    let mut n = 0;
    for (rel, path) in walk(src) {
        let parts = safe_path(&rel).ok_or_else(|| format!("{}: a bad path {rel}", src.display()))?;
        if !keep_voice(&parts) {
            continue;
        }
        let to = parts.iter().fold(dest.to_owned(), |p, part| p.join(part));
        fs::create_dir_all(to.parent().unwrap_or(dest)).map_err(|e| format!("{}: {e}", to.display()))?;
        fs::copy(&path, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Copies the note files below `src` into `dest`, preserving relative paths; the files copied. Only files that
/// parse as note files are kept, so install leftovers never ride along.
fn copy_notes(src: &Path, dest: &Path) -> Result<usize, Fail> {
    let mut n = 0;
    for (rel, path) in walk(src) {
        if !rel.to_ascii_lowercase().ends_with(".ini") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !is_note_text(&String::from_utf8_lossy(&bytes)) {
            continue;
        }
        let parts = safe_path(&rel).ok_or_else(|| format!("{}: a bad path {rel}", src.display()))?;
        let to = parts.iter().fold(dest.to_owned(), |p, part| p.join(part));
        fs::create_dir_all(to.parent().unwrap_or(dest)).map_err(|e| format!("{}: {e}", to.display()))?;
        fs::write(&to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
        n += 1;
    }
    Ok(n)
}

/// Validates a staged voice pack: its styles and range sets read, and its first style with its first range set
/// loads (catching dangling package references). Audio-only downloads fail here with what to add.
fn check_voice(dir: &Path) -> Result<(Vec<String>, Vec<String>), Fail> {
    let (voices, styles, ranges) = (codriver::voices(dir), codriver::styles(dir), codriver::range_sets(dir));
    if let Some(problem) = voice_problem(&voices, &styles, &ranges) {
        return Err(format!("{}: {problem}", dir.display()));
    }
    codriver::load(dir, &styles[0], &ranges[0]).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok((styles, ranges))
}

/// The MyPacenotes folder of an install's `Plugins/NGPCarMenu.ini` (`MyPacenotesPath`, relative to the install or
/// absolute), else NGPCarMenu's default; `None` when disabled or missing.
fn install_notes_dir(root: &Path) -> Option<PathBuf> {
    pacenotes::my_pacenotes_dir(root)
}

/// Imports the folder `src` (a plugin folder, a note folder, a folder with both, or a whole install to copy from)
/// into `home`: the lines saying what was added.
fn import_folder(home: &Path, src: &Path) -> Result<Vec<String>, Fail> {
    if !src.is_dir() {
        return Err(format!("{} is not a folder", src.display()));
    }
    let name = pack_name(src);
    // A whole existing install: its plugin and its MyPacenotes folder are the sources, copied, never modified.
    if src.join("Plugins").is_dir() {
        let mut lines = Vec::new();
        let plugin = src.join("Plugins").join("Pacenote");
        if is_plugin_dir(&plugin) {
            lines.extend(import_voice_from(home, &name, &plugin, |d| copy_voice(&plugin, d))?);
        }
        if let Some(notes) = install_notes_dir(src) {
            lines.extend(import_notes_from(home, &name, &notes, |d| copy_notes(&notes, d))?);
        }
        return match lines.is_empty() {
            true => Err(format!(
                "{}: no Pacenote plugin (Plugins/Pacenote) and no stage notes ({}): is it an RBR install?",
                src.display(),
                pacenotes::my_pacenotes_dir(src)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| { "Plugins/NGPCarMenu/MyPacenotes".into() })
            )),
            false => Ok(lines),
        };
    }
    let voice = find_voice_root(src);
    // The notes are searched recursively, so wrappers stay on: the stage folders keep their names.
    let notes = note_files(src).len();
    if let Some(root) = voice {
        if notes == 0 {
            return import_voice_from(home, &name, &root, |d| copy_voice(&root, d));
        }
        // Both: the voice goes to `codrivers/`, the notes to `notes/`, under one name each.
        let mut lines = import_voice_from(home, &name, &root, |d| copy_voice(&root, d))?;
        lines.extend(import_notes_from(home, &name, src, |d| copy_notes(src, d))?);
        return Ok(lines);
    }
    if notes > 0 {
        return import_notes_from(home, &name, src, |d| copy_notes(src, d));
    }
    Err(format!(
        "{}: no co-driver voices, call styles or stage notes in it: choose the plugin's folder \
         (PaceNote.ini, sounds/, config/), a MyPacenotes folder, or a whole install to copy from",
        src.display()
    ))
}

/// Stages a voice pack named `name` with `copy` (into a temp folder, validated, then renamed): the lines saying
/// what was added. A failure leaves no pack behind.
fn import_voice_from(
    home: &Path,
    name: &str,
    _src: &Path,
    copy: impl FnOnce(&Path) -> Result<usize, Fail>,
) -> Result<Vec<String>, Fail> {
    let dir = home.join(VOICES_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = safe_folder(name).unwrap_or_else(|| "pack".to_owned());
    let dest = free_dir(&dir, &name);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp = Temp(dir.join(format!(".import-{}-{stamp}", std::process::id())));
    fs::create_dir(&temp.0).map_err(|e| format!("{}: {e}", temp.0.display()))?;
    let staged = temp.0.join("pack");
    fs::create_dir(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
    copy(&staged)?;
    let (styles, ranges) = check_voice(&staged)?;
    fs::rename(&staged, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    drop(temp);
    let pack = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(vec![format!("Co-driver {pack}: {} call styles, {} range sets.", styles.len(), ranges.len())])
}

/// Stages a note pack named `name` with `copy`: the lines saying what was added.
fn import_notes_from(
    home: &Path,
    name: &str,
    _src: &Path,
    copy: impl FnOnce(&Path) -> Result<usize, Fail>,
) -> Result<Vec<String>, Fail> {
    let dir = home.join(NOTES_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = safe_folder(name).unwrap_or_else(|| "pack".to_owned());
    let dest = free_dir(&dir, &name);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp = Temp(dir.join(format!(".import-{}-{stamp}", std::process::id())));
    fs::create_dir(&temp.0).map_err(|e| format!("{}: {e}", temp.0.display()))?;
    let staged = temp.0.join("pack");
    fs::create_dir(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
    let n = copy(&staged)?;
    if note_files(&staged).is_empty() {
        return Err(format!(
            "{}: no stage notes (*.ini with [PACENOTES]) in it: choose a MyPacenotes folder or a note pack",
            _src.display()
        ));
    }
    fs::rename(&staged, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    drop(temp);
    let pack = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(vec![format!("Stage notes {pack}: {n} files.")])
}

/// The entries of the ZIP archive `src`: (path parts below the common wrapper, is a directory).
fn zip_entries(src: &Path) -> Result<Vec<(Vec<String>, bool)>, Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let zip = rbr_formats::rbz::open(src).map_err(|e| error(e.to_string()))?;
    let mut entries = Vec::new();
    for k in 0..zip.len() {
        let name = zip.name_for_index(k).and_then(Result::ok).ok_or_else(|| error("a bad entry".into()))?;
        let dir = name.ends_with(['/', '\\']);
        let Some(parts) = safe_path(&name) else { continue };
        if !dir {
            entries.push((parts.iter().map(|p| p.to_string()).collect(), false));
        }
    }
    Ok(strip_wrapper(entries))
}

/// The entries of the 7z archive `src`.
fn seven_entries(src: &Path) -> Result<Vec<(Vec<String>, bool)>, Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let file = File::open(src).map_err(|e| error(e.to_string()))?;
    let archive = sevenz_rust2::ArchiveReader::new(BufReader::new(file), sevenz_rust2::Password::empty())
        .map_err(|e| error(format!("not a .7z: {e}")))?;
    let mut entries = Vec::new();
    for f in &archive.archive().files {
        let Some(parts) = safe_path(&f.name) else { continue };
        if !f.is_directory {
            entries.push((parts.iter().map(|p| p.to_string()).collect(), false));
        }
    }
    Ok(strip_wrapper(entries))
}

/// The common single top folder (a download's wrapper directory), when every path shares one.
fn wrapper_of(paths: &[Vec<String>]) -> Option<String> {
    let first = paths.first()?.first().cloned()?;
    (!first.is_empty() && paths.iter().all(|p| p.len() > 1 && p[0] == first)).then_some(first)
}

/// Strips the common single top folder (a download's wrapper directory, or a direct `Pacenote` folder's parent),
/// when every entry shares it.
fn strip_wrapper(mut entries: Vec<(Vec<String>, bool)>) -> Vec<(Vec<String>, bool)> {
    if let Some(wrapper) = wrapper_of(&entries.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>()) {
        for (p, _) in &mut entries {
            if p.first().is_some_and(|f| *f == wrapper) {
                p.remove(0);
            }
        }
    }
    entries
}

/// Whether the stripped archive entries hold a plugin at `root` (relative parts): its settings, voices or
/// definitions below it.
fn archive_plugin_at(entries: &[(Vec<String>, bool)], root: &[String]) -> bool {
    let below = entries.iter().filter_map(|(p, _)| p.strip_prefix(root)).map(|p| p.join("/").to_ascii_lowercase());
    let mut has = (false, false, false);
    for rel in below {
        has.0 |= rel == "pacenote.ini";
        has.1 |= rel.starts_with("sounds/");
        has.2 |= rel.starts_with("config/pacenotes/") || rel.starts_with("config/ranges/");
    }
    has.0 || has.1 || has.2
}

/// The plugin's root in the stripped entries: top level, a single folder holding one (a direct `Pacenote` folder),
/// or an install's `Plugins/Pacenote`.
fn archive_plugin_root(entries: &[(Vec<String>, bool)]) -> Option<Vec<String>> {
    if archive_plugin_at(entries, &[]) {
        return Some(Vec::new());
    }
    let mut roots: Vec<Vec<String>> = entries.iter().filter_map(|(p, _)| p.first().map(|f| vec![f.clone()])).collect();
    roots.sort();
    roots.dedup();
    if let Some(root) = roots.into_iter().find(|r| archive_plugin_at(entries, r)) {
        return Some(root);
    }
    let install = vec!["Plugins".to_owned(), "Pacenote".to_owned()];
    archive_plugin_at(entries, &install).then_some(install)
}

/// The note files among the stripped entries: the INIs below a `MyPacenotes` folder or an install's NGPCarMenu
/// notes, else every INI (validated by parsing after extraction).
fn archive_note_candidates(entries: &[(Vec<String>, bool)]) -> Vec<Vec<String>> {
    let inis: Vec<Vec<String>> = entries
        .iter()
        .filter(|(p, _)| p.last().is_some_and(|n| n.to_ascii_lowercase().ends_with(".ini")))
        .map(|(p, _)| p.clone())
        .collect();
    let under = |folder: &str| {
        inis.iter()
            .filter(|p| p.iter().any(|c| c.eq_ignore_ascii_case(folder)))
            .map(|p| {
                let at = p.iter().position(|c| c.eq_ignore_ascii_case(folder)).unwrap_or(0);
                p[at + 1..].to_vec()
            })
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
    };
    // An install copy's notes live under NGPCarMenu's folder; a notes pack's under MyPacenotes.
    for folder in ["MyPacenotes", "NGPCarMenu"] {
        let found = under(folder);
        if !found.is_empty() {
            return found;
        }
    }
    // A bare note file or two at the top is a stage folder itself.
    inis
}

/// Extracts the ZIP's `wanted` (stripped entry path to pack-relative path) into `dest`; unsafe paths are skipped.
fn extract_zip(src: &Path, dest: &Path, wanted: &[(Vec<String>, Vec<String>)]) -> Result<(), Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let mut zip = rbr_formats::rbz::open(src).map_err(|e| error(e.to_string()))?;
    // The files' stripped parts by entry index (directory entries excluded, as when picked).
    let mut indexed: Vec<(usize, Vec<String>)> = Vec::new();
    for k in 0..zip.len() {
        let name = zip.name_for_index(k).and_then(Result::ok).unwrap_or_default().into_owned();
        if name.ends_with(['/', '\\']) {
            continue;
        }
        let Some(parts) = safe_path(&name) else { continue };
        indexed.push((k, parts.iter().map(|p| p.to_string()).collect()));
    }
    // The same wrapper stripped as when the wanted paths were picked.
    let paths: Vec<Vec<String>> = indexed.iter().map(|(_, p)| p.clone()).collect();
    let wrapper = wrapper_of(&paths);
    for (from, to) in wanted {
        let found = indexed.iter().find(|(_, p)| {
            let mut p = p.clone();
            if wrapper.as_deref() == p.first().map(String::as_str) && p.len() > 1 {
                p.remove(0);
            }
            p == *from
        });
        let Some((k, _)) = found else { continue };
        let mut file = zip.by_index(*k).map_err(|e| error(e.to_string()))?;
        write_to(dest, to, &mut file)?;
    }
    Ok(())
}

/// Writes `reader` to `dest`/`parts`, making its folders.
fn write_to(dest: &Path, parts: &[String], reader: &mut dyn Read) -> Result<(), Fail> {
    let safe: Vec<&str> = parts.iter().map(String::as_str).collect();
    let path = safe.iter().fold(dest.to_owned(), |p, part| p.join(part));
    fs::create_dir_all(path.parent().unwrap_or(dest)).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::copy(reader, &mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

/// Extracts the 7z's `wanted` (stripped entry path to pack-relative path) into `dest`.
fn extract_seven(src: &Path, dest: &Path, wanted: &[(Vec<String>, Vec<String>)]) -> Result<(), Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let file = File::open(src).map_err(|e| error(e.to_string()))?;
    let mut archive = sevenz_rust2::ArchiveReader::new(BufReader::new(file), sevenz_rust2::Password::empty())
        .map_err(|e| error(e.to_string()))?;
    let all: Vec<Vec<String>> = archive
        .archive()
        .files
        .iter()
        .filter(|f| !f.is_directory)
        .filter_map(|f| safe_path(&f.name))
        .map(|p| p.iter().map(|s| s.to_string()).collect())
        .collect();
    let wrapper = wrapper_of(&all);
    let mut failed: Option<Fail> = None;
    archive
        .for_each_entries(|entry, reader| {
            if entry.is_directory {
                return Ok(true);
            }
            let Some(parts) = safe_path(&entry.name) else { return Ok(true) };
            let mut stripped: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
            if wrapper.as_deref() == stripped.first().map(String::as_str) && stripped.len() > 1 {
                stripped.remove(0);
            }
            if let Some((_, to)) = wanted.iter().find(|(from, _)| *from == stripped) {
                match write_to(dest, to, reader) {
                    Ok(()) => Ok(true),
                    Err(e) => {
                        failed = Some(e);
                        Ok(false)
                    }
                }
            } else {
                Ok(true)
            }
        })
        .map_err(|e| error(e.to_string()))?;
    failed.map_or(Ok(()), Err)
}

/// Imports the archive `src` (ZIP or 7z): the lines saying what was added.
fn import_archive(home: &Path, src: &Path) -> Result<Vec<String>, Fail> {
    let lower = src.to_string_lossy().to_ascii_lowercase();
    let entries = match lower.ends_with(ZIP) {
        true => zip_entries(src),
        false => seven_entries(src),
    }?;
    if entries.iter().all(|(p, _)| {
        let n = p.last().map(|n| n.to_ascii_lowercase()).unwrap_or_default();
        n.ends_with(".dll") || n.ends_with(".exe")
    }) && !entries.is_empty()
    {
        return Err(format!("{}: no co-driver files in it, only executables", src.display()));
    }
    let name = pack_name(src);
    let mut lines = Vec::new();
    if let Some(root) = archive_plugin_root(&entries) {
        let wanted: Vec<(Vec<String>, Vec<String>)> = entries
            .iter()
            .filter(|(p, _)| p.len() >= root.len() && p[..root.len()] == root)
            .map(|(p, _)| p[root.len()..].to_vec())
            .filter(|p| !p.is_empty() && safe_path(&p.join("/")).is_some_and(|s| keep_voice(&s)))
            .map(|p| {
                let mut from = root.clone();
                from.extend(p.clone());
                (from, p)
            })
            .collect();
        if !wanted.is_empty() {
            lines.extend(import_voice_from(home, &name, src, |staged| {
                extract(home, src, staged, &wanted)?;
                Ok(count_files(staged))
            })?);
        }
    }
    let notes = archive_note_candidates(&entries);
    // Drop the voice entries' inis from the note candidates when both came from one plugin folder: config inis are
    // not stage notes (they do not parse as notes, and are pruned below anyway).
    if !notes.is_empty() {
        let before = lines.len();
        let result = import_notes_from(home, &name, src, |staged| {
            // Extract every candidate INI below its notes root; unparseable ones are pruned after.
            let wanted: Vec<(Vec<String>, Vec<String>)> = entries
                .iter()
                .filter(|(p, _)| p.last().is_some_and(|n| n.to_ascii_lowercase().ends_with(".ini")))
                .filter_map(|(p, _)| {
                    // The pack-relative path: below the notes root when one was found, else the file name.
                    let rel = match notes.iter().find(|n| p.len() >= n.len() && p[p.len() - n.len()..] == n[..]) {
                        Some(n) => n.clone(),
                        None => return None,
                    };
                    Some((p.clone(), rel))
                })
                .collect();
            extract(home, src, staged, &wanted)?;
            prune_not_notes(staged);
            Ok(count_files(staged))
        });
        match result {
            Ok(mut added) => lines.append(&mut added),
            Err(e) if !lines.is_empty() => {
                // A voice imported but no notes in the archive: not a failure.
                let _ = e;
            }
            Err(e) => {
                if lines.is_empty() {
                    return Err(e);
                }
            }
        }
        let _ = before;
    }
    match lines.is_empty() {
        true => Err(format!(
            "{}: no co-driver voices, call styles or stage notes in it: is it a co-driver or note pack?",
            src.display()
        )),
        false => Ok(lines),
    }
}

/// Extracts `wanted` from the archive `src` (its suffix decides ZIP or 7z) into `dest`.
fn extract(_home: &Path, src: &Path, dest: &Path, wanted: &[(Vec<String>, Vec<String>)]) -> Result<(), Fail> {
    match src.to_string_lossy().to_ascii_lowercase().ends_with(ZIP) {
        true => extract_zip(src, dest, wanted),
        false => extract_seven(src, dest, wanted),
    }
}

/// Deletes the staged INIs that do not parse as note files.
fn prune_not_notes(dir: &Path) {
    for (_, path) in walk(dir) {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) && !is_note_file(&path) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// The files below `dir`.
fn count_files(dir: &Path) -> usize {
    walk(dir).len()
}

/// Imports `src` (a folder, a ZIP/7z archive, or a whole install to copy from) into `home`: the lines saying what
/// was added. The source is only read, never modified; existing packs are never overwritten.
pub fn import(home: &Path, src: &Path) -> Result<Vec<String>, Fail> {
    if !src.exists() {
        return Err(format!("{} does not exist", src.display()));
    }
    if src.is_dir() {
        return import_folder(home, src);
    }
    let lower = src.to_string_lossy().to_ascii_lowercase();
    match lower.ends_with(ZIP) || lower.ends_with(SEVEN_Z) {
        true => import_archive(home, src),
        false => Err(format!("{}: take a folder or a .zip/.7z download", src.display())),
    }
}

/// Removes the voice pack `name` from `home`.
pub fn remove_voice(home: &Path, name: &str) -> Result<(), Fail> {
    let folder = safe_folder(name).ok_or_else(|| format!("a bad pack name {name}"))?;
    let path = home.join(VOICES_DIR).join(folder);
    match path.is_dir() {
        true => fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display())),
        false => Ok(()),
    }
}

/// Removes the note pack `name` from `home`.
pub fn remove_notes(home: &Path, name: &str) -> Result<(), Fail> {
    let folder = safe_folder(name).ok_or_else(|| format!("a bad pack name {name}"))?;
    let path = home.join(NOTES_DIR).join(folder);
    match path.is_dir() {
        true => fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display())),
        false => Ok(()),
    }
}

/// The command line's `--import-codriver SRC`, into the player's folder.
pub fn cli(home: &Path, args: &[String]) -> Result<(), String> {
    let [src] = args else { return Err("usage: rerbr --import-codriver SRC".into()) };
    let lines = import(home, Path::new(src))?;
    lines.iter().for_each(|l| println!("{l}"));
    println!("{}", home.display());
    Ok(())
}

impl Form {
    /// Re-reads the owned packs and points the co-driver at the chosen ones: the local voice pack
    /// (`codriver_pack`), else the RSF install's plugin; a missing local pack keeps the last lists and says so.
    pub fn rescan_codrivers(&mut self) {
        let home = self.here();
        self.owned_voices = voice_packs(&home);
        self.owned_notes = note_packs(&home);
        self.codriver_mods = self.active_mods();
    }

    /// The active co-driver plugin: the chosen local pack's folder, else the RSF install's. `None` without either
    /// (the original 1.02 voice then); a chosen pack that is missing or not ready also gives `None`.
    pub fn active_mods(&self) -> Option<crate::command::CodriverMods> {
        if let Some(pack) = self.codriver_pack.as_deref() {
            let dir = self.here().join(VOICES_DIR).join(pack);
            if is_plugin_dir(&dir) {
                return crate::command::CodriverMods::from_dir(dir);
            }
            return None;
        }
        crate::command::CodriverMods::scan(&self.rsf())
    }

    /// The note file the selected notes play on the stage with catalogue `names`: the chosen local pack's
    /// (searched by the launcher's own stage names), else the RSF install's MyPacenotes. `None` for the stage's
    /// original notes.
    pub fn resolve_notes(&self, names: &[&str]) -> Option<PathBuf> {
        if !self.my_pacenotes {
            return None;
        }
        if let Some(pack) = self.notes_pack.as_deref() {
            return find_note(&self.here().join(NOTES_DIR).join(pack), names);
        }
        let rsf = self.rsf();
        pacenotes::my_pacenotes_dir(Path::new(rsf.trim())).and_then(|dir| pacenotes::my_pacenotes(&dir, names))
    }

    /// What the Stage notes row says for the stage with catalogue `names`: the resolved file, or why the stage's
    /// original notes are used.
    pub fn notes_line(&self, names: &[&str]) -> String {
        if !self.my_pacenotes {
            return "Off: the stage's original notes".into();
        }
        if let Some(file) = self.resolve_notes(names) {
            return format!("Custom: {}", file.display());
        }
        match &self.notes_pack {
            Some(pack) => format!("{pack} has no notes for this stage: the original notes"),
            None => match self.rsf().trim().is_empty() {
                true => "No note pack: the stage's original notes".into(),
                false => "No custom notes for this stage: the original notes".into(),
            },
        }
    }

    /// Asks for a co-driver download (a folder or a .zip/.7z archive) and imports it on a thread.
    pub fn import_codriver(&mut self) {
        if self.codriver_job.is_some() || self.child.is_some() {
            return;
        }
        let picked = install::pick_folder("The co-driver download's folder (or Cancel, then choose an archive)")
            .or_else(|| crate::disc_install::pick_file("The co-driver download's .zip/.7z archive"));
        if let Some(src) = picked {
            let home = self.here();
            self.codriver_status = None;
            self.codriver_job = Some(Job::start(move |report, cancel| {
                let _ = (report, cancel);
                import(&home, Path::new(&src))
            }));
        }
    }

    /// The import job's progress line and Cancel, or its result; the owned packs re-read after.
    pub fn import_codriver_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let idle = self.codriver_job.is_none() && self.child.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Import co-driver…"))
                .on_hover_text(
                    "Choose a co-driver download: an extracted folder, a .zip/.7z archive, or a whole RBR/RSF \
                     install to copy from (only read). ReRBR copies its voices, call styles, distance calls and \
                     stage notes into your files' folder; the source is not changed.",
                )
                .clicked()
            {
                self.import_codriver();
            }
            let Some(job) = &mut self.codriver_job else { return };
            if let Some(result) = job.ui(ui) {
                self.codriver_job = None;
                self.codriver_status = Some(match result {
                    Err(e) if e.ends_with("cancelled") => Err("Cancelled; nothing was written.".into()),
                    Ok(lines) => Ok(lines.join(" ")),
                    Err(e) => Err(e),
                });
                self.rescan_codrivers();
            }
        });
        match &self.codriver_status {
            Some(Ok(text)) => _ = ui.colored_label(GOOD, format!("✔ {text}")),
            Some(Err(text)) => _ = ui.colored_label(WARN, format!("⚠ {text}")),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rbr-codriver-packs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal plugin source: one style, one range set, one voice take.
    fn plugin_source(dir: &Path) {
        write(&dir.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        write(&dir.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(&dir.join("config/pacenotes/calls.ini"), "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n");
        write(&dir.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(&dir.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        write(&dir.join("sounds/english/steve/50_1.ogg"), "OggS fake");
        // Leftovers that must not ride along.
        write(&dir.join("PaceNote.dll"), "MZ fake");
        write(&dir.join("editor.exe"), "MZ fake");
    }

    fn notes_source(dir: &Path, stage: &str) {
        write(
            &dir.join(format!("{stage}/tanner.ini")),
            "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n",
        );
    }

    #[test]
    fn an_extracted_plugin_folder_imports_and_loads() {
        let dir = temp("folder");
        let src = dir.join("download");
        plugin_source(&src);
        let home = dir.join("home");
        let lines = import(&home, &src).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        let pack = home.join(VOICES_DIR).join(pack_name(&src));
        assert!(pack.join("PaceNote.ini").is_file());
        assert!(pack.join("config/pacenotes/calls.ini").is_file());
        assert!(pack.join("sounds/english/steve/one_left.ogg").is_file());
        assert!(!pack.join("PaceNote.dll").exists());
        assert!(!pack.join("editor.exe").exists());
        // The copied files still load after the source is gone.
        fs::remove_dir_all(&src).unwrap();
        let calls = codriver::load(&pack, "Rbr.ini", "Rbr.ini").unwrap();
        assert!(calls.notes.contains_key(&0));
        assert!(calls.ranges.contains_key(&50));
        assert_eq!(codriver::voices(&pack), ["english/steve"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wrapper_directory_and_a_direct_pacenote_folder_import() {
        let dir = temp("wrapper");
        let inner = dir.join("download").join("wrapper");
        plugin_source(&inner.join("Pacenote"));
        let home = dir.join("home");
        // A wrapper directory around a direct Pacenote folder.
        let lines = import(&home, &dir.join("download")).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(home.join(VOICES_DIR).join("download").join("PaceNote.ini").is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_whole_install_is_a_copy_source_only() {
        let dir = temp("install");
        let install = dir.join("rsf");
        plugin_source(&install.join("Plugins/Pacenote"));
        notes_source(&install.join("Plugins/NGPCarMenu/MyPacenotes"), "Tanner");
        let home = dir.join("home");
        let lines = import(&home, &install).unwrap();
        assert_eq!(lines.len(), 2, "{lines:?}");
        // The source is only read.
        assert!(install.join("Plugins/Pacenote/PaceNote.ini").is_file());
        let voice = home.join(VOICES_DIR).join("rsf");
        let notes = home.join(NOTES_DIR).join("rsf");
        assert!(voice.join("PaceNote.ini").is_file());
        assert!(find_note(&notes, &["Tanner"]).is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn audio_without_definitions_fails_actionably_and_leaves_nothing() {
        let dir = temp("audio-only");
        let src = dir.join("voice");
        write(&src.join("sounds/english/steve/one_left.ogg"), "OggS fake");
        let home = dir.join("home");
        let err = import(&home, &src).unwrap_err();
        assert!(err.contains("no call styles"), "{err}");
        assert!(!home.join(VOICES_DIR).exists() || fs::read_dir(home.join(VOICES_DIR)).unwrap().count() == 0);
        // A notes-only pack imports through the same entry point.
        let notes = dir.join("mynotes");
        notes_source(&notes, "Tanner");
        let lines = import(&home, &notes).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(find_note(&home.join(NOTES_DIR).join("mynotes"), &["Tanner"]).is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn packs_with_the_same_rbr_ini_stay_isolated() {
        let dir = temp("isolated");
        for (pack, id) in [("a", 0), ("b", 7)] {
            let src = dir.join(pack);
            write(&src.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
            write(
                &src.join("config/pacenotes/calls.ini"),
                &format!("[PACENOTE::CALL_{pack}]\nid={id}\nSounds=1\nSnd0={pack}.ogg\n"),
            );
            write(&src.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
            write(&src.join(format!("sounds/v/{pack}.ogg")), "OggS fake");
        }
        let home = dir.join("home");
        import(&home, &dir.join("a")).unwrap();
        import(&home, &dir.join("b")).unwrap();
        let (a, b) = (home.join(VOICES_DIR).join("a"), home.join(VOICES_DIR).join("b"));
        assert!(codriver::load(&a, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&0));
        assert!(!codriver::load(&a, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&7));
        assert!(codriver::load(&b, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&7));
        assert!(!codriver::load(&b, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&0));
        // An existing pack is never overwritten: the same source again gets a new name.
        import(&home, &dir.join("a")).unwrap();
        assert!(!home.join(VOICES_DIR).join("a-2").is_file());
        assert!(home.join(VOICES_DIR).join("a-2").is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn zip_wrapper_and_unsafe_paths() {
        let dir = temp("zip");
        let src = dir.join("download");
        plugin_source(&src);
        // A ZIP with a wrapper directory.
        let archive = dir.join("Voice Pack.zip");
        {
            let file = File::create(&archive).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            // Info-ZIP's explicit directory entries, as real downloads have them.
            zip.add_directory("wrapper/", options).unwrap();
            for (rel, path) in walk(&src) {
                if path.is_dir() {
                    continue;
                }
                zip.start_file(format!("wrapper/{rel}"), options).unwrap();
                let bytes = fs::read(&path).unwrap();
                std::io::Write::write_all(&mut zip, &bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        let home = dir.join("home");
        let lines = import(&home, &archive).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        let pack = home.join(VOICES_DIR).join("Voice Pack");
        assert!(pack.join("PaceNote.ini").is_file());
        // An unsafe archive leaves no pack behind.
        let evil = dir.join("evil.zip");
        {
            let file = File::create(&evil).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("wrapper/../../evil.ini", options).unwrap();
            std::io::Write::write_all(&mut zip, b"[PACENOTES]\ncount = 0\n").unwrap();
            zip.start_file("wrapper/config/pacenotes/Rbr.ini", options).unwrap();
            std::io::Write::write_all(&mut zip, b"[PACKAGE::RBR]\nfile=calls.ini\n").unwrap();
            zip.finish().unwrap();
        }
        let before: Vec<String> = fs::read_dir(home.join(VOICES_DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let _ = import(&home, &evil);
        let after: Vec<String> = fs::read_dir(home.join(VOICES_DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!after.iter().any(|n| n.starts_with("evil")), "{after:?}");
        assert_eq!(before.len(), after.len(), "{before:?} vs {after:?}");
        assert!(!dir.join("evil.ini").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn seven_z_and_notes_only_archives() {
        let dir = temp("seven");
        let src = dir.join("notes-src");
        notes_source(&src, "Tanner");
        let archive = dir.join("tanner-notes.7z");
        {
            let mut out = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
            for (rel, path) in walk(&src) {
                if path.is_dir() {
                    continue;
                }
                let name = format!("top/{rel}");
                let bytes = fs::read(&path).unwrap();
                out.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(&name), Some(bytes.as_slice())).unwrap();
            }
            out.finish().unwrap();
        }
        let home = dir.join("home");
        let lines = import(&home, &archive).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(find_note(&home.join(NOTES_DIR).join("tanner-notes"), &["Tanner"]).is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn owned_packs_list_with_their_problems() {
        use crate::disc_install::Progress;
        use std::sync::atomic::AtomicBool;
        let dir = temp("list");
        let src = dir.join("voice");
        plugin_source(&src);
        let home = dir.join("home");
        import(&home, &src).unwrap();
        let voices = voice_packs(&home);
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].problem, None);
        let progress = Progress { step: "Importing", done: 0, total: 0 };
        assert_eq!(progress.line(), "Importing…");
        let _ = AtomicBool::new(false);
        fs::remove_dir_all(&dir).unwrap();
    }
}

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
//! existing install to COPY from — never modified), or a downloaded ZIP/7z archive of the same. The plugin is
//! found at the top, in a direct `Pacenote` (or `Plugins/Pacenote`) folder beside the notes, or under a single
//! wrapper directory (including `wrapper/Pacenote` and `wrapper/Plugins/Pacenote`); note paths are always kept
//! whole, so stage folders keep their names. Only relevant files are copied: `PaceNote.ini`,
//! `config/**/*.ini`, voice audio (`sounds/`, `.ogg`/`.wav`/`.mp3`/`.flac`/`.oga`) and parseable note INIs. No
//! DLLs, executables or anything else. A failed import leaves no active partial pack: everything stages in a temp
//! folder beside the destination and is renamed only when validated. Existing packs are never overwritten: a taken
//! name gets a `-2`, `-3` suffix. An audio-only download without the style/range definitions it needs fails with an
//! actionable error (or a pack listed with its missing dependency), and never appears ready.

use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rbr_formats::rsf::{codriver, pacenotes};

use crate::disc_install::{Job, Progress, Temp};
use crate::pages::{GOOD, WARN};
use crate::{install, Form};

/// The folders in the player's folder with the owned packs.
pub const VOICES_DIR: &str = "codrivers";
pub const NOTES_DIR: &str = "notes";
/// The archive suffixes taken as downloads.
const ZIP: &str = ".zip";
const SEVEN_Z: &str = ".7z";

type Fail = String;

/// A locally owned voice pack: its folder (as the plugin's folder) and what it offers, or why it is not ready
/// (the same validation `check_voice` refuses at activation, so a corrupt pack never shows ready).
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

/// The files below `dir` (relative `/`-separated paths and their absolute paths), sorted. Symlinks are never
/// followed or copied: a linked directory would loop, and a linked file belongs to its target's pack, not this one.
fn walk(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(top) = stack.pop() {
        let Ok(entries) = fs::read_dir(&top) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
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

/// The plugin's folder below `dir`: itself, a direct `Pacenote` (or `Plugins/Pacenote`) folder beside note
/// folders, or the same under a single wrapper directory (up to two levels down). `None` when there is none
/// (notes-only sources reach here too: their stage folders stay where they are, since the note lookup is
/// recursive). Archive discovery (`archive_plugin_root`) mirrors this list.
fn find_voice_root(dir: &Path) -> Option<PathBuf> {
    let candidates = voice_root_candidates(dir);
    candidates.into_iter().find(|c| is_plugin_dir(c))
}

/// The plugin folders to try below `dir`, in order: itself, the install's, each direct child, each child's
/// `Pacenote` and `Plugins/Pacenote`, then the same under a single wrapper directory (bounded: two levels).
fn voice_root_candidates(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir.to_owned(), dir.join("Plugins").join("Pacenote")];
    let children: Vec<PathBuf> = fs::read_dir(dir)
        .map(|e| e.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    // A download with the plugin beside the notes (`Pacenote/`, `MyPacenotes/`).
    for child in &children {
        out.push(child.clone());
    }
    for child in &children {
        out.push(child.join("Pacenote"));
        out.push(child.join("Plugins").join("Pacenote"));
    }
    // Up to two single wrapper directories down.
    let [child] = &children[..] else { return out };
    let grandchildren: Vec<PathBuf> = fs::read_dir(child)
        .map(|e| e.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    for grand in &grandchildren {
        out.push(grand.clone());
        out.push(grand.join("Pacenote"));
        out.push(grand.join("Plugins").join("Pacenote"));
    }
    out
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

/// Stops the import when `cancel` is set: every file and archive entry checks it.
fn cancelled(cancel: &AtomicBool) -> Result<(), Fail> {
    match cancel.load(Ordering::Relaxed) {
        true => Err("cancelled".into()),
        false => Ok(()),
    }
}

/// The pack folders below `dir`: directories only, hidden staging folders (`.import-…`) and symlinks left out.
fn pack_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|e| {
            e.filter_map(Result::ok)
                .filter(|e| {
                    e.file_name().to_str().is_some_and(|n| !n.starts_with('.'))
                        && e.file_type().is_ok_and(|k| k.is_dir() && !k.is_symlink())
                })
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The voice packs owned (each folder of `home/codrivers/`), with their problem when not ready: the missing
/// part, or the validation (`check_voice`) failure `active_mods` would refuse with, so the list never shows a
/// corrupt pack as ready.
pub fn voice_packs(home: &Path) -> Vec<VoicePack> {
    pack_names(&home.join(VOICES_DIR))
        .into_iter()
        .map(|name| {
            let dir = home.join(VOICES_DIR).join(&name);
            let (voices, styles, ranges) = (codriver::voices(&dir), codriver::styles(&dir), codriver::range_sets(&dir));
            let mut problem = voice_problem(&voices, &styles, &ranges);
            if problem.is_none() {
                problem = voice_check_problem(&dir);
            }
            VoicePack { name, dir, voices, styles, ranges, problem }
        })
        .collect()
}

/// The `check_voice` failure `active_mods` would refuse the pack with, without its folder prefix, if it fails.
fn voice_check_problem(dir: &Path) -> Option<String> {
    check_voice(dir).err().map(|e| e.strip_prefix(&format!("{}: ", dir.display())).unwrap_or(&e).to_owned())
}

/// Why the pack's voices, styles and range sets are not a ready co-driver, if they are not: each missing part
/// is named on its own, so an audio-only download learns which definitions to add.
fn voice_problem(voices: &[String], styles: &[String], ranges: &[String]) -> Option<String> {
    match (voices.is_empty(), styles.is_empty(), ranges.is_empty()) {
        (true, true, true) => Some("no voices, call styles or distance calls: is it a co-driver pack?".into()),
        (true, _, _) => Some(
            "no voices (sounds/*/*.ogg): import the voice pack these call styles need, or pick another pack".into(),
        ),
        (_, true, _) => Some(
            "no call styles (config/pacenotes/*.ini): import the style pack this voice needs, or pick another pack"
                .into(),
        ),
        (_, _, true) => Some(
            "no distance calls (config/ranges/*.ini): import the range set this voice needs, or pick another pack"
                .into(),
        ),
        _ => None,
    }
}

/// The stage-note packs owned (each folder of `home/notes/` with at least one note file).
pub fn note_packs(home: &Path) -> Vec<NotePack> {
    pack_names(&home.join(NOTES_DIR))
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

/// The note file the pack plays on the stage RSF calls by one of `names`: every note INI below `dir`, matched
/// first by its parent folder's name (folded as `pacenotes::my_pacenotes` matches stage folders), then by its file
/// stem (a bare `Tanner/tanner.ini` keeps its stage identity either way); the file modified last. Ours on a tie:
/// a file other than NGPCarMenu's `_default` export, then the name.
pub fn find_note(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    let wanted: Vec<String> = names.iter().map(|n| fold(n)).filter(|n| !n.is_empty()).collect();
    if wanted.is_empty() {
        return None;
    }
    let inis: Vec<(String, PathBuf)> =
        walk(dir).into_iter().filter(|(rel, _)| rel.to_ascii_lowercase().ends_with(".ini")).collect();
    let folder_hit: Vec<(String, PathBuf)> = inis
        .iter()
        .filter(|(rel, _)| {
            Path::new(rel)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .is_some_and(|n| wanted.contains(&fold(n)))
        })
        .cloned()
        .collect();
    // A bare note file at the pack's top still names its stage by its stem.
    let pool = match folder_hit.is_empty() {
        false => folder_hit,
        true => inis
            .into_iter()
            .filter(|(rel, _)| {
                Path::new(rel).file_stem().and_then(|n| n.to_str()).is_some_and(|n| wanted.contains(&fold(n)))
            })
            .collect(),
    };
    pool.into_iter()
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

/// Copies `path` to `to` (making its folders), keeping the source's modification time: the newest of several
/// stage-note files for one stage keeps winning after the copy. A time that cannot be kept is ignored.
fn copy_file(path: &Path, bytes: &[u8], to: &Path) -> Result<(), Fail> {
    let modified = path.metadata().and_then(|m| m.modified()).ok();
    fs::create_dir_all(to.parent().unwrap_or(to)).map_err(|e| format!("{}: {e}", to.display()))?;
    fs::write(to, bytes).map_err(|e| format!("{}: {e}", to.display()))?;
    if let Some(modified) = modified {
        let _ = fs::File::options().write(true).open(to).and_then(|f| f.set_modified(modified));
    }
    Ok(())
}

/// Copies the voice files (see [`keep_voice`]) from the plugin's folder `src` into `dest`, preserving relative
/// paths; the files copied.
fn copy_voice(src: &Path, dest: &Path, cancel: &AtomicBool) -> Result<usize, Fail> {
    let mut n = 0;
    for (rel, path) in walk(src) {
        cancelled(cancel)?;
        let parts = safe_path(&rel).ok_or_else(|| format!("{}: a bad path {rel}", src.display()))?;
        if !keep_voice(&parts) {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let to = parts.iter().fold(dest.to_owned(), |p, part| p.join(part));
        copy_file(&path, &bytes, &to)?;
        n += 1;
    }
    Ok(n)
}

/// Copies the note files below `src` into `dest`, preserving relative paths; the files copied. Only files that
/// parse as note files are kept, so install leftovers never ride along. A source that is one stage's own folder
/// (its note INIs at the top, none in subfolders) keeps its folder's name over them, so the stage identity
/// survives: a bare `Tanner/` folder still resolves as Tanner.
fn copy_notes(src: &Path, dest: &Path, cancel: &AtomicBool) -> Result<usize, Fail> {
    let mut found: Vec<(String, Vec<u8>, PathBuf)> = Vec::new();
    for (rel, path) in walk(src) {
        cancelled(cancel)?;
        if !rel.to_ascii_lowercase().ends_with(".ini") {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !is_note_text(&String::from_utf8_lossy(&bytes)) {
            continue;
        }
        found.push((rel, bytes, path));
    }
    let nested = found.iter().any(|(rel, _, _)| rel.contains('/'));
    let stage = (!nested).then(|| src.file_name().and_then(|n| n.to_str()).and_then(safe_folder)).flatten();
    let mut n = 0;
    for (rel, bytes, path) in &found {
        cancelled(cancel)?;
        let rel = match &stage {
            Some(stage) => format!("{stage}/{rel}"),
            None => rel.clone(),
        };
        let parts = safe_path(&rel).ok_or_else(|| format!("{}: a bad path {rel}", src.display()))?;
        let to = parts.iter().fold(dest.to_owned(), |p, part| p.join(part));
        copy_file(path, bytes, &to)?;
        n += 1;
    }
    Ok(n)
}

/// Validates a staged voice pack: its styles and range sets read with actual call definitions, and its first
/// style with its first range set loads (catching dangling package references). Audio-only downloads fail here
/// with what to add.
fn check_voice(dir: &Path) -> Result<(Vec<String>, Vec<String>), Fail> {
    let (voices, styles, ranges) = (codriver::voices(dir), codriver::styles(dir), codriver::range_sets(dir));
    if let Some(problem) = voice_problem(&voices, &styles, &ranges) {
        return Err(format!("{}: {problem}", dir.display()));
    }
    let calls = codriver::load(dir, &styles[0], &ranges[0]).map_err(|e| format!("{}: {e}", dir.display()))?;
    if calls.notes.is_empty() && calls.named.is_empty() {
        return Err(format!("{}: {} defines no calls", dir.display(), styles[0]));
    }
    if calls.ranges.is_empty() {
        return Err(format!("{}: {} defines no distance calls", dir.display(), ranges[0]));
    }
    Ok((styles, ranges))
}

/// The MyPacenotes folder of an install's `Plugins/NGPCarMenu.ini` (`MyPacenotesPath`, relative to the install or
/// absolute), else NGPCarMenu's default; `None` when disabled or missing.
fn install_notes_dir(root: &Path) -> Option<PathBuf> {
    pacenotes::my_pacenotes_dir(root)
}

/// Imports the folder `src` (a plugin folder, a note folder, a folder with both, or a whole install to copy from)
/// into `home`: the lines saying what was added.
fn import_folder(home: &Path, src: &Path, cancel: &AtomicBool) -> Result<Vec<String>, Fail> {
    if !src.is_dir() {
        return Err(format!("{} is not a folder", src.display()));
    }
    let name = pack_name(src);
    // A whole existing install: its plugin and its MyPacenotes folder are the sources, copied, never modified.
    if src.join("Plugins").is_dir() {
        let plugin = src.join("Plugins").join("Pacenote");
        let voice = is_plugin_dir(&plugin).then(|| plugin.clone());
        let notes = install_notes_dir(src);
        return match (voice, notes) {
            (None, None) => Err(format!(
                "{}: no Pacenote plugin (Plugins/Pacenote) and no stage notes ({}): is it an RBR install?",
                src.display(),
                pacenotes::my_pacenotes_dir(src)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| { "Plugins/NGPCarMenu/MyPacenotes".into() })
            )),
            (Some(plugin), None) => import_voice_from(home, &name, cancel, |d, c| copy_voice(&plugin, d, c)),
            (None, Some(notes)) => import_notes_from(home, &name, &notes, cancel, |d, c| copy_notes(&notes, d, c)),
            (Some(plugin), Some(notes)) => import_both(home, &name, cancel, &plugin, &notes),
        };
    }
    let voice = find_voice_root(src);
    // The notes are searched recursively, so wrappers stay on: the stage folders keep their names.
    let notes = note_files(src).len();
    if let Some(root) = voice {
        if notes == 0 {
            return import_voice_from(home, &name, cancel, |d, c| copy_voice(&root, d, c));
        }
        return import_both(home, &name, cancel, &root, src);
    }
    if notes > 0 {
        return import_notes_from(home, &name, src, cancel, |d, c| copy_notes(src, d, c));
    }
    Err(format!(
        "{}: no co-driver voices, call styles or stage notes in it: choose the plugin's folder \
         (PaceNote.ini, sounds/, config/), a MyPacenotes folder, or a whole install to copy from",
        src.display()
    ))
}

/// Imports a voice pack and a note pack as one atomic operation: both stage into temp folders and validate, and
/// only then both publish by rename. The cancellation flag is checked immediately before the commit, and a
/// failure publishing the notes removes the just-published voice again. A failure anywhere else leaves no pack
/// behind (no half-installed operation).
fn import_both(
    home: &Path,
    name: &str,
    cancel: &AtomicBool,
    voice_src: &Path,
    notes_src: &Path,
) -> Result<Vec<String>, Fail> {
    let voice = stage(home, VOICES_DIR, name)?;
    let notes = stage(home, NOTES_DIR, name)?;
    copy_voice(voice_src, &voice.staged, cancel)?;
    let (styles, ranges) = check_voice(&voice.staged)?;
    copy_notes(notes_src, &notes.staged, cancel)?;
    let n = check_notes(&notes.staged, notes_src)?;
    publish_pair(&voice, &notes, cancel)?;
    Ok(vec![voice_line(&voice.dest, &styles, &ranges), notes_line(&notes.dest, n)])
}

/// A staged import: the temp folder holding it, the validated folder inside, and the published folder it renames to.
struct Staged {
    _temp: Temp,
    staged: PathBuf,
    dest: PathBuf,
}

/// Stages a pack named `name` below `home/dir`, in a temp folder renamed to a free pack folder on publish.
fn stage(home: &Path, dir: &str, name: &str) -> Result<Staged, Fail> {
    let base = home.join(dir);
    fs::create_dir_all(&base).map_err(|e| format!("{}: {e}", base.display()))?;
    let dest = free_dir(&base, &safe_folder(name).unwrap_or_else(|| "pack".to_owned()));
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp = Temp(base.join(format!(".import-{}-{stamp}", std::process::id())));
    fs::create_dir(&temp.0).map_err(|e| format!("{}: {e}", temp.0.display()))?;
    let staged = temp.0.join("pack");
    fs::create_dir(&staged).map_err(|e| format!("{}: {e}", staged.display()))?;
    Ok(Staged { _temp: temp, staged, dest })
}

/// Publishes a validated staged pack by rename; dropping the staging removes its temp folder. Checks the
/// cancellation flag immediately before the commit, so a cancel after staging publishes nothing.
fn publish(op: &Staged, cancel: &AtomicBool) -> Result<(), Fail> {
    cancelled(cancel)?;
    fs::rename(&op.staged, &op.dest).map_err(|e| format!("{}: {e}", op.dest.display()))
}

/// Publishes a staged voice and note pair: the notes failing (or a cancel between the two renames) removes the
/// just-published voice again, so a dual publish never leaves a lone voice behind despite the atomicity claim.
fn publish_pair(voice: &Staged, notes: &Staged, cancel: &AtomicBool) -> Result<(), Fail> {
    publish(voice, cancel)?;
    match publish(notes, cancel) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_dir_all(&voice.dest);
            Err(e)
        }
    }
}

/// The line saying what voice pack published.
fn voice_line(dest: &Path, styles: &[String], ranges: &[String]) -> String {
    let pack = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    format!("Co-driver {pack}: {} call styles, {} range sets.", styles.len(), ranges.len())
}

/// The line saying what note pack published.
fn notes_line(dest: &Path, n: usize) -> String {
    let pack = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    format!("Stage notes {pack}: {n} files.")
}

/// Requires staged notes to hold at least one note file: the files, else what to choose instead.
fn check_notes(staged: &Path, src: &Path) -> Result<usize, Fail> {
    match note_files(staged).len() {
        0 => Err(format!(
            "{}: no stage notes (*.ini with [PACENOTES]) in it: choose a MyPacenotes folder or a note pack",
            src.display()
        )),
        n => Ok(n),
    }
}

/// Stages a voice pack named `name` with `copy` (into a temp folder, validated, then published): the lines saying
/// what was added. A failure leaves no pack behind.
fn import_voice_from(
    home: &Path,
    name: &str,
    cancel: &AtomicBool,
    copy: impl FnOnce(&Path, &AtomicBool) -> Result<usize, Fail>,
) -> Result<Vec<String>, Fail> {
    let op = stage(home, VOICES_DIR, name)?;
    copy(&op.staged, cancel)?;
    let (styles, ranges) = check_voice(&op.staged)?;
    publish(&op, cancel)?;
    Ok(vec![voice_line(&op.dest, &styles, &ranges)])
}

/// Stages a note pack named `name` with `copy`: the lines saying what was added.
fn import_notes_from(
    home: &Path,
    name: &str,
    src: &Path,
    cancel: &AtomicBool,
    copy: impl FnOnce(&Path, &AtomicBool) -> Result<usize, Fail>,
) -> Result<Vec<String>, Fail> {
    let op = stage(home, NOTES_DIR, name)?;
    copy(&op.staged, cancel)?;
    let n = check_notes(&op.staged, src)?;
    publish(&op, cancel)?;
    Ok(vec![notes_line(&op.dest, n)])
}

/// The file entries of the ZIP archive `src`: their raw path parts (no wrapper stripped, so a single-stage
/// archive's stage folder keeps its name). Unsafe paths and directory entries are left out.
fn zip_entries(src: &Path) -> Result<Vec<Vec<String>>, Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let zip = rbr_formats::rbz::open(src).map_err(|e| error(e.to_string()))?;
    let mut entries = Vec::new();
    for k in 0..zip.len() {
        let name = zip.name_for_index(k).and_then(Result::ok).ok_or_else(|| error("a bad entry".into()))?;
        let dir = name.ends_with(['/', '\\']);
        let Some(parts) = safe_path(&name) else { continue };
        if !dir {
            entries.push(parts.iter().map(|p| p.to_string()).collect());
        }
    }
    Ok(entries)
}

/// The file entries of the 7z archive `src`: raw path parts, as [`zip_entries`].
fn seven_entries(src: &Path) -> Result<Vec<Vec<String>>, Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let file = File::open(src).map_err(|e| error(e.to_string()))?;
    let archive = sevenz_rust2::ArchiveReader::new(BufReader::new(file), sevenz_rust2::Password::empty())
        .map_err(|e| error(format!("not a .7z: {e}")))?;
    let mut entries = Vec::new();
    for f in &archive.archive().files {
        let Some(parts) = safe_path(&f.name) else { continue };
        if !f.is_directory {
            entries.push(parts.iter().map(|p| p.to_string()).collect());
        }
    }
    Ok(entries)
}

/// Whether the archive entries hold a plugin at `root` (raw path parts): its settings, voices or definitions
/// below it.
fn archive_plugin_at(entries: &[Vec<String>], root: &[String]) -> bool {
    let below = entries.iter().filter_map(|p| p.strip_prefix(root)).map(|p| p.join("/").to_ascii_lowercase());
    let mut has = (false, false, false);
    for rel in below {
        has.0 |= rel == "pacenote.ini";
        has.1 |= rel.starts_with("sounds/");
        has.2 |= rel.starts_with("config/pacenotes/") || rel.starts_with("config/ranges/");
    }
    has.0 || has.1 || has.2
}

/// The plugin's root in the entries: top level, an install's `Plugins/Pacenote`, a single folder holding one
/// (a direct `Pacenote` folder beside the notes), or the same under a single wrapper directory (including
/// `wrapper/Pacenote` and `wrapper/Plugins/Pacenote`). Mirrors `find_voice_root`; note paths always stay whole,
/// so a single-stage archive's stage folder keeps its name.
fn archive_plugin_root(entries: &[Vec<String>]) -> Option<Vec<String>> {
    let mut candidates: Vec<Vec<String>> = vec![Vec::new(), vec!["Plugins".into(), "Pacenote".into()]];
    let mut tops: Vec<String> = entries.iter().filter_map(|p| p.first().cloned()).collect();
    tops.sort();
    tops.dedup();
    for top in &tops {
        candidates.push(vec![top.clone()]);
    }
    for top in &tops {
        candidates.push(vec![top.clone(), "Pacenote".into()]);
        candidates.push(vec![top.clone(), "Plugins".into(), "Pacenote".into()]);
    }
    // The same under a single wrapper directory (two levels down, bounded).
    if let [wrapper] = &tops[..] {
        let prefix = [wrapper.clone()];
        let mut subs: Vec<String> =
            entries.iter().filter_map(|p| p.strip_prefix(&prefix)).filter_map(|rest| rest.first().cloned()).collect();
        subs.sort();
        subs.dedup();
        for sub in &subs {
            candidates.push(vec![wrapper.clone(), sub.clone()]);
            candidates.push(vec![wrapper.clone(), sub.clone(), "Pacenote".into()]);
            candidates.push(vec![wrapper.clone(), sub.clone(), "Plugins".into(), "Pacenote".into()]);
        }
    }
    candidates.into_iter().find(|root| archive_plugin_at(entries, root))
}

/// The note files among the entries: every INI except the voice files below the plugin's `root` (the style and
/// range definitions are not stage notes; anything else unparseable is pruned after extraction). Paths stay whole,
/// so a single-stage archive's stage folder keeps its name.
fn archive_note_candidates(entries: &[Vec<String>], root: Option<&[String]>) -> Vec<Vec<String>> {
    entries
        .iter()
        .filter(|p| p.last().is_some_and(|n| n.to_ascii_lowercase().ends_with(".ini")))
        .filter(|p| match root {
            Some(root) if p.len() > root.len() && p[..root.len()] == root[..] => {
                let below: Vec<&str> = p[root.len()..].iter().map(String::as_str).collect();
                !keep_voice(&below)
            }
            _ => true,
        })
        .cloned()
        .collect()
}

/// A ZIP entry's modification time: days from the civil date, seconds from the time of day (the order the
/// stage-note newest-wins rule compares). Out-of-range values give `None`.
fn dos_time(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Option<std::time::SystemTime> {
    if !(1..=12).contains(&month) || day == 0 || day > 31 || hour > 23 || minute > 59 || second > 61 {
        return None;
    }
    let (y, m) = (i64::from(year), i64::from(month));
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let secs =
        (era * 146097 + doe - 719468) * 86400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    std::time::SystemTime::UNIX_EPOCH.checked_add(std::time::Duration::from_secs(secs.max(0) as u64))
}

/// Extracts the ZIP's `wanted` (raw entry path to pack-relative path) into `dest`, keeping each entry's
/// modification time where the archive names one. Unsafe paths are skipped.
fn extract_zip(
    src: &Path,
    dest: &Path,
    wanted: &[(Vec<String>, Vec<String>)],
    cancel: &AtomicBool,
) -> Result<(), Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let mut zip = rbr_formats::rbz::open(src).map_err(|e| error(e.to_string()))?;
    for (from, to) in wanted {
        cancelled(cancel)?;
        let found = (0..zip.len()).find(|&k| {
            zip.name_for_index(k)
                .and_then(Result::ok)
                .is_some_and(|n| safe_path(&n).is_some_and(|p| p == from.as_slice()))
        });
        let Some(k) = found else { continue };
        let mut file = zip.by_index(k).map_err(|e| error(e.to_string()))?;
        let modified =
            file.last_modified().and_then(|d| dos_time(d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second()));
        write_to(dest, to, &mut file, modified)?;
    }
    Ok(())
}

/// Writes `reader` to `dest`/`parts`, making its folders and keeping `modified` (best effort, for the stage-note
/// newest-wins order). Voice and note files only ever arrive through the allowlists above.
fn write_to(
    dest: &Path,
    parts: &[String],
    reader: &mut dyn Read,
    modified: Option<std::time::SystemTime>,
) -> Result<(), Fail> {
    let safe: Vec<&str> = parts.iter().map(String::as_str).collect();
    let path = safe.iter().fold(dest.to_owned(), |p, part| p.join(part));
    fs::create_dir_all(path.parent().unwrap_or(dest)).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::copy(reader, &mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    drop(out);
    if let Some(modified) = modified {
        let _ = fs::File::options().write(true).open(&path).and_then(|f| f.set_modified(modified));
    }
    Ok(())
}

/// Extracts the 7z's `wanted` (raw entry path to pack-relative path) into `dest`, keeping modification times.
fn extract_seven(
    src: &Path,
    dest: &Path,
    wanted: &[(Vec<String>, Vec<String>)],
    cancel: &AtomicBool,
) -> Result<(), Fail> {
    let error = |e: String| format!("{}: {e}", src.display());
    let file = File::open(src).map_err(|e| error(e.to_string()))?;
    let mut archive = sevenz_rust2::ArchiveReader::new(BufReader::new(file), sevenz_rust2::Password::empty())
        .map_err(|e| error(e.to_string()))?;
    let mut failed: Option<Fail> = None;
    archive
        .for_each_entries(|entry, reader| {
            if failed.is_some() {
                return Ok(false);
            }
            if cancel.load(Ordering::Relaxed) {
                failed = Some("cancelled".into());
                return Ok(false);
            }
            if entry.is_directory {
                return Ok(true);
            }
            let Some(parts) = safe_path(&entry.name) else { return Ok(true) };
            let parts: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
            if let Some((_, to)) = wanted.iter().find(|(from, _)| *from == parts) {
                let modified =
                    entry.has_last_modified_date.then(|| std::time::SystemTime::from(entry.last_modified_date()));
                match write_to(dest, to, reader, modified) {
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

/// Imports the archive `src` (ZIP or 7z) as one atomic operation: the voice and the notes stage and validate
/// first, and only then both publish (a notes publish failure rolls the just-published voice back). Note
/// candidates that prune to no parseable note file fall back to a voice-only import, so an unrelated INI next
/// to a valid voice never rejects it; an archive with no note INIs at all imports its voice alone, and a
/// notes-only archive with no parseable notes fails loudly. Extraction errors propagate, never swallowed.
fn import_archive(home: &Path, src: &Path, cancel: &AtomicBool) -> Result<Vec<String>, Fail> {
    let lower = src.to_string_lossy().to_ascii_lowercase();
    let entries = match lower.ends_with(ZIP) {
        true => zip_entries(src),
        false => seven_entries(src),
    }?;
    if entries.iter().all(|p| {
        let n = p.last().map(|n| n.to_ascii_lowercase()).unwrap_or_default();
        n.ends_with(".dll") || n.ends_with(".exe")
    }) && !entries.is_empty()
    {
        return Err(format!("{}: no co-driver files in it, only executables", src.display()));
    }
    let name = pack_name(src);
    let extract = |dest: &Path, wanted: &[(Vec<String>, Vec<String>)]| match src
        .to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(ZIP)
    {
        true => extract_zip(src, dest, wanted, cancel),
        false => extract_seven(src, dest, wanted, cancel),
    };
    let root = archive_plugin_root(&entries);
    let voice: Vec<(Vec<String>, Vec<String>)> = root
        .clone()
        .map(|root| {
            entries
                .iter()
                .filter(|p| p.len() >= root.len() && p[..root.len()] == root[..])
                .map(|p| p[root.len()..].to_vec())
                .filter(|p| !p.is_empty() && safe_path(&p.join("/")).is_some_and(|s| keep_voice(&s)))
                .map(|p| {
                    let mut from = root.clone();
                    from.extend(p.clone());
                    (from, p)
                })
                .collect()
        })
        .unwrap_or_default();
    let notes: Vec<Vec<String>> = archive_note_candidates(&entries, root.as_deref());
    match (voice.is_empty(), notes.is_empty()) {
        (true, true) => Err(format!(
            "{}: no co-driver voices, call styles or stage notes in it: is it a co-driver or note pack?",
            src.display()
        )),
        (false, true) => import_voice_from(home, &name, cancel, |d, _| {
            extract(d, &voice)?;
            Ok(walk(d).len())
        }),
        (true, false) => import_notes_from(home, &name, src, cancel, |d, _| {
            extract_notes(src, d, &notes, cancel)?;
            Ok(note_files(d).len())
        }),
        (false, false) => {
            let v = stage(home, VOICES_DIR, &name)?;
            let n = stage(home, NOTES_DIR, &name)?;
            extract(&v.staged, &voice)?;
            let (styles, ranges) = check_voice(&v.staged)?;
            extract_notes(src, &n.staged, &notes, cancel)?;
            if note_files(&n.staged).is_empty() {
                // Only unrelated INIs beside a valid voice: keep the voice, drop the empty notes staging.
                publish(&v, cancel)?;
                return Ok(vec![voice_line(&v.dest, &styles, &ranges)]);
            }
            let count = check_notes(&n.staged, src)?;
            publish_pair(&v, &n, cancel)?;
            Ok(vec![voice_line(&v.dest, &styles, &ranges), notes_line(&n.dest, count)])
        }
    }
}

/// Extracts every candidate note INI whole (paths stay whole, so stage folders keep their names) and prunes what
/// does not parse as a note file.
fn extract_notes(src: &Path, dest: &Path, notes: &[Vec<String>], cancel: &AtomicBool) -> Result<(), Fail> {
    let wanted: Vec<(Vec<String>, Vec<String>)> = notes.iter().map(|p| (p.clone(), p.clone())).collect();
    match src.to_string_lossy().to_ascii_lowercase().ends_with(ZIP) {
        true => extract_zip(src, dest, &wanted, cancel),
        false => extract_seven(src, dest, &wanted, cancel),
    }?;
    prune_not_notes(dest);
    cancelled(cancel)
}

/// Deletes the staged INIs that do not parse as note files.
fn prune_not_notes(dir: &Path) {
    for (_, path) in walk(dir) {
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")) && !is_note_file(&path) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Imports `src` (a folder, a ZIP/7z archive, or a whole install to copy from) into `home`: the lines saying what
/// was added. The source is only read, never modified; existing packs are never overwritten. `cancel` stops the
/// copy and the extraction; a cancelled or failed import publishes nothing.
pub fn import(home: &Path, src: &Path, cancel: &AtomicBool) -> Result<Vec<String>, Fail> {
    if !src.exists() {
        return Err(format!("{} does not exist", src.display()));
    }
    if src.is_dir() {
        return import_folder(home, src, cancel);
    }
    let lower = src.to_string_lossy().to_ascii_lowercase();
    match lower.ends_with(ZIP) || lower.ends_with(SEVEN_Z) {
        true => import_archive(home, src, cancel),
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
    let lines = import(home, Path::new(src), &AtomicBool::new(false))?;
    lines.iter().for_each(|l| println!("{l}"));
    println!("{}", home.display());
    Ok(())
}

impl Form {
    /// Re-reads the owned packs and points the co-driver at the chosen ones: the local voice pack
    /// (`codriver_pack`), else the RSF install's plugin; a missing local pack keeps the last lists and says so.
    /// Marks the RSF source and the Stage notes row for refresh (the row itself refreshes on draw).
    pub fn rescan_codrivers(&mut self) {
        let home = self.here();
        self.owned_voices = voice_packs(&home);
        self.owned_notes = note_packs(&home);
        self.codriver_mods = self.active_mods();
        self.rsf_plugin = crate::command::CodriverMods::scan(&self.rsf()).is_some();
        self.cached_notes_dirty = true;
    }

    /// Refreshes the Stage notes row's cached text when the map, the notes sources, the co-driver switch or voice,
    /// or an import changed it. A custom file only drives when the co-driver is on with a custom voice (the
    /// viewer only gets `--pacenotes` with one): otherwise the row says the custom notes are inactive instead of
    /// naming a file that is not used.
    pub fn refresh_notes_cache(&mut self) {
        let map = self.catalog.maps.get(self.map).map(|m| m.number).unwrap_or(0);
        if !self.cached_notes_dirty && self.cached_notes_map == map {
            return;
        }
        self.cached_notes_map = map;
        self.cached_notes_dirty = false;
        let names = self
            .catalog
            .maps
            .iter()
            .find(|m| m.number == map)
            .and_then(|m| m.stages.first())
            .map(|stage| self.stage_note_names(stage));
        let Some(names) = names else {
            self.cached_notes_short = "No stage: the original notes".into();
            self.cached_notes_full = String::new();
            return;
        };
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        if self.codriver_mods.is_some()
            && !(self.codriver
                && self.codriver_mods.as_ref().is_some_and(|m| m.voice(self.codriver_voice.as_deref()).is_some()))
        {
            self.cached_notes_short = match self.codriver {
                false => "Co-driver off: custom notes inactive".into(),
                true => "1.02 voice: custom notes inactive".into(),
            };
            self.cached_notes_full = String::new();
            return;
        }
        if !self.my_pacenotes {
            self.cached_notes_short = "Off: the stage's original notes".into();
            self.cached_notes_full = String::new();
            return;
        }
        if let Some(file) = self.resolve_notes(&refs) {
            self.cached_notes_full = file.display().to_string();
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.cached_notes_short = format!("Custom: {name}");
            return;
        }
        self.cached_notes_full = String::new();
        self.cached_notes_short = match &self.notes_pack {
            Some(pack) => format!("{pack} has no notes for this stage: the original notes"),
            None => match self.rsf().trim().is_empty() {
                true => "No note pack: the stage's original notes".into(),
                false => "No custom notes for this stage: the original notes".into(),
            },
        };
    }

    /// Marks the Stage notes row dirty when its inputs changed since `before` (note pack, checkbox, the co-driver
    /// switch or the voice): the row refreshes on the next draw, so repaint for it. Without this a switched note
    /// pack or checkbox keeps the old file until the map changes.
    pub fn invalidate_notes_cache(
        &mut self,
        ctx: &egui::Context,
        before: (Option<String>, bool, bool, Option<String>),
    ) {
        let now = (self.notes_pack.clone(), self.my_pacenotes, self.codriver, self.codriver_voice.clone());
        if before != now {
            self.cached_notes_dirty = true;
            ctx.request_repaint();
        }
    }
    /// (the original 1.02 voice then). A chosen pack listed as not ready, or failing validation, is refused: the
    /// viewer never gets a pack that cannot speak.
    pub fn active_mods(&self) -> Option<crate::command::CodriverMods> {
        if let Some(pack) = self.codriver_pack.as_deref() {
            let listed = self.owned_voices.iter().find(|p| p.name == pack).is_some_and(|p| p.problem.is_none());
            let dir = self.here().join(VOICES_DIR).join(pack);
            if listed && is_plugin_dir(&dir) && check_voice(&dir).is_ok() {
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

    /// Aligns the voice, style and range choices with the newly active plugin after the pack changed: a kept
    /// choice the new plugin does not offer becomes the plugin's default (an unset voice plays the pack's). The
    /// explicit 1.02 choice stays.
    pub fn normalize_codriver(&mut self) {
        let Some(mods) = self.codriver_mods.clone() else { return };
        if self
            .codriver_voice
            .as_deref()
            .is_some_and(|v| v != crate::command::ORIGINAL_VOICE && !mods.voices.iter().any(|o| o == v))
        {
            self.codriver_voice = None;
        }
        if !mods.styles.contains(&self.codriver_style) {
            self.codriver_style =
                mods.styles.first().cloned().unwrap_or_else(|| rbr_sound::custom_codriver::DEFAULT_STYLE.into());
        }
        if !mods.ranges.contains(&self.codriver_ranges) {
            self.codriver_ranges =
                mods.ranges.first().cloned().unwrap_or_else(|| rbr_sound::custom_codriver::DEFAULT_RANGES.into());
        }
    }

    /// Starts importing the co-driver download `src` (a folder or archive) on a thread; real progress with a
    /// working Cancel, and nothing published when cancelled.
    pub fn import_codriver_from(&mut self, src: String) {
        if self.codriver_job.is_some() || self.child.is_some() {
            return;
        }
        let home = self.here();
        self.codriver_status = None;
        self.codriver_job = Some(Job::start(move |report, cancel| {
            report(&Progress { step: "Importing co-driver", done: 0, total: 0 });
            import(&home, Path::new(&src), cancel)
        }));
    }

    /// Separate folder and archive imports: cancelling one dialog never opens the other.
    pub fn import_codriver_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let idle = self.codriver_job.is_none() && self.child.is_none();
            if ui
                .add_enabled(idle, egui::Button::new("Import folder…"))
                .on_hover_text(
                    "Choose an extracted co-driver download, a MyPacenotes folder, or a whole RBR/RSF install to \
                     copy from (only read). ReRBR copies its voices, call styles, distance calls and stage notes \
                     into your files' folder; the source is not changed.",
                )
                .clicked()
            {
                if let Some(src) = install::pick_folder("The co-driver download's folder") {
                    self.import_codriver_from(src);
                }
            }
            if ui
                .add_enabled(idle, egui::Button::new("Import archive…"))
                .on_hover_text(
                    "Choose a downloaded .zip/.7z co-driver or note pack. ReRBR copies its voices, call styles, \
                     distance calls and stage notes into your files' folder; the download is not changed.",
                )
                .clicked()
            {
                if let Some(src) = crate::disc_install::pick_file("The co-driver download's .zip/.7z archive") {
                    self.import_codriver_from(src);
                }
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

    fn imported(home: &Path, src: &Path) -> Result<Vec<String>, String> {
        import(home, src, &AtomicBool::new(false))
    }

    fn set_mtime(path: &Path, secs: u64) {
        let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        fs::File::options().write(true).open(path).unwrap().set_modified(time).unwrap();
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
        let lines = imported(&home, &src).unwrap();
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
        let lines = imported(&home, &dir.join("download")).unwrap();
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
        let lines = imported(&home, &install).unwrap();
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
        let err = imported(&home, &src).unwrap_err();
        assert!(err.contains("no call styles"), "{err}");
        assert!(!home.join(VOICES_DIR).exists() || fs::read_dir(home.join(VOICES_DIR)).unwrap().count() == 0);
        // A notes-only pack imports through the same entry point.
        let notes = dir.join("mynotes");
        notes_source(&notes, "Tanner");
        let lines = imported(&home, &notes).unwrap();
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
        imported(&home, &dir.join("a")).unwrap();
        imported(&home, &dir.join("b")).unwrap();
        let (a, b) = (home.join(VOICES_DIR).join("a"), home.join(VOICES_DIR).join("b"));
        assert!(codriver::load(&a, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&0));
        // Without a PaceNote.ini the packs still default to their discovered voice.
        let voice = |dir: &Path| crate::command::CodriverMods::from_dir(dir.to_owned()).unwrap().voice(None);
        assert_eq!(voice(&a).as_deref(), Some("v"));
        assert_eq!(voice(&b).as_deref(), Some("v"));
        assert!(!codriver::load(&a, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&7));
        assert!(codriver::load(&b, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&7));
        assert!(!codriver::load(&b, "Rbr.ini", "Rbr.ini").unwrap().notes.contains_key(&0));
        // An existing pack is never overwritten: the same source again gets a new name.
        imported(&home, &dir.join("a")).unwrap();
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
        let lines = imported(&home, &archive).unwrap();
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
        let _ = imported(&home, &evil);
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
        let lines = imported(&home, &archive).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(find_note(&home.join(NOTES_DIR).join("tanner-notes"), &["Tanner"]).is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_parts_are_named_not_panics() {
        let dir = temp("parts");
        let home = dir.join("home");
        // Ranges without styles (or voices) fail with the missing part, instead of indexing an empty list.
        let ranges_only = dir.join("ranges-only");
        write(&ranges_only.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(&ranges_only.join("sounds/english/steve/50_1.ogg"), "OggS fake");
        let err = imported(&home, &ranges_only).unwrap_err();
        assert!(err.contains("no call styles"), "{err}");
        // Styles without voices name the voices.
        let styles_only = dir.join("styles-only");
        write(&styles_only.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(
            &styles_only.join("config/pacenotes/calls.ini"),
            "[PACENOTE::ONE_LEFT]\nid=0\nSounds=1\nSnd0=one_left.ogg\n",
        );
        let err = imported(&home, &styles_only).unwrap_err();
        assert!(err.contains("no voices"), "{err}");
        assert!(!home.join(VOICES_DIR).join("ranges-only").exists());
        assert!(!home.join(VOICES_DIR).join("styles-only").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn symlinks_are_neither_followed_nor_copied() {
        use std::os::unix::fs::symlink;
        let dir = temp("symlinks");
        let src = dir.join("download");
        plugin_source(&src);
        // A loop back to the source itself, and a file link to another pack's file: neither is followed.
        symlink(&src, src.join("loop")).unwrap();
        symlink(src.join("PaceNote.ini"), src.join("sounds/english/steve/linked.ogg")).unwrap();
        let home = dir.join("home");
        imported(&home, &src).unwrap();
        let pack = home.join(VOICES_DIR).join("download");
        assert!(pack.join("PaceNote.ini").is_file());
        assert!(!pack.join("loop").exists());
        assert!(!pack.join("sounds/english/steve/linked.ogg").exists());
        // A symlinked pack folder is not listed as a pack either.
        symlink(home.join(VOICES_DIR).join("download"), home.join(VOICES_DIR).join("ghost")).unwrap();
        assert_eq!(voice_packs(&home).iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["download"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn single_stage_sources_keep_their_stage() {
        let dir = temp("single-stage");
        let home = dir.join("home");
        let note = "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 100.0\nflag = 0\n";
        // A bare stage folder: its own name stays over its notes.
        let folder = dir.join("Tanner");
        write(&folder.join("tanner.ini"), note);
        imported(&home, &folder).unwrap();
        assert!(find_note(&home.join(NOTES_DIR).join("Tanner"), &["Tanner"]).is_some());
        // A single-stage archive: the stage folder is kept, not stripped as a wrapper.
        for (archive, make) in [("single.zip", true), ("single.7z", false)] {
            let path = dir.join(archive);
            if make {
                let file = File::create(&path).unwrap();
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default();
                zip.start_file("Tanner/tanner.ini", options).unwrap();
                std::io::Write::write_all(&mut zip, note.as_bytes()).unwrap();
                zip.finish().unwrap();
            } else {
                let mut out = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
                let name = "Tanner/tanner.ini".to_owned();
                out.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(&name), Some(note.as_bytes())).unwrap();
                out.finish().unwrap();
            }
            imported(&home, &path).unwrap();
            let pack = home.join(NOTES_DIR).join("single");
            assert!(find_note(&pack, &["Tanner"]).is_some(), "{archive}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn newest_and_default_order_survives_import() {
        let dir = temp("order");
        let src = dir.join("notes");
        // A normal file and NGPCarMenu's `_default` export: the winner is newest first, default-ness second.
        write(&src.join("Tanner/custom.ini"), "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 1.0\nflag = 0\n");
        write(
            &src.join("Tanner/stage_default.ini"),
            "[PACENOTES]\ncount = 1\n\n[P0]\ntype = 16\ndistance = 2.0\nflag = 0\n",
        );
        let winner =
            |dir: &Path| find_note(dir, &["Tanner"]).and_then(|p| p.file_name()?.to_str().map(str::to_owned)).unwrap();
        let home = dir.join("home");
        // The newer default wins, before and after the copy (timestamps travel with the files).
        set_mtime(&src.join("Tanner/custom.ini"), 1_700_000_000);
        set_mtime(&src.join("Tanner/stage_default.ini"), 1_700_000_100);
        assert_eq!(winner(&src), "stage_default.ini");
        imported(&home, &src).unwrap();
        assert_eq!(winner(&home.join(NOTES_DIR).join("notes")), "stage_default.ini");
        // And the newer custom file wins when it is newer.
        set_mtime(&src.join("Tanner/custom.ini"), 1_700_000_200);
        assert_eq!(winner(&src), "custom.ini");
        imported(&home, &src).unwrap();
        assert_eq!(winner(&home.join(NOTES_DIR).join("notes-2")), "custom.ini");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_combined_import_publishes_nothing() {
        let dir = temp("atomic");
        let home = dir.join("home");
        let voice = dir.join("voice");
        plugin_source(&voice);
        // Notes that read as a folder but validate as nothing: the voice must not publish alone.
        let notes = dir.join("notes");
        write(&notes.join("Tanner/broken.ini"), "[SETTINGS]\nfoo = bar\n");
        let err = import_both(&home, "combo", &AtomicBool::new(false), &voice, &notes).unwrap_err();
        assert!(err.contains("no stage notes"), "{err}");
        assert!(!home.join(VOICES_DIR).join("combo").exists());
        assert!(!home.join(NOTES_DIR).join("combo").exists());
        // A cancelled import publishes nothing either.
        let cancelled = AtomicBool::new(true);
        let err = import(&home, &voice, &cancelled).unwrap_err();
        assert!(err.ends_with("cancelled"), "{err}");
        assert!(!home.join(VOICES_DIR).join("voice").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A ZIP archive of the folder `base` (relative paths kept whole).
    fn make_zip(archive: &Path, base: &Path) {
        let file = File::create(archive).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (rel, path) in walk(base) {
            if path.is_dir() {
                continue;
            }
            zip.start_file(rel, options).unwrap();
            let bytes = fs::read(&path).unwrap();
            std::io::Write::write_all(&mut zip, &bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn wrapper_plugin_layouts_import() {
        let dir = temp("wrapper-layouts");
        let home = dir.join("home");
        // Folder sources: the plugin under a wrapper, as `Pacenote` or `Plugins/Pacenote`.
        for (case, plugin) in [("folder-pacenote", "wrapper/Pacenote"), ("folder-install", "wrapper/Plugins/Pacenote")]
        {
            let src = dir.join(case);
            plugin_source(&src.join(plugin));
            let lines = imported(&home, &src).unwrap();
            assert_eq!(lines.len(), 1, "{case}: {lines:?}");
            assert!(home.join(VOICES_DIR).join(case).join("PaceNote.ini").is_file(), "{case}");
        }
        // Archive sources: the same layouts, kept whole (the stage folder is never a wrapper).
        let zipped = dir.join("zipped-pacenote");
        plugin_source(&zipped.join("wrapper/Pacenote"));
        let archive = dir.join("zipped-pacenote.zip");
        make_zip(&archive, &zipped);
        let lines = imported(&home, &archive).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(home.join(VOICES_DIR).join("zipped-pacenote").join("PaceNote.ini").is_file());
        let install = dir.join("zipped-install");
        plugin_source(&install.join("wrapper/Plugins/Pacenote"));
        let archive = dir.join("zipped-install.7z");
        {
            let mut out = sevenz_rust2::ArchiveWriter::create(&archive).unwrap();
            for (rel, path) in walk(&install) {
                if path.is_dir() {
                    continue;
                }
                let bytes = fs::read(&path).unwrap();
                out.push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(&rel), Some(bytes.as_slice())).unwrap();
            }
            out.finish().unwrap();
        }
        let lines = imported(&home, &archive).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(home.join(VOICES_DIR).join("zipped-install").join("PaceNote.ini").is_file());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_voice_archive_with_an_unrelated_ini_keeps_the_voice() {
        let dir = temp("unrelated-ini");
        let home = dir.join("home");
        // A valid voice with an unrelated INI next to it: the voice imports alone, nothing pruned loudly.
        let src = dir.join("download");
        plugin_source(&src);
        write(&src.join("Readme.ini"), "[ABOUT]\ntext = hello\n");
        let archive = dir.join("voice.zip");
        make_zip(&archive, &src);
        let lines = imported(&home, &archive).unwrap();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(home.join(VOICES_DIR).join("voice").join("PaceNote.ini").is_file());
        assert!(!home.join(NOTES_DIR).join("voice").exists());
        // A valid voice with a real note and an unrelated INI: both publish, the note kept.
        notes_source(&src, "Tanner");
        let archive = dir.join("both.zip");
        make_zip(&archive, &src);
        let lines = imported(&home, &archive).unwrap();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(find_note(&home.join(NOTES_DIR).join("both"), &["Tanner"]).is_some());
        // A notes-only archive with no parseable notes still fails loudly and publishes nothing.
        let junk = dir.join("junk");
        write(&junk.join("Readme.ini"), "[ABOUT]\ntext = hello\n");
        let archive = dir.join("junk.zip");
        make_zip(&archive, &junk);
        let err = imported(&home, &archive).unwrap_err();
        assert!(err.contains("no stage notes"), "{err}");
        assert!(!home.join(NOTES_DIR).join("junk").exists());
        assert!(!home.join(VOICES_DIR).join("junk").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_cancel_after_staging_publishes_nothing() {
        let dir = temp("late-cancel");
        let home = dir.join("home");
        let src = dir.join("voice");
        plugin_source(&src);
        let cancel = AtomicBool::new(false);
        // The copy flips the flag after staging, as a Cancel pressed during validation would: the commit,
        // checked immediately before renaming, refuses.
        let err = import_voice_from(&home, "late", &cancel, |d, c| {
            let n = copy_voice(&src, d, c)?;
            c.store(true, Ordering::Relaxed);
            Ok(n)
        })
        .unwrap_err();
        assert!(err.ends_with("cancelled"), "{err}");
        assert!(!home.join(VOICES_DIR).join("late").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_publish_failure_rolls_the_voice_back() {
        let dir = temp("rollback");
        let home = dir.join("home");
        let no_cancel = AtomicBool::new(false);
        // A notes publish that cannot rename (its staging is gone): the just-published voice is removed again,
        // so a dual publish never leaves a lone voice behind.
        let voice = stage(&home, VOICES_DIR, "pair").unwrap();
        let notes = stage(&home, NOTES_DIR, "pair").unwrap();
        write(&voice.staged.join("PaceNote.ini"), "[SETTINGS]\nsounds=english/steve\n");
        fs::remove_dir_all(&notes.staged).unwrap();
        let err = publish_pair(&voice, &notes, &no_cancel).unwrap_err();
        assert!(!home.join(VOICES_DIR).join("pair").exists(), "{err}");
        assert!(!home.join(NOTES_DIR).join("pair").exists(), "{err}");
        // A cancel checked immediately before the commit publishes nothing either.
        let staged = stage(&home, VOICES_DIR, "late").unwrap();
        let cancel = AtomicBool::new(true);
        let err = publish(&staged, &cancel).unwrap_err();
        assert!(err.ends_with("cancelled"), "{err}");
        assert!(!home.join(VOICES_DIR).join("late").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_packs_list_their_validation_problem() {
        let dir = temp("corrupt-list");
        let home = dir.join("home");
        let broken = home.join(VOICES_DIR).join("broken");
        // A style naming a package file that is not there: the lists see no missing part, but loading dangles.
        write(&broken.join("config/pacenotes/Rbr.ini"), "[PACKAGE::RBR]\nfile=calls.ini\n");
        write(&broken.join("config/ranges/Rbr.ini"), "[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n");
        write(&broken.join("sounds/english/steve/50_1.ogg"), "OggS fake");
        let packs = voice_packs(&home);
        assert_eq!(packs.len(), 1);
        let problem = packs[0].problem.as_deref().expect("a dangling pack lists its problem");
        assert!(problem.contains("calls.ini"), "{problem}");
        // The same reason activation refuses with: the list and the activation agree.
        let refused = check_voice(&broken).unwrap_err();
        assert!(refused.ends_with(problem), "{refused}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_voice_follows_the_installed_spelling() {
        let dir = temp("default-voice");
        let src = dir.join("pack");
        plugin_source(&src);
        let mods = crate::command::CodriverMods::from_dir(src.clone()).unwrap();
        assert_eq!(mods.default_voice.as_deref(), Some("english/steve"));
        // A differently-cased setting with the other separator still resolves to the installed voice.
        write(&src.join("PaceNote.ini"), "[SETTINGS]\nsounds=ENGLISH\\STEVE\n");
        let mods = crate::command::CodriverMods::from_dir(src.clone()).unwrap();
        assert_eq!(mods.default_voice.as_deref(), Some("english/steve"));
        // A setting naming no installed voice falls back to the first installed voice.
        write(&src.join("PaceNote.ini"), "[SETTINGS]\nsounds=ghost/voice\n");
        let mods = crate::command::CodriverMods::from_dir(src.clone()).unwrap();
        assert_eq!(mods.default_voice.as_deref(), Some("english/steve"));
        // Without a setting the first installed voice speaks.
        fs::remove_file(src.join("PaceNote.ini")).unwrap();
        let mods = crate::command::CodriverMods::from_dir(src).unwrap();
        assert_eq!(mods.default_voice.as_deref(), Some("english/steve"));
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
        imported(&home, &src).unwrap();
        let voices = voice_packs(&home);
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].problem, None);
        let progress = Progress { step: "Importing", done: 0, total: 0 };
        assert_eq!(progress.line(), "Importing…");
        let _ = AtomicBool::new(false);
        fs::remove_dir_all(&dir).unwrap();
    }
}

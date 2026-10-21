//! The co-driver of an RSF install's Pacenote plugin (`Plugins/Pacenote`, `notes/formats.md` "Pacenote plugin"): the
//! voices, the call styles and the distance calls a co-driver mod installs.
//!
//! - `PaceNote.ini` `[SETTINGS]`: `sounds` the voice, a folder under `sounds/` (`english\male\steve`, `Janne Laahanen
//!   EnNu A`), `muteReplayPacenotes`;
//! - a style, `config/pacenotes/<style>.ini` (`Rbr.ini`, `Numeric.ini`, `Descriptive.ini`): `[PACKAGE::<name>]`
//!   sections whose `file`, `file0`, `file1`, ... name package files, whose `[CATEGORY::<name>]` sections name category
//!   files, each path relative to the file naming it; the category files define the calls, `[PACENOTE::<NAME>]` with
//!   `id`, `Sounds`, `Snd<n>` (`[<label>::]<file>.ogg` in the voice's folder), `link` (the call said for a gap under
//!   about 10 m: `INTO`, `ONTO`, none) and `standard`, `column` for the plugin's editor. The style decides how a turn
//!   is counted: the same id is "six left" in one and "one left" in another. A call without `id=` is one of the
//!   plugin's own ([`super::pacenotes::NAMED`]) or a 1.02 modifier;
//! - a range set, `config/ranges/<set>.ini`, the same tree with `[RANGE::<metres>]` calls.
//!
//! A call without a sound file plays 1.02's sample of its name (`CoDriver.ini`, `Numbers.ini`). The definition files
//! carry `;` comments after values and section names.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use super::pacenotes::NAMED;
use crate::ini::{self, Section};
use crate::stage_files;

/// One call: its name, its takes (file names in the voice's folder) and its link call's name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Call {
    pub name: String,
    pub sounds: Vec<String>,
    pub link: Option<String>,
}

/// A style's calls by id and a range set's distance calls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Calls {
    pub notes: HashMap<u32, Call>,
    /// The calls without an id that are no plugin call: the 1.02 modifiers (`NARROWS`, `WIDEOUT`), by name.
    pub named: HashMap<String, Call>,
    /// The distance calls by metres.
    pub ranges: BTreeMap<u32, Call>,
}

impl Calls {
    /// The call named `name`: by its id (a definition's own name, or a plugin call), else a modifier.
    pub fn by_name(&self, name: &str) -> Option<&Call> {
        self.notes.values().find(|c| c.name.eq_ignore_ascii_case(name)).or_else(|| self.named.get(name))
    }
}

/// `PaceNote.ini`'s settings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// The voice, `sounds` (a path under `sounds/`, `\` or `/`).
    pub voice: Option<String>,
    /// `muteReplayPacenotes`.
    pub mute_replay: bool,
}

/// The plugin's folder of an RSF install, when it has one.
pub fn plugin_dir(root: &Path) -> Option<PathBuf> {
    stage_files::in_dir(root, "Plugins\\Pacenote").filter(|p| p.is_dir())
}

/// `PaceNote.ini` of the plugin's folder `dir`.
pub fn settings(dir: &Path) -> Settings {
    let sections = read_ini(&dir.join("PaceNote.ini")).unwrap_or_default();
    let get = |key: &str| sections.iter().find_map(|s| s.get(key)).map(|v| v.trim().to_owned());
    Settings {
        voice: get("sounds").filter(|v| !v.is_empty()),
        mute_replay: get("muteReplayPacenotes").is_some_and(|v| v == "1"),
    }
}

/// The voices under `sounds/`: the folders holding `.ogg` files, as paths from `sounds/` with `/`, sorted.
pub fn voices(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut has_ogg = false;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, &if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") }, out);
            } else if name.to_ascii_lowercase().ends_with(".ogg") {
                has_ogg = true;
            }
        }
        if has_ogg && !prefix.is_empty() {
            out.push(prefix.to_owned());
        }
    }
    let mut out = Vec::new();
    if let Some(sounds) = stage_files::in_dir(dir, "sounds") {
        walk(&sounds, "", &mut out);
    }
    out.sort();
    out
}

/// The voice `voice`'s folder (a path under `sounds/`, matched ignoring case).
pub fn voice_dir(dir: &Path, voice: &str) -> Option<PathBuf> {
    stage_files::in_dir(dir, &format!("sounds\\{voice}")).filter(|p| p.is_dir())
}

/// The styles: the `.ini` files of `config/pacenotes`, sorted.
pub fn styles(dir: &Path) -> Vec<String> {
    ini_names(&dir.join("config").join("pacenotes"))
}

/// The range sets: the `.ini` files of `config/ranges`, sorted.
pub fn range_sets(dir: &Path) -> Vec<String> {
    ini_names(&dir.join("config").join("ranges"))
}

fn ini_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.to_ascii_lowercase().ends_with(".ini"))
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|n| n.to_ascii_lowercase());
    names
}

/// The calls of style `style` and range set `ranges` (file names in `config/pacenotes` and `config/ranges`). A call
/// defined twice keeps its first definition.
pub fn load(dir: &Path, style: &str, ranges: &str) -> Result<Calls, String> {
    let mut calls = Calls::default();
    let config = dir.join("config");
    let style_file = stage_files::in_dir(&config.join("pacenotes"), style)
        .ok_or_else(|| format!("{}: no style {style}", config.join("pacenotes").display()))?;
    read_tree(&style_file, &mut calls, 0)?;
    let ranges_file = stage_files::in_dir(&config.join("ranges"), ranges)
        .ok_or_else(|| format!("{}: no range set {ranges}", config.join("ranges").display()))?;
    read_tree(&ranges_file, &mut calls, 0)?;
    Ok(calls)
}

/// Reads one definition file and the files it names, depth first in file order.
fn read_tree(path: &Path, calls: &mut Calls, depth: usize) -> Result<(), String> {
    if depth > 8 {
        return Err(format!("{}: files nested too deep", path.display()));
    }
    let sections = read_ini(path)?;
    let here = path.parent().unwrap_or(Path::new("."));
    for section in &sections {
        let (kind, name) = section.name.split_once("::").unwrap_or(("", section.name.as_str()));
        let name = name.trim();
        match kind.trim().to_ascii_uppercase().as_str() {
            "PACKAGE" | "CATEGORY" => {
                for (key, value) in &section.entries {
                    if key.to_ascii_lowercase().starts_with("file") && !value.is_empty() {
                        let file = stage_files::in_dir(here, value)
                            .ok_or_else(|| format!("{}: [{}] {key}: no {value}", path.display(), section.name))?;
                        read_tree(&file, calls, depth + 1)?;
                    }
                }
            }
            "PACENOTE" => {
                let call = call(name, section);
                match section.get("id").and_then(|v| v.parse::<u32>().ok()) {
                    Some(id) => {
                        calls.notes.entry(id).or_insert(call);
                    }
                    None => match NAMED.iter().find(|(n, _)| *n == name) {
                        Some(&(_, id)) => {
                            calls.notes.entry(id).or_insert(call);
                        }
                        None => {
                            calls.named.entry(name.to_owned()).or_insert(call);
                        }
                    },
                }
            }
            "RANGE" => {
                if let Ok(metres) = name.parse::<u32>() {
                    calls.ranges.entry(metres).or_insert_with(|| call(name, section));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// A `[PACENOTE::...]` or `[RANGE::...]` section's call.
fn call(name: &str, section: &Section) -> Call {
    let count = section.get("Sounds").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let sounds = (0..count)
        .filter_map(|k| section.get(&format!("Snd{k}")))
        .map(|v| v.rsplit("::").next().unwrap_or(v).trim().to_owned())
        .filter(|v| !v.is_empty())
        .collect();
    let link = section.get("link").map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    Call { name: name.to_owned(), sounds, link }
}

/// A definition file's sections, with the `;` comments after values cut off.
fn read_ini(path: &Path) -> Result<Vec<Section>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut sections = ini::parse(&String::from_utf8_lossy(&bytes)).map_err(|e| format!("{}: {e}", path.display()))?;
    for section in &mut sections {
        for (_, value) in &mut section.entries {
            if let Some((kept, _)) = value.split_once(';') {
                *value = kept.trim().to_owned();
            }
        }
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, path: &str, text: &str) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    #[test]
    fn a_style_reads_its_packages_categories_and_ranges() {
        let dir = std::env::temp_dir().join(format!("rbr-codriver-{}", std::process::id()));
        write(
            &dir,
            "config/pacenotes/Rbr.ini",
            "[PACKAGE::RBR]\nfile0=packages\\Corners.ini\nfile1=packages\\Links.ini\n",
        );
        write(&dir, "config/pacenotes/packages/Corners.ini", "[CATEGORY::CORNERS]\r\nfile=corners\\Numeric.ini\r\n");
        write(
            &dir,
            "config/pacenotes/packages/corners/Numeric.ini",
            "[PACENOTE::ONE_LEFT]\nid=0\nSounds=2\nSnd0=one_left.ogg\nSnd1=ONE::one_left2.ogg\nlink=INTO\n\
             [PACENOTE::OTHER]\nid=0\nSounds=1\nSnd0=x.ogg\n\
             [PACENOTE::FASTLEFT]\nid=4\t\t\t; legacy FASTLEFT\n",
        );
        write(
            &dir,
            "config/pacenotes/packages/Links.ini",
            "[PACENOTE::INTO]\nSounds=1\nSnd0=into1.ogg\n[PACENOTE::LONG]     ; legacy\nSounds=1\nSnd0=long1.ogg\nlink=\n\
             [PACENOTE::NARROWS]\nSounds=1\nSnd0=narrows1.ogg\n",
        );
        write(&dir, "config/ranges/Rbr.ini", "[PACKAGE::RBR]\nfile=packages\\Rbr.ini\n");
        write(
            &dir,
            "config/ranges/packages/Rbr.ini",
            "[RANGE::30]             ; legacy sound\n[RANGE::50]\nSounds=1\nSnd0=50_1.ogg\n",
        );
        let calls = load(&dir, "rbr.ini", "Rbr.ini").unwrap();
        let one = &calls.notes[&0];
        assert_eq!((one.name.as_str(), one.link.as_deref()), ("ONE_LEFT", Some("INTO")));
        assert_eq!(one.sounds, ["one_left.ogg", "one_left2.ogg"]);
        assert!(calls.notes[&4].sounds.is_empty());
        assert_eq!(calls.notes[&super::super::pacenotes::id::LONG].link, None);
        assert_eq!(calls.by_name("INTO").unwrap().sounds, ["into1.ogg"]);
        assert_eq!(calls.by_name("NARROWS").unwrap().sounds, ["narrows1.ogg"]);
        assert_eq!(calls.ranges.keys().copied().collect::<Vec<_>>(), [30, 50]);
        assert_eq!(calls.ranges[&50].sounds, ["50_1.ogg"]);
        assert_eq!(styles(&dir), ["Rbr.ini"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

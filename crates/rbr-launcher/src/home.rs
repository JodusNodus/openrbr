//! Where the player's files go: the settings, saves, records, ghosts, setups and added cars. The platform's user data
//! folder (macOS `~/Library/Application Support/ReRBR`, Linux `$XDG_DATA_HOME/rerbr`, default
//! `~/.local/share/rerbr`), or `$RERBR_HOME`. Not next to the launcher: inside a downloaded app bundle that is read
//! only. The launcher passes its files' paths to the viewer and `rbr-standalone`.
//!
//! Earlier versions wrote them next to the launcher: at the first start without the folder, they are copied over once
//! (`migrate`), and stay where they were.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::SETTINGS_FILE;

/// What earlier versions wrote next to the launcher: the settings file, the 1.02 saves and setup slots, NGP's
/// records and ghosts, the saved setups (`Physics/<car>/Setups`), the added cars and the 1.02 ghosts.
const MOVED: [&str; 6] =
    [SETTINGS_FILE, "SavedGames", "NGP", "Physics", crate::car_packs::DIR, crate::ghosts::GHOST_DIR];

/// The player's folder: `$RERBR_HOME`, else the platform's user data folder, else (no `HOME`) `fallback`.
pub fn dir(fallback: &Path) -> PathBuf {
    folder(|key| std::env::var_os(key), cfg!(target_os = "macos")).unwrap_or_else(|| fallback.to_path_buf())
}

fn folder(env: impl Fn(&str) -> Option<OsString>, macos: bool) -> Option<PathBuf> {
    let var = |key| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = var("RERBR_HOME") {
        return Some(dir);
    }
    let home = var("HOME");
    if macos {
        return home.map(|h| h.join("Library/Application Support/ReRBR"));
    }
    // The XDG spec ignores a relative XDG_DATA_HOME.
    let data = var("XDG_DATA_HOME").filter(|d| d.is_absolute()).or_else(|| home.map(|h| h.join(".local/share")));
    data.map(|d| d.join("rerbr"))
}

/// Copies the files of `MOVED` from `old` (the launcher's folder) into `new` when `new` does not exist yet and `old`
/// has a settings file; the line saying so, if it did. Copied whole into a sibling folder first and then renamed, so
/// a failed copy leaves no `new` and is tried again at the next start.
pub fn migrate(old: &Path, new: &Path) -> io::Result<Option<String>> {
    if new.exists() || !old.join(SETTINGS_FILE).is_file() || same(old, new) {
        return Ok(None);
    }
    let part = new.with_extension("part");
    if part.exists() {
        fs::remove_dir_all(&part)?;
    }
    fs::create_dir_all(&part)?;
    for name in MOVED {
        let from = old.join(name);
        if from.is_dir() {
            copy_tree(&from, &part.join(name))?;
        } else if from.is_file() {
            fs::copy(&from, part.join(name))?;
        }
    }
    fs::rename(&part, new)?;
    Ok(Some(format!("Copied your settings and saves from {} to {}", old.display(), new.display())))
}

/// Whether `a` and `b` are the same existing folder (`RERBR_HOME` set to the launcher's).
fn same(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn the_folder_is_the_platforms_or_rerbr_home() {
        let home = [("HOME", "/h")];
        assert_eq!(folder(env(&home), true), Some(PathBuf::from("/h/Library/Application Support/ReRBR")));
        assert_eq!(folder(env(&home), false), Some(PathBuf::from("/h/.local/share/rerbr")));
        let xdg = [("HOME", "/h"), ("XDG_DATA_HOME", "/x")];
        assert_eq!(folder(env(&xdg), false), Some(PathBuf::from("/x/rerbr")));
        let relative = [("HOME", "/h"), ("XDG_DATA_HOME", "x")];
        assert_eq!(folder(env(&relative), false), Some(PathBuf::from("/h/.local/share/rerbr")));
        let set = [("HOME", "/h"), ("RERBR_HOME", "/r")];
        assert_eq!(folder(env(&set), true), Some(PathBuf::from("/r")));
        let empty = [("HOME", "/h"), ("RERBR_HOME", "")];
        assert_eq!(folder(env(&empty), false), Some(PathBuf::from("/h/.local/share/rerbr")));
        assert_eq!(folder(env(&[]), false), None);
    }

    #[test]
    fn earlier_files_are_copied_once() {
        let root = std::env::temp_dir().join(format!("rerbr-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (old, new) = (root.join("launcher"), root.join("data/ReRBR"));
        let write = |path: &Path, text: &str| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        // Without a settings file next to the launcher there is nothing to copy.
        write(&old.join("NGP/SavedGames/pfresult.rst"), "records");
        assert_eq!(migrate(&old, &new).unwrap(), None);
        assert!(!new.exists());

        write(&old.join(SETTINGS_FILE), "[launcher]\n");
        write(&old.join("Physics/s_i2000/Setups/mine.lsp"), "setup");
        write(&old.join("cars/rsfdata/cache/cars.json"), "[]");
        write(&old.join("viewer"), "program");
        let line = migrate(&old, &new).unwrap().expect("copied");
        assert!(line.contains(&new.display().to_string()), "{line}");
        let read = |path: &str| fs::read_to_string(new.join(path)).unwrap();
        assert_eq!(read(SETTINGS_FILE), "[launcher]\n");
        assert_eq!(read("NGP/SavedGames/pfresult.rst"), "records");
        assert_eq!(read("Physics/s_i2000/Setups/mine.lsp"), "setup");
        assert_eq!(read("cars/rsfdata/cache/cars.json"), "[]");
        assert!(!new.join("viewer").exists() && !new.with_extension("part").exists());
        // Copied, not moved; and once: the folder now exists.
        assert!(old.join(SETTINGS_FILE).is_file());
        write(&old.join(SETTINGS_FILE), "[launcher]\ncar = COROLLA\n");
        assert_eq!(migrate(&old, &new).unwrap(), None);
        assert_eq!(read(SETTINGS_FILE), "[launcher]\n");
        fs::remove_dir_all(&root).unwrap();
    }
}

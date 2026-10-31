//! The RBR install folder the Settings page sets (`install_folder`): RBR 1.02 with NGP (RallySimFans or not), read in
//! place and never written, but by "Download NGP…" (`gvrc`). At the first start the launcher offers the first usual install place whose `physics.rbz`
//! is NGP's ([`detect`]). It looks only at fixed places in the folder:
//! - the game data and the 1.02 patch's files: the folder itself (an installed game holds the extracted layout's
//!   `Root\` files at its top, `rbr_formats::data`, and the patched exe, scripts and shaders in the retail ones'
//!   place);
//! - NGP's physics: `physics.rbz`, when it is NGP's (its `Physics/physics.lsp` sets `PhysicsRevision_NGP`; NGP and
//!   RallySimFans put it in the retail archive's place);
//! - RallySimFans: the folder, when it has `rsfdata/` (`rsfdata/cache`, `rsfdata/cars`, `Maps/<id>-<name>/`, `Cars/`).
//!
//! `--data` (or `RBR_DATA`) and `RBR_PATCH` override the first, and the Settings page's NGP and RSF fields (unsaved,
//! `RBR_NGP_PHYSICS` and `RBR_RSF`) the other two.

use std::io::Read;
use std::path::{Path, PathBuf};

use rbr_formats::rbz;

/// What the install folder gives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folder {
    /// The game data and the 1.02 patch's files.
    pub game: Option<PathBuf>,
    /// NGP's `physics.rbz`.
    pub ngp_physics: Option<PathBuf>,
    /// The RallySimFans folder.
    pub rsf_root: Option<PathBuf>,
}

impl Folder {
    /// The folder `dir`'s parts (none for an empty `dir`).
    pub fn read(dir: &str) -> Folder {
        let dir = dir.trim();
        if dir.is_empty() {
            return Folder::default();
        }
        let dir = PathBuf::from(dir);
        let archive = dir.join("physics.rbz");
        Folder {
            ngp_physics: is_ngp_archive(&archive).then_some(archive),
            rsf_root: dir.join("rsfdata").is_dir().then(|| dir.clone()),
            game: Some(dir),
        }
    }
}

/// Why the game folder `dir` did not load, in the player's words (the error itself goes to the log).
pub fn problem(dir: &str) -> String {
    let dir = Path::new(dir.trim());
    if !dir.is_dir() {
        return "There is no such folder.".into();
    }
    if !dir.join("Cars").is_dir() || !dir.join("Maps").is_dir() {
        return "This is not a Richard Burns Rally folder. Choose the one with RichardBurnsRally_SSE.exe in it, or \
                set up the game from the disc."
            .into();
    }
    "ReRBR can't read the game in this folder. It needs Richard Burns Rally with the official 1.02 patch: install \
     the patch into it, or set up the game from the disc."
        .into()
}

/// The folder name RBR and the RallySimFans installer use.
const RBR: &str = "Richard Burns Rally";

/// The first of the usual install places ([`places`]) whose `physics.rbz` is NGP's: only read.
pub fn detect() -> Option<PathBuf> {
    let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    places(std::env::consts::OS, env).into_iter().find(|dir| is_ngp_archive(&dir.join("physics.rbz")))
}

/// Where RBR is usually installed on `os` (`std::env::consts::OS`) with the environment `env`: on Windows
/// `C:\Richard Burns Rally`, `C:\Games\Richard Burns Rally` and the `Richard Burns Rally` of `%ProgramFiles%` and
/// `%ProgramFiles(x86)%`; elsewhere `~/Games/Richard Burns Rally`, and on Linux also the Wine prefix's, Lutris' and
/// Bottles' prefixes' (`~/.wine`, `~/Games/*/`, `~/.local/share/bottles/bottles/*/`) `drive_c/Richard Burns Rally`.
fn places(os: &str, env: impl Fn(&str) -> Option<PathBuf>) -> Vec<PathBuf> {
    if os == "windows" {
        let fixed = [r"C:\Richard Burns Rally", r"C:\Games\Richard Burns Rally"].map(PathBuf::from);
        let program_files =
            ["ProgramFiles", "ProgramFiles(x86)"].into_iter().filter_map(|key| Some(env(key)?.join(RBR)));
        return fixed.into_iter().chain(program_files).collect();
    }
    let Some(home) = env("HOME") else { return Vec::new() };
    let mut places = vec![home.join("Games").join(RBR)];
    if os == "linux" {
        places.push(home.join(".wine/drive_c").join(RBR));
        for prefixes in [home.join("Games"), home.join(".local/share/bottles/bottles")] {
            let mut found: Vec<PathBuf> = std::fs::read_dir(prefixes)
                .into_iter()
                .flatten()
                .filter_map(|e| Some(e.ok()?.path().join("drive_c").join(RBR)))
                .collect();
            found.sort();
            places.extend(found);
        }
    }
    places
}

/// The `Physics/physics.lsp` of the `physics.rbz` at `path`.
fn physics_lsp(path: &Path) -> Option<String> {
    let mut zip = rbz::open(path).ok()?;
    let mut text = String::new();
    rbz::by_name(&mut zip, "Physics/physics.lsp").ok()?.read_to_string(&mut text).ok()?;
    Some(text)
}

/// Whether the `physics.rbz` at `path` is NGP's.
pub fn is_ngp_archive(path: &Path) -> bool {
    physics_lsp(path).is_some_and(|text| crate::ngp_install::has_ngp_revision(text.as_bytes()))
}

/// The NGP revision the `physics.rbz` at `path` sets (`PhysicsRevision_NGP 778`), if it is NGP's.
pub fn ngp_revision(path: &Path) -> Option<u32> {
    revision(&physics_lsp(path)?)
}

/// The number after `PhysicsRevision_NGP` in a `physics.lsp`.
fn revision(lsp: &str) -> Option<u32> {
    let mut words = lsp.split_whitespace().skip_while(|w| *w != "PhysicsRevision_NGP");
    words.nth(1)?.parse().ok()
}

/// The Physics box's name for NGP with the archive's revision: the releases' version numbers (7.4.778 is NGP's
/// download, 7.5.779 RallySimFans', `notes/ngp.md`), else the bare revision.
pub fn ngp_label(revision: Option<u32>) -> String {
    match revision {
        Some(778) => "NGP 7.4.778".to_owned(),
        Some(779) => "NGP 7.5.779".to_owned(),
        Some(n) => format!("NGP revision {n}"),
        None => "NGP".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_ngp_or_rsf_gives_the_game_only() {
        let dir = crate::test_dir("install-folder");
        std::fs::create_dir_all(dir.join("rsfdata")).unwrap();
        let folder = Folder::read(&dir.display().to_string());
        assert_eq!(folder.game.as_deref(), Some(dir.as_path()));
        assert_eq!(folder.ngp_physics, None);
        assert_eq!(folder.rsf_root.as_deref(), Some(dir.as_path()));
        std::fs::remove_dir(dir.join("rsfdata")).unwrap();
        assert_eq!(Folder::read(&dir.display().to_string()).rsf_root, None);
        assert_eq!(Folder::read(" "), Folder::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder that does not load is told apart: missing, not the game, or the game without what ReRBR needs.
    #[test]
    fn a_folder_that_does_not_load_says_why() {
        let root = crate::test_dir("install-problem");
        let dir = root.join("game");
        let text = |dir: &Path| problem(&dir.display().to_string());
        assert!(text(&dir).starts_with("There is no such folder"));
        std::fs::create_dir_all(dir.join("Cars")).unwrap();
        assert!(text(&dir).starts_with("This is not a Richard Burns Rally folder"));
        std::fs::create_dir_all(dir.join("Maps")).unwrap();
        assert!(text(&dir).contains("1.02 patch"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn env<'a>(vars: &'a [(&str, &Path)]) -> impl Fn(&str) -> Option<PathBuf> + 'a {
        move |key| vars.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_path_buf())
    }

    /// The usual places: the Lutris and Bottles prefixes' among them, and on Windows Program Files'.
    #[test]
    fn the_usual_places_include_the_prefixes() {
        let home = crate::test_dir("install-places");
        let lutris = home.join("Games/rbr/drive_c").join(RBR);
        let bottle = home.join(".local/share/bottles/bottles/RSF/drive_c").join(RBR);
        std::fs::create_dir_all(&lutris).unwrap();
        std::fs::create_dir_all(&bottle).unwrap();
        let wine = home.join(".wine/drive_c").join(RBR);
        let with_home = [("HOME", home.as_path())];
        assert_eq!(places("linux", env(&with_home)), [home.join("Games").join(RBR), wine, lutris, bottle]);
        assert_eq!(places("macos", env(&with_home)), [home.join("Games").join(RBR)]);
        let fixed = [PathBuf::from(r"C:\Richard Burns Rally"), PathBuf::from(r"C:\Games\Richard Burns Rally")];
        assert_eq!(places("windows", env(&with_home)), fixed);
        let program_files =
            [("ProgramFiles", Path::new(r"D:\Programs")), ("ProgramFiles(x86)", Path::new(r"D:\Programs (x86)"))];
        let windows =
            [fixed.to_vec(), vec![Path::new(r"D:\Programs").join(RBR), Path::new(r"D:\Programs (x86)").join(RBR)]];
        assert_eq!(places("windows", env(&program_files)), windows.concat());
        assert!(places("linux", env(&[])).is_empty());
        std::fs::remove_dir_all(&home).unwrap();
    }

    /// NGP's archive is told from the retail one by its `physics.lsp` (`RBR_DATA`, `RBR_NGP_PHYSICS`).
    #[test]
    fn ngps_archive_is_told_from_the_retail_one() {
        let (Some(data), Some(ngp)) = (rbr_formats::data::Data::from_env(), std::env::var_os("RBR_NGP_PHYSICS")) else {
            eprintln!("skipping: RBR_DATA or RBR_NGP_PHYSICS not set");
            return;
        };
        assert!(!is_ngp_archive(&data.resolve("Root/physics.rbz").unwrap()));
        assert!(is_ngp_archive(Path::new(&ngp)));
        assert_eq!(ngp_revision(&data.resolve("Root/physics.rbz").unwrap()), None);
        assert_eq!(ngp_revision(Path::new(&ngp)), Some(778));
    }

    /// The Physics box names the archive's revision: RallySimFans' archive is 7.5.779.
    #[test]
    fn the_label_shows_the_archives_revision() {
        let lsp = |rev: &str| format!("  vecGravity 0 0 -9.81\r\n\r\n  PhysicsRevision_NGP {rev}\r\n\r\n  Flags\r\n");
        assert_eq!(ngp_label(revision(&lsp("778"))), "NGP 7.4.778");
        assert_eq!(ngp_label(revision(&lsp("779"))), "NGP 7.5.779");
        assert_eq!(ngp_label(revision(&lsp("780"))), "NGP revision 780");
        assert_eq!(ngp_label(revision("  Flags\r\n")), "NGP");
    }
}

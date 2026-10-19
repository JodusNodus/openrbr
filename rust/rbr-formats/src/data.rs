//! Data lookup over the retail install, with an optional 1.02 patch overlay.
//!
//! `RBR_DATA` is the extracted retail 1.0 install. `RBR_PATCH` (optional) is the 1.02 patch's
//! `files/Main_group` folder (`original/patch-1.02/files/Main_group`). The patch replaces some files
//! (the car shaders in `Fog/`, the executables), so a file the patch has wins over the install's, per
//! file. Names are DOS paths matched ignoring case, as in [`cars::resolve`]. See `notes/formats/shaders.md`.
//!
//! A car from outside the install (an RSF car, [`crate::rsf`]) takes one `Cars.ini` slot ([`Data::with_car`]): its
//! entry replaces the slot's in [`Data::car_slots`], and `Cars\` paths the install lacks resolve under its root. Its
//! skin ([`Data::with_skin`]) holds files over the model folder's.

use crate::cars;
use std::path::{Path, PathBuf};

/// The install root and the patch overlay, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Data {
    install: PathBuf,
    patch: Option<PathBuf>,
    car: Option<(PathBuf, cars::CarEntry)>,
    /// The car's skin folder (`Cars/<model>/skins/<skin>/`).
    skin: Option<PathBuf>,
}

impl Data {
    pub fn new(install: impl Into<PathBuf>, patch: Option<PathBuf>) -> Data {
        Data { install: install.into(), patch, car: None, skin: None }
    }

    /// The data with car `entry` in its slot (`entry.index`), its files under `root` (the RSF install).
    pub fn with_car(self, root: PathBuf, entry: cars::CarEntry) -> Data {
        Data { car: Some((root, entry)), ..self }
    }

    /// The data with the [`Data::with_car`] car's skin at `dir`: a file the skin holds at the same path below it
    /// (`textures/<name>.dds`, `<model>_shader_settings_clear.ini`) replaces the model folder's.
    pub fn with_skin(self, dir: PathBuf) -> Data {
        Data { skin: Some(dir), ..self }
    }

    /// `Cars.ini`'s `entries` with [`Data::with_car`]'s entry in its slot.
    pub fn car_slots(&self, mut entries: Vec<cars::CarEntry>) -> Vec<cars::CarEntry> {
        if let Some((_, car)) = &self.car {
            entries.retain(|e| e.index != car.index);
            entries.push(car.clone());
            entries.sort_by_key(|e| e.index);
        }
        entries
    }

    /// The data named by `RBR_DATA` (and `RBR_PATCH`, if set). `None` when `RBR_DATA` is not set.
    pub fn from_env() -> Option<Data> {
        let install = std::env::var_os("RBR_DATA")?;
        Some(Data::new(install, std::env::var_os("RBR_PATCH").map(PathBuf::from)))
    }

    /// The install root.
    pub fn install(&self) -> &Path {
        &self.install
    }

    /// The patch overlay's root, if one is set.
    pub fn patch(&self) -> Option<&Path> {
        self.patch.as_deref()
    }

    /// The file for `dos_path`: the car's skin's when it has one, else the patch's when the patch has one, else the
    /// install's, else the car's root's. The install's `Root\` path (the extracted retail layout's folder of the game
    /// folder's own files) is also looked up without it, as an installed game holds those files at its top.
    pub fn resolve(&self, dos_path: &str) -> Option<PathBuf> {
        let top = dos_path.strip_prefix("Root\\").or_else(|| dos_path.strip_prefix("Root/"));
        self.skin_file(dos_path)
            .or_else(|| self.patch.as_deref().and_then(|patch| cars::resolve(patch, dos_path)))
            .or_else(|| cars::resolve(&self.install, dos_path))
            .or_else(|| cars::resolve(&self.install, top?))
            .or_else(|| {
                let (root, _) = self.car.as_ref()?;
                let first = dos_path.split(['\\', '/']).find(|c| !c.is_empty())?;
                first.eq_ignore_ascii_case("Cars").then(|| cars::resolve(root, dos_path))?
            })
    }

    /// The sound archive `Root\audio.dat`, else `audio.dat.disabled`, as a RallySimFans install renames it (read only;
    /// RSF plays FMOD banks instead).
    pub fn audio_dat(&self) -> Option<PathBuf> {
        self.resolve("Root\\audio.dat").or_else(|| self.resolve("Root\\audio.dat.disabled"))
    }

    /// The skin's file for `dos_path`, a path in the car's model folder.
    fn skin_file(&self, dos_path: &str) -> Option<PathBuf> {
        let (skin, (_, car)) = (self.skin.as_deref()?, self.car.as_ref()?);
        let model = components(&car.file_name);
        let folder = &model[..model.len().checked_sub(1)?];
        let path = components(dos_path);
        let inside = path.len() > folder.len() && path.iter().zip(folder).all(|(a, b)| a.eq_ignore_ascii_case(b));
        inside.then(|| cars::resolve(skin, &path[folder.len()..].join("\\")))?
    }
}

/// A DOS path's components.
fn components(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|c| !c.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory for one test, with a file at each `(path, contents)` pair.
    fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rbr-data-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, contents).expect("write");
        }
        root
    }

    #[test]
    fn the_patch_file_wins_per_file_and_matches_case() {
        let install = tree(
            "install",
            &[("VertexShaders/fog/CarGlass.vso", "retail"), ("VertexShaders/fog/CarTexture.vso", "retail")],
        );
        let patch = tree("patch", &[("VertexShaders/Fog/CarGlass.vso", "1.02")]);
        let data = Data::new(&install, Some(patch.clone()));

        let glass = data.resolve("VertexShaders\\Fog\\CarGlass.vso").expect("glass");
        assert!(glass.starts_with(&patch));
        let texture = data.resolve("VertexShaders\\Fog\\CarTexture.vso").expect("texture");
        assert!(texture.starts_with(&install));
        assert_eq!(data.resolve("VertexShaders\\Fog\\Missing.vso"), None);
    }

    #[test]
    fn root_files_resolve_at_the_top_of_an_installed_game() {
        let extracted = tree("extracted", &[("Root/misc.rbz", "retail")]);
        let installed = tree("installed", &[("misc.rbz", "retail"), ("Scripts.rbz", "1.02")]);
        let file = Data::new(&extracted, None).resolve("Root/misc.rbz").expect("extracted");
        assert!(file.starts_with(&extracted));
        let data = Data::new(&installed, Some(installed.clone()));
        assert_eq!(data.resolve("Root\\misc.rbz"), Some(installed.join("misc.rbz")));
        assert_eq!(data.resolve("Root/Scripts.rbz"), Some(installed.join("Scripts.rbz")));
        assert_eq!(data.resolve("Root/season.dat"), None);
    }

    #[test]
    fn an_rsf_install_gives_its_renamed_audio_dat() {
        let rsf = tree("rsf-audio", &[("audio.dat.disabled", "retail")]);
        assert_eq!(Data::new(&rsf, None).audio_dat(), Some(rsf.join("audio.dat.disabled")));
        let both = tree("both-audio", &[("Root/audio.dat", "retail"), ("Root/audio.dat.disabled", "old")]);
        assert_eq!(Data::new(&both, None).audio_dat(), Some(both.join("Root/audio.dat")));
    }

    #[test]
    fn the_skin_file_wins_in_the_model_folder() {
        let install = tree("skin-install", &[("Cars/Cars.ini", "")]);
        let root = tree(
            "skin-root",
            &[
                ("Cars/Fiesta/Textures/body.dds", "car"),
                ("Cars/Fiesta/Textures/glass.dds", "car"),
                ("Cars/Fiesta/fiesta_shader_settings_clear.ini", "car"),
                ("Cars/Fiesta/skins/Monte/textures/body.dds", "skin"),
                ("Cars/Fiesta/skins/Monte/fiesta_shader_settings_clear.ini", "skin"),
            ],
        );
        let entry = crate::rsf::model_entry("Fiesta", "fiesta", 5);
        let data = Data::new(&install, None).with_car(root.clone(), entry);
        let skinned = data.clone().with_skin(root.join("Cars/Fiesta/skins/Monte"));
        let read = |data: &Data, path: &str| std::fs::read_to_string(data.resolve(path).expect(path)).unwrap();
        assert_eq!(read(&data, "Cars\\Fiesta\\Textures\\body.dds"), "car");
        assert_eq!(read(&skinned, "Cars\\fiesta\\Textures\\BODY.dds"), "skin");
        assert_eq!(read(&skinned, "Cars\\Fiesta\\fiesta_shader_settings_clear.ini"), "skin");
        assert_eq!(read(&skinned, "Cars\\Fiesta\\Textures\\glass.dds"), "car");
        assert!(skinned.resolve("Cars\\Cars.ini").unwrap().starts_with(&install));
    }

    #[test]
    fn without_a_patch_only_the_install_is_read() {
        let install = tree("no-patch", &[("PixelShaders/fog/CarGlass.pso", "retail")]);
        let data = Data::new(&install, None);
        let glass = data.resolve("PixelShaders\\Fog\\CarGlass.pso").expect("glass");
        assert!(glass.starts_with(&install));
    }
}

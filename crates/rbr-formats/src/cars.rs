//! Reader for `Cars/Cars.ini`, the index of the eight retail cars.
//!
//! Each `[CarNN]` section gives five DOS-style paths relative to the install root, such as
//! `"Cars\xsara\xsara.sgc"`. The game matches them case-insensitively: `Cars\Lancer_EVO_VII\...`
//! names the folder `LANCER_EVO_VII`. `ShaderSettings` is a prefix; the game appends
//! `_clear.ini` or `_overcast.ini`. The index `NN` selects the car's `Physics/<dir>/` folder
//! (see `notes/formats.md` "Cars"). Use [`resolve`] to map a path onto a case-sensitive file system.

use crate::ini;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

/// One `[CarNN]` section of `Cars/Cars.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarEntry {
    /// The `NN` of `[CarNN]`.
    pub index: usize,
    /// Model file, `*.sgc`.
    pub file_name: String,
    /// Per-car definition, `<car>.ini`.
    pub ini_file: String,
    /// Shader definitions, `<car>_shaders.ini`.
    pub shader_file: String,
    /// Prefix of the two shader setting files (`_clear.ini`, `_overcast.ini`).
    pub shader_settings: String,
    /// Directory of the car's textures.
    pub texture_path: String,
}

/// Failure to read `Cars.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Ini(ini::ParseError),
    /// A section that is not `Car` followed by decimal digits.
    BadSectionName(String),
    MissingKey {
        section: String,
        key: &'static str,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Ini(e) => write!(f, "Cars.ini: {e}"),
            Error::BadSectionName(name) => write!(f, "Cars.ini: bad section [{name}]"),
            Error::MissingKey { section, key } => {
                write!(f, "Cars.ini: [{section}] has no {key}")
            }
        }
    }
}

impl std::error::Error for Error {}

const PATH_KEYS: [&str; 5] = ["FileName", "IniFile", "ShaderFile", "ShaderSettings", "TexturePath"];

/// Parses `Cars/Cars.ini`. Every section must be `[CarNN]` with all five path keys.
pub fn parse(text: &str) -> Result<Vec<CarEntry>, Error> {
    let sections = ini::parse(text).map_err(Error::Ini)?;
    sections
        .iter()
        .map(|section| {
            let index = section
                .name
                .strip_prefix("Car")
                .filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|d| d.parse::<usize>().ok())
                .ok_or_else(|| Error::BadSectionName(section.name.clone()))?;
            let value = |key: &'static str| -> Result<String, Error> {
                let raw = section.get(key).ok_or_else(|| Error::MissingKey { section: section.name.clone(), key })?;
                Ok(raw.trim_matches('"').to_owned())
            };
            let [file_name, ini_file, shader_file, shader_settings, texture_path] = PATH_KEYS.map(value);
            Ok(CarEntry {
                index,
                file_name: file_name?,
                ini_file: ini_file?,
                shader_file: shader_file?,
                shader_settings: shader_settings?,
                texture_path: texture_path?,
            })
        })
        .collect()
}

/// Maps a DOS-style path (`Cars\xsara\xsara.sgc`) under `root` onto an existing file or directory,
/// matching each component ignoring ASCII case. Returns `None` if any component is missing.
pub fn resolve(root: &Path, dos_path: &str) -> Option<PathBuf> {
    dos_path.split(['\\', '/']).filter(|c| !c.is_empty()).try_fold(root.to_path_buf(), |dir, component| {
        std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .find(|name| name.to_str().is_some_and(|n| n.eq_ignore_ascii_case(component)))
            .map(|name| dir.join(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_CARS: &str = "; comment\r\n\r\n[Car00]\r\n\
        FileName \t= \"Cars\\xsara\\xsara.sgc\"\r\n\
        IniFile \t= \"Cars\\xsara\\xsara.ini\"\r\n\
        ShaderFile\t= \"Cars\\xsara\\xsara_shaders.ini\"\r\n\
        ShaderSettings \t= \"Cars\\xsara\\xsara_shader_settings\"\r\n\
        TexturePath\t= \"Cars\\xsara\\Textures\\\"\r\n\r\n\
        [Car07]\r\nFileName = \"Cars\\impreza00\\impreza00.sgc\"\r\n\
        IniFile = \"Cars\\impreza00\\impreza00.ini\"\r\n\
        ShaderFile = \"Cars\\impreza00\\impreza00_shaders.ini\"\r\n\
        ShaderSettings = \"Cars\\impreza00\\impreza00_shader_settings\"\r\n\
        TexturePath = \"Cars\\impreza00\\Textures\\\"\r\n";

    #[test]
    fn parses_index_and_quoted_paths() {
        let cars = parse(TWO_CARS).unwrap();
        assert_eq!(cars.len(), 2);
        assert_eq!(cars[0].index, 0);
        assert_eq!(cars[0].file_name, "Cars\\xsara\\xsara.sgc");
        assert_eq!(cars[0].shader_settings, "Cars\\xsara\\xsara_shader_settings");
        assert_eq!(cars[0].texture_path, "Cars\\xsara\\Textures\\");
        assert_eq!(cars[1].index, 7);
        assert_eq!(cars[1].ini_file, "Cars\\impreza00\\impreza00.ini");
    }

    #[test]
    fn empty_index_has_no_cars() {
        assert!(parse("; nothing here\r\n").unwrap().is_empty());
    }

    #[test]
    fn rejects_bad_section_names() {
        for name in ["Car", "Car+1", "Car-1", "CarX", "Settings", "car03"] {
            let text = TWO_CARS.replace("[Car07]", &format!("[{name}]"));
            assert_eq!(parse(&text).unwrap_err(), Error::BadSectionName(name.to_owned()), "{name}");
        }
    }

    #[test]
    fn rejects_missing_keys_and_bad_ini() {
        let text = TWO_CARS.replace("ShaderFile", "ShaderFilx");
        assert_eq!(parse(&text).unwrap_err(), Error::MissingKey { section: "Car00".to_owned(), key: "ShaderFile" });
        // Like `GetPrivateProfileStringA`, a line before the first section is not read.
        assert!(parse("FileName = x\n").unwrap().is_empty());
    }

    #[test]
    fn resolve_matches_components_ignoring_case() {
        let root = std::env::temp_dir().join(format!("rbr-cars-resolve-{}", std::process::id()));
        let model = root.join("Cars").join("LANCER_EVO_VII");
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("Lancer_Evo_VII.sgc"), b"x").unwrap();

        let found = resolve(&root, "Cars\\lancer_evo_vii\\lancer_evo_vii.SGC");
        assert_eq!(found, Some(model.join("Lancer_Evo_VII.sgc")));
        assert_eq!(resolve(&root, "Cars\\lancer_evo_vii"), Some(model.clone()));
        assert_eq!(resolve(&root, "Cars\\zr\\zr.sgc"), None);
        assert_eq!(resolve(&root, "Cars\\..\\Cars"), None);

        std::fs::remove_dir_all(&root).unwrap();
    }
}

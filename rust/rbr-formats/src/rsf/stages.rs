//! RallySimFans (RSF) stages: community stages in the original format, one folder per stage under the RSF install's
//! `Maps/` (`notes/formats.md` "RSF stages"). A folder holds the stage's files (`track-<id>_<letter>.lbs` and the rest,
//! read as retail ones through [`crate::stage_files`]) and RSF's own files:
//!
//! - `Tracks<id>.ini`: one `[Map<id>]` section with `TrackName` (`Maps\<folder>\track-<id>`), `Particles` (a `PS_*`
//!   file without `.ini`, often a retail one), `StageName`, and on newer stages `Surface`, `Length`, `Author`,
//!   `CountryCode`, `DefaultWetness`, `AnimationPath`;
//! - `TrackSettings<id>.ini`: the sky, fog and light sections of the stage, named as the retail
//!   `Maps/TrackSettings.ini`'s (`101O_E_hazy_partcloud`); the section names are also the weathers RSF offers;
//! - `TextureFilenameMap<id>.ini`: texture requests remapped, often to another stage's archive
//!   ([`stage_textures::FilenameMap::parse_rsf`]).
//!
//! `rsfdata/cache/stages_data.json` (RSF's stage list) adds each stage's surface and reference time.

use crate::data::Data;
use crate::ini;
use crate::stage_files;
use crate::stage_textures::FilenameMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The track letters by index (`Vf04`, 0x4a9fc0), as the season rows give them.
pub const LETTERS: [char; 5] = ['M', 'N', 'E', 'O', 'S'];
/// The weather presets by id (`Vf0c`).
pub const PRESETS: [&str; 10] =
    ["crisp", "hazy", "norain", "lightrain", "heavyrain", "nosnow", "lightsnow", "heavysnow", "lightfog", "heavyfog"];
/// The clouds by id (`Vf10`).
pub const CLOUDS: [&str; 4] = ["clear", "partcloud", "lightcloud", "heavycloud"];
/// The letters in the order a stage's variant is chosen when its weathers name none that is installed.
const VARIANT_ORDER: [char; 5] = ['O', 'M', 'N', 'E', 'S'];

/// One weather of the stage, from a `TrackSettings<id>.ini` section name: the season row's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeatherRow {
    /// The section (`101O_E_hazy_partcloud`).
    pub section: String,
    /// The track letter's index in [`LETTERS`].
    pub letter: u32,
    /// The cloud letter's index in [`LETTERS`] (`O` and `S` tracks name one), else 0.
    pub cloud_letter: u32,
    /// The weather id ([`PRESETS`]).
    pub preset: u32,
    /// The cloud id ([`CLOUDS`]).
    pub cloud: u32,
}

impl WeatherRow {
    /// Parses a section name `<id><letter>_[<cloud letter>_]<preset>_<cloud>` of stage `id`.
    pub fn parse(id: u32, section: &str) -> Option<WeatherRow> {
        let rest = section.trim().strip_prefix(&id.to_string())?;
        let mut chars = rest.chars();
        let letter = index(&LETTERS, &chars.next()?)?;
        let words: Vec<&str> = chars.as_str().strip_prefix('_')?.split('_').collect();
        let (cloud_letter, preset, cloud) = match words[..] {
            [l, p, c] if l.len() == 1 => (index(&LETTERS, &l.chars().next()?)?, p, c),
            [p, c] => (0, p, c),
            _ => return None,
        };
        let lower = |w: &str| w.to_ascii_lowercase();
        Some(WeatherRow {
            section: section.trim().to_owned(),
            letter,
            cloud_letter,
            preset: index(&PRESETS, &lower(preset).as_str())?,
            cloud: index(&CLOUDS, &lower(cloud).as_str())?,
        })
    }

    pub fn letter_char(&self) -> char {
        LETTERS[self.letter as usize]
    }

    /// Whether it rains, snows or is foggy: RSF's bad weathers (ours: the split RSF's menu offers is not traced).
    pub fn bad(&self) -> bool {
        matches!(self.preset, 3 | 4 | 6..=9)
    }
}

fn index<T: PartialEq>(table: &[T], value: &T) -> Option<u32> {
    table.iter().position(|v| v == value).map(|i| i as u32)
}

/// A stage's surface (RSF's `surface_id` 1, 2, 3 and `Tracks<id>.ini`'s `Surface` 0, 1, 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Tarmac,
    Gravel,
    Snow,
}

impl Surface {
    /// The suffix of the car's surface files, as the exe's `E14_4a1330` gives it.
    pub fn name(self) -> &'static str {
        match self {
            Surface::Tarmac => "tarmac",
            Surface::Gravel => "gravel",
            Surface::Snow => "snow",
        }
    }
}

/// One stage folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    pub id: u32,
    /// `StageName`, else the folder's name after the id.
    pub name: String,
    pub folder: PathBuf,
    /// The track's file stem without the letter (`track-101`).
    pub track: String,
    /// The installed variants: the letters with a `.lbs`, sorted.
    pub variants: Vec<char>,
    /// `Particles` (`Maps\ps_arctic`): a DOS path from the RSF root, without `.ini`.
    pub particles: Option<String>,
    pub surface: Option<Surface>,
    /// Length in km.
    pub length: Option<f32>,
    pub author: Option<String>,
    pub country: Option<String>,
    /// RSF's reference time in seconds (`stages_data.json` `deftime`).
    pub reference_time: Option<f32>,
    /// `DefaultWetness` (0 dry, 1 damp, 2 wet).
    pub default_wetness: Option<u32>,
    /// `AnimationPath` (`CustomData`): a folder of the stage's own character scripts and models.
    pub animation_path: Option<String>,
    /// The weathers of `TrackSettings<id>.ini`, in file order.
    pub weathers: Vec<WeatherRow>,
    /// Why the stage does not load, if it does not: the first missing or unreadable file.
    pub problem: Option<String>,
}

/// The extensions a stage variant must have.
const REQUIRED: [&str; 6] = ["lbs", "col", "trk", "dls", "mat", "ini"];

impl Stage {
    /// Reads stage folder `folder` (its `Tracks<id>.ini`), with RSF's stage list `list` (by id) when there is one.
    pub fn open(folder: &Path, list: &HashMap<u32, ListEntry>) -> Result<Stage, String> {
        let tracks = std::fs::read_dir(folder)
            .map_err(|e| format!("{}: {e}", folder.display()))?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .find(|n| {
                let l = n.to_ascii_lowercase();
                l.strip_prefix("tracks")
                    .and_then(|r| r.strip_suffix(".ini"))
                    .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
            })
            .ok_or_else(|| format!("{}: no Tracks<id>.ini", folder.display()))?;
        let text =
            String::from_utf8_lossy(&std::fs::read(folder.join(&tracks)).map_err(|e| e.to_string())?).into_owned();
        let sections = ini::parse(&text).map_err(|e| format!("{tracks}: {e}"))?;
        let section = sections
            .iter()
            .find(|s| s.name.len() > 3 && s.name[..3].eq_ignore_ascii_case("map"))
            .ok_or_else(|| format!("{tracks}: no [Map<id>] section"))?;
        let id: u32 = section.name[3..].trim().parse().map_err(|_| format!("{tracks}: [{}]", section.name))?;
        let value = |key: &str| section.get(key).map(|v| v.trim().trim_matches('"').trim().to_owned());
        let track = value("TrackName")
            .as_deref()
            .map(|t| t.rsplit(['\\', '/']).next().unwrap_or(t).to_owned())
            .unwrap_or_else(|| format!("track-{id}"));
        let field = |key: &str| list.get(&id).and_then(|e| e.get(key)).map(String::as_str);
        let folder_name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = value("StageName")
            .filter(|n| !n.is_empty())
            .or_else(|| field("name").map(str::to_owned))
            .unwrap_or_else(|| folder_name.split_once('-').map_or(folder_name.clone(), |(_, n)| n.to_owned()));
        let surface = field("surface_id")
            .and_then(|s| match s {
                "1" => Some(Surface::Tarmac),
                "2" => Some(Surface::Gravel),
                "3" => Some(Surface::Snow),
                _ => None,
            })
            .or_else(|| match value("Surface")?.as_str() {
                "0" => Some(Surface::Tarmac),
                "1" => Some(Surface::Gravel),
                "2" => Some(Surface::Snow),
                _ => None,
            });
        let mut variants: Vec<char> = std::fs::read_dir(folder)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| {
                let stem = n.strip_suffix(".lbs").or_else(|| n.strip_suffix(".LBS"))?;
                let (base, letter) = stem.rsplit_once('_')?;
                let mut l = letter.chars();
                match (base.eq_ignore_ascii_case(&track), l.next(), l.next()) {
                    (true, Some(c), None) => Some(c.to_ascii_uppercase()),
                    _ => None,
                }
            })
            .collect();
        variants.sort_unstable();
        variants.dedup();
        let weathers = settings_sections(folder, id).iter().filter_map(|s| WeatherRow::parse(id, s)).collect();
        let mut stage = Stage {
            id,
            name,
            folder: folder.to_owned(),
            track,
            variants,
            particles: value("Particles").filter(|p| !p.is_empty()),
            surface,
            length: value("Length")
                .and_then(|l| l.parse().ok())
                .or_else(|| field("length").and_then(|m| m.parse::<f32>().ok()).map(|m| m / 1000.0)),
            author: value("Author").or_else(|| field("author").map(str::to_owned)),
            country: value("CountryCode").or_else(|| field("short_country").map(str::to_owned)),
            reference_time: field("deftime").and_then(|t| t.parse().ok()).filter(|t| *t > 0.0),
            default_wetness: value("DefaultWetness").and_then(|w| match w.to_ascii_lowercase().as_str() {
                "dry" | "0" => Some(0),
                "damp" | "1" => Some(1),
                "wet" | "2" => Some(2),
                _ => None,
            }),
            animation_path: value("AnimationPath").filter(|p| !p.is_empty()),
            weathers,
            problem: None,
        };
        stage.problem = stage.check().err();
        Ok(stage)
    }

    /// The stage named for `letter` ([`stage_files`]): the path of its files without the extension.
    pub fn stage(&self, letter: char) -> String {
        self.folder.join(format!("{}_{letter}", self.track)).to_string_lossy().into_owned()
    }

    /// The variant to load when nothing chooses one: the first installed of O, M, N, E, S that has a weather, else
    /// the first installed (140 has O and M files but only M weathers).
    pub fn default_variant(&self) -> Option<char> {
        let installed = || VARIANT_ORDER.into_iter().filter(|l| self.variants.contains(l));
        installed().find(|&l| self.weathers.iter().any(|w| w.letter_char() == l)).or_else(|| installed().next())
    }

    /// The weathers whose variant is installed.
    pub fn installed_weathers(&self) -> impl Iterator<Item = &WeatherRow> {
        self.weathers.iter().filter(|w| self.variants.contains(&w.letter_char()))
    }

    /// The stage's texture remap (`TextureFilenameMap<id>.ini`), empty when it has none. An encrypted one is an error.
    pub fn texture_map(&self) -> Result<FilenameMap, String> {
        texture_map(&self.folder, self.id)
    }

    /// The stage's particle file: `Particles` with `.ini` (`_LOW.ini` when `low` and the stage has one), from the RSF
    /// root (the folder's `../..`), else from the game's data.
    pub fn particle_file(&self, data: &Data, low: bool) -> Option<PathBuf> {
        let stem = self.particles.as_deref()?;
        let root = root_of(&self.folder);
        let find = |file: String| root.and_then(|r| crate::cars::resolve(r, &file)).or_else(|| data.resolve(&file));
        low.then(|| find(format!("{stem}_LOW.ini"))).flatten().or_else(|| find(format!("{stem}.ini")))
    }

    /// Whether every variant has its files, readable: the first problem found.
    fn check(&self) -> Result<(), String> {
        if self.variants.is_empty() {
            return Err(format!("no {}_<letter>.lbs", self.track));
        }
        for &letter in &self.variants {
            let stage = self.stage(letter);
            for ext in REQUIRED {
                let path = stage_files::file_in(&self.folder, &stage, ext);
                let mut head = [0u8; 4];
                let read = std::fs::File::open(&path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head));
                if read.is_err() {
                    return Err(format!("{}: missing", stage_files::name(&stage).to_owned() + "." + ext));
                }
                if stage_files::encrypted(&head) {
                    return Err(format!("{}.{ext} is encrypted by RallySimFans", stage_files::name(&stage)));
                }
            }
            // The texture archive is optional (a stage may remap every texture or ship a loose folder).
            let textures = format!("{}_textures.rbz", stage_files::name(&stage));
            if let Some(path) = stage_files::in_dir(&self.folder, &textures) {
                let mut head = [0u8; 4];
                let read = std::fs::File::open(&path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head));
                if read.is_ok() && stage_files::encrypted(&head) {
                    return Err(format!("{textures} is encrypted by RallySimFans"));
                }
            }
        }
        self.texture_map().map(drop)
    }
}

/// The `[...]` names of the folder's `TrackSettings<id>.ini`, in file order; none when it has none.
fn settings_sections(folder: &Path, id: u32) -> Vec<String> {
    let Some(text) = settings_text(folder, id) else { return Vec::new() };
    ini::parse(&text).map(|s| s.into_iter().map(|s| s.name).collect()).unwrap_or_default()
}

/// The text of the stage's `TrackSettings<id>.ini`: the folder's, else the one in the RSF `Maps` folder above it (RSF
/// keeps a copy of most there). An empty file in the folder counts as none (370 ships one; ours, RSF's lookup is not
/// traced).
pub fn settings_text(folder: &Path, id: u32) -> Option<String> {
    let file = format!("TrackSettings{id}.ini");
    let read = |dir: &Path| {
        let bytes = std::fs::read(stage_files::in_dir(dir, &file)?).ok()?;
        (!bytes.iter().all(u8::is_ascii_whitespace)).then_some(bytes)
    };
    let bytes = read(folder).or_else(|| read(folder.parent()?))?;
    (!stage_files::encrypted(&bytes)).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// The folder's `TextureFilenameMap<id>.ini` remap, empty when it has none.
pub fn texture_map(folder: &Path, id: u32) -> Result<FilenameMap, String> {
    let Some(path) = stage_files::in_dir(folder, &format!("TextureFilenameMap{id}.ini")) else {
        return Ok(FilenameMap::parse_rsf(""));
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if stage_files::encrypted(&bytes) {
        return Err(format!("{}: encrypted by RallySimFans", path.display()));
    }
    Ok(FilenameMap::parse_rsf(&String::from_utf8_lossy(&bytes)))
}

/// The retail track a stage was built on, from the empty marker file `track-<id>_<base>_<name>` in its folder (`103`
/// has `track-103_21_Falstone II`): its textures stand in for the stage's that neither its archive nor its remap has
/// (ours, inferred from the files: RSF's plugin is not traced; `notes/formats.md` "RSF stages").
pub fn base_track(folder: &Path, id: u32) -> Option<u32> {
    let prefix = format!("track-{id}_");
    std::fs::read_dir(folder).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name().into_string().ok()?;
        let rest = name.get(prefix.len()..).filter(|_| name[..prefix.len()].eq_ignore_ascii_case(&prefix))?;
        let digits = rest.find(|c: char| !c.is_ascii_digit())?;
        let empty = entry.metadata().ok()?.is_file() && entry.metadata().ok()?.len() == 0;
        (digits > 0 && matches!(rest.as_bytes()[digits], b'_' | b' ') && empty).then(|| rest[..digits].parse().ok())?
    })
}

/// The id of a stage folder named by [`stage_files`] (`.../101-Autiovaara II/track-101_O` gives 101), when the
/// folder has a `Tracks<id>.ini`.
pub fn folder_id(stage: &str) -> Option<u32> {
    let folder = stage_files::folder(stage)?;
    let (number, _) = stage_files::parts(stage)?;
    stage_files::in_dir(folder, &format!("Tracks{number}.ini")).map(|_| number)
}

/// One object of `stages_data.json`: its string fields.
pub type ListEntry = HashMap<String, String>;

/// RSF's stage list (`<root>/rsfdata/cache/stages_data.json`) by id; empty when the file is missing or does not read.
pub fn stage_list(root: &Path) -> HashMap<u32, ListEntry> {
    let Some(path) = stage_files::in_dir(root, "rsfdata\\cache\\stages_data.json") else { return HashMap::new() };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    flat_objects(&text).into_iter().filter_map(|o| Some((o.get("id")?.parse().ok()?, o))).collect()
}

/// The RSF root of a stage folder (`<root>/Maps/<folder>`).
pub fn root_of(folder: &Path) -> Option<&Path> {
    folder.parent()?.parent()
}

/// Opens the stage folder `folder` with its RSF root's stage list.
pub fn open(folder: &Path) -> Result<Stage, String> {
    let list = root_of(folder).map(stage_list).unwrap_or_default();
    Stage::open(folder, &list)
}

/// Every stage folder under `<root>/Maps` (symlinked ones too), sorted by id: the ones that do not read at all as
/// `Err` with the folder.
pub fn scan(root: &Path) -> Vec<Result<Stage, (PathBuf, String)>> {
    let list = stage_list(root);
    let Some(maps) = stage_files::in_dir(root, "Maps") else { return Vec::new() };
    let mut folders: Vec<PathBuf> = std::fs::read_dir(maps)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    let mut stages: Vec<_> = folders.into_iter().map(|f| Stage::open(&f, &list).map_err(|e| (f, e))).collect();
    stages.sort_by_key(|s| s.as_ref().map(|s| s.id).unwrap_or(u32::MAX));
    stages
}

/// The stage folder of stage `id` under `<root>/Maps` (named `<id>-<name>`, else found by its `Tracks<id>.ini`).
pub fn find(root: &Path, id: u32) -> Result<Stage, String> {
    let maps = stage_files::in_dir(root, "Maps").ok_or_else(|| format!("{}: no Maps folder", root.display()))?;
    let prefix = format!("{id}-");
    let folder = std::fs::read_dir(&maps)
        .map_err(|e| format!("{}: {e}", maps.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .find(|p| {
            p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
                || stage_files::in_dir(p, &format!("Tracks{id}.ini")).is_some()
        })
        .ok_or_else(|| format!("RSF stage {id}: no folder in {}", maps.display()))?;
    Stage::open(&folder, &stage_list(root))
}

/// The objects of a JSON array of flat objects whose values are strings, numbers or `null` (RSF's cache files); a
/// `null` value is left out. Anything else ends the parse with what was read.
pub fn flat_objects(text: &str) -> Vec<HashMap<String, String>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    let skip_ws = |c: &mut std::iter::Peekable<std::str::Chars>| {
        while c.peek().is_some_and(|ch| ch.is_whitespace() || *ch == ',') {
            c.next();
        }
    };
    let string = |c: &mut std::iter::Peekable<std::str::Chars>| -> Option<String> {
        let mut s = String::new();
        while let Some(ch) = c.next() {
            match ch {
                '"' => return Some(s),
                '\\' => match c.next()? {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'u' => {
                        let hex: String = c.by_ref().take(4).collect();
                        s.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or('?'));
                    }
                    other => s.push(other),
                },
                _ => s.push(ch),
            }
        }
        None
    };
    skip_ws(&mut chars);
    if chars.next() != Some('[') {
        return out;
    }
    loop {
        skip_ws(&mut chars);
        if chars.next() != Some('{') {
            return out;
        }
        let mut object = HashMap::new();
        loop {
            skip_ws(&mut chars);
            match chars.next() {
                Some('}') => break,
                Some('"') => {}
                _ => return out,
            }
            let Some(key) = string(&mut chars) else { return out };
            skip_ws(&mut chars);
            if chars.next() != Some(':') {
                return out;
            }
            skip_ws(&mut chars);
            let value = match chars.peek() {
                Some('"') => {
                    chars.next();
                    string(&mut chars)
                }
                _ => {
                    let word: String = std::iter::from_fn(|| chars.next_if(|c| !matches!(c, ',' | '}'))).collect();
                    let word = word.trim().to_owned();
                    (word != "null").then_some(word)
                }
            };
            if let Some(value) = value {
                object.insert(key, value);
            }
        }
        out.push(object);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_rows_from_section_names() {
        let row = WeatherRow::parse(101, "101O_E_hazy_partcloud").unwrap();
        assert_eq!((row.letter_char(), row.cloud_letter, row.preset, row.cloud), ('O', 2, 1, 1));
        let row = WeatherRow::parse(129, "129M_crisp_clear").unwrap();
        assert_eq!((row.letter_char(), row.cloud_letter, row.preset, row.cloud), ('M', 0, 0, 0));
        assert!(!row.bad());
        assert!(WeatherRow::parse(294, "294O_N_heavyrain_heavycloud").unwrap().bad());
        assert_eq!(WeatherRow::parse(101, "10O_E_hazy_partcloud"), None);
        assert_eq!(WeatherRow::parse(101, "101O_E_sunny_partcloud"), None);
    }

    #[test]
    fn flat_json_objects() {
        let text = r#"[{"id":"10","name":"Kaihuä","fattrib":null,"n":5},{"id":"101","author_note":"a, b"}]"#;
        let objects = flat_objects(text);
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["name"], "Kaihuä");
        assert!(!objects[0].contains_key("fattrib"));
        assert_eq!(objects[0]["n"], "5");
        assert_eq!(objects[1]["author_note"], "a, b");
    }

    /// A stage folder with a `Tracks<id>.ini`, a `TrackSettings<id>.ini` and variant `O`'s files, the `.col` encrypted.
    #[test]
    fn a_folder_reads_and_an_encrypted_file_is_its_problem() {
        let root = std::env::temp_dir().join(format!("rbr-rsf-{}", std::process::id()));
        let folder = root.join("Maps").join("777-Test Stage");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("Tracks777.ini"),
            "[Map777]\r\nTrackName=\"Maps\\777-Test Stage\\track-777\"\r\nParticles=\"Maps\\ps_british\"\r\nStageName=\"Test\"\r\nSurface=1\r\n",
        )
        .unwrap();
        std::fs::write(folder.join("TrackSettings777.ini"), "[777O_E_crisp_clear]   ; a comment\r\nFogEnd = 5\r\n")
            .unwrap();
        for ext in REQUIRED {
            std::fs::write(folder.join(format!("track-777_O.{ext}")), b"plain").unwrap();
        }
        let stage = find(&root, 777).unwrap();
        assert_eq!((stage.id, stage.name.as_str(), stage.track.as_str()), (777, "Test", "track-777"));
        assert_eq!(stage.variants, ['O']);
        assert_eq!(stage.surface, Some(Surface::Gravel));
        assert_eq!(stage.weathers.len(), 1);
        assert_eq!(stage.problem, None);
        assert_eq!(folder_id(&stage.stage('O')), Some(777));
        assert_eq!(stage.default_variant(), Some('O'));
        // O comes first, but only M has a weather.
        for ext in REQUIRED {
            std::fs::write(folder.join(format!("track-777_M.{ext}")), b"plain").unwrap();
        }
        std::fs::write(folder.join("TrackSettings777.ini"), "[777M_E_crisp_clear]\r\nFogEnd = 5\r\n").unwrap();
        assert_eq!(open(&folder).unwrap().default_variant(), Some('M'));
        std::fs::write(folder.join("track-777_O.col"), [1u8, 0xa0, 0, 0, 9]).unwrap();
        let stage = open(&folder).unwrap();
        assert_eq!(stage.problem.as_deref(), Some("track-777_O.col is encrypted by RallySimFans"));
        assert_eq!(base_track(&folder, 777), None);
        std::fs::write(folder.join("track-777_21_Test Stage"), b"").unwrap();
        assert_eq!(base_track(&folder, 777), Some(21));
        // An empty file in the folder: the `Maps` folder's copy.
        std::fs::write(folder.join("TrackSettings777.ini"), b"").unwrap();
        assert_eq!(open(&folder).unwrap().weathers.len(), 0);
        std::fs::write(root.join("Maps").join("TrackSettings777.ini"), "[777M_E_crisp_clear]\r\n").unwrap();
        assert_eq!(open(&folder).unwrap().weathers.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stage folder symlinked into `Maps` is scanned like a plain one.
    #[cfg(unix)]
    #[test]
    fn scan_follows_a_symlinked_stage_folder() {
        let root = std::env::temp_dir().join(format!("rbr-rsf-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let real = root.join("elsewhere").join("778-Linked");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::create_dir_all(root.join("Maps")).unwrap();
        std::fs::write(real.join("Tracks778.ini"), "[Map778]\r\nStageName=\"Linked\"\r\n").unwrap();
        std::os::unix::fs::symlink(&real, root.join("Maps").join("778-Linked")).unwrap();
        let ids: Vec<u32> = scan(&root).into_iter().map(|s| s.unwrap().id).collect();
        assert_eq!(ids, [778]);
        assert_eq!(find(&root, 778).unwrap().id, 778);
        let _ = std::fs::remove_dir_all(&root);
    }
}

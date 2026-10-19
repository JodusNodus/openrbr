//! The skid (tyre) mark data of a particle file, `Maps/PS_<Rally>.ini`: its `Skidmarks` section and the
//! `Skidmark Aliases` after it. See `notes/marks-sound-ghost.md` 1.1.
//!
//! The particle file is not sectioned by `[Name]` headers. Each section starts at a `;  <Title>` comment line, and
//! its values are one per line (`value ;comment`), so this reader takes the counted values after the header and
//! does not look for the next one. A record is 12 lines: material, texture index, RGB, then the size, flush threshold,
//! general add and the rubber, depth and dirt reduction and multiplier pairs. Aliases are a count, then `from to` pairs.

use crate::dat;
use crate::data::Data;
use crate::ini::{self, ParseError};
use std::str::FromStr;

/// One `Skidmarks` record: the surface it lays a mark on and how the mark looks.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// The surface (material) id the record handles.
    pub material: u32,
    /// The name the file gives the record in a `; "name"` comment above it (`"Arctic"`, `"British_-_Gravel"`); empty
    /// when the section does not name every record.
    pub label: String,
    /// Row into `skidmarks.dds` (`notes/particles.md`, `E82D_Atlas::Row`).
    pub texture: u32,
    pub rgb: [f32; 3],
    /// Size modifier: the mark's width is scaled about its midpoint by this.
    pub size: f32,
    /// Flush threshold: the accumulated time after which a new mark starts (`E83System::EmitTire`).
    pub flush: f32,
    /// General add: the constant part of the mark's intensity.
    pub general: f32,
    pub rubber_reduction: f32,
    pub rubber_multiplier: f32,
    pub depth_reduction: f32,
    pub depth_multiplier: f32,
    pub dirt_reduction: f32,
    pub dirt_multiplier: f32,
}

/// The `Skidmarks` section and its aliases.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Skidmarks {
    pub records: Vec<Record>,
    /// `(from, to)`: a surface id that the skid record lookup treats as `to`.
    pub aliases: Vec<(u32, u32)>,
}

impl Skidmarks {
    /// The record for a surface id (the id itself, not its alias).
    pub fn record(&self, material: u32) -> Option<&Record> {
        self.records.iter().find(|r| r.material == material)
    }

    /// The surface id the skid lookup uses for `material`: the last alias of it (the loader writes the pairs in order,
    /// over an identity map, `E83System::LoadFile`), else itself.
    pub fn alias(&self, material: u32) -> u32 {
        self.aliases.iter().rev().find(|&&(from, _)| from == material).map_or(material, |&(_, to)| to)
    }
}

/// The skid table of a stage: the particle file its `Maps/Tracks.ini` entry names (`Particles`, for example
/// `Maps\ps_japan`), read from the install (or the patch). `Ok(None)` when no entry names the stage or the entry has no
/// particle file. The stage name is the track's: `track-54_E` is the `track-54` entry. A stage in a folder of its own
/// (`stage_files`) names its particle file in its `Tracks<id>.ini` ([`crate::rsf::Stage::particle_file`]).
pub fn for_stage(data: &Data, stage: &str) -> Result<Option<Skidmarks>, String> {
    // A stage folder's own `Tracks<id>.ini` (RallySimFans).
    if let Some(folder) = crate::stage_files::folder(stage) {
        let Some(path) = crate::rsf::open(folder)?.particle_file(data, false) else { return Ok(None) };
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        return parse(&text).map(Some).map_err(|e| format!("{}: {e}", path.display()));
    }
    let Some(tracks) = data.resolve("Maps\\Tracks.ini") else {
        return Err("no Maps\\Tracks.ini".to_owned());
    };
    let text = std::fs::read_to_string(&tracks).map_err(|e| format!("{}: {e}", tracks.display()))?;
    let sections = ini::parse(&text).map_err(|e| format!("{}: {e}", tracks.display()))?;
    let names_stage = |section: &ini::Section| {
        section
            .get("TrackName")
            .map(|name| name.trim_matches('"'))
            .and_then(|name| name.rsplit('\\').next())
            .is_some_and(|track| stage == track || stage.strip_prefix(track).is_some_and(|rest| rest.starts_with('_')))
    };
    let Some(particles) = sections.iter().find(|s| names_stage(s)).and_then(|s| s.get("Particles")) else {
        return Ok(None);
    };
    let file = particles.trim_matches('"');
    let Some(path) = data.resolve(&format!("{file}.ini")) else {
        return Err(format!("no {file}.ini"));
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map(Some).map_err(|e| format!("{}: {e}", path.display()))
}

/// The bytes of `skidmarks.dds`, from the `Textures/particles.dat` archive.
pub fn texture(data: &Data) -> Result<Vec<u8>, String> {
    let Some(path) = data.resolve("Textures\\particles.dat") else {
        return Err("no Textures\\particles.dat".to_owned());
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let entries = dat::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let entry = entries
        .iter()
        .find(|e| !e.is_dir && e.path.to_ascii_lowercase().ends_with("skidmarks.dds"))
        .ok_or_else(|| format!("{} has no skidmarks.dds", path.display()))?;
    dat::file_data(&bytes, entry)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| format!("{}: skidmarks.dds is truncated", path.display()))
}

/// The `; "name"` comment lines of the `Skidmarks` section, one per record in order, or empty names when the section
/// does not have one for every record.
fn labels(lines: &[&str], count: usize) -> Vec<String> {
    let is_title = |line: &str, title: &str| line.trim().strip_prefix(';').is_some_and(|t| t.trim() == title);
    let start = lines.iter().position(|line| is_title(line, "Skidmarks")).unwrap_or(0);
    let end = lines.iter().position(|line| is_title(line, "Skidmark Aliases")).unwrap_or(lines.len());
    let names: Vec<String> = lines[start..end]
        .iter()
        .filter_map(|line| {
            let comment = line.trim().strip_prefix(';')?.trim();
            Some(comment.strip_prefix('"')?.strip_suffix('"')?.to_string())
        })
        .collect();
    if names.len() == count {
        names
    } else {
        vec![String::new(); count]
    }
}

/// The value lines of a file after `header` (a `;  <Title>` comment line), each with its line number.
fn values_after<'a>(lines: &'a [&'a str], header: &str) -> Option<impl Iterator<Item = (usize, &'a str)> + 'a> {
    let at = lines.iter().position(|line| line.trim().strip_prefix(';').is_some_and(|title| title.trim() == header))?;
    Some(lines.iter().enumerate().skip(at + 1).filter_map(|(i, line)| {
        let value = line.split(';').next().unwrap_or("").trim();
        (!value.is_empty()).then_some((i + 1, value))
    }))
}

/// Parses the `Skidmarks` section and the `Skidmark Aliases` of a particle file. A file without the section is an
/// error; a section with zero records (the `_low` files) is not.
pub fn parse(text: &str) -> Result<Skidmarks, ParseError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.lines().collect();
    let mut values =
        values_after(&lines, "Skidmarks").ok_or(ParseError { line: 0, message: "no Skidmarks section" })?;
    let count: u32 = field(&mut values)?;
    let mut records = Vec::with_capacity(count as usize);
    for label in labels(&lines, count as usize) {
        let [material] = fields::<u32, 1>(&mut values)?;
        let [texture] = fields::<u32, 1>(&mut values)?;
        let rgb = fields::<f32, 3>(&mut values)?;
        let [size] = fields(&mut values)?;
        let [flush] = fields(&mut values)?;
        let [general] = fields(&mut values)?;
        let [rubber_reduction] = fields(&mut values)?;
        let [rubber_multiplier] = fields(&mut values)?;
        let [depth_reduction] = fields(&mut values)?;
        let [depth_multiplier] = fields(&mut values)?;
        let [dirt_reduction] = fields(&mut values)?;
        let [dirt_multiplier] = fields(&mut values)?;
        records.push(Record {
            material,
            label,
            texture,
            rgb,
            size,
            flush,
            general,
            rubber_reduction,
            rubber_multiplier,
            depth_reduction,
            depth_multiplier,
            dirt_reduction,
            dirt_multiplier,
        });
    }
    let mut values = values_after(&lines, "Skidmark Aliases")
        .ok_or(ParseError { line: 0, message: "no Skidmark Aliases section" })?;
    let count: u32 = field(&mut values)?;
    let mut aliases = Vec::with_capacity(count as usize);
    for _ in 0..count {
        aliases.push(fields::<u32, 2>(&mut values).map(|[from, to]| (from, to))?);
    }
    Ok(Skidmarks { records, aliases })
}

/// The next value line, or an error when the section ends early.
fn next_line<'a>(values: &mut impl Iterator<Item = (usize, &'a str)>) -> Result<(usize, &'a str), ParseError> {
    values.next().ok_or(ParseError { line: 0, message: "the section ends early" })
}

/// The `N` whitespace-separated values of the next value line (a `value ;comment` line can hold several).
fn fields<'a, T: FromStr + Default + Copy, const N: usize>(
    values: &mut impl Iterator<Item = (usize, &'a str)>,
) -> Result<[T; N], ParseError> {
    let (line, text) = next_line(values)?;
    let mut words = text.split_whitespace();
    let mut out = [T::default(); N];
    for slot in out.iter_mut() {
        *slot =
            words.next().and_then(|word| word.parse().ok()).ok_or(ParseError { line, message: "expected a number" })?;
    }
    Ok(out)
}

/// The one value of the next value line.
fn field<'a, T: FromStr + Default + Copy>(
    values: &mut impl Iterator<Item = (usize, &'a str)>,
) -> Result<T, ParseError> {
    Ok(fields::<T, 1>(values)?[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "version3\r\n;  Skidmarks\r\n;  ====\r\n2                                  ;number of skidmarks\r\n\
; \"Arctic\"\r\n103                                ;material to handle\r\n7                                  ;texture index\r\n\
1 1 1                              ;rgb color values\r\n1.8                                ;size modifier\r\n\
0.4                                ;flush threshold\r\n0.075                              ;general add\r\n\
0                                  ;rubber reduction\r\n0                                  ;rubber multiplier\r\n\
0                                  ;depth reduction\r\n0                                  ;depth multiplier\r\n\
0                                  ;dirt reduction\r\n0                                  ;dirt multiplier\r\n\
; \"Grass\"\r\n38                                 ;material to handle\r\n4                                  ;texture index\r\n\
0.31 0.31 0.31                     ;rgb color values\r\n1.6                                ;size modifier\r\n\
0.3                                ;flush threshold\r\n0                                  ;general add\r\n\
0                                  ;rubber reduction\r\n0.2                                ;rubber multiplier\r\n\
0                                  ;depth reduction\r\n0                                  ;depth multiplier\r\n\
-0.5                               ;dirt reduction\r\n0.2                                ;dirt multiplier\r\n\
;  Skidmark Aliases\r\n 2                                ;number of aliases\r\n;\r\n117              103               ;\r\n\
41               17                ;\r\n";

    #[test]
    fn records_and_aliases_read_by_count() {
        let parsed = parse(SAMPLE).expect("parse");
        assert_eq!(parsed.records.len(), 2);
        let first = &parsed.records[0];
        assert_eq!((first.material, first.texture), (103, 7));
        assert_eq!(first.label, "Arctic");
        assert_eq!(first.rgb, [1.0, 1.0, 1.0]);
        assert_eq!((first.size, first.flush, first.general), (1.8, 0.4, 0.075));
        let second = &parsed.records[1];
        assert_eq!((second.material, second.label.as_str()), (38, "Grass"));
        assert_eq!(second.rgb, [0.31, 0.31, 0.31]);
        assert_eq!(second.dirt_reduction, -0.5);
        assert_eq!(parsed.aliases, vec![(117, 103), (41, 17)]);
        assert_eq!(parsed.alias(117), 103);
        assert_eq!(parsed.alias(5), 5);
        let repeated = Skidmarks { records: vec![], aliases: vec![(14, 18), (14, 22)] };
        assert_eq!(repeated.alias(14), 22, "the later pair wins");
        assert_eq!(parsed.record(38).map(|r| r.texture), Some(4));
    }

    #[test]
    fn a_zero_count_section_has_no_records() {
        let parsed = parse(";  Skidmarks\r\n0 ;number\r\n;  Skidmark Aliases\r\n0 ;n\r\n").expect("parse");
        assert_eq!(parsed, Skidmarks::default());
    }

    #[test]
    fn a_file_without_the_section_is_an_error() {
        assert!(parse("version3\r\n").is_err());
    }

    /// The stage's table comes from its `Tracks.ini` entry: `track-54_E` is Japan's (`Maps\ps_japan`), with 5 records.
    #[test]
    fn a_stage_finds_its_particle_file() {
        let Some(data) = Data::from_env() else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        let table = for_stage(&data, "track-54_E").expect("tracks").expect("japan");
        assert_eq!(table.records.len(), 5);
        assert_eq!(for_stage(&data, "no-such-stage").expect("tracks"), None);
    }

    /// Every retail `Maps/PS_*.ini`: the record counts from `notes/marks-sound-ghost.md` 1.1, the `_low` files with
    /// none, and each alias count matching the number of pairs read. Runs only when `RBR_DATA` is set.
    #[test]
    fn retail_particle_files_read() {
        let Ok(data_dir) = std::env::var("RBR_DATA") else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        let maps = std::path::Path::new(&data_dir).join("Maps");
        let expected = [
            ("PS_Arctic.ini", 1),
            ("PS_Australia.ini", 2),
            ("PS_British.ini", 12),
            ("PS_Japan.ini", 5),
            ("PS_MontBlanc.ini", 6),
            ("PS_USA.ini", 7),
        ];
        for (name, records) in expected {
            let text = std::fs::read_to_string(maps.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let parsed = parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(parsed.records.len(), records, "{name}");
            assert!(parsed.records.iter().all(|r| !r.label.is_empty()), "{name} names every record");
            assert!(parsed.records.iter().all(|r| r.rgb.iter().all(|c| (0.0..=1.0).contains(c))), "{name} colour");
        }
        for name in [
            "PS_Arctic_LOW.ini",
            "PS_Australia_LOW.ini",
            "PS_British_LOW.ini",
            "PS_Japan_LOW.ini",
            "PS_MontBlanc_LOW.ini",
            "PS_USA_LOW.ini",
        ] {
            let text = std::fs::read_to_string(maps.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            let parsed = parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(parsed.records.is_empty(), "{name}");
        }
    }
}

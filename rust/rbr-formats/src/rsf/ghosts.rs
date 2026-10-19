//! RSF's ghost cars: the `.gch` files of `GhostCars/` and the folder layout RSF keeps them in
//! (`notes/formats/rsf-ghosts.md`).
//!
//! Layout (the sample install's 510 files): `GhostCars/<map>/<map>_latest.gch`, the last drive on the map, and per
//! condition (`Dry`, `Damp`, `Wet`) `GhostCars/<map>/<condition>/<map>.gch`, the best drive on the map,
//! `<map>_<car id>.gch`, the best of each RSF car, and `<map>_<group>.gch`, the best of each car group (the car's base
//! group of `cars.json`, `Group A8`, `Rally 3`). A best file is a byte copy of the drive that set it.
//!
//! `.gch` (RSF's own format, not the game's `.ghc` stream; read from the samples, RSF's plugin is not traced): a header
//! of twelve words, the samples, and 16 trailing bytes.
//!
//! - `01GC`, the map, the word 5 (every sample; RSF's car slot, [`super::SLOT`], inferred), the RSF car id, three words
//!   (0 0 0, or 0 1 0 in every `Damp` file and in some `Dry` ones: [`Gch::words`]), the two splits and the finish
//!   time (s, `f32`; the best files are the lowest finish times of their folder), the penalty seconds (`f32`; the
//!   finish already holds them: the race clock jumps by them during the drive), and the sample count.
//! - A sample (40 bytes), about every 100 ms: the time since the race clock's state 1 in ms (the car leaves the line
//!   7 s in), the distance along the stage (m, whole), the race clock (whole seconds, 0 through the countdown, with the
//!   penalties), the rotation quaternion `[x, y, z, w]` and the position, `f32` in the physics frame: the body's (x
//!   and y within 2 cm of ours on a start line; 0.42 to 0.45 m higher, not explained).
//! - 16 bytes that are no plain hash of the file (MD5, SHA, CRC32 tried; ours: probably RSF's signature). Files this
//!   crate writes keep zeros there; they are for us, not for RSF's plugin.

use std::path::{Path, PathBuf};

/// `.gch` magic.
pub const GCH_MAGIC: [u8; 4] = *b"01GC";
/// Header bytes before the samples.
pub const GCH_HEADER_LEN: usize = 0x30;
const SAMPLE_LEN: usize = 40;
const TRAILER_LEN: usize = 16;

/// One `.gch` sample.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GchSample {
    /// Since the race clock's state 1, ms.
    pub time: u32,
    /// Along the stage, m.
    pub distance: f32,
    /// The race clock's whole seconds.
    pub clock: f32,
    /// `[x, y, z, w]`.
    pub rotation: [f32; 4],
    pub position: [f32; 3],
}

/// A `.gch` file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gch {
    pub map: u32,
    /// Word 2, 5 in every sample.
    pub slot: u32,
    /// The RSF car id (0 in ours for a car that is not RSF's: [`Gch::slot`] is then its `Cars.ini` index).
    pub car: u32,
    /// Words 4 to 6. The middle one is 1 in every `Damp` file (ours: the wetness, written so).
    pub words: [u32; 3],
    pub splits: [f32; 2],
    /// The finish time with the penalties, s.
    pub finish: f32,
    pub penalty: f32,
    pub samples: Vec<GchSample>,
    pub trailer: [u8; TRAILER_LEN],
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn float(bytes: &[u8], at: usize) -> f32 {
    f32::from_bits(word(bytes, at))
}

impl Gch {
    pub fn parse(bytes: &[u8]) -> Result<Gch, String> {
        let mut gch = Gch::parse_header(bytes)?;
        let count = word(bytes, 0x2c) as usize;
        let end = GCH_HEADER_LEN + count * SAMPLE_LEN;
        if bytes.len() != end + TRAILER_LEN {
            return Err(format!("{count} samples need {} bytes, the file has {}", end + TRAILER_LEN, bytes.len()));
        }
        gch.samples = bytes[GCH_HEADER_LEN..end]
            .chunks_exact(SAMPLE_LEN)
            .map(|b| GchSample {
                time: word(b, 0),
                distance: float(b, 4),
                clock: float(b, 8),
                rotation: std::array::from_fn(|i| float(b, 12 + i * 4)),
                position: std::array::from_fn(|i| float(b, 28 + i * 4)),
            })
            .collect();
        gch.trailer = bytes[end..].try_into().unwrap();
        Ok(gch)
    }

    /// The header alone (no samples): enough to list and compare files.
    pub fn parse_header(bytes: &[u8]) -> Result<Gch, String> {
        if bytes.len() < GCH_HEADER_LEN {
            return Err(format!("{} bytes is shorter than the header", bytes.len()));
        }
        if bytes[..4] != GCH_MAGIC {
            return Err("not an RSF ghost (no 01GC)".to_owned());
        }
        Ok(Gch {
            map: word(bytes, 4),
            slot: word(bytes, 8),
            car: word(bytes, 12),
            words: std::array::from_fn(|i| word(bytes, 16 + i * 4)),
            splits: [float(bytes, 0x1c), float(bytes, 0x20)],
            finish: float(bytes, 0x24),
            penalty: float(bytes, 0x28),
            ..Gch::default()
        })
    }

    /// The header of the file at `path`, read alone.
    pub fn read_header(path: &Path) -> Option<Gch> {
        use std::io::Read;
        let mut header = Vec::new();
        std::fs::File::open(path).ok()?.take(GCH_HEADER_LEN as u64).read_to_end(&mut header).ok()?;
        Gch::parse_header(&header).ok()
    }

    pub fn write(&self) -> Vec<u8> {
        let mut out = GCH_MAGIC.to_vec();
        let words = [self.map, self.slot, self.car, self.words[0], self.words[1], self.words[2]];
        out.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        for f in [self.splits[0], self.splits[1], self.finish, self.penalty] {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out.extend_from_slice(&(self.samples.len() as u32).to_le_bytes());
        for s in &self.samples {
            out.extend_from_slice(&s.time.to_le_bytes());
            for f in [s.distance, s.clock].iter().chain(&s.rotation).chain(&s.position) {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.trailer);
        out
    }
}

/// The road's condition, a folder of the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Dry,
    Damp,
    Wet,
}

impl Condition {
    /// The weather daemon's wetness (0 to 2).
    pub fn from_wetness(wetness: u32) -> Condition {
        match wetness {
            0 => Condition::Dry,
            1 => Condition::Damp,
            _ => Condition::Wet,
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            Condition::Dry => "Dry",
            Condition::Damp => "Damp",
            Condition::Wet => "Wet",
        }
    }
}

/// Which ghost of a map: the best overall, the best of a car, of a group, or the latest drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Overall,
    Car,
    Group,
    Latest,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Overall, Kind::Car, Kind::Group, Kind::Latest];
}

/// The drive a ghost is looked up or kept for: the map, the condition, the RSF car id (none for a car that is not
/// RSF's) and its group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub map: u32,
    pub condition: Condition,
    pub car: Option<u32>,
    pub group: Option<String>,
}

impl Key {
    /// The file of `kind` under the `GhostCars` folder `root`; `None` for a car or group ghost without one.
    pub fn path(&self, root: &Path, kind: Kind) -> Option<PathBuf> {
        let map = root.join(self.map.to_string());
        let best = |name: String| map.join(self.condition.folder()).join(format!("{name}.gch"));
        Some(match kind {
            Kind::Latest => map.join(format!("{}_latest.gch", self.map)),
            Kind::Overall => best(self.map.to_string()),
            Kind::Car => best(format!("{}_{}", self.map, self.car?)),
            Kind::Group => best(format!("{}_{}", self.map, self.group.as_deref()?)),
        })
    }

    /// Keeps the finished drive `gch` under `root` as RSF does: the latest, and each best it beats (a lower finish,
    /// or no file yet). Returns the files written.
    pub fn store(&self, root: &Path, gch: &Gch) -> Result<Vec<PathBuf>, String> {
        let bytes = gch.write();
        let mut written = Vec::new();
        for kind in Kind::ALL {
            let Some(path) = self.path(root, kind) else { continue };
            let beats = kind == Kind::Latest || Gch::read_header(&path).is_none_or(|stored| gch.finish < stored.finish);
            if !beats {
                continue;
            }
            let made = path.parent().map_or(Ok(()), std::fs::create_dir_all);
            made.and_then(|()| std::fs::write(&path, &bytes)).map_err(|e| format!("{}: {e}", path.display()))?;
            written.push(path);
        }
        Ok(written)
    }

    /// The ghost of `kind` to race from the `GhostCars` folders `roots` (ours first, then RSF's install, read only),
    /// with its header: a best kind's fastest, the latest drive's newest file.
    pub fn pick(&self, roots: &[&Path], kind: Kind) -> Option<(PathBuf, Gch)> {
        let found = roots.iter().filter_map(|root| {
            let path = self.path(root, kind)?;
            let gch = Gch::read_header(&path)?;
            Some((path, gch))
        });
        match kind {
            Kind::Latest => found.max_by_key(|(p, _)| std::fs::metadata(p).and_then(|m| m.modified()).ok()),
            _ => found.min_by(|a, b| a.1.finish.total_cmp(&b.1.finish)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(car: u32, finish: f32) -> Gch {
        let sample =
            |time| GchSample { time, distance: 2.0, clock: 0.0, rotation: [0.0, 0.0, 0.0, 1.0], position: [1.0; 3] };
        Gch {
            map: 450,
            slot: 5,
            car,
            words: [0, 1, 0],
            splits: [70.0, 200.0],
            finish,
            penalty: 10.0,
            samples: vec![sample(9), sample(110)],
            trailer: [0; 16],
        }
    }

    #[test]
    fn a_gch_round_trips() {
        let gch = drive(94, 305.7);
        let bytes = gch.write();
        assert_eq!(bytes.len(), GCH_HEADER_LEN + 2 * SAMPLE_LEN + TRAILER_LEN);
        assert_eq!(Gch::parse(&bytes), Ok(gch));
        assert!(Gch::parse(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn drives_are_kept_and_picked_by_rsf_layout() {
        let root = std::env::temp_dir().join(format!("rsf-ghosts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let key = |car, group: &str| Key {
            map: 450,
            condition: Condition::Damp,
            car: Some(car),
            group: Some(group.to_owned()),
        };
        let names = |written: Vec<PathBuf>| -> Vec<String> {
            written.iter().map(|p| p.strip_prefix(&root).unwrap().display().to_string()).collect()
        };
        let sep = std::path::MAIN_SEPARATOR;
        let all = |car: &str, group: &str| {
            vec![
                format!("450{sep}Damp{sep}450.gch"),
                format!("450{sep}Damp{sep}450_{car}.gch"),
                format!("450{sep}Damp{sep}450_{group}.gch"),
                format!("450{sep}450_latest.gch"),
            ]
        };
        assert_eq!(names(key(9, "WRC 1.6").store(&root, &drive(9, 310.0)).unwrap()), all("9", "WRC 1.6"));
        // A slower car of another group: its own and its group's best, and the latest.
        let slower = key(88, "Rally 3").store(&root, &drive(88, 329.0)).unwrap();
        assert_eq!(names(slower), all("88", "Rally 3")[1..].to_vec());
        // Slower again in the same car: only the latest.
        assert_eq!(names(key(88, "Rally 3").store(&root, &drive(88, 340.0)).unwrap()), all("88", "Rally 3")[3..]);
        let rsf = std::env::temp_dir().join(format!("rsf-ghosts-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&rsf);
        key(80, "Rally 4").store(&rsf, &drive(80, 300.0)).unwrap();
        let pick = |k: &Key, kind| k.pick(&[&root, &rsf], kind).map(|(_, g)| (g.car, g.finish));
        let k = key(88, "Rally 3");
        assert_eq!(pick(&k, Kind::Overall), Some((80, 300.0)));
        assert_eq!(pick(&k, Kind::Car), Some((88, 329.0)));
        assert_eq!(pick(&k, Kind::Group), Some((88, 329.0)));
        assert_eq!(pick(&key(9, "Group B"), Kind::Group), None);
        assert_eq!(pick(&Key { condition: Condition::Dry, ..k.clone() }, Kind::Overall), None);
        assert_eq!(Key { car: None, group: None, ..k.clone() }.path(&root, Kind::Car), None);
        let _ = (std::fs::remove_dir_all(&root), std::fs::remove_dir_all(&rsf));
    }

    /// Every `.gch` of the install `RBR_RSF` reads and writes back byte for byte, and each condition's best overall is
    /// its folder's lowest finish.
    #[test]
    fn the_installs_ghosts_read_back() {
        let Some(rsf) = std::env::var_os("RBR_RSF").map(PathBuf::from) else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let mut files = Vec::new();
        let mut dirs = vec![rsf.join("GhostCars")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "gch") {
                    files.push(path);
                }
            }
        }
        if files.is_empty() {
            eprintln!("skipping: no GhostCars in {}", rsf.display());
            return;
        }
        for path in &files {
            let bytes = std::fs::read(path).unwrap();
            let gch = Gch::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(gch.write(), bytes, "{}", path.display());
            assert_eq!(gch.slot, super::super::SLOT);
            let name = path.file_stem().unwrap().to_str().unwrap();
            assert!(name.starts_with(&gch.map.to_string()), "{}", path.display());
            if name == gch.map.to_string() {
                let folder = path.parent().unwrap();
                let best = files
                    .iter()
                    .filter(|p| p.parent() == Some(folder))
                    .map(|p| Gch::read_header(p).unwrap().finish)
                    .fold(f32::INFINITY, f32::min);
                assert_eq!(gch.finish, best, "{}", path.display());
            }
        }
        eprintln!("{} RSF ghosts read", files.len());
    }
}

//! Whether another `NGP.dll` can be read like 7.4.778's: the DLL against the supported one, for `ngp-compat` and for
//! deciding to add a version to [`super::SUPPORTED`].
//!
//! What the decoders read ([`super::reads`]) is found in the candidate by its content, wherever it moved
//! ([`super::locate`]), the decoders ([`super::decode_with`]) run there, every table is compared with 7.4.778's and
//! the code they decode byte for byte, addresses aside. That does not say the physics is the same: changes elsewhere
//! in the DLL can be NGP code the Rust ports reproduce for 7.4.778, so a candidate whose reads are unchanged still
//! needs its drives replayed (`notes/ngp.md`, "Adding an NGP version").

use std::fmt::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::locate;
use super::{decode, decode_with, reads, Tables};
use crate::pe;

/// What a DLL is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub size: usize,
    pub sha256: String,
    /// The supported version it is, if it is one.
    pub supported: Option<&'static str>,
    /// The version resource's `FileVersion`, when there is one.
    pub file_version: Option<String>,
}

/// How the candidate's tables compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableCheck {
    /// Every table equal.
    Same,
    /// These tables differ.
    Differ(Vec<&'static str>),
    /// Something the decoders read is not in the candidate (it changed), or is in several places.
    NotFound(String),
    /// The decoders fail on what they found.
    Undecodable(String),
}

/// What it means for the candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The same file.
    Identical,
    /// What we read is unchanged; the DLL differs elsewhere (maybe nowhere we reproduce): replay its drives.
    ReadsUnchanged,
    /// What we read changed or is gone: the decoders or the ports need work for this DLL.
    ReadsChanged,
    /// The decoders fail on what they found: they need to be extended for this DLL.
    Unreadable,
}

/// The comparison of a candidate DLL with the reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub reference: Identity,
    pub candidate: Identity,
    pub tables: TableCheck,
    /// Where each range the decoders read is in the candidate: what it is, its address in 7.4.778 and here.
    pub located: Vec<(&'static str, u32, u32)>,
    /// What code the decoders read differs in the candidate.
    pub in_reads: Vec<&'static str>,
}

/// The UTF-16 string value after the key `key` in a version resource: the first text after the key's end.
pub(super) fn version_string(bytes: &[u8], key: &str) -> Option<String> {
    let key: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let at = bytes.windows(key.len()).position(|w| w == key)? + key.len();
    let words: Vec<u16> = bytes[at..].chunks_exact(2).take(80).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let text: Vec<u16> = words.into_iter().skip_while(|&w| w == 0).take_while(|&w| w != 0).collect();
    (!text.is_empty()).then(|| String::from_utf16_lossy(&text))
}

/// What `bytes` is: its hash, whether it is a supported version, and its `FileVersion`.
pub fn identify(bytes: &[u8]) -> Identity {
    Identity {
        size: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        supported: super::version(bytes),
        file_version: version_string(bytes, "FileVersion"),
    }
}

/// The tables that differ between `a` and `b`, by name.
fn differing(a: &Tables, b: &Tables) -> Vec<&'static str> {
    let checks = [
        ("stage constants", a.stage == b.stage),
        ("tyre presets", a.tyres == b.tyres),
        ("keys", a.keys == b.keys),
        ("removed keys", a.removed == b.removed),
        ("engine defaults", a.engine_defaults == b.engine_defaults),
        ("probe defaults", a.probe_defaults == b.probe_defaults),
        ("temperatures", a.temperatures == b.temperatures),
        ("tyre ambient", a.tyre_ambient == b.tyre_ambient),
        ("tyre temperature offsets", a.temperature_offsets == b.temperature_offsets),
        ("atmosphere", a.atmosphere == b.atmosphere),
        ("road surfaces", a.road_surfaces == b.road_surfaces),
    ];
    checks.into_iter().filter(|(_, same)| !same).map(|(name, _)| name).collect()
}

/// Compares `candidate` with `reference`, which must be a supported DLL.
pub fn compare(reference: &[u8], candidate: &[u8]) -> Result<Report, String> {
    super::validate(reference).map_err(|e| format!("the reference DLL: {e}"))?;
    let expected = decode(reference).map_err(|e| format!("the reference DLL: {e}"))?;
    let (reference_id, candidate_id) = (identify(reference), identify(candidate));
    let (a, b) = (
        pe::parse(reference).map_err(|e| format!("the reference DLL: {e}"))?,
        pe::parse(candidate).map_err(|e| format!("the candidate DLL: {e}"))?,
    );
    let rebase = locate::locate(&a, &b);
    let (tables, located) = match &rebase {
        Err(e) => (TableCheck::NotFound(e.clone()), Vec::new()),
        Ok(rebase) => {
            let located = reads().iter().filter_map(|r| Some((r.what, r.start, rebase.at(r.start).ok()?))).collect();
            let check = match decode_with(candidate, rebase) {
                Ok(found) => match differing(&expected, &found)[..] {
                    [] => TableCheck::Same,
                    ref names => TableCheck::Differ(names.to_vec()),
                },
                Err(e) => TableCheck::Undecodable(e),
            };
            (check, located)
        }
    };
    let in_reads = rebase.as_ref().map_or_else(|_| Vec::new(), |rebase| locate::changed_code(&a, &b, rebase));
    Ok(Report { reference: reference_id, candidate: candidate_id, tables, located, in_reads })
}

impl Report {
    pub fn verdict(&self) -> Verdict {
        if self.reference.sha256 == self.candidate.sha256 {
            Verdict::Identical
        } else if matches!(self.tables, TableCheck::Undecodable(_)) {
            Verdict::Unreadable
        } else if self.tables != TableCheck::Same || !self.in_reads.is_empty() {
            Verdict::ReadsChanged
        } else {
            Verdict::ReadsUnchanged
        }
    }

    /// The report for the player or the developer: identity, tables, code we read, and what to do.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (name, id) in [("reference", &self.reference), ("candidate", &self.candidate)] {
            let version = id.supported.map_or("not a supported version".to_owned(), |v| format!("supported NGP {v}"));
            let _ = writeln!(out, "{name}: {} bytes, sha256 {}, {version}", id.size, id.sha256);
            let file = id.file_version.as_deref().map_or("no FileVersion".to_owned(), |v| format!("FileVersion {v}"));
            let _ = writeln!(out, "  {file}");
        }
        let _ = match &self.tables {
            TableCheck::Same => writeln!(out, "tables: found in the candidate, and every decoded table is equal"),
            TableCheck::Differ(names) => {
                writeln!(out, "tables: found in the candidate, but they differ in {}", names.join(", "))
            }
            TableCheck::NotFound(e) => writeln!(out, "tables: {e}"),
            TableCheck::Undecodable(e) => writeln!(out, "tables: found, but the decoders fail on them: {e}"),
        };
        if !self.located.is_empty() {
            let mut shifts: std::collections::BTreeMap<i64, usize> = Default::default();
            for &(_, old, here) in &self.located {
                *shifts.entry(i64::from(here) - i64::from(old)).or_default() += 1;
            }
            let list: Vec<String> = shifts.iter().map(|(d, n)| format!("{d:+#x} ({n})")).collect();
            let _ = writeln!(out, "located: {} ranges we read, moved by {}", self.located.len(), list.join(", "));
        }
        let differs = if self.in_reads.is_empty() { "none".to_owned() } else { self.in_reads.join(", ") };
        let _ = writeln!(out, "code we read that differs: {differs}");
        let verdict = match self.verdict() {
            Verdict::Identical => "same file: supported if it is in the supported list",
            Verdict::ReadsUnchanged => {
                "everything we read is unchanged (wherever it moved); the other changes are NGP code or data we do \
                 not read, which the Rust ports may or may not reproduce: replay drives made with this version \
                 before adding it (notes/ngp.md, Adding an NGP version)"
            }
            Verdict::ReadsChanged => "what we read changed or is gone: the decoders or ports need work for this DLL",
            Verdict::Unreadable => "the decoders fail on what they found: they need extending for this DLL",
        };
        let _ = writeln!(out, "verdict: {verdict}");
        out
    }
}

/// The `PhysicsRevision_NGP` of the `physics.rbz` at `path` (`Physics/physics.lsp`): 778 for 7.4.778.
pub fn physics_revision(path: &Path) -> Result<Option<u32>, String> {
    use std::io::Read;
    let mut zip = crate::rbz::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut text = String::new();
    crate::rbz::by_name(&mut zip, "Physics/physics.lsp")
        .map_err(|e| format!("{}: {e}", path.display()))?
        .read_to_string(&mut text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut words = text.split_whitespace().skip_while(|w| *w != "PhysicsRevision_NGP");
    Ok(words.nth(1).and_then(|n| n.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Data;

    /// The supported DLL's bytes (`RBR_NGP_DLL`, or the data's), none without one.
    fn supported() -> Option<Vec<u8>> {
        let data = Data::from_env().unwrap_or_else(|| Data::new("", None));
        std::fs::read(super::super::path(&data)?).ok()
    }

    #[test]
    fn the_version_resource_is_read_past_its_padding() {
        let wide = |s: &str| s.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
        let mut bytes = b"junk".to_vec();
        bytes.extend(wide("FileVersion"));
        bytes.extend([0, 0, 0, 0]);
        bytes.extend(wide("7.5.779.0"));
        bytes.extend([0, 0]);
        bytes.extend(wide("ProductVersion"));
        assert_eq!(version_string(&bytes, "FileVersion").as_deref(), Some("7.5.779.0"));
        assert_eq!(version_string(&bytes, "Missing"), None);
        assert_eq!(identify(b"not a dll").file_version, None);
    }

    #[test]
    fn the_reference_must_be_a_supported_dll_and_the_candidate_a_pe() {
        assert!(compare(b"x", b"y").unwrap_err().starts_with("the reference DLL: unsupported NGP.dll"));
        let Some(dll) = supported() else { return };
        assert!(compare(&dll, b"not a pe").unwrap_err().starts_with("the candidate DLL: "));
    }

    #[test]
    fn a_dll_against_itself_is_identical() {
        let Some(dll) = supported() else { return };
        let report = compare(&dll, &dll).unwrap();
        assert_eq!(report.verdict(), Verdict::Identical);
        assert_eq!(report.tables, TableCheck::Same);
        assert!(report.in_reads.is_empty());
        assert_eq!(report.candidate.supported, Some("7.4.778"));
        assert!(report.render().contains("verdict: same file"));
    }

    /// A byte flipped in a table we read is told as changed reads (the table is not found); one in code we decode as
    /// changed code and tables; one flipped where we read nothing as "reads unchanged".
    #[test]
    fn changes_are_told_by_where_they_are() {
        let Some(dll) = supported() else { return };
        let image = pe::parse(&dll).unwrap();
        let file_offset = |va: u32| {
            // Raw data is mapped in order, so the offset of `va` is found by searching the section's bytes.
            let section =
                image.sections.iter().find(|s| s.address <= va && va < s.address + s.bytes.len() as u32).unwrap();
            let at = (va - section.address) as usize;
            let needle = &section.bytes[at..at + 24];
            dll.windows(24).position(|w| w == needle).unwrap()
        };
        // A road surface byte: a table we read.
        let mut table = dll.clone();
        table[file_offset(0x10171280 + 5)] ^= 0x7f;
        let report = compare(&dll, &table).unwrap();
        assert_eq!(report.verdict(), Verdict::ReadsChanged);
        assert!(
            matches!(&report.tables, TableCheck::NotFound(e) if e.contains("road surfaces")),
            "{:?}",
            report.tables
        );
        // A byte of the stage initializer's code: a constant it sets.
        let mut code = dll.clone();
        code[file_offset(0x10050000)] ^= 0x7f;
        let report = compare(&dll, &code).unwrap();
        assert_ne!(report.verdict(), Verdict::ReadsUnchanged, "{}", report.render());
        assert!(report.in_reads.contains(&"stage initializer"), "{}", report.render());
        // The entry point's first byte of the last section's tail: bytes nothing reads.
        let text = image.sections.iter().find(|s| s.name == ".text").unwrap();
        let reads = super::super::reads();
        let free = (0..text.bytes.len() as u32)
            .step_by(0x100)
            .map(|o| text.address + o)
            .find(|va| !reads.iter().any(|r| *va + 24 > r.start && *va < r.end))
            .unwrap();
        let mut elsewhere = dll.clone();
        elsewhere[file_offset(free)] ^= 0xff;
        let report = compare(&dll, &elsewhere).unwrap();
        assert_eq!(report.verdict(), Verdict::ReadsUnchanged, "{}", report.render());
        assert!(report.in_reads.is_empty(), "{}", report.render());
        assert!(report.render().contains("replay drives"));
        // A truncated file: a section outside the file.
        let short = &dll[..dll.len() / 2];
        assert!(compare(&dll, short).is_err() || compare(&dll, short).unwrap().verdict() == Verdict::Unreadable);
    }
}

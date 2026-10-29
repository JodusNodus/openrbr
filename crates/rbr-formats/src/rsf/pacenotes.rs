//! Pace-note files of an RSF install's Pacenote plugin (`notes/formats.md` "Pacenote plugin"): a stage's own notes
//! saved as INI (the plugin's "BTB" format, NGPCarMenu's MyPacenotes), and where MyPacenotes keeps them.
//!
//! A note file is `[PACENOTES] count = N` and `[P<k>]` sections with `type`, `distance` and `flag`, the three words of
//! the game's pace-note record (`type`, `flags`, `distance`, as the `.dls` `Driveline` keys; NGPCarMenu `RBRAPI.h`).
//! The plugin reads more into the words than 1.02 (read in `PaceNote.dll`):
//!
//! - `type` bits 0-15 are the call's id, bits 16-31 a signed value: the take of a `SOUND_INDEX` call, a `CALLOUT_*`
//!   setting in hundredths, or two characters of a `PACENOTES_FILE`/`RANGES_FILE` name;
//! - `flag` bit 0x4000 puts the id in bits 19-30 (the `type` then holds the call's 1.02 stand-in, the `standard=` of
//!   its definition), bit 3 a take in bits 16-18; bit 0x100 keeps a setting call for the notes after the next one
//!   (sticky) and on a spoken call means no `INTO`/`AND` from the gap to the next call; bits 0-13 as in 1.02
//!   (`NARROWS`, `WIDEOUT`, `LONG`, ...).
//!
//! MyPacenotes (`Plugins/NGPCarMenu.ini` `MyPacenotesPath`, default `Plugins\NGPCarMenu\MyPacenotes`) has a folder per
//! stage, named as RSF's stage list names it, and takes the note file modified last.

use std::path::{Path, PathBuf};

use crate::ini;
use crate::stage_files;

/// `flag` bit: the id is in bits 19-30.
pub const EXTENDED_ID: u32 = 0x4000;
/// `flag` bit: a take is in bits 16-18.
pub const HAS_TAKE: u32 = 0x8;
/// `flag` bit: a setting call stays (sticky); a spoken call takes no link from its gap.
pub const STICKY: u32 = 0x100;

/// The plugin's own call ids (`PaceNoteIds::insertLegacy`).
pub mod id {
    pub const DISTANCE_CALL: u32 = 13;
    pub const START: u32 = 21;
    pub const FINISH: u32 = 22;
    pub const SPLIT: u32 = 23;
    pub const END_OF_TRACK: u32 = 24;
    pub const CALLOUT_ADJUST: u32 = 0xfea;
    pub const EMPTY_CALL: u32 = 0xfeb;
    pub const PLACEHOLDER: u32 = 0xfec;
    pub const CAUTION_WATER: u32 = 0xfed;
    pub const SOUND_FILE: u32 = 0xfee;
    pub const SOUND_INDEX: u32 = 0xfef;
    pub const ONTO: u32 = 0xff2;
    pub const INTO: u32 = 0xff3;
    pub const AND: u32 = 0xff4;
    pub const CALLOUT_TIME: u32 = 0xff5;
    pub const CALLOUT_DISTANCE: u32 = 0xff6;
    pub const STANDARD_CALL: u32 = 0xff7;
    pub const TIGHTENS: u32 = 0xff8;
    pub const TIGHTENSBAD: u32 = 0xff9;
    pub const CUT: u32 = 0xffa;
    pub const DONTCUT: u32 = 0xffb;
    pub const LONG: u32 = 0xffc;
    pub const MAYBE: u32 = 0xffd;
    pub const RANGES_FILE: u32 = 0xffe;
    pub const PACENOTES_FILE: u32 = 0xfff;
}

/// The names of the plugin's calls without an `id=` in the definitions: a definition names them.
pub const NAMED: [(&str, u32); 19] = [
    ("EMPTY_CALL", id::EMPTY_CALL),
    ("DISTANCE_CALL", id::DISTANCE_CALL),
    ("SOUND_INDEX", id::SOUND_INDEX),
    ("PLACEHOLDER", id::PLACEHOLDER),
    ("PLACE_HOLDER", id::PLACEHOLDER),
    ("AND", id::AND),
    ("INTO", id::INTO),
    ("ONTO", id::ONTO),
    ("CALLOUT_TIME", id::CALLOUT_TIME),
    ("CALLOUT_DISTANCE", id::CALLOUT_DISTANCE),
    ("CALLOUT_ADJUST", id::CALLOUT_ADJUST),
    ("TIGHTENS", id::TIGHTENS),
    ("TIGHTENSBAD", id::TIGHTENSBAD),
    ("CUT", id::CUT),
    ("DONTCUT", id::DONTCUT),
    ("LONG", id::LONG),
    ("MAYBE", id::MAYBE),
    ("CAUTION_WATER", id::CAUTION_WATER),
    ("STANDARD_CALL", id::STANDARD_CALL),
];

/// The 1.02 modifier bits of `flag` and the call each says (`CPaceNoteHandler`'s flag map in the plugin: 0x10, 0x800
/// and 0x1000 name none). The first set bit in this order is said.
pub const MODIFIERS: [(u32, &str); 11] = [
    (0x1, "NARROWS"),
    (0x2, "WIDEOUT"),
    (0x4, "TIGHTENS"),
    (0x10, ""),
    (0x20, "DONTCUT"),
    (0x40, "CUT"),
    (0x80, "TIGHTENSBAD"),
    (0x400, "LONG"),
    (0x800, ""),
    (0x1000, ""),
    (0x2000, "MAYBE"),
];

/// One note: the game's record.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Note {
    pub kind: u32,
    pub flags: u32,
    pub distance: f32,
}

impl Note {
    /// The call's id: from `flag` bits 19-30 with [`EXTENDED_ID`], else `type` bits 0-15.
    pub fn id(&self) -> u32 {
        if self.flags & EXTENDED_ID != 0 {
            self.flags >> 19 & 0xfff
        } else {
            self.kind & 0xffff
        }
    }

    /// `type` bits 16-31, signed.
    pub fn value(&self) -> i16 {
        (self.kind >> 16) as i16
    }

    /// The take the note names: a `SOUND_INDEX` call's value, else `flag` bits 16-18 with [`HAS_TAKE`].
    pub fn take(&self) -> Option<i32> {
        if self.flags & HAS_TAKE == 0 {
            None
        } else if self.id() == id::SOUND_INDEX {
            Some(i32::from(self.value()))
        } else {
            Some((self.flags >> 16 & 7) as i32)
        }
    }

    pub fn sticky(&self) -> bool {
        self.flags & STICKY != 0
    }

    /// The 1.02 modifier the note's flags say, if any ([`MODIFIERS`]).
    pub fn modifier(&self) -> Option<&'static str> {
        MODIFIERS.iter().find(|(bit, _)| self.flags & bit != 0).map(|&(_, name)| name).filter(|n| !n.is_empty())
    }
}

impl From<crate::markers::MarkerKey> for Note {
    /// A `.dls` `Driveline` key: the same three words.
    fn from(key: crate::markers::MarkerKey) -> Note {
        Note { kind: key.tag, flags: key.word1, distance: key.distance }
    }
}

/// Parses a note file. Notes come back sorted by distance (the game sorts its list).
pub fn parse(text: &str) -> Result<Vec<Note>, String> {
    let sections = ini::parse(text).map_err(|e| e.to_string())?;
    let count = sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case("PACENOTES"))
        .and_then(|s| s.get("count"))
        .ok_or("no [PACENOTES] count")?
        .parse::<usize>()
        .map_err(|e| format!("[PACENOTES] count: {e}"))?;
    // No reservation: `count` comes from the file and can be `usize::MAX`; the loop below fails at the first
    // missing section.
    let mut notes = Vec::new();
    for k in 0..count {
        let name = format!("P{k}");
        let section = sections.iter().find(|s| s.name == name).ok_or_else(|| format!("no [{name}]"))?;
        let word = |key: &str| -> Result<u32, String> {
            let v = section.get(key).ok_or_else(|| format!("[{name}] has no {key}"))?;
            v.parse::<i64>().map(|n| n as u32).map_err(|e| format!("[{name}] {key}: {e}"))
        };
        let distance = section
            .get("distance")
            .ok_or_else(|| format!("[{name}] has no distance"))?
            .parse::<f64>()
            .map_err(|e| format!("[{name}] distance: {e}"))?;
        notes.push(Note { kind: word("type")?, flags: word("flag")?, distance: distance as f32 });
    }
    notes.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    Ok(notes)
}

/// Reads and parses a note file.
pub fn read(path: &Path) -> Result<Vec<Note>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&String::from_utf8_lossy(&bytes)).map_err(|e| format!("{}: {e}", path.display()))
}

/// The file name the notes' `PACENOTES_FILE` or `RANGES_FILE` calls spell (`call`), two characters a call.
pub fn named_file(notes: &[Note], call: u32) -> Option<String> {
    let bytes: Vec<u8> = notes
        .iter()
        .filter(|n| n.kind & 0xffff == call)
        .flat_map(|n| [(n.kind >> 16) as u8, (n.kind >> 24) as u8])
        .take_while(|&b| b != 0)
        .collect();
    (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// The MyPacenotes folder of an RSF install: `MyPacenotesPath` of `Plugins/NGPCarMenu.ini` (relative to the install
/// or absolute), else NGPCarMenu's default. `None` when it is not there or the feature is disabled.
pub fn my_pacenotes_dir(root: &Path) -> Option<PathBuf> {
    let configured = stage_files::in_dir(root, "Plugins\\NGPCarMenu.ini")
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| {
            let sections = ini::parse(&String::from_utf8_lossy(&bytes)).ok()?;
            sections.iter().find_map(|s| s.get("MyPacenotesPath")).map(str::to_owned)
        });
    let path = configured.unwrap_or_else(|| "Plugins\\NGPCarMenu\\MyPacenotes".to_owned());
    if path.eq_ignore_ascii_case("disabled") {
        return None;
    }
    let absolute = Path::new(&path);
    if absolute.is_absolute() && absolute.is_dir() {
        return Some(absolute.to_owned());
    }
    stage_files::in_dir(root, &path).filter(|p| p.is_dir())
}

/// The note file MyPacenotes plays on the stage RSF calls by one of `names`: in the stage's folder (its name matched
/// ignoring case, accents and punctuation: the folders replace characters a file name cannot have), the `.ini`
/// modified last. Ours on a tie (files unpacked together): a file other than NGPCarMenu's `_default` export, then the
/// name.
pub fn my_pacenotes(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    let wanted: Vec<String> = names.iter().map(|n| fold(n)).filter(|n| !n.is_empty()).collect();
    let folder = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_str().is_some_and(|n| wanted.contains(&fold(n))))?
        .path();
    std::fs::read_dir(folder)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_str().is_some_and(|n| n.to_ascii_lowercase().ends_with(".ini")))
        .filter_map(|e| {
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            let name = e.file_name().to_string_lossy().into_owned();
            Some((modified, !name.to_ascii_lowercase().ends_with("_default.ini"), name, e.path()))
        })
        .max_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then_with(|| b.2.cmp(&a.2)))
        .map(|(.., path)| path)
}

/// A name for matching: lowercase ASCII letters and digits, accented Latin letters as their base letter.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_parse_sorted_with_their_words() {
        let text = "[PACENOTES]\ncount = 3\n\n[P0]\ntype = 6885375\ndistance = 4e-05\nflag = 0\n\n\
                    [P1]\ntype = 16\ndistance = 1220.8999999999999\nflag = 1189626112\n\n\
                    [P2]\ntype = 4084\ndistance = 545.4\nflag = 256\n";
        let notes = parse(text).unwrap();
        assert_eq!(notes.iter().map(Note::id).collect::<Vec<_>>(), [0xfff, id::AND, 2269]);
        assert_eq!(notes[1].distance, 545.4);
        assert!(notes[1].sticky());
        // OVERCREST (16) standing in for OVERCREST_NOLINK (2269).
        assert_eq!(notes[2].kind, 16);
    }

    #[test]
    fn a_huge_count_is_an_error() {
        // `usize::MAX` notes would overflow the reservation; the file ends after the header.
        let text = "[PACENOTES]\ncount = 18446744073709551615\n";
        assert!(parse(text).is_err());
    }

    #[test]
    fn takes_and_modifiers() {
        let take = Note { kind: 3, flags: HAS_TAKE | 2 << 16 | 0x400, distance: 0.0 };
        assert_eq!(take.take(), Some(2));
        assert_eq!(take.modifier(), Some("LONG"));
        let index = Note { kind: 0x0003_0fef, flags: HAS_TAKE, distance: 0.0 };
        assert_eq!(index.take(), Some(3));
        // 0x10 comes first and says nothing.
        assert_eq!(Note { kind: 3, flags: 0x30, distance: 0.0 }.modifier(), None);
        assert_eq!(Note { kind: 3, flags: 0, distance: 0.0 }.take(), None);
    }

    #[test]
    fn file_names_are_spelt_two_characters_a_call() {
        let notes: Vec<Note> = [0x6252_0fff, 0x6252_0ffe, 0x2e72_0fff, 0x6e69_0fff, 0x0069_0fff, 21]
            .into_iter()
            .map(|kind| Note { kind, flags: 0, distance: 0.0 })
            .collect();
        assert_eq!(named_file(&notes, id::PACENOTES_FILE).as_deref(), Some("Rbr.ini"));
        assert_eq!(named_file(&notes, id::RANGES_FILE).as_deref(), Some("Rb"));
        assert_eq!(named_file(&notes[5..], id::PACENOTES_FILE), None);
    }

    #[test]
    fn stage_names_fold() {
        assert_eq!(fold("Bruchsal Unteröwisheim"), fold("Bruchsal Unterowisheim"));
        assert_eq!(fold("Cote D'Arbroz"), fold("Cote D_Arbroz"));
        assert_ne!(fold("RSI 1"), fold("RSI I"));
    }
}

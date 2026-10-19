//! Stage gameplay markers: start line, split and finish distances, end-of-stage
//! and pace-note callouts.
//!
//! They are not in `.trk`, `.col` or the stage `.ini`/Lua files. They are keys
//! of the `Driveline` animation set in the stage's `.dls` (AnimData) file:
//!
//! - `FUN_00479020` (`0x479020`) builds, for each animation, a key list of
//!   12-byte records `[u32 tag, u32 word1, f32 distance]`, sorted by distance
//!   with `FUN_00474760`. In the record table, this is count word 6 and
//!   offset word 14 (3-word records); see `DlsAnimationEntry::arrays[4]`.
//! - `NRegulation::CTimeKeeper::Load` (`0x4a01f0`) reads tags `0x15`, `0x16`
//!   and `0x17` from the `Driveline` set; the E12 tic `FUN_004783b0`
//!   (`0x4783b0`) dispatches every tag as the car passes it.
//! - The distance is the driveline's arc coordinate: the same domain as the
//!   `value` channel of the `.trk` kind-`0x14` driveline records.
//!
//! See `notes/formats.md` "Markers" for the evidence and the open tags.

use crate::dls::{self, DlsAnimationEntry, DlsFile};
use std::fmt::{self, Display, Formatter};

/// Start line (`CTimeKeeper::Load` reads it into `m_f30`).
pub const TAG_START: u32 = 0x15;
/// Finish line (`CTimeKeeper::Load` reads it into `m_f48`).
pub const TAG_FINISH: u32 = 0x16;
/// Split/checkpoint line (`CTimeKeeper::Load` pushes it, at most 16).
pub const TAG_SPLIT: u32 = 0x17;
/// End of stage: the tic emits `CSigEndRace` after the pace note.
pub const TAG_END: u32 = 0x18;
/// Most split keys the timekeeper keeps (`CTimeKeeper::Load` stops at 16).
pub const MAX_SPLITS: usize = 16;
/// Animation set name the loader selects with `_stricmp` (`FUN_004789b0`).
pub const SET_NAME: &str = "Driveline";
/// Index of the key list in `DlsAnimationEntry::arrays` (count word 6, offset word 14).
pub const KEY_ARRAY: usize = 4;

/// Index of the trigger data `{id, kind}` in `DlsAnimationEntry::arrays` (count word 1, offset word 9).
pub const TRIGGER_DATA_ARRAY: usize = 0;
/// Index of the animation channels `{trigger data index, distance, lead}` (count word 3, offset word 11).
pub const ANIMATION_CHANNEL_ARRAY: usize = 2;

/// One animation channel of the `Driveline` set: a stage character's trigger point. `E12_AnimSet::Load020`
/// (`0x479020`) points each row at its trigger-data record `{id, kind}` and keeps the distance and the lead time;
/// `E12_AnimSet::TrigLoop` (`0x477ff0`) emits `CSigTrigger` with the record's id once, when
/// `distance - lead * v < x` (`x` the car's driveline distance, `v` its speed along the driveline). The id is the
/// `.lbs` `(2, 14)` object the signal triggers (`E02TrigLoader::Fire`, `0x443d00`; `notes/renderer.md` "Characters",
/// section 4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationTrigger {
    /// The `(2, 14)` object id.
    pub object: u32,
    /// The trigger-data record's second word (0 in retail stages checked).
    pub kind: u32,
    /// Driveline distance of the trigger point, m.
    pub distance: f32,
    /// Lead time, s: the trigger fires this long before the car reaches `distance` at its current speed.
    pub lead: f32,
}

/// The animation channels of a stage's `Driveline` set, in file order. Rows whose trigger-data index is out of
/// range are dropped (none in retail).
pub fn animation_triggers(file: &DlsFile<'_>) -> Result<Vec<AnimationTrigger>, Error> {
    let entry = driveline_entry(file)?;
    let (Some(data), Some(channels)) =
        (entry.arrays.get(TRIGGER_DATA_ARRAY), entry.arrays.get(ANIMATION_CHANNEL_ARRAY))
    else {
        return Err(Error::NoDrivelineSet);
    };
    let records: Vec<&[u32]> = data.words.chunks_exact(2).collect();
    Ok(channels
        .words
        .chunks_exact(3)
        .filter_map(|row| {
            let record = records.get(row[0] as usize)?;
            Some(AnimationTrigger {
                object: record[0],
                kind: record[1],
                distance: f32::from_bits(row[1]),
                lead: f32::from_bits(row[2]),
            })
        })
        .collect())
}

/// Index of the plain trigger keys `{trigger data index, distance, flags}` in `DlsAnimationEntry::arrays` (count word
/// 2, offset word 10).
pub const PLAIN_TRIGGER_ARRAY: usize = 1;

/// One plain trigger key of the `Driveline` set. `E12_AnimSet::Load3b0` (`0x4783b0`) walks them in distance order as
/// the car's driveline distance `x` passes them: forward, a key at or before `x` emits `CSigTrigger` with the record's
/// id, and with bit 0 of `flags` set the signal is marked (`+0xc` = 1) so that `E02TrigLoader::Fire` ignores it;
/// backward (`x` back before the key) the mark is the reverse. The bird strikes are triggered this way: no animation
/// channel names them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlainTrigger {
    /// The `(2, 14)` object id.
    pub object: u32,
    pub kind: u32,
    pub distance: f32,
    pub flags: u32,
}

impl PlainTrigger {
    /// Whether the key triggers its object when the car passes it going forward.
    pub fn forward(&self) -> bool {
        self.flags & 1 == 0
    }
}

/// The plain trigger keys of a stage's `Driveline` set, sorted by distance (the loader's `qsort`; equal distances in
/// file order here). Rows whose trigger-data index is out of range are dropped.
pub fn plain_triggers(file: &DlsFile<'_>) -> Result<Vec<PlainTrigger>, Error> {
    let entry = driveline_entry(file)?;
    let (Some(data), Some(keys)) = (entry.arrays.get(TRIGGER_DATA_ARRAY), entry.arrays.get(PLAIN_TRIGGER_ARRAY)) else {
        return Err(Error::NoDrivelineSet);
    };
    let records: Vec<&[u32]> = data.words.chunks_exact(2).collect();
    let mut triggers: Vec<PlainTrigger> = keys
        .words
        .chunks_exact(3)
        .filter_map(|row| {
            let record = records.get(row[0] as usize)?;
            Some(PlainTrigger { object: record[0], kind: record[1], distance: f32::from_bits(row[1]), flags: row[2] })
        })
        .collect();
    triggers.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    Ok(triggers)
}

/// The trigger `E62GameMenu::M573d90` (0x573d90) fires by name once the start countdown falls below 6 s: the
/// countdown marshal's (`run_startrace01c`).
pub const START_ANIMATION: &str = "start_animation2";

/// One named trigger of a stage: a `.dls` table-9 record. `E12_AnimSet::Load020` (`0x479020`) keeps the records at
/// set `+0x38` (count `+0x34`) as `{name offset, id}` and the table-7 strings at `+0x3c`; `FindByName` (`0x4765c0`)
/// returns the id of the first record whose name equals the asked one (`_stricmp`), 0 when none does. `FireByIdx`
/// (`0x47a7d0`) then emits `CSigTrigger {id, ..., +0xc = 0}` (`FireTrig`, `0x478ac0`), and `E02TrigLoader::Fire`
/// (`0x443d00`) takes `+0` as the `(2, 14)` object and passes `isStart` 1. Retail names (86 files): `start_animation1`
/// and `start_animation2` (57 stages each), `end_animation0..2`, `bookmark0/1`, `end_race_camera`, `redbull_0..4` and
/// `redbull_cam` (the rally school).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedTrigger {
    pub name: String,
    /// The `(2, 14)` object id (the record's first word on disk).
    pub object: u32,
}

/// The named triggers of a `.dls` file, in file order. Records whose name offset lies outside the string table are
/// dropped (none in retail).
pub fn named_triggers(file: &DlsFile<'_>) -> Vec<NamedTrigger> {
    file.table_9
        .iter()
        .filter_map(|r| {
            let name = file.string_bytes.get(r.words[1] as usize..)?;
            let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            Some(NamedTrigger { name: String::from_utf8_lossy(&name[..end]).into_owned(), object: r.words[0] })
        })
        .collect()
}

/// `FindByName`: the object of the first trigger named `name`, compared case-insensitively.
pub fn find_named(triggers: &[NamedTrigger], name: &str) -> Option<u32> {
    triggers.iter().find(|t| t.name.eq_ignore_ascii_case(name)).map(|t| t.object)
}

/// One key of the `Driveline` set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerKey {
    pub tag: u32,
    /// Second word of the record. Zero on every start, finish, split and end
    /// key in retail. Its meaning elsewhere is unknown.
    pub word1: u32,
    /// Distance along the driveline (metres in the retail stages).
    pub distance: f32,
}

/// The sorted key list of one stage's `Driveline` set.
#[derive(Debug, Clone, PartialEq)]
pub struct StageMarkers {
    /// All keys, in ascending distance order (the loader's `qsort` order).
    /// Keys with equal distance have no defined order.
    pub keys: Vec<MarkerKey>,
}

/// Failure to find the key list in a `.dls` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Dls(dls::Error),
    /// No animation entry is named `Driveline`.
    NoDrivelineSet,
    /// The set's name offset points outside the string table.
    BadName {
        offset: u32,
        len: usize,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Dls(e) => write!(f, "{e}"),
            Error::NoDrivelineSet => write!(f, "no `{SET_NAME}` animation set in .dls"),
            Error::BadName { offset, len } => {
                write!(f, "animation name offset {offset:#x} is outside the {len}-byte string table")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<dls::Error> for Error {
    fn from(e: dls::Error) -> Self {
        Error::Dls(e)
    }
}

impl StageMarkers {
    /// Distance of the start line. `None` when the set has no start key.
    pub fn start(&self) -> Option<f32> {
        self.first_of(TAG_START)
    }

    /// Distance of the finish line. `None` when the set has no finish key.
    pub fn finish(&self) -> Option<f32> {
        self.first_of(TAG_FINISH)
    }

    /// Distance of the end-of-stage key.
    pub fn end(&self) -> Option<f32> {
        self.first_of(TAG_END)
    }

    /// Split distances in ascending order, at most [`MAX_SPLITS`].
    pub fn splits(&self) -> impl Iterator<Item = f32> + '_ {
        self.keys.iter().filter(|k| k.tag == TAG_SPLIT).map(|k| k.distance).take(MAX_SPLITS)
    }

    /// Every other key: pace-note callouts, tags outside `0x15..=0x18`.
    pub fn pace_notes(&self) -> impl Iterator<Item = &MarkerKey> + '_ {
        self.keys.iter().filter(|k| !(TAG_START..=TAG_END).contains(&k.tag))
    }

    /// The loader keeps the last key of a tag in sorted order.
    fn first_of(&self, tag: u32) -> Option<f32> {
        self.keys.iter().rev().find(|k| k.tag == tag).map(|k| k.distance)
    }
}

/// Reads the marker keys from a `.dls` file.
pub fn parse(bytes: &[u8]) -> Result<StageMarkers, Error> {
    from_dls(&dls::parse(bytes)?)
}

/// Reads the marker keys from an already parsed `.dls` file.
pub fn from_dls(file: &DlsFile<'_>) -> Result<StageMarkers, Error> {
    let entry = driveline_entry(file)?;
    let array = entry.arrays.get(KEY_ARRAY).ok_or(Error::NoDrivelineSet)?;
    Ok(keys_from_words(&array.words))
}

/// Finds the `Driveline` entry. The loader compares the names
/// case-insensitively and takes the first match.
fn driveline_entry<'a>(file: &'a DlsFile<'_>) -> Result<&'a DlsAnimationEntry, Error> {
    for entry in &file.animation_entries {
        let offset = entry.fields[0];
        let name =
            file.string_bytes.get(offset as usize..).ok_or(Error::BadName { offset, len: file.string_bytes.len() })?;
        let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
        if name[..end].eq_ignore_ascii_case(SET_NAME.as_bytes()) {
            return Ok(entry);
        }
    }
    Err(Error::NoDrivelineSet)
}

/// Splits the flat key words into `[tag, word1, distance]` records and sorts
/// them by distance. A trailing partial record is ignored.
pub fn keys_from_words(words: &[u32]) -> StageMarkers {
    let mut keys: Vec<MarkerKey> =
        words.chunks_exact(3).map(|r| MarkerKey { tag: r[0], word1: r[1], distance: f32::from_bits(r[2]) }).collect();
    keys.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    StageMarkers { keys }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(tag: u32, distance: f32) -> [u32; 3] {
        [tag, 0, distance.to_bits()]
    }

    #[test]
    fn keys_are_sorted_by_distance_and_tagged() {
        let words: Vec<u32> = [
            key(0x17, 2000.0),
            key(0x15, 90.0),
            key(0x04, 500.0),
            key(0x16, 6000.0),
            key(0x17, 1000.0),
            key(0x18, 6100.0),
        ]
        .concat();
        let m = keys_from_words(&words);
        assert_eq!(m.start(), Some(90.0));
        assert_eq!(m.finish(), Some(6000.0));
        assert_eq!(m.end(), Some(6100.0));
        assert_eq!(m.splits().collect::<Vec<_>>(), [1000.0, 2000.0]);
        let notes: Vec<u32> = m.pace_notes().map(|k| k.tag).collect();
        assert_eq!(notes, [0x04]);
        assert!(m.keys.windows(2).all(|w| w[0].distance <= w[1].distance));
    }

    #[test]
    fn splits_are_capped_and_missing_markers_are_none() {
        let words: Vec<u32> = (0..20).flat_map(|i| key(TAG_SPLIT, i as f32 * 10.0)).collect();
        let m = keys_from_words(&words);
        assert_eq!(m.splits().count(), MAX_SPLITS);
        assert_eq!(m.start(), None);
        assert_eq!(m.finish(), None);
        assert_eq!(m.end(), None);
    }

    #[test]
    fn trailing_partial_record_is_ignored() {
        let mut words = key(TAG_START, 12.0).to_vec();
        words.extend([TAG_FINISH, 0]);
        let m = keys_from_words(&words);
        assert_eq!(m.keys.len(), 1);
        assert_eq!(m.start(), Some(12.0));
    }

    #[test]
    fn last_key_of_a_tag_wins() {
        let words: Vec<u32> = [key(TAG_START, 10.0), key(TAG_START, 20.0)].concat();
        assert_eq!(keys_from_words(&words).start(), Some(20.0));
    }

    fn set_file(name: &[u8], words: &[u32]) -> DlsFile<'static> {
        let mut string_bytes = name.to_vec();
        string_bytes.push(0);
        let array =
            dls::DlsRawArray { count: (words.len() / 3) as u32, offset: 0, record_words: 3, words: words.to_vec() };
        let empty = dls::DlsRawArray { count: 0, offset: 0, record_words: 2, words: Vec::new() };
        let entry = DlsAnimationEntry {
            pair: [0, 0],
            record_offset: 0,
            fields: [0; 17],
            arrays: vec![empty.clone(), empty.clone(), empty.clone(), empty.clone(), array, empty],
            nested_groups: Vec::new(),
            replay: dls::DlsReplayStream {
                offset: 0,
                header: [0; 7],
                ten_word_records: Vec::new(),
                six_word_records: Vec::new(),
                pairs: Vec::new(),
            },
        };
        DlsFile {
            version: dls::RETAIL_VERSION,
            minor_version: 0,
            table_offsets: [0; 10],
            animation_entries: vec![entry],
            track_groups: Vec::new(),
            table_2: Vec::new(),
            table_3: Vec::new(),
            emitters: Vec::new(),
            table_5: Vec::new(),
            spheres: Vec::new(),
            string_bytes,
            world_vector: None,
            table_9: Vec::new(),
            raw: &[],
        }
    }

    #[test]
    fn set_name_is_matched_case_insensitively() {
        let words = [key(TAG_START, 5.0), key(TAG_FINISH, 50.0)].concat();
        let m = from_dls(&set_file(b"DriveLine", &words)).unwrap();
        assert_eq!(m.start(), Some(5.0));
        assert_eq!(m.finish(), Some(50.0));
    }

    #[test]
    fn other_set_names_are_rejected() {
        let file = set_file(b"Chimney", &[]);
        assert_eq!(from_dls(&file), Err(Error::NoDrivelineSet));
    }

    #[test]
    fn named_triggers_resolve_their_names_and_the_first_match_wins() {
        let mut file = set_file(b"Driveline", &[]);
        file.string_bytes = b"Start_Animation2\0end_animation0\0".to_vec();
        file.table_9 = [[0xc9cd, 0], [0xc7c1, 17], [0x1234, 0], [0x99, 400]]
            .into_iter()
            .map(|words| dls::DlsTable9Record { words })
            .collect();
        let named = named_triggers(&file);
        assert_eq!(named.len(), 3, "the record past the string table is dropped");
        assert_eq!(named[1], NamedTrigger { name: "end_animation0".into(), object: 0xc7c1 });
        assert_eq!(find_named(&named, START_ANIMATION), Some(0xc9cd));
        assert_eq!(find_named(&named, "bookmark0"), None);
    }

    #[test]
    fn name_offset_outside_string_table_is_an_error() {
        let mut file = set_file(b"Driveline", &[]);
        file.animation_entries[0].fields[0] = 100;
        assert!(matches!(from_dls(&file), Err(Error::BadName { offset: 100, .. })));
    }
}

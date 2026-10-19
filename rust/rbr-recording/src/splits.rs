//! The split record a live stepping process writes after each damage record, before the scenery (`rbr-standalone
//! --live`; the viewer shows the split comparison and the finish panel from it). Ours: the game's HUD reads the
//! signal, not a record. Little-endian:
//!
//! ```text
//! u32 tag          'SPL1'
//! u32 length       the payload's bytes: 24 + 8 * count, plus 72 with the standing
//! u32 flags        bit 0: the finish line was passed; bit 1: the race is over (its end page);
//!                  bit 2: it ended retired (DNF); bit 3: the standing follows the splits
//! u32 count        the split keys on the stage
//! f32 elapsed      the stage's clock since the start, s (the time card's, no penalties)
//! f32 race_time    the race clock, s, penalties included (the stage time once finished)
//! f32 penalties    the penalties the race clock took, s
//! f32 reference    the finish time from times.ini, s (NaN: none)
//! count times:
//!   f32 time       the split's time, s from the start (NaN: not passed)
//!   f32 reference  the split's reference time from times.ini, s (NaN: none)
//! with bit 3, the standing (the stage record and the personal best, `rbr_gameplay::record`):
//!   u32 verdict    0 nothing, 1 the personal best was beaten, 2 the stage record was
//!   u32 flags      bit 0: the stage has a personal best
//!   the stage record: u32 car, f32 split 1, f32 split 2, f32 finish time, 32 bytes holder (NUL-terminated)
//!   the personal best: u32 car, f32 split 1, f32 split 2, f32 finish time (zeros without one)
//! ```
//!
//! It is one of the tagged records (`crate::live`), so a reader that does not know it skips it by its length.

use std::io;

/// The tag.
pub const SPL1: [u8; 4] = *b"SPL1";
/// The payload's size without the splits.
const HEAD_BYTES: usize = 24;
/// Bytes per split in the payload.
const SPLIT_BYTES: usize = 8;
/// Bytes of the standing.
const STANDING_BYTES: usize = 72;
/// Most split keys a stage has (the time card holds 16).
pub const MAX_SPLITS: usize = 16;
/// The flag bits: a finished stage, a race that is over, and one that ended retired.
const FINISHED: u32 = 1;
const OVER: u32 = 2;
const RETIRED: u32 = 4;
const STANDING: u32 = 8;

/// One split: its time once the car passed it, and its reference time.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Split {
    pub time: Option<f32>,
    pub reference: Option<f32>,
}

/// A stage record or a personal best: the car, the splits and the finish time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mark {
    pub car: u32,
    pub splits: [f32; 2],
    pub time: f32,
    /// The record's holder; empty for a personal best.
    pub holder: String,
}

/// Which message the result shows (`rbr_gameplay::record::Verdict`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Verdict {
    #[default]
    None,
    PersonalBest,
    StageRecord,
}

/// The stage record and the personal best the race is compared with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Standing {
    pub record: Mark,
    pub personal: Option<Mark>,
    pub verdict: Verdict,
}

/// One split record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Splits {
    pub finished: bool,
    /// The race is over: the game shows its end page and steps the car no more.
    pub over: bool,
    /// The race ended retired.
    pub retired: bool,
    pub elapsed: f32,
    pub race_time: f32,
    pub penalties: f32,
    pub reference_finish: Option<f32>,
    pub splits: Vec<Split>,
    /// The stage record and the personal best, when the stepping process has the saves.
    pub standing: Option<Standing>,
}

/// The NaN a missing value is written as.
pub(crate) fn value(x: Option<f32>) -> f32 {
    x.unwrap_or(f32::NAN)
}

/// The value a NaN stands for, `None`.
pub(crate) fn option(x: f32) -> Option<f32> {
    (!x.is_nan()).then_some(x)
}

/// A mark's words, with its holder's name when `named`.
fn put_mark(payload: &mut Vec<u8>, mark: &Mark, named: bool) {
    payload.extend_from_slice(&mark.car.to_le_bytes());
    for float in [mark.splits[0], mark.splits[1], mark.time] {
        payload.extend_from_slice(&float.to_le_bytes());
    }
    if named {
        let mut name = [0u8; 32];
        let n = mark.holder.len().min(31);
        name[..n].copy_from_slice(&mark.holder.as_bytes()[..n]);
        payload.extend_from_slice(&name);
    }
}

impl Splits {
    /// The record. Splits past [`MAX_SPLITS`] are not written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let count = self.splits.len().min(MAX_SPLITS);
        let length = HEAD_BYTES + SPLIT_BYTES * count + self.standing.as_ref().map_or(0, |_| STANDING_BYTES);
        let mut payload = Vec::with_capacity(length);
        let flags = [
            (self.finished, FINISHED),
            (self.over, OVER),
            (self.retired, RETIRED),
            (self.standing.is_some(), STANDING),
        ]
        .iter()
        .filter(|(set, _)| *set)
        .fold(0, |flags, (_, bit)| flags | bit);
        for word in [flags, count as u32] {
            payload.extend_from_slice(&word.to_le_bytes());
        }
        for float in [self.elapsed, self.race_time, self.penalties, value(self.reference_finish)] {
            payload.extend_from_slice(&float.to_le_bytes());
        }
        for split in &self.splits[..count] {
            payload.extend_from_slice(&value(split.time).to_le_bytes());
            payload.extend_from_slice(&value(split.reference).to_le_bytes());
        }
        if let Some(standing) = &self.standing {
            let verdict = standing.verdict as u32;
            payload.extend_from_slice(&verdict.to_le_bytes());
            payload.extend_from_slice(&u32::from(standing.personal.is_some()).to_le_bytes());
            put_mark(&mut payload, &standing.record, true);
            put_mark(&mut payload, &standing.personal.clone().unwrap_or_default(), false);
        }
        let mut out = Vec::with_capacity(8 + length);
        crate::live::write_record(&mut out, SPL1, &payload);
        out
    }

    /// Reads the record's payload (the bytes after its tag and length, `crate::live::read_records`).
    pub fn from_payload(payload: &[u8]) -> io::Result<Splits> {
        let length = payload.len();
        let bad = || io::Error::new(io::ErrorKind::InvalidData, format!("a split record of {length} bytes"));
        if length < HEAD_BYTES {
            return Err(bad());
        }
        let word = |i: usize| u32::from_le_bytes(payload[4 * i..4 * i + 4].try_into().unwrap());
        let float = |i: usize| f32::from_bits(word(i));
        let standing_bytes = if word(0) & STANDING != 0 { STANDING_BYTES } else { 0 };
        let body = length - HEAD_BYTES;
        if body < standing_bytes || !(body - standing_bytes).is_multiple_of(SPLIT_BYTES) {
            return Err(bad());
        }
        let count = (body - standing_bytes) / SPLIT_BYTES;
        if count > MAX_SPLITS {
            return Err(bad());
        }
        if word(1) as usize != count {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "a split record's count and length disagree"));
        }
        let splits =
            (0..count).map(|i| Split { time: option(float(6 + 2 * i)), reference: option(float(7 + 2 * i)) }).collect();
        let standing = (standing_bytes != 0).then(|| {
            let at = (HEAD_BYTES + SPLIT_BYTES * count) / 4;
            let mark = |at: usize, named: bool| Mark {
                car: word(at),
                splits: [float(at + 1), float(at + 2)],
                time: float(at + 3),
                holder: if named {
                    let name = &payload[4 * (at + 4)..4 * (at + 12)];
                    String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or_default()).into_owned()
                } else {
                    String::new()
                },
            };
            Standing {
                verdict: match word(at) {
                    1 => Verdict::PersonalBest,
                    2 => Verdict::StageRecord,
                    _ => Verdict::None,
                },
                record: mark(at + 2, true),
                personal: (word(at + 1) & 1 != 0).then(|| mark(at + 14, false)),
            }
        });
        Ok(Splits {
            finished: word(0) & FINISHED != 0,
            over: word(0) & OVER != 0,
            retired: word(0) & RETIRED != 0,
            elapsed: float(2),
            race_time: float(3),
            penalties: float(4),
            reference_finish: option(float(5)),
            splits,
            standing,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips_with_missing_values() {
        let record = Splits {
            finished: false,
            over: false,
            retired: false,
            elapsed: 61.5,
            race_time: 70.25,
            penalties: 10.0,
            reference_finish: Some(185.0),
            splits: vec![
                Split { time: Some(57.25), reference: Some(57.0) },
                Split { time: None, reference: None },
                Split { time: Some(120.0), reference: None },
            ],
            standing: None,
        };
        let bytes = record.to_bytes();
        assert_eq!(&bytes[..4], b"SPL1");
        assert_eq!(bytes[4..8], ((HEAD_BYTES + 3 * SPLIT_BYTES) as u32).to_le_bytes());
        let back = Splits::from_payload(&bytes[8..]).expect("read");
        assert_eq!(back, record);
    }

    #[test]
    fn a_finished_record_without_splits_is_read() {
        let record = Splits { finished: true, over: true, race_time: 5.0, ..Splits::default() };
        let back = Splits::from_payload(&record.to_bytes()[8..]).expect("read");
        assert!(back.finished && back.over && !back.retired);
        let retired = Splits { over: true, retired: true, ..Splits::default() };
        assert_eq!(Splits::from_payload(&retired.to_bytes()[8..]).expect("read"), retired);
        assert_eq!(back.splits, Vec::<Split>::new());
        assert_eq!(back.reference_finish, None);
    }

    #[test]
    fn the_standing_round_trips_and_a_missing_personal_best_stays_missing() {
        let mark = |car, time| Mark { car, splits: [time / 3.0, time / 2.0], time, holder: String::new() };
        let mut record = Splits {
            splits: vec![Split { time: Some(10.0), reference: None }],
            standing: Some(Standing {
                record: Mark { holder: "Richard Burns".into(), ..mark(2, 185.0) },
                personal: Some(mark(6, 190.5)),
                verdict: Verdict::PersonalBest,
            }),
            ..Splits::default()
        };
        let bytes = record.to_bytes();
        assert_eq!(bytes[4..8], ((HEAD_BYTES + SPLIT_BYTES + STANDING_BYTES) as u32).to_le_bytes());
        assert_eq!(Splits::from_payload(&bytes[8..]).expect("read"), record);
        record.standing.as_mut().unwrap().personal = None;
        record.standing.as_mut().unwrap().verdict = Verdict::StageRecord;
        assert_eq!(Splits::from_payload(&record.to_bytes()[8..]).expect("read"), record);
    }

    #[test]
    fn a_bad_length_is_an_error() {
        assert!(Splits::from_payload(&[0; 7]).is_err());
    }
}

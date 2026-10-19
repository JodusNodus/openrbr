//! Ghost car files: the shipped `Ghosts\*.gho` (in `Root/ghosts.rbz`) and the player's saved `GhostCars\*.ghc`.
//!
//! Both hold the sample stream `CGhostCarManager` records (`Record1` 0x492730, `Record2` 0x4928c0) and plays
//! (`Playback` 0x493650). The stream is a list of runs, each a word `kind | count << 16` and `count` samples:
//!
//! - kind 2, a key sample (20 bytes): the position as three `f32`, the time in 1/1024 s in 24 bits, the rotation
//!   quaternion `[x, y, z, w]` as four `i8` times 127.5, and a spare byte (`Record1` writes its 8-byte record from a
//!   7-byte stack buffer, so the byte is stack garbage);
//! - kind 1, a delta sample (10 bytes): the time, the position change in 1/64 m as three `i8`, the quaternion.
//!
//! A kind or count of 0 ends the playback (the header word `BeginWrite` 0x492ca0 writes ends a recording). The
//! bytes from there to the stream's end are kept as [`Stream::tail`]: `rse0.gho` ends in the debug heap's fill
//! (`fd fd fd fd ab ab ...`), which `Playback` would read as header words of an unknown kind and skip.
//!
//! `.gho` (`E17_ParseGhost` 0x491cc0, `E13LoadRace` 0x47cb90): the bytes `SOHGRACT` (the dwords `GHOS`, `TCAR`),
//! the map, the car (`Cars.ini` `Car%.2d`; 5, the Impreza 2003, in every `rb_*` file) in the low half of the next
//! word and the version 2 in its high half, the stream's byte count, 12 reserved bytes (`0xff` in every retail
//! file; the loader skips them) and the stream, which may be followed by bytes the loader does not read
//! ([`Ghost::after`]).
//!
//! `.ghc` (`CGhostCarManager::Save` 0x492eb0, read by `E18_ConnGhostMemory` 0x493d50; save-file kind 5,
//! `<name>_<map:02>.ghc`, not encrypted): 14 words, then the stream. The checksum (`Crc32::Run` 0x490c00, the
//! game's own table, [`crate::saves::checksum`]) over everything after the second word, the byte count after it, the tag
//! `TSHG`, the map, the car, the weather state of the race (`CWeatherDaemon`, `notes/weather.md`) and the stream's
//! byte count.

use std::fmt::{self, Display, Formatter};

use crate::saves;

/// `.gho` magic: the dwords `0x47484f53`, `0x54434152`.
pub const MAGIC: [u8; 8] = *b"SOHGRACT";
/// `.gho` version, the high half of the fourth word (both loaders reach `int3` on another).
pub const VERSION: u16 = 2;
/// `.gho` header bytes before the stream.
pub const HEADER_LEN: usize = 0x20;
/// `.ghc` tag, the third word (`0x47485354`).
pub const SAVED_TAG: [u8; 4] = *b"TSHG";
/// `.ghc` header bytes before the stream.
pub const SAVED_HEADER_LEN: usize = 0x38;
/// Sample time units per second.
pub const TIME_SCALE: f32 = 1024.0;
/// Delta position units per metre.
pub const DELTA_SCALE: f32 = 64.0;
/// Quaternion units per 1.0 (`Record1`, `Record2` write `(int)(q * 127.5)`; `Playback` reads `q * (1 / 127.5)`).
pub const QUAT_SCALE: f32 = 127.5;
/// Samples in one run at most: the count is 16 bits, and the recorder opens a new run at 0xffff.
pub const RUN_MAX: usize = 0xffff;

const KIND_DELTA: u16 = 1;
const KIND_KEY: u16 = 2;
const KEY_LEN: usize = 20;
const DELTA_LEN: usize = 10;

/// A full pose: where a run of deltas starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// 1/1024 s, 24 bits.
    pub time: u32,
    pub position: [f32; 3],
    /// `[x, y, z, w]` times [`QUAT_SCALE`].
    pub rotation: [i8; 4],
    /// The record's eighth byte, stack garbage in the game's recordings.
    pub spare: u8,
}

/// A pose relative to the previous sample's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delta {
    /// 1/1024 s, 24 bits.
    pub time: u32,
    /// Position change, 1/64 m.
    pub offset: [i8; 3],
    /// `[x, y, z, w]` times [`QUAT_SCALE`].
    pub rotation: [i8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    Key(Key),
    Delta(Delta),
}

impl Sample {
    /// 1/1024 s.
    pub fn time(&self) -> u32 {
        match self {
            Sample::Key(k) => k.time,
            Sample::Delta(d) => d.time,
        }
    }

    pub fn seconds(&self) -> f32 {
        self.time() as f32 / TIME_SCALE
    }

    fn kind(&self) -> u16 {
        match self {
            Sample::Key(_) => KIND_KEY,
            Sample::Delta(_) => KIND_DELTA,
        }
    }
}

/// The recorded samples, in the order played.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stream {
    pub samples: Vec<Sample>,
    /// The bytes from the ending header word on (empty when the runs fill the stream).
    pub tail: Vec<u8>,
}

/// A `.gho` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Ghost {
    /// The stage's map id (`rb_<map>.gho`).
    pub map: u32,
    /// The car's `Cars.ini` index.
    pub car: u16,
    pub reserved: [u8; 12],
    pub stream: Stream,
    /// The bytes past the stream's count, which the loader does not copy: 45 and 28 more deltas in `rb_41` and
    /// `rb_71`, left from a longer recording in the buffer the file was written from.
    pub after: Vec<u8>,
}

/// The weather words a `.ghc` keeps, in file order: the `CWeatherDaemon` getters `Save` calls (`notes/weather.md`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Weather {
    /// `M9d50`: the stage variant letter, 0-4 (`M`, `N`, `E`, `O`, `S`).
    pub track_letter: u32,
    /// `M9c90`: the cloud id, 0-3.
    pub cloud: u32,
    /// `M9360`: the weather id, 0-9.
    pub weather: u32,
    /// `M9cf0`: the cloud letter, 0-2.
    pub cloud_letter: u32,
    /// `Vf18`: the tyre wear, 0-2.
    pub wear: u32,
    /// `Vf1c`: the wetness, 0-2.
    pub wetness: u32,
    /// `E25AppObj::M9b80(0)`.
    pub amount: f32,
    /// `CWeatherDaemon` +0x3c, a float (meaning not traced). Only the replay's weather (`E19ApplyTweak`) and the RB
    /// challenge set it (`M9230`); the constructor's 0.0 otherwise. The harness's `StartGame` races have 15.0.
    pub unk3c: f32,
}

/// A `.ghc` file.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedGhost {
    pub map: u32,
    pub car: u32,
    pub weather: Weather,
    pub stream: Stream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Short {
        len: usize,
    },
    Magic,
    Version(u16),
    /// The stream's byte count runs past the file.
    Count {
        count: usize,
        available: usize,
    },
    Checksum {
        stored: u32,
        computed: u32,
    },
    /// The second word of a `.ghc` is not its length less 8.
    Length {
        stored: usize,
        file: usize,
    },
    /// A run of more than [`RUN_MAX`] samples cannot be written.
    Run,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Short { len } => write!(f, "{len} bytes is shorter than the header"),
            Error::Magic => f.write_str("not a ghost file (bad magic)"),
            Error::Version(v) => write!(f, "ghost version {v}, want {VERSION}"),
            Error::Count { count, available } => write!(f, "stream of {count} bytes, {available} in the file"),
            Error::Checksum { stored, computed } => write!(f, "checksum {stored:#010x}, computed {computed:#010x}"),
            Error::Length { stored, file } => write!(f, "length word {stored}, file has {file} after it"),
            Error::Run => f.write_str("a run is longer than 0xffff samples"),
        }
    }
}

impl std::error::Error for Error {}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn time24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}

fn i8s<const N: usize>(b: &[u8]) -> [i8; N] {
    std::array::from_fn(|i| b[i] as i8)
}

impl Stream {
    /// Reads the runs as `Playback` walks them, up to the first header word that ends it or does not fit.
    pub fn parse(bytes: &[u8]) -> Stream {
        let mut samples = Vec::new();
        let mut at = 0;
        while at + 4 <= bytes.len() {
            let head = word(bytes, at);
            let (kind, count) = (head as u16, (head >> 16) as usize);
            let len = match kind {
                KIND_KEY => KEY_LEN,
                KIND_DELTA => DELTA_LEN,
                _ => break,
            };
            if count == 0 || at + 4 + count * len > bytes.len() {
                break;
            }
            for b in bytes[at + 4..at + 4 + count * len].chunks_exact(len) {
                samples.push(match kind {
                    KIND_KEY => Sample::Key(Key {
                        time: time24(&b[12..]),
                        position: std::array::from_fn(|i| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap())),
                        rotation: i8s(&b[15..]),
                        spare: b[19],
                    }),
                    _ => Sample::Delta(Delta { time: time24(b), offset: i8s(&b[3..]), rotation: i8s(&b[6..]) }),
                });
            }
            at += 4 + count * len;
        }
        Stream { samples, tail: bytes[at..].to_vec() }
    }

    /// The runs as the recorder groups them: consecutive samples of one kind, at most [`RUN_MAX`] each.
    pub fn write(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut head = None;
        for sample in &self.samples {
            let open = head.filter(|&at: &usize| {
                let w = word(&out, at);
                w as u16 == sample.kind() && ((w >> 16) as usize) < RUN_MAX
            });
            let at = match open {
                Some(at) => at,
                None => {
                    out.extend_from_slice(&u32::from(sample.kind()).to_le_bytes());
                    out.len() - 4
                }
            };
            let w = word(&out, at) + (1 << 16);
            out[at..at + 4].copy_from_slice(&w.to_le_bytes());
            head = Some(at);
            match sample {
                Sample::Key(k) => {
                    for p in k.position {
                        out.extend_from_slice(&p.to_le_bytes());
                    }
                    out.extend_from_slice(&k.time.to_le_bytes()[..3]);
                    out.extend(k.rotation.map(|q| q as u8));
                    out.push(k.spare);
                }
                Sample::Delta(d) => {
                    out.extend_from_slice(&d.time.to_le_bytes()[..3]);
                    out.extend(d.offset.map(|q| q as u8));
                    out.extend(d.rotation.map(|q| q as u8));
                }
            }
        }
        out.extend_from_slice(&self.tail);
        out
    }
}

impl Ghost {
    pub fn parse(bytes: &[u8]) -> Result<Ghost, Error> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::Short { len: bytes.len() });
        }
        if bytes[..8] != MAGIC {
            return Err(Error::Magic);
        }
        let version = (word(bytes, 12) >> 16) as u16;
        if version != VERSION {
            return Err(Error::Version(version));
        }
        let count = word(bytes, 16) as usize;
        let available = bytes.len() - HEADER_LEN;
        if count > available {
            return Err(Error::Count { count, available });
        }
        Ok(Ghost {
            map: word(bytes, 8),
            car: word(bytes, 12) as u16,
            reserved: bytes[20..32].try_into().unwrap(),
            stream: Stream::parse(&bytes[HEADER_LEN..HEADER_LEN + count]),
            after: bytes[HEADER_LEN + count..].to_vec(),
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let stream = self.stream.write();
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&self.map.to_le_bytes());
        out.extend_from_slice(&(u32::from(self.car) | u32::from(VERSION) << 16).to_le_bytes());
        out.extend_from_slice(&(stream.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.reserved);
        out.extend(stream);
        out.extend_from_slice(&self.after);
        out
    }
}

/// Header words 5 to 12 of a `.ghc`.
fn header_weather(bytes: &[u8]) -> Weather {
    let w = |i: usize| word(bytes, i * 4);
    Weather {
        track_letter: w(5),
        cloud: w(6),
        weather: w(7),
        cloud_letter: w(8),
        wear: w(9),
        wetness: w(10),
        amount: f32::from_bits(w(11)),
        unk3c: f32::from_bits(w(12)),
    }
}

impl SavedGhost {
    pub fn parse(bytes: &[u8]) -> Result<SavedGhost, Error> {
        if bytes.len() < SAVED_HEADER_LEN {
            return Err(Error::Short { len: bytes.len() });
        }
        let w = |i: usize| word(bytes, i * 4);
        let (stored, computed) = (w(0), saves::checksum(&bytes[8..]));
        if stored != computed {
            return Err(Error::Checksum { stored, computed });
        }
        if w(1) as usize != bytes.len() - 8 {
            return Err(Error::Length { stored: w(1) as usize, file: bytes.len() - 8 });
        }
        if bytes[8..12] != SAVED_TAG {
            return Err(Error::Magic);
        }
        let count = w(13) as usize;
        let available = bytes.len() - SAVED_HEADER_LEN;
        if count > available {
            return Err(Error::Count { count, available });
        }
        Ok(SavedGhost {
            map: w(3),
            car: w(4),
            weather: header_weather(bytes),
            stream: Stream::parse(&bytes[SAVED_HEADER_LEN..SAVED_HEADER_LEN + count]),
        })
    }

    /// The weather words of a `.ghc` from its first [`SAVED_HEADER_LEN`] bytes (the tag is checked, the checksum is
    /// not), for listing files without reading them whole.
    pub fn header_weather_words(header: &[u8]) -> Option<Weather> {
        (header.len() >= SAVED_HEADER_LEN && header[8..12] == SAVED_TAG).then(|| header_weather(header))
    }

    pub fn write(&self) -> Vec<u8> {
        let stream = self.stream.write();
        let size = SAVED_HEADER_LEN + stream.len();
        let w = &self.weather;
        let words = [
            0,
            (size - 8) as u32,
            u32::from_le_bytes(SAVED_TAG),
            self.map,
            self.car,
            w.track_letter,
            w.cloud,
            w.weather,
            w.cloud_letter,
            w.wear,
            w.wetness,
            w.amount.to_bits(),
            w.unk3c.to_bits(),
            stream.len() as u32,
        ];
        let mut out: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        out.extend(stream);
        let crc = saves::checksum(&out[8..]);
        out[..4].copy_from_slice(&crc.to_le_bytes());
        out
    }

    /// The save-file name (`E20MakeName` 0x49e230, kind 5) for the profile `name`.
    pub fn file_name(name: &str, map: u32) -> String {
        format!("{name}_{map:02}.ghc")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream() -> Stream {
        let key = Key { time: 0, position: [1.0, -2.0, 3.5], rotation: [0, 0, 110, -63], spare: 1 };
        let delta = |time| Delta { time, offset: [1, -1, 0], rotation: [0, 0, 110, -63] };
        Stream {
            samples: vec![Sample::Key(key), Sample::Delta(delta(40)), Sample::Delta(delta(122)), Sample::Key(key)],
            tail: vec![0; 4],
        }
    }

    #[test]
    fn stream_runs_round_trip() {
        let s = stream();
        let bytes = s.write();
        assert_eq!(bytes.len(), 4 + 20 + 4 + 2 * 10 + 4 + 20 + 4);
        assert_eq!(&bytes[..4], &[2, 0, 1, 0]);
        assert_eq!(&bytes[24..28], &[1, 0, 2, 0]);
        assert_eq!(Stream::parse(&bytes), s);
    }

    #[test]
    fn ghost_files_round_trip() {
        let ghost = Ghost { map: 13, car: 5, reserved: [0xff; 12], stream: stream(), after: vec![7; 3] };
        let bytes = ghost.write();
        assert_eq!(&bytes[..8], b"SOHGRACT");
        assert_eq!(Ghost::parse(&bytes).unwrap(), ghost);
        let saved =
            SavedGhost { map: 13, car: 5, weather: Weather { amount: 0.5, ..Weather::default() }, stream: stream() };
        let bytes = saved.write();
        assert_eq!(SavedGhost::parse(&bytes).unwrap(), saved);
        let mut bad = bytes.clone();
        bad[60] ^= 1;
        assert!(matches!(SavedGhost::parse(&bad), Err(Error::Checksum { .. })));
        assert_eq!(SavedGhost::file_name("Player", 4), "Player_04.ghc");
    }

    #[test]
    fn unknown_kind_ends_the_stream() {
        let mut bytes = stream().write();
        bytes.truncate(bytes.len() - 4);
        bytes.extend_from_slice(&[0xfd; 8]);
        let s = Stream::parse(&bytes);
        assert_eq!(s.samples.len(), 4);
        assert_eq!(s.tail, vec![0xfd; 8]);
        assert_eq!(s.write(), bytes);
    }
}

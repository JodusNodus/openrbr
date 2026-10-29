//! Reader for the per-step physics recordings written by the runtime harness
//! (`harness/plugin/RBRRecompHarness.cpp`, `Record`).
//!
//! One frame per physics `StepForward` call. All fields are little-endian:
//!
//! ```text
//! repeat to EOF:
//!   u32 magic      0x314d5246 ('FRM1')
//!   u32 pre        CPhysics step counter before the call
//!   u32 post       step counter after the call
//!   u32 substeps   StepForward argument (each substep is a fixed `STEP_SECONDS`, 7 ms; a live NGP run's 1/720 s,
//!                  `clock`)
//!   u32 car_bytes  length of the car block (0x1700 in 1.02)
//!   u8  control[0x20]       player's car-control payload fed to this step
//!   u8  car[car_bytes]      CCar state
//!   u8  wheel[4][0xa00]     CWheel state; all zero where the pointer was invalid
//! ```
//!
//! [`Frames`] walks a byte slice and yields borrowed [`Frame`]s. A bad magic
//! or a frame that runs past the end is reported with its byte offset, and the
//! iterator stops after that error. Use [`std::fs::read`] to load a file first.
//! The reader never panics on malformed input.

use std::fmt::{self, Display, Formatter};

pub mod camera;
pub mod clock;
pub mod damage;
pub mod dash;
pub mod detach;
pub mod ffb;
pub mod flow;
pub mod gearbox;
pub mod hub;
pub mod hud;
pub mod input;
pub mod live;
pub mod menu;
pub mod pacenote;
pub mod particles;
pub mod peek;
pub mod pose;
pub mod realtime;
pub mod shift;
pub mod sound;
pub mod splits;
pub mod trace;

/// First word of every frame: `'FRM1'` little-endian.
pub const MAGIC: u32 = 0x314d_5246;
/// Bytes before the control block: magic, pre, post, substeps, car_bytes.
pub const HEADER_BYTES: usize = 20;
/// Game time of one substep (`StepForward(1)`), seconds: `PhysicsEngine.MaxTimeStep` of the retail `physics.lsp`
/// (`notes/gameplay.md`). The game's frame splits wall time into whole steps of it (`rbr_gameplay::tick`), so a
/// recording's or a live run's N substeps are N times this of real time.
pub const STEP_SECONDS: f32 = 0.007;
/// Size of the player's control payload.
pub const CONTROL_BYTES: usize = 0x20;
/// Size of one wheel block.
pub const WHEEL_BYTES: usize = 0xa00;
/// Wheels stored after the car block.
pub const WHEELS: usize = 4;

/// Bytes in a frame whose car block is `car_len` bytes long.
fn frame_len(car_len: usize) -> usize {
    HEADER_BYTES + CONTROL_BYTES + car_len + WHEELS * WHEEL_BYTES
}

/// Reading failure, with the byte offset of the frame it occurred in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BadMagic { offset: usize, found: u32 },
    Truncated { offset: usize, needed: usize, len: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic { offset, found } => {
                write!(f, "bad magic at offset {offset}: found {found:#010x}, expected {MAGIC:#010x}")
            }
            Error::Truncated { offset, needed, len } => {
                write!(f, "truncated frame at offset {offset}: need {needed} bytes but {} remain", len - offset)
            }
        }
    }
}

impl std::error::Error for Error {}

/// One recorded physics step. Slices borrow from the buffer being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame<'a> {
    /// Byte offset of this frame's magic word.
    pub offset: usize,
    pub pre: u32,
    pub post: u32,
    pub substeps: u32,
    pub control: [u8; CONTROL_BYTES],
    pub car: &'a [u8],
    wheels: &'a [u8],
}

impl<'a> Frame<'a> {
    /// Wheel `index` (0 to 3), or `None` if out of range.
    pub fn wheel(&self, index: usize) -> Option<&'a [u8]> {
        if index >= WHEELS {
            return None;
        }
        self.wheels.get(index * WHEEL_BYTES..(index + 1) * WHEEL_BYTES)
    }

    /// The control payload as eight little-endian f32 words.
    pub fn control_f32s(&self) -> [f32; 8] {
        control_words(&self.control, f32::from_le_bytes)
    }

    /// The control payload as eight little-endian u32 words.
    pub fn control_u32s(&self) -> [u32; 8] {
        control_words(&self.control, u32::from_le_bytes)
    }
}

fn control_words<T>(control: &[u8; CONTROL_BYTES], decode: fn([u8; 4]) -> T) -> [T; 8] {
    let (words, _) = control.as_chunks::<4>();
    std::array::from_fn(|i| decode(words[i]))
}

/// Little-endian u32 at `offset` in a car or wheel block, or `None` if the four
/// bytes are not all present.
pub fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes.get(offset..)?.first_chunk::<4>().map(|word| u32::from_le_bytes(*word))
}

/// Little-endian f32 at `offset` in a car or wheel block, or `None` if the four
/// bytes are not all present.
pub fn read_f32(bytes: &[u8], offset: usize) -> Option<f32> {
    read_u32(bytes, offset).map(f32::from_bits)
}

/// The first bad frame's index and cause ([`read_all`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadError {
    /// The frame's index: the good frames before it.
    pub index: usize,
    /// Why the frame is bad.
    pub error: Error,
}

impl Display for ReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let offset = match self.error {
            Error::BadMagic { offset, .. } | Error::Truncated { offset, .. } => offset,
        };
        write!(f, "frame {} at offset {offset}: {}", self.index, self.error)
    }
}

impl std::error::Error for ReadError {}

/// All frames in `bytes`, or the first bad frame's index and byte offset.
///
/// A short trailing partial frame (the recorder stopped mid-write) ends the file instead: the frames so far are
/// returned, with a one-line warning on stderr. A bad magic anywhere else is an error, so a damaged recording is
/// never silently cut short to the prefix before it.
pub fn read_all(bytes: &[u8]) -> Result<Vec<Frame<'_>>, ReadError> {
    let mut out = Vec::new();
    let mut frames = Frames::new(bytes);
    loop {
        match frames.next() {
            None => return Ok(out),
            Some(Ok(frame)) => out.push(frame),
            Some(Err(Error::Truncated { offset, needed, len })) => {
                eprintln!(
                    "rbr-recording: frame {} at offset {offset} is truncated (need {needed} bytes, the file has {len}); taking the frames so far",
                    out.len()
                );
                return Ok(out);
            }
            Some(Err(error @ Error::BadMagic { .. })) => return Err(ReadError { index: out.len(), error }),
        }
    }
}

/// Iterator over the frames in a recording buffer.
#[derive(Debug, Clone)]
pub struct Frames<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Frames<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
}

impl<'a> Iterator for Frames<'a> {
    type Item = Result<Frame<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.bytes.len() {
            return None;
        }
        match parse_frame(self.bytes, self.offset) {
            Ok(frame) => {
                self.offset = self.offset.saturating_add(frame_len(frame.car.len()));
                Some(Ok(frame))
            }
            Err(e) => {
                self.offset = self.bytes.len();
                Some(Err(e))
            }
        }
    }
}

fn parse_frame(bytes: &[u8], offset: usize) -> Result<Frame<'_>, Error> {
    let truncated = |needed| Error::Truncated { offset, needed, len: bytes.len() };
    let rest = bytes.get(offset..).unwrap_or_default();
    let word = |i: usize| read_u32(rest, 4 * i).ok_or_else(|| truncated(HEADER_BYTES));

    let magic = word(0)?;
    if magic != MAGIC {
        return Err(Error::BadMagic { offset, found: magic });
    }
    let pre = word(1)?;
    let post = word(2)?;
    let substeps = word(3)?;
    let car_len = word(4)? as usize;

    let len = frame_len(car_len);
    let body = rest.get(HEADER_BYTES..len).ok_or_else(|| truncated(len))?;
    let (control, body) = body.split_first_chunk::<CONTROL_BYTES>().ok_or_else(|| truncated(len))?;
    let (car, wheels) = body.split_at_checked(car_len).ok_or_else(|| truncated(len))?;
    Ok(Frame { offset, pre, post, substeps, control: *control, car, wheels })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAR_BYTES: usize = 0x1700;

    /// Encodes one frame; `wheels` must be `WHEELS * WHEEL_BYTES` long.
    fn encode(pre: u32, post: u32, substeps: u32, control: &[u8], car: &[u8], wheels: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        for w in [MAGIC, pre, post, substeps, car.len() as u32] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        b.extend_from_slice(control);
        b.extend_from_slice(car);
        b.extend_from_slice(wheels);
        b
    }

    fn filled(len: usize, value: u8) -> Vec<u8> {
        vec![value; len]
    }

    fn control_bytes() -> Vec<u8> {
        (1..=8).flat_map(|i| (i as f32).to_le_bytes()).collect()
    }

    #[test]
    fn empty_buffer_has_no_frames() {
        assert_eq!(Frames::new(&[]).count(), 0);
    }

    #[test]
    fn good_frames_round_trip() {
        let mut wheels = Vec::new();
        for i in 0..WHEELS {
            wheels.extend(filled(WHEEL_BYTES, i as u8 + 1));
        }
        let car = filled(CAR_BYTES, 0xab);
        let mut b = encode(10, 11, 1, &control_bytes(), &car, &wheels);
        b.extend(encode(11, 13, 2, &control_bytes(), &car, &wheels));

        let frames: Vec<Frame> = Frames::new(&b).collect::<Result<_, _>>().unwrap();
        assert_eq!(frames.len(), 2);
        let first = frames[0];
        assert_eq!((first.offset, first.pre, first.post, first.substeps), (0, 10, 11, 1));
        assert_eq!(first.control.as_slice(), control_bytes().as_slice());
        assert_eq!(first.car, car.as_slice());
        for i in 0..WHEELS {
            assert_eq!(first.wheel(i), Some(filled(WHEEL_BYTES, i as u8 + 1).as_slice()));
        }
        assert_eq!(first.wheel(WHEELS), None);
        let second = frames[1];
        assert_eq!(second.offset, b.len() / 2);
        assert_eq!((second.pre, second.post, second.substeps), (11, 13, 2));
    }

    #[test]
    fn control_reads_as_floats_and_words() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let b = encode(0, 1, 1, &control_bytes(), &[], &wheels);
        let frame = Frames::new(&b).next().unwrap().unwrap();
        assert_eq!(frame.control_f32s(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(frame.control_u32s()[0], 1.0f32.to_bits());
        assert_eq!(frame.control_u32s()[7], 8.0f32.to_bits());
    }

    #[test]
    fn different_car_sizes_are_framed_by_header() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let mut b = encode(0, 1, 1, &control_bytes(), &filled(0x10, 1), &wheels);
        b.extend(encode(1, 2, 1, &control_bytes(), &[], &wheels));
        b.extend(encode(2, 3, 1, &control_bytes(), &filled(0x2000, 2), &wheels));

        let lens: Vec<usize> = Frames::new(&b).map(|f| f.unwrap().car.len()).collect();
        assert_eq!(lens, [0x10, 0, 0x2000]);
    }

    #[test]
    fn bad_magic_reports_offset_and_stops() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let good = encode(0, 1, 1, &control_bytes(), &[], &wheels);
        let mut b = good.clone();
        b.extend(encode(1, 2, 1, &control_bytes(), &[], &wheels));
        let second = good.len();
        b[second..second + 4].copy_from_slice(&0xdead_beefu32.to_le_bytes());

        let mut frames = Frames::new(&b);
        assert!(frames.next().unwrap().is_ok());
        assert_eq!(frames.next(), Some(Err(Error::BadMagic { offset: second, found: 0xdead_beef })));
        assert_eq!(frames.next(), None);
    }

    #[test]
    fn truncated_header_reports_offset() {
        let b = [0x46, 0x52, 0x4d];
        assert_eq!(Frames::new(&b).next(), Some(Err(Error::Truncated { offset: 0, needed: HEADER_BYTES, len: 3 })));
    }

    #[test]
    fn truncated_body_reports_frame_offset() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let good = encode(0, 1, 1, &control_bytes(), &filled(CAR_BYTES, 0), &wheels);
        let mut b = good.clone();
        b.extend(encode(1, 2, 1, &control_bytes(), &filled(CAR_BYTES, 0), &wheels));
        b.truncate(b.len() - 1);

        let mut frames = Frames::new(&b);
        assert!(frames.next().unwrap().is_ok());
        assert_eq!(
            frames.next(),
            Some(Err(Error::Truncated { offset: good.len(), needed: frame_len(CAR_BYTES), len: b.len() }))
        );
        assert_eq!(frames.next(), None);
    }

    #[test]
    fn huge_car_bytes_is_truncation_not_a_panic() {
        let mut b = Vec::new();
        for w in [MAGIC, 0, 0, 1, u32::MAX] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        assert!(matches!(Frames::new(&b).next(), Some(Err(Error::Truncated { offset: 0, .. }))));
    }

    #[test]
    fn read_all_takes_every_good_frame() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let car = filled(CAR_BYTES, 0xab);
        let mut b = encode(10, 11, 1, &control_bytes(), &car, &wheels);
        b.extend(encode(11, 13, 2, &control_bytes(), &car, &wheels));
        let frames = read_all(&b).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].pre, frames[1].pre), (10, 11));
        assert!(read_all(&[]).unwrap().is_empty());
    }

    #[test]
    fn read_all_names_corruption_in_the_middle() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let good = encode(0, 1, 1, &control_bytes(), &[], &wheels);
        let mut b = good.clone();
        b.extend(encode(1, 2, 1, &control_bytes(), &[], &wheels));
        let second = good.len();
        b[second..second + 4].copy_from_slice(&0xdead_beefu32.to_le_bytes());
        b.extend(encode(2, 3, 1, &control_bytes(), &[], &wheels));
        let err = read_all(&b).unwrap_err();
        assert_eq!(err.index, 1);
        assert_eq!(err.error, Error::BadMagic { offset: second, found: 0xdead_beef });
        assert_eq!(err.to_string(), format!("frame 1 at offset {second}: {}", err.error));
    }

    #[test]
    fn read_all_stops_at_a_truncated_tail() {
        let wheels = filled(WHEELS * WHEEL_BYTES, 0);
        let good = encode(0, 1, 1, &control_bytes(), &filled(CAR_BYTES, 0), &wheels);
        let mut b = good.clone();
        b.extend(encode(1, 2, 1, &control_bytes(), &filled(CAR_BYTES, 0), &wheels));
        b.truncate(b.len() - 1);
        let frames = read_all(&b).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].pre, 0);
    }

    #[test]
    fn error_messages_name_the_offset() {
        let bad = Error::BadMagic { offset: 16180, found: 0 };
        assert_eq!(bad.to_string(), "bad magic at offset 16180: found 0x00000000, expected 0x314d5246");
        let short = Error::Truncated { offset: 100, needed: 16180, len: 150 };
        assert_eq!(short.to_string(), "truncated frame at offset 100: need 16180 bytes but 50 remain");
    }

    #[test]
    fn scalar_reads_are_bounds_checked() {
        let bytes = 1.5f32.to_le_bytes();
        assert_eq!(read_f32(&bytes, 0), Some(1.5));
        assert_eq!(read_u32(&bytes, 0), Some(1.5f32.to_bits()));
        assert_eq!(read_u32(&bytes, 1), None);
        assert_eq!(read_f32(&bytes, 4), None);
        assert_eq!(read_u32(&bytes, usize::MAX), None);
    }
}

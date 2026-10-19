//! Reader for the scenery animations `SGA/*.sga` (one per animated stage
//! object: birds, marshals, the countdown, the prize-giving).
//!
//! Container: the same 3DS Max export framing as the car and scenery models
//! (see [`crate::sgc`]): the 23-byte banner, then records of `[u32
//! header_len = 12][u32 kind][u32 subtype][u32 1][u32 body_len][body]`
//! tiling the file. Every retail file holds one kind-20 range record and
//! kind-6/subtype-21 animation records. The LocoMotion scene loader
//! `LM_LoadScene` (`LocoMotion.dll` 1.02, `0x10018130`) reads the 6/21
//! records and the 20/0 range (`AddNamedRange`); anything else in the
//! 6/21 switch is skipped by its length. See `notes/formats/sgs.md`.
//!
//! A 6/21 body is a controller binding: a NUL-terminated `<node>::<slot>`
//! name, an inner header `[u32 len = 12][u32 type][u32 subtype][u32 1]`, a
//! controller length, and the controller bytes. The loader dispatches on the
//! inner type (`src/locomotion/CLM_SceneLoader.cpp`, `CLM_ControllerLoader.
//! cpp`, `CLM_TrackLoader.cpp`):
//!
//! - `(7, 33)` sampled PRS (`LoadPRSSampled`, `0x10014660`): a sample
//!   count word and the samples in one of three shapes, told apart by
//!   length — times-then-matrices (the loader's order; 3,859 retail
//!   bindings), interleaved `(time, matrix)` pairs (41 bindings in two
//!   unreferenced prize-can files), or one pair whose count word (293)
//!   disagrees with it (19 bindings in one unreferenced file).
//! - `(8, 31)` XYZ position: three float controllers (`::x`, `::y`, `::z`).
//! - `(10, 0)` scale and `(11, 24)` float (the `Camera ROTO` roll): one
//!   track each. Track types are linear (`0x2e` float, `0x2d` scale) and
//!   Bezier (`0x26` float, `0x25` scale); each track starts with two
//!   out-of-range words and a key count.
//! - `(22, 0)` IK chain: skipped by the loader (no switch case).
//!
//! The `(7, 28)` plain PRS, `(8, 26)` vector position, rotation (9),
//! look-at (`0x1e`), Euler (`0x20`), link-constraints (`0x1d`) and IK-load
//! (`0xc`/`0x14`) bindings the loader also accepts never occur in retail
//! files and are rejected here as unknown.
//!
//! All reads are bounds-checked against the record body, and [`parse`]
//! requires every body to be consumed exactly.

use crate::sg_framing::{self, FrameError};
use std::fmt::{self, Display, Formatter};

pub use crate::sg_framing::BANNER;

const KIND_ANIMATION: u32 = 6;
const SUBTYPE_ANIMATION: u32 = 21;
const KIND_RANGE: u32 = 20;

/// Inner controller types (`tag2[0]` in `LM_LoadScene`).
const INNER_SAMPLED_PRS: u32 = 7;
const INNER_POSITION_XYZ: u32 = 8;
const INNER_SCALE: u32 = 10;
const INNER_FLOAT: u32 = 11;
/// Inner subtypes dispatched by the loader.
const SUBTYPE_SAMPLED: u32 = 33;
const SUBTYPE_POSITION_XYZ: u32 = 31;

/// Track header type word (`chunk[0]`): a track of this controller's kind
/// follows. Other values mean an unanimated controller.
const TRACK: u32 = 13;

/// Float track types (`LoadFloatTrack`, `0x100160f0`).
pub const FLOAT_BEZIER: u32 = 0x26;
pub const FLOAT_LINEAR: u32 = 0x2e;
/// Scale track types (`LoadScaleTrack`, `0x10016c90`).
pub const SCALE_BEZIER: u32 = 0x25;
pub const SCALE_LINEAR: u32 = 0x2d;

pub use crate::sg_framing::Record;

/// Failure to read a scenery animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The file does not start with [`BANNER`].
    BadBanner,
    /// A record header length other than 12.
    HeaderLength { offset: usize, length: u32 },
    /// A record header or body runs past the end of the file.
    Truncated { offset: usize },
    /// A body is shorter than its layout, or a count is negative.
    BadBody { offset: usize, what: &'static str },
    /// A body ends before its layout does.
    Leftover { offset: usize, unread: usize },
    /// A 6/21 inner type, or a track type, this reader does not decode.
    /// Rotation, look-at, Euler, link-constraints and plain-PRS bindings
    /// never occur in retail files.
    UnknownBinding { offset: usize, controller: u32, subtype: u32 },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadBanner => write!(f, "not a scenery animation (banner mismatch)"),
            Error::HeaderLength { offset, length } => {
                write!(f, "record at {offset:#x}: header length {length}, expected 12")
            }
            Error::Truncated { offset } => write!(f, "record at {offset:#x} runs past the file"),
            Error::BadBody { offset, what } => write!(f, "record at {offset:#x}: {what}"),
            Error::Leftover { offset, unread } => {
                write!(f, "record at {offset:#x}: {unread} bytes unread")
            }
            Error::UnknownBinding { offset, controller, subtype } => {
                write!(f, "record at {offset:#x}: unknown animation binding ({controller}, {subtype})")
            }
        }
    }
}

impl std::error::Error for Error {}

impl FrameError for Error {
    fn bad_banner() -> Self {
        Error::BadBanner
    }
    fn header_length(offset: usize, length: u32) -> Self {
        Error::HeaderLength { offset, length }
    }
    fn truncated(offset: usize) -> Self {
        Error::Truncated { offset }
    }
    fn bad_body(offset: usize, what: &'static str) -> Self {
        Error::BadBody { offset, what }
    }
    fn leftover(offset: usize, unread: usize) -> Self {
        Error::Leftover { offset, unread }
    }
}

/// A decoded record.
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    /// Kind 6, subtype 21: one controller binding for a scene node.
    Animation(Animation),
    /// Kind 20, subtype 0: the controller range (`AddNamedRange`).
    Range(NamedRange),
}

/// One 6/21 controller binding: the target node slot and its controller.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// `<node>::<slot>` name, e.g. `D_pelvis::PRS` or `Camera ROTO::Roll`.
    pub name: String,
    pub controller: Controller,
}

/// A decoded controller binding.
#[derive(Debug, Clone, PartialEq)]
pub enum Controller {
    /// Inner (7, 33) in times-then-matrices order: the order
    /// `LoadPRSSampled` (`0x10014660`) reads.
    SampledPrs(SampledPrs),
    /// Inner (7, 33) as interleaved `(time, matrix)` pairs. Same data as
    /// [`Controller::SampledPrs`] in file order; only the two prize-can
    /// files (`anim2_REDBULL_BurnsCAN_01`, `anim2_REDBULL_playerCAN_a_01`,
    /// unreferenced by any script) use it, and the traced loader does not
    /// read this order — how the game plays them is open.
    InterleavedPrs(SampledPrs),
    /// Inner (7, 33) with one 3x4 matrix and two unexplained words. Only
    /// `anim2_REDBULL_playerCAN_b_01` (referenced by no script) uses it:
    /// `count` is 293 and `head` runs `i * 293 * 52` over the 19 records
    /// in file order, as if each binding held 293 samples. Both meanings
    /// are open — the loader would read `count` as a sample count and
    /// over-read the payload.
    SingleMatrix { count: u32, head: u32, matrix: [[f32; 4]; 3] },
    /// Inner (8, 31): three float tracks (the loader's `::x`, `::y`, `::z`).
    PositionXyz(PositionXyz),
    /// Inner (10, 0): one scale track.
    Scale(Track<ScaleKey>),
    /// Inner (11, 24): one float track.
    Float(Track<FloatKey>),
    /// Inner (22, 0): IK chain. Skipped by the loader (no switch case), so
    /// the payload shape is not decoded; retail payloads are always the
    /// 4-byte skip word 0.
    SkippedIk,
}

/// Baked PRS samples (`LoadPRSSampled`, `0x10014660`).
#[derive(Debug, Clone, PartialEq)]
pub struct SampledPrs {
    /// Sample times, one per matrix.
    pub times: Vec<f32>,
    /// One 3x4 matrix per sample, in file order.
    pub samples: Vec<[[f32; 4]; 3]>,
}

/// Three float tracks forming an XYZ position controller.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionXyz {
    pub x: Track<FloatKey>,
    pub y: Track<FloatKey>,
    pub z: Track<FloatKey>,
}

/// One keyed track: out-of-range behaviour words and the keys.
#[derive(Debug, Clone, PartialEq)]
pub struct Track<K> {
    pub before: i32,
    pub after: i32,
    pub keys: Vec<K>,
}

/// One linear or Bezier float key (`LoadFloatTrack`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatKey {
    pub time: f32,
    pub value: f32,
    /// Bezier tangents only (`FLOAT_BEZIER`); `None` for linear keys.
    pub tangents: Option<([f32; 2], [f32; 2])>,
}

/// One linear or Bezier scale key (`LoadScaleTrack`): a scale vector with an
/// axis-angle rotation, plus Bezier extras.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleKey {
    pub time: f32,
    pub scale: [f32; 3],
    pub angle: f32,
    pub axis: [f32; 3],
    /// Four extra vectors, in the loader's store order (slots 13, 7, 16,
    /// 10). `None` for linear keys.
    pub extras: Option<[[f32; 3]; 4]>,
}

/// The kind-20 controller range: two floats passed to `AddNamedRange`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NamedRange {
    pub start: f32,
    pub end: f32,
}

/// Reads the banner and the record list. Records must tile the file exactly.
pub fn records(bytes: &[u8]) -> Result<Vec<Record<'_>>, Error> {
    sg_framing::records(bytes)
}

/// Decodes every record of a scenery animation in file order.
pub fn parse(bytes: &[u8]) -> Result<Vec<Element>, Error> {
    records(bytes)?.iter().map(element).collect()
}

fn element(rec: &Record<'_>) -> Result<Element, Error> {
    let mut r = Reader::new(rec.body, rec.offset);
    let out = match (rec.kind, rec.subtype) {
        (KIND_ANIMATION, SUBTYPE_ANIMATION) => Element::Animation(animation(&mut r)?),
        (KIND_RANGE, 0) => Element::Range(NamedRange { start: r.f32()?, end: r.f32()? }),
        _ => return Err(Error::UnknownBinding { offset: rec.offset, controller: rec.kind, subtype: rec.subtype }),
    };
    r.finish()?;
    Ok(out)
}

/// A 6/21 body: name, inner header, controller length, controller bytes. The
/// length word is checked against the bytes that remain; the loader skips by
/// it after the call.
fn animation(r: &mut Reader<'_>) -> Result<Animation, Error> {
    let name = r.cstr()?;
    let header_len = r.u32()?;
    if header_len != 12 {
        return Err(Error::BadBody { offset: r.offset, what: "animation inner header is not 12 bytes" });
    }
    let (controller, subtype) = (r.u32()?, r.u32()?);
    let _third = r.u32()?;
    let controller_len = r.u32()? as usize;
    if controller_len != r.remaining() {
        return Err(Error::BadBody { offset: r.offset, what: "animation controller length mismatch" });
    }
    let decoded = match (controller, subtype) {
        (INNER_SAMPLED_PRS, SUBTYPE_SAMPLED) => sampled_prs(r)?,
        (INNER_POSITION_XYZ, SUBTYPE_POSITION_XYZ) => Controller::PositionXyz(position_xyz(r, &name)?),
        (INNER_SCALE, _) => Controller::Scale(scale_controller(r)?),
        (INNER_FLOAT, _) => Controller::Float(float_controller(r)?),
        // IK chain: the loader has no switch case for inner type 22, so it
        // skips these by the length word. Retail payloads are always empty
        // (`controller_len` 0); anything else is not this shape.
        (22, 0) => {
            if controller_len != 0 {
                return Err(Error::BadBody { offset: r.offset, what: "IK binding carries payload" });
            }
            Controller::SkippedIk
        }
        _ => return Err(Error::UnknownBinding { offset: r.offset, controller, subtype }),
    };
    Ok(Animation { name, controller: decoded })
}

/// Inner (7, 33): a sample count word and the samples.
///
/// Three retail shapes, told apart by the remaining length (52 bytes per
/// sample): `52 * count` is times-then-matrices (the loader's order),
/// `52 * (count + 1)` is interleaved `(time, matrix)` pairs, and a lone
/// 52 bytes are two unexplained words and one matrix.
fn sampled_prs(r: &mut Reader<'_>) -> Result<Controller, Error> {
    let count = r.u32()?;
    let count_usize = count as usize;
    if count_usize > (1 << 20) {
        return Err(Error::BadBody { offset: r.offset, what: "sample count overflows" });
    }
    let rest = r.remaining();
    if rest == count_usize * 52 {
        let mut times = Vec::with_capacity(count_usize.min(1 << 20));
        for _ in 0..count_usize {
            times.push(r.f32()?);
        }
        let mut samples = Vec::with_capacity(count_usize.min(1 << 16));
        for _ in 0..count_usize {
            samples.push([r.vec4()?, r.vec4()?, r.vec4()?]);
        }
        return Ok(Controller::SampledPrs(SampledPrs { times, samples }));
    }
    if rest == (count_usize + 1) * 52 {
        let mut times = Vec::with_capacity(count_usize + 1);
        let mut samples = Vec::with_capacity(count_usize + 1);
        for _ in 0..count_usize + 1 {
            times.push(r.f32()?);
            samples.push([r.vec4()?, r.vec4()?, r.vec4()?]);
        }
        return Ok(Controller::InterleavedPrs(SampledPrs { times, samples }));
    }
    if rest == 52 {
        return Ok(Controller::SingleMatrix { count, head: r.u32()?, matrix: [r.vec4()?, r.vec4()?, r.vec4()?] });
    }
    Err(Error::BadBody { offset: r.offset, what: "sampled PRS length matches no sample order" })
}

/// Inner (8, 31): three nested float controllers. The loader names them
/// `<name>::x|y|z` and does not check the wrapper chunk, so neither do we.
fn position_xyz(r: &mut Reader<'_>, _name: &str) -> Result<PositionXyz, Error> {
    Ok(PositionXyz { x: nested_float(r)?, y: nested_float(r)?, z: nested_float(r)? })
}

/// One nested controller: `[len][3 dwords][skip][payload]`. The loader skips
/// by `skip` after the call; the skip must land exactly at the next nested
/// controller (or the body end), which [`Reader::finish`] enforces per body
/// and the length check below enforces per controller.
fn nested_float(r: &mut Reader<'_>) -> Result<Track<FloatKey>, Error> {
    let header_len = r.u32()?;
    if header_len != 12 {
        return Err(Error::BadBody { offset: r.offset, what: "nested controller header is not 12 bytes" });
    }
    let _chunk = [r.u32()?, r.u32()?, r.u32()?];
    let skip = r.u32()? as usize;
    let end = r.pos + skip;
    if end > r.body.len() {
        return Err(Error::BadBody { offset: r.offset, what: "nested controller runs past the body" });
    }
    let track = float_controller(r)?;
    if r.pos != end {
        return Err(Error::BadBody { offset: r.offset, what: "nested controller length mismatch" });
    }
    Ok(track)
}

/// A float controller: `[len][type, track_type, 1][skip]` and, when the type
/// word is [`TRACK`], a float track of `track_type`.
fn float_controller(r: &mut Reader<'_>) -> Result<Track<FloatKey>, Error> {
    let header_len = r.u32()?;
    if header_len != 12 {
        return Err(Error::BadBody { offset: r.offset, what: "float controller header is not 12 bytes" });
    }
    let (track_flag, track_type) = (r.u32()?, r.u32()?);
    let _third = r.u32()?;
    let skip = r.u32()? as usize;
    let end = r.pos + skip;
    if end > r.body.len() {
        return Err(Error::BadBody { offset: r.offset, what: "float controller runs past the body" });
    }
    let track =
        if track_flag == TRACK { float_track(r, track_type)? } else { Track { before: 0, after: 0, keys: Vec::new() } };
    if r.pos != end {
        return Err(Error::BadBody { offset: r.offset, what: "float controller length mismatch" });
    }
    Ok(track)
}

/// A scale controller: same framing as a float controller with a scale track.
fn scale_controller(r: &mut Reader<'_>) -> Result<Track<ScaleKey>, Error> {
    let header_len = r.u32()?;
    if header_len != 12 {
        return Err(Error::BadBody { offset: r.offset, what: "scale controller header is not 12 bytes" });
    }
    let (track_flag, track_type) = (r.u32()?, r.u32()?);
    let _third = r.u32()?;
    let skip = r.u32()? as usize;
    let end = r.pos + skip;
    if end > r.body.len() {
        return Err(Error::BadBody { offset: r.offset, what: "scale controller runs past the body" });
    }
    let track =
        if track_flag == TRACK { scale_track(r, track_type)? } else { Track { before: 0, after: 0, keys: Vec::new() } };
    if r.pos != end {
        return Err(Error::BadBody { offset: r.offset, what: "scale controller length mismatch" });
    }
    Ok(track)
}

/// A float track: two out-of-range words, a key count, then linear
/// (`time, value`) or Bezier (`time, value, in-tangent, out-tangent`) keys.
fn float_track(r: &mut Reader<'_>, track_type: u32) -> Result<Track<FloatKey>, Error> {
    let (before, after) = (r.i32()?, r.i32()?);
    let key_count = r.count()?;
    let mut keys = Vec::with_capacity(key_count.min(1 << 20));
    for _ in 0..key_count {
        let (time, value) = (r.f32()?, r.f32()?);
        let tangents = match track_type {
            FLOAT_LINEAR => None,
            FLOAT_BEZIER => Some((r.vec2()?, r.vec2()?)),
            _ => return Err(Error::UnknownBinding { offset: r.offset, controller: INNER_FLOAT, subtype: track_type }),
        };
        keys.push(FloatKey { time, value, tangents });
    }
    Ok(Track { before, after, keys })
}

/// A scale track: two out-of-range words, a key count, then linear
/// (`time, scale, angle, axis`) or Bezier (plus four extra vectors, in the
/// loader's slot order 13, 7, 16, 10) keys.
fn scale_track(r: &mut Reader<'_>, track_type: u32) -> Result<Track<ScaleKey>, Error> {
    let (before, after) = (r.i32()?, r.i32()?);
    let key_count = r.count()?;
    let mut keys = Vec::with_capacity(key_count.min(1 << 20));
    for _ in 0..key_count {
        let time = r.f32()?;
        let scale = r.vec3()?;
        let angle = r.f32()?;
        let axis = r.vec3()?;
        let extras = match track_type {
            SCALE_LINEAR => None,
            SCALE_BEZIER => {
                let (a, b, c, d) = (r.vec3()?, r.vec3()?, r.vec3()?, r.vec3()?);
                // File order is slots 13, 7, 16, 10; the loader stores them
                // in that order, so the vector keeps file order.
                Some([a, b, c, d])
            }
            _ => return Err(Error::UnknownBinding { offset: r.offset, controller: INNER_SCALE, subtype: track_type }),
        };
        keys.push(ScaleKey { time, scale, angle, axis, extras });
    }
    Ok(Track { before, after, keys })
}

type Reader<'a> = sg_framing::Reader<'a, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sg_framing::RECORD_HEADER_LEN;

    /// A file from `(kind, subtype, body)` records.
    fn file(records: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = BANNER.to_vec();
        for (kind, subtype, body) in records {
            for word in [RECORD_HEADER_LEN, *kind, *subtype, 1, body.len() as u32] {
                out.extend_from_slice(&word.to_le_bytes());
            }
            out.extend_from_slice(body);
        }
        out
    }

    fn f32s(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// A 6/21 body head: name, `[12][type, subtype, 1]` and the controller
    /// bytes (the length word is filled in by `animation`'s caller here).
    fn binding(name: &str, controller: u32, subtype: u32, payload: &[u8]) -> Vec<u8> {
        let mut b = name.as_bytes().to_vec();
        b.push(0);
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&controller.to_le_bytes());
        b.extend_from_slice(&subtype.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b.extend_from_slice(payload);
        b
    }

    fn float_controller_bytes(track_type: u32, keys: &[f32]) -> Vec<u8> {
        // `keys` as (time, value) pairs for linear, plus tangents for Bezier.
        let mut track = 0i32.to_le_bytes().to_vec();
        track.extend_from_slice(&0i32.to_le_bytes());
        track.extend_from_slice(&((keys.len() / 2) as i32).to_le_bytes());
        for pair in keys.chunks(2) {
            track.extend(f32s(&[pair[0], pair[1]]));
            if track_type == FLOAT_BEZIER {
                track.extend(f32s(&[0.0, 0.0, 0.0, 0.0]));
            }
        }
        let mut b = 12u32.to_le_bytes().to_vec();
        b.extend_from_slice(&TRACK.to_le_bytes());
        b.extend_from_slice(&track_type.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(track.len() as u32).to_le_bytes());
        b.extend_from_slice(&track);
        b
    }

    #[test]
    fn sampled_prs_reads_times_and_matrices() {
        let mut payload = 2u32.to_le_bytes().to_vec();
        payload.extend(f32s(&[0.0, 0.04]));
        payload.extend(f32s(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        payload.extend(f32s(&[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]));
        let body = binding("D_pelvis::PRS", INNER_SAMPLED_PRS, SUBTYPE_SAMPLED, &payload);
        let bytes = file(&[(KIND_ANIMATION, SUBTYPE_ANIMATION, body)]);
        let Element::Animation(anim) = &parse(&bytes).expect("parse")[0] else {
            panic!("expected animation");
        };
        assert_eq!(anim.name, "D_pelvis::PRS");
        let Controller::SampledPrs(sampled) = &anim.controller else {
            panic!("expected sampled PRS");
        };
        assert_eq!(sampled.times, [0.0, 0.04]);
        assert_eq!(sampled.samples.len(), 2);
        assert_eq!(sampled.samples[1][0], [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn position_scale_and_float_bindings() {
        let axis = float_controller_bytes(FLOAT_LINEAR, &[0.0, 22.5]);
        let mut position = Vec::new();
        for _ in 0..3 {
            position.extend_from_slice(&12u32.to_le_bytes());
            position.extend_from_slice(&11u32.to_le_bytes());
            position.extend_from_slice(&24u32.to_le_bytes());
            position.extend_from_slice(&1u32.to_le_bytes());
            position.extend_from_slice(&(axis.len() as u32).to_le_bytes());
            position.extend_from_slice(&axis);
        }
        let scale_track = {
            let mut t = 0i32.to_le_bytes().to_vec();
            t.extend_from_slice(&0i32.to_le_bytes());
            t.extend_from_slice(&1i32.to_le_bytes());
            t.extend(f32s(&[0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0]));
            t
        };
        let mut scale = 12u32.to_le_bytes().to_vec();
        scale.extend_from_slice(&TRACK.to_le_bytes());
        scale.extend_from_slice(&SCALE_LINEAR.to_le_bytes());
        scale.extend_from_slice(&1u32.to_le_bytes());
        scale.extend_from_slice(&(scale_track.len() as u32).to_le_bytes());
        scale.extend_from_slice(&scale_track);
        let bytes = file(&[
            (
                KIND_ANIMATION,
                SUBTYPE_ANIMATION,
                binding("Camera ROTO::Position", INNER_POSITION_XYZ, SUBTYPE_POSITION_XYZ, &position),
            ),
            (KIND_ANIMATION, SUBTYPE_ANIMATION, binding("Camera ROTO::Scale", INNER_SCALE, 0, &scale)),
            (KIND_ANIMATION, SUBTYPE_ANIMATION, binding("Camera ROTO::Roll", INNER_FLOAT, 24, &axis)),
            (KIND_RANGE, 0, f32s(&[-0.4, 8.0])),
        ]);
        let elements = parse(&bytes).expect("parse");
        assert_eq!(elements.len(), 4);
        let Element::Animation(position_anim) = &elements[0] else {
            panic!("expected position");
        };
        let Controller::PositionXyz(xyz) = &position_anim.controller else {
            panic!("expected XYZ");
        };
        assert_eq!(xyz.x.keys.len(), 1);
        assert_eq!(xyz.x.keys[0].value, 22.5);
        let Element::Range(range) = &elements[3] else {
            panic!("expected range");
        };
        assert_eq!(range.start, -0.4);
    }

    #[test]
    fn unknown_bindings_are_errors() {
        // Inner (9, 27): a rotation binding, never in retail files.
        let body = binding("D_pelvis::PRS", 9, 27, &[0u8; 4]);
        let bytes = file(&[(KIND_ANIMATION, SUBTYPE_ANIMATION, body)]);
        assert!(matches!(parse(&bytes), Err(Error::UnknownBinding { .. })));
    }

    #[test]
    fn damaged_files_never_panic() {
        let mut payload = 730u32.to_le_bytes().to_vec();
        payload.extend(vec![0u8; 730 * 4 + 730 * 48]);
        let body = binding("D_pelvis::PRS", INNER_SAMPLED_PRS, SUBTYPE_SAMPLED, &payload);
        let bytes = file(&[(KIND_ANIMATION, SUBTYPE_ANIMATION, body)]);
        for cut in 0..bytes.len() {
            let _ = parse(&bytes[..cut]);
        }
        for at in BANNER.len()..bytes.len() {
            for value in [0u8, 0x7f, 0xff] {
                let mut copy = bytes.clone();
                copy[at] = value;
                let _ = parse(&copy);
            }
        }
    }
}

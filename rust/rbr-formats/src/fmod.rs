//! FMOD Studio banks (`<name>.bank` and `<name>.strings.bank`), as NGP's FMOD daemon loads them from `AudioFMOD/`
//! for an RSF car (`notes/formats/fmod-bank.md`): the recordings (the FSB5 sound bank inside, FMOD's headerless
//! Vorbis) and the events' parameter sheets, read far enough to play the loops as the bank lays them out.
//!
//! A bank is a RIFF file (`RIFF....FEV `) of chunks and `LIST`s. Each object's body starts with its 16-byte id;
//! lists inside a body are a u16 `count * 2 + 1` and a u16 element size. What is read, all of it ours from the
//! sample banks (no FMOD code was traced):
//!
//! - `PRMB` a parameter: id, 8 bytes, u16 name length, name, f32 min, f32 max;
//! - `WAV ` a waveform: id, u16, u32, u32 index in the FSB5;
//! - `LIST WAIT` a waveform instrument: `WAIB` (id, waveform id), `INST`, `PRPS`; `LIST MUIT` a multi instrument:
//!   `MUIB` (id), `PLST` (u32 mode, u32, list of {instrument id, f32 percent}), `INST`, `PRPS`;
//! - `INST` an instrument's settings: f32 volume (dB) at 16, f32 pitch (semitones) at 20, the auto-pitch parameter
//!   id at 53 with its reference value at 69 and the pitch at 0 at 73, and the track it plays on at 83;
//! - the mixing tree's buses, each with a `BUS` and a `PRPS`: `LIST GBUS` a track or a mixer group (`GBSB`: id, u16,
//!   parent id), `LIST IBUS` an event's input to the mixer (`IBSB`, the same), `LIST RBUS` a return (`RBSB`, the
//!   same) and `LIST MBUS` an event's master track or the mixer's master bus (`MBSB`: id). `BUS` is a u8, a u32,
//!   two lists of effect ids (taken as before and after the fader; an empty list is just its u16), a u16 and the
//!   f32 fader volume (dB);
//! - `PROP` an automated property: u32 property (0 volume, 1 pitch, 4 instrument volume), u32, the id of the `MAP`
//!   its curves' values go through, and the list of its automations;
//! - `CTRL` an automation: id, target object, curve id, u32 property; `CURV` its curve: id, id, list of points
//!   {f32 x, f32 y, f32 shape, u32}; `MAP` a value map: id, list of {f32 from, f32 to};
//! - `LIST PMLO` a parameter sheet: `PMLB` (id, parameter id, list of {instrument id, f32 start, f32 end, u8}) and
//!   `CTRO` (the list of the automations on its parameter);
//! - `LIST BEFF` an effect: `BEFB` (id, u32 FMOD DSP type), `PMEF` (u32 count, then per parameter a u32 type and a
//!   4-byte value: 0 a float, 1 an int; the others, data and bools, end what is read, except that the convolution
//!   reverb's impulse response is followed by its wet and dry levels and linked flag, read from the end), `PRPS`
//!   (property `1000 + k` automates parameter `k`); `LIST SEFF` a send: `SEFB` (id, the return bus id, f32 level dB);
//! - `EVTB` an event: its id, 32 bytes, its input bus id at 48 and its master track id at 64, the ids of its
//!   sheets somewhere in its body, and its user properties (a list of {u16 name length, name, f32 value}; all the
//!   RSF banks have one, `fluctuation_length`, on some events);
//! - `SND ` the FSB5 sound bank.
//!
//! The strings bank names the ids (`event:/engine_int`): `STDT` holds a radix tree of path pieces, the ids, and
//! for each id the tree node that ends its path.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

pub type Id = [u8; 16];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn err<T>(message: impl Into<String>) -> Result<T, Error> {
    Err(Error(message.into()))
}

/// The FSB5 codecs read: 16-bit PCM, and FMOD's Vorbis.
pub const PCM16: u32 = 2;
pub const VORBIS: u32 = 15;
/// The automated properties read.
const VOLUME: u32 = 0;
const PITCH: u32 = 1;
const INSTRUMENT_VOLUME: u32 = 4;
/// Property `1000 + k` is an effect's parameter `k`.
const EFFECT_PARAM: u32 = 1000;

/// A recording of the FSB5.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub name: String,
    pub rate: u32,
    pub channels: u8,
    pub frames: u32,
    /// The CRC32 of the Vorbis setup header the encoder used (FSB5 chunk 11).
    pub vorbis_crc: Option<u32>,
    /// Its FSB5's codec ([`PCM16`], [`VORBIS`], or another, not read).
    pub codec: u32,
    /// Its data in the bank's bytes.
    pub data: Range<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub min: f32,
    pub max: f32,
}

/// What an automation sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    /// Decibels, added to the volume.
    VolumeDb,
    /// A gain factor (an instrument's volume automation, 0 to 1).
    Gain,
    /// Semitones, added to the pitch.
    Semitones,
    /// An effect's parameter `k` ([`Effect::params`]), set to the value.
    Effect(u32),
}

/// FMOD's DSP types of the effects the banks use (`FMOD_DSP_TYPE`).
pub const DSP_LOWPASS: u32 = 3;
pub const DSP_HIGHPASS: u32 = 5;
pub const DSP_FADER: u32 = 7;
pub const DSP_DISTORTION: u32 = 9;
pub const DSP_PARAMEQ: u32 = 12;
pub const DSP_COMPRESSOR: u32 = 18;
pub const DSP_SFXREVERB: u32 = 19;
pub const DSP_LOWPASS_SIMPLE: u32 = 20;
pub const DSP_HIGHPASS_SIMPLE: u32 = 26;
pub const DSP_PAN: u32 = 27;
pub const DSP_THREE_EQ: u32 = 28;
pub const DSP_CONVOLUTION_REVERB: u32 = 32;
pub const DSP_SEND: u32 = 24;
pub const DSP_LOUDNESS_METER: u32 = 30;
pub const DSP_CHANNEL_MIX: u32 = 33;

/// An effect on a bus: its FMOD DSP type, its leading numeric parameters in FMOD's order (ints as floats), and the
/// automations of its parameters ([`Property::Effect`]). A send ([`DSP_SEND`]) has its level (dB) as parameter 0
/// and its return bus as `target`.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub kind: u32,
    pub params: Vec<f32>,
    pub automations: Vec<Automation>,
    pub target: Option<usize>,
}

/// A bus of the bank's mixing tree: an event's track or master track, an event's input to the mixer, a mixer group,
/// a return, or the master bus (no parent). Its signal runs through `pre`, its fader (`volume_db` plus its
/// [`Property::VolumeDb`] automations), then `post`, into its parent.
#[derive(Debug, Clone, PartialEq)]
pub struct Bus {
    /// `bus:/internal/engine_int` for the mixer's buses; empty for an event's.
    pub name: String,
    pub parent: Option<usize>,
    pub volume_db: f32,
    pub automations: Vec<Automation>,
    pub pre: Vec<Effect>,
    pub post: Vec<Effect>,
}

/// A property's curve over a parameter, its values already through the property's map.
#[derive(Debug, Clone, PartialEq)]
pub struct Automation {
    pub param: usize,
    pub property: Property,
    /// (parameter value, property value), by parameter value.
    pub points: Vec<[f32; 2]>,
}

/// An instrument's pitch following a parameter: `min` at 0, 1 at `reference`, linear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoPitch {
    pub param: usize,
    pub reference: f32,
    pub min: f32,
}

/// Where an instrument sits on a sheet: it plays while the parameter is in `start..=end`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub param: usize,
    pub start: f32,
    pub end: f32,
}

/// One instrument of an event's sheets, with its tracks' pitch automations folded in; it plays into `bus`, its
/// track.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// The samples it plays (a multi instrument's playlist), indices into [`Bank::samples`].
    pub samples: Vec<usize>,
    pub regions: Vec<Region>,
    pub auto_pitch: Option<AutoPitch>,
    /// The instrument's volume, dB.
    pub volume_db: f32,
    pub semitones: f32,
    pub automations: Vec<Automation>,
    pub bus: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// `event:/engine_int`.
    pub name: String,
    pub layers: Vec<Layer>,
    /// The `fluctuation_length` user property (seconds), when the event has it.
    pub fluctuation_length: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct Bank {
    pub bytes: Vec<u8>,
    pub samples: Vec<Sample>,
    pub params: Vec<Param>,
    pub events: Vec<Event>,
    /// The mixing tree, by id order.
    pub buses: Vec<Bus>,
}

impl Bank {
    pub fn event(&self, name: &str) -> Option<&Event> {
        self.events.iter().find(|e| e.name == name)
    }

    /// A sample's bytes (for Vorbis: u16-sized packets).
    pub fn sample_data(&self, sample: &Sample) -> &[u8] {
        &self.bytes[sample.data.clone()]
    }
}

/// A chunk, or a `LIST`'s kind and children.
enum Node<'a> {
    Chunk([u8; 4], &'a [u8]),
    List([u8; 4], Vec<Node<'a>>),
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    u32_at(b, at).map(f32::from_bits)
}

fn id_at(b: &[u8], at: usize) -> Option<Id> {
    b.get(at..at + 16)?.try_into().ok()
}

/// A `BUS` list of ids at `at` (an empty one is just its u16), and the offset after it.
fn ids_at(b: &[u8], at: usize) -> Option<(Vec<Id>, usize)> {
    let count = usize::from(u16_at(b, at)? >> 1);
    if count == 0 {
        return Some((Vec::new(), at + 2));
    }
    let size = usize::from(u16_at(b, at + 2)?);
    let ids = (0..count).map(|k| id_at(b, at + 4 + k * size)).collect::<Option<_>>()?;
    Some((ids, at + 4 + count * size))
}

/// A list at `at`: its elements' bytes, and the offset after it.
fn list_at(b: &[u8], at: usize) -> Option<(Vec<&[u8]>, usize)> {
    let count = usize::from(u16_at(b, at)? >> 1);
    if count == 0 {
        // An empty list still stores its element size when its low bit is set.
        return Some((Vec::new(), at + if u16_at(b, at)? & 1 == 1 { 4 } else { 2 }));
    }
    let size = usize::from(u16_at(b, at + 2)?);
    let items = (0..count).map(|k| b.get(at + 4 + k * size..at + 4 + (k + 1) * size)).collect::<Option<_>>()?;
    Some((items, at + 4 + count * size))
}

fn nodes(b: &[u8]) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 8 <= b.len() {
        let tag: [u8; 4] = b[at..at + 4].try_into().unwrap();
        let len = u32_at(b, at + 4).unwrap() as usize;
        let Some(body) = b.get(at + 8..at + 8 + len) else { break };
        if (&tag == b"LIST" || &tag == b"RIFF") && len >= 4 {
            out.push(Node::List(body[..4].try_into().unwrap(), nodes(&body[4..])));
        } else {
            out.push(Node::Chunk(tag, body));
        }
        at += 8 + len + (len & 1);
    }
    out
}

/// The chunks of a list's direct children with tag `tag`.
fn chunk<'a>(children: &[Node<'a>], tag: &[u8; 4]) -> Option<&'a [u8]> {
    children.iter().find_map(|n| match n {
        Node::Chunk(t, b) if t == tag => Some(*b),
        _ => None,
    })
}

/// The `PROP` chunks under a list's `PRPS`.
fn props<'a>(children: &[Node<'a>]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    for n in children {
        if let Node::List(kind, inner) = n {
            if kind == b"PRPS" {
                out.extend(inner.iter().filter_map(|n| match n {
                    Node::Chunk(t, b) => (t == b"PROP").then_some(*b),
                    Node::List(..) => None,
                }));
            }
        }
    }
    out
}

/// A bus: parent id, fader dB, effect ids before and after the fader.
type BusObject = (Option<Id>, f32, Vec<Id>, Vec<Id>);

/// What the bank's objects say, by id, before the events are put together.
#[derive(Default)]
struct Objects<'a> {
    params: Vec<(Id, Param)>,
    /// Waveform id: (sound bank, sample).
    waveforms: HashMap<Id, (u32, u32)>,
    /// Instrument id: (sample waveform ids or playlist instrument ids, `INST`, is a multi instrument).
    instruments: HashMap<Id, (Vec<Id>, &'a [u8], bool)>,
    buses: HashMap<Id, BusObject>,
    /// Effect id: (DSP type, parameters).
    effects: HashMap<Id, (u32, Vec<f32>)>,
    /// Send id: (return bus id, level dB).
    sends: HashMap<Id, (Id, f32)>,
    /// Automation id: (target id, curve id).
    automations: HashMap<Id, (Id, Id)>,
    curves: HashMap<Id, Vec<[f32; 2]>>,
    maps: HashMap<Id, Vec<[f32; 2]>>,
    /// Automation id: (property, map id).
    automated: HashMap<Id, (u32, Id)>,
    /// (sheet id, parameter id, placements (instrument, start, end), automation ids).
    sheets: Vec<Sheet>,
    events: Vec<&'a [u8]>,
    sound_banks: Vec<&'a [u8]>,
}

impl<'a> Objects<'a> {
    fn walk(&mut self, list: &[u8; 4], children: &[Node<'a>]) -> Option<()> {
        match list {
            b"WAIT" => {
                let b = chunk(children, b"WAIB")?;
                self.instruments.insert(id_at(b, 0)?, (vec![id_at(b, 16)?], chunk(children, b"INST")?, false));
                self.props(children)?;
            }
            b"MUIT" => {
                let playlist = chunk(children, b"PLST")?;
                let (entries, _) = list_at(playlist, 8)?;
                let ids = entries.iter().map(|e| id_at(e, 0)).collect::<Option<_>>()?;
                let id = id_at(chunk(children, b"MUIB")?, 0)?;
                self.instruments.insert(id, (ids, chunk(children, b"INST")?, true));
                self.props(children)?;
            }
            b"GBUS" | b"IBUS" | b"RBUS" | b"MBUS" => {
                let (head, parent) = match list {
                    b"GBUS" => (chunk(children, b"GBSB")?, true),
                    b"IBUS" => (chunk(children, b"IBSB")?, true),
                    b"RBUS" => (chunk(children, b"RBSB")?, true),
                    _ => (chunk(children, b"MBSB")?, false),
                };
                let parent = if parent { Some(id_at(head, 18)?) } else { None };
                let bus = chunk(children, b"BUS ")?;
                let (pre, at) = ids_at(bus, 5)?;
                let (post, at) = ids_at(bus, at)?;
                self.buses.insert(id_at(head, 0)?, (parent, f32_at(bus, at + 2)?, pre, post));
                self.props(children)?;
            }
            b"SEFF" => {
                let head = chunk(children, b"SEFB")?;
                self.sends.insert(id_at(head, 0)?, (id_at(head, 16)?, f32_at(head, 32)?));
                self.props(children)?;
            }
            b"BEFF" => {
                let head = chunk(children, b"BEFB")?;
                let kind = u32_at(head, 16)?;
                let p = chunk(children, b"PMEF")?;
                let mut params = Vec::new();
                for k in 0..u32_at(p, 0)? as usize {
                    match (u32_at(p, 4 + 8 * k), u32_at(p, 8 + 8 * k)) {
                        (Some(0), Some(v)) => params.push(f32::from_bits(v)),
                        (Some(1), Some(v)) => params.push(v as i32 as f32),
                        (Some(3), _) if kind == DSP_CONVOLUTION_REVERB && k == 0 => {
                            // The impulse response (0 here), then the wet and dry levels and the linked flag at the
                            // end, before a trailing byte.
                            params.push(0.0);
                            let n = p.len().checked_sub(22)?;
                            if let (Some(0), Some(0)) = (u32_at(p, n), u32_at(p, n + 8)) {
                                params.extend([f32_at(p, n + 4)?, f32_at(p, n + 12)?]);
                            }
                            break;
                        }
                        _ => break,
                    }
                }
                self.effects.insert(id_at(head, 0)?, (kind, params));
                self.props(children)?;
            }
            b"PMLO" => {
                let b = chunk(children, b"PMLB")?;
                let placed = list_at(b, 32)?
                    .0
                    .iter()
                    .map(|e| Some((id_at(e, 0)?, f32_at(e, 16)?, f32_at(e, 20)?)))
                    .collect::<Option<_>>()?;
                let automations = match chunk(children, b"CTRO") {
                    Some(c) => list_at(c, 0)?.0.iter().map(|e| id_at(e, 0)).collect::<Option<_>>()?,
                    None => Vec::new(),
                };
                self.sheets.push((id_at(b, 0)?, id_at(b, 16)?, placed, automations));
            }
            _ => {
                for n in children {
                    match n {
                        Node::List(kind, inner) => self.walk(kind, inner)?,
                        Node::Chunk(tag, b) => self.chunk(tag, b)?,
                    }
                }
            }
        }
        Some(())
    }

    fn chunk(&mut self, tag: &[u8; 4], b: &'a [u8]) -> Option<()> {
        match tag {
            b"PRMB" => {
                let len = usize::from(u16_at(b, 24)?);
                let name = b.get(26..26 + len)?;
                let name = String::from_utf8_lossy(name.split(|&c| c == 0).next()?).into_owned();
                let param = Param { name, min: f32_at(b, 26 + len)?, max: f32_at(b, 30 + len)? };
                self.params.push((id_at(b, 0)?, param));
            }
            b"WAV " => {
                self.waveforms.insert(id_at(b, 0)?, (u32_at(b, 18)?, u32_at(b, 22)?));
            }
            b"CTRL" => {
                self.automations.insert(id_at(b, 0)?, (id_at(b, 16)?, id_at(b, 32)?));
            }
            b"CURV" => {
                let points: Option<Vec<_>> =
                    list_at(b, 32)?.0.iter().map(|p| Some([f32_at(p, 0)?, f32_at(p, 4)?])).collect();
                self.curves.insert(id_at(b, 0)?, points?);
            }
            b"MAP " => {
                let pairs: Option<Vec<_>> =
                    list_at(b, 16)?.0.iter().map(|p| Some([f32_at(p, 0)?, f32_at(p, 4)?])).collect();
                self.maps.insert(id_at(b, 0)?, pairs?);
            }
            b"EVTB" => self.events.push(b),
            b"SND " => self.sound_banks.push(b),
            _ => {}
        }
        Some(())
    }

    fn props(&mut self, children: &[Node<'a>]) -> Option<()> {
        for p in props(children) {
            let property = u32_at(p, 0)?;
            let map = id_at(p, 8)?;
            for a in list_at(p, 24)?.0 {
                self.automated.insert(id_at(a, 0)?, (property, map));
            }
        }
        Some(())
    }
}

/// Piecewise linear through `points` (sorted by x), held at the ends.
pub fn interpolate(points: &[[f32; 2]], x: f32) -> f32 {
    let Some(first) = points.first() else { return 0.0 };
    if x <= first[0] {
        return first[1];
    }
    for w in points.windows(2) {
        let ([x0, y0], [x1, y1]) = (w[0], w[1]);
        if x <= x1 {
            return if x1 > x0 { y0 + (y1 - y0) * (x - x0) / (x1 - x0) } else { y1 };
        }
    }
    points[points.len() - 1][1]
}

/// An FSB5 sound bank's samples, with data ranges relative to `fsb`.
fn fsb5(fsb: &[u8]) -> Result<Vec<Sample>, Error> {
    let short = || Error("FSB5: short".to_owned());
    if fsb.get(..4) != Some(b"FSB5") {
        return err("SND: no FSB5");
    }
    let field = |k: usize| u32_at(fsb, 4 + 4 * k).ok_or_else(short);
    let (version, count, headers, names, data_size, codec) =
        (field(0)?, field(1)? as usize, field(2)? as usize, field(3)? as usize, field(4)? as usize, field(5)?);
    let header = if version == 0 { 0x40 } else { 0x3c };
    let data_start = header + headers + names;
    let mut at = header;
    let mut samples = Vec::with_capacity(count);
    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        let bits = u64::from_le_bytes(fsb.get(at..at + 8).ok_or_else(short)?.try_into().unwrap());
        at += 8;
        let mut rate = [4000, 8000, 11000, 11025, 16000, 22050, 24000, 32000, 44100, 48000, 96000]
            .get(((bits >> 1) & 0xf) as usize)
            .copied()
            .unwrap_or(44100);
        let mut channels = [1, 2, 6, 8][((bits >> 5) & 3) as usize];
        let offset = (((bits >> 7) & 0x7ff_ffff) << 5) as usize;
        let frames = ((bits >> 34) & 0x3fff_ffff) as u32;
        let mut more = bits & 1 == 1;
        let mut vorbis_crc = None;
        while more {
            let head = u32_at(fsb, at).ok_or_else(short)?;
            let size = ((head >> 1) & 0xff_ffff) as usize;
            let body = fsb.get(at + 4..at + 4 + size).ok_or_else(short)?;
            match head >> 25 {
                1 => channels = body[0],
                2 => rate = u32_at(body, 0).ok_or_else(short)?,
                11 => vorbis_crc = u32_at(body, 0),
                _ => {}
            }
            more = head & 1 == 1;
            at += 4 + size;
        }
        offsets.push(offset);
        samples.push(Sample { name: String::new(), rate, channels, frames, vorbis_crc, codec, data: 0..0 });
    }
    let name_table = header + headers;
    for (k, sample) in samples.iter_mut().enumerate() {
        let at = name_table + u32_at(fsb, name_table + 4 * k).ok_or_else(short)? as usize;
        let name = fsb.get(at..).ok_or_else(short)?.split(|&c| c == 0).next().unwrap_or_default();
        sample.name = String::from_utf8_lossy(name).into_owned();
        let end = offsets.get(k + 1).copied().unwrap_or(data_size);
        sample.data = data_start + offsets[k]..data_start + end;
        if sample.data.end > fsb.len() {
            return Err(short());
        }
    }
    Ok(samples)
}

/// The strings bank's names, by id.
pub fn strings(bytes: &[u8]) -> Result<HashMap<Id, String>, Error> {
    let short = || Error("strings bank: short STDT".to_owned());
    let at = bytes.windows(4).position(|w| w == b"STDT").ok_or(Error("strings bank: no STDT".to_owned()))?;
    let len = u32_at(bytes, at + 4).ok_or_else(short)? as usize;
    let b = bytes.get(at + 8..at + 8 + len).ok_or_else(short)?;
    let (nodes, next) = list_at(b, 4).ok_or_else(short)?;
    let (ids, next) = list_at(b, next).ok_or_else(short)?;
    let pool_len = usize::from(u16_at(b, next).ok_or_else(short)?);
    let pool = b.get(next + 2..next + 2 + pool_len).ok_or_else(short)?;
    let ends_at = next + 2 + pool_len;
    let count = usize::from(u16_at(b, ends_at).ok_or_else(short)?);
    // A node: u24 offset of its piece in the pool (0xffffff: none), u8, u24 first child, u8 child count.
    let node = |k: usize| -> Option<(u32, u32, u32)> {
        let n = nodes.get(k)?;
        let (a, c) = (u32_at(n, 0)?, u32_at(n, 4)?);
        Some((a & 0xff_ffff, c & 0xff_ffff, c >> 24))
    };
    let mut parent = HashMap::new();
    for k in 0..nodes.len() {
        let (_, first, children) = node(k).ok_or_else(short)?;
        for c in first..first + children {
            parent.insert(c as usize, k);
        }
    }
    let piece = |k: usize| -> Option<&[u8]> {
        let (offset, _, _) = node(k)?;
        if offset == 0xff_ffff {
            return Some(&[]);
        }
        pool.get(offset as usize..)?.split(|&c| c == 0).next()
    };
    let mut out = HashMap::new();
    for (k, id) in ids.iter().enumerate().take(count) {
        let at = ends_at + 2 + 3 * k;
        let leaf = b.get(at..at + 3).ok_or_else(short)?;
        let mut n = usize::from(leaf[0]) | usize::from(leaf[1]) << 8 | usize::from(leaf[2]) << 16;
        let mut pieces = Vec::new();
        while let Some(&p) = parent.get(&n) {
            pieces.push(piece(n).ok_or_else(short)?);
            n = p;
        }
        let name: Vec<u8> = pieces.into_iter().rev().flatten().copied().collect();
        out.insert(id_at(id, 0).ok_or_else(short)?, String::from_utf8_lossy(&name).into_owned());
    }
    Ok(out)
}

/// A bank from its bytes and its strings bank's.
pub fn parse(bytes: Vec<u8>, strings_bank: &[u8]) -> Result<Bank, Error> {
    let names = strings(strings_bank)?;
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"FEV ") {
        return err("not an FMOD Studio bank (RIFF FEV)");
    }
    let tree = nodes(&bytes[12..]);
    let mut o = Objects::default();
    o.walk(b"FEV ", &tree).ok_or(Error("bank: a short object".to_owned()))?;
    // The sound banks' samples one after the other; a waveform's (bank, sample) is an index into them.
    let mut samples = Vec::new();
    let mut first = Vec::new();
    for snd in &o.sound_banks {
        let start = snd.windows(4).position(|w| w == b"FSB5").ok_or(Error("SND: no FSB5".to_owned()))?;
        let base = snd.as_ptr() as usize - bytes.as_ptr() as usize + start;
        first.push(samples.len());
        samples.extend(
            fsb5(&snd[start..])?.into_iter().map(|s| Sample { data: s.data.start + base..s.data.end + base, ..s }),
        );
    }
    let waveforms: HashMap<Id, usize> =
        o.waveforms.iter().filter_map(|(id, &(bank, k))| Some((*id, first.get(bank as usize)? + k as usize))).collect();
    let param_index: HashMap<Id, usize> = o.params.iter().enumerate().map(|(k, (id, _))| (*id, k)).collect();
    // The mixing tree: each event's master track goes into its input bus.
    let mut routes = HashMap::new();
    for body in &o.events {
        if let (Some(input), Some(master)) = (id_at(body, 48), id_at(body, 64)) {
            if o.buses.contains_key(&input) && o.buses.contains_key(&master) {
                routes.insert(master, input);
            }
        }
    }
    let mut bus_ids: Vec<Id> = o.buses.keys().copied().collect();
    bus_ids.sort_unstable();
    let bus_index: HashMap<Id, usize> = bus_ids.iter().enumerate().map(|(k, id)| (*id, k)).collect();
    let mut effect_at = HashMap::new();
    let mut buses = Vec::new();
    for (k, id) in bus_ids.iter().enumerate() {
        let (parent, fader, pre, post) = &o.buses[id];
        let parent = parent.or(routes.get(id).copied()).and_then(|p| bus_index.get(&p).copied());
        let mut chain = |ids: &[Id], post: bool| -> Vec<Effect> {
            let mut out = Vec::new();
            for e in ids {
                let effect = if let Some((kind, params)) = o.effects.get(e) {
                    Effect { kind: *kind, params: params.clone(), automations: Vec::new(), target: None }
                } else if let Some((target, level)) = o.sends.get(e) {
                    let target = bus_index.get(target).copied();
                    Effect { kind: DSP_SEND, params: vec![*level], automations: Vec::new(), target }
                } else {
                    continue;
                };
                effect_at.insert(*e, (k, post, out.len()));
                out.push(effect);
            }
            out
        };
        let (pre, post) = (chain(pre, false), chain(post, true));
        let name = names.get(id).cloned().unwrap_or_default();
        buses.push(Bus { name, parent, volume_db: *fader, automations: Vec::new(), pre, post });
    }
    let mut events = Vec::new();
    for body in &o.events {
        let Some(name) = id_at(body, 0).and_then(|id| names.get(&id)) else { continue };
        let sheets: Vec<_> = o.sheets.iter().filter(|s| body.windows(16).skip(16).any(|w| w == s.0)).collect();
        let automations = automations(&o, &param_index, &sheets);
        // The automations of the event's buses and their effects.
        for (target, list) in &automations {
            if let Some(&k) = bus_index.get(target) {
                let volume = list.iter().filter(|a| a.property == Property::VolumeDb).cloned();
                buses[k].automations.extend(volume);
            } else if let Some(&(k, post, at)) = effect_at.get(target) {
                let effect = if post { &mut buses[k].post[at] } else { &mut buses[k].pre[at] };
                effect.automations.extend(list.iter().filter(|a| matches!(a.property, Property::Effect(_))).cloned());
            }
        }
        let layers = layers(&o, &param_index, &bus_index, &waveforms, &sheets, &automations, &samples)?;
        let fluctuation_length = user_property(body, "fluctuation_length");
        events.push(Event { name: name.clone(), layers, fluctuation_length });
    }
    let params = o.params.into_iter().map(|(_, p)| p).collect();
    Ok(Bank { bytes, samples, params, events, buses })
}

type Sheet = (Id, Id, Vec<(Id, f32, f32)>, Vec<Id>);

/// The value of the user property `name` in an event's body: its u16 name length and name, then an f32.
fn user_property(body: &[u8], name: &str) -> Option<f32> {
    let mut key = (name.len() as u16).to_le_bytes().to_vec();
    key.extend_from_slice(name.as_bytes());
    let at = body.windows(key.len()).position(|w| w == key)?;
    f32_at(body, at + key.len())
}

/// Each automation of an event's sheets, by its target.
fn automations(o: &Objects, param_index: &HashMap<Id, usize>, sheets: &[&Sheet]) -> HashMap<Id, Vec<Automation>> {
    let mut out: HashMap<Id, Vec<Automation>> = HashMap::new();
    for (_, param, _, list) in sheets {
        let Some(&param) = param_index.get(param) else { continue };
        for a in list {
            let (Some((target, curve)), Some((property, map))) = (o.automations.get(a), o.automated.get(a)) else {
                continue;
            };
            let property = match *property {
                VOLUME => Property::VolumeDb,
                PITCH => Property::Semitones,
                INSTRUMENT_VOLUME => Property::Gain,
                p if p >= EFFECT_PARAM => Property::Effect(p - EFFECT_PARAM),
                _ => continue,
            };
            let map = o.maps.get(map);
            let points = o
                .curves
                .get(curve)
                .map_or(Vec::new(), |c| c.iter().map(|&[x, y]| [x, map.map_or(y, |m| interpolate(m, y))]).collect());
            out.entry(*target).or_default().push(Automation { param, property, points });
        }
    }
    out
}

/// The layers of an event with sheets `sheets` and automations `automations`.
fn layers(
    o: &Objects,
    param_index: &HashMap<Id, usize>,
    bus_index: &HashMap<Id, usize>,
    waveforms: &HashMap<Id, usize>,
    sheets: &[&Sheet],
    automations: &HashMap<Id, Vec<Automation>>,
    samples: &[Sample],
) -> Result<Vec<Layer>, Error> {
    let mut out: Vec<(Id, Layer)> = Vec::new();
    for (_, param, placed, _) in sheets {
        let Some(&param) = param_index.get(param) else { continue };
        for &(id, start, end) in placed {
            let region = Region { param, start, end };
            if let Some((_, layer)) = out.iter_mut().find(|(i, _)| *i == id) {
                layer.regions.push(region);
                continue;
            }
            let Some((parts, inst, multi)) = o.instruments.get(&id) else { continue };
            let played: Vec<Id> = if *multi {
                parts.iter().filter_map(|p| o.instruments.get(p)).filter(|(_, _, m)| !m).map(|(w, _, _)| w[0]).collect()
            } else {
                parts.clone()
            };
            let sample_list: Vec<usize> =
                played.iter().filter_map(|w| waveforms.get(w).copied()).filter(|&k| k < samples.len()).collect();
            if sample_list.is_empty() {
                continue;
            }
            let short = || Error("INST: short".to_owned());
            let auto_pitch = param_index.get(&id_at(inst, 53).ok_or_else(short)?).map(|&param| AutoPitch {
                param,
                reference: f32_at(inst, 69).unwrap_or(1.0),
                min: f32_at(inst, 73).unwrap_or(0.0),
            });
            let track = id_at(inst, 83);
            let mut layer = Layer {
                samples: sample_list,
                regions: vec![region],
                auto_pitch,
                volume_db: f32_at(inst, 16).ok_or_else(short)?,
                semitones: f32_at(inst, 20).ok_or_else(short)?,
                automations: automations.get(&id).cloned().unwrap_or_default(),
                bus: track.and_then(|t| bus_index.get(&t).copied()),
            };
            // The pitch automations of the tracks above it, up to the event's master track.
            let mut track = track;
            for _ in 0..16 {
                let Some(t) = track else { break };
                let Some((parent, ..)) = o.buses.get(&t) else { break };
                let pitch = automations.get(&t).into_iter().flatten().filter(|a| a.property == Property::Semitones);
                layer.automations.extend(pitch.cloned());
                track = *parent;
            }
            out.push((id, layer));
        }
    }
    Ok(out.into_iter().map(|(_, l)| l).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_is_linear_and_held_at_the_ends() {
        let points = [[1000.0, 0.0], [2000.0, 1.0]];
        assert_eq!([0.0, 1000.0, 1500.0, 2000.0, 9000.0].map(|x| interpolate(&points, x)), [0.0, 0.0, 0.5, 1.0, 1.0]);
        assert_eq!(interpolate(&[], 1.0), 0.0);
    }

    #[test]
    fn lists_count_twice_plus_one() {
        let b = [5, 0, 2, 0, 1, 2, 3, 4, 9];
        let (items, next) = list_at(&b, 0).unwrap();
        assert_eq!(items, [&[1, 2][..], &[3, 4][..]]);
        assert_eq!(next, 8);
        assert_eq!(list_at(&[1, 0, 8, 0], 0).unwrap(), (Vec::new(), 4));
    }

    /// Every bank of `RBR_RSF`'s `AudioFMOD/` reads: plain banks, PCM or Vorbis samples (with a setup CRC), and an engine
    /// event inside and outside whose layers play recordings of the bank, their effects of the DSP types listed here.
    #[test]
    fn every_rsf_bank_reads() {
        let Some(root) = std::env::var_os("RBR_RSF") else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let dir = std::path::Path::new(&root).join("AudioFMOD");
        let mut read = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".bank").filter(|s| !s.ends_with(".strings")) else { continue };
            let strings = std::fs::read(dir.join(format!("{stem}.strings.bank"))).unwrap();
            let bank = parse(std::fs::read(&path).unwrap(), &strings).unwrap_or_else(|e| panic!("{name}: {e}"));
            for s in &bank.samples {
                assert!([PCM16, VORBIS].contains(&s.codec), "{name} {}: codec {}", s.name, s.codec);
                assert!(s.vorbis_crc.is_some() == (s.codec == VORBIS) && !s.data.is_empty(), "{name} {}", s.name);
            }
            for event in ["event:/engine_int", "event:/engine_ext"] {
                let e = bank.event(event).unwrap_or_else(|| panic!("{name}: no {event}"));
                assert!(e.layers.len() >= 2, "{name} {event}: {} layers", e.layers.len());
                let rpm = e.layers.iter().filter(|l| l.regions.iter().any(|r| bank.params[r.param].name == "rpms"));
                assert!(rpm.count() >= 2, "{name} {event}: no rpm layers");
                let kinds = [
                    DSP_SEND,
                    DSP_LOWPASS,
                    DSP_HIGHPASS,
                    DSP_FADER,
                    DSP_DISTORTION,
                    DSP_PARAMEQ,
                    DSP_COMPRESSOR,
                    DSP_SFXREVERB,
                    DSP_LOWPASS_SIMPLE,
                    DSP_HIGHPASS_SIMPLE,
                    DSP_PAN,
                    DSP_THREE_EQ,
                    DSP_LOUDNESS_METER,
                    DSP_CONVOLUTION_REVERB,
                    DSP_CHANNEL_MIX,
                ];
                for l in &e.layers {
                    let mut bus = l.bus;
                    let mut depth = 0;
                    while let Some(b) = bus {
                        for f in bank.buses[b].pre.iter().chain(&bank.buses[b].post) {
                            assert!(kinds.contains(&f.kind), "{name} {event}: DSP type {}", f.kind);
                        }
                        bus = bank.buses[b].parent;
                        depth += 1;
                        assert!(depth < 32, "{name} {event}: a bus loop");
                    }
                    // Every layer reaches the mixer's master bus.
                    let master = bank.buses.iter().position(|b| b.name == "bus:/");
                    assert!(depth >= 3 && master.is_some(), "{name} {event}: {depth} buses");
                }
            }
            // The fluctuation lengths the daemon reads (`NgpFmodDaemon.exe` 0x409ab0): on the two engine events and
            // the transmission of every bank.
            for event in ["event:/engine_int", "event:/engine_ext", "event:/transmission"] {
                let length = bank.event(event).and_then(|e| e.fluctuation_length);
                assert!(length.is_some_and(|l| (0.0..=2.0).contains(&l)), "{name} {event}: {length:?}");
            }
            read += 1;
        }
        assert!(read > 0, "no banks in {}", dir.display());
        eprintln!("{read} banks");
    }
}

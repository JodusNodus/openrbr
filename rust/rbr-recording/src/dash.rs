//! The dash record (`DSH1`) a live stepping process writes with each frame (`rbr_recording::live`): what the game's HUD
//! takes from the car besides HUD2 (`rbr_render::hud`). The HUD's `CSigCarSensorData` receiver (`0x573cc0`) copies the
//! warning-lamp values from the payload `CCar::Sample9800` (`0x4e9800`) packs; the HUD's `+0x20` is the car's distance
//! along the driveline; its `CSigCarStandStill` (`0x5735b0`) and `CSigCarProblem` (`0x573740`) receivers pick the
//! message. Payload, little-endian:
//!
//! ```text
//! f32 distance    the car's driveline distance, m (E12_SampleX of CCar +0x17c; NaN: off the driveline)
//! f32 start       the start key's distance, m (NaN: none)
//! f32 finish      the finish key's distance, m (NaN: none)
//! f32 water       sensor p[5]: the water temperature, degrees C (CEngine::SimE970)
//! f32 brakes      sensor p[11]: the E39 block's count (CCar +0x620, +0x80)
//! f32 engine      sensor p[10]: the engine's option block +0x58 (CEngine::Get48_58; 0 without the block)
//! f32 battery     sensor p[12]: the E39 block's scale (+0x8c) times 12, V
//! f32 boost       sensor p[3]: CEngine::SimEE60, the turbo pressure, Pa (0 without a turbo)
//! u32 flags       bit 0: the engine runs (sensor p[14], CEngine::SimE980); bit 1: a CSigCarStandStill this step;
//!                 bit 2: a CSigCarProblem this step; bit 3: the car goes the wrong way (the HUD's M579590 test,
//!                 `rbr_gameplay::driveline::wrong_way`); bits 8-15: the rally false starts for the warning banner
//!                 (HUD +0x148, `AssistSettings::false_start_warning`; 0 none)
//! u32 still[2]    the last CSigCarStandStill payload of the step: its count and its kind (0 when none)
//! u32 problem     the last CSigCarProblem payload's first word (0 when none)
//! u32 splits      the split keys' count, then their distances (f32 each)
//! u32 callouts    the PCN1 record's callout count, then each callout's key distance (f32), in PCN1's order
//! optional, the tail (a reader takes its absence as no start clock and no stack):
//! f32 clock       the start clock's time of day, s after midnight (`rbr_gameplay::weather::start_clock`; NaN: none)
//! u32 stack       the pace stack's key count (`rbr_gameplay::markers::pace_stack`), then for each: u32 tag,
//!                 u32 word1, f32 distance, f32 x, y, z (the key's driveline point, physics frame; NaN: none)
//! ```
//!
//! Ours: the game's HUD reads the signals, not a record.

use crate::splits::{option, value};

/// The tag.
pub const DSH1: [u8; 4] = *b"DSH1";
/// The payload's size without the two lists.
const HEAD_BYTES: usize = 4 * 12;
/// The flag bits.
const ENGINE_RUNS: u32 = 1;
const STILL: u32 = 2;
const PROBLEM: u32 = 4;
const WRONG_WAY: u32 = 8;
const FALSE_STARTS_SHIFT: u32 = 8;
/// Words per pace-stack key.
const STACK_WORDS: usize = 6;
/// Most entries a list may have (a guard against a bad stream).
const MAX_LIST: usize = 256;

/// One dash record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dash {
    pub distance: Option<f32>,
    pub start: Option<f32>,
    pub finish: Option<f32>,
    pub water: f32,
    pub brakes: f32,
    pub engine: f32,
    pub battery: f32,
    pub boost: f32,
    pub engine_runs: bool,
    /// The step's `CSigCarStandStill` payload: its count and its kind.
    pub still: Option<[u32; 2]>,
    /// The step's `CSigCarProblem` kind.
    pub problem: Option<u32>,
    pub splits: Vec<f32>,
    /// The key distance of each of the step's pace-note callouts, in PCN1's order.
    pub callouts: Vec<f32>,
    /// The car goes the wrong way (the HUD's test, before its 2 s wait).
    pub wrong_way: bool,
    /// The rally false starts the warning banner names (0 none).
    pub false_starts: u8,
    /// The start clock's time of day, s after midnight.
    pub clock: Option<f32>,
    /// The pace stack's keys: the symbol key behind the car, when there is one, then the next ones.
    pub stack: Vec<StackKey>,
}

/// A key of the pace stack.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StackKey {
    pub tag: u32,
    pub word1: u32,
    pub distance: f32,
    /// The key's point on the driveline, physics frame (z up).
    pub position: Option<[f32; 3]>,
}

impl Dash {
    /// The record's payload.
    pub fn to_bytes(&self) -> Vec<u8> {
        let flags = if self.engine_runs { ENGINE_RUNS } else { 0 }
            | if self.still.is_some() { STILL } else { 0 }
            | if self.problem.is_some() { PROBLEM } else { 0 }
            | if self.wrong_way { WRONG_WAY } else { 0 }
            | u32::from(self.false_starts) << FALSE_STARTS_SHIFT;
        let still = self.still.unwrap_or_default();
        let floats = [
            value(self.distance),
            value(self.start),
            value(self.finish),
            self.water,
            self.brakes,
            self.engine,
            self.battery,
            self.boost,
        ];
        let mut words: Vec<u32> = floats.iter().map(|f| f.to_bits()).collect();
        words.extend([flags, still[0], still[1], self.problem.unwrap_or(0)]);
        for list in [&self.splits, &self.callouts] {
            words.push(list.len() as u32);
            words.extend(list.iter().map(|f| f.to_bits()));
        }
        if self.clock.is_some() || !self.stack.is_empty() {
            words.push(value(self.clock).to_bits());
            words.push(self.stack.len() as u32);
            for key in &self.stack {
                let [x, y, z] = key.position.unwrap_or([f32::NAN; 3]);
                words.extend([key.tag, key.word1, key.distance.to_bits(), x.to_bits(), y.to_bits(), z.to_bits()]);
            }
        }
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// The record from its payload, or `None` when the payload's size does not match its lists.
    pub fn from_bytes(bytes: &[u8]) -> Option<Dash> {
        if bytes.len() < HEAD_BYTES + 4 || !bytes.len().is_multiple_of(4) {
            return None;
        }
        let words: Vec<u32> = bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        let float = |i: usize| f32::from_bits(words[i]);
        let mut at = HEAD_BYTES / 4;
        let mut list = || {
            let count = *words.get(at)? as usize;
            let floats = words.get(at + 1..at + 1 + count).filter(|_| count <= MAX_LIST)?;
            at += 1 + count;
            Some(floats.iter().map(|w| f32::from_bits(*w)).collect::<Vec<_>>())
        };
        let (splits, callouts) = (list()?, list()?);
        let (mut clock, mut stack) = (None, Vec::new());
        if at != words.len() {
            clock = option(f32::from_bits(*words.get(at)?));
            let count = *words.get(at + 1)? as usize;
            let body = words.get(at + 2..at + 2 + count * STACK_WORDS).filter(|_| count <= MAX_LIST)?;
            stack = body
                .chunks_exact(STACK_WORDS)
                .map(|w| {
                    let x = f32::from_bits(w[3]);
                    StackKey {
                        tag: w[0],
                        word1: w[1],
                        distance: f32::from_bits(w[2]),
                        position: (!x.is_nan()).then(|| [x, f32::from_bits(w[4]), f32::from_bits(w[5])]),
                    }
                })
                .collect();
            at += 2 + count * STACK_WORDS;
        }
        if at != words.len() {
            return None;
        }
        let flags = words[8];
        Some(Dash {
            distance: option(float(0)),
            start: option(float(1)),
            finish: option(float(2)),
            water: float(3),
            brakes: float(4),
            engine: float(5),
            battery: float(6),
            boost: float(7),
            engine_runs: flags & ENGINE_RUNS != 0,
            still: (flags & STILL != 0).then_some([words[9], words[10]]),
            problem: (flags & PROBLEM != 0).then_some(words[11]),
            splits,
            callouts,
            wrong_way: flags & WRONG_WAY != 0,
            false_starts: (flags >> FALSE_STARTS_SHIFT) as u8,
            clock,
            stack,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips() {
        let dash = Dash {
            distance: Some(1234.5),
            start: Some(20.0),
            finish: None,
            water: 86.8,
            brakes: 1.0,
            engine: 1.0,
            battery: 12.0,
            boost: 150_000.0,
            engine_runs: true,
            still: Some([6, 3]),
            problem: None,
            splits: vec![3000.0, 6000.0],
            callouts: vec![1300.0],
            wrong_way: true,
            false_starts: 2,
            clock: None,
            stack: Vec::new(),
        };
        let bytes = dash.to_bytes();
        assert_eq!(bytes.len(), HEAD_BYTES + 4 + 8 + 4 + 4);
        assert_eq!(Dash::from_bytes(&bytes), Some(dash.clone()));
        assert_eq!(Dash::from_bytes(&bytes[..bytes.len() - 4]), None);
        let key = StackKey { tag: 3, word1: 0, distance: 1400.0, position: Some([1.0, 2.0, 3.0]) };
        let tailed = Dash { clock: Some(29_700.0), stack: vec![key, StackKey { position: None, ..key }], ..dash };
        let bytes = tailed.to_bytes();
        assert_eq!(bytes.len(), HEAD_BYTES + 4 + 8 + 4 + 4 + 8 + 2 * 24);
        assert_eq!(Dash::from_bytes(&bytes), Some(tailed));
        assert_eq!(Dash::from_bytes(&bytes[..bytes.len() - 4]), None);
        let empty = Dash::default();
        assert_eq!(Dash::from_bytes(&empty.to_bytes()).unwrap().distance, None);
    }
}

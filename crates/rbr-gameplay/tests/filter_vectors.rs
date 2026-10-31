//! Replays the original's direct calls of the input filter's `Step` (0x48af90) and the quantizer's `Pack` (0x48c910)
//! through `filter::step` and `QuantizedControl::pack`. The records come from harness/vector_specs/gameplay_filter.py
//! (`harness/direct_calls.py --set gameplay_filter`), on fuzzed inputs: a playback feeds the filter only zeros, so the
//! curve, the rate limits, the fade and the steering byte are checked here and nowhere else.
//!
//! Needs `RBR_FILTER_VECTORS=<file>[:<file>...]`, the record files, or `RBR_VECTOR_STORE` (its `gameplay_filter/`);
//! skipped without either. The record format is the one in
//! harness/vcu_vectors.py: each call's arguments (`this` first), the windows before and after, and the x87 return.

use rbr_gameplay::control::QuantizedControl;
use rbr_gameplay::filter::{step, Cell, Filtered};

mod common;
use common::hex;

const STEP: u32 = 0x48af90;
const PACK: u32 = 0x48c910;
/// The filter header Step reads: five groups of 0x94 bytes (the selector word, then four 0x24-byte cells), and the
/// speed fade input m_36c at +0x36c. The record's window of the filter covers all of it.
const GROUP: usize = 0x94;
const CELL: usize = 0x24;
const SPEED: usize = 0x36c;
/// The nine bytes Pack writes into the state (E16SteerState +0x74..+0x7c).
const BYTES: std::ops::Range<usize> = 0x74..0x7d;

/// One window of a call: the bytes before the call and after it.
struct Window {
    pre: Vec<u8>,
    post: Vec<u8>,
}

struct Call {
    addr: u32,
    args: Vec<u32>,
    /// Window 0 is the `this` of a thiscall; window 1 the stack pointer the call takes.
    windows: Vec<Window>,
    st0: f64,
}

fn parse(text: &str) -> Vec<Call> {
    let mut calls: Vec<Call> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first().copied() {
            Some("call") => calls.push(Call {
                addr: u32::from_str_radix(f[2].trim_start_matches("0x"), 16).unwrap(),
                args: Vec::new(),
                windows: Vec::new(),
                st0: 0.0,
            }),
            Some("args") => {
                let args = f[1..].iter().map(|w| u32::from_str_radix(w, 16).unwrap());
                calls.last_mut().unwrap().args = args.collect();
            }
            Some("win") => {
                let bytes = hex(f[5]);
                let call = calls.last_mut().unwrap();
                match f[4] {
                    "pre" => call.windows.push(Window { pre: bytes, post: Vec::new() }),
                    _ => call.windows.last_mut().unwrap().post = bytes,
                }
            }
            Some("ret") => {
                let bits = u64::from_str_radix(f[2], 16).unwrap();
                calls.last_mut().unwrap().st0 = f64::from_bits(bits);
            }
            _ => {}
        }
    }
    calls
}

/// The records of every file in [`record_paths`].
fn records() -> Option<Vec<Call>> {
    let calls = record_paths("RBR_FILTER_VECTORS", "gameplay_filter")?
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            parse(&text)
        })
        .collect();
    Some(calls)
}

/// The record files: those `var` names, or else every `.txt` in `$RBR_VECTOR_STORE/<dir>`; `None` without either.
fn record_paths(var: &str, dir: &str) -> Option<Vec<std::path::PathBuf>> {
    if let Ok(paths) = std::env::var(var) {
        return Some(paths.split(':').map(Into::into).collect());
    }
    let dir = std::path::Path::new(&std::env::var_os("RBR_VECTOR_STORE")?).join(dir);
    let mut paths: Vec<_> = std::fs::read_dir(dir).ok()?.map(|e| e.unwrap().path()).collect();
    paths.retain(|p| p.extension().is_some_and(|x| x == "txt"));
    paths.sort();
    Some(paths)
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn float(bytes: &[u8], at: usize) -> f32 {
    f32::from_bits(word(bytes, at))
}

/// The cell `Step` uses for a channel: the group's selector picks one of its four cells.
fn cell_of(header: &[u8], channel: usize) -> Cell {
    let group = channel * GROUP;
    let at = group + 4 + word(header, group) as usize * CELL;
    let w = |i: usize| word(header, at + 4 * i);
    Cell {
        active: w(0) != 0,
        nonlinear: w(1) != 0,
        lin: float(header, at + 8),
        c2: float(header, at + 12),
        c3: float(header, at + 16),
        rise: float(header, at + 20),
        fall: float(header, at + 24),
        fade: float(header, at + 28),
        fade_speed: float(header, at + 32),
    }
}

#[test]
fn step_matches_the_original_on_every_record() {
    let Some(calls) = records() else {
        eprintln!("RBR_FILTER_VECTORS and RBR_VECTOR_STORE unset; skipped");
        return;
    };
    let mut checked = 0;
    for (n, call) in calls.iter().filter(|c| c.addr == STEP).enumerate() {
        let &[_this, channel, a, b, dt] = call.args.as_slice() else {
            panic!("Step record {n}: {} arguments", call.args.len());
        };
        let header = &call.windows[0];
        assert_eq!(header.pre, header.post, "Step record {n} writes the filter");
        let speed = float(&header.pre, SPEED);
        let got = step(
            &cell_of(&header.pre, channel as usize),
            f32::from_bits(a),
            f32::from_bits(b),
            f32::from_bits(dt),
            speed,
        );
        // the x87 return is a float, so the widened value narrows back exactly
        let want = call.st0 as f32;
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "Step record {n}: channel {channel}, a {:?}, b {:?}, dt {:?}, speed {speed:?}: ours {got:?}, original {want:?}",
            f32::from_bits(a),
            f32::from_bits(b),
            f32::from_bits(dt),
        );
        checked += 1;
    }
    assert!(checked > 0, "no Step records");
    eprintln!("Step: {checked} records equal");
}

#[test]
fn pack_matches_the_original_on_every_record() {
    let Some(calls) = records() else {
        eprintln!("RBR_FILTER_VECTORS and RBR_VECTOR_STORE unset; skipped");
        return;
    };
    let mut checked = 0;
    for (n, call) in calls.iter().filter(|c| c.addr == PACK).enumerate() {
        let (state, input) = (&call.windows[0], &call.windows[1]);
        assert_eq!(input.pre, input.post, "Pack record {n} writes its input");
        let p = &input.pre;
        let filtered = Filtered {
            flags: word(p, 0),
            aux: word(p, 4),
            byte2: word(p, 8),
            // steering, then the pedal channels 1 to 4 (+0x18 to +0x24)
            channels: [0x14, 0x18, 0x1c, 0x20, 0x24].map(|at| float(p, at)),
            gear: p[0x28],
        };
        let q = QuantizedControl::pack(&filtered);
        // byte 4 is filtered channel 1 (+0x18), the payload's word 0: the throttle as the car reads it
        let packed = [q.flags, q.aux, q.byte2, q.steering as u8, q.throttle, q.brake, q.handbrake, q.clutch, q.gear];
        assert_eq!(&state.post[BYTES], &packed, "Pack record {n}: the nine bytes");
        let outside = |b: &[u8]| [b[..BYTES.start].to_vec(), b[BYTES.end..].to_vec()];
        assert_eq!(outside(&state.post), outside(&state.pre), "Pack record {n} writes past its bytes");
        checked += 1;
    }
    assert!(checked > 0, "no Pack records");
    eprintln!("Pack: {checked} records equal");
}

//! Reads the call records of `harness/vcu_vectors.py` (the format is in its docstring) and maps
//! their windows into a `Mem`. Shared by the test files that replay original calls.

#![allow(dead_code)]

use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::{Backend, Mem};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A recorded memory window: its index, address and length, and the bytes before and after the call.
#[derive(Clone, Debug)]
pub struct Window {
    pub index: usize,
    pub at: u32,
    pub len: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

/// A memory run written by the call (an allocated object or a stack buffer), read on return.
#[derive(Clone, Debug)]
pub struct After {
    pub at: u32,
    pub len: usize,
    pub bytes: Vec<u8>,
}

/// One recorded call.
pub struct Call {
    pub index: usize,
    pub addr: u32,
    pub args: Vec<u32>,
    pub windows: Vec<Window>,
    pub after: Vec<After>,
    pub eax: u32,
    pub st0_bits: u64,
    /// External calls made by this one, in call order.
    pub inner: Vec<Inner>,
    pub branch: String,
}

/// An external call recorded inside a listed function (its arguments, `this` first, and its eax), with its own
/// windows (the bytes it reads and writes through its arguments) and its `after` runs.
pub struct Inner {
    pub addr: u32,
    pub eax: u32,
    /// The x87 result (st0) as f64 bits, for a float-returning callee.
    pub st0_bits: u64,
    pub args: Vec<u32>,
    pub windows: Vec<Window>,
    pub after: Vec<After>,
}

/// Two hex digits per byte (`parse` and the format tests share it).
pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn word(s: &str) -> u32 {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

pub fn parse(text: &str) -> Vec<Call> {
    let mut calls: Vec<Call> = Vec::new();
    let mut pending: Vec<(usize, u32, usize, Vec<u8>)> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.first().copied() {
            Some("call") => calls.push(Call {
                index: f[1].parse().unwrap(),
                addr: word(f[2]),
                args: Vec::new(),
                windows: Vec::new(),
                after: Vec::new(),
                eax: 0,
                st0_bits: 0,
                inner: Vec::new(),
                branch: String::new(),
            }),
            Some("args") => {
                let call = calls.last_mut().unwrap();
                call.args = f[1..].iter().map(|w| word(w)).collect();
            }
            Some("win") => {
                let k: usize = f[1].parse().unwrap();
                let at = word(f[2]);
                let len: usize = f[3].parse().unwrap();
                let bytes = if f.len() > 5 { hex(f[5]) } else { Vec::new() };
                if f[4] == "pre" {
                    pending.push((k, at, len, bytes));
                } else {
                    let (k0, at0, len0, pre) = pending.pop().unwrap();
                    assert_eq!((k0, at0, len0), (k, at, len));
                    calls.last_mut().unwrap().windows.push(Window { index: k, at, len, before: pre, after: bytes });
                }
            }
            Some("ret") => {
                let call = calls.last_mut().unwrap();
                call.eax = word(f[1]);
                call.st0_bits = u64::from_str_radix(f[2], 16).unwrap();
            }
            Some("inner") => calls.last_mut().unwrap().inner.push(Inner {
                addr: word(f[1]),
                eax: u32::from_str_radix(f[2], 16).unwrap(),
                st0_bits: u64::from_str_radix(f[3], 16).unwrap(),
                args: f[4..].iter().map(|w| u32::from_str_radix(w, 16).unwrap()).collect(),
                windows: Vec::new(),
                after: Vec::new(),
            }),
            // An inner call's window (`iwin k addr len pre post`) and its after run (`iawin addr len hex`) follow its line.
            Some("iwin") => {
                let k: usize = f[1].parse().unwrap();
                let len: usize = f[3].parse().unwrap();
                let inner = calls.last_mut().unwrap().inner.last_mut().unwrap();
                inner.windows.push(Window {
                    index: k,
                    at: word(f[2]),
                    len,
                    before: f.get(4).map_or(Vec::new(), |h| hex(h)),
                    after: f.get(5).map_or(Vec::new(), |h| hex(h)),
                });
            }
            Some("iawin") => {
                let len: usize = f[2].parse().unwrap();
                let bytes = hex(f[3]);
                calls.last_mut().unwrap().inner.last_mut().unwrap().after.push(After { at: word(f[1]), len, bytes });
            }
            Some("awin") => {
                let len: usize = f[2].parse().unwrap();
                let bytes = hex(f[3]);
                calls.last_mut().unwrap().after.push(After { at: word(f[1]), len, bytes });
            }
            Some("branch") => calls.last_mut().unwrap().branch = f[1].to_string(),
            _ => {}
        }
    }
    calls
}

/// The recorded calls of `addr` in the files named by the environment variable `var` (separated by
/// `:`), or None when the variable is unset (the test is skipped).
pub fn load(var: &str, addr: u32) -> Option<Vec<Call>> {
    let paths = std::env::var(var).ok()?;
    let mut calls = Vec::new();
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        calls.extend(parse(&text).into_iter().filter(|c| c.addr == addr));
    }
    Some(calls)
}

/// The recorded calls of `addr` made inside other listed calls (its `inner` records), in call order, from the files
/// named by `var`; None when the variable is unset.
pub fn load_inner(var: &str, addr: u32) -> Option<Vec<Inner>> {
    let paths = std::env::var(var).ok()?;
    let mut inner = Vec::new();
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        for call in parse(&text) {
            inner.extend(call.inner.into_iter().filter(|i| i.addr == addr));
        }
    }
    Some(inner)
}

/// Replays every recorded call of `addr` in `var`'s files through `run`: the windows are mapped from the record,
/// `run(call, mem)` runs the port, then every window must hold the original's bytes after the call. The calls, or
/// `None` (said why) when there are none.
fn replay_calls(var: &str, addr: u32, name: &str, mut run: impl FnMut(&Call, &mut Mem)) -> Option<Vec<Call>> {
    let Some(calls) = load(var, addr) else {
        eprintln!("skipping {name}: {var} not set");
        return None;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return None;
    }
    for call in &calls {
        let mut mem = map_windows(call);
        run(call, &mut mem);
        check_windows(call, &mem, name);
    }
    Some(calls)
}

/// [`replay_calls`], with the count in the summary.
pub fn replay(var: &str, addr: u32, name: &str, run: impl FnMut(&Call, &mut Mem)) {
    if let Some(calls) = replay_calls(var, addr, name, run) {
        eprintln!("{}: {} calls match", name, calls.len());
    }
}

/// [`replay_calls`], with the calls counted by branch (`call.branch`) in the summary.
pub fn replay_branches(var: &str, addr: u32, name: &str, run: impl FnMut(&Call, &mut Mem)) {
    if let Some(calls) = replay_calls(var, addr, name, run) {
        let mut branches = BTreeMap::<String, usize>::new();
        for call in &calls {
            *branches.entry(call.branch.clone()).or_default() += 1;
        }
        eprintln!("{}: {} calls match, by branch: {branches:?}", name, calls.len());
    }
}

/// Replays every recorded call of `addr` in `var`'s files through `port(mem, args)`, which returns eax. The windows
/// are mapped from the record, the port runs, then eax (when `eax_checked`: the original returns a value) and every
/// window are compared with the original's.
pub fn replay_eax(var: &str, addr: u32, name: &str, eax_checked: bool, port: impl Fn(&mut Mem, &[u32]) -> u32) {
    replay_branches(var, addr, name, |call, mem| {
        let eax = port(mem, &call.args);
        if eax_checked {
            assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        }
    });
}

/// Four little-endian f32 lanes of a 16-byte record (`math::vec::Vec4`).
pub fn vec4_of(bytes: &[u8]) -> Vec4 {
    let lane = |k: usize| f32::from_le_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap());
    Vec4 { x: lane(0), y: lane(1), z: lane(2), w: lane(3) }
}

/// The f32 port value as the record holds it (st0 as f64 bits, exact).
pub fn st0_of(value: f32) -> u64 {
    (value as f64).to_bits()
}

/// The port's f32 return against the record's st0, bit for bit (the NaN sign too).
pub fn check_st0(call: &Call, value: f32, name: &str) {
    assert_eq!(st0_of(value), call.st0_bits, "{name} call {}: st0", call.index);
}

/// The port's f64 return against the record's st0: a NaN matches any NaN (the default NaN's sign differs by
/// platform), other values bit for bit.
pub fn check_st0_nan_tolerant(call: &Call, st0: impl Into<f64>, name: &str) {
    let want = f64::from_bits(call.st0_bits);
    let got: f64 = st0.into();
    assert!(
        got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
        "{name} call {}: st0 {got} vs {want} ({})",
        call.index,
        call.branch
    );
}

/// Window `k` of an external call.
pub fn inner_window(inner: &Inner, k: usize) -> &Window {
    inner.windows.iter().find(|w| w.index == k).unwrap_or_else(|| panic!("inner call {:#x}: no window {k}", inner.addr))
}

/// The pre and post bytes of the window at `at` of an external call.
pub fn inner_window_at(inner: &Inner, at: u32) -> (&[u8], &[u8]) {
    let window = inner
        .windows
        .iter()
        .find(|w| w.at == at)
        .unwrap_or_else(|| panic!("no window at {at:#x} in the external call"));
    (&window.before, &window.after)
}

/// Maps the windows of `call` (the bytes before the call) into a fresh memory.
pub fn map_windows(call: &Call) -> Mem {
    map_window_list(&call.windows)
}

/// A byte map that its clones share, so a host and the test see one memory (`Mem::with_backend`). Reads fail on
/// unmapped bytes, as `Mem`'s own map does.
#[derive(Default)]
struct SharedBytes(Mutex<BTreeMap<u32, u8>>);

impl Backend for SharedBytes {
    fn read(&self, at: u32, out: &mut [u8]) -> bool {
        let bytes = self.0.lock().unwrap();
        (0..out.len() as u32).all(|i| match bytes.get(&(at + i)) {
            Some(&byte) => {
                out[i as usize] = byte;
                true
            }
            None => false,
        })
    }

    fn write(&self, at: u32, src: &[u8]) {
        let mut bytes = self.0.lock().unwrap();
        for (i, &byte) in src.iter().enumerate() {
            bytes.insert(at + i as u32, byte);
        }
    }
}

/// An empty memory on a shared byte map: its clones are the same memory, so a host can keep one.
pub fn shared_mem() -> Mem {
    Mem::with_backend(Arc::new(SharedBytes::default()))
}

/// Maps the windows of `call` (the bytes before the call) into a shared memory.
pub fn shared_windows(call: &Call) -> Mem {
    let mut mem = shared_mem();
    for window in &call.windows {
        mem.map(window.at, &window.before);
    }
    mem
}

/// Checks every window of `call` after the port ran: the bytes must be the original's.
pub fn check_windows(call: &Call, mem: &Mem, name: &str) {
    for window in &call.windows {
        assert_eq!(mem.read(window.at, window.len), window.after, "{name} call {} window {}", call.index, window.index);
    }
    check_after(&call.after, mem, name, call.index);
}

/// Checks the runs a call wrote and the caller reads on return (`after`): the bytes must be the original's.
pub fn check_after(after: &[After], mem: &Mem, name: &str, index: usize) {
    for run in after {
        assert_eq!(mem.read(run.at, run.len), run.bytes, "{name} call {index}: after {:#x}", run.at);
    }
}

/// Maps the runs `after` (written by the call) as zeros: an allocated object starts zeroed, and the port's writes
/// are checked against the original's bytes (`check_after`).
pub fn map_after_zero(after: &[After], mem: &mut Mem) {
    for run in after {
        mem.map(run.at, &vec![0; run.len]);
    }
}

/// Maps a list of windows (the bytes before the call) into a fresh memory.
pub fn map_window_list(windows: &[Window]) -> Mem {
    let mut mem = Mem::new();
    for window in windows {
        mem.map(window.at, &window.before);
    }
    mem
}

/// Checks a list of windows after the port ran (an inner call's windows, which have no `Call` of their own).
pub fn check_window_list(windows: &[Window], mem: &Mem, name: &str) {
    for window in windows {
        assert_eq!(mem.read(window.at, window.len), window.after, "{name} window {}", window.index);
    }
}

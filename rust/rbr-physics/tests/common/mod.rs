//! Reads the call records of `harness/vcu_vectors.py` (the format is in its docstring) and maps
//! their windows into a `Mem`. Shared by the test files that replay original calls.

#![allow(dead_code)]

use rbr_physics::mem::{Backend, Mem};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// A recorded memory window: (index, address, length, bytes before, bytes after).
pub type Window = (usize, u32, usize, Vec<u8>, Vec<u8>);

/// A memory run written by the call (an allocated object or a stack buffer), read on return: (address, length, bytes).
pub type After = (u32, usize, Vec<u8>);

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

fn hex(s: &str) -> Vec<u8> {
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
                    calls.last_mut().unwrap().windows.push((k, at, len, pre, bytes));
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
                inner.windows.push((
                    k,
                    word(f[2]),
                    len,
                    f.get(4).map_or(Vec::new(), |h| hex(h)),
                    f.get(5).map_or(Vec::new(), |h| hex(h)),
                ));
            }
            Some("iawin") => {
                let len: usize = f[2].parse().unwrap();
                let bytes = hex(f[3]);
                calls.last_mut().unwrap().inner.last_mut().unwrap().after.push((word(f[1]), len, bytes));
            }
            Some("awin") => {
                let len: usize = f[2].parse().unwrap();
                let bytes = hex(f[3]);
                calls.last_mut().unwrap().after.push((word(f[1]), len, bytes));
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

/// Maps the windows of `call` (the bytes before the call) into a fresh memory.
pub fn map_windows(call: &Call) -> Mem {
    let mut mem = Mem::new();
    for (_, at, _, pre, _) in &call.windows {
        mem.map(*at, pre);
    }
    mem
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
    for (_, at, _, pre, _) in &call.windows {
        mem.map(*at, pre);
    }
    mem
}

/// Checks every window of `call` after the port ran: the bytes must be the original's.
pub fn check_windows(call: &Call, mem: &Mem, name: &str) {
    for (k, at, len, _, post) in &call.windows {
        assert_eq!(mem.read(*at, *len), *post, "{name} call {} window {k}", call.index);
    }
    check_after(&call.after, mem, name, call.index);
}

/// Checks the runs a call wrote and the caller reads on return (`after`): the bytes must be the original's.
pub fn check_after(after: &[After], mem: &Mem, name: &str, index: usize) {
    for (at, len, bytes) in after {
        assert_eq!(mem.read(*at, *len), *bytes, "{name} call {index}: after {at:#x}");
    }
}

/// Maps the runs `after` (written by the call) as zeros: an allocated object starts zeroed, and the port's writes
/// are checked against the original's bytes (`check_after`).
pub fn map_after_zero(after: &[After], mem: &mut Mem) {
    for (at, len, _) in after {
        mem.map(*at, &vec![0; *len]);
    }
}

/// Maps a list of windows (the bytes before the call) into a fresh memory.
pub fn map_window_list(windows: &[Window]) -> Mem {
    let mut mem = Mem::new();
    for (_, at, _, pre, _) in windows {
        mem.map(*at, pre);
    }
    mem
}

/// Checks a list of windows after the port ran (an inner call's windows, which have no `Call` of their own).
pub fn check_window_list(windows: &[Window], mem: &Mem, name: &str) {
    for (k, at, len, _, post) in windows {
        assert_eq!(mem.read(*at, *len), *post, "{name} window {k}");
    }
}

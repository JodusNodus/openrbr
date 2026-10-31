//! Shared peek-file helpers for the gameplay integration tests: file reads, probe
//! reads and hex vectors.
//!
//! Each test target compiles this module separately and uses only a subset of it.

// Allow the unused subset: every test target compiles this whole module but
// uses only the helpers its tests need.
#![allow(dead_code)]

use rbr_recording::peek::{Probe, Record, Value};
use std::path::Path;

/// The whole file at `path` (panics naming it, as each test does).
pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The peek's value of the named probe in `record`.
pub fn peeked(record: &Record, probes: &[Probe], name: &str) -> Value {
    let index = probes.iter().position(|probe| probe.name == name).unwrap_or_else(|| panic!("no probe {name}"));
    record.value(index).expect("record holds every probe")
}

pub fn peeked_u32(record: &Record, probes: &[Probe], name: &str) -> u32 {
    match peeked(record, probes, name) {
        Value::U32(v) => v,
        other => panic!("{name}: {other:?}, expected u32"),
    }
}

pub fn peeked_f32(record: &Record, probes: &[Probe], name: &str) -> f32 {
    match peeked(record, probes, name) {
        Value::F32(v) => v,
        other => panic!("{name}: {other:?}, expected f32"),
    }
}

/// Bytes of hex text (vector records).
pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

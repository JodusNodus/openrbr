//! The impact path's callees (`E36DamageOps::Slot1`'s: `ZoneScaled`, `ApplyFunctors`, `EngineGate`, `RingStore`,
//! `E35_Rand`) against the original's direct calls (`harness/vector_specs/car_body_impact.py`, recorded with
//! `harness/direct_calls.py --set car_body_impact`). Needs record files: `RBR_CAR_BODY_VECTORS=<file>[:<file>...]`.
//! Without the variable the tests are skipped.

mod common;

use common::{check_windows, load, map_windows, shared_windows, Call, Inner};
use rbr_physics::car_body::layout::{self, DamageHitCalls};
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_CAR_BODY_VECTORS";

/// The signal object's global, which the emits' emitters are read from (not in the records' windows).
const SIGNALS: u32 = 0x0089_3600;
/// A stack for the ports' payload slots (outside the records' windows).
const SP: u32 = 0x0100_0000;

/// The records of `addr`, or None (skipped) without the variable.
fn records(addr: u32, name: &str) -> Option<Vec<Call>> {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return None;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    Some(calls)
}

/// The original's calls out of a record, answered in order from its inner records. A functor's writes are its
/// record's `after` runs, put into `mem` (the port's memory) when it is called.
struct Recorded<'a> {
    call: &'a Call,
    next: usize,
    name: &'a str,
    mem: Mem,
}

impl Recorded<'_> {
    /// The next inner record, which must be a call to `target`.
    fn next(&mut self, target: u32) -> &Inner {
        let (name, index, k) = (self.name, self.call.index, self.next);
        let inner =
            self.call.inner.get(k).unwrap_or_else(|| panic!("{name} call {index}: an extra call to {target:#x}"));
        assert_eq!(inner.addr, target, "{name} call {index}: inner call {k}");
        self.next += 1;
        inner
    }

    fn done(&self) {
        assert_eq!(self.next, self.call.inner.len(), "{} call {}: inner calls", self.name, self.call.index);
    }
}

impl DamageHitCalls for Recorded<'_> {
    fn functor(&mut self, _sp: u32, function: u32, this: u32, args: [u32; 4]) -> u32 {
        let (name, index) = (self.name, self.call.index);
        let inner = self.next(function);
        assert_eq!(inner.args[..], [&[this][..], &args[..]].concat(), "{name} call {index}: the functor's arguments");
        let (eax, after) = (inner.eax, inner.after.clone());
        for (at, _, bytes) in &after {
            self.mem.map(*at, bytes);
        }
        eax
    }

    fn copy_vec4(&mut self, _sp: u32, _dst: u32, _src: u32) {
        unreachable!("the impact callees do not copy vectors")
    }

    /// The signal is the port's stack slot (the record's is the original's), so only the emitter is compared.
    fn emit(&mut self, _sp: u32, emit: u32, emitter: u32, _signal: u32) {
        let (name, index) = (self.name, self.call.index);
        let inner = self.next(emit);
        assert_eq!(inner.args[0], emitter, "{name} call {index}: the emitter");
    }
}

/// Maps the record's windows, and the signal object so that the emitter of an emit (`offset` on it) is the record's.
fn mem_for(call: &Call, offset: u32) -> Mem {
    let mut mem = shared_windows(call);
    let signals = call.inner.first().map_or(0, |c| c.args[0].wrapping_sub(offset));
    mem.set_u32(SIGNALS, signals);
    mem
}

#[test]
fn zone_scaled_matches_original_calls() {
    let name = "CCarDamageHandler::ZoneScaled";
    let Some(calls) = records(0x4EDEE0, name) else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let eax = layout::zone_scaled(&mut mem, call.args[0], call.args[1], f32::from_bits(call.args[2]));
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls", calls.len());
}

#[test]
fn apply_functors_matches_original_calls() {
    let name = "CCarDamageHandler::ApplyFunctors";
    let Some(calls) = records(0x4EC560, name) else {
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        let mut host = Recorded { call, next: 0, name, mem: mem.clone() };
        let [this, idx, scale, best] = call.args[..4].try_into().unwrap();
        let eax = layout::apply_functors(&mut host, &mut mem, this, SP, idx, f32::from_bits(scale), best);
        host.done();
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls", calls.len());
}

#[test]
fn engine_gate_matches_original_calls() {
    let name = "E36DamageOps::EngineGate";
    let Some(calls) = records(0x4EF680, name) else {
        return;
    };
    for call in &calls {
        let mut mem = mem_for(call, 0xB840);
        let mut host = Recorded { call, next: 0, name, mem: mem.clone() };
        let (this, a, b) = (call.args[0], call.args[1], call.args[2]);
        let eax = layout::engine_gate(&mut host, &mut mem, this, SP, f32::from_bits(a), f32::from_bits(b));
        host.done();
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls", calls.len());
}

#[test]
fn ring_store_matches_original_calls() {
    let name = "E36DamageOps::RingStore";
    let Some(calls) = records(0x4EF5F0, name) else {
        return;
    };
    for call in &calls {
        let mut mem = mem_for(call, 0xCF48);
        let mut host = Recorded { call, next: 0, name, mem: mem.clone() };
        let args = call.args[1..5].try_into().unwrap();
        let eax = layout::ring_store(&mut host, &mut mem, call.args[0], SP, args);
        host.done();
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls", calls.len());
}

#[test]
fn damage_rand_matches_original_calls() {
    let name = "E35_Rand";
    let Some(calls) = records(0x4EBD70, name) else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let [lo, hi, seed] = call.args[..3].try_into().unwrap();
        let (value, eax) = layout::damage_rand(&mut mem, f32::from_bits(lo), f32::from_bits(hi), seed);
        assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
        assert_eq!((value as f64).to_bits(), call.st0_bits, "{name} call {}: st0", call.index);
        check_windows(call, &mem, name);
    }
    eprintln!("{name}: {} calls", calls.len());
}

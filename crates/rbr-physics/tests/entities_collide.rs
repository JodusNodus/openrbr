//! `E33Mat34::CollideInit` (0x51bd30) replayed against the original's recorded calls: the miss paths from
//! `harness/record_entities_b4.py` (the `collide-*` vector files) and the hit tails from fuzzed in-step records
//! (`harness/vector_specs/g_collide.py`, set `g-collide`, which also records the frame). Needs
//! `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! The port's external calls are answered from the record: each must be the next recorded call, with the same
//! arguments (a record may hold more stack words than the callee takes), and its recorded bytes are written where the
//! original wrote them. When the record holds the frame, each call's windows in the frame must hold, before the call,
//! the bytes the original had there. The test checks the eax, the call sequence and the call's windows.

mod common;

use common::{load, shared_windows, Inner};
use rbr_physics::entities::layout;
use rbr_physics::entities::layout::CollideHost;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::ZERO_VEC4;

const VAR: &str = "RBR_ENT_VECTORS";
const NAME: &str = "E33Mat34::CollideInit";
const COLLIDE_INIT: u32 = 0x51BD30;
/// The frame's size below the aligned esp, and where the sub-block (the first call's argument) is in it.
const FRAME: u32 = 0x860;
const SUB_BLOCK: u32 = 0x90;

/// The recorded external calls of one CollideInit call, answered in order, over the memory the original's calls wrote.
struct Answers<'a> {
    mem: Mem,
    inner: &'a [Inner],
    next: usize,
    /// The frame `[s, s + FRAME)`, when the record holds it.
    frame: Option<u32>,
}

impl Answers<'_> {
    /// The next recorded call, which must be `addr` (when given) with `args` first. Its windows in the frame must hold
    /// their recorded bytes; its writes are applied to memory. Returns the call.
    fn take(&mut self, addr: Option<u32>, args: &[u32]) -> &Inner {
        let n = self.next;
        let call = self.inner.get(n).unwrap_or_else(|| {
            panic!("the port made external call {n} ({addr:x?} {args:x?}), the original made {}", self.inner.len())
        });
        if let Some(addr) = addr {
            assert_eq!(call.addr, addr, "external call {n}: callee");
        }
        assert!(
            call.args.len() >= args.len() && call.args[..args.len()] == *args,
            "external call {n} ({:#x}): arguments {:x?}, the original's {:x?}",
            call.addr,
            args,
            call.args
        );
        // Every input is checked before any output is written (two windows may be the same bytes).
        for (_, at, len, pre, _) in &call.windows {
            if self.frame.is_some_and(|s| (s..s + FRAME).contains(at)) {
                assert_eq!(&self.mem.read(*at, *len), pre, "external call {n} ({:#x}) input at {at:#x}", call.addr);
            }
        }
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        for (at, _, bytes) in &call.after {
            self.mem.map(*at, bytes);
        }
        self.next += 1;
        call
    }

    fn eax(&mut self, addr: Option<u32>, args: &[u32]) -> u32 {
        self.take(addr, args).eax
    }
}

impl CollideHost for Answers<'_> {
    fn init_block(&mut self, _: u32, block: u32) {
        self.take(Some(0x4F3E80), &[block]);
    }

    fn gate_boxes(&mut self, _: u32, body: u32, a: u32, b: u32) {
        self.take(None, &[body, a, b]);
    }

    fn segment_gate(&mut self, _: u32, host: u32, a: u32, b: u32) -> u32 {
        self.eax(Some(0x4FF070), &[host, a, b, 0])
    }

    fn transform(&mut self, _: u32, body: u32) -> u32 {
        self.eax(None, &[body])
    }

    fn grid_source(&mut self, _: u32, body: u32) -> u32 {
        self.eax(None, &[body])
    }

    fn build_grid(&mut self, _: u32, grid: u32, source: u32, transform: u32, identity: u32) {
        self.take(Some(0x4F6610), &[grid, source, transform, identity]);
    }

    fn copy_from(&mut self, _: u32, to: u32, from: u32) {
        self.take(Some(0x43C5B0), &[to, from]);
    }

    fn ray_query(&mut self, _: u32, host: u32, a: u32, b: u32, out: u32) -> u32 {
        self.eax(Some(0x4FD760), &[host, a, b, out, 0])
    }

    fn fallback_query(&mut self, _: u32, host: u32, a: u32, out: u32) -> u32 {
        self.eax(Some(0x4FEBC0), &[host, a, out, 0])
    }

    fn ray_clear(&mut self, _: u32, grid: u32) {
        self.take(Some(0x4F6740), &[grid]);
    }

    fn compose(&mut self, _: u32, identity: u32, point: u32, transform: u32, out: u32) {
        self.take(Some(0x51B4C0), &[identity, point, transform, out]);
    }

    fn alloc_event(&mut self, _: u32, size: u32) -> u32 {
        self.eax(Some(0x4F4F10), &[size])
    }

    fn init_event(&mut self, _: u32, event: u32, kind: u32, body: u32, contact: u32, point: u32) -> u32 {
        self.eax(Some(0x4F50E0), &[event, kind, body, contact, ZERO_VEC4, point, 0])
    }

    fn sample_b(&mut self, _: u32, grid: u32, index: u32, out_a: u32, out_b: u32) {
        self.take(Some(0x4E0950), &[grid, index, out_a, out_b]);
    }

    fn sample_a(&mut self, _: u32, grid: u32, index: u32, t: f32, out_a: u32, out_b: u32) {
        self.take(Some(0x4E0840), &[grid, index, t.to_bits(), out_a, out_b]);
    }

    fn report(&mut self, _: u32, sink: u32, event: u32) {
        self.take(None, &[sink, event]);
    }

    fn lerp(&mut self, _: u32, a: u32, b: u32, t: f32, out: u32) {
        self.take(Some(0x473C90), &[a, b, t.to_bits(), out]);
    }

    fn normalize(&mut self, _: u32, vector: u32) {
        self.take(Some(0x43C6A0), &[vector]);
    }

    fn length(&mut self, _: u32, vector: u32) -> f32 {
        f64::from_bits(self.take(Some(0x473B30), &[vector]).st0_bits) as f32
    }
}

#[test]
fn collide_init_matches_original_calls() {
    let Some(calls) = load(VAR, COLLIDE_INIT) else {
        eprintln!("skipping {NAME}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{NAME}: no calls in the vector files");
        return;
    }
    let (mut past_gate, mut hits) = (0, 0);
    let mut kinds = [0usize; 3];
    for call in &calls {
        let s = call.inner[0].args[0] - SUB_BLOCK;
        let framed = call.windows.iter().any(|w| w.1 == s);
        let mut mem = shared_windows(call);
        let mut answers = Answers { mem: mem.clone(), inner: &call.inner, next: 0, frame: framed.then_some(s) };
        let eax = layout::collide_init(&mut answers, &mut mem, call.args[0], call.args[1], call.args[3], s + FRAME + 4);
        assert_eq!(eax, call.eax, "{NAME} call {}: eax", call.index);
        assert_eq!(answers.next, call.inner.len(), "{NAME} call {}: external calls", call.index);
        // The frame's own words (the loop's locals, the exception record) are not the port's: its window is not checked.
        for (k, at, len, _, post) in call.windows.iter().filter(|w| !framed || w.1 != s) {
            assert_eq!(&mem.read(*at, *len), post, "{NAME} call {} window {k}", call.index);
        }
        past_gate += usize::from(call.inner.len() > 3);
        hits += usize::from(eax != 0);
        for i in call.inner.iter().filter(|i| i.addr == 0x4F50E0) {
            kinds[(i.args[1] as usize).saturating_sub(3).min(2)] += 1;
        }
    }
    eprintln!(
        "{NAME}: {} calls match ({past_gate} past the gate, {hits} with a contact; events kind 4: {}, kind 5: {})",
        calls.len(),
        kinds[1],
        kinds[2]
    );
}

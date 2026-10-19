//! The entity slots `CEntity::E43VfD4` (0x519ae0) and the dummy's `VfCC` (0x6009b0) and `Vf110` (0x600c80) against a
//! scripted host: the calls' order, entry stack pointers and arguments, and the stores. The game's own runs are checked
//! by `harness/rust_step.py` (rbr-hybrid's bound.csv).

mod common;

use common::shared_mem;
use rbr_physics::entities::dummy::{self, DummyCalls};
use rbr_physics::entities::layout;
use rbr_physics::entities::slots::{self, D4Calls};
use rbr_physics::mem::Mem;

const IDENTITY_GLOBAL: u32 = 0x007c_bb00;
const IDENTITY: u32 = 0x0320_0000;
const DUMMY: u32 = 0x0340_0000;
const SPAN: u32 = 0x0350_0000;
const POST: u32 = 0x0351_0000;
const GROUP: u32 = 0x0360_0000;

/// Maps a block of zeros at `at`.
fn block(mem: &mut Mem, at: u32, len: u32) {
    mem.map(at, &vec![0; len as usize]);
}

/// Logs every call as (name, entry sp, arguments). `fetch_x` writes the origin of its transform (`origins`, lanes
/// x, y, z) to `out`, as the original's `FetchX` does.
struct Script {
    mem: Mem,
    log: Vec<(&'static str, u32, Vec<u32>)>,
    origins: Vec<(u32, [f32; 3])>,
}

impl DummyCalls for Script {
    fn fetch_x(&mut self, sp: u32, this: u32, out: u32, arg: u32) {
        self.log.push(("FetchX", sp, vec![this, out, arg]));
        let (_, lanes) = *self.origins.iter().find(|(t, _)| *t == this).unwrap();
        for (i, lane) in lanes.into_iter().enumerate() {
            self.mem.set_f32(out + 4 * i as u32, lane);
        }
    }

    fn group_on_hit(&mut self, sp: u32, group: u32) {
        self.log.push(("Group", sp, vec![group]));
    }
}

/// A dummy at `DUMMY` with its centre (x 1, z 3, y 2 as stored), half extents (0.5, 0.125, 0.25 as stored), group,
/// span and post; the span and post segments start with flag word 1. `frame` is the base of the test's frame.
fn dummy_memory(frame: u32) -> Mem {
    let mut mem = shared_mem();
    block(&mut mem, IDENTITY_GLOBAL, 4);
    mem.set_u32(IDENTITY_GLOBAL, IDENTITY);
    block(&mut mem, DUMMY, 0x100);
    for (at, v) in [(0x94, 1.0f32), (0x98, 2.0), (0x9c, 3.0), (0xa0, 0.5), (0xa4, 0.25), (0xa8, 0.125)] {
        mem.set_f32(DUMMY + at, v);
    }
    mem.set_u32(DUMMY + 0x7c, SPAN);
    mem.set_u32(DUMMY + 0x80, POST);
    mem.set_u32(DUMMY + 0xb0, GROUP);
    for segment in [SPAN, POST] {
        block(&mut mem, segment, 0x100);
        mem.set_u32(segment + 0x5c, 1);
    }
    block(&mut mem, frame - 0x40, 0x80);
    mem
}

#[test]
fn dummy_span_test_fetches_both_origins_then_handles_a_hit() {
    let frame = 0x0310_0000;
    let (a, b) = (0x0300_0000, 0x0301_0000);
    let mem = dummy_memory(frame);
    // The span from (0, 2, 1) to (2, 4, 3) has centre (1, 3, 2) and half extent (1, 1, 1): on each lane the dummy's
    // centre is at the span's centre, so the boxes overlap.
    let mut script =
        Script { mem: mem.clone(), log: vec![], origins: vec![(a, [0.0, 2.0, 1.0]), (b, [2.0, 4.0, 3.0])] };
    layout::dummy_span_test(&mut mem.clone(), DUMMY, a, b, frame, &mut script);
    assert_eq!(
        script.log,
        vec![
            ("FetchX", frame - 0x10, vec![a, frame + 0x1c, IDENTITY]),
            ("FetchX", frame - 0x10, vec![b, frame + 0xc, IDENTITY]),
            ("Group", frame - 0x8, vec![GROUP]),
        ]
    );
    assert_eq!(mem.u32(SPAN + 0x5c), 9);
    assert_eq!(mem.u32(POST + 0x5c), 9);
}

#[test]
fn dummy_span_test_misses_without_calling_the_group() {
    let frame = 0x0310_0000;
    let (a, b) = (0x0300_0000, 0x0301_0000);
    let mem = dummy_memory(frame);
    // A point span at the origin: lane 0 has |1 - 0| = 1, which is not below 0 + 0.5.
    let mut script = Script { mem: mem.clone(), log: vec![], origins: vec![(a, [0.0; 3]), (b, [0.0; 3])] };
    layout::dummy_span_test(&mut mem.clone(), DUMMY, a, b, frame, &mut script);
    assert_eq!(script.log.len(), 2);
    assert_eq!(mem.u32(SPAN + 0x5c), 1);
    assert_eq!(mem.u32(POST + 0x5c), 1);
}

#[test]
fn dummy_vf110_answers_present() {
    assert!(dummy::present());
}

/// The calls of `CEntity::E43VfD4` in order, with the arguments the port passes.
#[derive(Debug, PartialEq)]
enum Event {
    Flags,
    D8([u32; 5]),
    Vf114(u32),
    Dc([u32; 5]),
}

/// A scripted entity: the flags object's answer and the slots' answers.
struct D4Script {
    bits: u8,
    d8: u32,
    vf114: u32,
    dc: u32,
    log: Vec<Event>,
}

const FLAGS_OBJECT: u32 = 0x0400_0000;

impl D4Calls for D4Script {
    fn flags(&mut self) -> (u32, u8) {
        self.log.push(Event::Flags);
        (FLAGS_OBJECT, self.bits)
    }

    fn vf_d8(&mut self, args: [u32; 5]) -> u32 {
        self.log.push(Event::D8(args));
        self.d8
    }

    fn vf114(&mut self, e: u32) -> u32 {
        self.log.push(Event::Vf114(e));
        self.vf114
    }

    fn vf_dc(&mut self, args: [u32; 5]) -> u32 {
        self.log.push(Event::Dc(args));
        self.dc
    }
}

fn script(bits: u8, d8: u32, vf114: u32, dc: u32) -> D4Script {
    D4Script { bits, d8, vf114, dc, log: vec![] }
}

const ARGS: [u32; 5] = [0x0100_0004, 0x0100_000c, 0x0500_0000, 0x0600_0000, 0x0700_0000];

#[test]
fn d4_calls_vf_d8_then_vf114_and_vf_dc_when_both_flags_are_set() {
    let mut s = script(3, 0, 1, 7);
    assert_eq!(slots::vf_d4(&mut s, ARGS), 7);
    assert_eq!(s.log, vec![Event::Flags, Event::D8(ARGS), Event::Flags, Event::Vf114(ARGS[2]), Event::Dc(ARGS),]);
}

#[test]
fn d4_stops_when_vf_d8_answers_nonzero() {
    let mut s = script(3, 5, 1, 7);
    assert_eq!(slots::vf_d4(&mut s, ARGS), 5);
    assert_eq!(s.log, vec![Event::Flags, Event::D8(ARGS)]);
}

#[test]
fn d4_skips_vf_dc_when_vf114_answers_zero() {
    let mut s = script(2, 0, 0, 7);
    assert_eq!(slots::vf_d4(&mut s, ARGS), 0);
    assert_eq!(s.log, vec![Event::Flags, Event::Flags, Event::Vf114(ARGS[2])]);
}

#[test]
fn d4_with_no_flags_only_reads_the_flags_object() {
    let mut s = script(0, 5, 5, 5);
    assert_eq!(slots::vf_d4(&mut s, ARGS), FLAGS_OBJECT);
    assert_eq!(s.log, vec![Event::Flags, Event::Flags]);
}

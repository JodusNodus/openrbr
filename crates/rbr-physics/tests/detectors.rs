//! The detector group (`physics_core::detectors` and its `layout` sequences) against a scripted host: the frames and
//! the entry stack pointers the asm gives (0x5242c0, 0x525a20, 0x525bd0, 0x526ad0, 0x526cc0, 0x526d20), and the pure
//! parts. The game's own runs are checked by `harness/rust_step.py` (rbr-hybrid's bound.csv).

use rbr_physics::math::vec::Vec3;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::detectors::{fraction_overshoots, probe_box, step_fraction};
use rbr_physics::physics_core::layout::detector_hosts::{CollideHost, DetectorHost};
use rbr_physics::physics_core::layout::{self, KernelArgs};

/// Logs every call as (name, entry sp, arguments); answers the slot 0x118 with `grid`, the gates and the overlap with
/// 1, and the solver's fraction with `fraction`.
struct Script {
    log: Vec<(&'static str, u32, Vec<u32>)>,
    grid: u32,
    fraction: f64,
    d0_answer: u32,
}

impl Script {
    fn new(grid: u32) -> Self {
        Script { log: Vec::new(), grid, fraction: 0.5, d0_answer: 0 }
    }

    fn call(&mut self, name: &'static str, sp: u32, args: &[u32]) {
        self.log.push((name, sp, args.to_vec()));
    }

    fn names(&self) -> Vec<&'static str> {
        self.log.iter().map(|c| c.0).collect()
    }
}

impl DetectorHost for Script {
    fn collision_grid(&mut self, sp: u32, entity: u32) -> u32 {
        self.call("118", sp, &[entity]);
        self.grid
    }
    fn entity_110(&mut self, sp: u32, entity: u32) -> u32 {
        self.call("110", sp, &[entity]);
        1
    }
    fn entity_10c(&mut self, sp: u32, entity: u32) -> u32 {
        self.call("10c", sp, &[entity]);
        1
    }
    fn entity_30(&mut self, sp: u32, entity: u32) -> u32 {
        self.call("30", sp, &[entity]);
        0x30
    }
    fn entity_34(&mut self, sp: u32, entity: u32) -> u32 {
        self.call("34", sp, &[entity]);
        0x34
    }
    fn entity_box(&mut self, sp: u32, entity: u32, lo: u32, hi: u32) {
        self.call("120", sp, &[entity, lo, hi]);
    }
    fn overlaps_box(&mut self, sp: u32, entity: u32, lo: u32, hi: u32) -> u32 {
        self.call("cc", sp, &[entity, lo, hi]);
        1
    }
    fn entity_d0(&mut self, sp: u32, entity: u32, pair: u32, other: u32, extra: u32) -> u32 {
        self.call("d0", sp, &[entity, pair, other, extra]);
        self.d0_answer
    }
    fn entity_d4(&mut self, sp: u32, entity: u32, args: [u32; 7]) {
        let mut all = vec![entity];
        all.extend_from_slice(&args);
        self.call("d4", sp, &all);
    }
    fn child_gate(&mut self, sp: u32, entity: u32) -> u32 {
        self.call("80", sp, &[entity]);
        0
    }
    fn entity_30_with(&mut self, sp: u32, entity: u32, arg: u32) -> u32 {
        self.call("30w", sp, &[entity, arg]);
        0x31
    }
    fn exclusion_test(&mut self, sp: u32, entity: u32, other: u32) -> u32 {
        self.call("excl", sp, &[entity, other]);
        1
    }
    fn grid_ops(&mut self, sp: u32, at: u32, grid: u32, frame: u32, other_frame: u32) {
        self.call("ops", sp, &[at, grid, frame, other_frame]);
    }
    fn min_max(&mut self, sp: u32, at: u32, other: u32, lo: u32, hi: u32) {
        self.call("minmax", sp, &[at, other, lo, hi]);
    }
    fn ray_clear(&mut self, sp: u32, at: u32) {
        self.call("clear", sp, &[at]);
    }
    fn cells(&mut self, sp: u32, lo: u32, hi: u32, out: u32) {
        self.call("cells", sp, &[lo, hi, out]);
    }
    fn penetration_ctor(&mut self, sp: u32, at: u32, owner: u32, lo: u32, hi: u32, scale: u32, word: u32) {
        self.call("ctor", sp, &[at, owner, lo, hi, scale, word]);
    }
    fn notify_filtered(&mut self, sp: u32, cell: u32, detector: u32) {
        self.call("notify", sp, &[cell, detector]);
    }
    fn detect(&mut self, sp: u32, detector: u32, node: u32) {
        self.call("detect", sp, &[detector, node]);
    }
    fn walk(&mut self, sp: u32, entity: u32, detector: u32) {
        self.call("walk", sp, &[entity, detector]);
    }
    fn visit_list470(&mut self, sp: u32, physics: u32, detector: u32) {
        self.call("visit", sp, &[physics, detector]);
    }
}

impl CollideHost for Script {
    fn chan_enter(&mut self, sp: u32, chan: u32, sample: u32, name: u32) {
        self.call("enter", sp, &[chan, sample, name]);
    }
    fn prof_exit(&mut self, sp: u32, chan: u32) {
        self.call("exit", sp, &[chan]);
    }
    fn run_collision_detectors(&mut self, sp: u32, physics: u32, word: u32) {
        self.call("6a70", sp, &[physics, word]);
    }
    fn telemetry_70(&mut self, sp: u32, receiver: u32, events: u32) {
        self.call("t70", sp, &[receiver, events]);
    }
    fn telemetry_90(&mut self, sp: u32, receiver: u32) {
        self.call("t90", sp, &[receiver]);
    }
    fn solver_sweep_events(&mut self, sp: u32, solver: u32) {
        self.call("sweep", sp, &[solver]);
    }
    fn solver_fraction(&mut self, sp: u32, solver: u32) -> f64 {
        self.call("fraction", sp, &[solver]);
        self.fraction
    }
    fn solver_hit_count(&mut self, sp: u32, solver: u32) -> u32 {
        self.call("hits", sp, &[solver]);
        3
    }
    fn solver_pass(&mut self, sp: u32, solver: u32) {
        self.call("pass", sp, &[solver]);
    }
    fn solver_post(&mut self, sp: u32, target: u32, solver: u32) {
        self.call("post", sp, &[target, solver]);
    }
    fn advance_contact_bodies(&mut self, sp: u32, physics: u32, fraction: f32) {
        self.call("5640", sp, &[physics, fraction.to_bits()]);
    }
    fn advance_contact_bodies_remainder(&mut self, sp: u32, physics: u32, window: f32) {
        self.call("5760", sp, &[physics, window.to_bits()]);
    }
    fn entity_48(&mut self, sp: u32, target: u32, node: u32) {
        self.call("48", sp, &[target, node]);
    }
    fn entity_54(&mut self, sp: u32, target: u32, node: u32) {
        self.call("54", sp, &[target, node]);
    }
}

/// A zeroed block of `len` bytes at `at`.
fn block(mem: &mut Mem, at: u32, len: u32) {
    mem.map(at, &vec![0; len as usize]);
}

/// The physics object at `PHYSICS` with its state, scratch and def, one body (vtable `BODY_VTABLE`) on the state's
/// +0x450 list, and the stack below `ENTRY`.
const PHYSICS: u32 = 0x0100_0000;
const STATE: u32 = 0x0110_0000;
const SCRATCH: u32 = 0x0120_0000;
const DEF: u32 = 0x0130_0000;
const BODY: u32 = 0x0140_0000;
const BODY_VTABLE: u32 = 0x0141_0000;
const SOLVER_VTABLE: u32 = 0x0142_0000;
const ENTRY: u32 = 0x0200_0ffc;

fn world() -> Mem {
    let mut mem = Mem::new();
    block(&mut mem, ENTRY - 0x2000, 0x2010);
    block(&mut mem, PHYSICS, 0x100);
    block(&mut mem, STATE, 0x500);
    block(&mut mem, SCRATCH, 0x40);
    block(&mut mem, DEF, 0x16200);
    block(&mut mem, BODY, 0x100);
    block(&mut mem, BODY_VTABLE, 0x200);
    block(&mut mem, SOLVER_VTABLE, 0x10);
    block(&mut mem, 0x0165_9b70, 4);
    block(&mut mem, 0x0089_3634, 4);
    block(&mut mem, 0x007c_bb00, 4);
    mem.set_u32(0x0165_9b70, PHYSICS);
    mem.set_u32(0x0089_3634, 0x0089_0000);
    mem.set_u32(0x007c_bb00, 0x007c_0000);
    mem.set_u32(PHYSICS + 0x20, STATE);
    mem.set_u32(PHYSICS + 0x24, DEF);
    mem.set_u32(PHYSICS + 0x28, SCRATCH);
    mem.set_u32(SCRATCH + 0x10, SOLVER_VTABLE);
    mem.set_u32(SCRATCH + 0x14, 0x0150_0000);
    mem.set_u32(SOLVER_VTABLE + 8, 0x004f_c0b0);
    mem.set_u32(STATE + 0x450, BODY);
    mem.set_u32(BODY, BODY_VTABLE);
    mem.set_u32(BODY_VTABLE + 0x48, 0x0052_9dc0);
    mem.set_u32(BODY_VTABLE + 0x54, 0x0052_9e00);
    mem
}

#[test]
fn step_fraction_clamps_and_keeps_nan() {
    assert_eq!(step_fraction(0.5, 0.0, 2.0), 0.5);
    assert_eq!(step_fraction(-0.25, 0.0, 2.0).to_bits(), 0);
    assert_eq!(step_fraction(1.05, 0.0, 2.0), 1.0);
    assert!(step_fraction(f32::NAN, 0.0, 2.0).is_nan());
    assert!(fraction_overshoots(f64::NAN));
    assert!(!fraction_overshoots(f64::from(1.1f32)));
}

#[test]
fn kernel_box_is_the_difference_and_the_sum() {
    let a = Vec3 { x: 1.0, y: 2.0, z: 3.0 };
    let b = Vec3 { x: 0.5, y: 0.25, z: 4.0 };
    let (lo, hi) = probe_box(a, b);
    assert_eq!((lo.x, lo.y, lo.z), (0.5, 1.75, -1.0));
    assert_eq!((hi.x, hi.y, hi.z), (1.5, 2.25, 7.0));
}

#[test]
fn pen_check_runs_the_l1_detector_on_each_body() {
    let mut mem = world();
    let mut host = Script::new(0);
    layout::penetration_check(&mut mem, &mut host, PHYSICS, ENTRY);
    let det = ENTRY - 0x10;
    assert_eq!(mem.u32(det), 0x0073_f5b4);
    assert_eq!(mem.f32(det + 4), 1.0);
    assert_eq!(mem.u32(det + 8), SCRATCH + 0x10);
    assert_eq!(mem.u32(det + 0xc), STATE);
    assert_eq!(host.log, vec![("detect", ENTRY - 0x1c, vec![det, BODY]), ("walk", ENTRY - 0x1c, vec![BODY, det]),]);
}

#[test]
fn collide_work_steps_once_and_counts() {
    let mut mem = world();
    mem.set_u32(DEF + 0x160e0, 0x20000);
    mem.set_u32(PHYSICS + 0xc, 1);
    let mut host = Script::new(0);
    let passes = layout::collide_window(&mut mem, &mut host, PHYSICS, 2.0, ENTRY);
    assert_eq!(passes, 1);
    assert_eq!([4, 8, 0xc, 0x14, 0x18].map(|k| mem.u32(PHYSICS + k)), [3, 3, 3, 1, 1]);
    assert_eq!(
        host.names(),
        [
            "enter", "6a70", "exit", "t70", "sweep", "fraction", "hits", "hits", "5640", "pass", "48", "5760", "t90",
            "post", "54", "48"
        ]
    );
    let sp = |name| host.log.iter().find(|c| c.0 == name).unwrap().1;
    assert_eq!(sp("enter"), ENTRY - 0x40);
    assert_eq!(sp("6a70"), ENTRY - 0x38);
    assert_eq!(sp("exit"), ENTRY - 0x34);
    assert_eq!(sp("54"), ENTRY - 0x30);
    assert_eq!(host.log[8].2, vec![PHYSICS, 0.5f32.to_bits()]);
    assert_eq!(host.log[11].2, vec![PHYSICS, 2.0f32.to_bits()]);
}

#[test]
fn collide_work_ends_the_window_above_the_overshoot() {
    let mut mem = world();
    let mut host = Script::new(0);
    host.fraction = 1.5;
    layout::collide_window(&mut mem, &mut host, PHYSICS, 2.0, ENTRY);
    assert_eq!(host.names(), ["enter", "6a70", "exit", "t70", "sweep", "fraction", "t90", "post", "54", "48"]);
}

/// The base of a detector frame for `ENTRY` and `locals`: aligned below the saved ebp, the SEH record, the locals and
/// three saved registers.
fn frame(locals: u32) -> u32 {
    ((ENTRY - 4) & !0xf) - 0xc - locals - 0xc
}

#[test]
fn kernel_collects_the_entities_that_pass() {
    let mut mem = world();
    let (a, b, out, cell, node, grid) = (0x0300_0000, 0x0300_0010, 0x0310_0000, 0x0320_0000, 0x0320_0100, 0x0330_0000);
    block(&mut mem, a, 0x20);
    block(&mut mem, out, 0x4000);
    block(&mut mem, cell, 0x200);
    block(&mut mem, grid, 0x700);
    mem.set_u32(cell, node);
    mem.set_u32(node + 0xc, BODY);
    mem.set_u8(grid + 0x68d, 1);
    let f = frame(0x58);
    mem.set_u32(f + 0x50, cell);
    let mut host = Script::new(grid);
    let args = KernelArgs { a, b, mask: 8, excluded: 0, out };
    assert!(layout::kernel_collect(&mut mem, &mut host, args, ENTRY));
    assert_eq!((mem.u32(out), mem.u32(out + 4)), (1, BODY));
    assert_eq!(mem.u32(f + 0x28), f + 0x30);
    assert_eq!(host.names(), ["cells", "118", "118", "cc", "30", "118", "ops"]);
    assert_eq!(host.log[3], ("cc", f - 0xc, vec![BODY, f + 0x28, f + 0x20]));
    assert_eq!(host.log[6], ("ops", f - 0x10, vec![out + 0x30, grid, 0x30, 0x007c_0000]));
}

#[test]
fn giant_detector_combines_the_box_and_responds() {
    let mut mem = world();
    let (det, other) = (0x0300_0000, 0x0310_0000);
    block(&mut mem, det, 0x40);
    mem.set_u32(det + 4, BODY);
    mem.set_u32(det + 0x3c, 7);
    let mut host = Script::new(0);
    layout::giant_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    // No grid on the owner: the second gate ends it.
    assert_eq!(host.names(), ["118"]);
    let grid = 0x0330_0000;
    block(&mut mem, grid, 0x700);
    let mut host = Script::new(grid);
    layout::giant_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    let b = frame(0xda8);
    assert_eq!(host.log.last().unwrap(), &("clear", b - 4, vec![b + 0x6f0]));
    let d0 = host.log.iter().find(|c| c.0 == "d0").unwrap();
    assert_eq!(d0, &("d0", b - 0x10, vec![BODY, b + 0x18, other, 7]));
    assert_eq!((mem.u32(b + 0x10), mem.u32(b + 0x14)), (b + 0x20, other));
    assert_eq!((mem.u32(b + 0x18), mem.u32(b + 0x1c)), (b + 0x40, other));
}

#[test]
fn l1_penetration_detector_builds_a_giant_detector_per_entity() {
    let mut mem = world();
    let (det, other, listed) = (0x0300_0000, 0x0310_0000, 0x0320_0000);
    block(&mut mem, det, 0x10);
    block(&mut mem, listed, 0x20);
    mem.set_u32(det + 4, 1.0f32.to_bits());
    mem.set_u32(det + 8, 9);
    mem.set_u32(det + 0xc, STATE);
    mem.set_u32(STATE + 0x470, listed);
    let b = frame(0x788);
    mem.set_u32(b + 0x80, 0x0330_0000);
    let mut host = Script::new(0);
    layout::l1_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    assert_eq!(host.names(), ["118", "10c", "120", "ctor", "cells", "notify", "detect", "walk", "clear"]);
    assert_eq!(host.log[3], ("ctor", b - 0x18, vec![b + 0x90, other, b + 0x30, b + 0x20, 1.0f32.to_bits(), 9]));
    assert_eq!(host.log[6], ("detect", b - 8, vec![b + 0x40, listed]));
    assert_eq!(host.log[7], ("walk", b - 8, vec![other, b + 0x40]));
    let giant = [0, 4, 0x30, 0x34, 0x38, 0x3c].map(|k| mem.u32(b + 0x40 + k));
    assert_eq!(giant, [0x0073_f5d0, other, 0, 0, 1.0f32.to_bits(), 9]);
    assert_eq!(mem.u32(b + 0x90), 0x0073_f5c0);
}

#[test]
fn l1_collision_detector_fills_an_l2_detector() {
    let mut mem = world();
    let (det, other) = (0x0300_0000, 0x0310_0000);
    block(&mut mem, det, 0x10);
    mem.set_u32(det + 4, 2.0f32.to_bits());
    mem.set_u32(det + 8, 5);
    let mut host = Script::new(0);
    layout::l1_collision_detect(&mut mem, &mut host, det, other, ENTRY);
    let b = frame(0xde8);
    let l2 = b + 0x18;
    let words = [0, 4, 8, 0xc, 0x10, 0x14, 0x18, 0x1c, 0x20, 0x24].map(|k| mem.u32(l2 + k));
    assert_eq!(
        words,
        [0x0073_f5a8, other, b + 0x40, 0x007c_0000, b + 0x50, 0x007c_0000, b + 0x730, b + 0x80, 2.0f32.to_bits(), 5]
    );
    assert_eq!(host.log.iter().find(|c| c.0 == "visit").unwrap(), &("visit", b - 8, vec![PHYSICS, l2]));
}

#[test]
fn kernel_without_an_output_makes_no_call_and_stops_at_eight() {
    let mut mem = world();
    let mut host = Script::new(0);
    let args = KernelArgs { a: 0, b: 0, mask: 0, excluded: 0, out: 0 };
    assert!(!layout::kernel_collect(&mut mem, &mut host, args, ENTRY));
    assert!(host.log.is_empty());

    // Nine entities in one cell: the count stops at eight.
    let (a, out, cell, nodes, grid) = (0x0300_0000, 0x0310_0000, 0x0320_0000, 0x0320_0100, 0x0330_0000);
    block(&mut mem, a, 0x20);
    block(&mut mem, out, 0x4000);
    block(&mut mem, cell, 0x400);
    block(&mut mem, grid, 0x700);
    mem.set_u8(grid + 0x68d, 1);
    mem.set_u32(cell, nodes);
    for k in 0..9 {
        let node = nodes + 0x20 * k;
        mem.set_u32(node, if k < 8 { node + 0x20 } else { 0 });
        mem.set_u32(node + 0xc, BODY);
    }
    mem.set_u32(frame(0x58) + 0x50, cell);
    let mut host = Script::new(grid);
    let args = KernelArgs { a, b: a + 0x10, mask: 0, excluded: 0, out };
    assert!(layout::kernel_collect(&mut mem, &mut host, args, ENTRY));
    assert_eq!(mem.u32(out), 8);
    assert_eq!(host.names().iter().filter(|&&n| n == "ops").count(), 8);
}

#[test]
fn detectors_stop_at_a_disabled_grid() {
    let mut mem = world();
    let (det, other, grid) = (0x0300_0000, 0x0310_0000, 0x0330_0000);
    block(&mut mem, det, 0x40);
    block(&mut mem, grid, 0x700);
    mem.set_u32(grid + 0x67c, 1);
    mem.set_u32(det + 4, BODY);
    let mut host = Script::new(grid);
    layout::l1_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    layout::l1_collision_detect(&mut mem, &mut host, det, other, ENTRY);
    assert_eq!(host.names(), ["118", "118"]);
    let mut host = Script::new(grid);
    layout::giant_penetration_detect(&mut mem, &mut host, det, BODY, ENTRY);
    assert!(host.log.is_empty());
    layout::giant_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    assert_eq!(host.names(), ["118", "118"]);
}

#[test]
fn visit_470_skips_flagged_entities() {
    let mut mem = world();
    let second = BODY + 0x80;
    mem.set_u32(STATE + 0x470, BODY);
    mem.set_u32(BODY + 0x10, second);
    mem.set_u8(BODY + 0x60, 0x40);
    let mut host = Script::new(0);
    layout::visit_list470(&mut mem, &mut host, PHYSICS, 0x0300_0000, ENTRY);
    assert_eq!(
        host.log,
        vec![("detect", ENTRY - 0x10, vec![0x0300_0000, second]), ("walk", ENTRY - 0x10, vec![second, 0x0300_0000]),]
    );
}

#[test]
fn work_6a70_runs_the_l1_collision_detector_on_each_body() {
    let mut mem = world();
    let mut host = Script::new(0);
    layout::run_collision_detectors(&mut mem, &mut host, PHYSICS, 0x3f80_0000, ENTRY);
    let det = ENTRY - 0xc;
    assert_eq!([0, 4, 8].map(|k| mem.u32(det + k)), [0x0073_f7b0, 0x3f80_0000, SCRATCH + 0x10]);
    assert_eq!(host.log, vec![("detect", ENTRY - 0x18, vec![det, BODY]), ("walk", ENTRY - 0x18, vec![BODY, det]),]);
}

/// `CL2PenetrationDetector::Vf04`: the gates, the box from slot 0x120 (lo at +0x40, hi at +0x30 of the frame), the two
/// tests (the owner's corner less `other`'s high corner, then the low corner less the owner's second; both lanes
/// below zero), the owner's response, and the other's when the owner's answers zero.
#[test]
fn l2_penetration_responds_on_the_owner_then_on_the_other() {
    let mut mem = world();
    let (det, other) = (0x0300_0000, 0x0310_0000);
    block(&mut mem, det, 0x800);
    block(&mut mem, other, 0x100);
    mem.set_u32(det + 4, BODY);
    mem.set_u32(det + 0x6e4, 9);
    for i in 0..3 {
        mem.set_f32(det + 0x10 + 4 * i, -5.0);
        mem.set_f32(det + 0x20 + 4 * i, 5.0);
    }
    let b = frame(0xdb8);
    for i in 0..3 {
        mem.set_f32(b + 0x40 + 4 * i, -1.0);
    }
    let mut host = Script::new(0);
    layout::l2_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    assert_eq!(
        host.names(),
        ["118", "excl", "110", "120", "30", "30", "118", "ops", "d0", "30", "30", "118", "ops", "d0", "clear", "clear",]
    );
    let at = |i: usize| (host.log[i].1, host.log[i].2.clone());
    assert_eq!(at(3), (b - 0xc, vec![other, b + 0x40, b + 0x30]));
    assert_eq!(at(7), (b - 0x10, vec![b + 0x700, 0, 0x30, 0x30]));
    assert_eq!(at(8), (b - 0x10, vec![BODY, b + 0x18, other, 9]));
    assert_eq!(at(12), (b - 0x10, vec![b + 0x50, 0, 0x30, 0x30]));
    assert_eq!(at(13), (b - 0x10, vec![other, b + 0x20, BODY, 9]));
    assert_eq!(at(14), (b - 4, vec![b + 0x50]));
    assert_eq!(at(15), (b - 4, vec![b + 0x700]));
    assert_eq!((mem.u32(b + 0x18), mem.u32(b + 0x1c)), (b + 0x700, other));
    assert_eq!((mem.u32(b + 0x20), mem.u32(b + 0x24)), (b + 0x50, BODY));
    // The owner's response answers nonzero: no response from the other, the owner's ops are cleared.
    let mut host = Script::new(0);
    host.d0_answer = 1;
    layout::l2_penetration_detect(&mut mem, &mut host, det, other, ENTRY);
    assert_eq!(host.names().len(), 10);
    assert_eq!(host.log[9], ("clear", b - 4, vec![b + 0x700]));
}

/// `E46L2Detector::Vf04` with both sides: `other`'s slot 0x118 answers, so the mirrored grid ops and `E45PhysBody::M4250`
/// (its slot 0x30 and 0x34 of the second entity, then the first's slot 0xd4 with the two pairs and the words) run too.
#[test]
fn e46_l2_runs_both_sides_in_the_original_order() {
    let mut mem = world();
    let (det, other, grid) = (0x0300_0000, 0x0310_0000, 0x0330_0000);
    block(&mut mem, det, 0x100);
    block(&mut mem, other, 0x100);
    block(&mut mem, grid, 0x700);
    mem.set_u32(det + 4, BODY);
    mem.set_u32(det + 0x18, grid);
    mem.set_u32(det + 0x1c, grid);
    mem.set_u32(det + 0x20, 0x77);
    mem.set_u32(det + 0x24, 0x78);
    let mut host = Script::new(grid);
    layout::e46_l2_detect(&mut mem, &mut host, det, other, ENTRY);
    let b = frame(0x1ad8);
    let identity = 0x007c_0000;
    let want: Vec<(&str, u32)> = vec![
        ("110", b - 4),
        ("excl", b - 8),
        ("118", b - 4),
        ("cc", b - 0xc),
        ("34", b - 4),
        ("ops", b - 0x10),
        ("30", b - 4),
        ("ops", b - 0x10),
        ("30", b - 0x38),
        ("34", b - 0x38),
        ("d4", b - 0x54),
        ("118", b - 4),
        ("34", b - 4),
        ("34", b - 8),
        ("118", b - 0xc),
        ("ops", b - 0x10),
        ("30", b - 4),
        ("30w", b - 8),
        ("118", b - 0xc),
        ("ops", b - 0x10),
        ("30", b - 0x38),
        ("34", b - 0x38),
        ("d4", b - 0x54),
        ("clear", b - 4),
        ("clear", b - 4),
        ("clear", b - 4),
        ("clear", b - 4),
    ];
    assert_eq!(host.log.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>(), want);
    let args = |i: usize| host.log[i].2.clone();
    assert_eq!(args(3), vec![other, det + 8, det + 0x10]);
    assert_eq!(args(5), vec![b + 0x1420, grid, identity, 0x34]);
    assert_eq!(args(7), vec![b + 0x6c0, grid, identity, 0x30]);
    assert_eq!(args(10), vec![BODY, b + 0x1420, 0x34, b + 0x6c0, 0x30, other, 0x77, 0x78]);
    assert_eq!(args(15), vec![b + 0x10, grid, 0x34, 0x34]);
    assert_eq!(args(17), vec![other, 0x30]);
    assert_eq!(args(19), vec![b + 0xd70, grid, 0x31, 0x30]);
    assert_eq!(args(22), vec![other, b + 0x10, 0x34, b + 0xd70, 0x30, BODY, 0x77, 0x78]);
    assert_eq!(args(23), vec![b + 0xd70]);
    assert_eq!(args(26), vec![b + 0x1420]);
}

/// `CDummyObject::Vf11C`: the centre (+0x94) and half extents (+0xa0) make the box; the original stores y and z swapped.
#[test]
fn dummy_box_swaps_y_and_z() {
    let mut mem = Mem::new();
    let (dummy, lo, hi) = (0x0340_0000, 0x0350_0000, 0x0351_0000);
    block(&mut mem, dummy, 0x100);
    block(&mut mem, lo, 0x10);
    block(&mut mem, hi, 0x10);
    for (i, v) in [1.0f32, 2.0, 3.0].into_iter().enumerate() {
        mem.set_f32(dummy + 0x94 + 4 * i as u32, v);
    }
    for (i, v) in [0.5f32, 0.25, 0.125].into_iter().enumerate() {
        mem.set_f32(dummy + 0xa0 + 4 * i as u32, v);
    }
    rbr_physics::entities::layout::dummy_box(&mut mem, dummy, lo, hi);
    let read = |at: u32| [mem.f32(at), mem.f32(at + 4), mem.f32(at + 8)];
    assert_eq!(read(lo), [0.5, 2.875, 1.75]);
    assert_eq!(read(hi), [1.5, 3.125, 2.25]);
}

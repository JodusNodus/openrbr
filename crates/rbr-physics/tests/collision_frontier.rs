//! The collision sequences the detectors reach (`E37GridOps::M6610` 0x4f6610, `E34WallVf::VfCC` 0x4e42f0, the solver's
//! fraction 0x4fc500, `E33Vec4::MinMaxA` 0x4e0ae0) against a scripted host and hand-built memory, every branch. The
//! game's own runs are checked by `harness/rust_step.py` (rbr-hybrid's bound.csv).

mod common;

use rbr_physics::collision::brake_wall::{box_too_wide, corners_box};
use rbr_physics::collision::cells::{half_extent, point_bounds};
use rbr_physics::collision::contact_solver::earlier_fraction;
use rbr_physics::collision::layout;
use rbr_physics::collision::layout::grid_hosts::GridOpsHost;
use rbr_physics::collision::layout::wall_tree::WallVfCcHost;
use rbr_physics::math::vec::Vec4;
use rbr_physics::mem::Mem;

const ENTRY: u32 = 0x0200_0ffc;
const GRID: u32 = 0x0100_0000;
const SRC: u32 = 0x0110_0000;

fn block(mem: &mut Mem, at: u32, len: u32) {
    mem.map(at, &vec![0; len as usize]);
}

fn v(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4 { x, y, z, w }
}

/// The calls a scripted host saw: (name, entry sp, arguments).
type Log = Vec<(&'static str, u32, Vec<u32>)>;

/// Logs the calls; the copies leave the points as they are.
#[derive(Default)]
struct Ops(Log);

impl GridOpsHost for Ops {
    fn ray_seeds(&mut self, sp: u32, grid: u32) {
        self.0.push(("seeds", sp, vec![grid]));
    }
    fn copy_blocks_one_two(&mut self, sp: u32, a: u32, src: u32, o2: u32, dst: u32, n: u32) {
        self.0.push(("b460", sp, vec![a, src, o2, dst, n]));
    }
    fn copy_blocks_three_four(&mut self, sp: u32, a: u32, src: u32, o2: u32, dst: u32, n: u32) {
        self.0.push(("b500", sp, vec![a, src, o2, dst, n]));
    }
}

fn grids(points: &[Vec4]) -> Mem {
    let mut mem = Mem::new();
    block(&mut mem, GRID, 0x6b0);
    block(&mut mem, SRC, 0x6b0);
    mem.set_u8(SRC + 0x68d, points.len() as u8);
    mem.set_f32(SRC + 0x3c, 6.0);
    for (k, p) in points.iter().enumerate() {
        p.store(&mut mem, SRC + 0x40 + 16 * k as u32);
    }
    mem
}

#[test]
fn grid_ops_copies_the_source_and_bounds_its_points() {
    let points = [v(1.0, -2.0, 3.0, 0.0), v(-1.0, 4.0, 5.0, 2.0), v(0.0, 0.0, -3.0, 1.0)];
    let mut mem = grids(&points);
    let mut host = Ops::default();
    let eax = layout::grid_ops_m6610(&mut host, &mut mem, GRID, SRC, 0xa, 0xb, ENTRY);
    assert_eq!(eax, Some(GRID));
    assert_eq!(
        host.0,
        vec![
            ("b460", ENTRY - 0x28, vec![0xa, GRID + 0x40, 0xb, GRID + 0x40, 3]),
            ("b500", ENTRY - 0x28, vec![0xa, GRID + 0x220, 0xb, GRID + 0x220, 3]),
        ]
    );
    assert_eq!(Vec4::load(&mem, GRID), v(-1.0, -2.0, -3.0, 0.0));
    assert_eq!(Vec4::load(&mem, GRID + 0x10), v(1.0, 4.0, 5.0, 2.0));
    assert_eq!(Vec4::load(&mem, GRID + 0x20), v(0.0, 1.0, 1.0, 1.0));
    assert_eq!(Vec4::load(&mem, GRID + 0x30), v(1.0, 3.0, 4.0, 3.0));
    assert_eq!(mem.u8(GRID + 0x68d), 3);
}

#[test]
fn grid_ops_without_points_only_copies() {
    let mut mem = grids(&[]);
    let mut host = Ops::default();
    assert_eq!(layout::grid_ops_m6610(&mut host, &mut mem, GRID, SRC, 0xa, 0xb, ENTRY), Some(GRID));
    assert!(host.0.is_empty());
    assert_eq!(mem.f32(GRID + 0x3c), 6.0);
}

#[test]
fn grid_ops_without_a_source_seeds_the_rays() {
    let mut mem = grids(&[]);
    let mut host = Ops::default();
    assert_eq!(layout::grid_ops_m6610(&mut host, &mut mem, GRID, 0, 0xa, 0xb, ENTRY), Some(GRID));
    assert_eq!(host.0, vec![("seeds", ENTRY - 0x14, vec![GRID])]);
    assert_eq!(layout::grid_ops_m6610(&mut host, &mut mem, 0, 0, 0xa, 0xb, ENTRY), Some(0));
    assert_eq!(host.0.len(), 1);
}

#[test]
fn grid_ops_declines_one_point_before_any_change() {
    let mut mem = grids(&[v(1.0, 1.0, 1.0, 1.0)]);
    let mut host = Ops::default();
    assert_eq!(layout::grid_ops_m6610(&mut host, &mut mem, GRID, SRC, 0xa, 0xb, ENTRY), None);
    assert!(host.0.is_empty());
    assert_eq!(mem.u8(GRID + 0x68d), 0);
}

#[test]
fn bounds_and_half_extent_follow_minps_maxps() {
    let nan = f32::NAN;
    let (lo, hi) = point_bounds(&[v(nan, 1.0, 0.0, 0.0), v(2.0, nan, -0.0, 0.0)]);
    // An unordered lane takes the later point's value, as minps and maxps take their second operand.
    assert_eq!((lo.x, hi.x), (2.0, 2.0));
    assert!(lo.y.is_nan() && hi.y.is_nan());
    assert_eq!(lo.z.to_bits(), (-0.0f32).to_bits());
    let half = half_extent(v(-1.0, 0.0, 2.0, 9.0), v(3.0, 1.0, 2.0, -9.0), 5.0);
    assert_eq!(half, v(2.0, 0.5, 0.0, 2.5));
}

/// The wall's calls: its frame is `0xf`, each fetch writes the next corner's x and y to `out`, the query answers
/// `hits`. `mem` shares the sequence's memory.
struct Wall {
    mem: Mem,
    corners: Vec<[f32; 2]>,
    hits: u32,
    log: Log,
}

impl WallVfCcHost for Wall {
    fn frame(&mut self, sp: u32, wall: u32) -> u32 {
        self.log.push(("30", sp, vec![wall]));
        0xf
    }
    fn fetch_x(&mut self, sp: u32, corner: u32, out: u32, frame: u32) {
        self.log.push(("fetch", sp, vec![corner, out, frame]));
        let [x, y] = self.corners.remove(0);
        self.mem.set_f32(out, x);
        self.mem.set_f32(out + 4, y);
    }
    fn query_box(&mut self, sp: u32, wall: u32, bounds: u32, collector: u32) -> u32 {
        self.log.push(("query", sp, vec![wall, bounds, collector]));
        self.hits
    }
}

fn wall_vf_cc(corners: Vec<[f32; 2]>, hits: u32) -> (u32, Log, [f32; 4]) {
    let mut mem = common::shared_mem();
    block(&mut mem, ENTRY - 0x100, 0x110);
    let mut host = Wall { mem: mem.clone(), corners, hits, log: Vec::new() };
    let eax = layout::wall_vf_cc(&mut host, &mut mem, GRID, 0xa, 0xb, ENTRY);
    let frame = ((ENTRY - 4) & !0xf) - 0x30;
    let bounds = [0, 4, 8, 0xc].map(|k| mem.f32(frame + 0x10 + k));
    (eax, host.log, bounds)
}

#[test]
fn wall_vf_cc_queries_the_corners_box() {
    let frame = ((ENTRY - 4) & !0xf) - 0x30;
    let (eax, log, bounds) = wall_vf_cc(vec![[1.0, 2.0], [5.0, -2.0]], 7);
    assert_eq!(eax, 1);
    assert_eq!(bounds, [3.0, 0.0, 2.0, -2.0]);
    assert_eq!(
        log,
        vec![
            ("30", frame - 4, vec![GRID]),
            ("fetch", frame - 0xc, vec![0xa, frame + 0x20, 0xf]),
            ("30", frame - 4, vec![GRID]),
            ("fetch", frame - 0xc, vec![0xb, frame + 0x20, 0xf]),
            ("query", frame - 0xc, vec![GRID, frame + 0x10, GRID + 0x7c]),
        ]
    );
    assert_eq!(wall_vf_cc(vec![[1.0, 2.0], [5.0, -2.0]], 0).0, 0);
}

#[test]
fn wall_vf_cc_does_not_query_a_wide_box() {
    let (eax, log, _) = wall_vf_cc(vec![[0.0, 0.0], [20.5, 0.0]], 7);
    assert_eq!(eax, 0);
    assert_eq!(log.len(), 4);
    let (eax, log, _) = wall_vf_cc(vec![[0.0, 0.0], [0.0, -30.0]], 7);
    assert_eq!((eax, log.len()), (1, 5));
    assert!(!box_too_wide(f32::NAN, 10.0));
    assert!(box_too_wide(10.000001, 0.0));
    assert_eq!(corners_box([1.0, 1.0], [3.0, 5.0]), [2.0, 3.0, 1.0, 2.0]);
}

#[test]
fn earlier_fraction_keeps_the_fraction_unless_it_is_above() {
    assert_eq!(earlier_fraction(2.0, 0.5), 0.5);
    assert_eq!(earlier_fraction(0.5, 0.75), 0.5);
    assert_eq!(earlier_fraction(0.5, f32::NAN), 0.5);
    assert!(earlier_fraction(f32::NAN, 0.5).is_nan());
}

/// The solver at `SOLVER` with its event list (+4 head, +8 tail, +0xc count) and the events at `EVENTS`, 0x100 apart:
/// their link (+0xc next, +0x10 previous, +0x14 the list) and flags (+0xe4), with `CCollisionEvent`'s vtable (slot 0x1c
/// `Vf1Cx`).
const SOLVER: u32 = 0x0120_0000;
const EVENTS: u32 = 0x0130_0000;
const EVENT_VTABLE: u32 = 0x0140_0000;

fn solver(fractions: &[(f32, u32)]) -> Mem {
    let mut mem = Mem::new();
    block(&mut mem, SOLVER, 0x20);
    block(&mut mem, EVENTS, 0x100 * fractions.len() as u32 + 0x100);
    block(&mut mem, EVENT_VTABLE, 0x20);
    block(&mut mem, 0x008e_f7e0, 0x20);
    mem.set_u32(EVENT_VTABLE + 0x1c, 0x004f_52f0);
    let n = fractions.len() as u32;
    for (k, &(t, flags)) in fractions.iter().enumerate() {
        let e = EVENTS + 0x100 * k as u32;
        mem.set_u32(e, EVENT_VTABLE);
        mem.set_f32(e + 0x94, t);
        mem.set_u32(e + 0xe4, flags);
        mem.set_u32(e + 0xc, if (k as u32) + 1 < n { e + 0x100 } else { 0 });
        mem.set_u32(e + 0x10, if k > 0 { e - 0x100 } else { 0 });
        mem.set_u32(e + 0x14, SOLVER + 4);
    }
    if n > 0 {
        mem.set_u32(SOLVER + 4, EVENTS);
        mem.set_u32(SOLVER + 8, EVENTS + 0x100 * (n - 1));
        mem.set_u32(SOLVER + 0xc, n);
    }
    mem
}

#[test]
fn solver_fraction_is_the_earliest_event_from_two() {
    assert_eq!(layout::min_contact_time(&mut solver(&[]), SOLVER), Some(2.0));
    let mut mem = solver(&[(0.75, 0), (0.25, 0), (f32::NAN, 0), (3.0, 0)]);
    assert_eq!(layout::min_contact_time(&mut mem, SOLVER), Some(0.25));
    assert_eq!(mem.u32(SOLVER + 0xc), 4);
}

#[test]
fn solver_fraction_releases_flagged_events() {
    let mut mem = solver(&[(0.75, 0), (0.25, 0x2000), (0.5, 0)]);
    assert_eq!(layout::min_contact_time(&mut mem, SOLVER), Some(0.5));
    assert_eq!(mem.u32(SOLVER + 0xc), 2);
    assert_eq!(mem.u32(EVENTS + 0xc), EVENTS + 0x200);
    assert_eq!(mem.u32(EVENTS + 0x210), EVENTS);
}

#[test]
fn solver_fraction_declines_an_unknown_release() {
    let mut mem = solver(&[(0.75, 0x4000)]);
    mem.set_u32(EVENT_VTABLE + 0x1c, 0x0040_0000);
    assert_eq!(layout::min_contact_time(&mut mem, SOLVER), None);
    assert_eq!(mem.u32(SOLVER + 0xc), 1);
}

#[test]
fn min_max_a_reads_the_upper_pair_after_the_lower_store() {
    let mut mem = Mem::new();
    block(&mut mem, GRID, 0x40);
    v(1.0, 5.0, -1.0, 0.0).store(&mut mem, GRID);
    v(2.0, 2.0, 2.0, 2.0).store(&mut mem, GRID + 0x10);
    v(0.0, 6.0, 0.0, 1.0).store(&mut mem, GRID + 0x20);
    v(3.0, 1.0, 1.0, 1.0).store(&mut mem, GRID + 0x30);
    // `lo` is the receiver's upper vector, so the max sees the min's result.
    let eax = layout::min_max_a(&mut mem, GRID, GRID + 0x20, GRID + 0x10, GRID + 0x30);
    assert_eq!(eax, GRID + 0x30);
    assert_eq!(Vec4::load(&mem, GRID + 0x10), v(0.0, 5.0, -1.0, 0.0));
    // Read before the store, the max would be (3, 2, 2, 2).
    assert_eq!(Vec4::load(&mem, GRID + 0x30), v(3.0, 5.0, 1.0, 1.0));
}

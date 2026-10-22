//! The small ports of the C++-exact batch: the light setters, the brake-wall raycast entry, the segment test and the
//! profiler exit. The original's calls are checked by the all-bindings `rust_step` run (`rbr-hybrid/bound.csv`).

use rbr_physics::car_body::layout::{set_brake_light, set_neutral_light};
use rbr_physics::collision::layout::wall_tree::RaycastCalls;
use rbr_physics::collision::layout::{seg_test, wall_probe, wall_raycast};
use rbr_physics::fpu::Precision;
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::prof_calls::{EnterHost, ProfHost};
use rbr_physics::physics_core::layout::AccumHost;
use rbr_physics::physics_core::layout::{
    accumulate_forces, mat_scale_rows, prof_chan_enter, prof_enter, prof_exit, vec_add,
};

const BASE: u32 = 0x10000;

fn memory() -> Mem {
    let mut mem = Mem::new();
    mem.map(BASE, &vec![0; 0x2000]);
    mem
}

#[test]
fn light_setters_dirty_the_value_and_flag_the_change() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = memory();
    let (this, block, changes) = (BASE, BASE + 0x100, BASE + 0x400);
    mem.set_u32(this + 8, block);
    mem.set_u32(this + 0x14, changes);
    assert_eq!(set_brake_light(&mut mem, this, 0), block);
    assert_eq!(mem.u32(changes + 0xc), 0);
    assert_eq!(set_brake_light(&mut mem, this, 1), changes);
    assert_eq!((mem.u32(block + 0x58), mem.u32(block + 0x5c)), (1, 1));
    assert_eq!(mem.u32(changes + 0xc), 1);
    set_neutral_light(&mut mem, this, 7);
    assert_eq!((mem.u32(block + 0x68), mem.u32(block + 0x6c)), (1, 7));
    assert_eq!(mem.u32(changes + 0x10), 1);
}

#[derive(Default)]
struct Emits(Vec<(u32, u32)>);

impl RaycastCalls for Emits {
    fn emit_penetration(&mut self, this: u32, out: u32) {
        self.0.push((this, out));
    }
}

/// A wall whose root has one leaf at the origin (extents 10 by 10) with the index table at +0x4000 and the vertex
/// table at +0x5000. The leaf's first vertex index is `word`; the segment (-1,-2), (-1,0), (1,1), (-1,1) covers the origin.
fn wall_with_leaf(mem: &mut Mem, wall: u32, word: u16) {
    let (children, index, mesh) = (BASE + 0x3000, BASE + 0x4000, BASE + 0x5000);
    mem.set_u32(wall + 0x218, mesh);
    mem.set_u32(wall + 0x64 + 0x14, children);
    mem.set_f32(children + 8, 10.0);
    mem.set_f32(children + 12, 10.0);
    mem.set_u32(children + 0x10, 1);
    mem.set_u32(children + 0x14, index);
    mem.set_u8(index, word as u8);
    mem.set_u8(index + 1, (word >> 8) as u8);
    let vertices = [(-1.0f32, -2.0f32), (-1.0, 0.0), (1.0, 1.0), (-1.0, 1.0)];
    for (k, (x, y)) in vertices.into_iter().enumerate() {
        mem.set_f32(mesh + 8 * k as u32, x);
        mem.set_f32(mesh + 8 * k as u32 + 4, y);
    }
}

#[test]
fn raycast_entry_copies_the_point_and_runs_the_wall() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = Mem::new();
    mem.map(BASE, &vec![0; 0x8000]);
    mem.map(0x893600, &[0; 0x10]);
    let (manager, point, tmp, out) = (BASE, BASE + 0x100, BASE + 0x200, BASE + 0x300);
    mem.set_u32(point, 0);
    mem.set_u32(point + 4, 0x4000_0000);
    mem.set_u32(out + 0xc, 0x7777);
    let mut emits = Emits::default();
    assert_eq!(wall_probe(&mut emits, &mut mem, manager, point, out, tmp), 0);
    assert_eq!((mem.u32(tmp), mem.u32(tmp + 4)), (0, 0x4000_0000));
    assert!(emits.0.is_empty());

    let wall = BASE + 0x1000;
    mem.set_u32(manager + 4, wall);
    mem.set_u32(wall + 0x214, 1);
    assert_eq!(wall_probe(&mut emits, &mut mem, manager, point, out, tmp), 0);
    assert_eq!(mem.u32(wall + 0x214), 0);
    assert!(emits.0.is_empty());
}

#[test]
fn wall_raycast_answers_the_first_segment_the_point_is_inside_of() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = Mem::new();
    mem.map(BASE, &vec![0; 0x8000]);
    mem.map(0x893600, &[0; 0x10]);
    let (wall, point, out, hits) = (BASE + 0x1000, BASE + 0x100, BASE + 0x300, BASE + 0x400);
    mem.set_u32(hits + 0x190, 0);
    mem.set_f32(point, 0.0);
    mem.set_f32(point + 4, 0.0);
    mem.set_u32(out + 0xc, 0x7777);
    wall_with_leaf(&mut mem, wall, 0);

    let mut emits = Emits::default();
    mem.set_u32(wall + 0x214, 1);
    assert_eq!(wall_raycast(&mut emits, &mut mem, wall, point, out, hits), 1);
    assert_eq!(mem.u32(wall + 0x214), 0);
    let got = [0, 4, 8, 0x10, 0x14, 0x18].map(|at| mem.u32(out + at));
    assert_eq!(got, [1.0f32.to_bits(), 0, 0, 0, 1.0f32.to_bits(), 2]);
    assert_eq!(mem.u32(out + 0xc), 0x7777);
    assert!(emits.0.is_empty());

    mem.set_u32(0x893600, 0x1234_0000);
    wall_with_leaf(&mut mem, wall, 0x4000);
    assert_eq!(wall_raycast(&mut emits, &mut mem, wall, point, out, hits), 0);
    assert_eq!(mem.u32(out + 0x18), 4);
    assert_eq!(emits.0, [(0x1234_0000 + 0xa780, out)]);
}

#[test]
fn seg_test_builds_the_query_and_defaults_the_output() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = memory();
    let (this, a, b, query) = (BASE, BASE + 0x1100, BASE + 0x1200, BASE + 0x1300);
    for lane in 0..3 {
        mem.set_u32(a + 4 * lane, 10 + lane);
        mem.set_u32(b + 4 * lane, 20 + lane);
    }
    mem.set_u32(this + 0x1014, 3);
    assert_eq!(seg_test(&mut mem, this, a, b, 0, query), 0);
    assert_eq!(mem.u32(this + 0x1014), 0);
    assert_eq!(mem.read(query, 24)[..4], 10u32.to_le_bytes());
    assert_eq!(mem.u32(query + 12), 20);
    assert_eq!(mem.u32(query + 20), 22);
}

/// The host writes the counter before the port reads it; here the test has already stored it.
struct Counter;

impl ProfHost for Counter {
    fn counter(&mut self, _at: u32) -> u32 {
        0
    }
}

#[test]
fn prof_exit_stores_ticks_seconds_and_rate() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = memory();
    let (handle, sample) = (BASE, BASE + 0x100);
    mem.set_u32(handle, sample);
    mem.map(0x165b000, &[0; 0x1000]);
    mem.set_u32(0x165b208, 1000);
    mem.set_u32(sample + 0x10, 100);
    mem.set_u32(sample + 0x14, 7);
    let local = BASE + 0x300;
    mem.set_u32(local, 600);
    mem.set_u32(local + 4, 7);
    assert_eq!(prof_exit(&mut Counter, &mut mem, handle, local), 7);
    assert_eq!(mem.u32(sample + 0x18), 500);
    let seconds = f64::from_bits(mem.u32(sample + 0x20) as u64 | (mem.u32(sample + 0x24) as u64) << 32);
    assert_eq!(seconds, 0.5);
    assert_eq!(mem.u32(sample + 0x2c), 2.0f64.to_bits().wrapping_shr(32) as u32);
}

/// The profiler's host for the enter: the counter reads 1, and the unlinks are recorded.
#[derive(Default)]
struct Queue {
    unlinked: Vec<u32>,
}

impl ProfHost for Queue {
    fn counter(&mut self, _at: u32) -> u32 {
        1
    }
}

impl EnterHost for Queue {
    fn unlink(&mut self, sample: u32) {
        self.unlinked.push(sample);
    }
}

#[test]
fn prof_enter_queues_the_sample_at_the_tail() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = memory();
    mem.map(0x165f000, &[0; 0x200]);
    let (first, second, chan) = (BASE + 0x100, BASE + 0x200, BASE + 0x300);
    let mut host = Queue::default();
    assert_eq!(prof_enter(&mut host, &mut mem, first, 0x55), 1);
    assert_eq!((mem.u32(0x165f108), mem.u32(0x165f10c), mem.u32(0x165f110)), (first, first, 1));
    assert_eq!((mem.u32(first + 8), mem.u32(first + 0x30)), (0x165f108, 0x55));
    prof_enter(&mut host, &mut mem, second, 0x66);
    assert_eq!((mem.u32(0x165f108), mem.u32(0x165f10c), mem.u32(0x165f110)), (first, second, 2));
    assert_eq!((mem.u32(first), mem.u32(second + 4)), (second, first));
    assert!(host.unlinked.is_empty());
    // A linked sample leaves its queue first (the count drops and is raised again).
    assert_eq!(prof_chan_enter(&mut host, &mut mem, chan, first, 0x77, 1), 1);
    assert_eq!(host.unlinked, vec![first]);
    assert_eq!((mem.u32(chan), mem.u8(chan + 4), mem.u32(0x165f110)), (first, 1, 2));
}

#[test]
fn vec_add_and_scale_rows_work_on_each_row() {
    let _pc24 = Precision::Pc24.scope();
    let mut mem = memory();
    let (this, rows, mat) = (BASE, BASE + 0x100, BASE + 0x200);
    for (k, v) in [1.0f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
        mem.set_f32(this + 4 * k as u32, v);
    }
    for (k, v) in [10.0f32, 20.0, 30.0, 40.0, 0.5, 0.5, 0.5, 0.5].into_iter().enumerate() {
        mem.set_f32(rows + 4 * k as u32, v);
    }
    vec_add(&mut mem, this, rows, 2);
    let added: Vec<f32> = (0..8).map(|k| mem.f32(rows + 4 * k)).collect();
    assert_eq!(added, [11.0, 22.0, 33.0, 44.0, 1.5, 2.5, 3.5, 4.5]);

    // The matrix diag(1, 2, 3) with a tenth float of 9: the row (1, 2, 3, 4) becomes (1, 4, 9, 27).
    for (k, v) in [1.0f32, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 9.0].into_iter().enumerate() {
        mem.set_f32(mat + 4 * k as u32, v);
    }
    for (k, v) in [1.0f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
        mem.set_f32(rows + 4 * k as u32, v);
    }
    mat_scale_rows(&mut mem, mat, rows, 0);
    assert_eq!(mem.f32(rows + 4), 2.0);
    mat_scale_rows(&mut mem, mat, rows, 1);
    let scaled: Vec<f32> = (0..4).map(|k| mem.f32(rows + 4 * k)).collect();
    assert_eq!(scaled, [1.0, 4.0, 9.0, 27.0]);
}

/// The calls of `accumulate_forces`, in order, and the walk and point of the test's mock (none: the walk's output is preset).
#[derive(Default)]
struct Accum(Vec<&'static str>);

impl AccumHost for Accum {
    fn hook(&mut self, _body: u32) {
        self.0.push("hook");
    }

    fn base(&mut self, _body: u32) {
        self.0.push("base");
    }

    fn point(&mut self, _body: u32, _out: u32) -> u32 {
        self.0.push("point");
        0
    }

    fn walk(&mut self, _copy: u32, _point: u32, _out: u32) {
        self.0.push("walk");
    }

    fn accumulate(&mut self, _body: u32, _force: u32) {
        self.0.push("accumulate");
    }
}

#[test]
fn accum_250_walks_the_gravity_and_scales_it_into_the_force() {
    let _pc24 = Precision::Pc24.scope();
    let (globals, physics, def, body, object, temp) = (0x1659000, 0x1659100, 0x200000, 0x300000, 0x310000, 0x320000);
    let setup = |inverse_mass: f32, flags: u32| {
        let mut mem = Mem::new();
        mem.map(globals, &[0; 0x1000]);
        mem.map(def, &[0; 0x17000]);
        mem.map(body, &[0; 0x100]);
        mem.map(object, &[0; 0x200]);
        mem.map(temp, &[0; 0x100]);
        mem.set_u32(0x1659b70, physics);
        mem.set_u32(physics + 0x24, def);
        mem.set_u32(physics + 0x38, flags);
        mem.set_f32(def + 0x160b0 + 8, -4.0);
        mem.set_u32(body + 0x34, object);
        mem.set_f32(object + 0x16c, inverse_mass);
        for (k, v) in [1.0f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
            mem.set_f32(temp + 0x10 + 4 * k as u32, v);
        }
        mem
    };

    let mut mem = setup(0.5, 0);
    let mut calls = Accum::default();
    assert_eq!(accumulate_forces(&mut mem, &mut calls, body, temp), Some(()));
    assert_eq!(calls.0, ["base", "point", "walk", "accumulate"]);
    assert_eq!(mem.f32(temp + 8), -4.0);
    let force: Vec<f32> = (0..4).map(|k| mem.f32(temp + 0x10 + 4 * k)).collect();
    assert_eq!(force, [2.0, 4.0, 6.0, 8.0]);

    // The log flag on the physics object calls the hook first.
    let mut mem = setup(0.5, 0x4000);
    let mut calls = Accum::default();
    accumulate_forces(&mut mem, &mut calls, body, temp);
    assert_eq!(calls.0[..2], ["hook", "base"]);

    // A zero inverse mass stops after the base.
    let mut mem = setup(0.0, 0);
    let mut calls = Accum::default();
    accumulate_forces(&mut mem, &mut calls, body, temp);
    assert_eq!(calls.0, ["base"]);
}

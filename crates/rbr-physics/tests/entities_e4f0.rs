//! `CHingedRigidBody::SimE4F0`'s car path (`entities::hinge_car_transform`): the calls in the original's order, with the arguments the
//! asm passes. The calls are scripted here (no records): the order and arguments are the port's, checked against the asm.

mod common;

use common::{check_windows, load, map_windows, Call};
use rbr_physics::entities::hinge_car_transform::{car_path, E4F0Calls};
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";

#[derive(Default)]
struct Log(Vec<String>);

impl E4F0Calls for Log {
    fn car_transform(&mut self, buf: u32) -> u32 {
        self.0.push(format!("car {buf:x}"));
        0x1000 + buf
    }

    fn body_walker(&mut self, v: u32, u: u32) -> u32 {
        self.0.push(format!("body {v:x} {u:x}"));
        7
    }

    fn compose_a(&mut self, x: u32, a: u32, b: u32, c: u32) {
        self.0.push(format!("compose {x:x} {a:x} {b:x} {c:x}"));
    }

    fn walk(&mut self, x: u32, a: u32, b: u32, c: u32) {
        self.0.push(format!("walk {x:x} {a:x} {b:x} {c:x}"));
    }

    fn car_e138(&mut self, args: [u32; 6]) -> u32 {
        self.0.push(format!("car138 {args:x?}"));
        5
    }
}

#[test]
fn car_path_runs_the_calls_in_the_original_order() {
    let mut log = Log::default();
    let eax = car_path(&mut log, [0x10, 0x20, 0x30, 1, 2, 3], [0x40, 0x50, 0x60]);
    assert_eq!(eax, 5);
    assert_eq!(
        log.0,
        [
            "car 40",
            "body 10 1040",
            "compose 7 10 1040 40",
            "car 50",
            "body 20 1050",
            "walk 7 20 1050 50",
            "car 60",
            "body 30 1060",
            "walk 7 30 1060 60",
            "car138 [10, 20, 30, 1, 2, 3]",
        ]
    );
}

struct Recorded<'a> {
    call: &'a Call,
    mem: &'a mut Mem,
    next: usize,
    car: u32,
    body: u32,
    car_target: u32,
    body_target: u32,
    e138_target: u32,
}

impl Recorded<'_> {
    fn take(&mut self, target: u32, args: &[u32]) -> u32 {
        let inner = self
            .call
            .inner
            .get(self.next)
            .unwrap_or_else(|| panic!("SimE4F0 call {}: inner {} is missing", self.call.index, self.next));
        assert_eq!(inner.addr, target, "SimE4F0 call {}: inner {} target", self.call.index, self.next);
        assert!(
            inner.args.starts_with(args),
            "SimE4F0 call {}: inner {} args {:?}",
            self.call.index,
            self.next,
            inner.args
        );
        for (_, at, _, pre, post) in &inner.windows {
            self.mem.map(*at, pre);
            self.mem.map(*at, post);
        }
        self.next += 1;
        inner.eax
    }
}

impl E4F0Calls for Recorded<'_> {
    fn car_transform(&mut self, buf: u32) -> u32 {
        self.take(self.car_target, &[self.car, buf])
    }

    fn body_walker(&mut self, v: u32, u: u32) -> u32 {
        self.take(self.body_target, &[self.body, v, u])
    }

    fn compose_a(&mut self, x: u32, a: u32, b: u32, c: u32) {
        self.take(0x51b4c0, &[x, a, b, c]);
    }

    fn walk(&mut self, x: u32, a: u32, b: u32, c: u32) {
        self.take(0x51b560, &[x, a, b, c]);
    }

    fn car_e138(&mut self, args: [u32; 6]) -> u32 {
        let mut expected = vec![self.car];
        expected.extend(args);
        self.take(self.e138_target, &expected)
    }
}

/// One owner-car call of `addr` (`SimE4F0`'s car path) against its record.
fn check_owner_call(call: &Call, name: &str) {
    let mut mem = map_windows(call);
    let this = call.args[0];
    let car = mem.u32(this + 0x44);
    let inner = &call.inner;
    assert_eq!(inner.len(), 10, "{name} call {}: call count", call.index);
    let car_target = inner[0].addr;
    let body_target = inner[1].addr;
    assert!(inner.iter().take(7).step_by(3).all(|i| i.addr == car_target));
    assert!(inner.iter().skip(1).take(7).step_by(3).all(|i| i.addr == body_target));
    let buffers = [inner[0].args[1], inner[3].args[1], inner[6].args[1]];
    let args: [u32; 6] = call.args[1..7].try_into().expect("six SimE4F0 arguments");
    let mut host =
        Recorded { call, mem: &mut mem, next: 0, car, body: this, car_target, body_target, e138_target: inner[9].addr };
    let eax = car_path(&mut host, args, buffers);
    assert_eq!(host.next, inner.len(), "{name} call {}: external call count", call.index);
    assert_eq!(eax, call.eax, "{name} call {}: eax", call.index);
    check_windows(call, &mem, name);
}

#[test]
fn car_path_matches_owner_car_records() {
    let Some(calls) = load(VAR, 0x51e4f0) else {
        eprintln!("skipping SimE4F0 direct records: {VAR} not set");
        return;
    };
    for call in &calls {
        check_owner_call(call, "CHingedRigidBody::SimE4F0");
    }
    eprintln!("SimE4F0: {} owner-car calls match", calls.len());
}

/// `CMountedRigidBody::Vf138` (0x521a50, `g-ent-proofs` direct records on mounted bodies): the owner-car path as
/// `SimE4F0`'s, and an unowned body's forward of `this` and the six words to `CSolid::E43Vf138` (0x52b990), its eax
/// passed on (`hinge_owned` decides, as the binding does).
#[test]
fn mounted_vf138_matches_records() {
    let Some(calls) = load(VAR, 0x521a50) else {
        eprintln!("skipping CMountedRigidBody::Vf138 direct records: {VAR} not set");
        return;
    };
    let mut unowned = 0;
    for call in &calls {
        let mem = map_windows(call);
        if rbr_physics::entities::layout::hinge_owned(&mem, call.args[0]) {
            check_owner_call(call, "CMountedRigidBody::Vf138");
            continue;
        }
        unowned += 1;
        let forward = call.inner.last().expect("the forward to E43Vf138");
        assert_eq!((forward.addr, &forward.args), (0x52b990, &call.args), "Vf138 call {}: forward", call.index);
        assert_eq!(forward.eax, call.eax, "Vf138 call {}: eax", call.index);
    }
    assert!(unowned > 0 && unowned < calls.len(), "CMountedRigidBody::Vf138: both paths");
    eprintln!("CMountedRigidBody::Vf138: {} calls match, {unowned} unowned", calls.len());
}

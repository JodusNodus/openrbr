//! `CHingedRigidBody::SimE1D0` (0x51e1d0) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `e1d0-*` vector files). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! The three walks (`E43PairWalk::MB560`) and the `ComposeA` under `CCar::BodyCompose` are answered from the record: each
//! must be the next recorded call, with the same arguments, and its recorded output is written where the original wrote
//! it. The port's pivot and settle are stored in the hinged body. Checked: the calls, the stored values, and every window.

mod common;

use common::{check_windows, load, map_windows, Call, Inner, Window};
use rbr_physics::entities::hinge_pivot::{self, PivotCalls, PivotHandles};
use rbr_physics::entities::store::HingeStore;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_E1D0: u32 = 0x51E1D0;
const WALK_MB560: u32 = 0x51B560;
const COMPOSE_A: u32 = 0x51B4C0;

/// The answers to the port's calls: the next recorded call, with its post windows applied.
struct Answers<'a> {
    mem: Mem,
    inner: &'a [Inner],
    next: usize,
}

impl<'a> Answers<'a> {
    fn take(&mut self, addr: u32) -> &'a Inner {
        let call = self
            .inner
            .get(self.next)
            .unwrap_or_else(|| panic!("the port made call {}, the original made {}", self.next, self.inner.len()));
        assert_eq!(call.addr, addr, "call {}: callee", self.next);
        self.next += 1;
        for Window { index: _, at, len: _, before: _, after: post } in &call.windows {
            self.mem.map(*at, post);
        }
        call
    }

    fn output(&self, out: u32) -> [f32; 3] {
        [self.mem.f32(out), self.mem.f32(out + 4), self.mem.f32(out + 8)]
    }
}

impl PivotCalls for Answers<'_> {
    fn store_vector(&mut self, at: u32, words: [u32; 3]) {
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.mem.map(at, &bytes);
    }

    fn carry(&mut self, receiver: u32, a: u32, b: u32, out: u32) -> [f32; 3] {
        let call = self.take(WALK_MB560);
        assert_eq!(call.args, [receiver, a, b, out], "walk {}: arguments", self.next);
        self.output(out)
    }

    fn compose_with_car(&mut self, _car: u32, a: u32, out: u32) -> [f32; 3] {
        // ComposeA's receiver and `t` are the body's own matrix and the car's sub-object, not the car itself
        let call = self.take(COMPOSE_A);
        assert_eq!(call.args[1], a, "BodyCompose {}: a", self.next);
        assert_eq!(call.args[3], out, "BodyCompose {}: out", self.next);
        self.output(out)
    }
}

/// The scratch of the port, as the original's frame holds it: the walks' outputs and the vector's address.
fn frame_of(call: &Call) -> PivotHandles {
    let walks: Vec<&Inner> = call.inner.iter().filter(|i| i.addr == WALK_MB560).collect();
    assert_eq!(walks.len(), 3, "call {}: three walks", call.index);
    let compose =
        call.inner.iter().find(|i| i.addr == COMPOSE_A).unwrap_or_else(|| panic!("call {}: no ComposeA", call.index));
    let scratch = walks[2].args[3];
    assert_eq!(compose.args[3], scratch + 0x10, "call {}: BodyCompose writes mC + 1", call.index);
    PivotHandles {
        vector: walks[0].args[1],
        acceleration: walks[0].args[3],
        angular_acceleration: walks[1].args[3],
        angular_velocity: scratch,
        lever: scratch + 0x10,
    }
}

#[test]
fn sim_e1d0_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_E1D0) else {
        eprintln!("skipping SimE1D0: {VAR} not set");
        return;
    };
    let mut walks = 0;
    for call in &calls {
        let mem = map_windows(call);
        let this = call.args[0];
        let input = mem.pivot_input(this);
        let frame = frame_of(call);
        let mut answers = Answers { mem, inner: &call.inner, next: 0 };
        let result = hinge_pivot::pivot_terms(&mut answers, &input, &frame);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        answers.mem.store_pivot(this, &result);
        check_windows(call, &answers.mem, "CHingedRigidBody::SimE1D0");
        walks += answers.next;
    }
    eprintln!("CHingedRigidBody::SimE1D0: {} calls match ({walks} calls answered from the record)", calls.len());
}

//! `CHingedRigidBody::SimD160` (0x51d160) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `d160-*` vector files). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! Every call the port makes is answered from the record: it must be the next recorded call, with the same arguments. Before
//! each call, the body's bytes in the record's windows must be what the port has (its stores), and the record's post bytes then
//! replace them. Checked: the calls in order, the arguments, the body before each call, and every window at the end.

mod common;

use common::{check_windows, load, map_windows, Inner};
use rbr_physics::entities::hinge_pose::{self, PoseCalls, PoseState};
use rbr_physics::entities::store::EntityStore;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_D160: u32 = 0x51D160;
const SIM_C960: u32 = 0x51C960;
const XFORM_8130: u32 = 0x4E8130;
const MB560: u32 = 0x51B560;
const TO_QUAT: u32 = 0x4E4D10;
/// The body's extent: the window of `SimD160` itself (0x660 bytes from `this`).
const BODY_LEN: u32 = 0x660;

/// The answers to the port's calls: the next recorded call, checked against the body, then its post bytes applied.
struct Answers<'a> {
    mem: Mem,
    this: u32,
    inner: &'a [Inner],
    next: usize,
}

impl<'a> Answers<'a> {
    fn take(&mut self, addr: u32) -> &'a Inner {
        let index = self.next;
        let call = self
            .inner
            .get(index)
            .unwrap_or_else(|| panic!("the port made call {index}, the original made {}", self.inner.len()));
        assert_eq!(call.addr, addr, "call {index}: callee");
        self.next += 1;
        // every window inside the body: its bytes before the call are the port's stores (the products, the rotation)
        for (_, at, _, pre, _) in &call.windows {
            if (self.this..self.this + BODY_LEN).contains(at) {
                for (i, word) in pre.chunks_exact(4).enumerate() {
                    let recorded = u32::from_le_bytes(word.try_into().unwrap());
                    let word_at = at + 4 * i as u32;
                    assert_eq!(
                        self.mem.u32(word_at),
                        recorded,
                        "call {index}: the body at {word_at:#x} before the call"
                    );
                }
            }
        }
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        call
    }
}

impl PoseCalls for Answers<'_> {
    fn load(&self) -> PoseState {
        self.mem.d160_words(self.this)
    }

    fn store(&mut self, state: &PoseState) {
        self.mem.store_d160_words(self.this, state);
    }

    fn car_transform(&mut self, car: u32, pose: u32, out: u32) {
        let call = self.take(XFORM_8130);
        assert_eq!(call.args, [car, pose, out], "Xform8130: arguments");
    }

    fn carry_into_car(&mut self, car: u32, a: u32, identity: u32, out: u32) {
        let call = self.take(MB560);
        assert_eq!(call.args, [car, a, identity, out], "MB560: arguments");
    }

    fn update_hinge_rotation(&mut self, this: u32) {
        let call = self.take(SIM_C960);
        assert_eq!(call.args, [this], "SimC960: arguments");
    }

    fn matrix_to_quaternion(&mut self, matrix: u32, out: u32) {
        let call = self.take(TO_QUAT);
        assert_eq!(call.args, [matrix, out], "ToQuat: arguments");
    }
}

#[test]
fn sim_d160_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_D160) else {
        eprintln!("skipping SimD160: {VAR} not set");
        return;
    };
    let mut answered = 0;
    for call in &calls {
        let mem = map_windows(call);
        let this = call.args[0];
        let frame = mem.d160_frame(this);
        let mut answers = Answers { mem, this, inner: &call.inner, next: 0 };
        hinge_pose::update_pose(&mut answers, &frame);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        check_windows(call, &answers.mem, "CHingedRigidBody::SimD160");
        answered += answers.next;
    }
    assert!(answered > 0, "no record reaches the port's calls");
    eprintln!("CHingedRigidBody::SimD160: {} calls match ({answered} calls answered from the record)", calls.len());
}

//! `CHingedRigidBody::SimCFF0` (0x51cff0) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `cff0-*` vector files). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! The two walks (`E43PairWalk::MB560`) are answered from the record: each must be the next recorded call, with the same
//! receiver and arguments, and its recorded output is written where the original wrote it. The copy of the state vector
//! is made by the test (the port's `copy_vector`). Checked: the slip term (st0), the walks, and every window.

mod common;

use common::{check_windows, load, map_windows, Call, Inner};
use rbr_physics::entities::hinge_slip::{self, SlipCalls, SlipHandles};
use rbr_physics::entities::store::HingeStore;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_CFF0: u32 = 0x51CFF0;
const WALK_MB560: u32 = 0x51B560;

/// The f32 port value as the record holds it (st0 as f64 bits, exact).
fn st0_of(value: f32) -> u64 {
    (value as f64).to_bits()
}

/// The answers to the port's calls: the walks are the record's, in order, and the copy is made in memory.
struct Answers<'a> {
    mem: Mem,
    inner: &'a [Inner],
    next: usize,
}

impl SlipCalls for Answers<'_> {
    fn copy_vector(&mut self, to: u32, from: u32) {
        let bytes = self.mem.read(from, 16);
        self.mem.map(to, &bytes);
    }

    fn carry(&mut self, receiver: u32, a: u32, b: u32, out: u32) -> [f32; 3] {
        let call = self
            .inner
            .get(self.next)
            .unwrap_or_else(|| panic!("the port made walk {}, the original made {}", self.next, self.inner.len()));
        assert_eq!(call.addr, WALK_MB560, "walk {}: callee", self.next);
        assert_eq!(call.args, [receiver, a, b, out], "walk {}: arguments", self.next);
        self.next += 1;
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        [self.mem.f32(out), self.mem.f32(out + 4), self.mem.f32(out + 8)]
    }

    fn car_speed(&self, state: u32) -> f32 {
        self.mem.car_state_c00(state)
    }
}

/// The scratch of the port, as the original's frame holds it: the walks' outputs and the copy's address.
fn frame_of(call: &Call) -> SlipHandles {
    let walks: Vec<&Inner> = call.inner.iter().filter(|i| i.addr == WALK_MB560).collect();
    assert_eq!(walks.len(), 2, "call {}: two walks", call.index);
    SlipHandles { vector_copy: walks[1].args[1], face_normal: walks[0].args[3], velocity: walks[1].args[3] }
}

#[test]
fn slip_term_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_CFF0) else {
        eprintln!("skipping SimCFF0: {VAR} not set");
        return;
    };
    let mut walks = 0;
    for call in &calls {
        let mem = map_windows(call);
        let input = mem.slip_input(call.args[0]);
        let frame = frame_of(call);
        let mut answers = Answers { mem, inner: &call.inner, next: 0 };
        let result = hinge_slip::slip_torque(&mut answers, &input, &frame);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's walks", call.index);
        if f64::from_bits(call.st0_bits).is_nan() {
            // a NaN's sign is the platform's (rbr-recomp notes/rust-physics.md, section 8): both must be NaN
            assert!(result.is_nan(), "call {}: slip term NaN", call.index);
        } else {
            assert_eq!(st0_of(result), call.st0_bits, "call {}: slip term", call.index);
        }
        check_windows(call, &answers.mem, "CHingedRigidBody::SimCFF0");
        walks += answers.next;
    }
    eprintln!("CHingedRigidBody::SimCFF0: {} calls match ({walks} walks answered from the record)", calls.len());
}

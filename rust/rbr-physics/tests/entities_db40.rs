//! `CHingedRigidBody::SimDB40` (0x51db40) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `fb40-*` vector files, where SimDB40 is a record of its own). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`;
//! without it the test is skipped.
//!
//! The walks (`E43PairWalk::MB560`) and `CCar::Xform8130` are answered from the record: each must be the next recorded call,
//! with the same receiver and arguments, and its input vector must be the bytes the original had in the vector's window
//! before the call. Checked: the calls in order, the input vectors, and every window of the hinged body.

mod common;

use common::{check_windows, load, map_windows, Inner};
use rbr_physics::entities::hinge_db40::{self, DbCalls};
use rbr_physics::entities::layout;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_DB40: u32 = 0x51DB40;
const WALK_MB560: u32 = 0x51B560;
const XFORM_8130: u32 = 0x4E8130;
/// The argument index of the input vector (`this` is argument 0 for the walks and the transform).
const VECTOR_ARG: usize = 1;

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
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        call
    }
}

/// The port's vector is the bytes the original had at its address before the call (both NaN is the same NaN, as the default
/// NaN sign differs between x86 and arm64; see notes/rust-physics.md).
fn check_vector(call: &Inner, a: [f32; 3], index: usize) {
    // The window's index is its place in the spec's list (the walk's output is the second); its address is the argument.
    let at = call.args[VECTOR_ARG];
    let window = call
        .windows
        .iter()
        .find(|w| w.1 == at)
        .unwrap_or_else(|| panic!("call {index}: no window at the input vector {at:#x}"));
    for (i, value) in a.iter().enumerate() {
        let bytes: [u8; 4] = window.3[4 * i..4 * i + 4].try_into().unwrap();
        let recorded = f32::from_le_bytes(bytes);
        assert!(
            value.to_bits() == recorded.to_bits() || (value.is_nan() && recorded.is_nan()),
            "call {index}: input {i}: {value} (port) vs {recorded} (original)"
        );
    }
}

impl DbCalls for Answers<'_> {
    fn walk(&mut self, receiver: u32, a: [f32; 3], b: u32) {
        let index = self.next;
        let call = self.take(WALK_MB560);
        assert_eq!(call.args[0], receiver, "walk {index}: receiver");
        assert_eq!(call.args[2], b, "walk {index}: b");
        check_vector(call, a, index);
    }

    fn xform8130(&mut self, car: u32, a: [f32; 3]) {
        let index = self.next;
        let call = self.take(XFORM_8130);
        assert_eq!(call.args[0], car, "transform {index}: car");
        check_vector(call, a, index);
    }
}

#[test]
fn sim_db40_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_DB40) else {
        eprintln!("skipping SimDB40: {VAR} not set");
        return;
    };
    let mut walks = 0;
    let mut taken = 0;
    for call in &calls {
        let mem = map_windows(call);
        let this = call.args[0];
        let mut answers = Answers { mem, inner: &call.inner, next: 0 };
        match layout::db40_input(&answers.mem, this) {
            Some(input) => {
                taken += 1;
                hinge_db40::sim_db40(&mut answers, &input);
            }
            None => assert!(call.inner.is_empty(), "call {}: the guard fails, no calls", call.index),
        }
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        check_windows(call, &answers.mem, "CHingedRigidBody::SimDB40");
        walks += answers.next;
    }
    assert!(taken > 0, "no record passes the guard: the calls are not tested");
    eprintln!(
        "CHingedRigidBody::SimDB40: {} calls match, {taken} with the guard passed ({walks} calls answered from the record)",
        calls.len()
    );
}

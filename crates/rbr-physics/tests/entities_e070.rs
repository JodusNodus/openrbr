//! `CHingedRigidBody::VfE070` (0x51e070) replayed against the calls SimEFE0 made. The records are the inner calls of the
//! `efe0-*` files (`harness/vcu_vectors.py --set entities_b5`): VfE070 is external in SimEFE0's spec, so its six windows
//! (before and after the call) are the record. Needs `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! The port runs on the windows before the call and must leave the bytes the original left in them: the frame `e`, the
//! transposed `f` and `d`. Checked: every window after the call.

mod common;

use common::{load_inner, Inner};
use rbr_physics::entities::hinge_e070::E070Frame;
use rbr_physics::entities::layout;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const VF_E070: u32 = 0x51E070;

/// The windows the call wrote, after the port ran.
fn check_windows(call: &Inner, mem: &Mem, index: usize) {
    for (k, at, len, _, post) in &call.windows {
        assert_eq!(mem.read(*at, *len), *post, "VfE070 inner call {index} window {k}");
    }
}

#[test]
fn vf_e070_matches_original_calls() {
    let Some(calls) = load_inner(VAR, VF_E070) else {
        eprintln!("skipping VfE070: {VAR} not set");
        return;
    };
    for (index, call) in calls.iter().enumerate() {
        let mut mem = Mem::new();
        for (_, at, _, pre, _) in &call.windows {
            mem.map(*at, pre);
        }
        let [_this, a, b, c, d, e, f] = call.args[..] else {
            panic!("VfE070 inner call {index}: {} arguments", call.args.len());
        };
        layout::vf_e070(&mut mem, &E070Frame { a, b, c, d, e, f });
        check_windows(call, &mem, index);
    }
    eprintln!("CHingedRigidBody::VfE070: {} calls match (from the SimEFE0 records)", calls.len());
}

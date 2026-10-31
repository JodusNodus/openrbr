//! The hinged body's limit and rotation sims (`CHingedRigidBody::SimC960`, `SimCF10`, `SimEA80`) replayed against the
//! original's recorded calls (`harness/vcu_vectors.py --set entities_b5`, the `b5-*` vector files). Needs
//! `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the tests are skipped.
//!
//! Each port runs on the record's windows; its result (the x87 st0, as f64 bits) and every window must be the original's.
//! The CRT sine and cosine are checked on the results they were recorded with, as the rotation's inner calls.

mod common;

use common::{check_windows, load, map_windows, st0_of, Call, Inner};
use rbr_physics::entities::hinge_limits::{self, LimitBranch};
use rbr_physics::entities::layout;
use rbr_physics::entities::store::HingeStore;
use rbr_physics::fpu::Precision;
use rbr_physics::math::trig;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_C960: u32 = 0x51C960;
const SIM_CF10: u32 = 0x51CF10;
const SIM_EA80: u32 = 0x51EA80;
const CRT_SIN: u32 = 0x53F3B0;
const CRT_COS: u32 = 0x53F3A0;

/// The record's st0 rounded to f32 (the store the asm makes).
fn stored_as_f32(st0_bits: u64) -> u32 {
    (f64::from_bits(st0_bits) as f32).to_bits()
}

/// The inner call `addr` of `call` (its one x87 argument, the angle).
fn inner_call(call: &Call, addr: u32) -> &Inner {
    call.inner.iter().find(|i| i.addr == addr).unwrap_or_else(|| panic!("call {}: no inner call {addr:#x}", call.index))
}

#[test]
fn hinge_crt_sin_cos_match_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, SIM_C960) else {
        eprintln!("skipping the CRT sine and cosine: {VAR} not set");
        return;
    };
    let (mut exact, mut checked) = (0, 0);
    for call in &calls {
        let angle_bits = call.windows[0].before[0x3FC..0x400].iter().rev().fold(0u32, |w, b| (w << 8) | *b as u32);
        let angle = f32::from_bits(angle_bits);
        for (addr, port) in [(CRT_SIN, trig::x87_sin as fn(f32) -> f64), (CRT_COS, trig::x87_cos)] {
            let inner = inner_call(call, addr);
            assert_eq!(inner.args, [angle_bits], "call {}: {addr:#x} argument", call.index);
            // the rotation keeps the f32 store of the result; the f64 value is within f64 rounding of the record's
            assert_eq!(
                (port(angle) as f32).to_bits(),
                stored_as_f32(inner.st0_bits),
                "call {}: {addr:#x} of {angle:?}",
                call.index
            );
            exact += usize::from(port(angle).to_bits() == inner.st0_bits);
            checked += 1;
        }
    }
    eprintln!("CRT sin and cos: {checked} calls match at the f32 store ({exact} equal as f64)");
}

#[test]
fn hinge_rotation_block_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, SIM_C960) else {
        eprintln!("skipping SimC960: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        layout::hinge_rotation(&mut mem, call.args[0]);
        check_windows(call, &mem, "CHingedRigidBody::SimC960");
    }
    eprintln!("CHingedRigidBody::SimC960: {} calls match", calls.len());
}

/// The hinge limit's residual: `SimCF10(idx)` on the record's windows, with its st0 and its windows checked.
#[test]
fn hinge_limit_residual_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, SIM_CF10) else {
        eprintln!("skipping SimCF10: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let (this, idx) = (call.args[0], call.args[1]);
        let step = layout::hinge_limit_residual(&mut mem, this, idx);
        assert_eq!(step.torque.to_bits(), call.st0_bits, "call {}: residual", call.index);
        check_windows(call, &mem, "CHingedRigidBody::SimCF10");
        *branches.entry(format!("{:?}", step.branch)).or_default() += 1;
    }
    eprintln!("CHingedRigidBody::SimCF10: {} calls match, by branch: {branches:?}", calls.len());
}

/// The sum over the limits: `SimEA80` on the record's windows; its direct calls (`SimCF10` for each active limit) must be
/// the recorded ones, in order.
#[test]
fn hinge_limit_sum_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, SIM_EA80) else {
        eprintln!("skipping SimEA80: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let def = mem.u32(this + 0x34);
        let expected: Vec<u32> = mem
            .load_hinge_limits(def)
            .iter()
            .enumerate()
            .filter(|(_, limit)| limit.active())
            .map(|(idx, _)| idx as u32)
            .collect();
        let recorded: Vec<u32> = call.inner.iter().filter(|i| i.addr == SIM_CF10).map(|i| i.args[1]).collect();
        assert_eq!(recorded, expected, "call {}: the active limits, in order", call.index);
        let sum = layout::hinge_limit_sum(&mut mem, this);
        assert_eq!(st0_of(sum), call.st0_bits, "call {}: sum", call.index);
        check_windows(call, &mem, "CHingedRigidBody::SimEA80");
    }
    eprintln!("CHingedRigidBody::SimEA80: {} calls match", calls.len());
}

/// NaN takes the settled branch as the asm's `jbe` does. No record holds a NaN here, so this is a unit check of the
/// reading, not evidence from the original.
#[test]
fn hinge_limit_nan_settles_like_the_asm() {
    let _pc24 = Precision::Pc24.scope();
    let limit = hinge_limits::LimitSpring { kind: 2, stiffness: 1.0, damping: 1.0, max_displacement: 0.5, span: 0.25 };
    let state = hinge_limits::HingeState { spring_position: [0.0, 0.0], angle: f32::NAN, rate: 0.0 };
    let step = hinge_limits::limit_torque(&limit, state.spring_position[0], state.angle, state.rate);
    assert_eq!(step.branch, LimitBranch::Settled);
    assert_eq!(step.torque, 0.0);
    assert_eq!(step.new_position, None);
}

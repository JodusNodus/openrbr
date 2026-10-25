//! `CHingedRigidBody::SimEAD0` (0x51ead0) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `ead0-*` vector files). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! The sub-calls are answered from the record, each the next recorded call with the same callee and arguments, and its
//! recorded writes applied: `SimE1D0`, `SimCFF0`, `SimEA80` and the vote (`E47Solid::Vf81x`). The port's value is stored
//! at +0x624 and every window must then be the original's.

mod common;

use common::{check_windows, load, map_windows, Call, Inner};
use rbr_physics::entities::hinge_solve::{self, ContactHandles, Outcome, SolveCalls};
use rbr_physics::entities::store::HingeStore;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_EAD0: u32 = 0x51EAD0;
const SIM_E1D0: u32 = 0x51E1D0;
const SIM_CFF0: u32 = 0x51CFF0;
const SIM_EA80: u32 = 0x51EA80;
const VOTE: u32 = 0x52C510;
/// `CSolid::E43Vf64` (0x52cb20): the force pass an unowned body gets.
const FORCE_PASS: u32 = 0x52CB20;
/// The vote's third argument, 3.0f.
const VOTE_THIRD: u32 = 0x4040_0000;
/// The hinged body's settle output (+0x624).
const HINGE_OUT: u32 = 0x624;

/// The answers to the port's calls, each the next recorded call of the original's.
struct Answers<'a> {
    mem: Mem,
    inner: &'a [Inner],
    next: usize,
}

impl<'a> Answers<'a> {
    /// The next recorded call, which must be `addr` with `args`; its writes (post windows and `after` runs) are applied.
    fn take(&mut self, addr: u32, args: &[u32]) -> &'a Inner {
        let inner: &'a [Inner] = self.inner;
        let call = inner
            .get(self.next)
            .unwrap_or_else(|| panic!("the port made call {}, the original made {}", self.next, inner.len()));
        assert_eq!(call.addr, addr, "call {}: callee", self.next);
        assert_eq!(call.args, args, "call {} ({addr:#x}): arguments", self.next);
        self.next += 1;
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        for (at, _, bytes) in &call.after {
            self.mem.map(*at, bytes);
        }
        call
    }
}

impl SolveCalls for Answers<'_> {
    fn linked(&self, this: u32) -> bool {
        self.mem.hinge_linked(this)
    }

    fn owned(&self, this: u32) -> bool {
        self.mem.hinge_owned(this)
    }

    fn force_pass(&mut self, this: u32) -> u32 {
        self.take(FORCE_PASS, &[this]).eax
    }

    fn entry_terms(&self, this: u32) -> hinge_solve::EntryTerms {
        self.mem.solve_entry_terms(this)
    }

    fn update_pivot(&mut self, this: u32) {
        self.take(SIM_E1D0, &[this]);
    }

    fn solve_terms(&self, this: u32) -> hinge_solve::SolveTerms {
        self.mem.solve_terms(this)
    }

    fn slip_torque(&mut self, this: u32) -> f32 {
        f64::from_bits(self.take(SIM_CFF0, &[this]).st0_bits) as f32
    }

    fn limit_torque_sum(&mut self, this: u32) -> f32 {
        f64::from_bits(self.take(SIM_EA80, &[this]).st0_bits) as f32
    }

    fn contact_load(&mut self, this: u32, frame: &ContactHandles) -> Option<([f32; 3], [f32; 3])> {
        let call = self.take(VOTE, &[this, frame.force, frame.torque, VOTE_THIRD]);
        if call.eax == 0 {
            return None;
        }
        let vec3 = |at: u32| [self.mem.f32(at), self.mem.f32(at + 4), self.mem.f32(at + 8)];
        Some((vec3(frame.force), vec3(frame.torque)))
    }
}

/// The vote's frame: its two output vectors, the arguments of the recorded vote (none for an unowned body, which
/// makes no vote).
fn frame_of(call: &Call) -> ContactHandles {
    let (v90, v90b) = call.inner.iter().find(|i| i.addr == VOTE).map_or((0, 0), |vote| (vote.args[1], vote.args[2]));
    ContactHandles { force: v90, torque: v90b }
}

#[test]
fn hinge_solve_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_EAD0) else {
        eprintln!("skipping SimEAD0: {VAR} not set");
        return;
    };
    let (mut hits, mut unowned) = (0, 0);
    for call in &calls {
        let mem = map_windows(call);
        let this = call.args[0];
        let frame = frame_of(call);
        let mut answers = Answers { mem, inner: &call.inner, next: 0 };
        let outcome = hinge_solve::hinge_solve(&mut answers, this, &frame);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        unowned += usize::from(matches!(outcome, Outcome::Unowned { .. }));
        if let Outcome::Solved(value) = outcome {
            // a NaN keeps the record's bits: the sign of a default NaN is the platform's (rbr-recomp notes/rust-physics.md, section 8)
            let recorded =
                f32::from_le_bytes(call.windows[0].4[HINGE_OUT as usize..HINGE_OUT as usize + 4].try_into().unwrap());
            assert_eq!(value.is_nan(), recorded.is_nan(), "call {}: NaN", call.index);
            answers.mem.set_f32(this + HINGE_OUT, if value.is_nan() { recorded } else { value });
        }
        check_windows(call, &answers.mem, "CHingedRigidBody::SimEAD0");
        hits += usize::from(call.inner.iter().any(|i| i.addr == VOTE && i.eax != 0));
    }
    eprintln!("CHingedRigidBody::SimEAD0: {} calls match ({hits} with a vote hit, {unowned} unowned)", calls.len());
}

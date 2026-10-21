//! `CHingedRigidBody::SimD860` (0x51d860) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `d860-*` vector files, where SimD860 is a record of its own). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`;
//! without it the test is skipped.
//!
//! The walk (`E43PairWalk::MB560`) is answered from the record: it must be the next recorded call, with the same receiver and
//! `b`, and its input vector must be the bytes the original had in the vector's window before the call. Its output is the
//! record's. Checked: the call, the input vector, the stored `out`, and every window.

mod common;

use common::{check_windows, load, map_windows, Inner};
use rbr_physics::entities::hinge_offset::{self, D860Calls};
use rbr_physics::entities::layout;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_D860: u32 = 0x51D860;
const WALK_MB560: u32 = 0x51B560;

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

impl D860Calls for Answers<'_> {
    fn walk(&mut self, receiver: u32, a: [f32; 3], b: u32) -> [f32; 3] {
        let index = self.next;
        let call = self.take(WALK_MB560);
        assert_eq!(call.args[0], receiver, "walk {index}: receiver");
        assert_eq!(call.args[2], b, "walk {index}: b");
        // The input vector's window is found by its address; the output is the record's post bytes at `out`.
        let at = call.args[1];
        let window = call
            .windows
            .iter()
            .find(|w| w.1 == at)
            .unwrap_or_else(|| panic!("walk {index}: no window at the input vector {at:#x}"));
        let recorded = [0, 1, 2].map(|i| {
            let bytes: [u8; 4] = window.3[4 * i..4 * i + 4].try_into().unwrap();
            f32::from_le_bytes(bytes)
        });
        for (i, (port, original)) in a.iter().zip(recorded).enumerate() {
            assert!(
                port.to_bits() == original.to_bits() || (port.is_nan() && original.is_nan()),
                "walk {index}: input {i}: {port} (port) vs {original} (original)"
            );
        }
        let out = call.args[3];
        [0, 4, 8].map(|o| self.mem.f32(out + o))
    }
}

#[test]
fn sim_d860_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_D860) else {
        eprintln!("skipping SimD860: {VAR} not set");
        return;
    };
    let mut walks = 0;
    for call in &calls {
        let mem = map_windows(call);
        let this = call.args[0];
        let out = call.args[1];
        let input = layout::d860_input(&mem, this);
        let mut answers = Answers { mem, inner: &call.inner, next: 0 };
        let result = hinge_offset::sim_d860(&mut answers, &input);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        for (i, value) in result.iter().enumerate() {
            answers.mem.set_f32(out + 4 * i as u32, *value);
        }
        check_windows(call, &answers.mem, "CHingedRigidBody::SimD860");
        walks += answers.next;
    }
    assert!(walks > 0, "no record reaches the walk");
    eprintln!("CHingedRigidBody::SimD860: {} calls match ({walks} walks answered from the record)", calls.len());
}

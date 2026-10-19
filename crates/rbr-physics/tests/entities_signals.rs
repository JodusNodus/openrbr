//! `CHingedRigidBody::SimFB40` (0x51fb40) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `fb40-*` vector files: the load-time entry dumps, with and without fuzz). Needs
//! `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! Each call the port makes is the next recorded call: `SimDB40`, the pose source (`ComposeA` or `ToQuat`), the two
//! emits and `E43Vf28`. The emits' payloads are checked against the recorded bytes, except the padding words 1..=3 that
//! the original does not write. The recorded outputs of the pose source are written where the original wrote them.
//! Checked: the calls, the payloads, and every window.

mod common;

use common::{check_windows, load, map_windows, Inner};
use rbr_physics::entities::hinge_signals::{self, HingedBodySignal, SignalCalls, SolidGeneralState};
use rbr_physics::entities::layout;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_FB40: u32 = 0x51FB40;
const SIM_DB40: u32 = 0x51DB40;
const COMPOSE_A: u32 = 0x51B4C0;
const TO_QUAT: u32 = 0x4E4D10;
const EMIT_SOLID: u32 = 0x4E8330;
const EMIT_HINGED: u32 = 0x51F640;
const VF28: u32 = 0x51A3E0;

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

    fn floats(&self, at: u32) -> [f32; 4] {
        std::array::from_fn(|i| self.mem.f32(at + 4 * i as u32))
    }
}

/// The words of the recorded payload (its window), little-endian.
fn words(bytes: &[u8]) -> Vec<u32> {
    bytes.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

/// The solid state's words as the original lays them out (0x58 bytes); words 1..=3 are padding.
fn solid_words(state: &SolidGeneralState) -> [u32; 22] {
    let mut w = [0u32; 22];
    w[0] = state.key;
    let vectors = [state.pose, state.rotation, state.first, state.second];
    for (k, v) in vectors.iter().enumerate() {
        for (i, f) in v.iter().enumerate() {
            w[4 + 4 * k + i] = f.to_bits();
        }
    }
    w[20] = state.unit.to_bits();
    w
}

impl SignalCalls for Answers<'_> {
    fn sim_db40(&mut self, hinge: u32) {
        let call = self.take(SIM_DB40);
        assert_eq!(call.args, [hinge], "SimDB40: arguments");
    }

    fn compose_a(&mut self, hinge: u32, a: u32, t: u32) -> [f32; 4] {
        let call = self.take(COMPOSE_A);
        assert_eq!(call.args[..3], [hinge, a, t], "ComposeA: arguments");
        self.floats(call.args[3])
    }

    fn to_quat(&mut self, quat: u32) -> [f32; 4] {
        let call = self.take(TO_QUAT);
        assert_eq!(call.args[0], quat, "ToQuat: receiver");
        self.floats(call.args[1])
    }

    fn emit_solid(&mut self, state: &SolidGeneralState) {
        let call = self.take(EMIT_SOLID);
        let recorded = words(&call.windows[0].4);
        let expected = solid_words(state);
        for i in (0..22).filter(|i| !(1..=3).contains(i)) {
            assert_eq!(recorded[i], expected[i], "SolidGeneralState word {i}");
        }
    }

    fn emit_hinged(&mut self, body: &HingedBodySignal) {
        let call = self.take(EMIT_HINGED);
        let recorded = words(&call.windows[0].4);
        let expected = [body.definition_value, body.unmoved, body.angle_delta.to_bits()];
        assert_eq!(recorded, expected, "HingedBodySignal");
    }

    fn vf28(&mut self, hinge: u32) {
        let call = self.take(VF28);
        assert_eq!(call.args, [hinge], "E43Vf28: arguments");
    }
}

#[test]
fn sim_fb40_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_FB40) else {
        eprintln!("skipping SimFB40: {VAR} not set");
        return;
    };
    let mut answered = 0;
    for call in &calls {
        let mem = map_windows(call);
        let input = layout::signal_input(&mem, call.args[0]);
        let mut answers = Answers { mem, inner: &call.inner, next: 0 };
        hinge_signals::emit_state(&mut answers, &input);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        check_windows(call, &answers.mem, "CHingedRigidBody::SimFB40");
        answered += answers.next;
    }
    eprintln!("CHingedRigidBody::SimFB40: {} calls match ({answered} calls answered from the record)", calls.len());
}

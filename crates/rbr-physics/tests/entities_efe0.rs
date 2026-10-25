//! `CHingedRigidBody::SimEFE0` (0x51efe0) replayed against the original's recorded calls (`harness/vcu_vectors.py --set
//! entities_b5`, the `efe0-*` vector files). Needs `RBR_ENT_VECTORS=<file>[:<file>...]`; without it the test is skipped.
//!
//! Every call the port makes is answered from the record: it must be the next recorded call, with the same arguments. Before
//! the call, the body's bytes must be what the original had (the port's stores), and the record's post bytes then replace
//! them, so the port continues from the original's values. `VfE070` is the exception: the port makes it (`layout::vf_e070`)
//! on the record's windows, and the windows it leaves must be the record's post bytes. Checked: the calls in order, the
//! arguments, the body before each call, and every window at the end.

mod common;

use common::{check_windows, load, map_windows, Inner};
use rbr_physics::entities::hinge_axes::AxesHandles;
use rbr_physics::entities::hinge_refresh::{self, RefreshCalls, RefreshState};
use rbr_physics::entities::layout;
use rbr_physics::entities::store::HingeStore;
use rbr_physics::mem::Mem;

const VAR: &str = "RBR_ENT_VECTORS";
const SIM_EFE0: u32 = 0x51EFE0;
const SLOT36: u32 = 0x52BF30;
const VF_E070: u32 = 0x51E070;
const BODY_COMPOSE: u32 = 0x4E6A30;
const SIM_D160: u32 = 0x51D160;
const SIM_D860: u32 = 0x51D860;
const ADD_EDGE: u32 = 0x4F65B0;

/// The answers to the port's calls: the next recorded call, checked against the body, then its post bytes applied.
struct Answers<'a> {
    mem: Mem,
    this: u32,
    inner: &'a [Inner],
    next: usize,
}

impl<'a> Answers<'a> {
    /// The next recorded call, which must be `addr`.
    fn next_call(&mut self, addr: u32) -> &'a Inner {
        let index = self.next;
        let call = self
            .inner
            .get(index)
            .unwrap_or_else(|| panic!("the port made call {index}, the original made {}", self.inner.len()));
        assert_eq!(call.addr, addr, "call {index}: callee");
        self.next += 1;
        call
    }

    /// The body's bytes before `call`: the port's stores must be the original's.
    fn check_body(&self, index: usize, call: &Inner) {
        if let Some(pre) = call.windows.iter().find(|w| w.1 == self.this) {
            for (i, word) in pre.3.chunks_exact(4).enumerate() {
                let recorded = u32::from_le_bytes(word.try_into().unwrap());
                let at = self.this + 4 * i as u32;
                assert_eq!(self.mem.u32(at), recorded, "call {index}: the body at {at:#x} before the call");
            }
        }
    }

    /// Answers a call from the record: checks the body, then applies the record's post bytes.
    fn take(&mut self, addr: u32) -> &'a Inner {
        let index = self.next;
        let call = self.next_call(addr);
        self.check_body(index, call);
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        call
    }
}

impl RefreshCalls for Answers<'_> {
    fn load(&self) -> RefreshState {
        self.mem.efe0_state(self.this)
    }

    fn store(&mut self, state: &RefreshState) {
        self.mem.store_efe0_state(self.this, state);
    }

    fn wake_body(&mut self, this: u32) {
        let call = self.take(SLOT36);
        assert_eq!(call.args, [this], "slot 36: arguments");
    }

    fn build_axes(&mut self, this: u32, args: [u32; 6]) {
        let call = self.next_call(VF_E070);
        let mut expected = vec![this];
        expected.extend(args);
        assert_eq!(call.args, expected, "VfE070: arguments");
        // the port makes the call on the record's windows (the bytes before it); what it leaves must be the record's
        // after bytes, which then stand as the memory
        for (_, at, _, pre, _) in &call.windows {
            self.mem.map(*at, pre);
        }
        let [a, b, c, d, e, f] = args;
        layout::vf_e070(
            &mut self.mem,
            &AxesHandles { origin: a, direction: b, reference: c, inverse_origin: d, axes: e, inverse_axes: f },
        );
        for (k, at, len, _, post) in &call.windows {
            assert_eq!(self.mem.read(*at, *len), *post, "VfE070 window {k} (the port's product)");
        }
    }

    fn compose_with_car(&mut self, car: u32, a: u32, out: u32) {
        let call = self.take(BODY_COMPOSE);
        assert_eq!(call.args, [car, a, out], "BodyCompose: arguments");
    }

    fn update_pose(&mut self, this: u32) {
        let call = self.take(SIM_D160);
        assert_eq!(call.args, [this], "SimD160: arguments");
    }

    fn offset(&mut self, this: u32, out: u32) -> [f32; 3] {
        let call = self.take(SIM_D860);
        assert_eq!(call.args, [this, out], "SimD860: arguments");
        [0, 4, 8].map(|o| self.mem.f32(out + o))
    }

    fn add_grid_edge(&mut self, grid: u32, vec: [f32; 4]) {
        let call = self.take(ADD_EDGE);
        assert_eq!(call.args[0], grid, "AddEdge: grid");
        let at = call.args[1];
        let window = call
            .windows
            .iter()
            .find(|w| w.1 == at)
            .unwrap_or_else(|| panic!("AddEdge: no window at the vector {at:#x}"));
        for (i, value) in vec.iter().enumerate() {
            let recorded = f32::from_le_bytes(window.3[4 * i..4 * i + 4].try_into().unwrap());
            assert!(
                value.to_bits() == recorded.to_bits() || (value.is_nan() && recorded.is_nan()),
                "AddEdge: vector lane {i}: {value} (port) vs {recorded} (original)"
            );
        }
    }
}

#[test]
fn sim_efe0_matches_original_calls() {
    let Some(calls) = load(VAR, SIM_EFE0) else {
        eprintln!("skipping SimEFE0: {VAR} not set");
        return;
    };
    let mut answered = 0;
    for call in &calls {
        let mem = map_windows(call);
        let this = call.args[0];
        // SimD860's output is the caller's scratch: the record's own address for it
        let d860 = call
            .inner
            .iter()
            .find(|i| i.addr == SIM_D860)
            .unwrap_or_else(|| panic!("call {}: no SimD860", call.index))
            .args[1];
        let frame = mem.efe0_frame(this, d860);
        let mut answers = Answers { mem, this, inner: &call.inner, next: 0 };
        hinge_refresh::refresh_hinge(&mut answers, &frame);
        assert_eq!(answers.next, call.inner.len(), "call {}: the port's calls", call.index);
        // The fourth lane of the SimD860 copy (+0x5ec) is the original's stack garbage, not ours (hinge_refresh's module doc).
        let body = call.windows.iter().find(|w| w.0 == 0 && w.1 == this).expect("the body's window");
        let garbage = u32::from_le_bytes(body.4[0x5EC..0x5F0].try_into().unwrap());
        answers.mem.set_u32(this + 0x5EC, garbage);
        check_windows(call, &answers.mem, "CHingedRigidBody::SimEFE0");
        answered += answers.next;
    }
    assert!(answered > 0, "no record reaches the port's calls");
    eprintln!("CHingedRigidBody::SimEFE0: {} calls match ({answered} calls answered from the record)", calls.len());
}

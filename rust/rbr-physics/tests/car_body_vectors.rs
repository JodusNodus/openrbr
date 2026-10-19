//! Replays the original's car-body calls (`harness/vector_specs/car_body.py`, recorded with
//! `harness/vcu_vectors.py --set car_body`) through the ports in `rbr_physics::car_body`. Needs record files:
//! `RBR_CAR_BODY_VECTORS=<file>[:<file>...]`. Without the variable the tests are skipped.

mod common;

use common::{
    check_window_list, check_windows, load, load_inner, map_window_list, map_windows, shared_windows, Call, Inner,
    Window,
};
use rbr_physics::car_body::accessors;
use rbr_physics::car_body::aero::{self, Sim7Calls};
use rbr_physics::car_body::apply_seed::{self, SeedCalls};
use rbr_physics::car_body::assist::{AssistCalls, RetireCalls};
use rbr_physics::car_body::body::{self, BodyCalls};
use rbr_physics::car_body::damage::{self, ResetDamageCalls, Vf14xCalls};
use rbr_physics::car_body::damper;
use rbr_physics::car_body::drag::{self, DragCalls};
use rbr_physics::car_body::e36::{self, Triple};
use rbr_physics::car_body::edge::{self, EdgeCalls};
use rbr_physics::car_body::emit::{self, Receive};
use rbr_physics::car_body::forward::{self, Vf24xCalls};
use rbr_physics::car_body::layout;
use rbr_physics::car_body::light::{self, LightCalls};
use rbr_physics::car_body::links::{self, LinkRef, Links, ListRef};
use rbr_physics::car_body::probe::{self, ProbeCalls, Sample};
use rbr_physics::car_body::sample9800::{self, Sample9800Calls};
use rbr_physics::car_body::steering_ffb::{self, FfbNormCalls};
use rbr_physics::car_body::step_engine::{self, StepEngineCalls};
use rbr_physics::car_body::store::CarStore;
use rbr_physics::car_body::transform::{self, ComposeA, PairWalk};
use rbr_physics::car_body::tweak;
use rbr_physics::car_body::update_9af0::{self, Update9AF0Calls};
use rbr_physics::car_body::update_mrr::{self, UpdateMrrCalls};
use rbr_physics::car_body::update_seed::{self, UpdateSeedCalls};
use rbr_physics::car_body::vf28x::{self, Vf28xCalls};
use rbr_physics::car_body::vf64x::{self, Vf64xCalls};
use rbr_physics::car_body::vf6cx::{self, Vf6cxCalls};
use rbr_physics::car_body::wheel::{self, WheelCalls};
use rbr_physics::car_body::xform_8130::{self, Xform8130Calls};
use rbr_physics::math::quat;
use rbr_physics::math::vec::{Vec3, Vec4};
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout as entity_layout;
use rbr_physics::rigid_body::emit::BodyEvent;
use rbr_physics::rigid_body::layout as solid_layout;

const VAR: &str = "RBR_CAR_BODY_VECTORS";
const SET_DIRTY: u32 = 0x52f450;
const PRE_STEP: u32 = 0x519950;
const PAIR_WALK: u32 = 0x51b560;
const DRAG_FORCE: u32 = 0x529340;
const SPIN_DAMPING: u32 = 0x529ea0;
const LENGTH: u32 = 0x473b30;
const SCALE: u32 = 0x473cd0;
const CROSS3: u32 = 0x43c630;
const UNLINK: u32 = 0x4e0020;
const S8C: u32 = 0x5293e0;
const READ_PACKET: u32 = 0x537f20;
const NOP_D570: u32 = 0x50d570;
const M5970: u32 = 0x505970;
const E19_IS_STATE2: u32 = 0x499900;
const EMIT_PROBLEM: u32 = 0x4e8a70;
const FLOW_CHECK: u32 = 0x5666b0;
const TO_WORLD: u32 = 0x4e68e0;
const SAMPLER: u32 = 0x4e54a0;

/// The recorded calls of `addr`, or None when the variable is unset (the test is skipped).
fn calls(addr: u32, name: &str) -> Option<Vec<Call>> {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return None;
    };
    assert!(!calls.is_empty(), "{name}: no calls in the vector files");
    Some(calls)
}

/// The original's only external call of a setter: `CBase::SetDirty` on `at`.
fn check_set_dirty_only(call: &Call, at: u32, name: &str) {
    assert_eq!(call.inner.len(), 1, "{name} call {}: external calls", call.index);
    assert_eq!(call.inner[0].addr, SET_DIRTY, "{name} call {}", call.index);
    assert_eq!(call.inner[0].args, [at], "{name} call {}", call.index);
}

/// The x87 result of a float function, as the record holds it (f64 bits of the value).
fn check_st0(call: &Call, value: f32, name: &str) {
    assert_eq!((value as f64).to_bits(), call.st0_bits, "{name} call {}: st0", call.index);
}

/// Two f32 words are the same value, or both NaN (the sign of a NaN is the platform's default NaN, not compared).
fn same_word(ours: u32, theirs: u32) -> bool {
    ours == theirs || (f32::from_bits(ours).is_nan() && f32::from_bits(theirs).is_nan())
}

/// Every window of `call` after the port ran, word by word, NaN-tolerantly (`same_word`): for the float calls.
fn check_windows_tolerant(call: &Call, mem: &Mem, name: &str) {
    for (k, at, len, _, post) in &call.windows {
        for (w, bytes) in post.chunks_exact(4).enumerate() {
            let theirs = u32::from_le_bytes(bytes.try_into().unwrap());
            let ours = mem.u32(*at + 4 * w as u32);
            assert!(
                same_word(ours, theirs),
                "{name} call {} window {k} word {w}: {ours:#010x} vs {theirs:#010x}",
                call.index
            );
        }
        assert_eq!(len % 4, 0, "{name}: window {k} is a whole number of words");
    }
}

#[test]
fn to_quat_matches_original_calls() {
    let Some(calls) = calls(0x4e4d10, "E34Quat::ToQuat") else {
        return;
    };
    for call in &calls {
        let mut mem: Mem = map_windows(call);
        let (this, out) = (call.args[0], call.args[1]);
        let q = quat::to_quat(&mem.load_mat3(this), rbr_physics::Variant::Stock102);
        // The out window is compared NaN-tolerantly (`same_word`); the matrix is not written.
        let (_, _, _, _, out_post) = &call.windows[1];
        let ours = [q.x, q.y, q.z, q.w].map(f32::to_bits);
        for (k, word) in ours.into_iter().enumerate() {
            let theirs = u32::from_le_bytes(out_post[4 * k..4 * k + 4].try_into().unwrap());
            assert!(
                same_word(word, theirs),
                "E34Quat::ToQuat call {} word {k}: {word:#010x} vs {theirs:#010x}",
                call.index
            );
            mem.set_u32(out + 4 * k as u32, word);
        }
        assert_eq!(call.eax, out, "E34Quat::ToQuat call {}: eax", call.index);
        let (_, at, len, _, post) = &call.windows[0];
        assert_eq!(mem.read(*at, *len), *post, "E34Quat::ToQuat call {} matrix", call.index);
    }
}

#[test]
fn update_ffb_torque_matches_original_calls() {
    let Some(calls) = calls(0x4e7ff0, "CCar::UpdateFfbTorque") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let cell = mem.ffb_torque(this);
        let new = steering_ffb::update(
            mem.t_override(this),
            mem.speed(this),
            mem.steering_rack_ratio(this),
            mem.steer_torque_sum(this).value,
            cell.value,
        );
        accessors::set_cell(cell, new).store(&mut mem, this + 0xae0);
        check_set_dirty_only(call, this + 0xae0, "CCar::UpdateFfbTorque");
        check_windows_tolerant(call, &mem, "CCar::UpdateFfbTorque");
    }
}

#[test]
fn accessors_match_original_calls() {
    if let Some(calls) = calls(0x4e6730, "CCar::GetA48") {
        for call in &calls {
            let mem = map_windows(call);
            let (this, i, neg) = (call.args[0], call.args[1], call.args[2]);
            let v = accessors::lane_value(mem.lane_a48(this, i), neg != 0);
            check_st0(call, v, "CCar::GetA48");
        }
    }
    if let Some(calls) = calls(0x4e65b0, "CCar::StateAtb") {
        for call in &calls {
            assert_eq!(call.eax, layout::state_address(call.args[0]), "CCar::StateAtb call {}", call.index);
        }
    }
    for (addr, name) in [(0x4e65e0, "CCar::GetB0C"), (0x4e65f0, "CCar::GetB1C")] {
        if let Some(calls) = calls(addr, name) {
            for call in &calls {
                let mem = map_windows(call);
                let at = call.args[0];
                let v = if addr == 0x4e65e0 { mem.target_b0c(at) } else { mem.rate_b1c(at) };
                check_st0(call, v, name);
            }
        }
    }
}

#[test]
fn dirty_setters_match_original_calls() {
    if let Some(calls) = calls(0x4e6660, "CCar::AddADC") {
        for call in &calls {
            let mut mem = map_windows(call);
            let (this, v) = (call.args[0], f32::from_bits(call.args[1]));
            let demand = mem.steer_torque_sum(this);
            accessors::add_steer_torque(demand, v).store(&mut mem, this + 0xad0);
            check_set_dirty_only(call, this + 0xad0, "CCar::AddADC");
            check_windows(call, &mem, "CCar::AddADC");
        }
    }
    if let Some(calls) = calls(0x4e66a0, "CCar::SetA1C") {
        for call in &calls {
            let mut mem = map_windows(call);
            let (this, i, j, v) = (call.args[0], call.args[1], call.args[2], f32::from_bits(call.args[3]));
            let cell = mem.matrix_cell(this, i, j);
            accessors::set_cell(cell, v).store(&mut mem, layout::cell_at(this, i, j));
            check_set_dirty_only(call, layout::cell_at(this, i, j), "CCar::SetA1C");
            check_windows(call, &mem, "CCar::SetA1C");
        }
    }
}

/// Answers `ComposeA` from its record: the call must be the next recorded one (this, a, t, out). Its effect (the walk) is
/// not computed here: the test writes the recorded output of `out` (the outer window), and the walk's port checks it.
struct RecordedCompose<'a> {
    call: &'a Call,
    next: usize,
}

impl ComposeA for RecordedCompose<'_> {
    fn compose_a(&mut self, this: u32, a: u32, t: u32, out: u32) -> u32 {
        let inner = self.call.inner.iter().filter(|c| c.addr == 0x51B4C0).nth(self.next);
        let inner = inner.expect("the port called ComposeA more often than the original");
        assert_eq!(inner.args, [this, a, t, out], "ComposeA call {}: arguments", self.call.index);
        self.next += 1;
        out
    }
}

#[test]
fn xform_6870_matches_original_calls() {
    let Some(calls) = calls(0x4e6870, "CCar::Xform6870") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let (this, a, b) = (call.args[0], call.args[1], call.args[2]);
        let body = mem.body(this);
        // The identity S30 calls: the body's, then the car's (both return their `this`).
        let identity: Vec<&common::Inner> = call.inner.iter().filter(|c| c.addr == 0x519C00).collect();
        assert_eq!(identity.len(), 2, "Xform6870 call {}: S30 calls", call.index);
        assert_eq!(identity[0].args, [body], "Xform6870 call {}: body S30", call.index);
        assert_eq!(identity[1].args, [this], "Xform6870 call {}: car S30", call.index);
        // The output's effect is the recorded one (answered); the port's return value is b.
        let (_, _, _, _, out_post) = &call.windows[2];
        let mut compose = RecordedCompose { call, next: 0 };
        let eax = transform::xform_6870(&mut compose, this, body, a, b);
        assert_eq!(eax, call.eax, "Xform6870 call {}: eax", call.index);
        assert_eq!(compose.next, 1, "Xform6870 call {}: ComposeA calls", call.index);
        mem.map(b, out_post);
        check_windows(call, &mem, "CCar::Xform6870");
    }
}

/// The window `k` of an inner call: `k` counts the spec's `windows` in order (not the argument index).
fn inner_window(inner: &Inner, k: usize) -> &Window {
    inner.windows.iter().find(|w| w.0 == k).unwrap_or_else(|| panic!("inner call {:#x}: no window {k}", inner.addr))
}

/// Writes window `k` of an inner call as it was after the call, at `at`: the external's effect on the car's memory.
fn write_output(mem: &mut Mem, inner: &Inner, k: usize, at: u32) {
    let (_, _, _, _, output) = inner_window(inner, k);
    for (j, bytes) in output.chunks_exact(4).enumerate() {
        mem.set_u32(at + 4 * j as u32, u32::from_le_bytes(bytes.try_into().unwrap()));
    }
}

/// The first three lanes of a recorded vector, each checked against `ours` (the fourth lane is not modelled).
fn check_lanes(name: &str, index: usize, ours: Vec3, theirs: &[u8]) {
    let lanes = [ours.x, ours.y, ours.z];
    for (k, bytes) in theirs.chunks_exact(4).take(3).enumerate() {
        let theirs = u32::from_le_bytes(bytes.try_into().unwrap());
        assert!(
            same_word(lanes[k].to_bits(), theirs),
            "{name} call {index} lane {k}: {:#010x} vs {theirs:#010x}",
            lanes[k].to_bits()
        );
    }
}

/// The first three lanes of a recorded vector.
fn vec3_of(output: &[u8]) -> Vec3 {
    let lane = |k: usize| f32::from_le_bytes(output[4 * k..4 * k + 4].try_into().unwrap());
    Vec3 { x: lane(0), y: lane(1), z: lane(2) }
}

/// Answers `DragAndSpinDamp`'s externals from the record of one call. The pre-step's effect is the car window's
/// recorded bytes; the walks return the recorded output, and their input and the accumulator calls' vectors are
/// checked against the record.
struct RecordedDrag<'a> {
    call: &'a Call,
    pre_steps: usize,
    walks: usize,
    forces: usize,
    dampings: usize,
}

impl<'a> RecordedDrag<'a> {
    /// The `n`th inner call to `addr` of the record.
    fn nth_inner(&self, addr: u32, n: usize) -> &'a Inner {
        let call: &'a Call = self.call;
        call.inner
            .iter()
            .filter(|c| c.addr == addr)
            .nth(n)
            .unwrap_or_else(|| panic!("DragAndSpinDamp call {}: inner {addr:#x} {n}", call.index))
    }
}

impl DragCalls for RecordedDrag<'_> {
    fn pre_step(&mut self, mem: &mut Mem, car: u32) {
        let inner = self.nth_inner(PRE_STEP, self.pre_steps);
        assert_eq!(inner.args, [car], "DragAndSpinDamp call {}: S00", self.call.index);
        let (_, at, _, _, post) = &self.call.windows[0];
        assert_eq!(*at, car, "DragAndSpinDamp: window 0 is the car");
        for (k, bytes) in post.chunks_exact(4).enumerate() {
            mem.set_u32(car + 4 * k as u32, u32::from_le_bytes(bytes.try_into().unwrap()));
        }
        self.pre_steps += 1;
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        let inner = self.nth_inner(PAIR_WALK, self.walks);
        assert_eq!(inner.args[2], car, "DragAndSpinDamp call {}: walk {}", self.call.index, self.walks);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("DragAndSpinDamp walk input", self.call.index, a, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        self.walks += 1;
        vec3_of(output)
    }

    fn drag_force(&mut self, _mem: &mut Mem, car: u32, force: Vec3) {
        let inner = self.nth_inner(DRAG_FORCE, self.forces);
        assert_eq!(inner.args[0], car, "DragAndSpinDamp call {}: S70 this", self.call.index);
        let (_, _, _, vector, _) = inner_window(inner, 0);
        check_lanes("DragAndSpinDamp S70", self.call.index, force, vector);
        self.forces += 1;
    }

    fn spin_damping(&mut self, _mem: &mut Mem, car: u32, damping: Vec3) {
        let inner = self.nth_inner(SPIN_DAMPING, self.dampings);
        assert_eq!(inner.args[0], car, "DragAndSpinDamp call {}: S74 this", self.call.index);
        let (_, _, _, vector, _) = inner_window(inner, 0);
        check_lanes("DragAndSpinDamp S74", self.call.index, damping, vector);
        self.dampings += 1;
    }
}

#[test]
fn drag_and_spin_damp_matches_original_calls() {
    let Some(calls) = calls(0x4e7dc0, "CCar::DragAndSpinDamp") else {
        return;
    };
    let (mut gated, mut over_limit) = (0, 0);
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedDrag { call, pre_steps: 0, walks: 0, forces: 0, dampings: 0 };
        drag::drag_and_spin_damp(&mut answers, &mut mem, car, drag::DRAG_FACTOR, rbr_physics::Variant::Stock102);
        // The gate's S00 is the first record. Later S00 records (before S70 and S74 when the gate is on) are not in the
        // asm: the getter has no effect, so they are not answered (see `car_body/README.md` (git 9c28c0b8)).
        let recorded_gate = usize::from(call.inner.first().is_some_and(|c| c.addr == PRE_STEP));
        assert_eq!(answers.pre_steps, recorded_gate, "DragAndSpinDamp call {}: gate", call.index);
        assert_eq!((answers.walks, answers.forces, answers.dampings), (2, 1, 1), "DragAndSpinDamp call {}", call.index);
        check_windows(call, &mem, "CCar::DragAndSpinDamp");
        gated += answers.pre_steps;
        // The second Length is the walked spin's speed: the record's own st0.
        let spin_speed = call.inner.iter().filter(|c| c.addr == LENGTH).nth(1).unwrap();
        if f64::from_bits(spin_speed.st0_bits) > 5.0 {
            over_limit += 1;
        }
    }
    eprintln!("CCar::DragAndSpinDamp: {} calls, {gated} with the gate, {over_limit} over the spin limit", calls.len());
}

#[test]
fn append_a980_matches_original_calls() {
    let Some(calls) = calls(0x4ea980, "CCar::AppendA980") else {
        return;
    };
    let (mut unlinked, mut free) = (0, 0);
    for call in &calls {
        let mut mem = map_windows(call);
        let head = ListRef::new(call.args[0]);
        let a = LinkRef::new(call.args[1]);
        let was_linked = mem.link(a).owner.is_some();
        links::append(&mut mem, head, a);
        // The original unlinks `a` exactly when it has an owner (the `decl` and `Unlink` branch).
        let recorded_unlinks = call.inner.iter().filter(|c| c.addr == UNLINK).count();
        assert_eq!(recorded_unlinks, usize::from(was_linked), "AppendA980 call {}: Unlink calls", call.index);
        if was_linked {
            unlinked += 1;
        } else {
            free += 1;
        }
        // The head window holds both lists and every link the call touches.
        check_windows(call, &mem, "CCar::AppendA980");
    }
    eprintln!("CCar::AppendA980: {} calls, {unlinked} unlinking, {free} free links", calls.len());
}

/// Answers `MotionProbe`'s externals from the record of one call: the gate and the flow check are the words the original
/// returned; `ToWorld`'s input is checked by value and its output is the recorded one; the sampler's result and verdict
/// words are the recorded ones.
struct RecordedProbe<'a> {
    call: &'a Call,
}

impl RecordedProbe<'_> {
    /// The first inner call to `addr` of the record, which must have been made with `this` = `car`.
    fn inner(&self, addr: u32, car: u32) -> &Inner {
        let inner = self
            .call
            .inner
            .iter()
            .find(|c| c.addr == addr)
            .unwrap_or_else(|| panic!("MotionProbe call {}: no inner {addr:#x}", self.call.index));
        assert_eq!(inner.args[0], car, "MotionProbe call {}: {addr:#x} this", self.call.index);
        inner
    }
}

impl ProbeCalls for RecordedProbe<'_> {
    fn s8c(&mut self, car: u32) -> u32 {
        self.inner(S8C, car).eax
    }

    fn m5666b0(&mut self) -> u32 {
        self.call
            .inner
            .iter()
            .find(|c| c.addr == FLOW_CHECK)
            .map_or_else(|| panic!("MotionProbe call {}: no M5666b0", self.call.index), |c| c.eax)
    }

    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3 {
        let inner = self.inner(TO_WORLD, car);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("MotionProbe ToWorld input", self.call.index, v, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        vec3_of(output)
    }

    fn sampler(&mut self, car: u32) -> Sample {
        let inner = self.inner(SAMPLER, car);
        let (_, _, _, _, big) = inner_window(inner, 0);
        let word = |at: usize| u32::from_le_bytes(big[at..at + 4].try_into().unwrap());
        Sample { found: inner.eax, first: word(0x44), second: word(0x48) }
    }
}

#[test]
fn motion_probe_matches_original_calls() {
    let Some(calls) = calls(0x4e7730, "CCar::MotionProbe") else {
        return;
    };
    let mut seen = std::collections::BTreeMap::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let (car, out) = (call.args[0], call.args[1]);
        let outcome = probe::motion_probe(&mut RecordedProbe { call }, &mem, car);
        assert_eq!(outcome.ret(), call.eax, "MotionProbe call {}: return", call.index);
        mem.set_u32(out, outcome.out());
        check_windows(call, &mem, "CCar::MotionProbe");
        *seen.entry(format!("{outcome:?}")).or_insert(0) += 1;
    }
    eprintln!("CCar::MotionProbe: {} calls, outcomes {seen:?}", calls.len());
}

/// The windows after `Sim7AD0`, word by word, except the car's accumulators: S70 and S74 add into +0x370 and +0x310
/// (the externals' effect, not the port's), and the seed the port reads is checked as ToWorld's input.
fn check_windows_outside_accumulators(call: &Call, mem: &Mem, name: &str) {
    let car = call.args[0];
    for (k, at, _, _, post) in &call.windows {
        for (w, bytes) in post.chunks_exact(4).enumerate() {
            let addr = *at + 4 * w as u32;
            let accumulator = (car + 0x310..car + 0x320).contains(&addr) || (car + 0x370..car + 0x380).contains(&addr);
            if accumulator {
                continue;
            }
            let theirs = u32::from_le_bytes(bytes.try_into().unwrap());
            assert_eq!(mem.u32(addr), theirs, "{name} call {} window {k} word {w}", call.index);
        }
    }
}

/// The records of `addr` after ToWorld's record. The inner records are in return order, so ToWorld's own nested calls
/// (its S30 identity and an MB560 walk) come before its record; `Sim7AD0`'s calls follow it.
fn after_to_world(call: &Call, addr: u32) -> impl Iterator<Item = &Inner> {
    call.inner.iter().skip_while(|c| c.addr != TO_WORLD).skip(1).filter(move |c| c.addr == addr)
}

/// Answers `Sim7AD0`'s externals from the record of one call: ToWorld (its input is checked), the two MB560 walks and
/// the S70 and S74 sends (their vectors are checked against the record).
struct RecordedAero<'a> {
    call: &'a Call,
    seeds: usize,
    walks: usize,
    forces: usize,
    spins: usize,
}

impl<'a> RecordedAero<'a> {
    /// The `n`th call to `addr` made by `Sim7AD0` itself.
    fn nth_direct(&self, addr: u32, n: usize) -> &'a Inner {
        after_to_world(self.call, addr)
            .nth(n)
            .unwrap_or_else(|| panic!("Sim7AD0 call {}: inner {addr:#x} {n}", self.call.index))
    }
}

impl Sim7Calls for RecordedAero<'_> {
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3 {
        let inner = self
            .call
            .inner
            .iter()
            .find(|c| c.addr == TO_WORLD)
            .unwrap_or_else(|| panic!("Sim7AD0 call {}: no ToWorld", self.call.index));
        assert_eq!(inner.args[0], car, "Sim7AD0 call {}: ToWorld this", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("Sim7AD0 ToWorld input", self.call.index, v, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        self.seeds += 1;
        vec3_of(output)
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        let inner = self.nth_direct(PAIR_WALK, self.walks);
        assert_eq!(inner.args[2], car, "Sim7AD0 call {}: walk {}", self.call.index, self.walks);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("Sim7AD0 walk input", self.call.index, a, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        self.walks += 1;
        vec3_of(output)
    }

    fn aero_force(&mut self, _mem: &mut Mem, car: u32, force: Vec3) {
        let inner = self.nth_direct(DRAG_FORCE, self.forces);
        assert_eq!(inner.args[0], car, "Sim7AD0 call {}: S70 this", self.call.index);
        let (_, _, _, vector, _) = inner_window(inner, 0);
        check_lanes("Sim7AD0 S70", self.call.index, force, vector);
        self.forces += 1;
    }

    fn aero_spin(&mut self, _mem: &mut Mem, car: u32, spin: Vec3) {
        let inner = self.nth_direct(SPIN_DAMPING, self.spins);
        assert_eq!(inner.args[0], car, "Sim7AD0 call {}: S74 this", self.call.index);
        let (_, _, _, vector, _) = inner_window(inner, 0);
        check_lanes("Sim7AD0 S74", self.call.index, spin, vector);
        self.spins += 1;
    }
}

/// `CCar::Sim7AD0` against its records. The port's externals must be exactly the record's (the gust returns before
/// ToWorld, before the walks, or before the sends as the record does), and the car's windows must be unchanged.
#[test]
fn sim7ad0_matches_original_calls() {
    let Some(calls) = calls(0x4e7ad0, "CCar::Sim7AD0") else {
        return;
    };
    let (mut sent, mut after_seed, mut before_seed) = (0, 0, 0);
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedAero { call, seeds: 0, walks: 0, forces: 0, spins: 0 };
        aero::sim7ad0(&mut answers, &mut mem, car);
        assert_eq!(
            (answers.seeds, answers.walks, answers.forces, answers.spins),
            (
                call.inner.iter().filter(|c| c.addr == TO_WORLD).count(),
                after_to_world(call, PAIR_WALK).count(),
                after_to_world(call, DRAG_FORCE).count(),
                after_to_world(call, SPIN_DAMPING).count(),
            ),
            "Sim7AD0 call {}: externals",
            call.index
        );
        check_windows_outside_accumulators(call, &mem, "CCar::Sim7AD0");
        if answers.forces > 0 {
            sent += 1;
        } else if answers.seeds > 0 {
            after_seed += 1;
        } else {
            before_seed += 1;
        }
    }
    eprintln!(
        "CCar::Sim7AD0: {} calls, {sent} sent, {after_seed} returned after ToWorld, {before_seed} before it",
        calls.len()
    );
}

/// `CCar::Damp66D0` against its records: the two SetDirty calls (on each CReal) and the car's windows after the port.
#[test]
fn damp_66d0_matches_original_calls() {
    let Some(calls) = calls(0x4e66d0, "CCar::Damp66D0") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        damper::update_dampers(&mut mem, car);
        let dirtied: Vec<u32> = call.inner.iter().map(|c| c.args[0]).collect();
        assert_eq!(dirtied, [0, 1].map(|i| layout::damper_real_at(car, i)), "Damp66D0 call {}: SetDirty", call.index);
        assert!(call.inner.iter().all(|c| c.addr == SET_DIRTY));
        check_windows_tolerant(call, &mem, "CCar::Damp66D0");
    }
    eprintln!("CCar::Damp66D0: {} calls", calls.len());
}

/// Answers `Update9AF0`'s externals from its records, in call order. A callee's own nested records come before its
/// record (the records are in return order), so each answer skips to its record (`until`).
struct RecordedUpdate<'a> {
    call: &'a Call,
    next: usize,
}

impl<'a> RecordedUpdate<'a> {
    /// The next record of `addr` at or after the cursor; the records skipped are the callee's nested calls.
    fn until(&mut self, addr: u32) -> &'a Inner {
        record_after(self.call, &mut self.next, addr)
    }

    /// The emit at `0x4e88f0` that follows the cursor, if any (the probe's answer is read from it, see `probe`).
    fn emit_after_cursor(&self) -> Option<&'a Inner> {
        let call: &'a Call = self.call;
        call.inner[self.next..].iter().find(|c| c.addr == EMIT_8F0)
    }
}

const TIMER: u32 = 0x4a0290;
const FLOW_CHECK_566640: u32 = 0x566640;
const EMIT_DF0: u32 = 0x4e8df0;
const EMIT_770: u32 = 0x4e8770;
const EMIT_8F0: u32 = 0x4e88f0;
const TEST_3B20: u32 = 0x4ec0d0;
const TEST_4B0: u32 = 0x504840;
const TEST_690: u32 = 0x50ecf0;

impl Update9AF0Calls for RecordedUpdate<'_> {
    fn timer_gap(&mut self, obj: u32, at: u32) -> f64 {
        let inner = self.until(TIMER);
        assert_eq!(inner.args, [obj, at], "Update9AF0 call {}: timer", self.call.index);
        f64::from_bits(inner.st0_bits)
    }

    fn flow_check(&mut self, obj: u32) -> u32 {
        let inner = self.until(FLOW_CHECK_566640);
        assert_eq!(inner.args[0], obj, "Update9AF0 call {}: flow", self.call.index);
        inner.eax
    }

    fn s8c(&mut self, car: u32) -> u32 {
        let inner = self.until(S8C);
        assert_eq!(inner.args[0], car, "Update9AF0 call {}: S8C", self.call.index);
        inner.eax
    }

    fn emit_df0(&mut self, car: u32, words: [u32; 3]) {
        let inner = self.until(EMIT_DF0);
        assert_eq!(inner.args, [car, words[0], words[1], words[2]], "Update9AF0 call {}: 4e8df0", self.call.index);
    }

    fn emit_770(&mut self, obj: u32, code: u32) {
        let inner = self.until(EMIT_770);
        assert_eq!(inner.args[0], obj, "Update9AF0 call {}: 4e8770 this", self.call.index);
        let (_, _, _, pre, _) = inner_window(inner, 0);
        assert_eq!(
            u32::from_le_bytes(pre[0..4].try_into().unwrap()),
            code,
            "Update9AF0 call {}: 4e8770 code",
            self.call.index
        );
    }

    fn test_3b20(&mut self, obj: u32) -> u32 {
        let inner = self.until(TEST_3B20);
        assert_eq!(inner.args[0], obj, "Update9AF0 call {}: 4ec0d0", self.call.index);
        inner.eax
    }

    fn test_4b0(&mut self, obj: u32) -> u32 {
        let inner = self.until(TEST_4B0);
        assert_eq!(inner.args[0], obj, "Update9AF0 call {}: 504840", self.call.index);
        inner.eax
    }

    fn test_690(&mut self, obj: u32) -> u32 {
        let inner = self.until(TEST_690);
        assert_eq!(inner.args[0], obj, "Update9AF0 call {}: 50ecf0", self.call.index);
        inner.eax
    }

    /// MotionProbe's own record is not kept (the spec is not external, so its answer is not in the records). The
    /// `(ret, out)` pair is read back from the emit that follows it: code 0 (ret 1), 3 (out 0) or 4 (out 3); no
    /// emit means `out` 4 (the early return). MotionProbe's vectors are checked in `motion_probe_matches_original_calls`.
    fn motion_probe(&mut self, car: u32) -> (u32, u32) {
        let _ = car;
        let Some(emit) = self.emit_after_cursor() else {
            return (0, 4);
        };
        let (_, _, _, pre, _) = inner_window(emit, 0);
        match u32::from_le_bytes(pre[4..8].try_into().unwrap()) {
            0 => (1, 0),
            4 => (0, 3),
            _ => (0, 0),
        }
    }

    fn emit_8f0(&mut self, obj: u32, pair: [u32; 2]) {
        let inner = self.until(EMIT_8F0);
        assert_eq!(inner.args[0], obj, "Update9AF0 call {}: 4e88f0 this", self.call.index);
        let (_, _, _, pre, _) = inner_window(inner, 0);
        let words = [0, 4].map(|at| u32::from_le_bytes(pre[at..at + 4].try_into().unwrap()));
        assert_eq!(words, pair, "Update9AF0 call {}: 4e88f0 pair", self.call.index);
    }
}

/// `CCar::Update9AF0` against its records: the timer, the flow and gate answers, the tests, the emits; the car's
/// windows after the port are checked (the emits' own writes are not in them, so the car's own words only).
#[test]
fn update_9af0_matches_original_calls() {
    let Some(calls) = calls(0x4e9af0, "CCar::Update9AF0") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedUpdate { call, next: 0 };
        update_9af0::emit_signals(&mut answers, &mut mem, car, call.args[1]);
        assert_eq!(answers.next, call.inner.len(), "Update9AF0 call {}: records left", call.index);
        // The externals write other car words (not modelled here); the port's own write is the flag at +0x9ec.
        let (_, _, _, _, post) = &call.windows[0];
        let recorded = u32::from_le_bytes(post[0x9EC..0x9F0].try_into().unwrap());
        assert_eq!(mem.u32(car + 0x9EC), recorded, "Update9AF0 call {}: +0x9ec", call.index);
    }
    eprintln!("CCar::Update9AF0: {} calls", calls.len());
}

/// The next record of `addr` at or after `*next`: the records skipped are the nested calls of the callees the port
/// answers earlier, which the original logs first (the records are in return order).
fn next_record<'a>(call: &'a Call, next: &mut usize, addr: u32) -> &'a Inner {
    let (at, inner) = call.inner[*next..]
        .iter()
        .enumerate()
        .find(|(_, c)| c.addr == addr)
        .unwrap_or_else(|| panic!("call {}: no {addr:#x} record", call.index));
    *next += at + 1;
    inner
}

/// Like `next_record`, for the first record of `addr` after the cursor that `matches` accepts.
fn next_record_if<'a>(call: &'a Call, next: &mut usize, addr: u32, matches: impl Fn(&Inner) -> bool) -> &'a Inner {
    let (at, inner) = call.inner[*next..]
        .iter()
        .enumerate()
        .find(|(_, c)| c.addr == addr && matches(c))
        .unwrap_or_else(|| panic!("call {}: no {addr:#x} record", call.index));
    *next += at + 1;
    inner
}

/// Answers `Vf64x`'s drag, aero gust and own externals from one call's records, in call order.
struct RecordedVf64x<'a> {
    call: &'a Call,
    next: usize,
    /// The car's vector at +0x380 as `e52cb20` left it (its record's output): the walk's input.
    car_380: Vec<u8>,
}

impl<'a> RecordedVf64x<'a> {
    fn record(&mut self, addr: u32) -> &'a Inner {
        let call: &'a Call = self.call;
        next_record(call, &mut self.next, addr)
    }

    /// Like `record`, but skips earlier records of `addr` that `matches` rejects (other objects' calls).
    fn record_if(&mut self, addr: u32, matches: impl Fn(&Inner) -> bool) -> &'a Inner {
        let call: &'a Call = self.call;
        next_record_if(call, &mut self.next, addr, matches)
    }

    /// An MB560 walk of `a` on the car (both `DragCalls` and `Sim7Calls` walk this way).
    fn walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        let inner = self.record(PAIR_WALK);
        assert_eq!(inner.args[2], car, "Vf64x call {}: walk", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("Vf64x walk input", self.call.index, a, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        vec3_of(output)
    }

    /// An S70 or S74 send of `v` on the car, which leaves the car's accumulator at `offset` as recorded.
    fn send(&mut self, mem: &mut Mem, addr: u32, car: u32, v: Vec3, offset: u32) {
        let inner = self.record(addr);
        assert_eq!(inner.args[0], car, "Vf64x call {}: send {addr:#x}", self.call.index);
        let (_, _, _, vector, _) = inner_window(inner, 0);
        check_lanes("Vf64x send", self.call.index, v, vector);
        write_output(mem, inner, 1, car + offset);
    }
}

impl DragCalls for RecordedVf64x<'_> {
    fn pre_step(&mut self, _mem: &mut Mem, car: u32) {
        let inner = self.record(PRE_STEP);
        assert_eq!(inner.args, [car], "Vf64x call {}: S00", self.call.index);
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        self.walk(a, car)
    }

    fn drag_force(&mut self, mem: &mut Mem, car: u32, force: Vec3) {
        self.send(mem, DRAG_FORCE, car, force, 0x370);
    }

    fn spin_damping(&mut self, mem: &mut Mem, car: u32, damping: Vec3) {
        self.send(mem, SPIN_DAMPING, car, damping, 0x310);
    }
}

impl Sim7Calls for RecordedVf64x<'_> {
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3 {
        let inner = self.record(TO_WORLD);
        assert_eq!(inner.args[0], car, "Vf64x call {}: ToWorld", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("Vf64x ToWorld input", self.call.index, v, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        vec3_of(output)
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        self.walk(a, car)
    }

    fn aero_force(&mut self, mem: &mut Mem, car: u32, force: Vec3) {
        self.send(mem, DRAG_FORCE, car, force, 0x370);
    }

    fn aero_spin(&mut self, mem: &mut Mem, car: u32, spin: Vec3) {
        self.send(mem, SPIN_DAMPING, car, spin, 0x310);
    }
}

impl Vf64xCalls for RecordedVf64x<'_> {
    fn e5224f0(&mut self, obj: u32) {
        let inner = self.record(0x5224F0);
        assert_eq!(inner.args, [obj], "Vf64x call {}: 5224f0", self.call.index);
    }

    /// `0x52cb20` also runs on other objects before the car's own call, so the car is matched by its argument. Its
    /// recorded output is the car's vector at +0x380, which the walk reads next.
    fn e52cb20(&mut self, mem: &mut Mem, car: u32) {
        let inner = self.record_if(0x52CB20, |c| c.args[0] == car);
        write_output(mem, inner, 0, car + 0x380);
        let (_, _, _, _, output) = inner_window(inner, 0);
        self.car_380 = output.to_vec();
    }

    /// The walk of the car's vector at +0x380: its input is the vector `e52cb20` left (all four lanes), and the output
    /// is the record's.
    fn walk_380(&mut self, car: u32) -> Vec4 {
        let inner = self.record(PAIR_WALK);
        assert_eq!(inner.args[2], car, "Vf64x call {}: walk 380", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        assert_eq!(input, self.car_380.as_slice(), "Vf64x call {}: walk 380 input", self.call.index);
        let (_, _, _, _, output) = inner_window(inner, 1);
        vec4_of(output)
    }

    fn e495090(&mut self, obj: u32, _first: f32, _second: f32) {
        let inner = self.record(0x495090);
        assert_eq!(inner.args[0], obj, "Vf64x call {}: 495090 this", self.call.index);
    }

    fn e521920(&mut self, obj: u32) -> u32 {
        let inner = self.record(0x521920);
        assert_eq!(inner.args, [obj], "Vf64x call {}: 521920", self.call.index);
        inner.eax
    }

    fn e521740(&mut self, obj: u32) {
        let inner = self.record(0x521740);
        assert_eq!(inner.args, [obj], "Vf64x call {}: 521740", self.call.index);
    }
}

/// `CCar::Vf64x` against its records: its drag and aero gust (their sends and walks are the records'), its own walks
/// and sends, and the car's blended vector at +0x15d0 (lanes 0 to 2, the port's own write).
#[test]
fn vf64x_matches_original_calls() {
    let Some(calls) = calls(0x4e9020, "CCar::Vf64x") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedVf64x { call, next: 0, car_380: Vec::new() };
        vf64x::step_aero(
            &mut answers,
            &mut mem,
            car,
            rbr_physics::car_body::drag::DRAG_FACTOR,
            rbr_physics::Variant::Stock102,
        );
        let (_, _, _, _, post) = &call.windows[0];
        for k in 0..4 {
            let theirs = u32::from_le_bytes(post[0x15D0 + 4 * k..0x15D4 + 4 * k].try_into().unwrap());
            let ours = mem.u32(car + 0x15D0 + 4 * k as u32);
            assert!(
                same_word(ours, theirs),
                "Vf64x call {}: +0x15d0 lane {k}: {ours:#010x} vs {theirs:#010x}",
                call.index
            );
        }
    }
    eprintln!("CCar::Vf64x: {} calls", calls.len());
}

/// `StepEngine`'s externals (the scratch sim, drive, entity, `UpdateFfbTorque` and `Snap9360`) are not answered: their
/// effects on the car are not checked here, only the cells `StepEngine` writes and its `SetDirty` calls.
struct RecordedStep;

impl StepEngineCalls for RecordedStep {
    fn scratch_step(&mut self, _mem: &mut Mem, _car: u32, _dt: f32) {}

    fn drive_step(&mut self, _mem: &mut Mem, _car: u32, _dt: f32) {}

    fn entity_vf58(&mut self, _mem: &mut Mem, _car: u32, _dt: f32) {}

    fn update_ffb_torque(&mut self, _mem: &mut Mem, _car: u32) {}

    fn snap_9360(&mut self, _mem: &mut Mem, _car: u32) {}
}

#[test]
fn step_engine_matches_original_calls() {
    let Some(calls) = calls(0x4e9930, "CCar::StepEngine") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let dt = f32::from_bits(call.args[1]);
        step_engine::step_engine(&mut RecordedStep, &mut mem, car, dt);
        // The four cells are dirtied in this order; the externals' own nested SetDirty calls come after them.
        let mut next = 0;
        for offset in [0xAF0, 0xB00, 0xB10, 0xAD0] {
            next_record_if(call, &mut next, SET_DIRTY, |c| c.args[0] == car + offset);
        }
        // The demand's value is not checked: the externals add to it after this function clears it (`AddADC`).
        let (_, _, _, _, post) = &call.windows[0];
        for offset in [0xAF8, 0xAFC, 0xB08, 0xB0C, 0xB18, 0xB1C, 0xAD8] {
            let theirs = u32::from_le_bytes(post[offset..offset + 4].try_into().unwrap());
            let ours = mem.u32(car + offset as u32);
            assert!(
                same_word(ours, theirs),
                "StepEngine call {}: +{offset:#x}: {ours:#010x} vs {theirs:#010x}",
                call.index
            );
        }
    }
    eprintln!("CCar::StepEngine: {} calls", calls.len());
}

/// Writes the words of window `k` that the external changed (post differs from pre) at the window's address, so the
/// words the port wrote before the call stay as they are.
fn apply_changes(mem: &mut Mem, inner: &Inner, k: usize) {
    let (_, at, _, pre, post) = inner_window(inner, k);
    for (j, (before, after)) in pre.chunks_exact(4).zip(post.chunks_exact(4)).enumerate() {
        if before != after {
            mem.set_u32(*at + 4 * j as u32, u32::from_le_bytes(after.try_into().unwrap()));
        }
    }
}

/// Applies the wheel's flag words an external wrote (its node window 0, absent when the flag pointer is null).
fn apply_flag_changes(mem: &mut Mem, inner: &Inner) {
    if inner.windows.iter().any(|w| w.0 == 0) {
        apply_changes(mem, inner, 0);
    }
}

/// Checks the bytes an external received (window `k`, before the call) against the port's values.
fn check_input(name: &str, index: usize, k: usize, inner: &Inner, ours: &[u8]) {
    let (_, _, _, pre, _) = inner_window(inner, k);
    assert_eq!(pre[..ours.len()], *ours, "Vf6cx call {index}: {name} payload");
}

/// Answers `Vf6cx`'s externals from one call's records, in call order. The wheels and bodies answer with the record's eax.
struct RecordedVf6cx<'a> {
    call: &'a Call,
    next: usize,
}

impl<'a> RecordedVf6cx<'a> {
    fn record(&mut self, addr: u32) -> &'a Inner {
        let call: &'a Call = self.call;
        next_record(call, &mut self.next, addr)
    }

    /// The next record of `addr` whose `this` (its first argument) is `this`: nested calls of the same function (a
    /// wheel's S80 inside the wheel loop, say) are listed before the caller and must be skipped.
    fn record_of(&mut self, addr: u32, this: u32) -> &'a Inner {
        let call: &'a Call = self.call;
        next_record_if(call, &mut self.next, addr, |c| c.args[0] == this)
    }

    fn walk_of(&mut self, a: Vec3, car: u32) -> Vec3 {
        let inner = self.record(PAIR_WALK);
        assert_eq!(inner.args[2], car, "Vf6cx call {}: walk", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes("Vf6cx walk input", self.call.index, a, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        let lane = |k: usize| f32::from_le_bytes(output[4 * k..4 * k + 4].try_into().unwrap());
        Vec3 { x: lane(0), y: lane(1), z: lane(2) }
    }

    fn check_dt(&self, name: &str, inner: &Inner, dt: f32) {
        assert_eq!(inner.args[1], dt.to_bits(), "Vf6cx call {}: {name} dt", self.call.index);
    }
}

impl Vf6cxCalls for RecordedVf6cx<'_> {
    fn passenger_step(&mut self, _mem: &mut Mem, _passenger: u32, dt: f32) {
        let inner = self.record(0x523160);
        self.check_dt("passenger", inner, dt);
    }

    fn scratch_nop_a(&mut self, _mem: &mut Mem, _scratch: u32, dt: f32) {
        let inner = self.record(0x536F70);
        self.check_dt("NopA", inner, dt);
    }

    fn drive_nop3b0(&mut self, _mem: &mut Mem, _drive: u32, dt: f32) {
        let inner = self.record(0x5043B0);
        self.check_dt("Nop3B0", inner, dt);
    }

    fn solid_vf6c(&mut self, mem: &mut Mem, car: u32, dt: f32) {
        let inner = self.record_of(0x52CC30, car);
        self.check_dt("E43Vf6C", inner, dt);
        apply_changes(mem, inner, 0);
    }

    fn raycast(&mut self, _mem: &mut Mem, car: u32, out: &mut [u8; 28]) -> u32 {
        let inner = self.record(0x4E4960);
        assert_eq!(inner.args[1], car + 0x140, "Vf6cx call {}: raycast position", self.call.index);
        let (_, _, _, _, post) = inner_window(inner, 0);
        out.copy_from_slice(post);
        inner.eax
    }

    fn impulse(&mut self, mem: &mut Mem, _car: u32, out: &mut [u8; 28], f: f32, p: u32, dt: f32) {
        let inner = self.record(0x529640);
        assert_eq!(inner.args[2], f.to_bits(), "Vf6cx call {}: impulse f", self.call.index);
        assert_eq!(inner.args[3], p, "Vf6cx call {}: impulse p", self.call.index);
        assert_eq!(inner.args[4], dt.to_bits(), "Vf6cx call {}: impulse dt", self.call.index);
        apply_changes(mem, inner, 0);
        let (_, _, _, _, post) = inner_window(inner, 1);
        out.copy_from_slice(post);
    }

    fn emit_brake_wall(&mut self, _mem: &mut Mem, out: &[u8; 28]) {
        let inner = self.record(0x4E4490);
        check_input("brake-wall emit", self.call.index, 0, inner, &out[..16]);
    }

    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3 {
        self.walk_of(a, car)
    }

    fn wheel_s80(&mut self, mem: &mut Mem, wheel: u32) -> u32 {
        let inner = self.record_of(0x5293D0, wheel);
        apply_flag_changes(mem, inner);
        inner.eax
    }

    fn wheel_s90(&mut self, mem: &mut Mem, wheel: u32) {
        let inner = self.record_of(0x52BF30, wheel);
        apply_flag_changes(mem, inner);
    }

    fn wheel_simfd00(&mut self, mem: &mut Mem, wheel: u32) -> u32 {
        let inner = self.record_of(0x51FD00, wheel);
        apply_flag_changes(mem, inner);
        inner.eax
    }

    fn body_test_1920(&mut self, _mem: &mut Mem, body: u32) -> u32 {
        self.record_of(0x521920, body).eax
    }

    fn body_vf6c(&mut self, _mem: &mut Mem, body: u32, dt: f32) {
        let inner = self.record_of(0x521D90, body);
        self.check_dt("Vf6c", inner, dt);
    }

    fn body_test_1940(&mut self, _mem: &mut Mem, body: u32) -> u32 {
        self.record_of(0x521940, body).eax
    }

    fn body_step_big(&mut self, _mem: &mut Mem, body: u32) -> u32 {
        self.record_of(0x522100, body).eax
    }

    fn body_excl(&mut self, _mem: &mut Mem, _body: u32, car: u32) {
        let inner = self.record(0x51A060);
        assert_eq!(inner.args[1], car, "Vf6cx call {}: ExclA060 car", self.call.index);
    }

    fn drive_thunk(&mut self, mem: &mut Mem, car: u32, _timer: f64) -> f32 {
        let inner = self.record(0x50A4F0);
        assert_eq!(inner.args[1], mem.drive_line_word(car), "Vf6cx call {}: thunk word", self.call.index);
        f64::from_bits(inner.st0_bits) as f32
    }

    fn timekeeper_record(&mut self, _mem: &mut Mem, f8: f32, timer: f64) {
        let inner = self.record(0x4A0540);
        assert_eq!(inner.args[1], f8.to_bits(), "Vf6cx call {}: Record f8", self.call.index);
        check_input("Record timer", self.call.index, 0, inner, &timer.to_le_bytes());
    }

    fn timekeeper_update(&mut self, _mem: &mut Mem) -> u32 {
        self.record(0x4A0320).eax
    }

    fn drive_get4540(&mut self, _mem: &mut Mem, _drive: u32) -> f32 {
        f64::from_bits(self.record(0x504540).st0_bits) as f32
    }

    /// `Update9AF0` is ported and its record is not an inner call here: its effects are not compared (in the drive it
    /// returns at once, with its argument 0).
    fn emit_signals(&mut self, _mem: &mut Mem, _car: u32, _v: u32) {}

    fn emit_position(&mut self, _mem: &mut Mem, payload: [u32; 5]) {
        let inner = self.record(0x4E8B30);
        let bytes: Vec<u8> = payload.iter().flat_map(|w| w.to_le_bytes()).collect();
        check_input("position emit", self.call.index, 0, inner, &bytes);
    }

    fn emit_mrr(&mut self, _mem: &mut Mem, payload: [u32; 3]) {
        let inner = self.record(0x4E8BF0);
        let bytes: Vec<u8> = payload.iter().flat_map(|w| w.to_le_bytes()).collect();
        check_input("MrR emit", self.call.index, 0, inner, &bytes);
    }
}

#[test]
fn vf6cx_matches_original_calls() {
    let Some(calls) = calls(0x4ea300, "CCar::Vf6cx") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let dt = f32::from_bits(call.args[1]);
        let mut answers = RecordedVf6cx { call, next: 0 };
        vf6cx::step_car(&mut answers, &mut mem, car, dt, rbr_physics::Variant::Stock102);
        let (_, _, _, _, post) = &call.windows[0];
        for offset in [
            0x15E8, 0x15F4, 0x15F8, 0x15FC, 0x1600, 0x1610, 0x1614, 0x1618, 0x161C, 0x166C, 0x1668, 0x1670, 0x9E8,
            0x9EC,
        ] {
            let theirs = u32::from_le_bytes(post[offset..offset + 4].try_into().unwrap());
            let ours = mem.u32(car + offset as u32);
            assert!(same_word(ours, theirs), "Vf6cx call {}: +{offset:#x}: {ours:#010x} vs {theirs:#010x}", call.index);
        }
    }
    eprintln!("CCar::Vf6cx: {} calls", calls.len());
}

// ---- CCar::Sample9800 ----

/// Answers `Sample9800`'s getters, timer gap and emit from one call's records, in call order.
struct RecordedSample<'a> {
    call: &'a Call,
    next: usize,
}

impl<'a> RecordedSample<'a> {
    /// The next record of `addr` after the cursor (the records skipped are nested calls, as in `RecordedUpdate`).
    fn until(&mut self, addr: u32) -> &'a Inner {
        let call: &'a Call = self.call;
        let (at, inner) = call.inner[self.next..]
            .iter()
            .enumerate()
            .find(|(_, c)| c.addr == addr)
            .unwrap_or_else(|| panic!("Sample9800 call {}: no {addr:#x} record", call.index));
        self.next += at + 1;
        inner
    }

    /// A getter on `this` that returns a float in st0 (the record's st0, as a single).
    fn float_getter(&mut self, addr: u32, this: u32) -> f32 {
        let index = self.call.index;
        let inner = self.until(addr);
        assert_eq!(inner.args[0], this, "Sample9800 call {index}: {addr:#x} this");
        f64::from_bits(inner.st0_bits) as f32
    }

    /// A getter on `this` that returns in eax.
    fn word_getter(&mut self, addr: u32, this: u32) -> u32 {
        let index = self.call.index;
        let inner = self.until(addr);
        assert_eq!(inner.args[0], this, "Sample9800 call {index}: {addr:#x} this");
        inner.eax
    }
}

impl<'a> Sample9800Calls for RecordedSample<'a> {
    fn engine_state_188(&mut self, engine: u32) -> f32 {
        self.float_getter(0x50EB80, engine)
    }

    fn drive_4530(&mut self, drive: u32) -> u32 {
        self.word_getter(0x504530, drive)
    }

    fn drive_4540(&mut self, drive: u32) -> f32 {
        self.float_getter(0x504540, drive)
    }

    fn engine_sim_ee60(&mut self, engine: u32) -> f32 {
        self.float_getter(0x50EE60, engine)
    }

    fn engine_sim_e970(&mut self, engine: u32) -> f32 {
        self.float_getter(0x50E970, engine)
    }

    fn engine_48_58(&mut self, engine: u32) -> f32 {
        self.float_getter(0x50D380, engine)
    }

    fn engine_sim_e980(&mut self, engine: u32) -> u32 {
        self.word_getter(0x50E980, engine)
    }

    fn timer_gap(&mut self, obj: u32, at: u32) -> f64 {
        let index = self.call.index;
        let inner = self.until(0x4A0290);
        assert_eq!(inner.args[0], obj, "Sample9800 call {index}: timer object");
        assert_eq!(inner.args[1], at, "Sample9800 call {index}: timer address");
        f64::from_bits(inner.st0_bits)
    }

    fn emit_sensor(&mut self, payload: &[u32; 16]) {
        let inner = self.until(0x4E8270);
        let bytes: Vec<u8> = payload.iter().flat_map(|w| w.to_le_bytes()).collect();
        check_input("sensor emit", self.call.index, 0, inner, &bytes);
    }
}

#[test]
fn sample9800_matches_original_calls() {
    let Some(calls) = calls(0x4e9800, "CCar::Sample9800") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedSample { call, next: 0 };
        sample9800::sample_sensors(&mut answers, &mem, car);
        assert_eq!(answers.next, call.inner.len(), "Sample9800 call {}: records left over", call.index);
    }
    eprintln!("CCar::Sample9800: {} calls", calls.len());
}

/// The next record of `addr` at or after `*next` (`*next` moves past it): the records skipped are the nested calls of
/// the callees, which the original logs first.
fn record_after<'a>(call: &'a Call, next: &mut usize, addr: u32) -> &'a Inner {
    let (at, inner) = call.inner[*next..]
        .iter()
        .enumerate()
        .find(|(_, c)| c.addr == addr)
        .unwrap_or_else(|| panic!("call {}: no {addr:#x} record", call.index));
    *next += at + 1;
    inner
}

/// Answers `UpdateSeed`'s externals from its records, in call order.
struct RecordedSeed<'a> {
    call: &'a Call,
    next: usize,
    /// Set when Assist8DF0 ran: it calls UpdateSeed again (at 0x4e8eda), and that nested run's writes are not modelled.
    nested: bool,
}

impl<'a> RecordedSeed<'a> {
    fn until(&mut self, addr: u32) -> &'a Inner {
        record_after(self.call, &mut self.next, addr)
    }

    /// ReadPacket, NopD570 and M5970: the seed buffer is their second argument, and its post-state is written back.
    fn seed_call(&mut self, mem: &mut Mem, addr: u32, obj: u32, seed: u32) {
        let inner = self.until(addr);
        assert_eq!(inner.args, [obj, seed], "UpdateSeed call {}: {addr:#x}", self.call.index);
        apply_changes(mem, inner, 0);
    }
}

impl UpdateSeedCalls for RecordedSeed<'_> {
    fn read_packet(&mut self, mem: &mut Mem, obj: u32, seed: u32) {
        self.seed_call(mem, READ_PACKET, obj, seed);
    }

    fn nop_d570(&mut self, mem: &mut Mem, obj: u32, seed: u32) {
        self.seed_call(mem, NOP_D570, obj, seed);
    }

    fn m5970(&mut self, mem: &mut Mem, obj: u32, seed: u32) {
        self.seed_call(mem, M5970, obj, seed);
    }

    fn is_state2(&mut self, obj: u32) -> u32 {
        let inner = self.until(E19_IS_STATE2);
        assert_eq!(inner.args, [obj], "UpdateSeed call {}: E19", self.call.index);
        inner.eax
    }

    fn s8c(&mut self, car: u32) -> u32 {
        let inner = self.until(S8C);
        assert_eq!(inner.args, [car], "UpdateSeed call {}: S8C", self.call.index);
        inner.eax
    }

    fn s158(&mut self, car: u32) {
        let Some(at) = self.call.inner[self.next..]
            .iter()
            .position(|inner| inner.addr == 0x4E5EE0)
            .map(|offset| self.next + offset)
        else {
            // Older records predate the nested S158 spec and contain no call subtree.
            self.next = self.call.inner.len();
            return;
        };
        let slot = &self.call.inner[at];
        assert_eq!(slot.args, [car], "UpdateSeed call {}: S158 this", self.call.index);
        // UpdateSeed's direct-call capture only records the virtual call boundary (no slot windows or child tree).
        // Consume it here; direct Vf158x and solid records independently exercise the implementation.
        self.next = at + 1;
    }

    /// MotionProbe has no record of its own (see `RecordedUpdate::motion_probe`): its answer is read from the emit
    /// that follows it, where the out word is the pair's first word, or from Assist8DF0, which means a nonzero return.
    fn motion_probe(&mut self, _car: u32) -> (u32, u32) {
        let next = self.call.inner[self.next..]
            .iter()
            .find(|c| c.addr == EMIT_PROBLEM || c.addr == EMIT_DF0)
            .unwrap_or_else(|| panic!("UpdateSeed call {}: no probe emit", self.call.index));
        if next.addr == EMIT_DF0 {
            return (1, 0);
        }
        let (_, _, _, pre, _) = inner_window(next, 0);
        (0, u32::from_le_bytes(pre[..4].try_into().unwrap()))
    }

    fn emit_problem(&mut self, obj: u32, pair: [u32; 2]) {
        let inner = self.until(EMIT_PROBLEM);
        assert_eq!(inner.args[0], obj, "UpdateSeed call {}: 4e8a70 this", self.call.index);
        let (_, _, _, pre, _) = inner_window(inner, 0);
        let words = [0, 4].map(|at| u32::from_le_bytes(pre[at..at + 4].try_into().unwrap()));
        assert_eq!(words, pair, "UpdateSeed call {}: 4e8a70 pair", self.call.index);
    }

    fn assist_df0(&mut self, car: u32, words: [u32; 3]) {
        let inner = self.until(EMIT_DF0);
        assert_eq!(inner.args, [car, words[0], words[1], words[2]], "UpdateSeed call {}: 4e8df0", self.call.index);
        self.nested = true;
    }
}

/// `CCar::UpdateSeed` against its records: the seed's externals, the gates, the emits and the step counter. The car
/// words the port writes are compared after the run, except after an Assist8DF0 (its nested UpdateSeed writes them).
#[test]
fn update_seed_matches_original_calls() {
    let Some(calls) = calls(0x4e91f0, "CCar::UpdateSeed") else {
        return;
    };
    let mut nested = 0;
    for call in &calls {
        let mut mem = shared_windows(call);
        let car = call.args[0];
        let p = call.args[1];
        // The local seed's block is the buffer ReadPacket receives (the original's stack).
        let scratch = call.inner.iter().find(|c| c.addr == READ_PACKET).map_or(0, |c| c.args[1]);
        let mut answers = RecordedSeed { call, next: 0, nested: false };
        update_seed::update_seed(&mut answers, &mut mem, car, p, scratch);
        assert_eq!(answers.next, call.inner.len(), "UpdateSeed call {}: records left", call.index);
        if answers.nested {
            nested += 1;
            continue;
        }
        let (_, _, _, _, post) = &call.windows[0];
        let words = [0x1694, 0x1698, 0x16A4, 0x950, 0x9F0]
            .into_iter()
            .chain((0..4).flat_map(|q| [0x954 + 4 * q, 0x974 + 0x20 * q]));
        for at in words {
            let recorded = u32::from_le_bytes(post[at as usize..at as usize + 4].try_into().unwrap());
            assert_eq!(mem.u32(car + at), recorded, "UpdateSeed call {}: +{at:#x}", call.index);
        }
    }
    eprintln!(
        "CCar::UpdateSeed: {} calls, {nested} with a nested run (Assist8DF0), not compared word by word",
        calls.len()
    );
}

/// `E19IsState2` (0x499900, no record of its own: UpdateSeed's records keep only its eax): `sete` on the word at +0x10.
#[test]
fn replay_is_state2_tests_the_state_word() {
    let mut mem = Mem::new();
    mem.map(0x1000, &[0; 0x20]);
    for (state, want) in [(0, 0), (1, 0), (2, 1), (3, 0), (0x102, 0)] {
        mem.set_u32(0x1010, state);
        assert_eq!(mem.replay_is_state2(0x1000), want, "{state:#x}");
    }
}

/// The stack block UpdateMrr gives UpdateSeed: the test maps only the words the port writes (the block's pointer is not
/// checked, since UpdateSeed's own records are not kept).
const UPDATE_MRR_BLOCK: u32 = 0x0F00_0000;
const UPDATE_SEED_CALL: u32 = 0x4e91f0;
const EMIT_MRR: u32 = 0x4e8cb0;

/// Answers `UpdateMrr`'s calls from its records, in call order.
struct RecordedMrr<'a> {
    call: &'a Call,
    next: usize,
}

impl UpdateMrrCalls for RecordedMrr<'_> {
    fn call(&mut self, callee: u32, this: u32, args: &[u32]) {
        if callee == UPDATE_SEED_CALL {
            // UpdateSeed has no record of its own: its externals come before the emit that follows it.
            let rest = self.call.inner[self.next..]
                .iter()
                .position(|c| c.addr == EMIT_MRR)
                .unwrap_or_else(|| panic!("UpdateMrr call {}: no emit after UpdateSeed", self.call.index));
            self.next += rest;
            return;
        }
        let inner = record_after(self.call, &mut self.next, callee);
        let expected: Vec<u32> = std::iter::once(this).chain(args.iter().copied()).collect();
        assert_eq!(inner.args, expected, "UpdateMrr call {}: {callee:#x}", self.call.index);
    }

    fn emit(&mut self, obj: u32, pair: [u32; 2]) {
        let inner = record_after(self.call, &mut self.next, EMIT_MRR);
        assert_eq!(inner.args[0], obj, "UpdateMrr call {}: 4e8cb0 this", self.call.index);
        let (_, _, _, pre, _) = inner_window(inner, 0);
        let words = [0, 4].map(|at| u32::from_le_bytes(pre[at..at + 4].try_into().unwrap()));
        assert_eq!(words, pair, "UpdateMrr call {}: 4e8cb0 pair", self.call.index);
    }
}

/// `CCar::UpdateMrr` against its records: the wiring calls and the list loops in order, UpdateSeed (its externals are
/// skipped to the emit after them) and the emit. The car's own write (+0x9f4) is compared after the run.
#[test]
fn update_mrr_matches_original_calls() {
    let Some(calls) = calls(0x4e9c70, "CCar::UpdateMrr") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedMrr { call, next: 0 };
        update_mrr::update_mrr(&mut answers, &mut mem, car, UPDATE_MRR_BLOCK);
        assert_eq!(answers.next, call.inner.len(), "UpdateMrr call {}: records left", call.index);
        let (_, _, _, _, post) = &call.windows[0];
        let recorded = u32::from_le_bytes(post[0x9F4..0x9F8].try_into().unwrap());
        assert_eq!(mem.u32(car + 0x9F4), recorded, "UpdateMrr call {}: +0x9f4", call.index);
    }
    eprintln!("CCar::UpdateMrr: {} calls", calls.len());
}

const BODY_COMPOSE: u32 = 0x4e6a30;
const IDENTITY_S30: u32 = 0x519c00;
const COMPOSE_A: u32 = 0x51b4c0;

/// Answers `Xform8130`'s calls from its records, in call order.
struct RecordedXform<'a> {
    call: &'a Call,
    next: usize,
}

impl ComposeA for RecordedXform<'_> {
    fn compose_a(&mut self, this: u32, a: u32, t: u32, out: u32) -> u32 {
        let inner = record_after(self.call, &mut self.next, COMPOSE_A);
        assert_eq!(inner.args, [this, a, t, out], "Xform8130 call {}: ComposeA", self.call.index);
        out
    }
}

impl Xform8130Calls for RecordedXform<'_> {
    fn body_compose(&mut self, mem: &mut Mem, car: u32, a: u32, out: u32) {
        let inner = record_after(self.call, &mut self.next, BODY_COMPOSE);
        assert_eq!(inner.args, [car, a, out], "Xform8130 call {}: BodyCompose", self.call.index);
        apply_changes(mem, inner, 1);
    }

    fn s30(&mut self, car: u32) -> u32 {
        let inner = record_after(self.call, &mut self.next, IDENTITY_S30);
        assert_eq!(inner.args, [car], "Xform8130 call {}: S30", self.call.index);
        inner.eax
    }
}

/// `CCar::Xform8130` against its records: BodyCompose, the identity S30 and ComposeA, in order, and the return (`b`).
#[test]
fn xform_8130_matches_original_calls() {
    let Some(calls) = calls(0x4e8130, "CCar::Xform8130") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let (car, a, b) = (call.args[0], call.args[1], call.args[2]);
        // The 28-byte frame is BodyCompose's out argument (the original's stack, so it is taken from its record).
        let body = call
            .inner
            .iter()
            .find(|c| c.addr == BODY_COMPOSE)
            .unwrap_or_else(|| panic!("Xform8130 call {}: no BodyCompose record", call.index));
        let m1 = body.args[2];
        let mut answers = RecordedXform { call, next: 0 };
        let ret = xform_8130::xform_8130(&mut answers, &mut mem, car, a, b, m1);
        assert_eq!(ret, b, "Xform8130 call {}: return", call.index);
        assert_eq!(answers.next, call.inner.len(), "Xform8130 call {}: records left", call.index);
    }
    eprintln!("CCar::Xform8130: {} calls", calls.len());
}

// ---- CCar::Vf28x ----

/// The frame's length in the record (the nodes window at Vf28x's esp-relative locals).
const VF28X_FRAME_LEN: usize = 0x180;

/// Answers `Vf28x`'s calls from its record's inner calls, matched by callee, `this` and the stack arguments the record
/// keeps (the inner calls of its nested externals are listed too, so the match is on all three). Each record answers once: the
/// external's windows are applied (what it wrote) and its eax is returned.
struct RecordedVf28x<'a> {
    root: &'a Call,
    used: Vec<bool>,
}

impl RecordedVf28x<'_> {
    fn answer(&mut self, mem: &mut Mem, callee: u32, this: u32, args: &[u32]) -> u32 {
        // A record keeps only the stack arguments its callee pops, so the arguments are compared up to its length.
        let matches = |c: &Inner| {
            c.addr == callee
                && c.args.first() == Some(&this)
                && c.args[1..].iter().zip(args).all(|(theirs, ours)| theirs == ours)
        };
        let k = self.root.inner.iter().zip(&self.used).position(|(c, used)| !used && matches(c)).unwrap_or_else(|| {
            panic!("Vf28x call {}: no {callee:#x} record with this {this:#x} args {args:x?}", self.root.index)
        });
        self.used[k] = true;
        let inner = &self.root.inner[k];
        for w in &inner.windows {
            apply_changes(mem, inner, w.0);
        }
        inner.eax
    }
}

impl Vf28xCalls for RecordedVf28x<'_> {
    fn call(&mut self, mem: &mut Mem, callee: u32, this: u32, args: &[u32]) -> u32 {
        self.answer(mem, callee, this, args)
    }

    fn emit(&mut self, mem: &mut Mem, callee: u32, signal: u32, payload: u32) {
        self.answer(mem, callee, signal, &[payload]);
    }
}

#[test]
fn vf28x_matches_original_calls() {
    let Some(calls) = calls(0x4ea020, "CCar::Vf28x") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let frame = call.windows.iter().find(|w| w.2 == VF28X_FRAME_LEN).expect("Vf28x: the frame window").1;
        let mut answers = RecordedVf28x { root: call, used: vec![false; call.inner.len()] };
        vf28x::update_sensors(&mut answers, &mut mem, car, frame);
        check_window_list(&call.windows[..2], &mem, "CCar::Vf28x");
        // The frame words the port does not model: the prologue's three pushes (E+0..0xc), the loop counter (E+0x14),
        // the engine pointer the port passes as a value (E+0x1c) and the dead copy of car+0x100 (E+0xc0..0xd0).
        let (_, _, _, _, post) = call.windows.iter().find(|w| w.2 == VF28X_FRAME_LEN).unwrap();
        for o in (0..VF28X_FRAME_LEN).step_by(4) {
            if (0..0x18).contains(&o) || o == 0x1C || (0xC0..0xD0).contains(&o) {
                continue;
            }
            assert_eq!(mem.read(frame + o as u32, 4), post[o..o + 4], "Vf28x call {}: frame +{o:#x}", call.index);
        }
    }
    eprintln!("CCar::Vf28x: {} calls", calls.len());
}

// ---- CCar::ToWorld, WorldCompose, BodyToWorldPair, BodyCompose ----

const WORLD_COMPOSE: u32 = 0x4e6950;
const BODY_TO_WORLD_PAIR: u32 = 0x4e69c0;

/// Answers `E43PairWalk::MB560` from the record, in call order. The port's `this`, `b` (the frame or the car) and input
/// `a` must be the original's; the walk's output is the record's `c`.
struct RecordedWalk<'a> {
    call: &'a Call,
    next: usize,
    name: &'static str,
}

impl PairWalk for RecordedWalk<'_> {
    fn pair_walk(&mut self, this: u32, a: Vec3, b: u32) -> Vec3 {
        let inner = record_after(self.call, &mut self.next, PAIR_WALK);
        assert_eq!(
            [inner.args[0], inner.args[2]],
            [this, b],
            "{} call {}: walk this and b",
            self.name,
            self.call.index
        );
        let (_, _, _, input, _) = inner_window(inner, 0);
        check_lanes(self.name, self.call.index, a, input);
        let (_, _, _, _, output) = inner_window(inner, 1);
        vec3_of(output)
    }
}

/// Maps the frame global to the frame the original walked or composed against: the third argument of the first inner
/// call to `addr` (`(this, a, frame, out)` for both MB560 and ComposeA).
fn map_frame(mem: &mut Mem, call: &Call, addr: u32) {
    let inner = call
        .inner
        .iter()
        .find(|c| c.addr == addr)
        .unwrap_or_else(|| panic!("call {}: no {addr:#x} record", call.index));
    mem.map(transform::FRAME, &inner.args[2].to_le_bytes());
}

/// Maps the body pointer (`car + 0x4a0`) to the body the original's second identity S30 was called on.
fn map_body(mem: &mut Mem, call: &Call) -> u32 {
    let body = call
        .inner
        .iter()
        .filter(|c| c.addr == IDENTITY_S30)
        .nth(1)
        .unwrap_or_else(|| panic!("call {}: no body S30 record", call.index))
        .args[0];
    mem.map(call.args[0] + 0x4A0, &body.to_le_bytes());
    body
}

/// `CCar::ToWorld(a, b)` against its record: the walk of `a` against the frame, and the walked vector in `b`.
#[test]
fn to_world_matches_original_calls() {
    let Some(calls) = calls(TO_WORLD, "CCar::ToWorld") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        map_frame(&mut mem, call, PAIR_WALK);
        let (car, a) = (call.args[0], vec3_of(&call.windows[0].3));
        let mut walk = RecordedWalk { call, next: 0, name: "ToWorld" };
        let out = transform::to_world(&mut walk, &mem, car, a);
        assert_eq!(walk.next, call.inner.len(), "ToWorld call {}: records left", call.index);
        assert_eq!(call.eax, call.args[2], "ToWorld call {}: eax", call.index);
        let (_, _, _, _, b_post) = &call.windows[1];
        check_lanes("ToWorld", call.index, out, b_post);
    }
    eprintln!("CCar::ToWorld: {} calls", calls.len());
}

/// `CCar::WorldCompose(a, b)` against its record: `ComposeA(car, a, frame, b)`; `b` is the recorded output.
#[test]
fn world_compose_matches_original_calls() {
    let Some(calls) = calls(WORLD_COMPOSE, "CCar::WorldCompose") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        map_frame(&mut mem, call, COMPOSE_A);
        let (car, a, b) = (call.args[0], call.args[1], call.args[2]);
        let mut answers = RecordedCompose { call, next: 0 };
        let eax = transform::world_compose(&mut answers, &mem, car, a, b);
        assert_eq!(eax, call.eax, "WorldCompose call {}: eax", call.index);
        assert_eq!(answers.next, 1, "WorldCompose call {}: ComposeA", call.index);
        let (_, _, _, _, out_post) = &call.windows[1];
        mem.map(b, out_post);
        check_windows(call, &mem, "CCar::WorldCompose");
    }
    eprintln!("CCar::WorldCompose: {} calls", calls.len());
}

/// `CCar::BodyToWorldPair(a, b)` (direct calls: the step does not reach it): `MB560(body, a, car, b)`.
#[test]
fn body_to_world_pair_matches_original_calls() {
    let Some(calls) = calls(BODY_TO_WORLD_PAIR, "CCar::BodyToWorldPair") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        map_body(&mut mem, call);
        let (car, a) = (call.args[0], vec3_of(&call.windows[0].3));
        let mut walk = RecordedWalk { call, next: 0, name: "BodyToWorldPair" };
        let out = transform::body_to_world_pair(&mut walk, &mem, car, a);
        assert_eq!(walk.next, call.inner.len(), "BodyToWorldPair call {}: records left", call.index);
        assert_eq!(call.eax, call.args[2], "BodyToWorldPair call {}: eax", call.index);
        let (_, _, _, _, b_post) = &call.windows[1];
        check_lanes("BodyToWorldPair", call.index, out, b_post);
    }
    eprintln!("CCar::BodyToWorldPair: {} calls", calls.len());
}

/// `CCar::BodyCompose(a, b)` against its record: `ComposeA(body, a, car, b)`; `b` is the recorded output.
#[test]
fn body_compose_matches_original_calls() {
    let Some(calls) = calls(BODY_COMPOSE, "CCar::BodyCompose") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        map_body(&mut mem, call);
        let (car, a, b) = (call.args[0], call.args[1], call.args[2]);
        let mut answers = RecordedCompose { call, next: 0 };
        let eax = transform::body_compose(&mut answers, &mem, car, a, b);
        assert_eq!(eax, call.eax, "BodyCompose call {}: eax", call.index);
        assert_eq!(answers.next, 1, "BodyCompose call {}: ComposeA", call.index);
        let (_, _, _, _, out_post) = &call.windows[1];
        mem.map(b, out_post);
        check_windows(call, &mem, "CCar::BodyCompose");
    }
    eprintln!("CCar::BodyCompose: {} calls", calls.len());
}

// ---- CCar::GetDir, FfbNorm ----

const GET_DIR: u32 = 0x4e5cf0;
const FFB_NORM: u32 = 0x4e76c0;

/// `CCar::GetDir` against its record: the wheel directions (the car's wheel pointers, mapped by the car window) give the
/// heading in lanes 0 to 2. Lane 3 is the caller's word, scaled on the unit path (asm); the port does not return it.
#[test]
fn get_dir_matches_original_calls() {
    let Some(calls) = calls(GET_DIR, "CCar::GetDir") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let car = call.args[0];
        let front = mem.wheel(car, 0).map(|w| mem.wheel_direction(w));
        let rear = mem.wheel(car, 1).map(|w| mem.wheel_direction(w));
        let out = accessors::get_dir(front, rear);
        let (_, _, _, _, post) = &call.windows[1];
        check_lanes("CCar::GetDir", call.index, out, post);
    }
    eprintln!("CCar::GetDir: {} calls", calls.len());
}

/// Answers `FfbNorm`'s gate from its record: the S8C call, in order, with the car as `this`.
struct RecordedGate<'a> {
    call: &'a Call,
    next: usize,
}

impl FfbNormCalls for RecordedGate<'_> {
    fn s8c(&mut self, car: u32) -> u32 {
        let inner = record_after(self.call, &mut self.next, S8C);
        assert_eq!(inner.args, [car], "FfbNorm call {}: S8C", self.call.index);
        inner.eax
    }
}

/// `CCar::FfbNorm` against its record (the return is st0, NaN-tolerant).
#[test]
fn ffb_norm_matches_original_calls() {
    let Some(calls) = calls(FFB_NORM, "CCar::FfbNorm") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let mut gate = RecordedGate { call, next: 0 };
        let norm = steering_ffb::ffb_norm(&mut gate, &mem, call.args[0]);
        let theirs = f64::from_bits(call.st0_bits);
        if norm.is_nan() {
            assert!(theirs.is_nan(), "FfbNorm call {}: st0 NaN", call.index);
        } else {
            check_st0(call, norm, "CCar::FfbNorm");
        }
        assert_eq!(gate.next, call.inner.len(), "FfbNorm call {}: S8C records", call.index);
    }
    eprintln!("CCar::FfbNorm: {} calls", calls.len());
}

// ---- CCar::PushGridEdge, RefreshEdge, Vf24x ----

const PUSH_GRID_EDGE: u32 = 0x4e7270;
const REFRESH_EDGE: u32 = 0x4e72d0;
const ADD_EDGE: u32 = 0x4f65b0;
const VF20: u32 = 0x529200;
const VF150X: u32 = 0x4e5f20;
const SET3FB0: u32 = 0x4ebfb0;
const COPY_FROM: u32 = 0x4ec470;
const TEST_3B20_DAMAGE: u32 = 0x4ec0d0;
const ADD_FUNCTOR: u32 = 0x4ef720;
const SUB_BLOCK_INIT: u32 = 0x4f3e80;
const BLOCK_RESET: u32 = 0x4f19b0;
const DEF_TABLE_VF20X: u32 = 0x4f17e0;
const LOAD_BINARY: u32 = 0x4f3d20;
const CROSS_NORM: u32 = 0x4f3d40;
const MIN_PICK: u32 = 0x4f3ec0;
const RESET_DAMAGE: u32 = 0x4ec1e0;
const VF14X: u32 = 0x4ec440;
const EMIT_LIGHTS: u32 = 0x4f20c0;
const UPDATE_LIGHTS: u32 = 0x4f2150;
const UPDATE_A720: u32 = 0x4ea720;
const UPDATE_MRR: u32 = 0x4e9c70;
const VF9CX: u32 = 0x4eb820;
const VFA0X: u32 = 0x4eb8c0;
const BODY_SLOT0: u32 = 0x519950;
const LIGHT_EMIT: u32 = 0x4f2000;
const SIGNAL_MANAGER: u32 = 0x893600;
const TAG_520940: u32 = 0x520940;
const E43_VFEC: u32 = 0x51a040;
const E43_VF150: u32 = 0x51a370;

/// The lanes of a recorded 16-byte vector (four words).
fn vec4_of(bytes: &[u8]) -> Vec4 {
    let lane = |k: usize| f32::from_le_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap());
    Vec4 { x: lane(0), y: lane(1), z: lane(2), w: lane(3) }
}

/// Answers the edge's externals from the record, in call order: BodyCompose (its zero input and its output lanes), the
/// AddEdge on the grid (the edge it is given), and E43Vf20 on the car.
struct RecordedEdge<'a> {
    call: &'a Call,
    next: usize,
}

impl EdgeCalls for RecordedEdge<'_> {
    fn body_compose(&mut self, car: u32, a: Vec4) -> Vec4 {
        let inner = record_after(self.call, &mut self.next, BODY_COMPOSE);
        assert_eq!(inner.args[0], car, "PushGridEdge call {}: BodyCompose this", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        assert_eq!(vec4_of(input), a, "PushGridEdge call {}: BodyCompose input", self.call.index);
        let (_, _, _, _, output) = inner_window(inner, 1);
        vec4_of(output)
    }

    fn add_edge(&mut self, grid: u32, edge: Vec4) {
        let inner = record_after(self.call, &mut self.next, ADD_EDGE);
        assert_eq!(inner.args[0], grid, "PushGridEdge call {}: AddEdge grid", self.call.index);
        let (_, _, _, input, _) = inner_window(inner, 0);
        assert_eq!(vec4_of(input), edge, "PushGridEdge call {}: AddEdge edge", self.call.index);
    }

    /// The car's own Vf20 record: it finishes last, after the nested calls it makes on other objects.
    fn vf20(&mut self, car: u32, a: u32) -> u32 {
        let inner = self
            .call
            .inner
            .iter()
            .rev()
            .find(|c| c.addr == VF20 && c.args[0] == car)
            .unwrap_or_else(|| panic!("RefreshEdge call {}: no E43Vf20 on the car", self.call.index));
        assert_eq!(inner.args, [car, a], "RefreshEdge call {}: E43Vf20", self.call.index);
        inner.eax
    }
}

/// `CCar::PushGridEdge` against its records: the early return or the full path, and the flag it leaves.
#[test]
fn push_grid_edge_matches_original_calls() {
    let Some(calls) = calls(PUSH_GRID_EDGE, "CCar::PushGridEdge") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedEdge { call, next: 0 };
        let pushed = edge::push_grid_edge(&mut answers, car, mem.grid_pushed(car), mem.grid_object(car));
        mem.set_grid_pushed(car, pushed);
        assert_eq!(answers.next, call.inner.len(), "PushGridEdge call {}: records left", call.index);
        check_windows(call, &mem, "CCar::PushGridEdge");
    }
    eprintln!("CCar::PushGridEdge: {} calls", calls.len());
}

/// `CCar::RefreshEdge(a)`: the edge (already pushed in the step) and `E43Vf20(a)`; its return is Vf20's eax.
#[test]
fn refresh_edge_matches_original_calls() {
    let Some(calls) = calls(REFRESH_EDGE, "CCar::RefreshEdge") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedEdge { call, next: 0 };
        let (_, eax) = edge::refresh_edge(&mut answers, car, mem.grid_pushed(car), mem.grid_object(car), call.args[1]);
        assert_eq!(eax, call.eax, "RefreshEdge call {}: eax", call.index);
    }
    eprintln!("CCar::RefreshEdge: {} calls", calls.len());
}

/// Answers `Vf150x`'s calls from the record, in call order: the car's E43VfEC on the wheel, the wheel's on the car (both
/// 0x51a040), then E43Vf150 on the car. The E43Vf150 record is kept: its list append is applied to the memory after the
/// port returns (the port's last call).
struct RecordedWheel<'a> {
    call: &'a Call,
    next: usize,
    list_append: Option<&'a Inner>,
}

impl WheelCalls for RecordedWheel<'_> {
    fn car_vfec(&mut self, car: u32, wheel: u32) {
        let inner = record_after(self.call, &mut self.next, E43_VFEC);
        assert_eq!(inner.args, [car, wheel], "Vf150x call {}: car E43VfEC", self.call.index);
    }

    fn wheel_vfec(&mut self, wheel: u32, car: u32) {
        let inner = record_after(self.call, &mut self.next, E43_VFEC);
        assert_eq!(inner.args, [wheel, car], "Vf150x call {}: wheel E43VfEC", self.call.index);
    }

    fn e43vf150(&mut self, car: u32, wheel: u32) -> u32 {
        let inner = record_after(self.call, &mut self.next, E43_VF150);
        assert_eq!(inner.args, [car, wheel], "Vf150x call {}: E43Vf150", self.call.index);
        self.list_append = Some(inner);
        inner.eax
    }
}

/// `CCar::Vf150x` against its records: every flag path the direct runs reach (the list, the body, and both slot lists,
/// with the slot empty or taken and the index in or out of range), with the windows and the return.
#[test]
fn vf150x_matches_original_calls() {
    let Some(calls) = calls(VF150X, "CCar::Vf150x") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let (car, wheel) = (call.args[0], call.args[1]);
        let mut answers = RecordedWheel { call, next: 0, list_append: None };
        let eax = wheel::attach_wheel(&mut answers, &mut mem, car, wheel);
        if let Some(inner) = answers.list_append {
            apply_changes(&mut mem, inner, 0);
            apply_changes(&mut mem, inner, 1);
        }
        assert_eq!(answers.next, call.inner.len(), "Vf150x call {}: records left", call.index);
        assert_eq!(eax, call.eax, "Vf150x call {}: eax", call.index);
        check_windows(call, &mem, "CCar::Vf150x");
    }
    eprintln!("CCar::Vf150x: {} calls", calls.len());
}

/// `CCar::Vf150x(0)`: returns 0 and makes no call. The direct runs never pass a null wheel, so no record covers it.
#[test]
fn vf150x_null_wheel_returns_zero() {
    struct NoCalls;
    impl WheelCalls for NoCalls {
        fn car_vfec(&mut self, _: u32, _: u32) {
            panic!("Vf150x(0) makes no call");
        }
        fn wheel_vfec(&mut self, _: u32, _: u32) {
            panic!("Vf150x(0) makes no call");
        }
        fn e43vf150(&mut self, _: u32, _: u32) -> u32 {
            panic!("Vf150x(0) makes no call");
        }
    }
    let mut mem = Mem::new();
    assert_eq!(wheel::attach_wheel(&mut NoCalls, &mut mem, 0x1000, 0), 0);
}

/// `CCar::Vf24x` (no record: the drive never calls it in the snapshots, and a direct run records nothing for the tail jump):
/// the forward passes the drive member `car + 0x4b0` (the asm's `add` before the jump).
#[test]
fn vf24x_forwards_to_drive_member() {
    struct Drive(Option<u32>);
    impl Vf24xCalls for Drive {
        fn drive(&mut self, drive: u32) {
            self.0 = Some(drive);
        }
    }
    let mut drive = Drive(None);
    forward::vf24x(&mut drive, 0x0BF1_D7B8);
    assert_eq!(drive.0, Some(0x0BF1_D7B8 + 0x4B0));
}

// ---- CMultiSignal1<T>::Emit ----

/// Answers each `Receive` from the record: the next inner call of the emit whose arguments are the node and the signal.
struct RecordedReceive<'a> {
    call: &'a Call,
    next: usize,
}

impl Receive for RecordedReceive<'_> {
    fn receive(&mut self, node: u32, signal: u32) {
        let at = self.call.inner[self.next..]
            .iter()
            .position(|c| c.args == [node, signal])
            .unwrap_or_else(|| panic!("Emit call {}: no Receive of {node:#x}", self.call.index));
        self.next += at + 1;
    }
}

/// The `CMultiSignal1<T>::Emit` instances against their records: the position and MrR state (one record per drive
/// snapshot), the retire and outside-assistance signals (direct calls, `g-retire` set). The other six are not reached by the
/// snapshots (`car_body/README.md` (git 9c28c0b8)).
#[test]
fn emit_matches_original_calls() {
    for (addr, name) in
        [(0x4E8B30, "position"), (0x4E8BF0, "MrR state"), (0x4E8770, "retire"), (0x4E89B0, "outside assistance")]
    {
        let Some(calls) = calls(addr, name) else {
            return;
        };
        for call in &calls {
            let mut mem = map_windows(call);
            let (emitter, signal) = (call.args[0], call.args[1]);
            let mut sink = RecordedReceive { call, next: 0 };
            emit::emit(&mut sink, &mut mem, emitter, signal);
            assert_eq!(sink.next, call.inner.len(), "Emit {name} call {}: receives left", call.index);
            check_windows(call, &mem, name);
        }
        eprintln!("CMultiSignal1::Emit {name}: {} calls", calls.len());
    }
}

/// The three lanes at `at`, as `Vec3`.
fn read_vec3(mem: &Mem, at: u32) -> Vec3 {
    Vec3::load(mem, at)
}

/// `E01VecSetA::Cross` (0x43c630): the inner calls of the car's records write three lanes of their out vector and
/// leave the fourth alone (the record's out window is 16 bytes, its last word unchanged).
#[test]
fn cross3_matches_original_calls() {
    let Some(calls) = load_inner(VAR, CROSS3) else {
        eprintln!("skipping E01VecSetA::Cross: {VAR} not set");
        return;
    };
    for inner in &calls {
        let mut mem = map_window_list(&inner.windows);
        let (a, b, out) = (inner.args[0], inner.args[1], inner.args[2]);
        let v = read_vec3(&mem, a).cross3(read_vec3(&mem, b));
        mem.set_f32(out, v.x);
        mem.set_f32(out + 4, v.y);
        mem.set_f32(out + 8, v.z);
        check_window_list(&inner.windows, &mem, "E01VecSetA::Cross");
    }
}

/// `E01Vec4::Scale` (0x473cd0): the inner calls scale all four lanes of `a` into `out`; the record's out window
/// changes its last word too, so the port is the four-lane `Vec4::scale`. NaN words compare by class (`same_word`).
#[test]
fn scale_matches_original_calls() {
    let Some(calls) = load_inner(VAR, SCALE) else {
        eprintln!("skipping E01Vec4::Scale: {VAR} not set");
        return;
    };
    for inner in &calls {
        let mut mem = map_window_list(&inner.windows);
        let (a, out) = (inner.args[0], inner.args[2]);
        let s = f32::from_bits(inner.args[1]);
        let v = Vec4::load(&mem, a).scale(s);
        v.store(&mut mem, out);
        for (k, at, _, _, post) in &inner.windows {
            for (w, bytes) in post.chunks_exact(4).enumerate() {
                let theirs = u32::from_le_bytes(bytes.try_into().unwrap());
                let ours = mem.u32(*at + 4 * w as u32);
                assert!(same_word(ours, theirs), "E01Vec4::Scale window {k} word {w}: {ours:#010x} vs {theirs:#010x}");
            }
        }
    }
}

/// `CCarDamageHandler::Set3FB0(level)` against its records: the counter, the level and the threshold for each case (the
/// direct runs cover 0, 1, 2, 3, the default word, and the negative and large words).
#[test]
fn set3fb0_matches_original_calls() {
    let Some(calls) = calls(SET3FB0, "CCarDamageHandler::Set3FB0") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        damage::set_level(&mut mem, call.args[0], call.args[1]);
        check_windows(call, &mem, "CCarDamageHandler::Set3FB0");
    }
    eprintln!("CCarDamageHandler::Set3FB0: {} calls", calls.len());
}

/// `CCarDamageHandler::CopyFrom(src)` against its records: the zones, the floats and the tail, from a random source.
#[test]
fn copy_from_matches_original_calls() {
    let Some(calls) = calls(COPY_FROM, "CCarDamageHandler::CopyFrom") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        damage::copy_from(&mut mem, call.args[0], call.args[1]);
        check_windows(call, &mem, "CCarDamageHandler::CopyFrom");
    }
    eprintln!("CCarDamageHandler::CopyFrom: {} calls", calls.len());
}

/// `CCarDamageHandler::Test3B20()` against its records: the enable word, the NaN and zero level, and the counted words.
#[test]
fn test_3b20_matches_original_calls() {
    let Some(calls) = calls(TEST_3B20_DAMAGE, "CCarDamageHandler::Test3B20") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let ours = damage::test_3b20(&mem, call.args[0]);
        assert_eq!(u32::from(ours), call.eax, "Test3B20 call {}: eax", call.index);
        check_windows(call, &mem, "CCarDamageHandler::Test3B20");
    }
    eprintln!("CCarDamageHandler::Test3B20: {} calls", calls.len());
}

/// `E36DamageOps::AddFunctor(idx, a, b, c)` against its records: every index the switch maps (each with a free table), the
/// two it leaves out, the full table and the out-of-range indices (direct runs, 200 calls).
#[test]
fn add_functor_matches_original_calls() {
    let Some(calls) = calls(ADD_FUNCTOR, "E36DamageOps::AddFunctor") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let ours =
            damage::add_functor(&mut mem, call.args[0], call.args[1] as i32, call.args[2], call.args[3], call.args[4]);
        assert_eq!(u32::from(ours), call.eax, "AddFunctor call {}: eax", call.index);
        check_windows(call, &mem, "E36DamageOps::AddFunctor");
    }
    eprintln!("E36DamageOps::AddFunctor: {} calls", calls.len());
}

/// `CE36SubBlock::Init()` against its records: the two zeroed words and the zeroed float, from random words.
#[test]
fn sub_block_init_matches_original_calls() {
    let Some(calls) = calls(SUB_BLOCK_INIT, "CE36SubBlock::Init") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        light::sub_block_init(&mut mem, call.args[0]);
        check_windows(call, &mem, "CE36SubBlock::Init");
    }
    eprintln!("CE36SubBlock::Init: {} calls", calls.len());
}

/// `E36LightBlock::Reset()` against its records: the four groups and the flag word, from random block words.
#[test]
fn block_reset_matches_original_calls() {
    let Some(calls) = calls(BLOCK_RESET, "E36LightBlock::Reset") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        light::block_reset(&mut mem, call.args[0]);
        check_windows(call, &mem, "E36LightBlock::Reset");
    }
    eprintln!("E36LightBlock::Reset: {} calls", calls.len());
}

/// `CCarLightSystem::CDef::Vf20x()` against its records: the static table's address (eax).
#[test]
fn def_table_matches_original_calls() {
    let Some(calls) = calls(DEF_TABLE_VF20X, "CCarLightSystem::CDef::Vf20x") else {
        return;
    };
    for call in &calls {
        assert_eq!(light::def_table(), call.eax, "Vf20x call {}: eax", call.index);
    }
    eprintln!("CCarLightSystem::CDef::Vf20x: {} calls", calls.len());
}

/// `NTweak::CFloat3::LoadBinary(data)` against its records: the four words copied, and the return (`this`).
#[test]
fn load_binary_matches_original_calls() {
    let Some(calls) = calls(LOAD_BINARY, "NTweak::CFloat3::LoadBinary") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let this = tweak::load_binary(&mut mem, call.args[0], call.args[1]);
        assert_eq!(this, call.eax, "LoadBinary call {}: eax", call.index);
        check_windows(call, &mem, "NTweak::CFloat3::LoadBinary");
    }
    eprintln!("NTweak::CFloat3::LoadBinary: {} calls", calls.len());
}

/// `E36VecU::CrossNorm(a, out)` against its records (direct runs, 40 calls from random lanes, zeros and NaN or infinite
/// words): the output lanes (NaN-tolerantly, the sign of a NaN is the platform's) and the return (`out`).
#[test]
fn cross_norm_matches_original_calls() {
    let Some(calls) = calls(CROSS_NORM, "E36VecU::CrossNorm") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let [this, a, out] = [call.args[0], call.args[1], call.args[2]];
        let v = e36::cross_norm(vec4_of(&mem.read(this, 16)), vec4_of(&mem.read(a, 16)));
        for (k, lane) in [v.x, v.y, v.z, v.w].into_iter().enumerate() {
            mem.set_f32(out + 4 * k as u32, lane);
        }
        assert_eq!(out, call.eax, "CrossNorm call {}: eax", call.index);
        check_windows_tolerant(call, &mem, "E36VecU::CrossNorm");
    }
    eprintln!("E36VecU::CrossNorm: {} calls", calls.len());
}

/// `E36Triple::MinPick()` against its records: `m_a` for `m_f < 0.5`, `m_b` otherwise (NaN included).
#[test]
fn min_pick_matches_original_calls() {
    let Some(calls) = calls(MIN_PICK, "E36Triple::MinPick") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let at = call.args[0];
        let triple = Triple { a: mem.u32(at) as i32, b: mem.u32(at + 4) as i32, f: mem.f32(at + 8) };
        assert_eq!(e36::min_pick(triple) as u32, call.eax, "MinPick call {}: eax", call.index);
        check_windows(call, &mem, "E36Triple::MinPick");
    }
    eprintln!("E36Triple::MinPick: {} calls", calls.len());
}

/// Answers `ResetDamage`'s member calls from its records, in call order (the light block is the port's own call).
struct RecordedReset<'a> {
    call: &'a Call,
    next: usize,
}

impl ResetDamageCalls for RecordedReset<'_> {
    fn call(&mut self, mem: &mut Mem, callee: u32, this: u32) {
        let inner = next_record(self.call, &mut self.next, callee);
        assert_eq!(inner.args, [this], "ResetDamage call {}: {callee:#x}", self.call.index);
        apply_changes(mem, inner, 0);
    }
}

/// `CCarDamageHandler::ResetDamage()` against its records: the zones and floats, the member calls (in order, answered from
/// the records), the flags and the table words (direct runs, 40 calls from random handler bytes).
#[test]
fn reset_damage_matches_original_calls() {
    let Some(calls) = calls(RESET_DAMAGE, "CCarDamageHandler::ResetDamage") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let mut answers = RecordedReset { call, next: 0 };
        damage::reset_damage(&mut answers, &mut mem, call.args[0]);
        assert_eq!(answers.next, call.inner.len(), "ResetDamage call {}: records left", call.index);
        check_windows(call, &mem, "CCarDamageHandler::ResetDamage");
    }
    eprintln!("CCarDamageHandler::ResetDamage: {} calls", calls.len());
}

/// Answers `Vf14x`'s material lookup from its records (the tag's entry is the record's eax).
struct RecordedVf14x<'a> {
    call: &'a Call,
}

impl Vf14xCalls for RecordedVf14x<'_> {
    fn tag_520940(&mut self, this: u32, id: u32) -> u32 {
        let inner = self
            .call
            .inner
            .iter()
            .find(|c| c.addr == TAG_520940)
            .unwrap_or_else(|| panic!("Vf14x call {}: no Tag520940 record", self.call.index));
        assert_eq!(inner.args, [this, id], "Vf14x call {}: Tag520940", self.call.index);
        inner.eax
    }
}

/// `CCarDamageHandler::Vf14x()` against its records: the flag-clear return (the manager), and the name of the tag's entry
/// when the flag is set. The records do not carry the manager's words, so the test maps the world global, the manager's
/// flag word (set when the record has the tag lookup), and the entry's word at +4 (the recorded name).
#[test]
fn vf14x_matches_original_calls() {
    let Some(calls) = calls(VF14X, "CCarDamageHandler::Vf14x") else {
        return;
    };
    const APP: u32 = 0x0A00_0000;
    for call in &calls {
        let mut mem = map_windows(call);
        let flag_set = call.inner.iter().any(|c| c.addr == TAG_520940);
        let manager = if flag_set { call.inner[0].args[0] - 0x18 } else { call.eax };
        mem.map(layout::WORLD, &APP.to_le_bytes());
        mem.map(APP + 0x24, &manager.to_le_bytes());
        mem.map(manager, &[0; 0x160E8]);
        mem.set_u32(manager + 0x160E0, if flag_set { 0x0100_0000 } else { 0 });
        if flag_set {
            let tag = call.inner[0].eax;
            mem.set_u32(tag + 4, call.eax);
        }
        let ours = damage::vf14x(&mut RecordedVf14x { call }, &mem, call.args[0]);
        assert_eq!(ours, call.eax, "Vf14x call {}: eax", call.index);
        check_windows(call, &mem, "CCarDamageHandler::Vf14x");
    }
    eprintln!("CCarDamageHandler::Vf14x: {} calls", calls.len());
}

/// Answers the light emit (0x4f2000) from the records, in call order: the emitter, and the payload's five words (the
/// signal window the record kept). `emitted` counts the emits answered.
struct RecordedLights<'a> {
    call: &'a Call,
    next: usize,
    emitted: usize,
}

impl LightCalls for RecordedLights<'_> {
    fn emit_light(&mut self, emitter: u32, payload: [u32; 5]) {
        let inner = next_record(self.call, &mut self.next, LIGHT_EMIT);
        self.emitted += 1;
        assert_eq!(inner.args[0], emitter, "light emit call {}: emitter", self.call.index);
        let (_, _, _, _, post) = inner_window(inner, 0);
        let words: Vec<u32> = post.chunks_exact(4).map(|w| u32::from_le_bytes(w.try_into().unwrap())).collect();
        assert_eq!(&words[..5], &payload, "light emit call {}: payload", self.call.index);
    }
}

/// The light emitter's manager (the global's value): the emitter the records name, less the manager's +0x9af0.
fn light_manager(call: &Call) -> u32 {
    call.inner.iter().find(|c| c.addr == LIGHT_EMIT).map_or(0x0A00_0000, |c| c.args[0] - 0x9AF0)
}

/// `CCarLightSystem::EmitLights(kind, lights, flags, scaled)` against its records (direct runs, 40 calls): the lights with
/// heads set emit their payloads, each checked against the signal window of its record.
#[test]
fn emit_lights_matches_original_calls() {
    let Some(calls) = calls(EMIT_LIGHTS, "CCarLightSystem::EmitLights") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        mem.map(SIGNAL_MANAGER, &light_manager(call).to_le_bytes());
        let mut answers = RecordedLights { call, next: 0, emitted: 0 };
        let a = &call.args;
        light::emit_lights(&mut answers, &mem, a[1], a[2], a[3], a[4]);
        let recorded = call.inner.iter().filter(|c| c.addr == LIGHT_EMIT).count();
        assert_eq!(answers.emitted, recorded, "EmitLights call {}: emits", call.index);
    }
    eprintln!("CCarLightSystem::EmitLights: {} calls", calls.len());
}

/// `CCarLightSystem::Update()` against its records (direct runs, 40 calls): the six gates, their emits in order (each
/// checked), and the flag words the gates clear (the flag block is in the window).
#[test]
fn update_matches_original_calls() {
    let Some(calls) = calls(UPDATE_LIGHTS, "CCarLightSystem::Update") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        mem.map(SIGNAL_MANAGER, &light_manager(call).to_le_bytes());
        let mut answers = RecordedLights { call, next: 0, emitted: 0 };
        light::update(&mut answers, &mut mem, call.args[0]);
        let recorded = call.inner.iter().filter(|c| c.addr == LIGHT_EMIT).count();
        assert_eq!(answers.emitted, recorded, "Update call {}: emits", call.index);
        check_windows(call, &mem, "CCarLightSystem::Update");
    }
    eprintln!("CCarLightSystem::Update: {} calls", calls.len());
}

/// Answers the body's slot-0 call from the records (when the world's flag gates it).
struct RecordedBody<'a> {
    call: &'a Call,
    next: usize,
}

impl BodyCalls for RecordedBody<'_> {
    fn slot0(&mut self, body: u32) {
        let inner = next_record(self.call, &mut self.next, BODY_SLOT0);
        assert_eq!(inner.args, [body], "scatter call {}: slot 0", self.call.index);
    }
}

/// Runs one scatter against its records: the points' lanes 0 to 2 (lane 3 is the original's stack word, not modelled), the
/// body's window, and the slot-0 call when the records have it. The edge loads the gate, the base and the points, and
/// stores the points back; the port returns nothing, so eax is checked as the original's `out`.
fn check_scatter(addr: u32, name: &str, run: impl Fn(&mut RecordedBody, bool, u32, Option<Vec3>, &mut [Vec4])) {
    let Some(calls) = calls(addr, name) else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        // The world global and its manager: the flag word (+0x160f4) has bit 0x1000 when the records have the slot-0 call.
        let (host, manager): (u32, u32) = (0x0A00_0000, 0x0A10_0000);
        mem.map(layout::WORLD, &host.to_le_bytes());
        mem.map(host + 0x24, &manager.to_le_bytes());
        mem.map(host + 0x38, &[0; 4]);
        mem.map(manager, &[0; 0x160F8]);
        let gated = call.inner.iter().any(|c| c.addr == BODY_SLOT0);
        mem.set_u32(manager + 0x160F4, if gated { 0x1000 } else { 0 });
        let [body, out, n] = [call.args[0], call.args[1], call.args[2]];
        let gate = mem.body_gate();
        let base = mem.body_base(body);
        let mut points = mem.load_points(out, n);
        let mut answers = RecordedBody { call, next: 0 };
        run(&mut answers, gate, body, base, &mut points);
        mem.store_points(out, &points);
        assert_eq!(out, call.eax, "{name} call {}: eax", call.index);
        let (_, _, _, _, post) = &call.windows[1];
        for (i, point) in post.chunks_exact(16).enumerate().take(n as usize) {
            for k in 0..3 {
                let theirs = u32::from_le_bytes(point[4 * k..4 * k + 4].try_into().unwrap());
                let ours = mem.u32(out + 16 * i as u32 + 4 * k as u32);
                assert_eq!(ours, theirs, "{name} call {} point {i} lane {k}", call.index);
            }
        }
        let (_, _, _, _, body_post) = &call.windows[0];
        assert_eq!(mem.read(body, body_post.len()), *body_post, "{name} call {}: body", call.index);
    }
    eprintln!("{name}: {} calls", calls.len());
}

/// `CCarBody::Vf9Cx(out, n)` against its records (direct runs, 40 calls): the points minus the base.
#[test]
fn vf9cx_matches_original_calls() {
    check_scatter(VF9CX, "CCarBody::Vf9Cx", |c, gate, body, base, points| {
        body::subtract_points(c, gate, body, base, points)
    });
}

/// `CCarBody::VfA0x(out, n)` against its records (direct runs, 40 calls): the points plus the base.
#[test]
fn vfa0x_matches_original_calls() {
    check_scatter(VFA0X, "CCarBody::VfA0x", |c, gate, body, base, points| {
        body::add_points(c, gate, body, base, points)
    });
}

/// Answers `UpdateA720`'s reads, externals and UpdateMrr from its records, in call order. Each record's windows are
/// applied (the car's words and the wheels' defs the external wrote). The reads load the seed, the wheel list and the
/// frame object from the memory as it is at the read; the frame stores go to the memory directly, and the root's
/// windows check them.
struct RecordedApplySeed<'a> {
    call: &'a Call,
    next: usize,
    mem: &'a mut Mem,
    car: u32,
    seed_at: u32,
}

impl SeedCalls for RecordedApplySeed<'_> {
    fn seed(&mut self) -> layout::Seed {
        layout::Seed::load(self.mem, self.seed_at)
    }

    fn wheels(&mut self) -> [u32; 4] {
        self.mem.wheel_pointers(self.car)
    }

    fn frame_object(&mut self) -> u32 {
        self.mem.frame_object(self.car)
    }

    fn call(&mut self, callee: u32, this: u32, args: &[u32]) {
        let inner = record_after(self.call, &mut self.next, callee);
        let expected: Vec<u32> = std::iter::once(this).chain(args.iter().copied()).collect();
        assert_eq!(inner.args, expected, "UpdateA720 call {}: {callee:#x}", self.call.index);
        for k in 0..inner.windows.len() {
            apply_changes(self.mem, inner, k);
        }
    }

    fn store_word(&mut self, at: u32, word: u32) {
        self.mem.set_u32(at, word);
    }

    fn update_mrr(&mut self, car: u32) {
        let inner = record_after(self.call, &mut self.next, UPDATE_MRR);
        assert_eq!(inner.args, [car], "UpdateA720 call {}: UpdateMrr", self.call.index);
        for k in 0..inner.windows.len() {
            apply_changes(self.mem, inner, k);
        }
    }
}

/// `CCar::UpdateA720(seed)` against its records (direct runs from the snapshot's car, 40 calls): the externals in order,
/// the frame stores (the car's +0x34 is the car, so they land in the car window), and the car after UpdateMrr.
#[test]
fn update_a720_matches_original_calls() {
    let Some(calls) = calls(UPDATE_A720, "CCar::UpdateA720") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let [car, seed_at] = [call.args[0], call.args[1]];
        let mut answers = RecordedApplySeed { call, next: 0, mem: &mut mem, car, seed_at };
        apply_seed::apply_seed(&mut answers, car);
        check_windows(call, &mem, "CCar::UpdateA720");
    }
    eprintln!("CCar::UpdateA720: {} calls", calls.len());
}

/// E60Flow::M5666b0 (direct records, g-records): the damage-select word, the host's +0x17854 when the host's first word
/// is 0x7b, else the global. The records hold the flow, the host's first word and its +0x17854, and the global.
#[test]
fn m5666b0_direct_matches_original_calls() {
    let Some(calls) = load(VAR, FLOW_CHECK) else {
        eprintln!("skipping M5666b0 direct: {VAR} not set");
        return;
    };
    let (mut host_word, mut global) = (0, 0);
    for call in &calls {
        let mem = map_windows(call);
        let flow = call.args[0];
        assert_eq!(mem.damage_select(flow), call.eax, "M5666b0 call {}: eax", call.index);
        if mem.read(mem.u32(flow + 0xC98), 2) == [0x7B, 0] {
            host_word += 1;
        } else {
            global += 1;
        }
    }
    assert!(host_word + global > 0, "{VAR} has no M5666b0 calls");
    eprintln!("M5666b0: {} calls match, {host_word} from the host, {global} from the global", calls.len());
}

/// E60Flow::M566640 (direct records, g-flow): the flow check, 1 when the flow holds. The records hold the flow's window,
/// the host's tag and words, and the mode global with the word its pointer holds.
#[test]
fn flow_566640_direct_matches_original_calls() {
    // E60Flow::M5665f0 (0x5665f0) is byte-identical to M566640: the same port answers both.
    for addr in [FLOW_CHECK_566640, 0x5665F0] {
        let Some(calls) = load(VAR, addr) else {
            eprintln!("skipping flow {addr:#x} direct: {VAR} not set");
            return;
        };
        assert!(!calls.is_empty(), "{VAR} has no {addr:#x} calls");
        let mut holds = 0;
        for call in &calls {
            let mem = map_windows(call);
            let eax = mem.flow_holds(call.args[0]);
            assert_eq!(eax, call.eax, "{addr:#x} call {}: eax", call.index);
            holds += eax;
        }
        eprintln!("{addr:#x}: {} calls match, {holds} hold", calls.len());
    }
}

/// Answers `CCar::Collide6000`'s nested calls from the record, in order, and maps each one's windows into the shared
/// memory the port reads (the grid's flags). The grid is EdgeEdge's first argument: the record of the grid getter (its
/// vtable slot 0x118) is not kept.
struct Collide6000Recorded<'a> {
    inner: &'a [Inner],
    mem: Mem,
    grid: u32,
    next: usize,
}

impl Collide6000Recorded<'_> {
    fn take(&mut self, addr: u32, args: &[u32]) -> u32 {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("Collide6000 missing nested {addr:#x}"));
        assert_eq!((call.addr, call.args.as_slice()), (addr, args), "Collide6000 nested call {}", self.next);
        self.next += 1;
        for (_, at, _, _, post) in &call.windows {
            self.mem.map(*at, post);
        }
        call.eax
    }
}

impl layout::Collide6000Calls for Collide6000Recorded<'_> {
    fn grid(&mut self, _sp: u32, _car: u32, _target: u32) -> u32 {
        self.grid
    }

    fn edge_edge(&mut self, _sp: u32, grid: u32, args: [u32; 6]) -> u32 {
        let mut expected = vec![grid];
        expected.extend(args);
        self.take(0x4f8280, &expected)
    }
}

#[test]
fn car_collide6000_matches_direct_records() {
    let Some(calls) = load(VAR, 0x4e6000) else {
        eprintln!("skipping CCar::Collide6000: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        // The EdgeEdge record's first pointer is the body's v40 (at body + 0x30): it fixes the body's esp.
        let edge = call.inner.iter().find(|i| i.addr == 0x4f8280).expect("Collide6000 EdgeEdge record");
        let frame = edge.args[1] - 0x30 + 0x64;
        let mut host = Collide6000Recorded { inner: &call.inner, mem: mem.clone(), grid: edge.args[0], next: 0 };
        let [car, a, b, c] = call.args[..4].try_into().expect("Collide6000 four words");
        // The grid getter's slot is read from the car's vtable, which the windows do not cover (its target is not checked).
        mem.map(mem.u32(car) + 0x118, &[0; 4]);
        layout::collide6000(&mut mem, car, a, b, c, frame, &mut host);
        assert_eq!(host.next, call.inner.len(), "Collide6000 nested calls");
        check_windows(call, &mem, "CCar::Collide6000");
    }
    eprintln!("CCar::Collide6000: {} direct calls match", calls.len());
}

const LANE_TEST: u32 = 0x4E7910;
const ASSIST_DF0: u32 = 0x4E8DF0;
const VF158X: u32 = 0x4E5EE0;
const RETIRE_A: u32 = 0x4E8FA0;
const EMIT_89B0: u32 = 0x4E89B0;
const UPDATE_SEED: u32 = 0x4E91F0;
/// The entry esp the retire and assist tests give the port (the frames are below it).
const ENTRY: u32 = 0x0080_0000;

/// `CCar::Test7910` (0x4e7910) against its records: BlendB's point comes from the drive table the records carry, and the
/// ToWorld answer is the record's (its input is (0, -1, 0)). The verdict is the record's eax.
#[test]
fn lane_test_matches_original_calls() {
    let Some(calls) = calls(LANE_TEST, "CCar::Test7910") else {
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let mut next = 0;
        let verdict = layout::lane_test(&mem, call.args[0], |v| {
            let inner = record_after(call, &mut next, TO_WORLD);
            // The inner windows are renumbered from 0 in the spec's order: the input, then the output.
            assert!(vec3_of(&inner_window(inner, 0).3) == v, "Test7910 call {}: ToWorld input", call.index);
            vec3_of(&inner_window(inner, 1).4)
        });
        assert_eq!(verdict, Some(call.eax), "Test7910 call {}: eax", call.index);
    }
    eprintln!("CCar::Test7910: {} calls", calls.len());
}

/// `Assist8DF0`'s calls answered from its records: the lane test's eax (kept for the code), the outside emit (its word is
/// checked against the code the asm picks), the legacy S158 answer and UpdateSeed and SetDirty by address.
struct RecordedAssist<'a> {
    call: &'a Call,
    next: usize,
    lane: u32,
}

impl AssistCalls for RecordedAssist<'_> {
    fn lane_test(&mut self, car: u32) -> u32 {
        let inner = record_after(self.call, &mut self.next, LANE_TEST);
        assert_eq!(inner.args[0], car, "Assist8DF0 call {}: Test7910 this", self.call.index);
        self.lane = inner.eax;
        inner.eax
    }

    fn emit_outside(&mut self, _code_at: u32) {
        let inner = record_after(self.call, &mut self.next, EMIT_89B0);
        let (_, _, _, pre, _) = inner_window(inner, 1);
        let code = u32::from_le_bytes(pre[..4].try_into().unwrap());
        let car = self.call.args[0];
        let partial = f32::from_le_bytes(window_at(self.call, car, 0x9E8)) > 1.0;
        let expected = if partial { 1 } else { 3 - u32::from(self.lane != 0) };
        assert_eq!(code, expected, "Assist8DF0 call {}: outside code", self.call.index);
    }

    fn s158(&mut self, _car: u32) {}

    fn update_seed(&mut self, car: u32, _block: u32) {
        let inner = record_after(self.call, &mut self.next, UPDATE_SEED);
        assert_eq!(inner.args[0], car, "Assist8DF0 call {}: UpdateSeed this", self.call.index);
    }

    fn set_dirty(&mut self, state: u32) {
        let inner = record_after(self.call, &mut self.next, SET_DIRTY);
        assert_eq!(inner.args[0], state, "Assist8DF0 call {}: SetDirty this", self.call.index);
    }
}

/// The four bytes at `car + off` in the call's car window, before (`pre`) the call.
fn window_at(call: &Call, car: u32, off: u32) -> [u8; 4] {
    let (_, at, _, pre, _) = call.windows.iter().find(|w| w.0 == 0).expect("the car window");
    let start = (car + off - at) as usize;
    pre[start..start + 4].try_into().unwrap()
}

/// `CCar::Assist8DF0` against its records: the words it writes (+0x15e4, +0x1670, the probe at +0x1680, +0x9f0, +0xaec)
/// after the port are the original's, and its calls take the records' answers in order.
#[test]
fn assist_8df0_matches_original_calls() {
    let Some(calls) = calls(ASSIST_DF0, "CCar::Assist8DF0") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let car = call.args[0];
        let mut answers = RecordedAssist { call, next: 0, lane: 0 };
        layout::assist_8df0(&mut answers, &mut mem, car, ENTRY, call.args[1], call.args[2], call.args[3]);
        let post = call.windows.iter().find(|w| w.0 == 0).map(|w| w.4.clone()).expect("the car window");
        for off in [0x15E4, 0x1670, 0x1680, 0x1684, 0x1688, 0x168C, 0x9F0, 0xAEC] {
            let theirs = u32::from_le_bytes(window_post(call, car, off, &post));
            assert_eq!(mem.u32(car + off), theirs, "Assist8DF0 call {}: +{off:#x}", call.index);
        }
    }
    eprintln!("CCar::Assist8DF0: {} calls", calls.len());
}

struct RecordedVf158<'a> {
    call: &'a Call,
    next: usize,
    entry: u32,
}

impl layout::Vf158xCalls for RecordedVf158<'_> {
    fn slot0(&mut self, mem: &Mem, target: u32, _sp: u32, car: u32) {
        let inner = record_after(self.call, &mut self.next, target);
        assert_eq!(target, 0x519950, "Vf158x call {}: slot 0 target", self.call.index);
        assert_eq!(inner.args, [car], "Vf158x call {}: slot 0 this", self.call.index);
        assert_eq!(inner.eax, entity_layout::entity_vf00(mem, car), "Vf158x call {}: slot 0 return", self.call.index);
    }

    fn solid_vf158(&mut self, mem: &mut Mem, _sp: u32, car: u32) -> u32 {
        assert_eq!(self.call.addr, 0x4E5EE0);
        assert_eq!(self.call.args, [car]);
        let eax = self.call.eax;
        let windows = self.call.windows.as_slice();
        let mut host = RecordedSolidVf158 { call: self.call, next: self.next, index: self.call.index };
        let frame = ((self.entry - 4) & !0xF) - 0x64;
        let result = solid_layout::solid_vf158(mem, &mut host, car, frame).expect("recorded E43Vf158 has all calls");
        self.next = host.next;
        assert_eq!(result, eax, "Vf158x call {}: solid eax", self.call.index);
        check_window_list(windows, mem, "CSolid::E43Vf158");
        result
    }
}

struct RecordedSolidVf158<'a> {
    call: &'a Call,
    next: usize,
    index: usize,
}

impl solid_layout::VerySoftHost for RecordedSolidVf158<'_> {
    fn slot0(
        &mut self,
        _mem: &Mem,
        _sp: u32,
        _this: u32,
    ) -> Result<(), rbr_physics::rigid_body::host::HostUnavailable> {
        panic!("Vf158x call {}: parentful fixture called slot 0", self.index);
    }

    fn entity_vf158(
        &mut self,
        mem: &mut Mem,
        sp: u32,
        this: u32,
    ) -> Result<(), rbr_physics::rigid_body::host::HostUnavailable> {
        let inner = record_after(self.call, &mut self.next, 0x519AA0);
        assert_eq!(inner.args, [this], "Vf158x call {}: entity this", self.index);
        let parent = mem.u32(this + 0x44);
        assert_ne!(parent, 0, "Vf158x call {}: expected parent branch", self.index);
        let parent_vtable = mem.u32(parent);
        let target = mem.u32(parent_vtable + 0x158);
        assert_eq!(mem.u8(target), 0xC3, "Vf158x call {}: parent slot-86 return stub", self.index);
        let mut relay = RecordedEntityVf158 { inner, index: self.index, object: this, parent, target };
        let eax = entity_layout::entity_vf158(mem, &mut relay, this, sp).expect("recorded parent slot-86 relay");
        assert_eq!(eax, inner.eax, "Vf158x call {}: entity eax", self.index);
        Ok(())
    }

    fn emit(
        &mut self,
        _mem: &mut Mem,
        _frame: u32,
        event: &BodyEvent,
    ) -> Result<u32, rbr_physics::rigid_body::host::HostUnavailable> {
        let inner = record_after(self.call, &mut self.next, 0x51F7C0);
        let (_, _, _, bytes, _) = inner_window(inner, 0);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), event.kind);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), event.owner);
        for (off, values) in [
            (0x10, [event.position.x, event.position.y, event.position.z, event.position.w]),
            (0x20, [event.orientation.x, event.orientation.y, event.orientation.z, event.orientation.w]),
            (0x30, [event.position_again.x, event.position_again.y, event.position_again.z, event.position_again.w]),
            (
                0x40,
                [
                    event.orientation_again.x,
                    event.orientation_again.y,
                    event.orientation_again.z,
                    event.orientation_again.w,
                ],
            ),
        ] {
            for (lane, value) in values.into_iter().enumerate() {
                let got = u32::from_le_bytes(bytes[off + lane * 4..off + lane * 4 + 4].try_into().unwrap());
                assert_eq!(got, value.to_bits(), "Vf158x call {}: event +{off:#x}+{lane}", self.index);
            }
        }
        Ok(inner.eax)
    }
}

struct RecordedEntityVf158<'a> {
    inner: &'a Inner,
    index: usize,
    object: u32,
    parent: u32,
    target: u32,
}

impl entity_layout::FilingHost for RecordedEntityVf158<'_> {
    fn filing(
        &mut self,
        _mem: &Mem,
        _object: u32,
    ) -> Result<rbr_physics::physics_core::signal::Filing, rbr_physics::rigid_body::host::HostUnavailable> {
        panic!("Vf158x call {}: parent path requested filing", self.index)
    }
}

impl entity_layout::EntityVf158Host for RecordedEntityVf158<'_> {
    fn slot158(
        &mut self,
        _sp: u32,
        target: u32,
        parent: u32,
    ) -> Result<u32, rbr_physics::rigid_body::host::HostUnavailable> {
        assert_eq!(self.inner.args, [self.object], "Vf158x call {}: entity this", self.index);
        assert_eq!(parent, self.parent, "Vf158x call {}: parent slot receiver", self.index);
        // The fixture's parent vtable resolves this call to a one-byte `ret` stub.
        assert_eq!(target, self.target, "Vf158x call {}: parent slot target", self.index);
        Ok(self.inner.eax)
    }
}

/// `CCar::Vf158x`'s direct records include the slot-0 call, the `CSolid::E43Vf158` body, its entity relay and event.
#[test]
fn vf158x_matches_original_calls() {
    let Some(calls) = calls(VF158X, "CCar::Vf158x") else {
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        let mut host = RecordedVf158 { call, next: 0, entry: ENTRY };
        let eax = layout::vf158x(&mut host, &mut mem, call.args[0], ENTRY);
        assert_eq!(eax, call.eax, "Vf158x call {}: eax", call.index);
        assert_eq!(host.next, call.inner.len(), "Vf158x call {}: records left", call.index);
        check_windows(call, &mem, "CCar::Vf158x");
    }
    eprintln!("CCar::Vf158x: {} calls", calls.len());
}

/// `CSolid::E43Vf158`'s own records run the same body path independently of the car's tail-call wrapper.
#[test]
fn solid_vf158_matches_original_calls() {
    let Some(mut calls) = calls(0x52c030, "CSolid::E43Vf158") else {
        return;
    };
    // Direct-call captures of UpdateSeed record the external slot call as a bare boundary, without its call tree.
    // The solid's own vectors and Vf158x tail-call vectors have the child records needed for a layout replay.
    calls.retain(|call| !call.inner.is_empty());
    for call in &calls {
        let mut mem = shared_windows(call);
        let mut host = RecordedSolidVf158 { call, next: 0, index: call.index };
        let eax = solid_layout::solid_vf158(&mut mem, &mut host, call.args[0], ((ENTRY - 4) & !0xF) - 0x64)
            .expect("recorded E43Vf158 has all calls");
        assert_eq!(eax, call.eax, "E43Vf158 call {}: eax", call.index);
        assert_eq!(host.next, call.inner.len(), "E43Vf158 call {}: records left", call.index);
        check_windows(call, &mem, "CSolid::E43Vf158");
    }
    eprintln!("CSolid::E43Vf158: {} calls", calls.len());
}

/// The four bytes at `car + off` in the call's car window, after (`post`) the call.
fn window_post(call: &Call, car: u32, off: u32, post: &[u8]) -> [u8; 4] {
    let at = call.windows.iter().find(|w| w.0 == 0).unwrap().1;
    let start = (car + off - at) as usize;
    post[start..start + 4].try_into().unwrap()
}

/// The retire announcer's calls answered from its records: the retire emit (its code word is 3) and Assist8DF0 with the
/// arguments the flow check picked. The flow check is the port's own, on the mapped flow object.
struct RecordedRetire<'a> {
    call: &'a Call,
    next: usize,
}

impl RetireCalls for RecordedRetire<'_> {
    fn emit_retire(&mut self, _code_at: u32) {
        let inner = record_after(self.call, &mut self.next, EMIT_770);
        let (_, _, _, pre, _) = inner_window(inner, 1);
        assert_eq!(
            u32::from_le_bytes(pre[..4].try_into().unwrap()),
            3,
            "RetireA call {}: the retire code",
            self.call.index
        );
    }

    fn assist(&mut self, car: u32, a: u32, b: u32, c: u32) {
        let inner = record_after(self.call, &mut self.next, ASSIST_DF0);
        assert_eq!(inner.args, [car, a, b, c], "RetireA call {}: Assist8DF0", self.call.index);
    }
}

/// `E34_RetireA` against its records: the emit, the flow check's branch (the port's `flow_holds` on the mapped flow
/// object) and the Assist8DF0 arguments that branch picks.
#[test]
fn retire_a_matches_original_calls() {
    let Some(calls) = calls(RETIRE_A, "E34_RetireA") else {
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let mut answers = RecordedRetire { call, next: 0 };
        layout::retire_announce(&mut answers, &mut mem, call.args[0], ENTRY);
    }
    eprintln!("E34_RetireA: {} calls", calls.len());
}

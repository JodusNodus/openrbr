//! Replays the original's rigid-body calls (`harness/vcu_vectors.py --set rigid-body`) through the ports in
//! `rbr_physics::rigid_body`. Needs record files: `RBR_RIGID_BODY_VECTORS=<file>[:<file>...]`. Without the variable
//! the tests are skipped.

mod common;

use common::{
    check_window_list, check_windows, load, map_window_list, map_windows, parse, shared_windows, Call, Inner, Window,
};
use rbr_physics::collision::layout::body_hosts::Vf40Host;
use rbr_physics::fpu::Precision;
use rbr_physics::math::quat::{from_quat, mul, normalize, Quat};
use rbr_physics::mem::Mem;
use rbr_physics::physics_core::layout::construct::Heap;
use rbr_physics::physics_core::layout::Filing;
use rbr_physics::physics_core::layout::FilingHost;
use rbr_physics::rigid_body::contact::{contact_step, ContactHost, Impulse, Input, SampleHook};
use rbr_physics::rigid_body::contact_list::{contact_list_step, Event, ListHost, Record};
use rbr_physics::rigid_body::dot::{dot60, e43vf138, Dot138};
use rbr_physics::rigid_body::emit::{emit_body_event, BodyEvent, EmitHost, SLEEP};
use rbr_physics::rigid_body::force::{accumulate_force, force_pass, ForceHost};
use rbr_physics::rigid_body::host::{BodyCalls, CellHost, HostUnavailable, StateHost};
use rbr_physics::rigid_body::host::{SolidCtorHost, Vf20Host, Vf54Host};
use rbr_physics::rigid_body::impulse::{impulse, ImpulseHost};
use rbr_physics::rigid_body::integrate::{integrate, IntegrateHost, StateSignal};
use rbr_physics::rigid_body::layout::{
    cdef_new_solid, construct_state, contact_vote, entity_vf20_jump, load_contact, new_def, refresh_state_rotation,
    scale_add_rows, scratch_reset_after_entity, solid_def_ctor, solid_filing, solid_sleep, vf16_helper, RestHost,
};
use rbr_physics::rigid_body::layout::{
    clamp_velocity_along, def_flag, fold_body_rows, fold_body_rows_reordered, fold_quat_body_rows_c4,
    fold_quat_body_rows_c8, point_velocity, quat_body_rows_b8, quat_diff_body_rows, quat_plus_body_rows,
    quat_transform_body_rows, solid_e43vfa0, solid_inverse_mass, store_scratch_base_init, store_scratch_reset,
    transform_body_rows, transform_body_rows3, Body, Def, ParentFrame, Vec4,
};
use rbr_physics::rigid_body::layout::{impulse_tail, ring_append, ImpulseTailCalls};
use rbr_physics::rigid_body::pose::{pose_step, PoseHost};
use rbr_physics::rigid_body::response::{response, ResponseHost};
use rbr_physics::rigid_body::rotation::MulHost;
use rbr_physics::rigid_body::scratch::{accumulate, FlushHost, FLUSH_BIT};
use rbr_physics::rigid_body::sleep;
use rbr_physics::rigid_body::vote::{probe_ground, ContactVoteHost, Vote, VoteHost};
use rbr_physics::rigid_body::window::{advance_rest, back_step};
use std::collections::VecDeque;
use std::ops::Range;

const VAR: &str = "RBR_RIGID_BODY_VECTORS";
const VF24X: u32 = 0x52B740;
const VFDX: u32 = 0x52CFD0;
const BIG_A: u32 = 0x52B9F0;
const SLOT80: u32 = 0x51E5C0;
const SAMPLE_SINK: u32 = 0x4E6000;
/// The body's slot-80 target in BigB's dumps (`CSolid::E43Vf140`): the response there is the same call as `SLOT80`.
const E43_VF140: u32 = 0x52A570;
const MB560: u32 = 0x51B560;
const BIG_B: u32 = 0x52C220;
const E36_COLLECT: u32 = 0x4F3950;
const RETIRE_A: u32 = 0x4E8FA0;
const EMIT: u32 = 0x52BE70;
/// The body's slot 28 (`S28`, the force apply) and slot 29 (`S29`, the torque apply) in BigB's dumps.
const APPLY_FORCE: u32 = 0x529340;
const APPLY_TORQUE: u32 = 0x529EA0;
/// `CSolid::Dot60` (0x52a500, x87) and `CSolid::E43Vf138` (slot 78, 0x52b990): the dot pair `dot::e43vf138` ports.
const DOT60: u32 = 0x52A500;
const E43_VF138: u32 = 0x52B990;
const STEP77: u32 = 0x529D30;
/// The vote Step77 makes (S00 of the global at 0x7c755c).
const VOTE: u32 = 0x4FE810;
/// CSolid::Step76 (0x529c80): the velocity of a point, and CSolid::Vf85 (0x52b870): the definition's flag word.
const STEP76: u32 = 0x529C80;
const VF85: u32 = 0x52B870;
/// The body's slot 0 (`CEntity::Vf00`, 0x519950), which `E43Vf74` runs when its gate has bit 0x4000.
const SLOT0_FLUSH: u32 = 0x519950;
/// The window index of BigA's tuning node (after its five argument windows and the definition at `this+0x34`).
const BIG_A_TUNING: usize = 7;
const VFEX: u32 = 0x52D110;
const M83DD0: u32 = 0x483DD0;
const E18_NORMALIZE: u32 = 0x4921A0;
const E34_NORM: u32 = 0x4E4C70;
const VFC8: u32 = 0x52C8F0;
const INTEGRATE: u32 = 0x52CC30;
const ENTITY_WALK: u32 = 0x51A330;
/// `CEntity::E43Vf64` (0x51a2c0): the gated walk the force pass starts with.
const ENTITY_WALK_VF64: u32 = 0x51A2C0;
const FORCE_PASS: u32 = 0x52CB20;
const E43_VF90: u32 = 0x52BF30;
const S18: u32 = 0x529DC0;
const ENTITY_NOTIFY: u32 = 0x519A60;
const EVENT_EMIT: u32 = 0x51F7C0;
const E43_VF140_DIRECT: u32 = 0x52A570;
const E43_VF128: u32 = 0x52A0E0;
const MOUNTED_VF128: u32 = 0x521B20;
const E46_IMPULSE640: u32 = 0x529640;
const E43_VF13C: u32 = 0x5297D0;
const ACCUM_340: u32 = 0x529340;
const SCRATCH_VF04: u32 = 0x529AF0;
const SCRATCH_BASE_INIT: u32 = 0x52B930;
const SET_FROM_QUAT: u32 = 0x43C6E0;
const E43_VFAC: u32 = 0x52AD40;
const E43_VFA0: u32 = 0x52AD10;
const E43_VFB0: u32 = 0x52ADD0;
const E43_VFBC: u32 = 0x52AEC0;
const E43_VFC0: u32 = 0x52AFD0;
const E43_VFA4: u32 = 0x52B110;
const E43_VFA8: u32 = 0x52B1F0;
const E43_VFB4: u32 = 0x52B2E0;
const E43_VFB8: u32 = 0x52B380;
const E43_VFC4: u32 = 0x52B460;
const E43_VFC8: u32 = 0x52B580;
const VF17X: u32 = 0x529A50;
const VF54_HELPER: u32 = 0x52B6B0;
const E46_MAT_MUL: u32 = 0x528C20;
const VF39X: u32 = 0x52AC80;
const VF16_HELPER: u32 = 0x52B6D0;
const CSTATE_UNK44: u32 = 0x4E33E0;
const WALK_VF40: u32 = 0x51A4B0;
const NEW_DEF: u32 = 0x52B8B0;
const OPERATOR_NEW: u32 = 0x70BEC2;
const CDEF_CTOR: u32 = 0x529840;
const STATE_EMIT: u32 = 0x51F700;
const SLOT37: u32 = 0x52C130;
const VF81X: u32 = 0x52C510;
const LOOKUP_TWIN: u32 = 0x4E03B0;
const APPEND_CELL: u32 = 0x4EA980;
const SAMPLE_BIG: u32 = 0x50B300;
const COMPOSE_A: u32 = 0x51B4C0;
/// The body's link node, written by `CCar::AppendA980` (`this+0x160`, 12 bytes). Not modelled by the port.
const CELL_LINK: Range<usize> = 0x160..0x16C;
/// The body's bytes at +0x14..+0x18 that a trip changes through the caller's cell removal (slot 37 calls 0x519a80,
/// which calls 0x5276a0 when the body has no parent). Not ported: the trip's window check skips them.
const CELL_REMOVAL: Range<usize> = 0x14..0x19;
/// The body bytes the state refresh (`CSolid::Vf40`, slot 16) writes in the original, through the walkers it calls:
/// the entity matrix at +0x80..+0xa0 and the E43 matrix at +0x110..+0x140. Not ported (the host's work).
const REFRESH_WRITES: [Range<usize>; 2] = [0x80..0xa0, 0x110..0x140];

/// Checks the body window (window 0) of `call` against the original, except the `masked` bytes. The other windows
/// must match exactly.
fn check_except(call: &Call, mem: &Mem, name: &str, masked: &[Range<usize>]) {
    for Window { index: k, at, len, after: post, .. } in &call.windows {
        let now = mem.read(*at, *len);
        for (i, (ours, theirs)) in now.iter().zip(post).enumerate() {
            assert!(
                *k == 0 && masked.iter().any(|r| r.contains(&i)) || ours == theirs,
                "{name} call {} window {k} byte {i:#x}: {ours:#04x} vs {theirs:#04x}",
                call.index
            );
        }
    }
}

#[test]
fn vf24x_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VF24X) else {
        eprintln!("skipping vf24x: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let def_at = mem.u32(this + 0x34);
        let def = Def::load(&mem, def_at);
        let mut body = Body::load(&mem, this);
        let dt = f32::from_bits(call.args[1]);
        let slept = sleep::step(&mut body, &def, dt);
        if slept {
            sleep::deactivate(&mut body); // the caller's slot 37 call
        }
        body.store(&mut mem, this);
        assert_eq!(slept as u32, call.eax, "Vf24x call {} ({})", call.index, call.branch);
        if slept {
            check_except(call, &mem, "Vf24x trip", &[CELL_REMOVAL]);
        } else {
            check_windows(call, &mem, "Vf24x");
        }
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("Vf24x: {} calls match, by branch: {branches:?}", calls.len());
}

/// A quaternion at `at` in the game's memory (`x, y, z, w`).
fn load_quat(mem: &Mem, at: u32) -> Quat {
    Quat { x: mem.f32(at), y: mem.f32(at + 0x4), z: mem.f32(at + 0x8), w: mem.f32(at + 0xc) }
}

fn store_quat(mem: &mut Mem, at: u32, q: Quat) {
    mem.set_f32(at, q.x);
    mem.set_f32(at + 0x4, q.y);
    mem.set_f32(at + 0x8, q.z);
    mem.set_f32(at + 0xc, q.w);
}

#[test]
fn quat_mul_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, M83DD0, "M83DD0", |call, mem| {
        let (p, q) = (load_quat(mem, call.args[0]), load_quat(mem, call.args[1]));
        store_quat(mem, call.args[2], mul(p, q));
        assert_eq!(call.eax, call.args[2], "M83DD0 call {} returns out", call.index);
    });
}

#[test]
fn normalize_in_place_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E18_NORMALIZE, "E18_QuatNormalize", |call, mem| {
        let q = load_quat(mem, call.args[0]);
        store_quat(mem, call.args[0], normalize(q));
        assert_eq!(call.eax, call.args[0], "E18_QuatNormalize call {} returns q", call.index);
    });
}

#[test]
fn normalize_to_output_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E34_NORM, "E34Quat::Norm", |call, mem| {
        let v = load_quat(mem, call.args[0]);
        store_quat(mem, call.args[1], normalize(v));
        assert_eq!(call.eax, call.args[1], "E34Quat::Norm call {} returns out", call.index);
    });
}

/// Replays a call's recorded callees, in order, as the pose step's host. Each host call must be the next record, with
/// the arguments the port passes. The parent transform's result is not in any window, so the port gets zeros.
struct Replay<'a> {
    inner: &'a [Inner],
    next: usize,
    this: u32,
}

/// `E09Vec4::Clear` records made inside the cell code (`LookupTwin` and `AppendA980` call it on the link and the
/// cell's nodes). VfC8 itself never calls it, so the replay skips these records.
const VEC4_CLEAR: u32 = 0x465D80;
/// `E01Vec4::CopyFrom` records: the root path copies the position into stack temps before `LookupTwin`. The port makes
/// no host call for them, so the replay skips them (their bytes are not checked here).
const VEC4_COPY: u32 = 0x43C5B0;
/// `CFloat3::Length` in the sample path. The `SampleBig` argument is the body's `+0x17c` (not this result), so the port
/// does not make the call and the replay skips it.
const VEC3_LENGTH: u32 = 0x473B30;

/// The records the replay skips: the cell code's `Clear`s, the pose step's copies and its sample-path `Length`.
fn is_local(addr: u32) -> bool {
    addr == VEC4_CLEAR || addr == VEC4_COPY || addr == VEC3_LENGTH
}

impl Replay<'_> {
    /// The next recorded call that is not a cell-code `Clear`, which must be `addr`.
    fn take(&mut self, addr: u32) -> &Inner {
        while self.inner.get(self.next).is_some_and(|r| is_local(r.addr)) {
            self.next += 1;
        }
        let record = self.inner.get(self.next).expect("the port made more calls than the original");
        assert_eq!(record.addr, addr, "callee {} of the original", self.next);
        self.next += 1;
        record
    }

    /// The first recorded call of `addr` from the next record on, whose first argument is `this` when it is given. The
    /// records before it are the calls the port does not make itself: a walk's inner calls and the walks of the child
    /// bodies, which return before their caller does.
    fn take_through(&mut self, addr: u32, this: Option<u32>) -> &Inner {
        let inner = self.inner;
        let at = inner[self.next..]
            .iter()
            .position(|r| r.addr == addr && this.is_none_or(|t| r.args.first() == Some(&t)))
            .unwrap_or_else(|| panic!("the original's call {addr:#x} after record {}", self.next));
        self.next += at + 1;
        &inner[self.next - 1]
    }

    fn done(&self) {
        let left: Vec<u32> = self.inner[self.next..].iter().map(|r| r.addr).collect();
        assert!(left.iter().all(|&addr| is_local(addr)), "the original made calls the port did not: {left:x?}");
    }
}

impl CellHost for Replay<'_> {
    fn register_cell(&mut self, link: u32, x: f32, y: f32) -> Result<(), HostUnavailable> {
        let lookup = self.take(LOOKUP_TWIN);
        assert_eq!((lookup.args[1], lookup.args[2]), (x.to_bits(), y.to_bits()), "LookupTwin arguments");
        let hit = lookup.eax;
        let append = self.take(APPEND_CELL);
        assert_eq!((append.args[0], append.args[1]), (hit, self.this + link), "AppendA980 arguments");
        Ok(())
    }
}

impl PoseHost for Replay<'_> {
    fn to_parent_frame(&mut self, _local: Vec4) -> Vec4 {
        // The parent's slot 0x30 (its target is the recorded dispatch), then the matrix step.
        let s30 = self.inner.iter().skip(self.next).find(|r| !is_local(r.addr)).map(|r| r.addr);
        self.take(s30.expect("the original's parent transform"));
        self.take(COMPOSE_A);
        Vec4::default()
    }

    fn sample_big(&mut self, _at: Vec4, value: f32) -> f32 {
        let sample = self.take(SAMPLE_BIG);
        assert_eq!(sample.args[2], value.to_bits(), "SampleBig argument");
        f64::from_bits(sample.st0_bits) as f32
    }
}

impl IntegrateHost for Replay<'_> {
    fn entity_walk(&mut self, body: &Body, dt: f32) -> Body {
        let walk = self.take_through(ENTITY_WALK, Some(self.this));
        assert_eq!(walk.args[1], dt.to_bits(), "the entity walk's dt");
        *body
    }

    fn sleep_trip(&mut self, body: &Body) -> Result<Body, HostUnavailable> {
        self.take_through(SLOT37, Some(self.this));
        Ok(*body)
    }

    fn emit_state(&mut self, state: &StateSignal) -> Result<(), HostUnavailable> {
        let emit = self.take_through(STATE_EMIT, None);
        let payload = &emit.windows[0].after;
        let orientation =
            Vec4 { x: state.orientation.x, y: state.orientation.y, z: state.orientation.z, w: state.orientation.w };
        assert_eq!(payload[..4], state.owner.to_le_bytes(), "the state signal's owner");
        assert_eq!(payload[0x10..0x20], vec4_bytes(state.position)[..], "the state signal's position");
        assert_eq!(payload[0x20..0x30], vec4_bytes(orientation)[..], "the state signal's orientation");
        Ok(())
    }
}

#[test]
fn integrate_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, INTEGRATE) else {
        eprintln!("skipping E43Vf6C: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let def = Def::load(&mem, mem.u32(this + 0x34));
        let mut body = Body::load(&mem, this);
        // The port clears a slow flagged body below NGP's squared speed 4e-7, 1.02 below 1e-5: a record whose speed
        // after the pass lies between the two is 1.02's alone.
        let dt = f32::from_bits(call.args[1]);
        let band = |v: Vec4, a: Vec4| {
            let (x, y, z) = ((a.x * dt) + v.x, (a.y * dt) + v.y, (a.z * dt) + v.z);
            let sq = (z * z + y * y) + x * x;
            f32::from_bits(0x34D6_BF94) <= sq && sq < 1e-5
        };
        if body.body_flags & 8 != 0
            && (band(body.linear_velocity, body.linear_accel) || band(body.angular_velocity, body.angular_accel))
        {
            continue;
        }
        let parent = (body.parent != 0).then(|| ParentFrame::load(&mem, body.parent));
        let mut host = Replay { inner: &call.inner, next: 0, this };
        integrate(&mut body, &def, f32::from_bits(call.args[1]), parent, &mut host)
            .expect("E43Vf6C: the replay host answers every call");
        host.done();
        body.store(&mut mem, this);
        // Masked: the entity walk's state refresh, the root cell link, and the cell removal of a sleep trip.
        let mut masked = REFRESH_WRITES.to_vec();
        if parent.is_none() {
            masked.push(CELL_LINK);
        }
        if call.inner.iter().any(|r| r.addr == SLOT37) {
            masked.push(CELL_REMOVAL);
        }
        check_except(call, &mem, "E43Vf6C", &masked);
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("E43Vf6C: {} calls match, by branch: {branches:?}", calls.len());
}

#[test]
fn pose_step_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VFC8) else {
        eprintln!("skipping VfC8: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let mut body = Body::load(&mem, this);
        let parent = (body.parent != 0).then(|| ParentFrame::load(&mem, body.parent));
        let mut host = Replay { inner: &call.inner, next: 0, this };
        pose_step(&mut body, f32::from_bits(call.args[1]), call.args[2] != 0, call.args[3] != 0, parent, &mut host)
            .expect("VfC8: the replay host answers every call");
        host.done();
        body.store(&mut mem, this);
        // Root bodies: the link node is the host's (CCar::AppendA980). Every other byte must match.
        let masked: &[Range<usize>] = if parent.is_none() { &[CELL_LINK] } else { &[] };
        check_except(call, &mem, "VfC8", masked);
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("VfC8: {} calls match, by branch: {branches:?}", calls.len());
}

/// The window steps' host. A root body's cell registration replays its recorded `LookupTwin` and `AppendA980`. The
/// state refresh (the slot 16 subtree after them) is masked, so the replay takes the rest of the records as its own.
struct WindowReplay<'a> {
    cells: Replay<'a>,
}

impl CellHost for WindowReplay<'_> {
    fn register_cell(&mut self, link: u32, x: f32, y: f32) -> Result<(), HostUnavailable> {
        self.cells.register_cell(link, x, y)
    }
}

impl StateHost for WindowReplay<'_> {
    fn refresh_state(&mut self, _body: &Body) {
        self.cells.next = self.cells.inner.len();
    }
}

/// The bytes a window step writes outside its result: the refresh's, and a root body's link node.
fn window_mask(body: &Body) -> Vec<Range<usize>> {
    let mut masked = REFRESH_WRITES.to_vec();
    if body.parent == 0 {
        masked.push(CELL_LINK);
    }
    masked
}

#[test]
fn advance_rest_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VFEX) else {
        eprintln!("skipping VfEx: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let mut body = Body::load(&mem, this);
        let mut host = WindowReplay { cells: Replay { inner: &call.inner, next: 0, this } };
        advance_rest(&mut body, f32::from_bits(call.args[1]), &mut host)
            .expect("VfEx: the replay host answers every call");
        body.store(&mut mem, this);
        check_except(call, &mem, "VfEx", &window_mask(&body));
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("VfEx: {} calls match, by branch: {branches:?}", calls.len());
}

#[test]
fn back_step_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VFDX) else {
        eprintln!("skipping VfDx: {VAR} not set");
        return;
    };
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let mut body = Body::load(&mem, this);
        let mut host = WindowReplay { cells: Replay { inner: &call.inner, next: 0, this } };
        back_step(&mut body, f32::from_bits(call.args[1]), &mut host)
            .expect("VfDx: the replay host answers every call");
        body.store(&mut mem, this);
        check_except(call, &mem, "VfDx", &window_mask(&body));
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("VfDx: {} calls match, by branch: {branches:?}", calls.len());
}

/// A float lane `i` of little-endian bytes (0.0 past the end).
fn lane(bytes: &[u8], i: usize) -> f32 {
    bytes.get(4 * i..4 * i + 4).map_or(0.0, |b| f32::from_le_bytes(b.try_into().unwrap()))
}

fn vec4_of(bytes: &[u8]) -> Vec4 {
    Vec4 { x: lane(bytes, 0), y: lane(bytes, 1), z: lane(bytes, 2), w: lane(bytes, 3) }
}

/// The bytes of window `k` of `call` before the call.
fn window_pre(call: &Call, k: usize) -> &[u8] {
    &call.windows.iter().find(|w| w.index == k).expect("window").before
}

/// The callees of one recorded BigA call, as the port's host. The frame point is the call's first ComposeA. The
/// slot-80 response and the MB560 transform are the original's results, taken only when the original made those
/// calls (a call with no negative depth or no hit makes fewer). The transform's source is checked: x, y, z (the fourth
/// lane is the original's stack garbage, see the README).
struct ContactReplay {
    frame: Vec4,
    /// The slot-80 response, when the original made the call.
    response: Option<Vec4>,
    /// The MB560 source and result, when the original made the call.
    transform: Option<(Vec4, Vec4)>,
    /// The sample hook's arguments (its record's, `this` first), when the original made the call.
    hook: Option<Vec<u32>>,
    /// The hook calls the port made.
    hooked: usize,
}

impl BodyCalls for ContactReplay {
    fn response(&mut self, _point: Vec4) -> Vec4 {
        self.response.expect("the original made no slot-80 call")
    }

    fn transform(&mut self, src: Vec4) -> Vec4 {
        let (want, out) = self.transform.expect("the original made no MB560 call");
        assert_eq!(
            [src.x, src.y, src.z].map(f32::to_bits),
            [want.x, want.y, want.z].map(f32::to_bits),
            "MB560 source (the scaled normal)"
        );
        out
    }
}

impl ContactHost for ContactReplay {
    fn compose(&mut self, _vector: Vec4) -> Vec4 {
        self.frame
    }

    /// The two words `0x4e6000` wrote, as the original's hook got them.
    fn sample(&mut self) -> Result<(u32, u32), HostUnavailable> {
        let hook = self.hook.as_ref().expect("the original made no sample call");
        Ok((hook[1], hook[2]))
    }

    fn sample_hook(&mut self, h: &SampleHook) -> Result<(), HostUnavailable> {
        let hook = self.hook.as_ref().expect("the original made no sample hook call");
        assert_eq!(
            [h.id, h.length.to_bits(), h.reach.to_bits()],
            [hook[3], hook[4], hook[5]],
            "the sample hook's id, length and reach"
        );
        self.hooked += 1;
        Ok(())
    }
}

#[test]
fn contact_step_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, BIG_A) else {
        eprintln!("skipping BigA: {VAR} not set");
        return;
    };
    let mut hits = 0;
    let mut sink_calls = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let (this, a, b, c, d, e) =
            (call.args[0], call.args[1], call.args[2], call.args[3], call.args[4], call.args[5]);
        let def = mem.u32(this + 0x34);
        let max_step = lane(window_pre(call, BIG_A_TUNING), 0);
        let input = Input {
            contact: load_contact(&mem, e),
            inverse_mass: mem.f32(def + 0x16C),
            max_step,
            range: f32::from_bits(call.args[6]),
            sample: mem.u8(this + 0x60) & 8 != 0 && mem.u32(this + 0x3CC) != 0,
        };
        let inner_after = |addr: u32, at_arg: u32| -> Vec<u8> {
            let r = call.inner.iter().find(|r| r.addr == addr).expect("the original's callee");
            r.after.iter().find(|run| run.at == at_arg).expect("the callee's output").bytes.clone()
        };
        let compose = call.inner.iter().find(|r| r.addr == COMPOSE_A).expect("ComposeA");
        // Inner windows are numbered in the order the spec lists them: 0 the source, 1 the matrix, 2 the output.
        let transform = call
            .inner
            .iter()
            .find(|r| r.addr == MB560 && r.args[3] == a)
            .map(|r| (vec4_of(&r.windows[0].before), vec4_of(&r.windows[2].after)));
        let response = call.inner.iter().find(|r| r.addr == SLOT80).map(|r| vec4_of(&inner_after(SLOT80, r.args[2])));
        // The sample hook: the call on the hook object (its ecx) after `0x4e6000`.
        let hook_object = mem.u32(this + 0x3CC);
        let hook = call
            .inner
            .iter()
            .position(|r| r.addr == SAMPLE_SINK)
            .and_then(|at| call.inner[at..].iter().find(|r| r.args.first() == Some(&hook_object)))
            .map(|r| r.args.clone());
        let mut host = ContactReplay {
            frame: vec4_of(&inner_after(COMPOSE_A, compose.args[3])),
            response,
            transform,
            hook,
            hooked: 0,
        };
        let result = contact_step(&input, &mut host).expect("BigA: the replay answers every call");
        assert_eq!(host.hooked, usize::from(host.hook.is_some()), "BigA call {}: sample hook calls", call.index);
        sink_calls += host.hooked;
        mem.set_f32(d, result.depth);
        if let Some(force) = result.force {
            mem.set_f32(c, force);
        }
        if let Some(impulse) = result.impulse {
            for (i, v) in [impulse.force.x, impulse.force.y, impulse.force.z, impulse.force.w].into_iter().enumerate() {
                mem.set_f32(a + 4 * i as u32, v);
            }
            // The torque is 12 bytes (x, y, z); lane w is the original's stack and is not written.
            for (i, v) in [impulse.torque.x, impulse.torque.y, impulse.torque.z].into_iter().enumerate() {
                mem.set_f32(b + 4 * i as u32, v);
            }
        }
        assert_eq!(call.eax, result.impulse.is_some() as u32, "BigA call {} result", call.index);
        hits += result.impulse.is_some() as usize;
        check_except(call, &mem, "BigA", &[]);
    }
    eprintln!("BigA: {} calls match, {hits} hits, {sink_calls} with the sample sink", calls.len());
}

/// The callees of one recorded BigB call, as the port's host. The list is E36Collect's (none when it returned 0). The
/// responses and transforms are the original's results, taken in the order it made them; each transform's source is
/// checked against the original's.
struct ListReplay {
    records: Option<Vec<Record>>,
    responses: VecDeque<Vec4>,
    transforms: VecDeque<(Vec4, Vec4)>,
    retires: usize,
    applied: usize,
    emitted: Option<Event>,
}

impl BodyCalls for ListReplay {
    fn response(&mut self, _point: Vec4) -> Vec4 {
        self.responses.pop_front().expect("the original made no further slot-80 call")
    }

    fn transform(&mut self, src: Vec4) -> Vec4 {
        let (want, out) = self.transforms.pop_front().expect("the original made no further MB560 call");
        assert_eq!(
            [src.x, src.y, src.z].map(f32::to_bits),
            [want.x, want.y, want.z].map(f32::to_bits),
            "MB560 source (the force or the torque)"
        );
        out
    }
}

impl ListHost for ListReplay {
    fn collect(&mut self) -> Result<Option<Vec<Record>>, HostUnavailable> {
        Ok(self.records.take())
    }

    fn retire(&mut self) -> Result<(), HostUnavailable> {
        self.retires += 1;
        Ok(())
    }

    fn apply_force(&mut self, _force: Vec4) -> Result<(), HostUnavailable> {
        self.applied += 1;
        Ok(())
    }

    fn apply_torque(&mut self, _torque: Vec4) -> Result<(), HostUnavailable> {
        self.applied += 1;
        Ok(())
    }

    fn emit(&mut self, event: &Event) -> Result<(), HostUnavailable> {
        self.emitted = Some(*event);
        Ok(())
    }
}

fn u32_of(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

/// One `0x40`-byte record of E36Collect's list.
fn record_of(bytes: &[u8]) -> Record {
    Record {
        point: vec4_of(&bytes[0x00..0x10]),
        direction: vec4_of(&bytes[0x10..0x20]),
        aux: vec4_of(&bytes[0x20..0x30]),
        weight: lane(bytes, 0x30 / 4),
        kind: u32_of(&bytes[0x34..0x38]),
        factor: lane(bytes, 0x38 / 4),
    }
}

/// The event `Emit` was given (`0x44` bytes).
fn event_of(bytes: &[u8]) -> Event {
    Event {
        weight: lane(bytes, 0),
        kind: u32_of(&bytes[0x04..0x08]),
        aux: vec4_of(&bytes[0x10..0x20]),
        contact: vec4_of(&bytes[0x20..0x30]),
        point: vec4_of(&bytes[0x30..0x40]),
    }
}

fn lanes_bits(v: Vec4) -> [u32; 4] {
    [v.x, v.y, v.z, v.w].map(f32::to_bits)
}

/// The bytes a callee wrote for the pointer `at` (its `after` output).
fn output_at(call: &Inner, at: u32) -> Vec<u8> {
    call.after.iter().find(|a| a.at == at).expect("the callee's output").bytes.clone()
}

/// The contact list `E36Collect` returned (`None` when it returned 0: no list).
fn collected(collect: &Inner) -> Option<Vec<Record>> {
    (collect.eax != 0).then(|| {
        // The list pointer and the count are the cells of windows 0 and 1, read after the call.
        let list = u32_of(&collect.windows[0].after);
        let count = u32_of(&collect.windows[1].after) as usize;
        let bytes = if count == 0 { Vec::new() } else { output_at(collect, list) };
        bytes.chunks_exact(0x40).take(count).map(record_of).collect::<Vec<_>>()
    })
}

/// `BigB`'s host over `own`, the calls after its `E36Collect`: the slot-80 responses, and the MB560 transforms whose ecx
/// is one of `matrices`, in order. The caller checks the applies and the emit.
fn list_replay(own: &[Inner], matrices: &[u32], records: Option<Vec<Record>>) -> ListReplay {
    ListReplay {
        records,
        responses: own
            .iter()
            .filter(|r| r.addr == SLOT80 || r.addr == E43_VF140)
            .map(|r| vec4_of(&output_at(r, r.args[2])))
            .collect(),
        transforms: own
            .iter()
            .filter(|r| r.addr == MB560 && matrices.contains(&r.args[0]))
            .map(|r| (vec4_of(&r.windows[0].before), vec4_of(&r.windows[2].after)))
            .collect(),
        retires: 0,
        applied: 0,
        emitted: None,
    }
}

#[test]
fn contact_list_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, BIG_B) else {
        eprintln!("skipping BigB: {VAR} not set");
        return;
    };
    let mut with_contacts = 0;
    let mut emits = 0;
    for call in &calls {
        let mem = map_windows(call);
        // E36Collect's children come before it in the flat records; BigB's own calls come after it.
        let at = call.inner.iter().position(|r| r.addr == E36_COLLECT).expect("E36Collect");
        let collect = &call.inner[at];
        let own = &call.inner[at + 1..];
        let records = collected(collect);
        with_contacts += records.as_ref().is_some_and(|r| !r.is_empty()) as usize;
        // BigB's MB560 calls take the E43PairWalk global (node window 1) as ecx.
        let walker = u32_of(window_pre(call, 1));
        let transforms = own.iter().filter(|r| r.addr == MB560 && r.args[0] == walker).count();
        let mut host = list_replay(own, &[walker], records);
        let body = Body::load(&mem, call.args[0]);
        contact_list_step(&body, &mut host).expect("the replay answers every call");
        assert!(
            host.responses.is_empty() && host.transforms.is_empty(),
            "BigB call {}: the port made fewer calls than the original",
            call.index
        );
        assert_eq!(host.retires, own.iter().filter(|r| r.addr == RETIRE_A).count(), "BigB call {} retires", call.index);
        assert_eq!(host.applied, transforms, "BigB call {} applies", call.index);
        match (own.iter().find(|r| r.addr == EMIT), host.emitted) {
            (None, None) => {}
            (Some(r), Some(event)) => {
                let want = event_of(&r.windows[0].before);
                assert_eq!(
                    (event.weight.to_bits(), event.kind),
                    (want.weight.to_bits(), want.kind),
                    "BigB call {} event header",
                    call.index
                );
                assert_eq!(lanes_bits(event.aux), lanes_bits(want.aux), "event aux");
                assert_eq!(lanes_bits(event.contact), lanes_bits(want.contact), "event contact");
                // The fourth lane of the point is the original's stack garbage (the response writes three floats).
                assert_eq!(lanes_bits(event.point)[..3], lanes_bits(want.point)[..3], "event point");
                emits += 1;
            }
            _ => panic!("BigB call {}: the port and the original disagree on Emit", call.index),
        }
        // The body bytes the original's callees write (the port writes none): E34_RetireA takes the body out of its cell
        // and the active set (+0x14..+0x1a, IsActive at +0x1dc, +0x1ec); the slot-29 and slot-28 applies write the
        // accumulators at +0x310 and +0x370.
        let mut callee_writes = Vec::new();
        if own.iter().any(|r| r.addr == RETIRE_A) {
            callee_writes.extend([0x14..0x1A, 0x1DC..0x1E0, 0x1EC..0x1F0]);
        }
        if own.iter().any(|r| r.addr == APPLY_FORCE || r.addr == APPLY_TORQUE) {
            callee_writes.extend([0x310..0x320, 0x370..0x380]);
        }
        check_except(call, &mem, "BigB", &callee_writes);
    }
    eprintln!("BigB: {} calls match, {with_contacts} with a contact list, {emits} emits", calls.len());
}

/// `E43Vf64`'s host: the vote's answer (its record: eax and the two vectors), `BigB`'s calls through `ListReplay`, and the
/// bodies the applies left (after the vote's, then after `BigB`'s), in the order the pass reads them.
struct ForceReplay {
    vote: Option<(Vec4, Vec4)>,
    list: ListReplay,
    mids: VecDeque<Body>,
}

impl BodyCalls for ForceReplay {
    fn response(&mut self, point: Vec4) -> Vec4 {
        self.list.response(point)
    }

    fn transform(&mut self, src: Vec4) -> Vec4 {
        self.list.transform(src)
    }
}

impl ListHost for ForceReplay {
    fn collect(&mut self) -> Result<Option<Vec<Record>>, HostUnavailable> {
        self.list.collect()
    }

    fn retire(&mut self) -> Result<(), HostUnavailable> {
        self.list.retire()
    }

    fn apply_force(&mut self, force: Vec4) -> Result<(), HostUnavailable> {
        self.list.apply_force(force)
    }

    fn apply_torque(&mut self, torque: Vec4) -> Result<(), HostUnavailable> {
        self.list.apply_torque(torque)
    }

    fn emit(&mut self, event: &Event) -> Result<(), HostUnavailable> {
        self.list.emit(event)
    }
}

impl ForceHost for ForceReplay {
    fn entity_walk(&mut self) {}

    fn vote(&mut self) -> Result<Option<(Vec4, Vec4)>, HostUnavailable> {
        Ok(self.vote.take())
    }

    fn reread(&mut self) -> Body {
        self.mids.pop_front().expect("the pass reads the body once more than the replay has")
    }
}

/// The body after the apply record `apply` (its post window 0).
fn body_after(call: &Call, apply: &Inner) -> Body {
    assert_eq!(apply.windows[0].at, call.args[0], "the apply's body window");
    let mut after = map_windows(call);
    after.map(call.args[0], &apply.windows[0].after);
    Body::load(&after, call.args[0])
}

/// Each recorded force pass of a file with its vote (the last vote record on its body before it, range 1.0).
fn passes_with_votes(calls: &[Call]) -> Vec<(&Call, Option<&Call>)> {
    let mut votes: Vec<&Call> = Vec::new();
    let mut passes = Vec::new();
    for call in calls {
        if call.addr == VF81X && call.args[3] == 1.0f32.to_bits() {
            votes.push(call);
        } else if call.addr == FORCE_PASS {
            let this = call.args[0];
            passes.push((call, votes.iter().rev().find(|v| v.args[0] == this).copied()));
        }
    }
    passes
}

#[test]
fn force_pass_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(files) = load_files() else {
        eprintln!("skipping E43Vf64: {VAR} not set");
        return;
    };
    let (mut calls, mut with_contacts, mut emits, mut lists, mut hits) = (0, 0, 0, 0, 0);
    for (call, vote) in files.iter().flat_map(|f| passes_with_votes(f)) {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let def = Def::load(&mem, mem.u32(this + 0x34));
        // The entity walk's children come before it in the flat records, then the vote's calls (its record's inner
        // records). E36Collect's children come before it too: the pass's own calls (the vote's applies, BigB's, then the
        // transforms) come after the vote's, and BigB's after E36Collect.
        let at = call
            .inner
            .iter()
            .position(|r| r.addr == ENTITY_WALK_VF64 && r.args.first() == Some(&this))
            .expect("CEntity::E43Vf64");
        let vote = vote.unwrap_or_else(|| panic!("E43Vf64 call {}: no vote record", call.index));
        let walked = &call.inner[at + 1 + vote.inner.len()..];
        lists += usize::from(mem.u32(this + 0x274) != 0);
        let voted = (vote.eax != 0).then(|| {
            let v = |k: usize| vec4_of(&vote.windows.iter().find(|w| w.index == k).expect("the vote's vector").after);
            (v(1), v(2))
        });
        hits += usize::from(voted.is_some());
        let (records, own) = match walked.iter().position(|r| r.addr == E36_COLLECT) {
            Some(c) => (collected(&walked[c]), &walked[c + 1..]),
            None => (None, walked),
        };
        with_contacts += records.as_ref().is_some_and(|r| !r.is_empty()) as usize;
        // BigB's MB560 calls take the E43PairWalk global (node window 2) as ecx; the pass's own take the body.
        let walker = u32_of(window_pre(call, 2));
        // The bodies the pass reads again: after the vote's torque apply (its first), then after the last apply (BigB's,
        // else the vote's, else the body as it was), each from the apply's post window of the body (window 0).
        let applies: Vec<&Inner> = walked.iter().filter(|r| r.addr == APPLY_FORCE || r.addr == APPLY_TORQUE).collect();
        let mut mids = VecDeque::new();
        if voted.is_some() {
            mids.push_back(body_after(call, applies[1]));
        }
        mids.push_back(applies.last().map_or_else(|| Body::load(&mem, this), |a| body_after(call, a)));
        let mut body = Body::load(&mem, this);
        let mut host = ForceReplay { vote: voted, list: list_replay(own, &[walker, this], records), mids };
        force_pass(&mut body, &def, &mut host).expect("E43Vf64: the replay answers every call");
        assert!(
            host.list.responses.is_empty() && host.list.transforms.is_empty(),
            "E43Vf64 call {}: the port made fewer calls than the original",
            call.index
        );
        assert_eq!(
            host.list.retires,
            own.iter().filter(|r| r.addr == RETIRE_A).count(),
            "E43Vf64 call {} retires",
            call.index
        );
        assert_eq!(host.list.applied, applies.len(), "E43Vf64 call {} applies", call.index);
        assert_eq!(own.iter().any(|r| r.addr == EMIT), host.list.emitted.is_some(), "E43Vf64 call {} emit", call.index);
        emits += host.list.emitted.is_some() as usize;
        body.store(&mut mem, this);
        // The retire's writes (`BigB` with a kind-200 record) are the original's, not the port's.
        let mut masked = Vec::new();
        if own.iter().any(|r| r.addr == RETIRE_A) {
            masked.extend([0x14..0x1A, 0x1DC..0x1E0, 0x1EC..0x1F0]);
        }
        check_except(call, &mem, "E43Vf64", &masked);
        calls += 1;
    }
    eprintln!(
        "E43Vf64: {calls} calls match, {lists} with a vote list ({hits} hits), {with_contacts} with E36Collect contacts, {emits} emits"
    );
}

/// The calls of each record file named by `VAR`, one list per file in its order; None when the variable is unset.
fn load_files() -> Option<Vec<Vec<Call>>> {
    let paths = std::env::var(VAR).ok()?;
    Some(paths.split(':').map(|p| parse(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}")))).collect())
}

/// Each recorded vote of a file with the `BigA` calls it made: the `BigA` records on its body since the file's previous
/// vote (a listed callee's record comes before its caller's).
fn votes_with_contacts(calls: &[Call]) -> Vec<(&Call, Vec<&Call>)> {
    let mut votes = Vec::new();
    let mut pending = Vec::new();
    for call in calls {
        if call.addr == BIG_A {
            pending.push(call);
        } else if call.addr == VF81X {
            let this = call.args[0];
            let contacts = pending.drain(..).filter(|c| c.args[0] == this).collect();
            votes.push((call, contacts));
        }
    }
    votes
}

/// The vote's `BigA` calls, answered from their records in order: eax, and on a hit the pair from the post window of
/// `a` (`a`, then `b` 16 bytes on). The record's contact is checked against the list's.
struct ContactVoteReplay<'a> {
    mem: Mem,
    this: u32,
    contacts: std::vec::IntoIter<&'a Call>,
}

impl ContactVoteHost for ContactVoteReplay<'_> {
    fn contact(&mut self, k: u32) -> Result<Option<Impulse>, HostUnavailable> {
        let c = self.contacts.next().expect("the original made no further BigA call");
        let e = rbr_physics::rigid_body::layout::contact_record(&self.mem, self.this, k);
        assert_eq!(c.args[5], e, "BigA's contact");
        Ok((c.eax != 0).then(|| {
            let a = &c.windows.iter().find(|w| w.index == 1).expect("BigA's a").after;
            Impulse { force: vec4_of(&a[..0x10]), torque: vec4_of(&a[0x10..0x20]) }
        }))
    }
}

#[test]
fn vote_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(files) = load_files() else {
        eprintln!("skipping Vf81x: {VAR} not set");
        return;
    };
    let (mut calls, mut lists, mut hits, mut contacts) = (0, 0, 0, 0);
    for file in &files {
        for (call, made) in votes_with_contacts(file) {
            let mut mem = map_windows(call);
            let this = call.args[0];
            contacts += made.len();
            let mut host = ContactVoteReplay { mem: mem.clone(), this, contacts: made.into_iter() };
            let eax = contact_vote(&mut mem, this, call.args[1], call.args[2], &mut host)
                .expect("Vf81x: the replay answers every call");
            assert_eq!(eax, call.eax, "Vf81x call {}", call.index);
            assert!(host.contacts.next().is_none(), "Vf81x call {}: the port made fewer BigA calls", call.index);
            check_windows(call, &mem, "Vf81x");
            calls += 1;
            lists += usize::from(mem.u32(this + 0x274) != 0);
            hits += usize::from(eax != 0);
        }
    }
    eprintln!("Vf81x: {calls} calls match, {lists} with a contact list ({contacts} contacts), {hits} hits");
}

/// Counts the flushes the port asks for.
#[derive(Default)]
struct FlushCount(usize);

impl FlushHost for FlushCount {
    fn flush(&mut self) -> Result<(), HostUnavailable> {
        self.0 += 1;
        Ok(())
    }
}

#[test]
fn e43vf74_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(paths) = std::env::var(VAR).ok() else {
        eprintln!("skipping E43Vf74: {VAR} not set");
        return;
    };
    let (mut calls, mut flushed) = (0, 0);
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        for call in parse(&text) {
            for (i, rec) in call.inner.iter().enumerate() {
                if rec.addr != APPLY_TORQUE {
                    continue;
                }
                let this = rec.args[0];
                // Windows 2 and 3 are the gate words (`q` and `b`, see the spec).
                let gate = u32_of(&rec.windows[2].before) | u32_of(&rec.windows[3].before);
                // The flush is a child of the call, so the original records it just before the call's line.
                let recorded = i > 0 && call.inner[i - 1].addr == SLOT0_FLUSH && call.inner[i - 1].args == [this];
                assert_eq!(recorded, gate & FLUSH_BIT != 0, "E43Vf74 call {} flush", call.index);
                let mut mem = map_window_list(&rec.windows);
                let mut body = Body::load(&mem, this);
                let at = rec.args[1];
                let pp = Vec4::load(&mem, at);
                let mut host = FlushCount::default();
                accumulate(&mut body, pp, gate, &mut host).unwrap();
                body.store(&mut mem, this);
                assert_eq!(host.0, recorded as usize, "E43Vf74 call {} flush count", call.index);
                check_window_list(&rec.windows, &mem, "E43Vf74");
                calls += 1;
                flushed += recorded as usize;
            }
        }
    }
    assert!(calls > 0, "{VAR} has no E43Vf74 calls");
    eprintln!("E43Vf74: {calls} calls match, {flushed} with the flush");
}

/// The first three lanes of the vector at window `k` (the window's bytes before the call).
fn vec3_at(windows: &[Window], k: usize) -> Vec4 {
    let [x, y, z] = floats3(&windows[k].before);
    Vec4 { x, y, z, w: 0.0 }
}

/// The three floats at the start of `bytes`.
fn floats3(bytes: &[u8]) -> [f32; 3] {
    let f = |i: usize| f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap());
    [f(0), f(1), f(2)]
}

fn f32_at(windows: &[Window], k: usize) -> f32 {
    f32::from_le_bytes(windows[k].before[..4].try_into().unwrap())
}

/// One Dot60 call (top-level or inside E43Vf138): windows 0..2 are `a`, `b` and `c`, window 3 the def's vector.
fn check_dot60(windows: &[Window], st0: u64, name: &str) {
    let got = dot60(vec3_at(windows, 0), vec3_at(windows, 1), floats3(&windows[2].before), vec3_at(windows, 3));
    assert_eq!(f64::from(got).to_bits(), st0, "{name}: Dot60 result");
}

#[test]
fn dot60_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(paths) = std::env::var(VAR).ok() else {
        eprintln!("skipping Dot60: {VAR} not set");
        return;
    };
    let (mut top, mut inner) = (0, 0);
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        for call in parse(&text) {
            if call.addr == DOT60 {
                check_dot60(&call.windows, call.st0_bits, "Dot60 call");
                top += 1;
            }
            for rec in call.inner.iter().filter(|r| r.addr == DOT60) {
                check_dot60(&rec.windows, rec.st0_bits, "Dot60 inner");
                inner += 1;
            }
        }
    }
    assert!(top + inner > 0, "{VAR} has no Dot60 calls");
    eprintln!("Dot60: {top} top-level and {inner} inner calls match");
}

#[test]
fn e43vf138_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(paths) = std::env::var(VAR).ok() else {
        eprintln!("skipping E43Vf138: {VAR} not set");
        return;
    };
    let mut calls = 0;
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        for call in parse(&text).into_iter().filter(|c| c.addr == E43_VF138) {
            let w = &call.windows;
            let input =
                Dot138 { a: vec3_at(w, 0), b: vec3_at(w, 1), c: vec3_at(w, 2), scale: vec3_at(w, 5), k: f32_at(w, 6) };
            let (f, e) = e43vf138(&input, call.args[4], f32_at(w, 4), f32_at(w, 3));
            let mut mem = map_window_list(w);
            mem.set_f32(w[4].at, f);
            mem.set_f32(w[3].at, e);
            check_window_list(w, &mem, "E43Vf138");
            calls += 1;
        }
    }
    assert!(calls > 0, "{VAR} has no E43Vf138 calls");
    eprintln!("E43Vf138: {calls} calls match");
}

/// Replays the vote a Step77 call made (its recorded `S00`): checks the position and the point it was given, and
/// answers with the recorded eax and output.
struct VoteReplay<'a>(&'a Inner);

impl VoteHost for VoteReplay<'_> {
    fn vote(&mut self, pos: Vec4, point: Vec4) -> Result<Option<Vote>, HostUnavailable> {
        let rec = self.0;
        assert_eq!(rec.windows[0].before, vec4_bytes(pos), "vote position");
        assert_eq!(rec.windows[1].before, vec4_bytes(point), "vote point");
        if rec.eax == 0 {
            return Ok(None);
        }
        let out = &rec.after[0].bytes;
        Ok(Some(Vote { index: u32_of(&out[0x20..]), scale: f32::from_le_bytes(out[0x30..0x34].try_into().unwrap()) }))
    }
}

/// A vector's 16 bytes, as the game stores it.
fn vec4_bytes(v: Vec4) -> Vec<u8> {
    [v.x, v.y, v.z, v.w].iter().flat_map(|f| f.to_le_bytes()).collect()
}

#[test]
fn step77_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, STEP77) else {
        eprintln!("skipping Step77: {VAR} not set");
        return;
    };
    let (mut voted, mut passed) = (0, 0);
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let (a, b_at) = (call.args[1], call.args[2]);
        let pos = vec4_of(&mem.read(this + 0x140, 16));
        let rec = call.inner.iter().find(|r| r.addr == VOTE).expect("Step77 made no vote");
        let out = probe_ground(pos, mem.f32(b_at), &mut VoteReplay(rec)).expect("the replay host answers the vote");
        assert_eq!(out.ret as u32, call.eax, "Step77 call {}: eax", call.index);
        mem.set_u32(a, out.a);
        mem.set_f32(b_at, out.b);
        check_windows(call, &mem, "Step77");
        if out.ret != 0 {
            voted += 1;
        } else {
            passed += 1;
        }
    }
    assert!(voted + passed > 0, "{VAR} has no Step77 calls");
    eprintln!("Step77: {} calls match, {voted} with a vote", voted + passed);
}

/// CSolid::Step76 (direct records): the point's velocity, (p - pos) x w + v, into the out argument (xyz only).
#[test]
fn step76_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, STEP76, "Step76", |call, mem| {
        let (this, point, out) = (call.args[0], call.args[1], call.args[2]);
        point_velocity(mem, this, point, out);
    });
}

/// CSolid::Vf85 (direct records): the answer is the definition's word at +0x1ec being nonzero, and the call writes nothing.
#[test]
fn vf85_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VF85) else {
        eprintln!("skipping Vf85: {VAR} not set");
        return;
    };
    let (mut set, mut clear) = (0, 0);
    for call in &calls {
        let mem = map_windows(call);
        let flag = def_flag(&mem, call.args[0]);
        assert_eq!(flag as u32, call.eax, "Vf85 call {}: eax", call.index);
        if flag {
            set += 1;
        } else {
            clear += 1;
        }
    }
    assert!(set + clear > 0, "{VAR} has no Vf85 calls");
    eprintln!("Vf85: {} calls match, {set} set and {clear} clear", set + clear);
}

/// Replays `E43Vf90`'s host: counts the slot-18 call and the notify, and keeps the event.
#[derive(Default)]
struct EmitReplay {
    slot18: usize,
    notified: usize,
    emitted: Option<BodyEvent>,
}

impl EmitHost for EmitReplay {
    fn slot18(&mut self, _body: &mut Body) {
        self.slot18 += 1;
    }

    fn entity_notify(&mut self) {
        self.notified += 1;
    }

    fn emit(&mut self, event: &BodyEvent) -> Result<(), HostUnavailable> {
        self.emitted = Some(*event);
        Ok(())
    }
}

fn quat_bytes(q: Quat) -> Vec<u8> {
    vec4_bytes(Vec4 { x: q.x, y: q.y, z: q.z, w: q.w })
}

#[test]
fn emit_body_event_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, E43_VF90) else {
        eprintln!("skipping E43Vf90: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        // Slot 18 runs in the original before the port's part and writes the window start. The port reads the body
        // as slot 18 left it, so its post-window goes into the memory first.
        // The root's own calls are the ones on `this`: the host notify's children (other bodies) come before them.
        let s18 = call.inner.iter().find(|r| r.addr == S18 && r.args.first() == Some(&this)).expect("S18");
        let Window { index: _, at: _, len: _, before: _, after } =
            s18.windows.iter().find(|w| w.at == this).expect("S18's body window");
        mem.map(this, after);
        let mut body = Body::load(&mem, this);
        let mut host = EmitReplay::default();
        emit_body_event(&mut body, &mut host).expect("E43Vf90: the replay answers every call");
        body.store(&mut mem, this);
        assert_eq!(host.slot18, 1, "E43Vf90 call {}: slot 18", call.index);
        assert_eq!(
            host.notified,
            call.inner.iter().filter(|r| r.addr == ENTITY_NOTIFY && r.args.first() == Some(&this)).count(),
            "E43Vf90 call {} notify",
            call.index
        );
        let event = host.emitted.expect("E43Vf90: the port emits");
        check_event(call, &event, 1);
        check_except(call, &mem, "E43Vf90", &[]);
    }
    eprintln!("E43Vf90: {} calls match", calls.len());
}

/// The event payload of `call`'s emit (its last emit, the root's own), against the port's `event`.
fn check_event(call: &Call, event: &BodyEvent, kind: u32) {
    let emit = call.inner.iter().rev().find(|r| r.addr == EVENT_EMIT).expect("the event's emit");
    let payload = &emit.windows[0].after;
    assert_eq!(u32_of(&payload[..4]), kind, "the event's kind");
    assert_eq!(u32_of(&payload[4..8]), event.owner, "the event's owner");
    assert_eq!(payload[0x10..0x20], vec4_bytes(event.position)[..], "the event's position");
    assert_eq!(payload[0x20..0x30], quat_bytes(event.orientation)[..], "the event's orientation");
    assert_eq!(payload[0x30..0x40], vec4_bytes(event.position_again)[..], "the event's position again");
    assert_eq!(payload[0x40..0x50], quat_bytes(event.orientation_again)[..], "the event's orientation again");
}

/// Replays `E43Vf94`'s calls: the filing is the solid's own (`solid_filing`), and the event is kept. The sleep makes no
/// slot-18 call.
#[derive(Default)]
struct SleepReplay {
    emitted: Option<BodyEvent>,
}

impl FilingHost for SleepReplay {
    fn filing(&mut self, mem: &Mem, object: u32) -> Result<Filing, HostUnavailable> {
        solid_filing(mem, object)
    }
}

impl RestHost for SleepReplay {
    fn slot18(&mut self, _mem: &mut Mem, _this: u32) -> Result<(), HostUnavailable> {
        unreachable!("E43Vf94 makes no slot-18 call")
    }

    fn emit(&mut self, _mem: &Mem, event: &BodyEvent) -> Result<(), HostUnavailable> {
        self.emitted = Some(*event);
        Ok(())
    }
}

#[test]
fn sleep_body_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, SLOT37) else {
        eprintln!("skipping E43Vf94: {VAR} not set");
        return;
    };
    let mut parentless = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        // Without a parent the notify runs `WallRel`, which reads the physics object and the relay's globals: the record
        // does not hold them. Those calls are checked on the stage (`load_stage`) and in the chains.
        if Body::load(&mem, this).parent == 0 {
            parentless += 1;
            continue;
        }
        let mut host = SleepReplay::default();
        solid_sleep(&mut mem, &mut host, this).expect("E43Vf94: the replay answers every call");
        let event = host.emitted.expect("E43Vf94: the port emits");
        assert_eq!(event.kind, SLEEP, "E43Vf94 call {}: the event's kind", call.index);
        check_event(call, &event, SLEEP);
        check_except(call, &mem, "E43Vf94", &[]);
    }
    eprintln!("E43Vf94: {} calls match, {parentless} parentless calls not run", calls.len() - parentless);
}

/// The parent path is not reached by the direct runs (their bodies have no parent); the test panics if it is taken.
struct NoParent;

impl ResponseHost for NoParent {
    fn parent_response(&mut self, _parent: u32, _point: Vec4) -> Vec4 {
        panic!("the body has a parent")
    }

    fn compose(&mut self, _parent: u32, _point: Vec4) -> Vec4 {
        panic!("the body has a parent")
    }

    fn parent_transform(&mut self, _parent: u32, _v: Vec4) -> Vec4 {
        panic!("the body has a parent")
    }
}

#[test]
fn response_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, E43_VF140_DIRECT) else {
        eprintln!("skipping E43Vf140: {VAR} not set");
        return;
    };
    let mut checked = 0;
    for call in &calls {
        // Records of other layouts (the store holds older roots of this address) have no body window: skipped. The
        // parent path is not replayed here: a body with a parent is skipped.
        let this = call.args[0];
        let has_body = call.windows.iter().any(|w| w.index == 0 && w.at == this);
        let has_out = call.windows.iter().any(|w| w.index == 2 && w.len == 0xC);
        if !(has_body && has_out) {
            continue;
        }
        let mut mem = map_windows(call);
        let body = Body::load(&mem, this);
        if body.parent != 0 {
            continue;
        }
        checked += 1;
        let point = vec4_of(&call.windows.iter().find(|w| w.index == 1).expect("the point").before);
        let out = response(&body, point, &mut NoParent);
        // The original writes three floats at the output (window 2); the port's are written there before the check.
        let Window { index: _, at, len: _, before: _, after: _ } =
            call.windows.iter().find(|w| w.index == 2).expect("the output");
        mem.map(*at, &vec4_bytes(out)[..12]);
        check_windows(call, &mem, "E43Vf140");
    }
    eprintln!("E43Vf140: {checked} calls match (no parent)");
}

#[test]
fn e44_jmp_a570_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    // CMountedRigidBody::M1B40 (0x521b40) is the same jump.
    let (Some(mut calls), Some(m1b40)) = (load(VAR, 0x51e5c0), load(VAR, 0x521b40)) else {
        eprintln!("skipping E44_JmpA570: {VAR} not set");
        return;
    };
    calls.extend(m1b40);
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let body = Body::load(&mem, this);
        assert_eq!(body.parent, 0, "E44_JmpA570 call {}: direct fuzz parent", call.index);
        let point = vec4_of(&call.windows.iter().find(|w| w.index == 1).expect("point window").before);
        let out = response(&body, point, &mut NoParent);
        let Window { index: _, at, len: _, before: _, after: _ } =
            call.windows.iter().find(|w| w.index == 2).expect("output window");
        mem.map(*at, &vec4_bytes(out)[..12]);
        assert_eq!(call.eax, *at, "E44_JmpA570 call {}: eax", call.index);
        check_windows(call, &mem, "E44_JmpA570");
    }
    eprintln!("E44_JmpA570: {} calls match", calls.len());
}

#[test]
fn inverse_mass_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, E43_VF13C) else {
        eprintln!("skipping E43Vf13C: {VAR} not set");
        return;
    };
    for call in &calls {
        let mem = map_windows(call);
        let value = solid_inverse_mass(&mem, call.args[0]);
        assert_eq!(call.st0_bits, f64::from(value).to_bits(), "E43Vf13C call {}", call.index);
    }
    eprintln!("E43Vf13C: {} calls match", calls.len());
}

#[test]
fn impulse640_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, E46_IMPULSE640) else {
        eprintln!("skipping Impulse640: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        clamp_velocity_along(
            &mut mem,
            call.args[0],
            call.args[1],
            f32::from_bits(call.args[2]),
            f32::from_bits(call.args[4]),
        );
        check_windows(call, &mem, "E46Impulse::Impulse640");
    }
    eprintln!("Impulse640: {} calls match (including direction lane 3)", calls.len());
}

/// The physics object global, the first of the tail's node windows.
const PHYSICS_GLOBAL: u32 = 0x1659b70;

/// `E43Vf128`'s tail calls from the record, in order: the arguments must be the recorded ones (`this` first; the
/// virtual calls are known by them alone); the calls' post windows are mapped.
struct TailReplay<'a> {
    index: usize,
    inner: &'a [common::Inner],
    next: usize,
}

impl TailReplay<'_> {
    fn take(&mut self, mem: &mut Mem, addr: Option<u32>, args: &[u32]) -> u32 {
        let call = self
            .inner
            .get(self.next)
            .unwrap_or_else(|| panic!("E43Vf128 call {}: more tail calls than recorded", self.index));
        if let Some(addr) = addr {
            assert_eq!(call.addr, addr, "E43Vf128 call {}: tail call {}", self.index, self.next);
        }
        assert_eq!(call.args, args, "E43Vf128 call {}: tail call {} args", self.index, self.next);
        for Window { index: _, at, len: _, before: _, after: post } in &call.windows {
            mem.map(*at, post);
        }
        self.next += 1;
        call.eax
    }
}

impl ImpulseTailCalls for TailReplay<'_> {
    fn pose(&mut self, mem: &mut Mem, _: u32, body: u32, _pushed: u32) -> u32 {
        self.take(mem, None, &[body])
    }

    /// The walker (ecx) is the binding's: the recorded one.
    fn compose_a(&mut self, mem: &mut Mem, _: u32, point: u32, matrix: u32, out: u32) {
        let walker = self.inner.get(self.next).map_or(0, |r| r.args[0]);
        self.take(mem, Some(0x51b4c0), &[walker, point, matrix, out]);
    }

    fn hook(&mut self, mem: &mut Mem, _: u32, hook: u32, args: [u32; 8]) {
        let mut expected = vec![hook];
        expected.extend(args);
        self.take(mem, None, &expected);
    }

    /// G-v1stops' records list `E46_RingAppend` as a call of its own, not as an inner one: the port runs it there.
    fn ring_append(&mut self, mem: &mut Mem, _: u32, ring: u32, args: [u32; 4]) {
        if self.inner.get(self.next).map(|r| r.addr) != Some(0x529100) {
            let [src, a, b, c] = args;
            ring_append(mem, ring, src, [a, b, c]);
            return;
        }
        let mut expected = vec![ring];
        expected.extend(args);
        self.take(mem, Some(0x529100), &expected);
    }
}

#[test]
fn ring_append_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, 0x529100) else {
        eprintln!("skipping E46_RingAppend: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = map_windows(call);
        let [ring, src, a, b, c] = call.args[..].try_into().expect("RingAppend args");
        let eax = ring_append(&mut mem, ring, src, [a, b, c]);
        assert_eq!(eax, call.eax, "E46_RingAppend call {}: eax", call.index);
        check_windows(call, &mem, "E46_RingAppend");
    }
    eprintln!("E46_RingAppend: {} direct calls match", calls.len());
}

/// Replays the impulse's two transforms in the original's order: each must take the source the port computed.
struct ImpulseReplay {
    transforms: VecDeque<(Vec4, Vec4)>,
}

impl ImpulseReplay {
    fn next(&mut self, src: Vec4) -> Vec4 {
        let (want, out) = self.transforms.pop_front().expect("the original made no further MB560 call");
        assert_eq!(lanes_bits(src)[..3], lanes_bits(want)[..3], "MB560 source (the torque or the scaled torque)");
        out
    }
}

impl ImpulseHost for ImpulseReplay {
    fn transform_by_body(&mut self, src: Vec4) -> Vec4 {
        self.next(src)
    }

    fn transform_by_walker(&mut self, src: Vec4) -> Vec4 {
        self.next(src)
    }
}

/// E43Vf128's arithmetic path on the call's body, its two MB560 transforms answered from the records, stored to `mem`.
fn replay_impulse(call: &Call, mem: &mut Mem) {
    let this = call.args[0];
    let def = Def::load(mem, mem.u32(this + 0x34));
    let mut body = Body::load(mem, this);
    let point = vec4_of(window_pre(call, 1));
    let kick = vec4_of(window_pre(call, 2));
    // The two MB560 records, in order: the torque's transform, then the scaled torque's.
    let mut host = ImpulseReplay {
        transforms: call
            .inner
            .iter()
            .filter(|r| r.addr == MB560)
            .map(|r| (vec4_of(&r.windows[0].before), vec4_of(&r.windows[2].after)))
            .collect(),
    };
    impulse(&mut body, &def, point, kick, &mut host);
    assert!(host.transforms.is_empty(), "E43Vf128 call {}: the port made fewer MB560 calls", call.index);
    body.store(mem, this);
}

#[test]
fn impulse_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, E43_VF128) else {
        eprintln!("skipping E43Vf128: {VAR} not set");
        return;
    };
    let mut tails = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        replay_impulse(call, &mut mem);
        // The records with the physics global's debug word (G-hinge3's) also replay the tail: the calls after the
        // second MB560, the first one's output 0x30 above the frame.
        if call.windows.iter().any(|w| w.at == PHYSICS_GLOBAL) {
            let mb560: Vec<usize> = (0..call.inner.len()).filter(|&k| call.inner[k].addr == MB560).collect();
            let mut tail = TailReplay { index: call.index, inner: &call.inner[mb560[1] + 1..], next: 0 };
            let frame = call.inner[mb560[0]].args[3] - 0x30;
            let args: [u32; 8] = call.args[1..9].try_into().expect("E43Vf128 eight args");
            impulse_tail(&mut mem, this, args, frame, &mut tail);
            assert_eq!(tail.next, tail.inner.len(), "E43Vf128 call {}: fewer tail calls", call.index);
            tails += usize::from(tail.next != 0);
        }
        check_windows(call, &mem, "E43Vf128");
    }
    eprintln!("E43Vf128: {} calls match, {tails} with tail calls", calls.len());
}

/// `CMountedRigidBody::Vf128` (0x521b20, direct records, `fuzz_mounted_vf128`): an owned body returns the owner and
/// writes nothing; an unowned one is E43Vf128.
#[test]
fn mounted_vf128_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, MOUNTED_VF128) else {
        eprintln!("skipping CMountedRigidBody::Vf128: {VAR} not set");
        return;
    };
    let mut owned = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let owner = mem.u32(call.args[0] + 0x3D0);
        if owner != 0 {
            assert_eq!(call.eax, owner, "CMountedRigidBody::Vf128 call {}: eax", call.index);
            owned += 1;
        } else {
            replay_impulse(call, &mut mem);
        }
        check_windows(call, &mem, "CMountedRigidBody::Vf128");
    }
    eprintln!("CMountedRigidBody::Vf128: {} calls match, {owned} owned", calls.len());
}

#[test]
fn accumulate_force_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, ACCUM_340) else {
        eprintln!("skipping E43Vf70: {VAR} not set");
        return;
    };
    let mut flushed = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let this = call.args[0];
        let mut body = Body::load(&mem, this);
        let pp = vec4_of(window_pre(call, 1));
        // The gate words are not windows, so the gate bit comes from the flush record (the slot-0 call the original
        // makes on the body). E43Vf74's test checks the gate expression itself: this one uses the same bit.
        let recorded = call.inner.iter().any(|r| r.addr == SLOT0_FLUSH && r.args == [this]);
        let gate = if recorded { FLUSH_BIT } else { 0 };
        let mut host = FlushCount::default();
        accumulate_force(&mut body, pp, gate, &mut host).unwrap();
        assert_eq!(host.0, recorded as usize, "E43Vf70 call {} flush count", call.index);
        body.store(&mut mem, this);
        check_windows(call, &mem, "E43Vf70");
        flushed += recorded as usize;
    }
    eprintln!("E43Vf70: {} calls match, {flushed} with the flush", calls.len());
}

#[test]
fn reset_block_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, SCRATCH_VF04, "E47SolidScratch::Vf04", |call, mem| {
        store_scratch_reset(mem, call.args[0]);
    });
}

#[test]
fn scratch_base_init_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, SCRATCH_BASE_INIT, "E47_ScratchBaseInit", |call, mem| {
        store_scratch_base_init(mem, call.args[0]);
    });
}

#[test]
fn set_from_quat_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, SET_FROM_QUAT, "E01Mat3::SetFromQuat", |call, mem| {
        let q = vec4_of(window_pre(call, 1));
        let m = from_quat(Quat { x: q.x, y: q.y, z: q.z, w: q.w });
        for (k, v) in m.iter().flatten().enumerate() {
            mem.set_f32(call.args[0] + 4 * k as u32, *v);
        }
    });
}

#[test]
fn transform_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFAC, "CSolid::E43VfAC", |call, mem| {
        transform_body_rows(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn transform_rows3_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFB0, "CSolid::E43VfB0", |call, mem| {
        transform_body_rows3(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn subtract_origin_and_transform_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFA0, "CSolid::E43VfA0", |call, mem| {
        solid_e43vfa0(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn fold_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFBC, "CSolid::E43VfBC", |call, mem| {
        fold_body_rows(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn fold_rows_reordered_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFC0, "CSolid::E43VfC0", |call, mem| {
        fold_body_rows_reordered(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn quat_plus_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFA4, "CSolid::E43VfA4", |call, mem| {
        quat_plus_body_rows(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn quat_diff_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFA8, "CSolid::E43VfA8", |call, mem| {
        quat_diff_body_rows(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn quat_transform_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFB4, "CSolid::E43VfB4", |call, mem| {
        quat_transform_body_rows(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn quat_rows_b8_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFB8, "CSolid::E43VfB8", |call, mem| {
        quat_body_rows_b8(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn quat_fold_rows_c4_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFC4, "CSolid::E43VfC4", |call, mem| {
        fold_quat_body_rows_c4(mem, call.args[0], call.args[1], call.args[2]);
    });
}

#[test]
fn quat_fold_rows_c8_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, E43_VFC8, "CSolid::E43VfC8", |call, mem| {
        fold_quat_body_rows_c8(mem, call.args[0], call.args[1], call.args[2]);
    });
}

/// One recorded `E46Mat::Mul` (the child of `Vf17x`): its inputs and its output.
struct MulRecord {
    m: [u32; 9],
    b: [u32; 9],
    out: [f32; 9],
}

fn mul_record(inner: &Inner) -> MulRecord {
    // Window 0 is the matrix (before), 1 the output (after), 2 the diagonal (before), each nine words.
    let before = |k: usize| -> [u32; 9] {
        let w = &inner.windows.iter().find(|w| w.index == k).expect("window").before;
        std::array::from_fn(|i| u32::from_le_bytes(w[4 * i..4 * i + 4].try_into().unwrap()))
    };
    let out = &inner.windows.iter().find(|w| w.index == 1).expect("window").after;
    MulRecord {
        m: before(0),
        b: before(2),
        out: std::array::from_fn(|i| f32::from_le_bytes(out[4 * i..4 * i + 4].try_into().unwrap())),
    }
}

/// The product calls of one `Vf17x` call, in order, as the port's host: each must be made with the recorded inputs.
struct MulReplay {
    calls: VecDeque<MulRecord>,
}

impl MulHost for MulReplay {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], HostUnavailable> {
        let rec = self.calls.pop_front().expect("the original made no further E46Mat::Mul");
        assert_eq!(m.map(f32::to_bits), rec.m, "E46Mat::Mul matrix input");
        assert_eq!(b.map(f32::to_bits), rec.b, "E46Mat::Mul diagonal input");
        Ok(rec.out)
    }
}

#[test]
fn refresh_state_rotation_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, VF17X) else {
        eprintln!("skipping E47SolidState::Vf17x: {VAR} not set");
        return;
    };
    let mut products = 0;
    for call in &calls {
        let mut mem = map_windows(call);
        let mut host =
            MulReplay { calls: call.inner.iter().filter(|i| i.addr == E46_MAT_MUL).map(mul_record).collect() };
        products += host.calls.len();
        refresh_state_rotation(&mut mem, call.args[0], &mut host)
            .expect("E47SolidState::Vf17x: the product host is not called");
        assert!(host.calls.is_empty(), "E47SolidState::Vf17x call {}: fewer product calls", call.index);
        check_windows(call, &mem, "E47SolidState::Vf17x");
    }
    eprintln!("E47SolidState::Vf17x: {} calls match, {products} with the product", calls.len());
}

/// Counts the entity calls the helper makes (the record has no window for them; their effects are the body's, which
/// the body window checks).
#[derive(Default)]
struct EntityCount(usize);

impl Vf54Host for EntityCount {
    fn entity_scratch_tail(&mut self) -> Result<(), HostUnavailable> {
        self.0 += 1;
        Ok(())
    }
}

#[test]
fn vf54_helper_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, VF54_HELPER, "E47_Vf54Helper", |call, mem| {
        let mut host = EntityCount::default();
        scratch_reset_after_entity(mem, call.args[0], &mut host)
            .expect("E47_Vf54Helper: the entity host is not called");
        assert_eq!(host.0, 1, "E47_Vf54Helper call {}: one entity call", call.index);
    });
}

/// Writes the recorded windows of one physics-core call into `mem`, after checking the bytes before the call.
fn replay_windows(mem: &mut Mem, windows: &[Window], name: &str) {
    for Window { index: k, at, len, before: pre, after: post } in windows {
        assert_eq!(mem.read(*at, *len), *pre, "{name} window {k} before");
        mem.map(*at, post);
    }
}

#[test]
fn scale_add_rows_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, VF39X, "E47Solid::Vf39x", |call, mem| {
        let ret = scale_add_rows(mem, call.args[0], call.args[1], call.args[2]);
        assert_eq!(ret, call.eax, "E47Solid::Vf39x call {}: returns dst", call.index);
    });
}

/// The calls one `Vf16Helper` call makes outside the port: the product calls of its two `Vf17x` calls (in order), the
/// tail's `CBody::CState::Unk44`, and the entity walk, which the tail jump records as a call of its own.
struct Vf16Replay<'a> {
    /// The test's memory (a clone of a shared backend), so the host writes the windows at each call.
    mem: Mem,
    products: VecDeque<MulRecord>,
    unk44: Option<&'a Inner>,
    walk: &'a Call,
}

impl MulHost for Vf16Replay<'_> {
    fn mul(&mut self, m: [f32; 9], b: [f32; 9]) -> Result<[f32; 9], HostUnavailable> {
        let rec = self.products.pop_front().expect("the original made no further E46Mat::Mul");
        assert_eq!(m.map(f32::to_bits), rec.m, "E46Mat::Mul matrix input");
        assert_eq!(b.map(f32::to_bits), rec.b, "E46Mat::Mul diagonal input");
        Ok(rec.out)
    }
}

impl Vf40Host for Vf16Replay<'_> {
    fn state_unka44(&mut self, state: u32) -> u32 {
        let call = self.unk44.take().expect("the original made no CBody::CState::Unk44 call");
        assert_eq!(call.args, [state], "CBody::CState::Unk44: arguments");
        replay_windows(&mut self.mem, &call.windows, "CBody::CState::Unk44");
        call.eax
    }

    fn entity_vf40(&mut self, body: u32) -> u32 {
        assert_eq!(self.walk.args, [body], "CEntity::Vf40: arguments");
        replay_windows(&mut self.mem, &self.walk.windows, "CEntity::Vf40");
        self.walk.eax
    }
}

#[test]
fn vf16_helper_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let (Some(calls), Some(walks)) = (load(VAR, VF16_HELPER), load(VAR, WALK_VF40)) else {
        eprintln!("skipping E47_Vf16Helper: {VAR} not set");
        return;
    };
    assert_eq!(calls.len(), walks.len(), "E47_Vf16Helper: one entity walk per call");
    let mut products = 0;
    for (call, walk) in calls.iter().zip(&walks) {
        let mut mem = shared_windows(call);
        map_inner_windows(&mut mem, call);
        let mut host = Vf16Replay {
            mem: mem.clone(),
            products: call.inner.iter().filter(|i| i.addr == E46_MAT_MUL).map(mul_record).collect(),
            unk44: call.inner.iter().find(|i| i.addr == CSTATE_UNK44),
            walk,
        };
        products += host.products.len();
        let ret = vf16_helper(&mut mem, call.args[0], &mut host).expect("E47_Vf16Helper: a host is not called");
        assert!(host.products.is_empty(), "E47_Vf16Helper call {}: fewer product calls", call.index);
        assert_eq!(ret, call.eax, "E47_Vf16Helper call {}: returns the walk's result", call.index);
        check_windows(call, &mem, "E47_Vf16Helper");
    }
    eprintln!("E47_Vf16Helper: {} calls match, {products} products", calls.len());
}

/// The walk the slot-8 jump reaches: records its arguments and returns a marker.
#[derive(Default)]
struct WalkRecord(Vec<(u32, u32)>);

impl Vf20Host for WalkRecord {
    fn entity_walk_s20(&mut self, body: u32, arg: u32) -> u32 {
        self.0.push((body, arg));
        0x0051_A3A0
    }
}

/// `CSolid::E43Vf20` (0x529200) is a bare jump: the walk gets the body and the same stack argument, and its result is
/// returned as it is (the record for this is unit-level; the walk's own vectors are `entities_vectors`).
#[test]
fn entity_vf20_jump_passes_through() {
    let _pc24 = Precision::Pc24.scope();
    let mut walk = WalkRecord::default();
    let ret = entity_vf20_jump(0x6000_1000, 0x6000_2000, &mut walk);
    assert_eq!(walk.0, vec![(0x6000_1000, 0x6000_2000)]);
    assert_eq!(ret, 0x0051_A3A0);
}

/// Maps the windows of the call's inner calls that its own windows do not cover, as recorded before the call, so a
/// replayed child can read them (the owner object of `CBody::CState::Unk44`, the block of the `CDef` constructor).
fn map_inner_windows(mem: &mut Mem, call: &Call) {
    for Window { index: _, at, len, before: pre, after: _ } in call.inner.iter().flat_map(|i| &i.windows) {
        if !call.windows.iter().any(|w| w.at <= *at && *at + *len as u32 <= w.at + w.len as u32) {
            mem.map(*at, pre);
        }
    }
}

/// The heap block of one `CSolid::NewDef` call, as recorded (`operator new` returned it).
struct NewDefReplay {
    block: u32,
}

impl Heap for NewDefReplay {
    fn alloc(&mut self, _mem: &mut Mem, _size: u32) -> u32 {
        self.block
    }
}

/// Checks the windows of the original's `CSolid::CDef` call, nested in `NewDef`, after the port ran. Its windows hold the
/// final bytes of the block (an inner call's window can hold a vptr that a later write in the same constructor replaces).
fn check_cdef_windows(call: &Call, mem: &Mem, name: &str) {
    for inner in call.inner.iter().filter(|i| i.addr == CDEF_CTOR) {
        for Window { index: k, at, len, after: post, .. } in &inner.windows {
            assert_eq!(mem.read(*at, *len), *post, "{name} call {} CSolid::CDef window {k}", call.index);
        }
    }
}

#[test]
fn new_def_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, NEW_DEF) else {
        eprintln!("skipping CSolid::NewDef: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        map_inner_windows(&mut mem, call);
        let block = call.inner.iter().find(|i| i.addr == OPERATOR_NEW).map_or(0, |i| i.eax);
        // The name argument is the constructor's second argument (operator new's stack words are not recorded).
        let outer = call.inner.iter().find(|i| i.addr == CDEF_CTOR).map_or(0, |c| c.args[1]);
        let ret = new_def(&mut mem, outer, &mut NewDefReplay { block });
        assert_eq!(ret, call.eax, "CSolid::NewDef call {}: returns the block", call.index);
        check_windows(call, &mem, "CSolid::NewDef");
        check_cdef_windows(call, &mem, "CSolid::NewDef");
    }
    eprintln!("CSolid::NewDef: {} calls match", calls.len());
}

/// `CSolid::CDef::Vf50` (0x52d640): a body from the heap, built by the `CSolid` constructor on the definition with the two
/// arguments. Its recorded calls need the constructor's object graph (not reproduced), so this is a unit test: a failed
/// allocation builds nothing, and a made body gets the constructor's arguments.
#[test]
fn cdef_new_solid_builds_on_the_heap_block() {
    let _pc24 = Precision::Pc24.scope();
    struct SolidFake {
        block: u32,
        built: Vec<(u32, u32, u32, u32)>,
    }

    impl Heap for SolidFake {
        fn alloc(&mut self, _mem: &mut Mem, size: u32) -> u32 {
            assert_eq!(size, 0x3D0, "the body's size");
            self.block
        }
    }

    impl SolidCtorHost for SolidFake {
        fn solid_ctor(
            &mut self,
            _mem: &mut Mem,
            solid: u32,
            def: u32,
            a: u32,
            b: u32,
            flags: u32,
        ) -> Result<(), HostUnavailable> {
            assert_eq!(flags, 0, "CDef::Vf50 passes zero flags");
            self.built.push((solid, def, a, b));
            Ok(())
        }
    }

    let mut mem = Mem::new();
    let mut host = SolidFake { block: 0x6000_8000, built: Vec::new() };
    let body = cdef_new_solid(&mut mem, 0x6000_1000, 0x6000_2000, 0x6000_3000, &mut host).expect("the solid is built");
    assert_eq!(body, 0x6000_8000);
    assert_eq!(host.built, vec![(0x6000_8000, 0x6000_1000, 0x6000_2000, 0x6000_3000)]);

    host.block = 0;
    host.built.clear();
    let body = cdef_new_solid(&mut mem, 0x6000_1000, 0x6000_2000, 0x6000_3000, &mut host)
        .expect("a failed allocation is not a host failure");
    assert_eq!(body, 0);
    assert!(host.built.is_empty());
}

const E47_STATE_CTOR: u32 = 0x52D280;

#[test]
fn e47_state_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, E47_STATE_CTOR) else {
        eprintln!("skipping E47SolidState::ctor: {VAR} not set");
        return;
    };
    for call in &calls {
        let mut mem = shared_windows(call);
        map_inner_windows(&mut mem, call);
        // The arguments: this (the state), the owner, the name, and the unused word.
        construct_state(&mut mem, call.args[0], call.args[1], call.args[2]);
        check_windows(call, &mem, "E47SolidState::ctor");
    }
    eprintln!("E47SolidState::ctor: {} calls match", calls.len());
}

/// `CSolid::CDef::CDef` (0x529840) on a definition block, with the name argument (direct calls).
#[test]
fn cdef_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    common::replay(VAR, CDEF_CTOR, "CSolid::CDef::CDef", |call, mem| {
        solid_def_ctor(mem, call.args[0], call.args[1]);
    });
}

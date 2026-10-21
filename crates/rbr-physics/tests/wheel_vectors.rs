//! Replays the original's wheel calls (`harness/vcu_vectors.py --set wheel`, `harness/direct_calls.py`) through the
//! ports in `rbr_physics::wheel`. Needs record files: `RBR_WHEEL_VECTORS=<file>[:<file>...]`. Without the variable the
//! tests are skipped. Each test loads the port's inputs from the recorded memory, runs the port with the original's
//! answers to its host calls, and stores the port's results; every window must then hold the original's bytes.

mod common;

use common::{load, map_windows, shared_windows, Call, Inner};
use rbr_physics::car_body::transform::FRAME;
use rbr_physics::fpu::Precision;
use rbr_physics::math::vec::{clear_vec4, Vec4};
use rbr_physics::mem::Mem;
use rbr_physics::ntweak;
use rbr_physics::physics_core::construct::Heap;
use rbr_physics::physics_core::signal::{self, SignalHost};
use rbr_physics::rigid_body::emit::{BodyEvent, EmitHost};
use rbr_physics::rigid_body::host::{HostUnavailable, SolidCtorHost};
use rbr_physics::tyre;
use rbr_physics::wheel::axes::{self, Trig};
use rbr_physics::wheel::layout::{
    self, BlendCalls, CarSlotCalls, CarXform, DefCopyCalls, EmbeddedCalls, FinishCalls, HookCalls, SlotCalls,
    SolidCalls, StrutCalls, SubBlockInit, SubCalls, WalkCalls, WheelStore,
};

const VAR: &str = "RBR_WHEEL_VECTORS";

/// The recorded car transform `CCar::Xform6870` (0x4e6870) that a `Copy100` call made: the tmp it was given (its
/// argument 1, the value `copy` builds), checked against the record, and the vector it wrote (its window 2, `x`).
fn xform_from_record(call: &Call) -> impl FnOnce(u32, Vec4) -> Vec4 + '_ {
    move |car, tmp| {
        let inner = call.inner.iter().find(|i| i.addr == 0x4E6870).expect("Copy100 records one Xform6870");
        assert_eq!(car, inner.args[0], "Copy100 call {}: receiver", call.index);
        let (_, _, _, tmp_pre, _) = &inner.windows[1];
        assert_eq!(bytes_of(tmp), *tmp_pre, "Copy100 call {}: tmp", call.index);
        let (_, _, _, _, x_post) = &inner.windows[2];
        vec4_of(x_post)
    }
}

fn bytes_of(v: Vec4) -> Vec<u8> {
    [v.x, v.y, v.z, v.w].iter().flat_map(|f| f.to_bits().to_le_bytes()).collect()
}

fn vec4_of(bytes: &[u8]) -> Vec4 {
    let f = |i: usize| f32::from_bits(u32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()));
    Vec4 { x: f(0), y: f(1), z: f(2), w: f(3) }
}

/// Replays every recorded call of `addr`: `port` runs the port on the call's memory, then every window must hold the
/// original's bytes after the call.
fn replay(addr: u32, name: &str, port: impl Fn(&mut Mem, &Call)) {
    replay_with(addr, name, map_windows, port);
}

/// Like `replay`, with the memory `map` builds from each call (`shared_windows` when the port's host keeps a clone).
fn replay_with(addr: u32, name: &str, map: fn(&Call) -> Mem, port: impl Fn(&mut Mem, &Call)) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for call in &calls {
        let mut mem = map(call);
        port(&mut mem, call);
        for (k, at, len, _, post) in &call.windows {
            let got = mem.read(*at, *len);
            if let Some(i) = (0..*len).find(|&i| got[i] != post[i]) {
                panic!(
                    "{name} call {} window {k}: first difference at +{i:#x}: got {:#x}, want {:#x}",
                    call.index, got[i], post[i]
                );
            }
        }
        *branches.entry(call.branch.clone()).or_default() += 1;
    }
    eprintln!("{name}: {} calls match, by branch: {branches:?}", calls.len());
}

#[test]
fn transform_and_copy_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x5390D0, "copy100", |mem, c| {
        let (this, x, out) = (c.args[0], c.args[1], c.args[2]);
        layout::transform_and_copy(mem, this, x, out, xform_from_record(c));
    });
}

#[test]
fn blend_pair_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x539180, "blend4", |mem, c| {
        let [this, a, b, cc, d] = [c.args[0], c.args[1], c.args[2], c.args[3], c.args[4]];
        layout::blend_pair(mem, this, a, b, cc, d);
    });
}

#[test]
fn pick_contact_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x539290, "unk9290", |mem, c| {
        let [this, a, b, out] = [c.args[0], c.args[1], c.args[2], c.args[3]];
        layout::pick_contact(mem, this, a, b, out);
    });
}

#[test]
fn uint_select_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x539660, "unk9660", |mem, c| {
        let [this, p, n] = [c.args[0], c.args[1], c.args[2]];
        let wraps = layout::uint_select(mem, this, p, n);
        assert_eq!(wraps, c.eax, "Unk9660 call {}: wrap count", c.index);
    });
}

/// `CE36SubBlock::Init` as the original's record has it: the next recorded inner call must be `Init` on the same block.
struct RecordedInit<'a> {
    call: &'a Call,
    next: usize,
}

impl SubBlockInit for RecordedInit<'_> {
    fn init_sub_block(&mut self, block: u32) {
        let inner = next_call(self.call, &mut self.next, 0x4F3E80);
        assert_eq!(inner.args[0], block, "Unk96e0 call {}: Init block", self.call.index);
    }
}

#[test]
fn table_lerp_pair_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x5396E0, "unk96e0", |mem, c| {
        let li = next_call(c, &mut 0, 0x4F3E80).args[0];
        let mut init = RecordedInit { call: c, next: 0 };
        let block = layout::table_lerp_pair(&mut init, mem, c.args[0], li, layout::WheelTuning::STOCK);
        assert_eq!(block, c.eax, "Unk96e0 call {}: the tyre block (eax)", c.index);
    });
}

/// `M3B5B0`'s host: the base call's recorded writes are applied at call time. The tail jump leaves no record (the
/// recorder finds calls by return address), so its writes are checked by rust_step only, and its eax is the record's.
struct RecordedEmbedded<'a> {
    call: &'a Call,
    next: usize,
    mem: Mem,
}

impl EmbeddedCalls for RecordedEmbedded<'_> {
    fn solid_vf40(&mut self, wheel: u32) -> Result<(), HostUnavailable> {
        let inner = next_call(self.call, &mut self.next, 0x52B6D0);
        assert_eq!(inner.args[0], wheel, "M3B5B0 call {}: base", self.call.index);
        apply_writes(&mut self.mem, &windows_of(inner));
        Ok(())
    }

    fn embedded_slot17(&mut self, obj: u32) -> Result<u32, HostUnavailable> {
        assert_eq!(obj, self.call.args[0] + 0x3D0, "M3B5B0 call {}: tail object", self.call.index);
        Ok(self.call.eax)
    }
}

#[test]
fn embedded_dispatch_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_with(0x53B5B0, "m3b5b0", shared_windows, |mem, c| {
        let mut host = RecordedEmbedded { call: c, next: 0, mem: mem.clone() };
        let eax = layout::embedded_dispatch(&mut host, c.args[0]).unwrap();
        assert_eq!(eax, c.eax, "M3B5B0 call {}: eax", c.index);
    });
}

/// The next recorded call to `addr` at or after `*next` (calls the port does not make, such as a walk nested in a car
/// transform, are passed over).
fn next_call<'a>(call: &'a Call, next: &mut usize, addr: u32) -> &'a Inner {
    let at = (*next..call.inner.len())
        .find(|&i| call.inner[i].addr == addr)
        .unwrap_or_else(|| panic!("call {}: no recorded call to {addr:#x} left", call.index));
    *next = at + 1;
    &call.inner[at]
}

/// Answers the CRT sine and cosine from the original's records (their x87 results, as f64).
struct RecordedTrig<'a> {
    call: &'a Call,
    next: usize,
}

impl RecordedTrig<'_> {
    fn answer(&mut self, addr: u32, angle: f32) -> f64 {
        let inner = next_call(self.call, &mut self.next, addr);
        assert_eq!(inner.args[0], angle.to_bits(), "call {}: CRT argument", self.call.index);
        f64::from_bits(inner.st0_bits)
    }
}

impl Trig for RecordedTrig<'_> {
    fn sin(&mut self, angle: f32) -> f64 {
        self.answer(0x53F3B0, angle)
    }

    fn cos(&mut self, angle: f32) -> f64 {
        self.answer(0x53F3A0, angle)
    }
}

/// Answers the car's transforms from the original's inner records of `M3A8A0`, in call order. Each call must be the next
/// recorded one (its address and receiver). A source is checked against the bytes the original read (its window 0); the
/// result is the vector the original wrote (its window 1).
struct RecordedXform<'a> {
    call: &'a Call,
    next: usize,
}

impl RecordedXform<'_> {
    fn answer(&mut self, addr: u32, car: u32, a: Option<Vec4>) -> Vec4 {
        let inner = next_call(self.call, &mut self.next, addr);
        assert_eq!(inner.args[0], car, "M3A8A0 call {}: receiver", self.call.index);
        let (_, _, _, src_pre, _) = &inner.windows[0];
        match a {
            Some(v) => assert_eq!(bytes_of(v), *src_pre, "M3A8A0 call {}: source", self.call.index),
            None => assert_eq!(inner.args[1], 0, "M3A8A0 call {}: null source", self.call.index),
        }
        let (_, _, _, _, out_post) = &inner.windows[1];
        vec4_of(out_post)
    }
}

impl CarXform for RecordedXform<'_> {
    fn body_to_world_pair(&mut self, car: u32, a: Option<Vec4>) -> Vec4 {
        self.answer(0x4E69C0, car, a)
    }

    fn body_compose(&mut self, car: u32, a: Option<Vec4>) -> Vec4 {
        self.answer(0x4E6A30, car, a)
    }
}

#[test]
fn car_frames_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53A8A0, "m3a8a0", |mem, c| {
        let mut host = RecordedXform { call: c, next: 0 };
        layout::car_frames(&mut host, mem, c.args[0]);
        assert_eq!(host.next, c.inner.len(), "M3A8A0 call {}: transforms not all made", c.index);
    });
}

/// Answers `MB560` from its records, in call order: each call must be the next recorded one (walker, a, car). The `a`
/// vector is checked against the bytes the original read (window 0), and the result is the vector the original wrote to
/// `c` (window 1).
struct RecordedWalk<'a> {
    call: &'a Call,
    next: usize,
}

impl WalkCalls for RecordedWalk<'_> {
    fn pair_walk(&mut self, walker: u32, a: Option<Vec4>, car: u32) -> Vec4 {
        let inner = next_call(self.call, &mut self.next, 0x51B560);
        assert_eq!(inner.args[0], walker, "walk call {}: walker", self.call.index);
        assert_eq!(inner.args[2], car, "walk call {}: car", self.call.index);
        let (_, _, _, a_pre, _) = &inner.windows[0];
        match a {
            Some(v) => assert_eq!(bytes_of(v), *a_pre, "walk call {}: a bytes", self.call.index),
            None => assert_eq!(inner.args[1], 0, "walk call {}: null a", self.call.index),
        }
        let (_, _, _, _, c_post) = &inner.windows[1];
        vec4_of(c_post)
    }
}

#[test]
fn turn_axis_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53A9E0, "m3a9e0", |mem, c| {
        let this = c.args[0];
        let mut trig = RecordedTrig { call: c, next: 0 };
        let input = mem.load_axle_wobble(this);
        mem.store_wobbled_axle(this, axes::wobbled_axle(&mut trig, &input));
    });
}

#[test]
fn axis_from_angle_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53AB00, "m3ab00", |mem, c| {
        let this = c.args[0];
        let mut trig = RecordedTrig { call: c, next: 0 };
        let input = mem.load_inclination(this);
        mem.store_inclined_axle(this, &axes::inclined_axle(&mut trig, &input));
    });
}

/// Like `replay`, for a function whose result is its eax: every recorded call must return what `port` returns.
fn returns(addr: u32, name: &str, port: impl Fn(&Call) -> u32) {
    let Some(calls) = load(VAR, addr) else {
        eprintln!("skipping {name}: {VAR} not set");
        return;
    };
    if calls.is_empty() {
        eprintln!("{name}: no calls in the vector files");
        return;
    }
    for call in &calls {
        let got = port(call);
        assert_eq!(got, call.eax, "{name} call {}: eax", call.index);
    }
    eprintln!("{name}: {} calls match", calls.len());
}

#[test]
fn state_lerp_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53AB90, "m3ab90", |mem, c| {
        let this = c.args[0];
        let input = mem.load_top_mount_deform(this);
        mem.store_deformed_top_mount(this, axes::deformed_top_mount(&input));
    });
}

#[test]
fn set_input_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53AC50, "m3ac50", |mem, c| {
        let this = c.args[0];
        let mut trig = RecordedTrig { call: c, next: 0 };
        let input = mem.load_hub_damage(this, f32::from_bits(c.args[1]));
        let update = axes::apply_hub_torque(&mut trig, &input);
        mem.store_hub_damage(this, update.as_ref());
    });
}

#[test]
fn walk_result_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53AEF0, "m3aef0", |mem, c| {
        let this = c.args[0];
        let mut host = RecordedWalk { call: c, next: 0 };
        // The walker is the world frame global; the original reads it, so the test maps it to the recorded walker.
        mem.set_u32(FRAME, c.inner[0].args[0]);
        layout::walk_result(&mut host, mem, this);
        assert_eq!(host.next, c.inner.len(), "walk call {}: MB560 not all made", c.index);
    });
}

#[test]
fn spin_cross_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53AFB0, "m3afb0", |mem, c| {
        let this = c.args[0];
        let mut host = RecordedWalk { call: c, next: 0 };
        mem.set_u32(FRAME, c.inner[0].args[0]);
        layout::spin_cross(&mut host, mem, this, c.args[1]);
        assert_eq!(host.next, c.inner.len(), "spin call {}: MB560 not all made", c.index);
    });
}

/// The memory a host call leaves behind: each recorded window's post bytes, applied to the wheel after the port (the
/// host has no memory of its own in the port's trait).
type Writes = Vec<(u32, Vec<u8>)>;

fn apply_writes(mem: &mut Mem, writes: &Writes) {
    for (at, post) in writes {
        for (i, word) in post.chunks(4).enumerate() {
            mem.set_u32(at + 4 * i as u32, u32::from_le_bytes(word.try_into().unwrap()));
        }
    }
}

fn windows_of(inner: &Inner) -> Writes {
    inner.windows.iter().map(|(_, at, _, _, post)| (*at, post.clone())).collect()
}

/// Answers `M3D030`'s host calls from its records, in call order: the car transforms (through `RecordedXform`) and the
/// strut's `GetName`, `SetDirty`, `M3B900` and `UpdateAndReset`. Each call must be the next recorded one of its address.
/// The writes of `SetDirty` and `M3B900` are collected for the test to apply.
struct RecordedStrut<'a> {
    xform: RecordedXform<'a>,
    call: &'a Call,
    next: usize,
    writes: Writes,
}

impl CarXform for RecordedStrut<'_> {
    fn body_to_world_pair(&mut self, car: u32, a: Option<Vec4>) -> Vec4 {
        self.xform.body_to_world_pair(car, a)
    }

    fn body_compose(&mut self, car: u32, a: Option<Vec4>) -> Vec4 {
        self.xform.body_compose(car, a)
    }
}

impl StrutCalls for RecordedStrut<'_> {
    fn strut_name(&mut self, strut: u32) -> u32 {
        let inner = next_call(self.call, &mut self.next, 0x52DB10);
        assert_eq!(inner.args[0], strut, "M3D030 call {}: GetName receiver", self.call.index);
        inner.eax
    }

    fn mark_dirty(&mut self, object: u32) {
        let inner = next_call(self.call, &mut self.next, 0x52F450);
        assert_eq!(inner.args[0], object, "M3D030 call {}: SetDirty receiver", self.call.index);
        self.writes.extend(windows_of(inner));
    }

    fn apply_dirty_sum(&mut self, wheel: u32, sum: f32) {
        let inner = next_call(self.call, &mut self.next, 0x53B900);
        assert_eq!([inner.args[0], inner.args[1]], [wheel, sum.to_bits()], "M3D030 call {}: M3B900", self.call.index);
        self.writes.extend(windows_of(inner));
    }

    /// The original tail-jumps here, so its call is a record of its own: M3D030's record (and its windows) end at the
    /// jump, and the strut's update is not part of it.
    fn update_and_reset(&mut self, _strut: u32) {}
}

#[test]
fn update_strut_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53D030, "m3d030", |mem, c| {
        let mut host =
            RecordedStrut { xform: RecordedXform { call: c, next: 0 }, call: c, next: 0, writes: Vec::new() };
        let mut trig = RecordedTrig { call: c, next: 0 };
        layout::update_strut(&mut host, &mut trig, mem, c.args[0]);
        apply_writes(mem, &host.writes);
    });
}

/// Answers `Vf20`'s two calls from the records, in call order: `wheel_vf40` (its window is the wheel, applied after the
/// port) and `solid_vf20` (its window is the wheel too). Each must be the next recorded call of its address.
struct RecordedSlots<'a> {
    call: &'a Call,
    next: usize,
    writes: Writes,
}

impl SlotCalls for RecordedSlots<'_> {
    fn wheel_vf40(&mut self, wheel: u32) {
        let inner = next_call(self.call, &mut self.next, 0x53B5B0);
        assert_eq!(inner.args[0], wheel, "E43Vf20 call {}: Vf40 receiver", self.call.index);
        self.writes.extend(windows_of(inner));
    }

    fn solid_vf20(&mut self, wheel: u32, p: u32) {
        let inner = next_call(self.call, &mut self.next, 0x529200);
        assert_eq!([inner.args[0], inner.args[1]], [wheel, p], "E43Vf20 call {}: forward", self.call.index);
        self.writes.extend(windows_of(inner));
    }
}

/// Answers `E43Vf5C`'s two car calls from their records, in order. Each callee's after-windows go into the test's memory
/// (a clone of the shared backend) at the moment of the call, so the edge's clears after them land on top, as in the asm.
struct RecordedCarSlots<'a> {
    call: &'a Call,
    next: usize,
    mem: Mem,
}

impl RecordedCarSlots<'_> {
    fn car_call(&mut self, addr: u32, car: u32, p: u32) {
        let inner = next_call(self.call, &mut self.next, addr);
        assert_eq!([inner.args[0], inner.args[1]], [car, p], "E43Vf5C call {}: car call {addr:#x}", self.call.index);
        apply_writes(&mut self.mem, &windows_of(inner));
    }
}

impl CarSlotCalls for RecordedCarSlots<'_> {
    fn car_vf70(&mut self, car: u32, p: u32) {
        self.car_call(0x529340, car, p);
    }

    fn car_vf74(&mut self, car: u32, p: u32) {
        self.car_call(0x529EA0, car, p);
    }
}

#[test]
fn forward_to_car_and_clear_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_with(0x539A90, "e43vf5c", shared_windows, |mem, c| {
        let mut host = RecordedCarSlots { call: c, next: 0, mem: mem.clone() };
        layout::forward_to_car_and_clear(&mut host, mem, c.args[0]);
        assert_eq!(host.next, c.inner.len(), "E43Vf5C call {}: calls not all made", c.index);
    });
}

#[test]
fn clear_and_forward_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x539090, "e43vf20", |mem, c| {
        let (this, p) = (c.args[0], c.args[1]);
        let mut host = RecordedSlots { call: c, next: 0, writes: Vec::new() };
        layout::clear_and_forward(&mut host, mem, this, p);
        apply_writes(mem, &host.writes);
        assert_eq!(host.next, c.inner.len(), "E43Vf20 call {}: calls not all made", c.index);
    });
}

/// Answers `Unk97a0`'s `Unk94d0` calls from their records, in call order (the sub-object `+0xd00`, then `+0xc90`). The
/// callee's windows go into the test's memory at the moment of the call.
struct RecordedSub<'a> {
    call: &'a Call,
    next: usize,
    mem: Mem,
}

impl SubCalls for RecordedSub<'_> {
    fn sub_unk94d0(&mut self, wheel: u32, sub: u32, v: Vec4, a: u32, b: u32, c: u32) {
        let inner = next_call(self.call, &mut self.next, 0x5394D0);
        assert_eq!(
            [inner.args[0], inner.args[1], inner.args[3], inner.args[4], inner.args[5]],
            [wheel, sub, a, b, c],
            "Unk97a0 call {}: Unk94d0 arguments",
            self.call.index
        );
        // Window 2 is the stack vector `v` as the asm wrote it before the call (lane 3 is never written, so not checked).
        let (_, _, _, pre, _) = inner.windows.iter().find(|w| w.0 == 2).expect("Unk94d0 record: the vector window");
        let want = vec4_of(pre);
        assert_eq!(
            [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()],
            [want.x.to_bits(), want.y.to_bits(), want.z.to_bits()],
            "Unk97a0 call {}: the vector v",
            self.call.index
        );
        apply_writes(&mut self.mem, &windows_of(inner));
    }
}

/// `Unk94d0` (direct records, `split_fuzz`): the split on the recorded memory, entered 0x18 above the stack word it
/// reads as the remainder's lane 3 (the record's node window, 6). A word may differ from the record only as two NaNs
/// that differ in the sign: where both operands of an `addss`/`mulss` are NaNs, the emulator the records come from
/// picks the x87 way (the larger significand, then the positive sign) and `math::sse` the right operand's.
#[test]
fn split_on_sub_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, 0x5394D0) else {
        eprintln!("skipping Unk94d0: {VAR} not set");
        return;
    };
    let nan_sign_only = |a: u32, b: u32| f32::from_bits(a).is_nan() && f32::from_bits(b).is_nan() && a ^ b == 1 << 31;
    let (mut sign_only, mut long_rest) = (0, 0);
    for c in &calls {
        let mut mem = map_windows(c);
        let (_, residue_at, ..) = c.windows.iter().find(|w| w.0 == 6).expect("Unk94d0 record: the stack word");
        let eax = layout::split_on_sub(&mut mem, c.args[0], c.args[1], c.args[2], c.args[3], residue_at + 0x18);
        assert_eq!(eax, c.eax, "Unk94d0 call {}: eax", c.index);
        for (k, at, _, _, post) in &c.windows {
            for (w, bytes) in post.chunks_exact(4).enumerate() {
                let (ours, theirs) = (mem.u32(at + 4 * w as u32), u32::from_le_bytes(bytes.try_into().unwrap()));
                if ours != theirs {
                    assert!(
                        nan_sign_only(ours, theirs),
                        "Unk94d0 call {} window {k} word {w}: {ours:#x}, want {theirs:#x}",
                        c.index
                    );
                    sign_only += 1;
                }
            }
        }
        let rest = Vec4::load(&mem, c.args[0] + 0xE40);
        long_rest += usize::from(((rest.x * rest.x + rest.y * rest.y) + rest.z * rest.z).sqrt() > 0.001);
    }
    assert!(!calls.is_empty(), "{VAR} has no Unk94d0 calls");
    eprintln!(
        "Unk94d0: {} calls match ({long_rest} with the remainder's term), {sign_only} words NaN-sign only",
        calls.len()
    );
}

#[test]
fn unk97a0_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_with(0x5397A0, "unk97a0", shared_windows, |mem, c| {
        let mut host = RecordedSub { call: c, next: 0, mem: mem.clone() };
        let gated = layout::update_sub_objects(&mut host, mem, c.args[0], c.args[1], c.args[2], c.args[3]);
        assert_eq!(u32::from(gated), c.eax, "Unk97a0 call {}: result", c.index);
        assert_eq!(host.next, c.inner.len(), "Unk97a0 call {}: calls not all made", c.index);
    });
}

/// Answers `CopyCF30`'s three host calls from their records, in call order (each address is searched from the last one this
/// struct answered; the CRT calls are `RecordedTrig`'s). The callees' windows go into the test's memory at the moment of
/// the call.
struct RecordedDefCopy<'a> {
    call: &'a Call,
    next: usize,
    mem: Mem,
    matched: usize,
}

impl RecordedDefCopy<'_> {
    fn answer(&mut self, addr: u32, obj: u32, args: &[u32]) {
        let inner = next_call(self.call, &mut self.next, addr);
        let want: Vec<u32> = std::iter::once(obj).chain(args.iter().copied()).collect();
        assert_eq!(&inner.args[..want.len()], &want[..], "CopyCF30 call {}: {addr:#x} arguments", self.call.index);
        self.matched += 1;
        apply_writes(&mut self.mem, &windows_of(inner));
    }
}

impl DefCopyCalls for RecordedDefCopy<'_> {
    fn copy_state_to_def(&mut self, _: &mut Mem, strut: u32, b: u32) {
        self.answer(0x52DBD0, strut, &[b]);
    }

    fn copy_def_from(&mut self, _: &mut Mem, scratch: u32, c: u32) {
        self.answer(0x534760, scratch, &[c]);
    }

    fn set_dirty(&mut self, _: &mut Mem, base: u32) {
        self.answer(0x52F450, base, &[]);
    }
}

#[test]
fn copy_cf30_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_with(0x53CF30, "copycf30", shared_windows, |mem, c| {
        let mut host = RecordedDefCopy { call: c, next: 0, mem: mem.clone(), matched: 0 };
        let mut trig = RecordedTrig { call: c, next: 0 };
        layout::copy_cf30(&mut host, &mut trig, mem, c.args[0], c.args[1], c.args[2], c.args[3]);
        let crt = c.inner.iter().filter(|i| matches!(i.addr, 0x53F3B0 | 0x53F3A0)).count();
        assert_eq!(host.matched, c.inner.len() - crt, "CopyCF30 call {}: host calls not all made", c.index);
    });
}

/// Records the wheel's slot 96 call `CWheel::Hook` makes. Hook has no record of its own: the slot-96 callee is the step,
/// whose subtree the record would hold, so this checks the call's arguments only.
struct RecordedStep(Vec<(u32, u32)>);

impl HookCalls for RecordedStep {
    fn wheel_vf180(&mut self, wheel: u32, p: u32) {
        self.0.push((wheel, p));
    }
}

#[test]
fn hook_calls_slot_96() {
    let _pc24 = Precision::Pc24.scope();
    let mut host = RecordedStep(Vec::new());
    layout::hook(&mut host, 0x0BF1_F520, 0x1234_5678);
    assert_eq!(host.0, [(0x0BF1_F520, 0x1234_5678)]);
}

/// Answers `CSolid::E43Vf6C` from its own record: `Vf6C` is a tail call, so the solid's record is the call before the
/// `Vf6C` call (the wheel's record has no inner call). The arguments must be the wheel and `f`.
struct RecordedSolidCall<'a> {
    solid: &'a Call,
    writes: Writes,
}

impl SolidCalls for RecordedSolidCall<'_> {
    fn solid_vf6c(&mut self, wheel: u32, f: f32) {
        assert_eq!(
            [self.solid.args[0], self.solid.args[1]],
            [wheel, f.to_bits()],
            "E43Vf6C call {}: arguments",
            self.solid.index
        );
        // The tail call's writes are in the wheel's post window, so the test applies them.
        self.writes.extend(self.solid.windows.iter().map(|(_, at, _, _, post)| (*at, post.clone())));
    }
}

#[test]
fn forward_vf6c_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(solids) = load(VAR, 0x52CC30) else {
        eprintln!("skipping vf6c: {VAR} not set");
        return;
    };
    replay(0x539EB0, "vf6c", |mem, c| {
        let solid = solids
            .iter()
            .find(|s| s.index + 1 == c.index)
            .unwrap_or_else(|| panic!("Vf6C call {}: no solid record before it", c.index));
        let mut host = RecordedSolidCall { solid, writes: Vec::new() };
        layout::forward_vf6c(&mut host, c.args[0], f32::from_bits(c.args[1]));
        apply_writes(mem, &host.writes);
    });
}

/// Answers `Unk9900`'s two host calls from their records: `BlendTable` (its outputs are the post bytes of its two
/// out-pointer windows) and `E50BlendOps::Blend`. The writes of every inner record of the call (`BlendTable`'s own
/// `SetDirty` calls too) are applied by the test after the port.
struct RecordedBlendCalls<'a> {
    call: &'a Call,
    next: usize,
}

impl BlendCalls for RecordedBlendCalls<'_> {
    fn blend_coefficients(&mut self, scratch: u32, i: u32, j: u32, a: f32, b: f32) -> (f32, f32) {
        let inner = next_call(self.call, &mut self.next, 0x534E70);
        assert_eq!(
            [inner.args[0], inner.args[1], inner.args[2], inner.args[3], inner.args[4]],
            [scratch, i, j, a.to_bits(), b.to_bits()],
            "Unk9900 call {}: BlendTable arguments",
            self.call.index
        );
        let (_, _, _, _, o1) = &inner.windows[1];
        let (_, _, _, _, o2) = &inner.windows[2];
        (f32::from_bits(word_of(o1)), f32::from_bits(word_of(o2)))
    }

    fn blend_materials(&mut self, ops: u32, s1: u32, s2: u32, a: f32, b: f32, z: f32) {
        let inner = next_call(self.call, &mut self.next, 0x5381F0);
        assert_eq!(
            [inner.args[0], inner.args[1], inner.args[2], inner.args[3], inner.args[4], inner.args[5]],
            [ops, s1, s2, a.to_bits(), b.to_bits(), z.to_bits()],
            "Unk9900 call {}: Blend arguments",
            self.call.index
        );
    }
}

fn word_of(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().unwrap())
}

#[test]
fn blend_tags_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x539900, "unk9900", |mem, c| {
        let this = c.args[0];
        let mut host = RecordedBlendCalls { call: c, next: 0 };
        layout::blend_tags(&mut host, mem, this, c.args[1], c.args[2], f32::from_bits(c.args[3]));
        for inner in &c.inner {
            apply_writes(mem, &windows_of(inner));
        }
        assert_eq!(host.next, c.inner.len(), "Unk9900 call {}: host calls not all made", c.index);
    });
}

/// `E51MultiTire::Emit` (0x53d0d0) on a signal with listeners and on its guard, count and range paths. No snapshot reaches
/// the emit, so these check the edge against the asm, not against a recording.
mod tire_emit {
    use rbr_physics::mem::Mem;
    use rbr_physics::wheel::layout;
    use rbr_physics::wheel::layout::EmitCalls;

    const TIRE: u32 = 0x1000;
    const COUNT: u32 = 0x1400;
    const VTABLE: u32 = 0x1800;
    const INVOKE: u32 = 0x9000;

    #[derive(Default)]
    struct Log(Vec<String>);

    impl EmitCalls for Log {
        fn invoke(&mut self, target: u32, conn: u32, sig: u32) {
            self.0.push(format!("invoke {target:#x} {conn:#x} {sig:#x}"));
        }

        fn log_recursive(&mut self) {
            self.0.push("recursive".to_string());
        }

        fn log_range(&mut self, count: u32) {
            self.0.push(format!("range {count}"));
        }
    }

    /// The signal (0x218 bytes) with its count pointer at +4 and the count at `count`.
    fn signal(count: u32) -> Mem {
        let mut mem = Mem::new();
        mem.map(TIRE, &[0; 0x218]);
        mem.map(COUNT, &[0; 4]);
        mem.map(VTABLE, &[0; 8]);
        mem.set_u32(VTABLE + 4, INVOKE);
        mem.set_u32(TIRE + 4, COUNT);
        mem.set_u32(COUNT, count);
        mem
    }

    /// Links the connections (vtable at +0, next at +4) into the list at `head`, in order.
    fn link(mem: &mut Mem, head: u32, conns: &[u32]) {
        mem.set_u32(head, conns.first().copied().unwrap_or(0));
        for (i, &conn) in conns.iter().enumerate() {
            mem.map(conn, &[0; 0x8]);
            mem.set_u32(conn, VTABLE);
            mem.set_u32(conn + 4, conns.get(i + 1).copied().unwrap_or(0));
        }
    }

    #[test]
    fn overflow_goes_before_the_count_slot() {
        let mut mem = signal(3);
        link(&mut mem, TIRE + 8 + 16 * 3, &[0x2000, 0x2100]);
        link(&mut mem, TIRE + 0x208, &[0x3000]);
        let mut log = Log::default();
        layout::tire_emit(&mut mem, &mut log, TIRE, 0x4000);
        assert_eq!(
            log.0,
            ["invoke 0x9000 0x3000 0x4000", "invoke 0x9000 0x2000 0x4000", "invoke 0x9000 0x2100 0x4000"]
        );
        assert_eq!(mem.u32(TIRE), 0);
    }

    #[test]
    fn empty_lists_call_nothing() {
        let mut mem = signal(0);
        let mut log = Log::default();
        layout::tire_emit(&mut mem, &mut log, TIRE, 0x4000);
        assert!(log.0.is_empty());
        assert_eq!(mem.u32(TIRE), 0);
    }

    #[test]
    fn null_count_pointer_returns_silently() {
        let mut mem = signal(0);
        mem.set_u32(TIRE + 4, 0);
        let mut log = Log::default();
        layout::tire_emit(&mut mem, &mut log, TIRE, 0x4000);
        assert!(log.0.is_empty());
        assert_eq!(mem.u32(TIRE), 0);
    }

    #[test]
    fn set_guard_logs_and_returns() {
        let mut mem = signal(0);
        mem.set_u32(TIRE, 1);
        let mut log = Log::default();
        layout::tire_emit(&mut mem, &mut log, TIRE, 0x4000);
        assert_eq!(log.0, ["recursive"]);
        assert_eq!(mem.u32(TIRE), 1);
    }

    #[test]
    fn out_of_range_count_logs_and_keeps_the_guard() {
        let mut mem = signal(0x20);
        let mut log = Log::default();
        layout::tire_emit(&mut mem, &mut log, TIRE, 0x4000);
        assert_eq!(log.0, ["range 32"]);
        assert_eq!(mem.u32(TIRE), 1);
    }
}

/// `M3B900`'s host: `SetDirty` (its record's writes are applied), the tyre tail (`TyreTail_Get10` reads the pointer at
/// +0x10 of the container, unrecorded), and the wheel's `Vf40`, the `M3B5B0` record (its writes, and the eax the refresh
/// returns).
struct RecordedRefresh<'a> {
    call: &'a Call,
    next: usize,
    mem: Mem,
}

impl layout::RefreshCalls for RecordedRefresh<'_> {
    fn set_dirty(&mut self, object: u32) {
        let inner = next_call(self.call, &mut self.next, 0x52F450);
        assert_eq!(inner.args[0], object, "M3B900 call {}: SetDirty object", self.call.index);
        apply_writes(&mut self.mem, &windows_of(inner));
    }

    fn tyre_tail(&mut self, container: u32) -> u32 {
        self.mem.u32(container + 0x10)
    }

    fn vf40(&mut self, wheel: u32) -> u32 {
        let inner = next_call(self.call, &mut self.next, 0x53B5B0);
        assert_eq!(inner.args[0], wheel, "M3B900 call {}: Vf40 object", self.call.index);
        apply_writes(&mut self.mem, &windows_of(inner));
        inner.eax
    }
}

#[test]
fn refresh_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay_with(0x53B900, "m3b900", shared_windows, |mem, c| {
        let this = c.args[0];
        let travel = f32::from_bits(c.args[1]);
        // The prologue's two calls: the lerp (`M3AB90`) and the turn axis (`M3A9E0`), each stored before the rest.
        let top_mount = axes::deformed_top_mount(&mem.load_top_mount_deform(this));
        mem.store_deformed_top_mount(this, top_mount);
        let axle = axes::wobbled_axle(&mut axes::X87Trig, &mem.load_axle_wobble(this));
        mem.store_wobbled_axle(this, axle);
        let mut host = RecordedRefresh { call: c, next: 0, mem: mem.clone() };
        let eax = layout::refresh(mem, &mut host, this, travel);
        assert_eq!(eax, c.eax, "M3B900 call {}: eax", c.index);
    });
}

#[test]
fn constant_accessors_match_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    returns(0x538B40, "CDef::Unk4C", |_| layout::cdef_unk4c());
    returns(0x538D70, "CDef::Unk20", |_| layout::cdef_unk20());
    returns(0x53B550, "E51Setup::Vf20", |_| layout::setup_vf20());
    returns(0x53B570, "E51Setup::Vf28", |_| layout::setup_vf28());
}

#[test]
fn damage_zero_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53B050, "damage zero", |mem, c| layout::damage_zero(mem, c.args[0]));
}

#[test]
fn scratch_vf04_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x538F50, "scratch vf04", |mem, c| layout::scratch_vf04(mem, c.args[0]));
}

#[test]
fn scratch_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53B670, "scratch ctor", |mem, c| {
        layout::scratch_ctor(mem, c.args[0]);
    });
    returns(0x53B670, "scratch ctor", |c| c.args[0]);
}

#[test]
fn refresh_travel_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53B880, "refresh travel", |mem, c| {
        assert_eq!(layout::refresh_travel(mem, c.args[0]), c.eax, "M3B880 call {}: eax", c.index);
    });
}

#[test]
fn tyre_tail_setup_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53A860, "tyre tail setup", |mem, c| {
        layout::tyre_tail_setup(mem, c.args[0], c.args[1], c.args[2], c.args[3])
    });
}

#[test]
fn ntweak_constructors_match_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x52FF80, "NTweak::CVector::CVector", |mem, c| ntweak::construct_vector(mem, c.args[0]));
    replay(0x52FEA0, "NTweak::CReal::CReal", |mem, c| ntweak::construct_real(mem, c.args[0]));
}

#[test]
fn ntweak_base_and_members_match_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x52F090, "NTweak::CBase::CBase", |mem, c| ntweak::construct_base(mem, c.args[0], c.args[1]));
    replay(0x52FFF0, "NTweak::CVector4::CVector4", |mem, c| ntweak::construct_vector4(mem, c.args[0]));
    replay(0x52FEC0, "NTweak::CUnsigned::CUnsigned", |mem, c| ntweak::construct_unsigned(mem, c.args[0]));
}

#[test]
fn set_dirty_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x52F450, "NTweak::CBase::SetDirty", |mem, c| ntweak::set_dirty(mem, c.args[0]));
}

#[test]
fn vec4_clear_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x465D80, "E09Vec4::Clear", |mem, c| {
        let zero = clear_vec4();
        for (k, lane) in [zero.x, zero.y, zero.z, zero.w].into_iter().enumerate() {
            mem.set_f32(c.args[0] + 4 * k as u32, lane);
        }
    });
}

#[test]
fn cstate_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x538DD0, "CWheel::CState::CState", |mem, c| {
        layout::state_ctor(mem, c.args[0], c.args[1], c.args[2]);
    });
    returns(0x538DD0, "CWheel::CState::CState", |c| c.args[0]);
}

/// The solid base the original's `CSolid::CSolid` call made (`E51Wheel::Ctor`'s nested call), replayed as its recorded
/// window, and the owner's wake (`CSolid::E43Vf90`, made when the record has an owner), replayed as its recorded window
/// on the wheel.
struct SolidReplay<'a> {
    call: &'a Call,
    mem: Mem,
}

impl SolidCtorHost for SolidReplay<'_> {
    fn solid_ctor(
        &mut self,
        mem: &mut Mem,
        solid: u32,
        def: u32,
        a: u32,
        b: u32,
        flags: u32,
    ) -> Result<(), HostUnavailable> {
        let inner = self.call.inner.iter().find(|i| i.addr == 0x52D480).expect("CSolid::CSolid");
        assert_eq!(inner.args, [solid, def, a, b, flags], "CSolid::CSolid arguments");
        for (_, at, _, _, post) in &inner.windows {
            mem.map(*at, post);
        }
        Ok(())
    }
}

/// The wake is the recorded `E43Vf90`, so its parts are never asked.
impl EmitHost for SolidReplay<'_> {
    fn slot18(&mut self, _body: &mut rbr_physics::rigid_body::layout::Body) {
        unreachable!("the wake is the recorded E43Vf90")
    }

    fn entity_notify(&mut self) {
        unreachable!("the wake is the recorded E43Vf90")
    }

    fn emit(&mut self, _event: &BodyEvent) -> Result<(), HostUnavailable> {
        unreachable!("the wake is the recorded E43Vf90")
    }

    fn wake(&mut self, body: &mut rbr_physics::rigid_body::layout::Body) -> Result<(), HostUnavailable> {
        let inner = self.call.inner.iter().find(|i| i.addr == 0x52BF30).expect("CSolid::E43Vf90");
        let (_, at, _, _, post) = &inner.windows[0];
        self.mem.map(*at, post);
        *body = rbr_physics::rigid_body::layout::Body::load(&self.mem, inner.args[0]);
        Ok(())
    }
}

/// `E51Wheel::Ctor` (direct records, `wheel_ctor_fuzz`; the nested calls of `M3D1C0`'s records too), with and without an
/// owner.
#[test]
fn wheel_ctor_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let woken = std::cell::Cell::new(0);
    replay_with(0x53B6E0, "E51Wheel::Ctor", shared_windows, |mem, c| {
        let mut host = SolidReplay { call: c, mem: mem.clone() };
        let this = layout::wheel_ctor(mem, &mut host, c.args[0], c.args[1], c.args[2], c.args[3], c.args[4])
            .expect("the recorded calls have no host gap");
        assert_eq!(this, c.args[0]);
        woken.set(woken.get() + usize::from(c.args[3] != 0));
    });
    returns(0x53B6E0, "E51Wheel::Ctor", |c| c.args[0]);
    eprintln!("E51Wheel::Ctor: {} calls with an owner", woken.get());
}

/// `M3D1C0`'s calls: `operator new` answered with its recorded block, the constructor's solid base and wake from the
/// call's (flattened) inner records.
struct NewWheel<'a> {
    block: u32,
    solid: SolidReplay<'a>,
}

impl Heap for NewWheel<'_> {
    fn alloc(&mut self, _mem: &mut Mem, size: u32) -> u32 {
        assert_eq!(size, layout::WHEEL_SIZE);
        self.block
    }
}

impl SolidCtorHost for NewWheel<'_> {
    fn solid_ctor(
        &mut self,
        mem: &mut Mem,
        solid: u32,
        def: u32,
        a: u32,
        b: u32,
        flags: u32,
    ) -> Result<(), HostUnavailable> {
        self.solid.solid_ctor(mem, solid, def, a, b, flags)
    }
}

impl EmitHost for NewWheel<'_> {
    fn slot18(&mut self, body: &mut rbr_physics::rigid_body::layout::Body) {
        self.solid.slot18(body)
    }

    fn entity_notify(&mut self) {
        self.solid.entity_notify()
    }

    fn emit(&mut self, event: &BodyEvent) -> Result<(), HostUnavailable> {
        self.solid.emit(event)
    }

    fn wake(&mut self, body: &mut rbr_physics::rigid_body::layout::Body) -> Result<(), HostUnavailable> {
        self.solid.wake(body)
    }
}

/// `E51Wheel::M3D1C0` (direct records, `new_wheel_fuzz`): `new_wheel` with the recorded block (zeroed, as the fuzz heap
/// is) must return it and leave the original's wheel bytes.
#[test]
fn new_wheel_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    let Some(calls) = load(VAR, 0x53D1C0) else {
        eprintln!("skipping E51Wheel::M3D1C0: {VAR} not set");
        return;
    };
    for c in &calls {
        let block = c.inner.iter().find(|i| i.addr == 0x70BEC2).expect("operator new").eax;
        let mut mem = shared_windows(c);
        mem.map(block, &vec![0; layout::WHEEL_SIZE as usize]);
        let mut host = NewWheel { block, solid: SolidReplay { call: c, mem: mem.clone() } };
        let wheel = layout::new_wheel(&mut mem, &mut host, c.args[0], c.args[1], c.args[2])
            .expect("the recorded calls have no host gap");
        assert_eq!(wheel, c.eax, "M3D1C0 call {}: eax", c.index);
        for (at, len, bytes) in &c.after {
            assert_eq!(&mem.read(*at, *len), bytes, "M3D1C0 call {}: the wheel", c.index);
        }
    }
    eprintln!("E51Wheel::M3D1C0: {} calls match", calls.len());
}

/// `E50BlendOps::Blend` (0x5381f0): the thirteen blended words with weights 0.5, 0.25 and 0.5 (`a` = 8, `b` = 16): the
/// +0x48, +0x58, +0x68 and +0x78 words are 0.25 a + 0.125 b, the others 0.5 a + 0.5 b. Each is marked dirty.
#[test]
fn blend_ops_weights_match_the_asm() {
    let _pc24 = Precision::Pc24.scope();
    const WORDS: [u32; 13] = [0x18, 0x28, 0x38, 0x48, 0x58, 0x68, 0x78, 0x88, 0x98, 0xA8, 0xB8, 0xE8, 0xF8];
    const WEIGHTED_A: [u32; 4] = [0x48, 0x58, 0x68, 0x78];
    let (a, b, this) = (0x1000, 0x2000, 0x3000);
    let mut mem = Mem::new();
    for at in WORDS {
        mem.set_f32(a + at, 8.0);
        mem.set_f32(b + at, 16.0);
    }
    layout::blend_ops(&mut mem, this, a, b, 0.5, 0.25, 0.5);
    for at in WORDS {
        let want = if WEIGHTED_A.contains(&at) { 4.0 } else { 12.0 };
        assert_eq!(mem.f32(this + at), want, "word +{at:#x}");
        assert_eq!(mem.u32(this + at - 4), 1, "dirty flag of +{at:#x}");
    }
}

/// The recorded `Invoke` calls of a signal emit, in order: each must be the connection's vtable slot 1, thiscall on the
/// connection with the payload (the fuzzed connections all point at a plain `ret 4`).
struct InvokeReplay<'a> {
    call: &'a Call,
    next: usize,
}

impl SignalHost for InvokeReplay<'_> {
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32) {
        let inner = &self.call.inner[self.next];
        self.next += 1;
        assert_eq!(inner.addr, mem.u32(mem.u32(conn) + 4), "Invoke target");
        assert_eq!(inner.args, [conn, sig], "Invoke arguments");
    }

    fn log_recursive(&mut self) {
        unreachable!("the recorded guard is clear")
    }

    fn log_range(&mut self, _count: u32) {
        unreachable!("the recorded count is in range")
    }
}

/// `CMultiSignal1<CSigTyreNoise>::Emit` (0x534f80): the signal walk of 0x526580 (`physics_core::signal::emit`).
#[test]
fn tyre_noise_emit_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x534F80, "CMultiSignal1<CSigTyreNoise>::Emit", |mem, c| {
        let mut host = InvokeReplay { call: c, next: 0 };
        signal::emit(mem, &mut host, c.args[0], c.args[1]);
        assert_eq!(host.next, c.inner.len(), "invokes made");
    });
}

/// `CTyre::CScratch::EmitNoise` (0x535890): the emit on `g_signalManager + 0x6268` with the scratch's block (+0x420).
#[test]
fn emit_noise_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x535890, "CScratch::EmitNoise", |mem, c| {
        let mut host = InvokeReplay { call: c, next: 0 };
        tyre::layout::emit_noise(mem, &mut host, c.args[0]);
        assert_eq!(host.next, c.inner.len(), "invokes made");
        assert_eq!(host.call.inner.iter().filter(|i| i.args[1] != c.args[0] + 0x420).count(), 0, "payload");
    });
}

/// `CMultiSignal1<CSigTireParticles>::Emit` (0x53d0d0): the same walk on the tire-particle signal.
#[test]
fn tire_particles_emit_matches_original_calls() {
    let _pc24 = Precision::Pc24.scope();
    replay(0x53D0D0, "CMultiSignal1<CSigTireParticles>::Emit", |mem, c| {
        let mut host = InvokeReplay { call: c, next: 0 };
        signal::emit(mem, &mut host, c.args[0], c.args[1]);
        assert_eq!(host.next, c.inner.len(), "invokes made");
    });
}

/// `CWheel::Vf28` (0x53f230) with a fake host: the calls in the asm's order, each at its entry stack pointer, and the
/// payload's word 0 (wheel+0x30). No record covers it, so the order and the depths are read from the asm; the body's
/// frame is `FRAME_AT`.
#[test]
fn finish_calls_in_original_order() {
    let _pc24 = Precision::Pc24.scope();
    const WHEEL: u32 = 0x0BF1_F520;
    const FRAME_AT: u32 = 0x0BF0_0000;
    const SIGNAL_MANAGER: u32 = 0x0089_3600;
    const SIGNAL_BASE: u32 = 0x0BE0_0000;

    #[derive(Default)]
    struct Log(Vec<String>);

    impl FinishCalls for Log {
        fn reset_outputs(&mut self, strut: u32, sp: u32) {
            self.0.push(format!("reset_outputs {strut:#x} sp {sp:#x}"));
        }

        fn tyre_tail(&mut self, scratch: u32, sp: u32) {
            self.0.push(format!("tyre_tail {scratch:#x} sp {sp:#x}"));
        }

        fn step77(&mut self, this: u32, a: u32, b: u32, sp: u32) {
            self.0.push(format!("step77 {this:#x} {a:#x} {b:#x} sp {sp:#x}"));
        }

        fn emit(&mut self, signal: u32, payload: u32, sp: u32) {
            self.0.push(format!("emit {signal:#x} {payload:#x} sp {sp:#x}"));
        }

        fn emit_noise(&mut self, scratch: u32, sp: u32) {
            self.0.push(format!("emit_noise {scratch:#x} sp {sp:#x}"));
        }
    }

    let mut mem = Mem::new();
    mem.map(FRAME_AT - 0x100, &[0; 0x400]);
    mem.map(WHEEL, &[0; 0x1000]);
    mem.map(SIGNAL_MANAGER, &[0; 4]);
    mem.map(SIGNAL_BASE, &[0; 0x300]);
    mem.map(0x0072_AE44, &[0; 4]);
    mem.set_u32(SIGNAL_MANAGER, SIGNAL_BASE);
    mem.set_u32(WHEEL + 0x30, 0xCAFE_0001);
    mem.set_u32(0x0072_AE44, 0x40A0_0000);
    // The flag word M3B880 reads, with bit 0x400 clear: its eax is the word.
    const SETTINGS: u32 = 0x0BD0_0000;
    mem.map(layout::DEBUG_GLOBAL, &[0; 4]);
    mem.map(SETTINGS, &[0; 0x28]);
    mem.map(SETTINGS + 0x160E0, &[0; 4]);
    mem.set_u32(layout::DEBUG_GLOBAL, SETTINGS);
    mem.set_u32(SETTINGS + 0x24, SETTINGS);
    mem.set_u32(SETTINGS + 0x160E0, 0x55);
    let f = FRAME_AT;

    let mut log = Log::default();
    let eax = layout::finish(&mut mem, &mut log, WHEEL, f);

    assert_eq!(eax, 0x55, "the result is M3B880's");
    let sig = SIGNAL_BASE + 0x29E0;
    assert_eq!(
        log.0,
        [
            format!("reset_outputs {:#x} sp {:#x}", WHEEL + 0xEC4, f - 4),
            format!("tyre_tail {:#x} sp {:#x}", WHEEL + 0xF00, f - 4),
            format!("step77 {WHEEL:#x} {:#x} {:#x} sp {:#x}", f + 0x90, f + 0x94, f - 12),
            format!("emit {sig:#x} {:#x} sp {:#x}", f + 0x40, f - 8),
            format!("emit_noise {:#x} sp {:#x}", WHEEL + 0xF00, f - 4),
        ]
    );
    assert_eq!(mem.u32(f + 0x40), 0xCAFE_0001, "payload word 0 is the wheel's key");
    assert_eq!(mem.u32(f + 0x94), 0x40A0_0000, "Step77's b is the constant at 0x72ae44");
}

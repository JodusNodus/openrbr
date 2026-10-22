//! The wheel's axis, lerp, setter, tag and transform calls (`M3A8A0`-`M3D460`, `Copy100`, `Blend4`, `Unk9290`,
//! `Unk97a0`) and the slot forwards.

use super::fields::{
    AXLE, BLEND_C_COPY_A40, BLEND_C_COPY_DE0, BLEND_D_COPY_A20, BLEND_D_COPY_E00, BLOCK_CLAMP_A, BLOCK_CLAMP_B,
    BLOCK_DIFF, BLOCK_TAG_1, BLOCK_TAG_2, BLOCK_TARGET, BLOCK_TOTAL, BLOCK_WEIGHT, CAR_FLAG_XFORM, CLEAR_190,
    CLEAR_1C0, CONTACT_A70, CONTACT_A80, CROSS_ADD, DEF_FLAGS, DEF_PAIR_TAG, FLAG_PAIR_TAG, FORWARD_INT_A10,
    FORWARD_VEC_9F0, FORWARD_VEC_A00, FRAME_AXIS, HUB_POSITION, INV_ARM, INV_RACK, NAME_F28, NAME_F38, ORIENTATION,
    SPIN_RATE, STRUT, STRUT_DIR, STRUT_LENGTH, STRUT_TWEAK, SUB_A, SUB_A_FLAG, SUB_A_SCALE, SUB_B, SUB_B_FLAG,
    SUB_B_SCALE, WALK_OUT, XFORM_CAR, XFORM_PAIRS,
};
use super::host::{transform_call, CarSlotCalls, CarXform, SlotCalls, StrutCalls, SubCalls, WalkCalls};
use super::probe::PROBE_CLEAR;
use super::store::WheelStore;
use crate::car_body::store::CarStore;
use crate::car_body::transform::FRAME;
use crate::math::vec::Vec4;
use crate::ngp;
use crate::wheel::axes::{self, Trig};
use crate::wheel::axis_force::{self, AxisForceIn};
use crate::wheel::{contact, hub, ray, strut, walk};

/// `M3D460` (0x53d460): when the CDef's flag bit is set, the block takes the CDef's pair tag for both tags and a zero
/// weight (written before `M3D230`). Then the blend runs, NGP's (`ngp::layout::tag_blend`, its call at `0x53d49d`; 1.02's
/// `M3D230` is not run), and the difference `target - total` is read (after the call) and stored at `+0x64`; the flag
/// limit of `ray::limit_ray` follows.
pub fn pose_flag<S: WheelStore>(mem: &mut S, blk: u32, b: f32, c: f32) {
    let def = mem.world_def();
    if mem.u32(def + DEF_FLAGS) & FLAG_PAIR_TAG != 0 {
        let tag = mem.u32(def + DEF_PAIR_TAG);
        mem.set_u32(blk + BLOCK_TAG_1, tag);
        mem.set_u32(blk + BLOCK_TAG_2, tag);
        mem.set_f32(blk + BLOCK_WEIGHT, 0.0);
    }
    ngp::layout::tag_blend::update(mem.mem_mut(), blk);
    let d = mem.f32(blk + BLOCK_TARGET) - mem.f32(blk + BLOCK_TOTAL);
    let out = ray::limit_ray(d, b, c);
    mem.set_f32(blk + BLOCK_DIFF, out.difference);
    mem.set_u32(blk + PROBE_CLEAR, u32::from(out.valid));
    if out.reset_limits {
        mem.set_f32(blk + BLOCK_CLAMP_A, 0.0);
        mem.set_f32(blk + BLOCK_CLAMP_B, 0.0);
    }
}

// The edge functions: each runs one original function in its order. They read the inputs and store the results between
// the host calls, as the asm does; the ports they call are pure. The tests and the hybrid binding call these.

/// `Copy100` (0x5390d0): `x` is `Xform6870` (`xform_6870`, on the car) of the tyre's centre, then the orientation is copied
/// to `out`. The asm's test of `this+0xf0` (its address, never zero) always takes `this+0x100`.
pub fn transform_and_copy<S: WheelStore>(
    mem: &mut S,
    this: u32,
    x_at: u32,
    out_at: u32,
    xform_6870: impl FnOnce(u32, Vec4) -> Vec4,
) {
    let tmp = hub::tyre_centre(mem.load_at(this, AXLE), mem.load_hub_offset(this), mem.load_at(this, HUB_POSITION));
    let x = xform_6870(mem.load_car(this), tmp);
    x.store(mem, x_at);
    mem.load_at(this, ORIENTATION).store(mem, out_at);
}

/// `Blend4` (0x539180): `c` is stored before `a` and `b` are read again for `d`, as the asm does, so an overlap of `c`
/// with an operand gives the asm's `d`.
pub fn blend_pair<S: WheelStore>(mem: &mut S, this: u32, a_at: u32, b_at: u32, c_at: u32, d_at: u32) {
    let k = mem.load_carcass_width(this) * 0.5;
    let c = hub::along_axle(Vec4::load(mem, a_at), k, Vec4::load(mem, b_at));
    c.store(mem, c_at);
    let d = hub::along_axle(Vec4::load(mem, a_at), 0.0 - k, Vec4::load(mem, b_at));
    d.store(mem, d_at);
    mem.store_at(this, BLEND_D_COPY_A20, d);
    mem.store_at(this, BLEND_C_COPY_A40, c);
    mem.store_at(this, BLEND_C_COPY_DE0, c);
    mem.store_at(this, BLEND_D_COPY_E00, d);
}

/// `Unk9290` (0x539290): `out` is `m * axis`, then `+ contact_a70 * second` and `+ contact_a80 * first`. Each step is
/// stored before the next reads `out`, which may overlap the contact vectors.
pub fn pick_contact<S: WheelStore>(mem: &mut S, this: u32, a_at: u32, b_at: u32, out_at: u32) {
    let axis = mem.load_at(this, FRAME_AXIS);
    let mix = contact::mix_contacts(axis, &mem.load_ray_block(a_at), &mem.load_ray_block(b_at));
    axis.scale(mix.axis_scale).store(mem, out_at);
    let v = mem.load_at(this, CONTACT_A70).scale(mix.second) + Vec4::load(mem, out_at);
    v.store(mem, out_at);
    let v = mem.load_at(this, CONTACT_A80).scale(mix.first) + Vec4::load(mem, out_at);
    v.store(mem, out_at);
}

/// `M3A8A0` (0x53a8a0): the car's eight transforms into the CDef's axes. The car is `this+0x44` when that is set and its
/// flags carry bit 3, else 0 (the calls still go to it). Each pair's source is read just before its call, after the calls
/// before it, and its result is stored after the call.
pub fn car_frames<S: WheelStore>(host: &mut impl CarXform, mem: &mut S, this: u32) {
    let obj = mem.load_xform_car(this);
    let car = if obj != 0 && mem.car_flags(obj) & CAR_FLAG_XFORM != 0 { obj } else { 0 };
    for (i, &(guard, src, out)) in XFORM_PAIRS.iter().enumerate() {
        let def = mem.load_def(this);
        let a = (def.wrapping_add(guard) != 0).then(|| Vec4::load(mem, def + src));
        let v = transform_call(host, i, car, a);
        v.store(mem, def + out);
    }
}

/// `M3AEF0` (0x53aef0): the walk `MB560` on the world frame (its `c` is the walk output), then `offset * gain + walk`
/// stored to the walk output. `GetB1C` is read only after the walk call, when `|ratio_den| > 0.001` (as the asm does).
pub fn walk_result<S: WheelStore + CarStore>(host: &mut impl WalkCalls, mem: &mut S, this: u32) -> Vec4 {
    let car = mem.load_car(this);
    let a = (car.wrapping_add(0x180) != 0).then(|| Vec4::load(mem, car + 0x190));
    let walk = host.pair_walk(mem.u32(FRAME), a, car);
    let gain =
        walk::steer_gain(mem.load_f32(this, INV_ARM), mem.load_f32(this, INV_RACK), || mem.steer_target_rate(car));
    let v = walk::steered_walk(mem.load_at(this, STRUT_DIR), gain, walk);
    mem.store_at(this, WALK_OUT, v);
    v
}

/// `M3AFB0` (0x53afb0): runs `walk_result`, then writes the spin's cross product to the caller's `p`.
pub fn spin_cross<S: WheelStore + CarStore>(host: &mut impl WalkCalls, mem: &mut S, this: u32, p: u32) {
    let walk = walk_result(host, mem, this);
    let cross = walk::spin_cross(walk, mem.load_at(this, AXLE), mem.load_f32(this, SPIN_RATE));
    cross.store(mem, p);
}

/// `M3AB90` (0x53ab90): the deformed top mount, stored to `+0x570`.
pub fn state_lerp<S: WheelStore>(mem: &mut S, this: u32) {
    let v = axes::deformed_top_mount(&mem.load_top_mount_deform(this));
    mem.store_deformed_top_mount(this, v);
}

/// `M3A9E0` (0x53a9e0): the wobbled axle, stored to `+0x5f0`.
pub fn turn_axis<S: WheelStore>(trig: &mut impl Trig, mem: &mut S, this: u32) {
    let v = axes::wobbled_axle(trig, &mem.load_axle_wobble(this));
    mem.store_wobbled_axle(this, v);
}

/// `M3AC50` (0x53ac50): the torque on the hub. The hub damage and the axle are stored when `v` is above the CDef's
/// threshold.
pub fn set_input<S: WheelStore>(trig: &mut impl Trig, mem: &mut S, this: u32, v: f32) {
    let update = axes::apply_hub_torque(trig, &mem.load_hub_damage(this, v));
    mem.store_hub_damage(this, update.as_ref());
}

/// `M3D030` (0x53d030): the car frames, the state lerp and the axes, then the strut's two names (`GetName`, each read
/// after its call), `SetDirty`, the dirty sum (stored before `M3B900`, which reads it), `M3B900` and `UpdateAndReset`.
pub fn update_strut<S: WheelStore>(
    host: &mut (impl CarXform + StrutCalls),
    trig: &mut impl Trig,
    mem: &mut S,
    this: u32,
) {
    car_frames(host, mem, this);
    let deform = mem.load_top_mount_deform(this);
    mem.store_deformed_top_mount(this, axes::deformed_top_mount(&deform));
    let inclination = mem.load_inclination(this);
    mem.store_inclined_axle(this, &axes::inclined_axle(trig, &inclination));
    let strut = this + STRUT;
    let name = host.strut_name(strut);
    let f1 = mem.f32(name + NAME_F28);
    let name = host.strut_name(strut);
    let f2 = mem.f32(name + NAME_F38);
    let sum = strut::rest_travel(f1, f2);
    host.mark_dirty(this + STRUT_TWEAK);
    mem.set_f32(this + STRUT_LENGTH, sum);
    host.apply_dirty_sum(this, sum);
    host.update_and_reset(strut);
}

/// `CWheel::E43Vf20` (0x539090, vtable slot 8): clears the vectors at `+0x1c0` and `+0x190`, calls the wheel's `Vf40`
/// (`wheel_vf40`, 0x53b5b0), then forwards `p` to `CSolid::E43Vf20` (`solid_vf20`, 0x529200).
pub fn clear_and_forward<S: WheelStore>(host: &mut impl SlotCalls, mem: &mut S, this: u32, p: u32) {
    mem.store_at(this, CLEAR_1C0, Vec4::default());
    mem.store_at(this, CLEAR_190, Vec4::default());
    host.wheel_vf40(this);
    host.solid_vf20(this, p);
}

/// `CWheel::E43Vf5C` (0x539a90, vtable slot 23): the car's slot 28 with `this+0x9f0`, then its slot 29 with `this+0xa00`.
/// The asm reads the car again before the second call. Then both vectors are cleared and `+0xa10` is zeroed.
pub fn forward_to_car_and_clear<S: WheelStore>(host: &mut impl CarSlotCalls, mem: &mut S, this: u32) {
    let car = mem.u32(this + XFORM_CAR);
    host.car_vf70(car, this + FORWARD_VEC_9F0);
    let car = mem.u32(this + XFORM_CAR);
    host.car_vf74(car, this + FORWARD_VEC_A00);
    mem.store_at(this, FORWARD_VEC_9F0, Vec4::default());
    mem.store_at(this, FORWARD_VEC_A00, Vec4::default());
    mem.store_u32(this, FORWARD_INT_A10, 0);
}

/// The sub-object's axis (`+0x20`) and scale (`+0x5c`, the gate's scale), and the wheel's copy of `Unk94d0`'s
/// remainder (`+0xe40`).
const SUB_AXIS: u32 = 0x20;
const SUB_SCALE: u32 = 0x5C;
const SUB_REST: u32 = 0xE40;
/// Where `Unk94d0` reads its remainder's lane 3: this far below its aligned `ebp` (`(sp - 4) & !0xf`).
const SUB_RESIDUE: u32 = 0x14;

/// `CWheel::Unk94d0` (0x5394d0, thiscall on the wheel; stack `sub`, `v`, `acc` and two words it does not read; `ret
/// 0x14`) entered at `sp`: `axis_force::axis_force` of the vector at `v` on the sub-object `sub`, into the accumulator at `acc` and
/// `this+0xe40` (name ours). The frame's temporaries are not written. Returns eax, the copy's destination.
pub fn split_on_sub<S: WheelStore>(mem: &mut S, this: u32, sub: u32, v: u32, acc: u32, sp: u32) -> u32 {
    let input = AxisForceIn {
        axis: mem.load_at(sub, SUB_AXIS),
        scale: mem.f32(sub + SUB_SCALE),
        spin: mem.load_at(this, CONTACT_A80),
        velocity: mem.load_at(v, 0),
        force: mem.load_at(acc, 0),
        residue: mem.f32(((sp - 4) & !0xF) - SUB_RESIDUE),
    };
    let out = axis_force::axis_force(&input);
    mem.store_at(acc, 0, out.force);
    mem.store_at(this, SUB_REST, out.across);
    this + SUB_REST
}

/// Whether a sub-object is gated on: its flag is nonzero and its scale is not zero (a NaN scale counts as nonzero, as the
/// asm's `ucomiss` pair gives).
fn gated_on<S: WheelStore>(mem: &S, this: u32, flag: u32, scale: u32) -> bool {
    mem.u32(this + flag) != 0 && mem.f32(this + scale) != 0.0
}

/// `CWheel::Unk97a0` (0x5397a0, thiscall, stack `a`, `b`, `c`; returns 1 when a sub-object is gated on) (name ours).
/// The vector `v` is the cross of `+0xa70` and `+0xa80`, scaled by `+0xa50 * 0.2`, plus `+0xa60..+0xa68`. Each gated-on
/// sub-object gets `v` through `sub_unk94d0`. The port sets the cross's lane 3 and `v`'s lane 3 to 0. The asm's
/// four-lane multiply carries the stack's old lane 3 through the cross into `v`, but `Unk94d0` reads only `v`'s first
/// three lanes, so it has no effect.
pub fn update_sub_objects<S: WheelStore>(
    host: &mut impl SubCalls,
    mem: &mut S,
    this: u32,
    a: u32,
    b: u32,
    c: u32,
) -> bool {
    let on_a = gated_on(mem, this, SUB_A_FLAG, SUB_A_SCALE);
    let on_b = gated_on(mem, this, SUB_B_FLAG, SUB_B_SCALE);
    if !on_a && !on_b {
        return false;
    }
    let cross = mem.load_at(this, CONTACT_A70).cross3(mem.load_at(this, CONTACT_A80));
    let k = mem.f32(this + SPIN_RATE) * 0.2;
    let v = Vec4 {
        x: mem.f32(this + CROSS_ADD) + k * cross.x,
        y: mem.f32(this + CROSS_ADD + 4) + k * cross.y,
        z: mem.f32(this + CROSS_ADD + 8) + k * cross.z,
        w: 0.0,
    };
    if on_a {
        host.sub_unk94d0(this, this + SUB_A, v, a, b, c);
    }
    if on_b {
        host.sub_unk94d0(this, this + SUB_B, v, a, b, c);
    }
    true
}

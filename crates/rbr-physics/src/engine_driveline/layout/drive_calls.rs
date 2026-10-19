//! `CDrive`'s and `CDriveLine`'s small calls: the lanes, the drive line samples, the record getters and the
//! `E39` matrix helpers.

use super::drive_objects::{DriveObjects, Lane};
use super::engine_calls::rpm_above_40;
use crate::engine_driveline::store::EngineStore;
use crate::engine_driveline::{drive_line, mat4};
use crate::fpu;
use crate::math::ftol::ftol2;
use crate::math::le_or_unordered;
use crate::math::vec::Vec4;
use crate::ntweak::set_dirty;
use crate::vcu::locks::lock_output;

/// `E39_ClearLanes` (0x502c00, thiscall, no arguments): the lanes at `this+0x10` and `this+0x44` are set dirty and
/// zeroed (the original's `SetDirty` on each, then a float zero stored into its value).
pub fn clear_lanes<S: EngineStore>(mem: &mut S, this: u32) {
    for at in [this + 0x10, this + 0x44] {
        let lane = Lane { dirty: 1, value: 0.0 };
        lane.store(mem, at);
    }
}

/// `E12_SampleX` (0x475ea0, stdcall `(x)`, st0) on the drive line at the global `DRIVE_LINE`: the count and each entry's
/// arc length. None where the original asserts (see `drive_line::sample_x`).
pub fn sample_x<S: EngineStore>(mem: &S, x: f32) -> Option<f32> {
    let this = mem.u32(crate::stage::layout::DRIVE_LINE);
    drive_line::sample_x(mem.u32(this + drive_line::COUNT_AT), |k| mem.f32(drive_line::entry_at(this, k) + 0x20), x)
}

/// `CDriveLine::SampleBig` (0x50b300, thiscall `(pos, x)`, st0): the arc position of the point at `pos` on the drive
/// line, bisected from the index `x`. The reads are the point's three floats and the table (`ClampMix`, `BlendB` and
/// the entries' positions and tangents). The original traps where the index is outside the table, and so does this.
pub fn sample_big<S: EngineStore>(mem: &S, this: u32, pos: u32, x: f32) -> f32 {
    let n = mem.u32(this + drive_line::COUNT_AT) as i32;
    if n == 0 {
        return 0.0;
    }
    let ps = [mem.f32(pos), mem.f32(pos + 4), mem.f32(pos + 8)];
    let q_at = |k: i32| drive_line::entry_at(this, k as u32);
    let ix = fpu::trunc_i32(x as f64);
    assert!(ix >= 0, "CDriveLine::SampleBig: index {ix} outside the table (the original traps)");
    let va = if ix < n { drive_line::clamp_mix(mem, this, x) } else { Vec4::load(mem, q_at(n - 1)) };
    let d = Vec4 { x: va.x - ps[0], y: va.y - ps[1], z: va.z - ps[2], w: 0.0 };
    let h = (d.length3() - 6.0) * 0.5;
    let k = match ftol2(h as f64) as i32 {
        k if k > 0 => k,
        _ => 1,
    };
    // The asm's `jns` (0x50b3e2) tests the flags of `sub ebx, eax`: a negative start is 0 (`lea`/`mov` leave them).
    let ib = ix.wrapping_sub(k).max(0);
    let end = ix.wrapping_add(k);
    let cnt = if end < n { end } else { n - 1 };
    assert!((ib as u32) < n as u32, "CDriveLine::SampleBig: index {ib} outside the table (the original traps)");

    // The nearest entry in ib..=cnt by squared distance from the point (the first wins a tie).
    let offset = |k: i32| {
        let q = Vec4::load(mem, q_at(k));
        [ps[0] - q.x, ps[1] - q.y, ps[2] - q.z]
    };
    let dist = |e: [f32; 3]| (e[2] * e[2] + e[1] * e[1]) + e[0] * e[0];
    let first = ib;
    let mut ib = ib;
    let mut best = dist(offset(ib));
    for u in first + 1..=cnt {
        let dd = dist(offset(u));
        if best > dd {
            best = dd;
            ib = u;
        }
    }
    if ib != 0 {
        if ib == n - 1 {
            ib -= 1;
        } else {
            // A tangent that points back (its dot is negative) steps the entry back one.
            let e = offset(ib);
            let v = Vec4::load(mem, q_at(ib) + 0x10);
            if (v.y * e[1] + v.z * e[2]) + v.x * e[0] < 0.0 {
                ib -= 1;
            }
        }
    }
    if ib + 1 >= n {
        return (n - 1) as f32;
    }

    // The bisection between the two tangents of ib and ib + 1: the mid point, until it is close or the steps run out.
    let v_lo = Vec4::load(mem, q_at(ib) + 0x10);
    let v_hi = Vec4::load(mem, q_at(ib + 1) + 0x10);
    let (mut f20, mut f24) = (0.0f32, 1.0f32);
    let mut steps = 0;
    loop {
        let mid = (f24 + f20) * 0.5;
        let vb = drive_line::blend_b(mem, this, ib, mid);
        let d = [ps[0] - vb.x, ps[1] - vb.y, ps[2] - vb.z];
        let w = v_lo.lerp(v_hi, mid);
        let dot = (w.z * d[2] + w.y * d[1]) + d[0] * w.x;
        let ad = if dot < 0.0 { -dot } else { dot };
        let at = ib as f32 + mid;
        if 0.01 >= ad {
            return at;
        }
        let taken = steps;
        steps += 1;
        if taken as u32 > 8 {
            return at;
        }
        // The asm's `jbe`: a NaN dot takes the lower bound too.
        if !(dot > 0.0) {
            f24 = mid;
        } else {
            f20 = mid;
        }
    }
}

/// `CDrive::M6DB0` (0x506db0): the float at `+0x64` of record `a` (0xc8 bytes) of the table at `this+8`.
pub fn m6db0<S: EngineStore>(mem: &S, this: u32, a: u32) -> f32 {
    mem.drive_record_f32(this, a, 0x64)
}

/// `CDrive::M6DD0` (0x506dd0): the float at `+0x54` of record `a` (0xc8 bytes) of the table at `this+8`.
pub fn m6dd0<S: EngineStore>(mem: &S, this: u32, a: u32) -> f32 {
    mem.drive_record_f32(this, a, 0x54)
}

/// `CDrive::SetTwo` (0x5043c0): sets the `+0xc` of the two sub-objects at `this + a * 0x10 + 0xe8` (to `x`) and
/// `this + a * 0x10 + 0x128` (to `y`), each dirtied first. Returns the eax the original leaves (`a * 0x10`).
pub fn set_two<S: EngineStore>(mem: &mut S, this: u32, a: u32, x: f32, y: f32) -> u32 {
    let base = this.wrapping_add(a.wrapping_mul(0x10));
    for (at, v) in [(base.wrapping_add(0xe8), x), (base.wrapping_add(0x128), y)] {
        set_dirty(mem.mem_mut(), at);
        mem.set_f32(at + 0xc, v);
    }
    a.wrapping_mul(0x10)
}

/// `CDrive::Set4570` (0x504570): the diff lock of channel `a` (0 centre, 1 front, 2 rear) for `value`, clamped to
/// [0, 1]. The lock record `a` (0x14 bytes at `this+8`) takes `vcu::locks::lock_output` at `+0x18`, with its ratio
/// at `+0xc` and the maximum torque at `+0x58` of def entry `a` (0x60 bytes at `this+4`). It is enabled when the word
/// at `[this+0x170]+0x90` is nonzero; otherwise the lock is 0. Returns the eax the original leaves: the state pointer
/// when disabled, `a * 0x60` when enabled.
pub fn set_diff_lock<S: EngineStore>(mem: &mut S, this: u32, a: u32, value: f32) -> u32 {
    let objects = DriveObjects::at(mem, this);
    let rec = objects.state.wrapping_add(a.wrapping_mul(0x14));
    if mem.u32(mem.u32(this + 0x170).wrapping_add(0x90)) == 0 {
        mem.set_f32(rec + 0x18, 0.0);
        return objects.state;
    }
    let ratio = mem.f32(rec + 0xc);
    let max_torque = mem.f32(objects.def.wrapping_add(a.wrapping_mul(0x60)).wrapping_add(0x58));
    mem.set_f32(rec + 0x18, lock_output(value, ratio, true, max_torque));
    a.wrapping_mul(0x60)
}

/// `CDrive::Test4840` (0x504840): 1 when the drive's gate holds, else 0 (name ours). It counts the gear scales from the second
/// (the words at worker `+0x48`, indices 2 up to the gear count at def `+0x654`) above 0.2 (0x72f428). With a count,
/// the worker's enable word (`+0x78`) must be set, and then either the car's engine is on (`SimE980` of the car at the
/// state's `+0x580`, plus `+0x690`) or 0.5 (0x795e18) is not above the speed term (`+0x8c`) and the ready word (`+0x7c`)
/// is set. Last, 0.2 must not be above the torque gain (`+0x84`), a NaN gain counting as not above.
pub fn drive_gate<S: EngineStore>(mem: &S, this: u32) -> u32 {
    let objects = DriveObjects::at(mem, this);
    let worker = mem.u32(this + 0x170);
    let count = mem.u32(objects.def + 0x654);
    let above = (2..count).filter(|&i| mem.f32(worker + 0x48 + 4 * i) > 0.2).count();
    if above == 0 || mem.u32(worker + 0x78) == 0 {
        return 0;
    }
    let engine = mem.u32(objects.state + 0x580) + 0x690;
    let engine_on = rpm_above_40(mem, engine) != 0;
    if !engine_on && (0.5 > mem.f32(worker + 0x8c) || mem.u32(worker + 0x7c) == 0) {
        return 0;
    }
    u32::from(le_or_unordered(0.2, mem.f32(worker + 0x84)))
}

/// `E39_MgrD` (0x502400, name ours): the matrix at `m` (its four columns, 16 bytes each) times the vector at `v`,
/// `out = c0 * v.x + c1 * v.y + c2 * v.z + c3 * v.w` lane by lane in the asm's order. Returns eax, the output pointer.
pub fn mat4_mul_vec4<S: EngineStore>(mem: &mut S, m: u32, v: u32, out: u32) -> u32 {
    let c: [[f32; 4]; 4] = std::array::from_fn(|k| mem.f32s(m + 0x10 * k as u32));
    let s: [f32; 4] = mem.f32s(v);
    let lanes: [f32; 4] = std::array::from_fn(|i| c[0][i] * s[0] + c[1][i] * s[1] + c[2][i] * s[2] + c[3][i] * s[3]);
    for (i, lane) in lanes.into_iter().enumerate() {
        mem.set_f32(out + 4 * i as u32, lane);
    }
    out
}

/// `E39_MgrD` (0x502460, name ours): copies row `idx` (16 bytes at `table + 0x10 idx`) to `out` as four words. Returns
/// eax, `out`.
pub fn copy_row<S: EngineStore>(mem: &mut S, table: u32, idx: u32, out: u32) -> u32 {
    let row = table + idx.wrapping_mul(0x10);
    for k in 0..4 {
        let w = mem.u32(row + 4 * k);
        mem.set_u32(out + 4 * k, w);
    }
    out
}

/// `E39_RowsCopy` (0x502570, name ours): the 16 words at `this` are copied to `other` transposed (`mat4::transpose`).
pub fn rows_copy<S: EngineStore>(mem: &mut S, this: u32, other: u32) {
    let m: [f32; 16] = mem.f32s(this);
    for (k, v) in mat4::transpose(m).into_iter().enumerate() {
        mem.set_f32(other + 4 * k as u32, v);
    }
}

/// `E39_Step4990` (0x504990, name ours): the matrix at `obj` (16 floats) through `mat4::inverse_transposed` into `out`. The
/// original copies `obj` into a local with `E39_RowsCopy` first; the port reads it straight.
pub fn mat4_inverse_transposed<S: EngineStore>(mem: &mut S, obj: u32, out: u32) {
    let m: [f32; 16] = mem.f32s(obj);
    for (k, v) in mat4::inverse_transposed(m).into_iter().enumerate() {
        mem.set_f32(out + 4 * k as u32, v);
    }
}

/// `E39_MgrB` (0x502060, name ours): the field at `+0xc` of entry `idx` (20 bytes each) of the table at `this`.
pub fn mgr_b_field_at(this: u32, idx: u32) -> u32 {
    this.wrapping_add(idx.wrapping_mul(20)).wrapping_add(0xc)
}

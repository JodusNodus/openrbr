//! The E40 drive workers (`RBR_E40_Mid.cpp`, `RBR_E40_Big.cpp`): group 6 of the plan. Each worker works on the typed
//! `DriveWorker` (see `layout.rs`); the calls it makes outside the port are a host trait, answered in the original's
//! order (the tests replay the recorded calls). Names are ours.

use super::drive_leaf::{
    axle_pair_stages, clutch_flags, clutch_values, gear_speed_error, pair_axle_stage, pair_clutch_values, pair_latch,
    pair_slip_value, sign,
};
use super::layout::{DriveWorker, GearWork, RatioRing};
use crate::math::{abs_asm, le_or_unordered};
use crate::ngp;

/// The calls `CDrive::M79B0` makes outside the port.
pub trait GearHost {
    /// `CCar::Get30` (0x4e67f0, thiscall) on the car at `car`: returns the car's word at `+0x30`.
    fn car_get30(&mut self, car: u32) -> u32;
    /// `CMultiSignal1<const CSigMrRCarGearChange&>::Emit` (0x506f20) on the signal manager's slot, with the four-word
    /// payload (the original passes the address of the payload on the stack).
    fn emit_gear_change(&mut self, payload: [u32; 4]);
}

/// The call `CDrive::M7C10` makes outside the port: `E39_Step4990` (0x504990, group 7), the matrix `mat` in and the
/// matrix it writes out.
pub trait Step4990Host {
    fn mat4_inverse_transposed(&mut self, mat: [f32; 16]) -> [f32; 16];
}

/// The call `CDrive::M9BC0` makes to `E39_MatVec` (0x502400, group 7): the matrix and the vector in, the product out.
pub trait MatVecHost {
    fn mat_vec(&mut self, m: [f32; 16], v: [f32; 4]) -> [f32; 4];
}

/// The call `CDrive::M9BC0` makes to `E39_Step3870` (0x503870, group 7) for wheel `i`: `free_wheel_step`, kept a
/// trait so the records' calls can stand in for it.
pub trait Step3870Host {
    /// (name ours)
    fn free_wheel_step(&mut self, w: &mut DriveWorker, i: usize, dt: f32);
}

/// `CDrive::M79B0` (0x5079b0, thiscall, `x` on the stack): the gear-change signal. The payload is the car's `Get30`
/// word, the state's gear, `x`, and the gear flag.
pub fn signal_gear_change(w: &DriveWorker, host: &mut impl GearHost, x: u32) {
    let get30 = host.car_get30(w.state.car);
    let payload = [get30, w.state.drive.gear as u32, x, w.state.gear_flag.value as u32];
    host.emit_gear_change(payload);
}

/// `CDrive::M9120` (0x509120, thiscall, `dt` on the stack, int result): the gear guard. Nothing happens unless the CDef's
/// flags (`0x80000`) are set. Otherwise `dt` is compared with the band of the state's gear: below the lower edge or above
/// the upper one, the gear flag is cleared, the trigger is set and the gear change is signalled with `x = 0`. Returns
/// whether the band was left. NaN is in neither band.
pub fn gear_band_guard(w: &mut DriveWorker, g: &GearWork, host: &mut impl GearHost, dt: f32) -> bool {
    if !w.def.gear_guard {
        return false;
    }
    let lower = w.obj.scale * g.band.lower_edge * g.scale;
    let upper = w.obj.scale * g.band.upper_edge * g.scale;
    if dt < lower || upper < dt {
        w.state.gear_flag.set(0);
        w.state.trigger = 1;
        signal_gear_change(w, host, 0);
        return true;
    }
    false
}

/// `CDrive::M9210` (0x509210, thiscall, `dt` on the stack, void): the gear-logic worker. `|dt|` is taken, and with the
/// gear guard the state's random word steps and the gear's scale moves by the random term. The speed term `|dt| * 0.0333`
/// is capped at 0.8 and the state's `sqrt(term + 0.2)` is kept as the gear level and emitted as the gear change (with
/// the square root's bits). The x87 chains run at 24-bit precision (each op rounds to f32, the D3D default), so f32
/// arithmetic in the original's order matches them.
pub fn gear_logic(w: &mut DriveWorker, g: &mut GearWork, host: &mut impl GearHost, dt: f32) {
    let dt = if dt > 0.0 { dt } else { 0.0 - dt };
    if w.def.gear_guard {
        let rng = w.state.rng.wrapping_mul(0x2000081).wrapping_sub(0x19DC_0BE3);
        w.state.rng = rng;
        // The remainder is below 100001, so the original's unsigned-to-x87 correction never applies.
        let m = rng % 0x186A1;
        let rate = dt * w.obj.rate * 0.000_5;
        let step = m as f32 * 1.0e-5 * (rate * rate);
        let f = g.scale - step;
        let f = if f < 0.0 { 0.0 } else { f };
        let f = if f > 1.0 { 1.0 } else { f };
        g.scale = f;
        g.dt = dt;
    }
    let term = dt * 0.033_333_335;
    let term = if term > 0.0 { term } else { 0.0 - term };
    let level = if term < 0.0 {
        0.0
    } else if term > 0.8 {
        0.8
    } else {
        term
    };
    let sq = (level + 0.2).sqrt();
    w.this.gear_level = sq;
    signal_gear_change(w, host, sq.to_bits());
}

/// `CDrive::M8570` (0x508570, thiscall, `dt` on the stack, void): the gear trim. While the state's gear differs from its
/// target and the object's enable is set, the trim accumulator is compared with the slope and the reach. If the slope is
/// not above the accumulator and the reach is at or below it (NaN included), the trim takes the target gear and returns;
/// else, with the gear flag set, the flag clears and the gear change is signalled. Either way, the accumulator grows by
/// `dt`. With the trim off, a set flag holds the trim (`shift_speed` zero) and a clear one steps it from `gear_speed_error`.
/// NaN passes the asm's comparisons (see `cpp-fixes-from-rust.txt`).
pub fn gear_trim(w: &mut DriveWorker, g: &GearWork, host: &mut impl GearHost, dt: f32) {
    if w.state.drive.gear != w.state.drive.commanded_gear.value && w.obj.enable != 0 {
        let limit = w.state.trim_accum;
        let slope = w.def.slope_limit;
        if le_or_unordered(slope, limit) {
            let reach = w.def.reach_limit + slope;
            if le_or_unordered(reach, limit) {
                w.state.gear_flag.set(1);
                w.state.trim = 1.0;
                w.state.trigger = 1;
                w.state.shift_from = [w.state.commanded_aux, w.state.drive.commanded_gear.dirty];
                w.state.drive.gear = w.state.drive.commanded_gear.value;
                w.state.trim_accum = 0.0;
                return;
            }
            if w.state.gear_flag.value != 0 {
                w.state.gear_flag.set(0);
                w.state.trigger = 1;
                signal_gear_change(w, host, 0);
            }
        }
        w.state.trim_accum += dt;
        return;
    }
    if w.state.gear_flag.value != 0 {
        w.this.shift_speed = 0.0;
        w.this.shift_trim = w.state.trim.to_bits();
        return;
    }
    let speed_error = gear_speed_error(w, g) as f32;
    let f = abs_asm(speed_error);
    let trim = w.state.trim;
    let h = trim * trim * f * 0.025;
    let h = if h < 0.3 {
        0.3
    } else if h > 4.0 {
        4.0
    } else {
        h
    };
    let k = trim - h * dt;
    let k = if k < 0.0 {
        0.0
    } else if k > 1.0 {
        1.0
    } else {
        k
    };
    w.state.trim = k;
    w.this.shift_speed = f;
    w.this.shift_trim = w.state.trim.to_bits();
}

/// `CDrive::M93A0` (0x5093a0, thiscall, `a` and `[b, c, d]` on the stack, the out pointer's value): the clutch-torque
/// worker. Returns the value the original writes to the out pointer on this call, or `None` when the call leaves it
/// unchanged. With the gear's ratio nonzero (NaN passes), the gear flag set and the sign word clear, the trigger picks
/// one of two blocks. Clear: `|a|` against `b` decides between the gear guard (its zero result sets the speed to the
/// drive speed key times `c` and writes `a`) and setting the sign word to the sign of `a`, the trigger, and the guard
/// of the scaled torque. Set: the `f6 = a - f5 / d` path, with `f5` the slip term passed to `gear_logic`. Then, with the
/// ratio and the flag still set, the sign word is worked off against the speed term; otherwise the speed takes the
/// integrated `a` and the sign word is cleared.
///
/// NGP's rewrite (`0x10047300`, in `CDrive::Step1E0`'s call at `0x50a34b`) takes its own sign ([`ngp::sign`]) and
/// zeroes the speed when working off the sign word changes its sign.
pub fn clutch_torque(
    w: &mut DriveWorker,
    g: &mut GearWork,
    host: &mut impl GearHost,
    a: f32,
    [b, c, d]: [f32; 3],
) -> Option<f32> {
    let sign = ngp::sign;
    let mut out = None;
    let ratio = g.band.ratio;
    if ratio != 0.0 && w.state.gear_flag.value != 0 && w.state.sign_word == 0 {
        if w.state.trigger == 0 {
            if le_or_unordered(abs_asm(a), b) {
                if !gear_band_guard(w, g, host, a) {
                    w.state.drive.engine_speed = w.state.drive.drive_speed_key * c;
                    out = Some(a);
                }
            } else {
                let s = sign(a);
                w.state.sign_word = s;
                w.state.trigger = 1;
                let f4 = s as f32 * b;
                out = Some(f4);
                gear_band_guard(w, g, host, f4);
            }
        } else {
            let f7 = w.state.drive.drive_speed_key * c;
            let f5 = (w.def.slip_gain_b + w.def.slip_gain_a) * (f7 - w.state.drive.engine_speed);
            let f6 = a - f5 / d;
            if le_or_unordered(abs_asm(f6), b) {
                w.state.drive.engine_speed = f7;
                out = Some(f6);
            } else {
                let s = sign(f6);
                w.state.sign_word = s;
                w.state.trigger = 1;
                out = Some(s as f32 * b);
            }
            gear_logic(w, g, host, f5);
        }
    }
    if ratio != 0.0 && w.state.gear_flag.value != 0 {
        let word = w.state.sign_word;
        if word != 0 {
            let s = word as f32;
            let old = w.state.drive.engine_speed;
            let mut speed = (a - s * b) / w.def.slip_gain_a * d + old;
            if sign(old) != sign(speed) {
                speed = 0.0;
            }
            w.state.drive.engine_speed = speed;
            out = Some(s * b);
            let f5 = w.state.drive.drive_speed_key * c;
            if sign(speed - f5) != word {
                w.state.trigger = 1;
                w.state.sign_word = 0;
                w.state.drive.engine_speed = f5;
            }
        }
        return out;
    }
    let speed = (a / (w.def.slip_gain_b + w.def.slip_gain_a)) * d + w.state.drive.engine_speed;
    w.state.drive.engine_speed = speed;
    w.state.sign_word = 0;
    Some(0.0)
}

/// `CDrive::M7C10` (0x507c10, thiscall, void): the reset worker. Runs the clutch flags, zeroes the clutch matrix, runs
/// the clutch values for wheels 0 to 3, clears the four pack outputs and steps the matrix (`E39_Step4990`) into the step
/// matrix. It bumps the reset counter and clears the pending word. If the stepped value's first word is NaN, the whole
/// reset repeats.
pub fn reset_wheels(w: &mut DriveWorker, host: &mut impl Step4990Host) {
    reset_lanes(w);
    w.this.pack_out = [0.0; 4];
    w.state.step_matrix = host.mat4_inverse_transposed(w.state.clutch_matrix);
    w.state.reset_count.set(w.state.reset_count.value + 1.0);
    w.state.pending = 0;
    if w.state.step_matrix[0].is_nan() {
        reset_lanes(w);
        w.state.step_matrix = host.mat4_inverse_transposed(w.state.clutch_matrix);
    }
}

/// The lane reset of `M7C10`: the clutch flags, the matrix clear, and the clutch values for wheels 0 to 3.
fn reset_lanes(w: &mut DriveWorker) {
    clutch_flags(w);
    w.state.clutch_matrix = [0.0; 16];
    for x in 0..4 {
        clutch_values(w, x);
    }
}

/// `CDrive::M9BC0` (0x509bc0, thiscall, `dt` on the stack, void): the wheel worker. Resets the wheels (`M7C10`) when the
/// pending word is set. With the branch word zero it latches the wheels (`M6260`) and steps the axle pairs (`M6630`).
/// Otherwise it sets the speed terms, takes the slip values (`M5FE0`) and steps the matrix (`E39_MatVec`), repeating
/// the reset while a wheel's sign changes. It integrates the terms over `dt`, latches the wheels, sets the blocks'
/// forces and runs the clutch state (`M7D60`). Last, each block takes its slip: `E39_Step3870` when the object's enable
/// word for the wheel is zero, else the force's change over `dt`; then the previous values follow the current ones.
pub fn wheel_worker(
    w: &mut DriveWorker,
    host: &mut (impl Step4990Host + MatVecHost + Step3870Host),
    dt: f32,
    split: f32,
) {
    // The original's calls of `M6260`, `M5FE0` and `M7D60` go to NGP's rewrites (`ngp::drive`).
    let latch = |w: &mut DriveWorker, x| ngp::drive::wheel_latch(w, x, split);
    let slip = |w: &mut DriveWorker, x| ngp::drive::wheel_slip_value(w, x, split);
    if w.state.pending != 0 {
        reset_wheels(w, host);
    }
    if w.state.branch == 0 {
        w.this.loc0.set(0.0);
        for term in &mut w.state.terms[..3] {
            term.speed = 0.0;
        }
        for x in 0..4 {
            latch(w, x);
        }
        axle_pair_stages(w);
    } else {
        set_speed_terms(w);
        for x in 0..4 {
            slip(w, x);
        }
        loop {
            w.this.loc = host.mat_vec(w.state.step_matrix, w.state.step_vec);
            let mut again = false;
            for g in 0..3 {
                let li = w.def.axles[g].active_index as usize;
                if w.state.lane_active[li] == 1 {
                    let v = w.this.loc[li];
                    if abs_asm(v) > w.state.terms[g].scale {
                        w.state.terms[g].sign = sign(v);
                        again = true;
                    }
                }
            }
            if !again {
                break;
            }
            w.state.repeat_count = w.state.repeat_count.wrapping_add(1);
            reset_wheels(w, host);
            set_speed_terms(w);
            for x in 0..4 {
                slip(w, x);
            }
        }
        for g in 0..3 {
            let li = w.def.axles[g].active_index as usize;
            if w.state.lane_active[li] == 1 {
                w.state.terms[g].speed = w.this.loc[li];
            }
        }
        let loc = [0, 1, 2, 3].map(|k| if w.state.lane_active[k] == 0 { w.this.loc[k] } else { 0.0 });
        w.this.loc0.set(0.0);
        for x in 0..4 {
            latch(w, x);
        }
        w.this.loc0.set(loc[0]);
        w.state.terms[0].value += loc[0] * dt;
        for g in 0..3 {
            let li = w.def.axles[g].active_index as usize;
            w.state.terms[g].term += loc[li] * dt;
        }
        for g in 0..3 {
            if sign(w.state.terms[g].term) != w.state.terms[g].sign {
                w.state.terms[g].term = 0.0;
                w.state.terms[g].sign = 0;
                bump_dirty(w);
            }
        }
        let f16 = 0.5f32;
        let f15 = w.state.terms[0].term * f16;
        let scale0 = w.def.axles[0].scale;
        let ratio1 = w.def.axles[1].direction as f32 / scale0;
        w.state.terms[1].value = ratio1 * w.state.terms[0].value + f15;
        let ratio2 = w.def.axles[2].direction as f32 / scale0;
        w.state.terms[2].value = ratio2 * w.state.terms[0].value + f15;
        for k in 0..4 {
            if w.state.blocks[k].gate.value != 0 && w.this.pack_out[k] == 0.0 {
                let ix = w.def.wheels[k].pack as usize;
                let term = w.state.terms[ix];
                let force = (term.value / w.def.axles[ix].scale) * w.def.wheels[k].weight + term.term * f16;
                w.state.blocks[k].force.set(force);
            }
        }
        ngp::drive::clutch_state(w);
    }
    w.state.drive.drive_speed_key = w.state.terms[0].value;
    for i in 0..4 {
        if w.obj.wheel_enabled[i] == 0 {
            host.free_wheel_step(w, i, dt);
        } else {
            held_wheel_step(w, i, dt);
        }
    }
}

/// The step of a wheel the drive holds (the tail of `M9BC0` and `M96C0`): the slip is the force's change over `dt`,
/// and the previous lanes take the current ones.
fn held_wheel_step(w: &mut DriveWorker, i: usize, dt: f32) {
    let block = &w.state.blocks[i];
    let value = (block.force.value - block.prev_force.value) / dt;
    w.state.set_slip(i, value);
    let block = &mut w.state.blocks[i];
    block.prev_force_aux = block.force_aux;
    block.prev_force = block.force;
    block.prev_gate_aux = block.gate_aux;
    block.prev_gate = block.gate;
}

/// `E39_Step3870` (0x503870, `i` and `dt` on the stack): the step of free wheel `i` (one the drive does not hold). With
/// its latch (`prev_gate`) set, `prev_force` moves by the latch torque over the inertia times `dt`; a sign change
/// against the latch clears both, and the slip is the change over `dt`. With the latch clear, a wheel offset beyond the
/// latch bound sets the latch to its sign and the slip to zero.
pub fn free_wheel_step(w: &mut DriveWorker, i: usize, dt: f32) {
    let block = w.state.blocks[i];
    let offset = w.this.wheel_offset[i];
    let latch = block.prev_gate.value;
    if latch == 0 {
        if abs_asm(offset) > block.latch_bound {
            w.state.blocks[i].prev_gate.set(sign(offset));
            w.state.set_slip(i, 0.0);
        }
        return;
    }
    let old = block.prev_force.value;
    let torque = offset - (latch as f32) * block.latch_bound;
    let v = torque * (1.0 / block.inertia) * dt + old;
    let b = &mut w.state.blocks[i];
    b.prev_force.set(v);
    if sign(v) != latch {
        b.prev_force.set(0.0);
        b.prev_gate.set(0);
    }
    let slip = (b.prev_force.value - old) / dt;
    w.state.set_slip(i, slip);
}

/// `CDrive::M7AA0` (0x507aa0, thiscall, void): the pair worker's reset. The first pair flag clears; the second is set
/// when term 1's sign is clear and either block of axle 1's pair is gated. The pair matrix is zeroed and gets the rows
/// of the pair's two wheels (`M5B00`), the two words clear, and the inverse is the adjugate over the determinant
/// (`1 / (a11 a00 - a10 a01)`). The reset counter goes up by one and the pending word clears.
pub fn reset_pair(w: &mut DriveWorker) {
    let [a, b] = w.def.axles[1].block_pair.map(|k| k as usize);
    w.state.pair_active[0] = 0;
    let gated = w.state.blocks[a].gate.value != 0 || w.state.blocks[b].gate.value != 0;
    w.state.pair_active[1] = u32::from(w.state.terms[1].sign == 0 && gated);
    w.state.pair_matrix = [0.0; 4];
    pair_clutch_values(w, a);
    pair_clutch_values(w, b);
    w.state.pair_words = [0; 2];
    let [a00, a01, a10, a11] = w.state.pair_matrix;
    let inv = 1.0 / (a11 * a00 - a10 * a01);
    w.state.pair_inverse = [inv * a11, 0.0 - inv * a01, 0.0 - inv * a10, inv * a00];
    w.state.reset_count.set(w.state.reset_count.value + 1.0);
    w.state.pending = 0;
}

/// `CDrive::M96C0` (0x5096c0, thiscall, `dt` on the stack, void): the pair worker, the wheel worker of a drive of mode
/// 3 (`DriveType` 3, the two-wheel drive of the MG ZR: wheels 0 and 1 driven, 2 and 3 free). It runs on term 1 and the 2x2 pair
/// matrix where `M9BC0` runs on the three terms and the 4x4 matrix. With both blocks ungated it only latches the wheels
/// (`M60D0`) and stages the axle (`M6540`). Otherwise it solves the pair (the inverse times the slip values, `M5F10`),
/// repeating the reset (`M7AA0`) while term 1's active index is set and its MatVec output (`loc`, which nothing here
/// writes) is beyond the scale, integrates term 1 over `dt`, latches, sets the gated blocks' forces and stages the
/// axle until no block's gate disagrees with its force. The drive speed key is `0 - ` term 1's value (the asm's
/// register is zero there, not the C++'s `f14`). Wheels 0 and 1 then take their step (`E39_Step3870` or the held step,
/// by the enable word), and wheels 2 and 3 take `E39_Step3870`.
pub fn pair_worker(w: &mut DriveWorker, dt: f32) {
    w.state.drive_gain_neg = 0.0 - w.state.drive_gain;
    if w.state.pending != 0 {
        reset_pair(w);
    }
    if w.state.blocks[0].gate.value == 0 && w.state.blocks[1].gate.value == 0 {
        w.this.loc0.set(0.0);
        w.state.terms[1].speed = 0.0;
        pair_latch(w, 0);
        pair_latch(w, 1);
        pair_axle_stage(w);
    } else {
        pair_slip_values(w);
        let solved = loop {
            let [i0, i1, i2, i3] = w.state.pair_inverse;
            let [e0, e1] = w.this.pair_slip;
            let solved = [i0 * e0 + i1 * e1, i2 * e0 + i3 * e1];
            w.this.pair_loc = solved;
            let gi = w.def.axles[1].active_index as usize;
            if w.state.lane_active[gi] != 1 || !(abs_asm(w.this.loc[gi]) > w.state.terms[1].scale) {
                break solved;
            }
            w.state.terms[1].sign = sign(w.this.loc[gi]);
            w.state.repeat_count = w.state.repeat_count.wrapping_add(1);
            reset_pair(w);
            pair_slip_values(w);
        };
        if w.state.pair_active[1] == 1 {
            w.state.terms[1].speed = solved[1];
        }
        let value_rate = if w.state.pair_active[0] == 0 { solved[0] } else { 0.0 };
        let term_rate = if w.state.pair_active[1] == 0 { solved[1] } else { 0.0 };
        w.this.loc0.set(0.0);
        pair_latch(w, 0);
        pair_latch(w, 1);
        w.this.loc0.set(value_rate);
        let t = &mut w.state.terms[1];
        t.value += value_rate * dt;
        t.term += term_rate * dt;
        if sign(t.term) != t.sign {
            t.term = 0.0;
            t.sign = 0;
            bump_dirty(w);
        }
        for k in 0..2 {
            if w.state.blocks[k].gate.value != 0 && w.this.pair_pack[k] == 0.0 {
                let ix = w.def.wheels[k].pack as usize;
                let term = w.state.terms[ix];
                let force = (term.value / w.def.axles[ix].scale) * w.def.wheels[k].weight + term.term * 0.5;
                w.state.blocks[k].force.set(force);
            }
        }
        pair_axle_stage(w);
        loop {
            let mut any = false;
            for k in 0..2 {
                let block = w.state.blocks[k];
                if block.gate.value != sign(block.force.value) && w.this.pair_pack[k & 1] == 0.0 {
                    w.state.blocks[k].force.set(0.0);
                    w.state.blocks[k].gate.set(0);
                    any = true;
                }
            }
            if !any {
                break;
            }
            pair_axle_stage(w);
        }
    }
    w.state.drive.drive_speed_key = 0.0 - w.state.terms[1].value;
    for i in 0..2 {
        if w.obj.wheel_enabled[i] == 0 {
            free_wheel_step(w, i, dt);
        } else {
            held_wheel_step(w, i, dt);
        }
    }
    free_wheel_step(w, 2, dt);
    free_wheel_step(w, 3, dt);
}

/// Term 1's speed (its sign times its scale) and the two slip values (`M5F10` for wheels 0 and 1).
fn pair_slip_values(w: &mut DriveWorker) {
    let t = &mut w.state.terms[1];
    t.speed = t.sign as f32 * t.scale;
    pair_slip_value(w, 0);
    pair_slip_value(w, 1);
}

/// The speed of each of terms 0 to 2: its sign times its scale.
fn set_speed_terms(w: &mut DriveWorker) {
    for term in &mut w.state.terms[..3] {
        term.speed = term.sign as f32 * term.scale;
    }
}

/// The dirty bump of a sign change: the dirty count goes up when `pending` was clear, and `pending` is set.
pub fn bump_dirty(w: &mut DriveWorker) {
    if w.state.pending == 0 {
        w.state.dirty_count = w.state.dirty_count.wrapping_add(1);
    }
    w.state.pending = 1;
}

/// `E39_RatioCA0` (0x503ca0, stdcall, `v` and the ring on the stack, the result on x87): stores `v` in the ring's next
/// slot (the index steps and wraps to 0 at the count), then returns the mean of the first `count` values. The count is
/// at most 8, the ring's size. Group 7 (used by `Step1E0`); names are ours.
pub fn ring_average(ring: &mut RatioRing, v: f32) -> f32 {
    let n = ring.count;
    let next = ring.index.wrapping_add(1);
    let idx = if next > n.wrapping_sub(1) { 0 } else { next };
    ring.index = idx;
    ring.values[idx as usize] = v;
    let sum = (0..n).fold(0.0f32, |s, i| ring.values[i as usize] + s);
    (sum as f64 / n as f64) as f32
}

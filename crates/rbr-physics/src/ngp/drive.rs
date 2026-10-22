//! NGP's rewrites of three drive wheel workers that the wheel worker calls: [`wheel_slip_value`], [`wheel_latch`] and
//! [`clutch_state`]. The first two's drive term splits the torque between the axles by NGP's per-car split (0.5, or
//! for drive type 7 the car's split percentage / 100, capped at 1): `(1 - split)` for wheels 0 and 1, `split` for 2
//! and 3, where 1.02 takes a fixed quarter. The split factor comes back on the x87, so the product with the drive
//! gain and the sum or difference after it run at NGP's 53-bit precision, rounded to f32 once.

use super::sign;
use crate::engine_driveline::drive_leaf::{block_pair_gate, latch};
use crate::engine_driveline::layout::DriveWorker;

const HALF: f32 = 0.5;

/// Wheel `wheel`'s share of `v` by the front/rear `split`.
pub fn split_factor(v: f32, wheel: usize, split: f32) -> f32 {
    if wheel < 2 {
        (1.0 - split) * v
    } else {
        split * v
    }
}

/// The split factor of the axle term `(direction * torque scale) * g * 0.5` for wheel `x`, with `g` the axle's scale
/// times the wheel's weight; and `g`.
fn drive_factor(w: &DriveWorker, x: usize, split: f32) -> (f32, f32) {
    let wheel = w.def.wheels[x];
    let axle = w.def.axles[wheel.pack as usize];
    let g = axle.scale * wheel.weight;
    let v = (axle.direction as f32) * w.def.torque_scale * g * HALF;
    (split_factor(v, x, split), g)
}

/// As 1.02's `drive_leaf::wheel_slip_value`, with the split drive term.
pub fn wheel_slip_value(w: &mut DriveWorker, x: usize, split: f32) {
    let block = w.state.blocks[x];
    if block.gate.value == 0 {
        w.state.step_vec[x] = 0.0;
        return;
    }
    let mut f = 0.0f32;
    if w.obj.wheel_enabled[x] != 0 {
        f = (0.0 - (block.gate.value as f32) * block.latch_bound) + w.this.wheel_offset[x];
    }
    let (factor, g) = drive_factor(w, x, split);
    let pack = w.def.wheels[x].pack as usize;
    let rest = (f - w.state.terms[pack].speed) - g * HALF * w.state.terms[0].speed;
    let drive = f64::from(factor) * f64::from(w.state.drive_gain);
    w.state.step_vec[x] = (drive + f64::from(rest)) as f32;
}

/// As 1.02's `drive_leaf::wheel_latch`, with the split drive term, and a NaN `f` latches `+1` (a negated compare
/// jumps on unordered), where 1.02 zeroes the output.
pub fn wheel_latch(w: &mut DriveWorker, x: usize, split: f32) {
    let block = w.state.blocks[x];
    if block.gate.value != 0 {
        w.this.pack_out[x] = 0.0;
        return;
    }
    let (factor, g) = drive_factor(w, x, split);
    let pack = w.def.wheels[x].pack as usize;
    let gain = w.state.drive_gain - w.state.pack_gain * w.this.loc0.value;
    let slip = g * HALF * w.state.terms[0].speed + w.state.terms[pack].speed;
    let f = (f64::from(factor) * f64::from(gain) - f64::from(slip)) as f32;
    let (k, h) = if w.obj.wheel_enabled[x] == 0 { (0.0, 0.0) } else { (w.this.wheel_offset[x], block.latch_bound) };
    if !(h - k >= f) {
        latch(w, x, 1);
    } else if !(f >= 0.0 - (k + h)) {
        latch(w, x, -1);
    } else {
        w.this.pack_out[x] = 0.0;
    }
}

/// The clutch state, in passes until one changes nothing. Unlike 1.02's `drive_worker::clutch_state`: a block reset on either axle arms that axle's update, which keeps its value when the
/// pair's sum has the term's sign (1.02 compares the sum with the sign); axle 2's reset does not ask for another pass;
/// both axles open clear the signs too; the lane step is NGP's ([`lane_step`]); and the block writes leave the dirty
/// flags.
pub fn clutch_state(w: &mut DriveWorker) {
    while clutch_pass(w) {}
}

fn clutch_pass(w: &mut DriveWorker) -> bool {
    let mut count = [0u32; 3];
    let mut armed = [false; 3];
    let mut changed = false;
    let mut cleared = false;
    for k in 0..4 {
        let pack = w.def.wheels[k].pack as usize;
        let block = w.state.blocks[k];
        if block.gate.value == 0 {
            count[pack] += 1;
        } else if sign(block.force.value) != block.gate.value && w.this.pack_out[k] == 0.0 {
            w.state.blocks[k].force.value = 0.0;
            w.state.blocks[k].gate.value = 0;
            armed[pack] = true;
            changed = true;
            count[pack] += 1;
        }
    }
    for u in 1..3 {
        if block_pair_gate(w, u) {
            count[u] += 1;
            armed[u] = true;
            changed = true;
        }
    }
    if !changed {
        return false;
    }
    if w.state.pending == 0 {
        w.state.count_a = w.state.count_a.wrapping_add(1);
    }
    w.state.pending = 1;
    for u in 1..3 {
        if !(armed[u] && count[u] == 1) {
            continue;
        }
        let axle = w.def.axles[u];
        let (a, b) = (axle.block_pair[0] as usize, axle.block_pair[1] as usize);
        let (fa, fb) = (w.state.blocks[a].force.value, w.state.blocks[b].force.value);
        let sum = fb + fa;
        w.state.terms[u].term = sum;
        if sign(sum) == w.state.terms[u].sign {
            let weighted = fb * w.def.wheels[b].weight + fa * w.def.wheels[a].weight;
            w.state.terms[u].value = weighted * HALF * axle.scale;
        } else {
            clear(w, u);
            if block_pair_gate(w, u) {
                count[u] = 2;
                changed = true;
            }
        }
    }
    let scale = w.def.torque_scale;
    let dir = |w: &DriveWorker, u: usize| w.def.axles[u].direction as f32;
    let t0 = w.state.terms[0].value;
    let speed = scale * HALF * ((dir(w, 1) * w.state.terms[1].value + 0.0) + dir(w, 2) * w.state.terms[2].value);
    if !(speed.abs() > t0.abs()) {
        w.state.terms[0].value = speed;
        let sum = w.state.terms[1].value + 0.0 + w.state.terms[2].value;
        w.state.terms[0].term = sum;
        if sign(sum) != w.state.terms[0].sign {
            w.state.terms[0].term = 0.0;
            w.state.terms[0].sign = 0;
            cleared = true;
        }
    } else if count[1] == 2 && count[2] == 2 {
        for u in 0..3 {
            clear(w, u);
        }
    } else if count[1] == 2 || count[2] == 2 {
        // One axle open: term 0 from its direction, and the other axle's value from term 0.
        let (open, other) = if count[1] == 2 { (1, 2) } else { (2, 1) };
        let mut value = t0;
        let mut term = ((t0 * -2.0) / scale) * dir(w, open);
        w.state.terms[0].term = term;
        if sign(term) != w.state.terms[0].sign {
            clear(w, 0);
            (value, term) = (0.0, 0.0);
            cleared = true;
        }
        w.state.terms[other].value = (dir(w, other) / scale) * value + term * HALF;
        lane_step(w, other);
    } else {
        for (u, &n) in count.iter().enumerate().skip(1) {
            let value = (dir(w, u) / scale) * w.state.terms[0].value + w.state.terms[0].term * HALF;
            w.state.terms[u].value = value;
            if n == 1 {
                let axle = w.def.axles[u];
                let mut t = axle.block_pair[0] as usize;
                if w.state.blocks[t].gate.value != 0 {
                    t = axle.block_pair[1] as usize;
                }
                w.state.terms[u].term = ((w.def.wheels[t].weight * -2.0) / axle.scale) * value;
            }
            lane_step(w, u);
        }
    }
    changed || cleared
}

/// Clears term `u`'s value, term and sign.
fn clear(w: &mut DriveWorker, u: usize) {
    w.state.terms[u].value = 0.0;
    w.state.terms[u].term = 0.0;
    w.state.terms[u].sign = 0;
}

/// NGP's lane step of axle `x`: each latched block takes `(weight / scale) value + term / 2`, or 0
/// when that is zero or its sign is not the latch's. 1.02's `drive_leaf::block_lane_step` steps the first block only.
fn lane_step(w: &mut DriveWorker, x: usize) {
    let axle = w.def.axles[x];
    let term = w.state.terms[x];
    for t in axle.block_pair.map(|t| t as usize) {
        let gate = w.state.blocks[t].gate.value;
        if gate != 0 {
            let v = (w.def.wheels[t].weight / axle.scale) * term.value + term.term * HALF;
            w.state.blocks[t].force.value = if v == 0.0 || sign(v) != gate { 0.0 } else { v };
        }
    }
}

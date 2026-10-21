//! The drive's small workers on the drive's worker state (`DriveWorker`, see `layout`): the lane scale, the clutch
//! flags and matrices, the pair and axle stages, and the block gates.

use super::layout::{DriveWorker, GearWork, IntLane, LaneScale};

/// Sets the lane scale. The scale lane takes `x`'s scale, the scaled lane takes the scale times the definition's
/// `lane_scale_gain`, and the gate word takes `x`'s word.
pub fn set_lane_scale(w: &mut DriveWorker, x: LaneScale) {
    w.this.lane_scale.set(x.scale);
    w.this.lane_scaled.set(w.def.lane_scale_gain * x.scale);
    w.this.lane_gate = x.gate;
}

/// The engine speed over the current gear's ratio, less the definition's `ratio_gain` times the state's
/// `drive_speed_key`. Zero while the gear flag is set or the ratio is zero (NaN passes). Widened to `f64`, as the x87
/// returns it.
pub fn gear_speed_error(w: &DriveWorker, g: &GearWork) -> f64 {
    if w.state.gear_flag.value != 0 {
        return 0.0;
    }
    let ratio = g.band.ratio;
    if ratio == 0.0 {
        return 0.0;
    }
    let speed = w.state.drive.engine_speed / ratio;
    let gain = w.def.ratio_gain * w.state.drive.drive_speed_key;
    f64::from(speed - gain)
}

/// The active flags (`lane_active`, one word per axle index) from the four block gates and the axles' pair gates. The
/// branch word is 0 when nothing is gated, 2 when all four block gates and the first gate are set, else 1. The first
/// word is cleared before the writes.
pub fn clutch_flags(w: &mut DriveWorker) {
    let gate = |k: usize| w.state.blocks[k].gate.value != 0;
    let (b3, b4, b5) = (gate(0), gate(1), gate(2));
    let gated = gate(3);
    let axles = w.def.axles;
    w.state.lane_active[0] = 0;
    if !gated && !b3 && !b4 && !b5 {
        w.state.branch = 0;
        w.state.lane_active[1..].fill(0);
        return;
    }
    if gated && b3 && b4 && b5 {
        w.state.branch = 2;
        for (k, axle) in axles.iter().take(3).enumerate() {
            let flag = u32::from(w.state.terms[k].sign == 0);
            w.state.lane_active[axle.active_index as usize] = flag;
        }
        return;
    }
    w.state.branch = 1;
    let index = axles[0].active_index as usize;
    w.state.lane_active[index] = u32::from(w.state.terms[0].sign == 0);
    let clear = pair_clear(w, 1);
    let index = axles[1].active_index as usize;
    w.state.lane_active[index] = u32::from(w.state.terms[1].sign == 0 && !clear);
    let clear = pair_clear(w, 2);
    let index = axles[2].active_index as usize;
    w.state.lane_active[index] = u32::from(w.state.terms[2].sign == 0 && !clear);
}

/// Whether both block gates of axle `x`'s pair are clear.
fn pair_clear(w: &DriveWorker, x: usize) -> bool {
    w.def.axles[x].block_pair.iter().all(|&k| w.state.blocks[k as usize].gate.value == 0)
}

/// The clutch-pack values of wheel `x`, written to `clutch_matrix` at `x + 4 c` for the
/// pack's index `c` (the axle 0 index) and at `x + 4 c'` for the index `c'` of the wheel's axle. The block gate and the
/// active flags pick the value.
pub fn clutch_values(w: &mut DriveWorker, x: usize) {
    let wheel = w.def.wheels[x];
    let block = w.state.blocks[x];
    let axle = w.def.axles[wheel.pack as usize];
    let f = (axle.direction as f32) * w.def.torque_scale;
    let g = axle.scale * wheel.weight;
    let gated = block.gate.value != 0;
    let load = block.clutch_load;
    let value = if gated { load / (f * g) + w.state.pack_gain * f * g * 0.25 } else { load / (f * g) };
    w.state.clutch_matrix[x] = value;
    let c = w.def.axles[0].active_index as usize;
    let slot = x + 4 * c;
    if w.state.lane_active[c] == 0 {
        w.state.clutch_matrix[slot] = load / (g * 2.0);
    } else if !gated {
        w.state.clutch_matrix[slot] = 0.0;
    } else {
        w.state.clutch_matrix[slot] = g * 0.5;
    }
    let c = axle.active_index as usize;
    let slot = x + 4 * c;
    if w.state.lane_active[c] == 0 {
        w.state.clutch_matrix[slot] = load * 0.5;
    } else if gated {
        w.state.clutch_matrix[slot] = 1.0;
    } else {
        w.state.clutch_matrix[slot] = 0.0;
    }
}

/// The wheel latch: the output takes the wheel's weight, the block's gate is set to `sign`, and the count
/// goes up the first time `pending` is clear, which then becomes set.
pub(crate) fn latch(w: &mut DriveWorker, x: usize, sign: i32) {
    w.this.pack_out[x] = w.def.wheels[x].weight;
    w.state.blocks[x].gate = IntLane { dirty: 1, value: sign };
    if w.state.pending == 0 {
        w.state.latch_count = w.state.latch_count.wrapping_add(1);
    }
    w.state.pending = 1;
}

/// Two axle-pair stages, then the sums of the two. Stage one is the pair of axle 1
/// (`block_pair`, scale `axles[1].scale`) into term 1; stage two is axle 2's pair, reversed, into term 2. Term 0 gets
/// the sum of the two values, its value the same sum weighted with the axles' directions and the torque scale, and its
/// sign the sign of the sum.
pub fn axle_pair_stages(w: &mut DriveWorker) {
    let [a, b] = w.def.axles[1].block_pair;
    pair_stage(w, (a, b), w.def.axles[1].scale, 1);
    let [c, d] = w.def.axles[2].block_pair;
    pair_stage(w, (d, c), w.def.axles[2].scale, 2);

    let f = w.state.terms[2].value + w.state.terms[1].value;
    w.state.terms[0].term = f;
    let s34 = w.state.terms[2].value;
    let s20 = w.state.terms[1].value;
    let c = ((w.def.axles[2].direction as f32) * s34 + (w.def.axles[1].direction as f32) * s20)
        * (w.def.torque_scale * 0.5);
    w.state.terms[0].value = c;
    w.state.terms[0].sign = sign(f);
}

/// One stage of [`axle_pair_stages`]: the sum of the two blocks' forces, and the weighted sum (the wheels' weights times the forces,
/// times `scale * 0.5`). The sign is `sign(sum)`, or when the sum is exactly zero the sign of the two blocks' gates.
pub(super) fn pair_stage(w: &mut DriveWorker, (i0, i1): (u32, u32), scale: f32, term: usize) {
    let (b0, b1) = (i0 as usize, i1 as usize);
    let (g0, g1) = (w.state.blocks[b0].force.value, w.state.blocks[b1].force.value);
    let f = g0 + g1;
    let w0 = w.def.wheels[b0].weight;
    let w1 = w.def.wheels[b1].weight;
    let weighted = (w0 * g0 + w1 * g1) * (scale * 0.5);
    let gate = w.state.blocks[b0].gate.value.wrapping_add(w.state.blocks[b1].gate.value);
    let s = if f == 0.0 { gate.signum() } else { sign(f) };
    let t = &mut w.state.terms[term];
    t.term = f;
    t.value = weighted;
    t.sign = s;
}

/// The pair worker's axle stage, `pair_stage` of axle 1's pair taken in reverse into term 1.
pub fn pair_axle_stage(w: &mut DriveWorker) {
    let [a, b] = w.def.axles[1].block_pair;
    pair_stage(w, (b, a), w.def.axles[1].scale, 1);
}

/// Wheel `x`'s row of the pair matrix (row `x & 1`). The first entry is
/// the clutch load over `g` (the axle's scale times the wheel's weight), plus `pack_gain * g * 0.5` when the block is
/// gated. The second is half the load when the pair's second flag is clear, else 1 or 0 by the gate.
pub fn pair_clutch_values(w: &mut DriveWorker, x: usize) {
    let wheel = w.def.wheels[x];
    let block = w.state.blocks[x];
    let g = w.def.axles[wheel.pack as usize].scale * wheel.weight;
    let load = block.clutch_load;
    let row = 2 * (x & 1);
    w.state.pair_matrix[row] = if block.gate.value != 0 { w.state.pack_gain * g * 0.5 + load / g } else { load / g };
    w.state.pair_matrix[row + 1] = if w.state.pair_active[1] == 0 {
        load * 0.5
    } else if block.gate.value != 0 {
        1.0
    } else {
        0.0
    };
}

/// Wheel `x`'s slip value, written to `pair_slip[x & 1]`. Zero while the
/// block is not gated; else half of `g` times the negated drive gain, plus the latch term (when the wheel is enabled)
/// less the speed of the wheel's term.
pub fn pair_slip_value(w: &mut DriveWorker, x: usize) {
    let wheel = w.def.wheels[x];
    let block = w.state.blocks[x];
    if block.gate.value == 0 {
        w.this.pair_slip[x & 1] = 0.0;
        return;
    }
    let pack = wheel.pack as usize;
    let g = w.def.axles[pack].scale * wheel.weight;
    let mut f = 0.0f32;
    if w.obj.wheel_enabled[x] != 0 {
        f = (0.0 - (block.gate.value as f32) * block.latch_bound) + w.this.wheel_offset[x];
    }
    w.this.pair_slip[x & 1] = g * 0.5 * w.state.drive_gain_neg + (f - w.state.terms[pack].speed);
}

/// The pair worker's clamp-and-latch of wheel `x`, written to
/// `pair_pack[x & 1]`. A gated block clears the output. Otherwise `f` is the negated drive gain less the pack term,
/// times half of `g`, less the speed of the wheel's term; above `h - k` it latches `+1`, below `-(h + k)` `-1` (`k`, `h`
/// the wheel offset and the latch bound, zero when the wheel is not enabled), else the output is zero.
pub fn pair_latch(w: &mut DriveWorker, x: usize) {
    let wheel = w.def.wheels[x];
    let block = w.state.blocks[x];
    if block.gate.value != 0 {
        w.this.pair_pack[x & 1] = 0.0;
        return;
    }
    let pack = wheel.pack as usize;
    let drive = w.state.drive_gain_neg - w.state.pack_gain * w.this.loc0.value;
    let g = w.def.axles[pack].scale * wheel.weight * 0.5;
    let f = drive * g - w.state.terms[pack].speed;
    let (k, h) = if w.obj.wheel_enabled[x] == 0 { (0.0, 0.0) } else { (w.this.wheel_offset[x], block.latch_bound) };
    let s = if f > h - k {
        1
    } else if 0.0 - (h + k) > f {
        -1
    } else {
        w.this.pair_pack[x & 1] = 0.0;
        return;
    };
    w.this.pair_pack[x & 1] = wheel.weight;
    w.state.blocks[x].gate.set(s);
    if w.state.pending == 0 {
        w.state.latch_count = w.state.latch_count.wrapping_add(1);
    }
    w.state.pending = 1;
}

/// The force of block `k` (the axle's first block index) when its gate is
/// set: the wheel's weight over the axle's scale, times the term's value, plus half the term's term.
pub fn block_lane_step(w: &mut DriveWorker, x: usize) {
    let axle = w.def.axles[x];
    let k = axle.block_pair[0] as usize;
    if w.state.blocks[k].gate.value == 0 {
        return;
    }
    let ratio = w.def.wheels[k].weight / axle.scale;
    let term = w.state.terms[x];
    let value = ratio * term.value + term.term * 0.5;
    w.state.blocks[k].force.set(value);
}

/// Zeroes the force and the gate of block `idx`. Only `block_pair_gate` calls it, when the pack's outputs are zero.
fn zero_block(w: &mut DriveWorker, idx: usize) {
    w.state.blocks[idx].force.set(0.0);
    w.state.blocks[idx].gate.set(0);
}

/// The gates of axle `x`'s two blocks (`k` and `m`). `zeros` is how many
/// of the two gates are zero. With one zero gate and the term's sign clear: when both pack outputs are zero, the block
/// with the set gate is zeroed, the term's value and term are cleared and the result is true. Otherwise the term's sign
/// takes the sum of the gates. With two zero gates the sign is cleared. The value and term are cleared unless the
/// term's sign was set; with one zero gate and the sign set nothing is written.
pub fn block_pair_gate(w: &mut DriveWorker, x: usize) -> bool {
    let axle = w.def.axles[x];
    let k = axle.block_pair[0] as usize;
    let m = axle.block_pair[1] as usize;
    let g0 = w.state.blocks[k].gate.value;
    let g1 = w.state.blocks[m].gate.value;
    let zeros = u32::from(g0 == 0) + u32::from(g1 == 0);
    if zeros == 1 {
        if w.state.terms[x].sign != 0 {
            return false;
        }
        if w.this.pack_out[k] == 0.0 && w.this.pack_out[m] == 0.0 {
            zero_block(w, if g0 != 0 { k } else { m });
            clear_term_value(w, x);
            return true;
        }
        w.state.terms[x].sign = g0.wrapping_add(g1);
    } else if zeros == 2 {
        w.state.terms[x].sign = 0;
    } else {
        return false;
    }
    clear_term_value(w, x);
    false
}

fn clear_term_value(w: &mut DriveWorker, x: usize) {
    w.state.terms[x].value = 0.0;
    w.state.terms[x].term = 0.0;
}

/// The sign word of a stage value: 1, -1, or 0 (also for NaN).
pub(super) fn sign(f: f32) -> i32 {
    if f > 0.0 {
        1
    } else if f < 0.0 {
        -1
    } else {
        0
    }
}

//! NGP's axle worker: the wheel worker of drivetrain types 4 and 5, which drive axle `g` (1 or 2) only. It is 1.02's
//! pair worker rewritten for either axle: the axle's two blocks (`axles[g].block_pair`) solve their 2x2 clutch matrix
//! (inverted in doubles), term `g` integrates, and the blocks latch and sync. Unlike 1.02 it writes the lanes' values
//! only (their dirty words stay), indexes the active flags by `g + 1`, and the other axle's wheels take the free-wheel
//! step. All SSE.

use super::sign;
use crate::engine_driveline::drive_worker::{bump_dirty, free_wheel_step};
use crate::engine_driveline::layout::DriveWorker;

const HALF: f32 = 0.5;

/// The two blocks of axle `g`.
fn blocks(w: &DriveWorker, g: usize) -> [usize; 2] {
    w.def.axles[g].block_pair.map(|k| k as usize)
}

/// Block `k`'s row of the 2x2 clutch matrix (row `k & 1`).
fn clutch_row(w: &mut DriveWorker, k: usize) {
    let wheel = w.def.wheels[k];
    let pack = wheel.pack as usize;
    let blk = w.state.blocks[k];
    let gate = blk.gate.value;
    let reach = w.def.axles[pack].scale * wheel.weight;
    let mut v = blk.clutch_load / reach;
    if gate != 0 {
        v += w.state.pack_gain * HALF * reach;
    }
    let row = 2 * (k & 1);
    w.state.clutch_matrix[row] = v;
    w.state.clutch_matrix[row + 1] = if w.state.lane_active[pack + 1] == 0 {
        blk.clutch_load * HALF
    } else if gate != 0 {
        1.0
    } else {
        0.0
    };
}

/// The reset. Axle `g`'s active flag (its sign clear and a block gated), the matrix rows, and the
/// inverse (in doubles).
fn reset(w: &mut DriveWorker, g: usize) {
    let [a, b] = blocks(w, g);
    let gated = w.state.blocks[a].gate.value != 0 || w.state.blocks[b].gate.value != 0;
    w.state.lane_active[g + 1] = u32::from(w.state.terms[g].sign == 0 && gated);
    clutch_row(w, a);
    clutch_row(w, b);
    w.state.pending = 0;
    let m = &mut w.state.clutch_matrix;
    let [m0, m1, m2, m3] = [m[0], m[1], m[2], m[3]].map(f64::from);
    let det = m3 * m0 - m2 * m1;
    m[4] = (m3 / det) as f32;
    m[5] = (-(m1 / det)) as f32;
    m[6] = (-(m2 / det)) as f32;
    m[7] = (m0 / det) as f32;
    w.state.reset_count.value += 1.0;
}

/// Block `k`'s target slip value: zero when ungated, else the drive term plus the
/// wheel's offset beyond its latch, less the pack term's speed.
fn slip_value(w: &mut DriveWorker, k: usize) {
    let blk = w.state.blocks[k];
    if blk.gate.value == 0 {
        w.state.step_vec[k] = 0.0;
        return;
    }
    let wheel = w.def.wheels[k];
    let pack = wheel.pack as usize;
    let axle = w.def.axles[pack];
    let mut off = 0.0;
    if w.obj.wheel_enabled[k] != 0 {
        off = w.this.wheel_offset[k] - blk.gate.value as f32 * blk.latch_bound;
    }
    let off = off - w.state.terms[pack].speed;
    let reach = axle.scale * HALF * wheel.weight;
    w.state.step_vec[k] = axle.direction as f32 * w.state.drive_gain * reach + off;
}

/// Block `k`'s latch. An ungated block whose drive term is beyond its latch bound (less the offset)
/// gates to its side, and its pack output takes the wheel's weight; otherwise the pack output is zero.
fn latch(w: &mut DriveWorker, k: usize) {
    if w.state.blocks[k].gate.value != 0 {
        w.this.pack_out[k] = 0.0;
        return;
    }
    let wheel = w.def.wheels[k];
    let pack = wheel.pack as usize;
    let axle = w.def.axles[pack];
    let s = &w.state;
    let dir = axle.direction as f32;
    let v = (dir * s.drive_gain - dir * w.this.loc0.value * s.pack_gain) * HALF * axle.scale * wheel.weight
        - s.terms[pack].speed;
    let (bound, off) =
        if w.obj.wheel_enabled[k] != 0 { (s.blocks[k].latch_bound, w.this.wheel_offset[k]) } else { (0.0, 0.0) };
    let gate = if !(bound - off >= v) {
        1
    } else if !(v >= -(off + bound)) {
        -1
    } else {
        w.this.pack_out[k] = 0.0;
        return;
    };
    w.state.blocks[k].gate.value = gate;
    w.this.pack_out[k] = wheel.weight;
    if w.state.pending == 0 {
        w.state.latch_count = w.state.latch_count.wrapping_add(1);
    }
    w.state.pending = 1;
}

/// Term `g` from its blocks' forces: the sum, the weighted sum, and the sign (of the sum, or of the
/// gates when the sum is zero).
fn sync(w: &mut DriveWorker, g: usize) {
    let [a, b] = blocks(w, g);
    let (fa, fb) = (w.state.blocks[a].force.value, w.state.blocks[b].force.value);
    let (wa, wb) = (w.def.wheels[a].weight, w.def.wheels[b].weight);
    let sum = fb + fa;
    let gates = w.state.blocks[b].gate.value.wrapping_add(w.state.blocks[a].gate.value);
    let t = &mut w.state.terms[g];
    t.term = sum;
    t.value = (fb * wb + fa * wa) * (w.def.axles[g].scale * HALF);
    t.sign = if sum == 0.0 { gates.signum() } else { sign(sum) };
}

/// Block `k`'s slip value, raw (block 0's is also term 3's scale, see `WorkerState::set_slip`).
fn set_slip_value(w: &mut DriveWorker, k: usize, v: f32) {
    w.state.blocks[k].slip.value = v;
    if k == 0 {
        w.state.terms[3].scale = v;
    }
}

/// The worker on axle `g` (1 or 2). The blocks' wheel packs must be at most 2 (the active flags are indexed by
/// `pack + 1`).
pub fn axle_worker(w: &mut DriveWorker, dt: f32, g: usize) {
    let [a, b] = blocks(w, g);
    if w.state.pending != 0 {
        reset(w, g);
    }
    if w.state.blocks[a].gate.value == 0 && w.state.blocks[b].gate.value == 0 {
        w.this.loc0.value = 0.0;
        w.state.terms[g].speed = 0.0;
        latch(w, a);
        latch(w, b);
        sync(w, g);
    } else {
        for _ in 0..5 {
            let t = &mut w.state.terms[g];
            t.speed = t.sign as f32 * t.scale;
            slip_value(w, a);
            slip_value(w, b);
            let m = w.state.clutch_matrix;
            let (va, vb) = (w.state.step_vec[a], w.state.step_vec[b]);
            w.this.loc[0] = vb * m[5] + va * m[4];
            let loc = vb * m[7] + va * m[6];
            w.this.loc[g] = loc;
            if w.state.lane_active[g + 1] != 1 || w.state.terms[g].scale >= loc.abs() {
                break;
            }
            w.state.terms[g].sign = sign(loc);
            w.state.repeat_count = w.state.repeat_count.wrapping_add(1);
            reset(w, g);
        }
        let value_rate = w.this.loc[0];
        let loc = w.this.loc[g];
        let term_rate = if w.state.lane_active[g + 1] == 1 {
            w.state.terms[g].speed = loc;
            0.0
        } else {
            loc
        };
        w.this.loc0.value = 0.0;
        latch(w, a);
        latch(w, b);
        w.this.loc0.value = w.def.axles[g].direction as f32 * value_rate;
        let t = &mut w.state.terms[g];
        t.value += value_rate * dt;
        t.term += term_rate * dt;
        if sign(t.term) != t.sign {
            t.term = 0.0;
            t.sign = 0;
            bump_dirty(w);
        }
        let t = w.state.terms[g];
        for k in [a, b] {
            if w.state.blocks[k].gate.value != 0 && w.this.pack_out[k] == 0.0 {
                w.state.blocks[k].force.value = t.value * w.def.wheels[k].weight / w.def.axles[g].scale + t.term * HALF;
            }
        }
        loop {
            sync(w, g);
            let mut any = false;
            for k in [a, b] {
                let blk = &mut w.state.blocks[k];
                if blk.gate.value != sign(blk.force.value) && w.this.pack_out[k] == 0.0 {
                    blk.force.value = 0.0;
                    blk.gate.value = 0;
                    any = true;
                }
            }
            if !any {
                break;
            }
        }
    }
    w.state.drive.drive_speed_key = w.def.axles[g].direction as f32 * w.state.terms[g].value;
    for k in [a, b] {
        if w.obj.wheel_enabled[k] != 0 {
            let blk = &mut w.state.blocks[k];
            let force = blk.force.value;
            let slip = (force - blk.prev_force.value) / dt;
            blk.prev_force.value = force;
            blk.prev_gate.value = blk.gate.value;
            set_slip_value(w, k, slip);
        } else {
            free_wheel_step(w, k, dt);
        }
        free_wheel_step(w, if k < 2 { k + 2 } else { k - 2 }, dt);
    }
}

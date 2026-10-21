//! NGP's rewrite of `CDrive::M6C90` (`0x100456c0`, the call at `0x50a25c` in `Step1E0`): the starter. Like 1.02's
//! (`drive_leaf::speed_term`) it drains the worker object's term (`+0x8c`) at its rate (`+0x74`) while the gate word
//! (`+0x50`, the IGNITION bit) and the ready word (`+0x7c`) are set, but only from 0.5 s to 2.5 s after the gate was
//! last clear. Its knee is 0.75 of NGP's idle band (`10171a98`) rather than 300, the term stays at or above 0, and
//! the gear-1 gate is gone. A crank on a stopped engine (`|speed| < 0.001`) sets the engine speed to 0.001. All SSE.
//!
//! NGP times the hold by the wall clock (`QueryPerformanceCounter` in us, stamped at `10176718` while the gate is
//! clear); the caller gives the hold, by simulation time ([`sim_held`]) so a playback cranks for the same steps at any
//! speed, or by a recording's clock.

use crate::engine_driveline::layout::DriveWorker;

/// The hold window, in us: cranking needs more than 0.5 s and at most 2.5 s.
const WINDOW: std::ops::RangeInclusive<u64> = 500_001..=2_500_000;

/// `maxss a, b`: `b` when they are unordered.
fn max_ss(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// The hold by simulation time: `held` plus `dt` (us) while the gate is set, else 0.
pub fn sim_held(gated: bool, dt: f32, held: u64) -> u64 {
    if gated {
        held.saturating_add((f64::from(dt) * 1.0e6).round() as u64)
    } else {
        0
    }
}

/// `0x100456c0` on the worker for `dt`, with NGP's idle band and `held`, the time since the gate was last clear (us).
/// Returns the starter's term (st0).
pub fn starter(w: &mut DriveWorker, dt: f32, band: f32, held: u64) -> f32 {
    if w.this.lane_gate == 0 || !WINDOW.contains(&held) || w.obj.ready == 0 {
        w.this.speed_factor = 0.0;
        w.this.scaled_term = 0.0;
        return 0.0;
    }
    let knee = band * 0.75;
    let speed = w.state.drive.engine_speed;
    let term = w.obj.speed_term;
    let factor = if speed > knee { (1.0 - (speed - knee) / (band - knee)) * term } else { term };
    w.this.speed_factor = factor;
    w.obj.speed_term = max_ss(0.0, term - w.obj.rate * 0.02 * factor * dt);
    let out = factor * 80.0;
    if out > 0.0 && 0.001 > speed.abs() {
        w.state.drive.engine_speed = 0.001;
    }
    w.this.scaled_term = out;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker() -> DriveWorker {
        let mut w = DriveWorker::default();
        w.this.lane_gate = 1;
        w.obj.ready = 1;
        w.obj.speed_term = 1.0;
        w.obj.rate = 1.0;
        w
    }

    #[test]
    fn cranks_only_inside_the_hold_window() {
        let mut w = worker();
        let (dt, mut held) = (0.0025, 0);
        let mut cranked = Vec::new();
        for step in 1..=1100 {
            held = sim_held(true, dt, held);
            let v = starter(&mut w, dt, 100.0, held);
            if v > 0.0 {
                cranked.push(step);
            }
        }
        assert_eq!((cranked[0], *cranked.last().unwrap()), (201, 1000));
        w.this.lane_gate = 0;
        assert_eq!(starter(&mut w, dt, 100.0, held), 0.0);
    }

    #[test]
    fn kicks_a_stopped_engine_and_fades_above_the_knee() {
        let mut w = worker();
        let v = starter(&mut w, 0.0025, 100.0, 600_000);
        assert_eq!((v, w.state.drive.engine_speed), (80.0, 0.001));
        let mut w = worker();
        w.state.drive.engine_speed = 87.5;
        let v = starter(&mut w, 0.0025, 100.0, 600_000);
        assert_eq!(v, 40.0);
    }
}

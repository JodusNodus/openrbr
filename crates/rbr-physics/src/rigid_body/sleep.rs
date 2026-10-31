//! The sleep check: the body's timer runs while both velocities are below their thresholds, and at the def's
//! `SleepDelay` the body is put to sleep: both velocities are cleared and the caller must run the body's
//! after-sleep call. The dot products sum `(z*z + y*y) + x*x`.

use super::integrate::squared_speed;
use super::layout::{Body, Def, Vec4};

/// One sleep step over `dt` seconds. Returns true when the body fell asleep this step (the caller then runs the
/// after-sleep call). The thresholds compare squared speeds, and a NaN speed never counts as below a threshold.
pub fn step(body: &mut Body, def: &Def, dt: f32) -> bool {
    if def.sleepable == 0 {
        return false;
    }
    let linear_sq = squared_speed(body.linear_velocity);
    let angular_sq = squared_speed(body.angular_velocity);
    let k1 = def.linear_sleep_velocity;
    let k2 = def.angular_sleep_velocity;
    if !(k1 * k1 > linear_sq) || !(k2 * k2 > angular_sq) {
        body.sleep_timer = 0.0;
        return false;
    }
    let timer = body.sleep_timer + dt;
    body.sleep_timer = timer;
    let delay = def.sleep_delay;
    if body.sleep_armed != 0 && delay <= timer && timer != delay {
        body.linear_velocity = Vec4::default();
        body.angular_velocity = Vec4::default();
        body.sleep_timer = 0.0;
        return true;
    }
    false
}

/// The in-object part of the after-sleep call, which the caller runs after a trip: the body leaves the active set
/// and its armed flag is cleared. The rest of that call (the tweak dirty marks and the body-event signal) is outside
/// the body and is not ported here.
pub fn deactivate(body: &mut Body) {
    body.is_active = 0;
    body.sleep_armed = 0;
}

/// Wakes the body, before its calls: both tweaks marked dirty, the body joins the active set, and the very-soft
/// and armed words are cleared (the branch on the armed word stores the same zero). The calls follow in the body's
/// emit or wake.
pub fn wake_up(body: &mut Body) {
    body.tweak_a_dirty = 1;
    body.is_active = 1;
    body.tweak_b_dirty = 1;
    body.very_soft = 0;
    body.sleep_armed = 0;
}

/// Puts the body to sleep, before its calls: both tweaks marked dirty, the body leaves the active set, and the
/// very-soft and armed words are cleared. The calls (the sleep event) follow in the body's sleep.
pub fn fall_asleep(body: &mut Body) {
    body.tweak_a_dirty = 1;
    body.is_active = 0;
    body.tweak_b_dirty = 1;
    body.very_soft = 0;
    body.sleep_armed = 0;
}

/// On a parentless body, marks both tweaks dirty, removes it from the active set, and sets the very-soft word. The
/// armed word is cleared in either parent branch.
pub fn become_very_soft(body: &mut Body) {
    body.tweak_a_dirty = 1;
    body.is_active = 0;
    body.tweak_b_dirty = 1;
    body.very_soft = 1;
    body.sleep_armed = 0;
}

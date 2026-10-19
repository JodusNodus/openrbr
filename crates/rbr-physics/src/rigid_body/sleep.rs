//! `CSolid::Vf24x` (0x52b740, slot 24): the sleep check. The body's timer runs while both velocities are below
//! their thresholds, and at the def's `SleepDelay` the body is put to sleep: both velocities are cleared and the
//! caller must call the body's slot 37 (`S37`, not ported). The asm (not the C++ in `src/rbr`) decides the order
//! of the dot products: `(z*z + y*y) + x*x`.

use super::integrate::squared_speed;
use super::layout::{Body, Def, Vec4};

/// One sleep step over `dt` seconds. Returns true when the body fell asleep this step (the caller then calls the
/// body's slot 37). The thresholds compare squared_speed speeds, and a NaN speed never counts as below a threshold.
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

/// The in-object part of the body's slot 37 (`CSolid::E43Vf94`, 0x52c130), which the caller runs after a trip:
/// the body leaves the active set and its armed flag is cleared. The rest of that call (the tweak `SetDirty`
/// calls, the E43 helper at 0x519a80 and the body-event signal) is outside the body and is not ported here.
pub fn deactivate(body: &mut Body) {
    body.is_active = 0;
    body.sleep_armed = 0;
}

/// `CSolid::E43Vf90` (0x52bf30) on the body, before its calls: both tweaks marked dirty (`SetDirty`), the body joins
/// the active set, and the very-soft and armed words are cleared (the asm's branch on the armed word stores the same
/// zero). The calls follow in `emit::emit_body_event` or `layout::solid_wake`.
pub fn wake_up(body: &mut Body) {
    body.tweak_a_dirty = 1;
    body.is_active = 1;
    body.tweak_b_dirty = 1;
    body.very_soft = 0;
    body.sleep_armed = 0;
}

/// `CSolid::E43Vf94` (0x52c130) on the body, before its calls: both tweaks marked dirty (`SetDirty`), the body leaves
/// the active set, and the very-soft and armed words are cleared. The calls (`CEntity::E43Vf94`, then the body's
/// [`super::emit::SLEEP`] event) follow in `layout::solid_sleep`.
pub fn fall_asleep(body: &mut Body) {
    body.tweak_a_dirty = 1;
    body.is_active = 0;
    body.tweak_b_dirty = 1;
    body.very_soft = 0;
    body.sleep_armed = 0;
}

/// `CSolid::E43Vf158` (0x52c030): on a parentless body, mark both tweaks dirty, remove it from the active set, and set
/// the very-soft word. The slot-86 method clears the armed word in either parent branch.
pub fn become_very_soft(body: &mut Body) {
    body.tweak_a_dirty = 1;
    body.is_active = 0;
    body.tweak_b_dirty = 1;
    body.very_soft = 1;
    body.sleep_armed = 0;
}

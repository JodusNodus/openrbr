//! Gear logic: the launch-time shift requests for gears 0, 1 and 2 and the shift requests on the rpm thresholds
//! of every gear, with the shift lockout that follows each request. The branches and the NaN behaviour are fixed:
//! a comparison that is false for NaN goes to the same place. The gear 1 downshift tests `rpm > 5000`, not its
//! negation.

use super::cut::check_slip;
use super::layout::{Def, Gearbox, Pedals, State, Tuning, GEARS, RPM_PER_RAD_S};

/// The gear logic's thresholds (see notes/physics.md "VCU").
const HALF_LIMIT: f32 = 0.5;
const LOW_PEDAL: f32 = 0.3;
const HIGH_PEDAL: f32 = 0.9;
const SECOND_GEAR_TIMER_LIMIT: f32 = 0.4;
const NEUTRAL_TIMER_LIMIT: f32 = -0.4;

/// What the gear logic reads outside the drive and the state: the scratch's flags and pedals, the
/// engine speed (rad/s), the car speed (rounded to float as it is stored), the frame time, and the gear count
/// of the drive's own definition (the one the gear setter clamps to; it is not the scratch's `Def`).
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub launch_enabled: u32,
    pub vcu_enabled: u32,
    pub pedals: Pedals,
    pub engine: f32,
    pub speed: f32,
    pub dt: f32,
    pub gear_count: u32,
    /// The shift cooldown and the launch rpm.
    pub tuning: Tuning,
}

/// One step of the gear logic. The drive's commanded gear and the state change in place.
pub fn update(input: Input, def: &Def, state: &mut State, drive: &mut Gearbox) {
    if input.launch_enabled == 0 || drive.gear != drive.commanded {
        return;
    }
    let rpm = input.engine * RPM_PER_RAD_S;
    let speed = input.speed.abs();
    if input.vcu_enabled != 0 {
        match drive.gear {
            2 => second_gear(&input, speed, def, state, drive),
            0 => neutral(&input, speed, def, state, drive),
            1 => first_gear(&input, rpm, def, state, drive),
            _ => {
                state.gear_timer = 0.0;
                state.pedal_swap = 0;
            }
        }
    }
    // The launch rules may have requested a gear, so the rpm rules look at the commanded gear again.
    if drive.gear != drive.commanded {
        return;
    }
    let k = gear_index(drive.gear);
    let up = def.gear_upshift_rpm[k];
    if up != 0.0 && rpm > up && lockout_clear(state) && !check_slip(input.vcu_enabled, drive.slip, speed) {
        request(drive, input.gear_count, 1);
        state.shift_lockout = def.shift_lockout_up_s;
        state.shift_cooldown = input.tuning.cooldown;
        return;
    }
    let down = def.gear_downshift_rpm[k];
    if down != 0.0 && down > rpm && lockout_clear(state) {
        request(drive, input.gear_count, -1);
        state.shift_lockout = def.shift_lockout_down_s;
        state.shift_cooldown = input.tuning.cooldown;
        return;
    }
    if state.shift_lockout > 0.0 {
        let left = state.shift_lockout - input.dt;
        state.shift_lockout = if 0.0 > left { 0.0 } else { left };
    }
}

/// The gear's row in the Def tables. Gears are 0..GEARS-1 (the setup's Gear0..Gear9 keys). The original
/// reads past the table for any other gear; the port clamps to the last row instead.
pub(super) fn gear_index(gear: u32) -> usize {
    debug_assert!((gear as usize) < GEARS, "gear {gear} outside the Def tables");
    (gear as usize).min(GEARS - 1)
}

/// Moves the commanded gear by `delta`, held to 0..gear_count-1, and sets the changed flag.
pub(super) fn request(drive: &mut Gearbox, gear_count: u32, delta: i32) {
    let top = gear_count as i32 - 1;
    let next = drive.commanded as i32 + delta;
    drive.commanded = if next < 0 {
        0
    } else if next > top {
        top as u32
    } else {
        next as u32
    };
    drive.changed = 1;
}

/// The commanded gear for `value`, which is kept when it is below the gear count (signed), else 1. Unlike
/// `request`, a negative value is kept, and there is no clamp at 0.
pub fn commanded_gear(gear_count: u32, value: u32) -> u32 {
    if (value as i32) < gear_count as i32 {
        value
    } else {
        1
    }
}

/// On the gearbox that the flag dispatch writes: the commanded gear, and the request flag.
pub(super) fn set_gear(drive: &mut Gearbox, gear_count: u32, value: u32) {
    drive.commanded = commanded_gear(gear_count, value);
    drive.changed = 1;
}

/// The shift lockout has run out. NaN is not zero.
fn lockout_clear(state: &State) -> bool {
    state.shift_lockout == 0.0
}

fn second_gear(input: &Input, speed: f32, def: &Def, state: &mut State, drive: &mut Gearbox) {
    if !(HALF_LIMIT > speed && LOW_PEDAL > input.pedals.throttle) {
        return;
    }
    if state.gear_timer < 0.0 {
        state.gear_timer = input.dt;
    } else if state.gear_timer > SECOND_GEAR_TIMER_LIMIT {
        if lockout_clear(state) {
            state.gear_timer = 0.0;
            request(drive, input.gear_count, -1);
            state.shift_lockout = def.shift_lockout_down_s;
            state.shift_cooldown = input.tuning.cooldown;
        }
    } else {
        state.gear_timer += input.dt;
    }
}

fn neutral(input: &Input, speed: f32, def: &Def, state: &mut State, drive: &mut Gearbox) {
    if !(HALF_LIMIT > speed && LOW_PEDAL > input.pedals.brake) {
        return;
    }
    if state.gear_timer > 0.0 {
        // 0 - dt, as the asm writes it (so a zero dt gives +0).
        state.gear_timer = 0.0 - input.dt;
    } else if NEUTRAL_TIMER_LIMIT > state.gear_timer {
        if lockout_clear(state) {
            state.gear_timer = 0.0;
            request(drive, input.gear_count, 1);
            state.shift_lockout = def.shift_lockout_down_s;
            state.shift_cooldown = input.tuning.cooldown;
        }
    } else {
        state.gear_timer -= input.dt;
    }
}

fn first_gear(input: &Input, rpm: f32, def: &Def, state: &mut State, drive: &mut Gearbox) {
    let pedals = input.pedals;
    state.pedal_swap = u32::from(pedals.brake > pedals.throttle);
    if pedals.throttle > HIGH_PEDAL
        && HALF_LIMIT > pedals.brake
        && lockout_clear(state)
        && rpm > input.tuning.launch_rpm
    {
        request(drive, input.gear_count, 1);
        state.shift_lockout = def.shift_lockout_up_s;
        state.shift_cooldown = input.tuning.cooldown;
    } else if HALF_LIMIT > pedals.throttle
        && pedals.brake > HIGH_PEDAL
        && lockout_clear(state)
        && rpm > input.tuning.launch_rpm
    {
        request(drive, input.gear_count, -1);
        state.shift_lockout = def.shift_lockout_up_s;
        state.shift_cooldown = input.tuning.cooldown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_held_to_the_gears() {
        let mut drive = Gearbox { gear: 1, commanded: 0, changed: 0, slip: 0 };
        request(&mut drive, 4, -1);
        assert_eq!((drive.commanded, drive.changed), (0, 1));
        request(&mut drive, 4, 9);
        assert_eq!(drive.commanded, 3);
    }

    #[test]
    fn nan_lockout_is_not_clear() {
        let state = State { shift_lockout: f32::NAN, ..State::default() };
        assert!(!lockout_clear(&state));
        assert!(lockout_clear(&State::default()));
    }
}

//! The car's per-step composite after the physics step. The order is the original's: the override timer, the passenger,
//! scratch, drive and solid steps, the brake-wall raycast (its impulse, emit and contact time), the walk of the velocity,
//! the hinged-body and mounted-body loops, the timekeeper, the dangerous-acceleration flag, the partial-contact time,
//! the speed sum, the speed and the two state emits. The externals are answered by `StepCalls`; the signal emit is one
//! of them (it is ported, but its record is not an inner call of this function).

use super::accessors::set_cell;
use super::layout::{self, Real};
use crate::car_body::store::CarStore;
use crate::fpu::round_x87;
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;

/// A brake-wall ray hit (28 bytes): the wall normal (16 bytes), the penetration (`PENETRATION_AT`, a float, m), the
/// perpendicular distance (`PERPENDICULAR_AT`, a float) and the flags (`FLAGS_AT`, bit 0 from the wall index).
pub type WallHit = [u8; 28];

const PENETRATION_AT: usize = 0x10;
const PERPENDICULAR_AT: usize = 0x14;
const FLAGS_AT: usize = 0x18;

/// The calls the step makes out of its own body. The test answers them from the original's records.
pub trait StepCalls<S = Mem> {
    /// The passenger's step.
    fn passenger_step(&mut self, mem: &mut S, passenger: u32, dt: f32);
    /// The scratch sim's no-op step.
    fn scratch_idle(&mut self, mem: &mut S, scratch: u32, dt: f32);
    /// The drive's no-op step.
    fn drive_idle(&mut self, mem: &mut S, drive: u32, dt: f32);
    /// The solid's step on the car.
    fn solid_step(&mut self, mem: &mut S, car: u32, dt: f32);
    /// The brake wall's raycast from the car's position into `hit`. Nonzero on a hit.
    fn raycast(&mut self, mem: &mut S, car: u32, hit: &mut WallHit) -> u32;
    /// The wall's impulse on the car after a hit: the perpendicular distance, the penetration word and the step.
    fn impulse(&mut self, mem: &mut S, car: u32, hit: &mut WallHit, perpendicular: f32, penetration: u32, dt: f32);
    /// The brake-wall penetration signal, emitted from the hit's first 16 bytes.
    fn emit_brake_wall(&mut self, mem: &mut S, hit: &WallHit);
    /// Walks `a` through the world with the car as the identity entity.
    fn walk_vector(&mut self, a: Vec3, car: u32) -> Vec3;
    /// The hinged body's activity: nonzero when it answers.
    fn hinged_active(&mut self, mem: &mut S, hinged: u32) -> u32;
    /// The hinged body's step while it is not active.
    fn hinged_inactive_step(&mut self, mem: &mut S, hinged: u32);
    /// The hinged body's detach check: nonzero to clear its slot.
    fn hinged_detach(&mut self, mem: &mut S, hinged: u32) -> u32;
    /// The mounted body's first test: nonzero when it holds.
    fn mounted_test(&mut self, mem: &mut S, body: u32) -> u32;
    /// The mounted body's step.
    fn mounted_step(&mut self, mem: &mut S, body: u32, dt: f32);
    /// The mounted body's second test.
    fn mounted_second_test(&mut self, mem: &mut S, body: u32) -> u32;
    /// The mounted body's detach step: nonzero to detach it.
    fn mounted_detach_step(&mut self, mem: &mut S, body: u32) -> u32;
    /// Excludes the mounted body from the car's collisions.
    fn mounted_exclude(&mut self, mem: &mut S, body: u32, car: u32);
    /// The drive line's value for the car's drive line position and the timer; returns a float.
    fn drive_line_value(&mut self, mem: &mut S, car: u32, timer: f64) -> f32;
    /// The timekeeper's record of `value` at `timer`.
    fn timekeeper_record(&mut self, mem: &mut S, value: f32, timer: f64);
    /// The timekeeper's update: an int that the signal emit takes.
    fn timekeeper_update(&mut self, mem: &mut S) -> u32;
    /// The drive's speed.
    fn drive_speed(&mut self, mem: &mut S, drive: u32) -> f32;
    /// The car's signal emit with the timekeeper's word (ported, called here as an external).
    fn check_retire(&mut self, mem: &mut S, car: u32, v: u32);
    /// The position signal: the car's position lanes and the timer word, 20 bytes.
    fn emit_position(&mut self, mem: &mut S, payload: [u32; 5]);
    /// The MrR car state signal: the car, its sort key and the drive's last float, 12 bytes.
    fn emit_mrr(&mut self, mem: &mut S, payload: [u32; 3]);
}

/// The penetration, m, above which a wall hit counts as contact.
const CONTACT_PENETRATION: f32 = 0.5;

/// The speed, m/s, below which the direction is cleared instead of scaled.
const GATE_SPEED: f32 = 0.001;

/// The direction opposite the velocity is the velocity times `-1 / speed`.
const MINUS_ONE: f32 = -1.0;

/// Runs the step of `car` for the step `dt`, s.
pub fn step_car<S: CarStore>(calls: &mut impl StepCalls<S>, mem: &mut S, car: u32, dt: f32) {
    let timer = mem.override_timer(car);
    if timer > 0.0 {
        let left = timer - dt;
        mem.set_override_timer(car, if left > 0.0 { left } else { 0.0 });
    }
    calls.passenger_step(mem, layout::passenger_at(car), dt);
    calls.scratch_idle(mem, layout::scratch_at(car), dt);
    calls.drive_idle(mem, layout::drive_at(car), dt);
    calls.solid_step(mem, car, dt);

    let mut hit = [0u8; 28];
    let penetration = if calls.raycast(mem, car, &mut hit) != 0 {
        let perpendicular = f32::from_le_bytes(hit[PERPENDICULAR_AT..PERPENDICULAR_AT + 4].try_into().unwrap());
        let penetration_word = u32::from_le_bytes(hit[PENETRATION_AT..PENETRATION_AT + 4].try_into().unwrap());
        calls.impulse(mem, car, &mut hit, perpendicular, penetration_word, dt);
        calls.emit_brake_wall(mem, &hit);
        let penetration = f32::from_le_bytes(hit[PENETRATION_AT..PENETRATION_AT + 4].try_into().unwrap());
        let in_contact = hit[FLAGS_AT] & 1 != 0 && penetration > CONTACT_PENETRATION;
        let contact = if in_contact { mem.wall_contact(car) + dt } else { 0.0 };
        mem.set_wall_contact(car, contact);
        penetration
    } else {
        0.0
    };
    mem.set_wall_penetration(car, penetration);

    let walked = calls.walk_vector(mem.velocity(car), car);
    mem.set_forward_speed(car, 0.0 - walked.y);

    for i in 0..4 {
        let slot = layout::hinged_slot(car, i);
        let hinged = mem.u32(slot);
        if hinged == 0 {
            continue;
        }
        if mem.hinged_state_word(hinged, 0) != 1 && calls.hinged_active(mem, hinged) == 0 {
            calls.hinged_inactive_step(mem, hinged);
        }
        let hinged = mem.u32(slot);
        if mem.hinged_state_word(hinged, 0) != 1
            && mem.hinged_state_word(hinged, 1) != 1
            && calls.hinged_detach(mem, hinged) != 0
        {
            mem.set_u32(slot, 0);
        }
    }

    for i in 0..4 {
        let slot = layout::mounted_slot(car, i);
        let body = mem.u32(slot);
        if body == 0 {
            continue;
        }
        if calls.mounted_test(mem, body) != 0 {
            calls.mounted_step(mem, body, dt);
        } else if calls.mounted_second_test(mem, body) != 0 && calls.mounted_detach_step(mem, body) != 0 {
            calls.mounted_exclude(mem, body, car);
            mem.set_u32(slot, 0);
        }
    }

    let timer = mem.timer_double();
    let drive_line_value = calls.drive_line_value(mem, car, timer);
    calls.timekeeper_record(mem, drive_line_value, timer);

    let dangerous = mem.dangerous_acceleration(car);
    let acceleration = mem.acceleration(car);
    if ((acceleration.z * acceleration.z + acceleration.y * acceleration.y) + acceleration.x * acceleration.x)
        > dangerous * dangerous
    {
        mem.set_overload_flag(car);
    }

    let wheels_on = (0..4).filter(|&i| mem.wheel_contact(car, i) != 0).count();
    let partial = if wheels_on < 3 { dt + mem.partial_contact(car) } else { 0.0 };
    mem.set_partial_contact(car, partial);

    let v = calls.timekeeper_update(mem);
    calls.check_retire(mem, car, v);

    // The multiply and the add round at the extended precision (single in 1.02); the store rounds the sum to single.
    let speed = calls.drive_speed(mem, layout::drive_at(car));
    let product = round_x87(f64::from(speed) * f64::from(mem.eff_wheel_radius(car)));
    let sum = round_x87(product + f64::from(mem.speed_sum(car))) as f32;
    let sum_at = layout::speed_sum_at(car);
    set_cell(Real::load(mem, sum_at), sum).store(mem, sum_at);

    let velocity = mem.velocity4(car);
    let speed = velocity.length3();
    mem.set_speed_pre(car, speed);
    if GATE_SPEED <= speed {
        let k = MINUS_ONE / speed;
        mem.set_dir_back(car, velocity.scale(k));
    } else {
        mem.set_dir_back(car, Vec4::default());
    }
    mem.set_speed(car, speed);

    let words = mem.position_words(car);
    let timer_word = mem.timer_word();
    calls.emit_position(mem, [words[0], words[1], words[2], words[3], timer_word]);
    let payload = [car, mem.sort_key(car), mem.mrr_word(car)];
    calls.emit_mrr(mem, payload);
}

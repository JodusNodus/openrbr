//! `CCar::Vf6cx` (exe 0x4ea300, `ret 4`): the car's per-step composite. The asm fixes the order: the override timer,
//! the passenger, scratch, drive and solid steps, the brake-wall raycast (its impulse, emit and contact time), the walk
//! of the velocity, the wheel and rigid-body loops, the timekeeper, the 50g flag, the partial-contact time, the position
//! update, the speed and the two state emits. The externals are answered by `Vf6cxCalls`; `Update9AF0` is one of them
//! (it is ported, but its record is not an inner call of this function).

use super::accessors::set_cell;
use super::layout::{self, Real};
use crate::car_body::store::CarStore;
use crate::fpu::round_x87;
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;

/// The calls `Vf6cx` makes out of its own body. The test answers them from the original's records.
pub trait Vf6cxCalls<S = Mem> {
    /// `CPassenger::Step(dt)` (0x523160) on the passenger at +0x8b0.
    fn passenger_step(&mut self, mem: &mut S, passenger: u32, dt: f32);
    /// `CScratchSim::NopA(dt)` (0x536f70) on the scratch sim at +0x850.
    fn scratch_nop_a(&mut self, mem: &mut S, scratch: u32, dt: f32);
    /// `CDrive::Nop3B0(dt)` (0x5043b0) on the drive at +0x4b0.
    fn drive_nop3b0(&mut self, mem: &mut S, drive: u32, dt: f32);
    /// `CSolid::E43Vf6C(dt)` (0x52cc30) on the car.
    fn solid_vf6c(&mut self, mem: &mut S, car: u32, dt: f32);
    /// `CBrakeWallManager::RaycastEntry` (0x4e4960) on the car's position (+0x140), into the 28-byte buffer. Nonzero on a hit.
    fn raycast(&mut self, mem: &mut S, car: u32, out: &mut [u8; 28]) -> u32;
    /// `E46Impulse::Impulse640(out, f, p, dt)` (0x529640) on the car, after a hit.
    fn impulse(&mut self, mem: &mut S, car: u32, out: &mut [u8; 28], f: f32, p: u32, dt: f32);
    /// The brake-wall penetration signal (0x4e4490), emitted from the buffer's first 16 bytes.
    fn emit_brake_wall(&mut self, mem: &mut S, out: &[u8; 28]);
    /// `E43PairWalk::MB560` (0x51b560) on the velocity: the walk of `a` with the car as the S30 identity.
    fn pair_walk(&mut self, a: Vec3, car: u32) -> Vec3;
    /// The wheel's `S80` (slot 32): nonzero when the wheel answers.
    fn wheel_s80(&mut self, mem: &mut S, wheel: u32) -> u32;
    /// The wheel's `S90` (slot 36).
    fn wheel_s90(&mut self, mem: &mut S, wheel: u32);
    /// `0x51fd00` on the wheel (`SimFD00`): nonzero to clear the wheel's slot.
    fn wheel_simfd00(&mut self, mem: &mut S, wheel: u32) -> u32;
    /// `0x521920` on a rigid body: nonzero when its first test holds.
    fn body_test_1920(&mut self, mem: &mut S, body: u32) -> u32;
    /// `0x521d90` on a rigid body: its step (`Vf6c(dt)`).
    fn body_vf6c(&mut self, mem: &mut S, body: u32, dt: f32);
    /// `0x521940` on a rigid body: the second test.
    fn body_test_1940(&mut self, mem: &mut S, body: u32) -> u32;
    /// `0x522100` on a rigid body (`StepBig`): nonzero to detach it.
    fn body_step_big(&mut self, mem: &mut S, body: u32) -> u32;
    /// `0x51a060` on a rigid body (`ExclA060`) with the car.
    fn body_excl(&mut self, mem: &mut S, body: u32, car: u32);
    /// The drive line's thunk (0x50a4f0, jumps to 0x475ea0) on the car's word at +0x17c and the timer; returns a float.
    fn drive_thunk(&mut self, mem: &mut S, car: u32, timer: f64) -> f32;
    /// The timekeeper's `Record(f8, timer)` (0x4a0540).
    fn timekeeper_record(&mut self, mem: &mut S, f8: f32, timer: f64);
    /// The timekeeper's `Update()` (0x4a0320): an int that `Update9AF0` takes.
    fn timekeeper_update(&mut self, mem: &mut S) -> u32;
    /// `CDrive::Get4540()` (0x504540), a float.
    fn drive_get4540(&mut self, mem: &mut S, drive: u32) -> f32;
    /// `CCar::Update9AF0(v)` (0x4e9af0), ported and called here as an external.
    fn emit_signals(&mut self, mem: &mut S, car: u32, v: u32);
    /// The position signal (0x4e8b30): the car's position lanes and the timer word, 20 bytes.
    fn emit_position(&mut self, mem: &mut S, payload: [u32; 5]);
    /// The MrR car state signal (0x4e8bf0): the car, its owner word and the float at `*(car+0x4b8)+0x45c`, 12 bytes.
    fn emit_mrr(&mut self, mem: &mut S, payload: [u32; 3]);
}

/// The brake-wall flag test's penetration threshold (`0.5`, asm 0x795e18).
const CONTACT_PENETRATION: f32 = 0.5;

/// The speed below which the direction is cleared instead of scaled (`0.001`, asm 0x795e94).
const GATE_SPEED: f32 = f32::from_bits(0x3A83_126F);

/// `-1.0` (asm 0x795e20): the direction is the velocity times `-1 / speed`.
const MINUS_ONE: f32 = -1.0;

/// `CCar::Vf6cx(this, dt)` (name ours).
pub fn step_car<S: CarStore>(calls: &mut impl Vf6cxCalls<S>, mem: &mut S, car: u32, dt: f32) {
    let timer = mem.t_override(car);
    if timer > 0.0 {
        let left = timer - dt;
        mem.set_t_override(car, if left > 0.0 { left } else { 0.0 });
    }
    calls.passenger_step(mem, layout::passenger_at(car), dt);
    calls.scratch_nop_a(mem, layout::scratch_at(car), dt);
    calls.drive_nop3b0(mem, layout::drive_at(car), dt);
    calls.solid_vf6c(mem, car, dt);

    let mut bw = [0u8; 28];
    let penetration = if calls.raycast(mem, car, &mut bw) != 0 {
        let f = f32::from_le_bytes(bw[0x14..0x18].try_into().unwrap());
        let p = u32::from_le_bytes(bw[0x10..0x14].try_into().unwrap());
        calls.impulse(mem, car, &mut bw, f, p, dt);
        calls.emit_brake_wall(mem, &bw);
        let penetration = f32::from_le_bytes(bw[0x10..0x14].try_into().unwrap());
        let in_contact = bw[0x18] & 1 != 0 && penetration > CONTACT_PENETRATION;
        let contact = if in_contact { mem.wall_contact(car) + dt } else { 0.0 };
        mem.set_wall_contact(car, contact);
        penetration
    } else {
        0.0
    };
    mem.set_wall_penetration(car, penetration);

    let walked = calls.pair_walk(mem.velocity(car), car);
    mem.set_neg_walk_y(car, 0.0 - walked.y);

    for i in 0..4 {
        let slot = layout::wheel_slot(car, i);
        let wheel = mem.u32(slot);
        if wheel == 0 {
            continue;
        }
        if mem.wheel_flag(wheel, 0) != 1 && calls.wheel_s80(mem, wheel) == 0 {
            calls.wheel_s90(mem, wheel);
        }
        let wheel = mem.u32(slot);
        if mem.wheel_flag(wheel, 0) != 1 && mem.wheel_flag(wheel, 1) != 1 && calls.wheel_simfd00(mem, wheel) != 0 {
            mem.set_u32(slot, 0);
        }
    }

    for i in 0..4 {
        let slot = layout::body_slot(car, i);
        let body = mem.u32(slot);
        if body == 0 {
            continue;
        }
        if calls.body_test_1920(mem, body) != 0 {
            calls.body_vf6c(mem, body, dt);
        } else if calls.body_test_1940(mem, body) != 0 && calls.body_step_big(mem, body) != 0 {
            calls.body_excl(mem, body, car);
            mem.set_u32(slot, 0);
        }
    }

    let timer = mem.timer_double();
    let f8 = calls.drive_thunk(mem, car, timer);
    calls.timekeeper_record(mem, f8, timer);

    let dangerous = mem.dangerous_acceleration(car);
    let accel = mem.accel_lanes(car);
    if ((accel.z * accel.z + accel.y * accel.y) + accel.x * accel.x) > dangerous * dangerous {
        mem.set_flag_50g(car);
    }

    let wheels_on = (0..4).filter(|&i| mem.wheel_ebc(car, i) != 0).count();
    let partial = if wheels_on < 3 { dt + mem.partial_contact(car) } else { 0.0 };
    mem.set_partial_contact(car, partial);

    let v = calls.timekeeper_update(mem);
    calls.emit_signals(mem, car, v);

    // The asm's `fmul`/`fadd` round at the x87 precision (f32 in 1.02); the `fstp` rounds the sum to f32.
    let gain = calls.drive_get4540(mem, layout::drive_at(car));
    let product = round_x87(f64::from(gain) * f64::from(mem.def_gain_24c(car)));
    let position = round_x87(product + f64::from(mem.value_15f4(car))) as f32;
    let position_at = layout::value_15f4_at(car);
    set_cell(Real::load(mem, position_at), position).store(mem, position_at);

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
    let w18 = mem.timer_word_18();
    calls.emit_position(mem, [words[0], words[1], words[2], words[3], w18]);
    let payload = [car, mem.owner_word(car), mem.mrr_word(car)];
    calls.emit_mrr(mem, payload);
}

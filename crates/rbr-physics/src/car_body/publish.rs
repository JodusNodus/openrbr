//! The car's sensor and signal composite, run each step. The order is the original's: the drive, passenger, engine and
//! light updates, the mounted-body loop, the car's vectors copied into the frame (the ground probe, the world placement
//! of the body frame, the wheel loop and the two state emits), the sensor sampler and its emit, the drive line
//! position's emit, the clamped speed's emit, the walk and the direction into the frame, the keeper branch, the damage
//! object's update and the engine's last update. `frame` is the base of the step's locals (`layout::publish::frame`):
//! each pointer the original passes is `frame + offset`. The externals are answered by `PublishCalls`.

use super::layout::{self, publish as callee, publish::frame as local};
use crate::car_body::store::CarStore;
use crate::entities::layout::ROOT_IDENTITY;
use crate::mem::Mem;

/// The calls the publish step makes out of its own body. The test answers them from the original's records.
pub trait PublishCalls<S = Mem> {
    /// A call on `this` with its stack arguments, returning the result word. The virtual targets go through here too.
    fn call(&mut self, mem: &mut S, callee: u32, this: u32, args: &[u32]) -> u32;
    /// A signal emit on the emitter `signal` with the payload pointer `payload`.
    fn emit(&mut self, mem: &mut S, callee: u32, signal: u32, payload: u32);
}

/// Publishes the state of `car`.
pub fn publish_state<S: CarStore>(calls: &mut impl PublishCalls<S>, mem: &mut S, car: u32, frame: u32) {
    let at = |offset: u32| frame + offset;
    calls.call(mem, callee::DRIVE_UPDATE, layout::drive_at(car), &[]);
    calls.call(mem, callee::PASSENGER_UPDATE, layout::passenger_at(car), &[]);
    calls.call(mem, callee::ENGINE_UPDATE, layout::engine_at(car), &[]);
    calls.call(mem, callee::LIGHTS_UPDATE, layout::light_system_at(car), &[]);
    calls.call(mem, callee::CAR_UPDATE, car, &[]);
    for i in 0..4 {
        let body = mem.mounted_body(car, i);
        if body != 0 && calls.call(mem, callee::MOUNTED_TEST, body, &[]) != 0 {
            calls.call(mem, callee::MOUNTED_UPDATE, body, &[]);
        }
    }

    if mem.sort_key(car) != 0 {
        calls.call(mem, layout::VEC4_COPY_FROM, at(local::POSITION), &[layout::position_at(car)]);
        // The 16 bytes after the position go to the frame in the original; no call reads them, so they are not copied.
        calls.call(mem, layout::VEC4_COPY_FROM, at(local::VELOCITY), &[car + layout::VELOCITY]);
        calls.call(mem, layout::VEC4_COPY_FROM, at(local::ANGULAR_VELOCITY), &[car + layout::ANGULAR_VELOCITY]);
        mem.set_u32(at(local::STATE_PAYLOAD), mem.sort_key(car));
        mem.set_f32(at(local::PROBE_DISTANCE), callee::PROBE_DISTANCE);
        calls.call(mem, callee::PROBE_GROUND, car, &[at(local::PROBE_OUT), at(local::PROBE_DISTANCE)]);
        calls.call(mem, layout::VEC4_CLEAR, at(local::ZERO), &[]);
        calls.call(mem, callee::COMPOSE_BODY_INTO_WORLD, car, &[at(local::ZERO), at(local::POSITION)]);
        for i in 0..4 {
            let wheel = mem.wheel_pointers(car)[i as usize];
            let (a, b) = (at(local::WHEEL_A + 0x10 * i), at(local::WHEEL_B + 0x10 * i));
            calls.call(mem, callee::WHEEL_UPDATE, wheel, &[a, b]);
        }
        calls.emit(
            mem,
            callee::STATE_EMIT_A,
            layout::signal_at(mem, callee::STATE_EMIT_A_AT),
            at(local::STATE_PAYLOAD),
        );
        calls.emit(
            mem,
            callee::STATE_EMIT_B,
            layout::signal_at(mem, callee::STATE_EMIT_B_AT),
            at(local::STATE_PAYLOAD),
        );
    }

    calls.call(mem, callee::SAMPLE_SENSORS, car, &[]);
    mem.set_u32(at(local::DRIVE_LINE_PAYLOAD), mem.u32(layout::drive_line_position_at(car)));
    calls.emit(
        mem,
        callee::DRIVE_LINE_EMIT,
        layout::signal_at(mem, callee::DRIVE_LINE_EMIT_AT),
        at(local::DRIVE_LINE_PAYLOAD),
    );
    // `max(x, 0)` as the original's compare takes it: a NaN keeps x.
    let speed = mem.forward_speed(car);
    mem.set_f32(at(local::SPEED_PAYLOAD), if 0.0 > speed { 0.0 } else { speed });
    let world_frame = mem.u32(ROOT_IDENTITY);
    calls.call(mem, callee::WALK_VECTOR, world_frame, &[layout::walk_source_at(car), car, at(local::WALKED)]);
    calls.call(mem, layout::VEC4_COPY_FROM, at(local::WALKED_COPY), &[at(local::WALKED)]);
    calls.call(mem, callee::GROUND_DIRECTION, car, &[at(local::DIRECTION)]);
    calls.call(mem, layout::VEC4_COPY_FROM, at(local::DIRECTION_COPY), &[at(local::DIRECTION)]);
    calls.emit(mem, callee::SPEED_EMIT, layout::signal_at(mem, callee::SPEED_EMIT_AT), at(local::SPEED_PAYLOAD));

    // The keeper branch. The body's and the car's identity getters return `this`, and they leave their pushed arguments
    // on the stack for the compose calls that pop them.
    if mem.settings_flags() & KEEPER_BRANCH != 0 {
        calls.call(mem, layout::VEC4_CLEAR, at(local::ZERO), &[]);
        let body = mem.body(car);
        let body_identity = layout::virtual_target(mem, body, layout::IDENTITY_SLOT);
        let r1 = calls.call(mem, body_identity, body, &[at(local::BODY_FRAME)]);
        let car_identity = layout::virtual_target(mem, car, layout::IDENTITY_SLOT);
        let r2 = calls.call(mem, car_identity, car, &[at(local::ZERO), r1]);
        calls.call(mem, callee::COMPOSE_FRAMES, r2, &[at(local::ZERO), r1, at(local::BODY_FRAME)]);
        let world_frame = mem.u32(ROOT_IDENTITY);
        let r3 = calls.call(mem, car_identity, car, &[at(local::ZERO), world_frame, at(local::WORLD_FRAME)]);
        calls.call(mem, callee::COMPOSE_FRAMES, r3, &[at(local::ZERO), world_frame, at(local::WORLD_FRAME)]);
    }

    let damage = mem.u32(layout::damage_source_at(car));
    let damage_publish = layout::virtual_target(mem, damage, layout::DAMAGE_PUBLISH_SLOT);
    calls.call(mem, damage_publish, damage, &[]);
    calls.call(mem, callee::ENGINE_LAST_UPDATE, layout::engine_at(car), &[]);
}

/// The world settings' flag that enables the keeper branch.
const KEEPER_BRANCH: u32 = 8;

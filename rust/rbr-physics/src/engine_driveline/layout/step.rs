//! `CDrive::Step1E0` (0x50a1e0), the drive's step.

use super::curve::{
    curve_input_at, curve_state_at, curve_term_at, hook_block_at, ratio_ring_at, with_gear, with_worker,
};
use super::drive_objects::{Drive, DriveObjects, Lane, DRIVE_STATE_ENGINE_SPEED};
use super::worker::{DriveWorker, GearWork, RatioRing};
use crate::engine_driveline::drive_step::{self, Step1Host};
use crate::engine_driveline::store::EngineStore;
use crate::engine_driveline::{drive_leaf, drive_worker};
use crate::Variant;

/// `CDrive::Step1E0` (0x50a1e0, thiscall, `dt` on the stack): the drive's step, in the original's order. The car
/// forwarder is a host call; the wheel, pair and gear workers run through their typed ports, each on a fresh load of the
/// worker, as their bindings do. Under NGP the speed term, the curve terms and the wheel worker are NGP's rewrites (host
/// calls), and the clutch input takes NGP's `clutch_scale`.
pub fn step1e0<S: EngineStore>(
    host: &mut impl Step1Host,
    mem: &mut S,
    this: u32,
    dt: f32,
    variant: Variant,
    clutch_scale: f32,
) {
    let ngp = variant == Variant::Ngp778;
    let objects = DriveObjects::at(mem, this);
    let (def, state) = (objects.def, objects.state);
    // The words a step reads or writes alone are read and written alone: loading and storing the whole worker for one
    // of them writes back the words it loaded, which nothing changed in between.
    let state_at = |mem: &S| mem.u32(this + 8);

    let at = state_at(mem);
    let mut count = Lane::load(mem, at + STATE_STEP_COUNT);
    count.set(count.value + 1.0);
    let (car, speed) = (mem.u32(at + STATE_CAR), mem.f32(at + DRIVE_STATE_ENGINE_SPEED));
    count.store(mem, at + STATE_STEP_COUNT);
    let fr = drive_step::ratio_input(host.fwd18e20(car, dt, speed));
    let ring_at = ratio_ring_at(state);
    let mut ring = RatioRing::load(mem, ring_at);
    let ratio = drive_worker::ring_average(&mut ring, fr);
    ring.store(mem, ring_at);
    mem.set_f32(this + THIS_RING_RATIO, ratio);

    let speed_term =
        if ngp { host.ngp_speed_term(dt) } else { with_worker(mem, this, |w| drive_leaf::speed_term(w, dt)) };
    let sum = drive_step::clutch_sum(speed_term, ratio);
    let bias: [f32; 4] = std::array::from_fn(|k| mem.f32(this + THIS_WHEEL_BIAS + 0x10 * k as u32));
    for (k, wheel_bias) in bias.iter().enumerate() {
        let k = k as u32;
        let (state_at_k, block_at) = (curve_state_at(state, k), hook_block_at(def, k));
        let term = if ngp {
            host.ngp_curve_term([state_at_k, block_at, def, curve_input_at(this)], dt)
        } else {
            curve_term_at(mem, this, def, state_at_k, block_at, dt, variant)
        };
        let block = state_at(mem) + 200 * k;
        mem.set_f32(block + BLOCK_LATCH_BOUND, drive_step::wheel_latch(term, *wheel_bias));
    }

    with_gear(mem, this, |w, g| drive_worker::gear_trim(w, g, host, dt));
    let ratio_gain = mem.f32(mem.u32(this + 4) + DEF_RATIO_GAIN);
    let k2 = drive_step::clutch_ratio(GearWork::load(mem, this).band.ratio, ratio_gain);

    let slip = Drive::load(mem, this).clutch_slip.value;
    let gain = mem.f32(mem.u32(this + 0x170) + OBJ_TORQUE_GAIN);
    let b = drive_step::clutch_input(slip, gain, clutch_scale);
    let written = with_gear(mem, this, |w, g| drive_worker::clutch_torque(w, g, host, sum, [b, k2, dt], variant));
    let dummy = written.unwrap_or(0.0);

    with_gear(mem, this, |w, g| {
        w.state.drive_gain = drive_step::drive_gain(w, g, dummy, k2);
        if drive_step::trips(w) {
            drive_step::trip(w, g);
        }
    });

    if drive_step::takes_alternate_worker(mem.u32(mem.u32(this + 4) + DEF_MODE)) {
        with_worker(mem, this, |w| drive_worker::pair_worker(w, dt));
    } else if ngp {
        host.ngp_wheel_worker(dt);
    } else {
        with_worker(mem, this, |w| drive_worker::wheel_worker(w, host, dt, None));
    }

    let w = DriveWorker::load(mem, this);
    mem.set_u32(this + THIS_SPEED_COPY, drive_step::speed_copy(&w, k2));
    mem.set_f32(this + THIS_ABS_DUMMY, drive_step::abs_magnitude(dummy));
}

/// The worker words `step1e0` reads or writes alone: the def's (`this+4`), the state's (`this+8`), the drive's and the
/// worker object's (`this+0x170`).
pub(super) const DEF_RATIO_GAIN: u32 = 0xA54;
pub(super) const DEF_MODE: u32 = 0x640;
pub(super) const STATE_CAR: u32 = 0x580;
pub(super) const STATE_STEP_COUNT: u32 = 0x584;
pub(super) const BLOCK_LATCH_BOUND: u32 = 0xF8;
pub(super) const THIS_RING_RATIO: u32 = 0x19C;
pub(super) const THIS_WHEEL_BIAS: u32 = 0x134;
pub(super) const THIS_SPEED_COPY: u32 = 0x1A4;
pub(super) const THIS_ABS_DUMMY: u32 = 0x1BC;
pub(super) const OBJ_TORQUE_GAIN: u32 = 0x84;

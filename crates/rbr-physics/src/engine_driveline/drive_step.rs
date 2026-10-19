//! `CDrive::Step1E0` (0x50a1e0, RBR_E40_Big.cpp:859): the pure parts of the drive's step. The memory sequence is
//! `layout::step1e0`, which runs the typed ports and the host calls in the original's order and does the edges. The host
//! calls are typed here, with no memory. Names are ours.

use super::drive_worker::{GearHost, MatVecHost, Step3870Host, Step4990Host};
use super::layout::{DriveWorker, GearWork};

/// The car forwarder's value is scaled by this (`0x73a2e0`).
const FWD_SCALE: f32 = 1.05;
/// The clutch torque's scale: the gain times this (`0x73a140`; NGP's per-car `10171aa4`).
pub const CLUTCH_SCALE: f32 = 900.0;

/// The host calls of `Step1E0`: the wheel and gear callees (their own traits), the car forwarder and NGP's rewrites.
pub trait Step1Host: GearHost + Step4990Host + MatVecHost + Step3870Host {
    /// `CCar::Fwd18E20` (0x4e6650, thiscall on the car `car`): the forwarder's value for `dt` and the shaft `speed`.
    fn fwd18e20(&mut self, car: u32, dt: f32, speed: f32) -> f64;
    /// NGP's `M6C90` (`0x100456c0`, the starter, `ngp::starter`): st0 for `dt`.
    fn ngp_speed_term(&mut self, dt: f32) -> f32;
    /// NGP's `Hook4160` (`0x10046880`, not ported): st0 for the curve state, the def's block, the def, the drive's
    /// input (`Hook4160`'s first four arguments) and `dt`.
    fn ngp_curve_term(&mut self, args: [u32; 4], dt: f32) -> f32;
    /// NGP's wrapper of `M9BC0` (`0x10048d50`: its shift logic, then the wheel worker; not ported).
    fn ngp_wheel_worker(&mut self, dt: f32);
}

/// The forwarder's st0 as the ratio ring takes it: one x87 product, stored as a float.
pub fn ratio_input(forwarder: f64) -> f32 {
    (forwarder * f64::from(FWD_SCALE)) as f32
}

/// The sum the clutch torque takes: the speed term and the ring's mean.
pub fn clutch_sum(speed_term: f32, ring: f32) -> f32 {
    speed_term + ring
}

/// The curve term plus the wheel's bias: the value the block's latch bound takes.
pub fn wheel_latch(term: f32, bias: f32) -> f32 {
    term + bias
}

/// The clutch torque's `b`: the drive's clutch slip gives `1 - slip`, scaled by the object's gain and `scale`.
pub fn clutch_input(slip: f32, torque_gain: f32, scale: f32) -> f32 {
    (1.0 - slip) * torque_gain * scale
}

/// The clutch's `k2`: the gear's ratio times the definition's ratio gain.
pub fn clutch_ratio(gear_ratio: f32, ratio_gain: f32) -> f32 {
    gear_ratio * ratio_gain
}

/// Whether the pair worker (`M96C0`) runs instead of the wheel worker: the mode (`DriveType`) is 3.
pub fn takes_alternate_worker(mode: u32) -> bool {
    mode == 3
}

/// `dri+0x37c`, the drive gain: zero when the gear flag is clear or the gear's ratio is zero, else the clutch's
/// magnitude times `k2`.
pub fn drive_gain(w: &DriveWorker, g: &GearWork, dummy: f32, k2: f32) -> f32 {
    if w.state.gear_flag.value == 0 || g.band.ratio == 0.0 {
        0.0
    } else {
        dummy * k2
    }
}

/// Whether the step trips: the trigger (`+0x578`) or the object's gate trip (`obj+0x70`) is set.
pub fn trips(w: &DriveWorker) -> bool {
    w.state.trigger != 0 || w.obj.gate_trip != 0
}

/// The trip: the pack gain takes the trip's term, the pending word is set, the step count goes up, and the trigger and
/// the gate trip are cleared. The term is the gear's ratio squared times the slip gain (`+0x664` when the sign word is 0,
/// and `+0x674` added to it), only while the gear flag is set.
pub fn trip(w: &mut DriveWorker, g: &GearWork) {
    let divg = g.band.ratio;
    let mut term = 0.0;
    if w.state.gear_flag.value != 0 {
        let slip = if w.state.sign_word == 0 { w.def.slip_gain_a + w.def.slip_gain_b } else { w.def.slip_gain_b };
        term = divg * divg * slip;
    }
    w.state.pack_gain = (w.def.pack_term + term) * w.def.ratio_gain * w.def.ratio_gain;
    w.state.pending = 1;
    w.state.step_iter = w.state.step_iter.wrapping_add(1);
    w.state.trigger = 0;
    w.obj.gate_trip = 0;
}

/// `+0x1a4`, the speed copy, as bits: the engine speed (`+0x45c`) unless the sign word, the gear flag and `k2` are all
/// set, when it is the drive speed key (`+0x384`).
pub fn speed_copy(w: &DriveWorker, k2: f32) -> u32 {
    if w.state.sign_word == 0 || w.state.gear_flag.value == 0 || k2 == 0.0 {
        w.state.drive.engine_speed.to_bits()
    } else {
        w.state.drive.drive_speed_key.to_bits()
    }
}

/// `+0x1bc`, the magnitude of the clutch torque: `dummy` when it is positive, else `0 - dummy`.
pub fn abs_magnitude(dummy: f32) -> f32 {
    if dummy > 0.0 {
        dummy
    } else {
        0.0 - dummy
    }
}

//! The steering force-feedback torque (`notes/ngp-ffb.md`): its filter (`CCar::UpdateFfbTorque`, Hook1FF0, exe 0x4e7ff0). The asm (not the C++) fixes the order:
//! the constants are the exe's own words (0.2f is 1 ulp below 0.2, `0x3e4ccccc`; 1/6 is `0x3e2aaaab`).

use crate::car_body::store::CarStore;
use crate::Variant;

/// `CCar::UpdateFfbTorque`: returns the new filtered torque. While `t_override` is above 0 the value is 0. Otherwise
/// the target is `rack_ratio * torque_sum` ramped by a speed factor `f4` (0.5 at rest, 0.7 above 13 m/s), blended with the
/// current value. The caller dirties the cell in both cases (`accessors::set_cell`).
pub fn update(t_override: f32, speed: f32, rack_ratio: f32, torque_sum: f32, torque: f32) -> f32 {
    if t_override > 0.0 {
        return 0.0;
    }
    // NaN-preserving clamp (the asm's `jbe` takes the unordered case): NaN stays NaN.
    let mut f3 = (speed - 7.0) * 0.16666667;
    if f3 < 0.0 {
        f3 = 0.0;
    } else if 1.0 < f3 {
        f3 = 1.0;
    }
    let f4 = f3 * 0.19999998807907104 + 0.5;
    // x87 order of the asm: ((rack_ratio * torque_sum) * f4) + ((1 - f4) * torque).
    let target = rack_ratio * torque_sum;
    (target * f4) + ((1.0 - f4) * torque)
}

/// The calls `FfbNorm` makes out of its own body: the slot-35 gate (`S8C`) on the car.
pub trait FfbNormCalls {
    /// `S8C` (vtable slot 35 of the car): nonzero makes the norm 0.
    fn s8c(&mut self, car: u32) -> u32;
}

/// `CCar::FfbNorm` (0x4e76c0, `ret`): the filtered torque over `MaxFFTorque`, clamped to [-1, 1]; 0 when the gate
/// fires. The asm's clamp takes the NaN case (its `jbe` and `ja` both fall through): NaN stays NaN.
pub fn ffb_norm<S: CarStore>(calls: &mut impl FfbNormCalls, mem: &S, car: u32) -> f32 {
    if calls.s8c(car) != 0 {
        return 0.0;
    }
    let f = mem.ffb_torque(car).value / mem.max_ff_torque(car);
    if -1.0 > f {
        -1.0
    } else if f > 1.0 {
        1.0
    } else {
        f
    }
}
/// The force `CFFBController::EmitForce` sends the wheel for the car's state, in [-1, 1] before the player's strength
/// and direction: `FfbNorm` in 1.02, NGP's curve (`ngp::ffb`) under NGP. Both are 0 while the car's `+0x1ec` is set.
pub fn force<S: CarStore>(mem: &S, car: u32, variant: Variant) -> f32 {
    let gate = mem.u32(car + 0x1EC);
    match variant {
        Variant::Stock102 => ffb_norm(&mut Gate(gate), mem, car),
        Variant::Ngp778 => crate::ngp::ffb::curve(mem.ffb_torque(car).value, gate != 0, mem.max_ff_torque(car)),
    }
}

/// `FfbNorm`'s gate, slot 35 of the car: `CSolid::E43Vf8C` (0x5293e0) returns the car's `+0x1ec`.
struct Gate(u32);

impl FfbNormCalls for Gate {
    fn s8c(&mut self, _car: u32) -> u32 {
        self.0
    }
}

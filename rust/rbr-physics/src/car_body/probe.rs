//! `CCar::MotionProbe(out)` (exe 0x4e7730, `ret 4`): a probe of the surface along the car's `+z` axis, for the car step
//! (`StepEngine` calls it at 0x4e92c7 and 0x4e9c1f). The asm fixes the order of the tests and the dot product; the
//! result is the `out` word and the return value (`Probe::out`, `Probe::ret`).

use crate::car_body::store::CarStore;
use crate::math::vec::Vec3;

/// The calls the probe makes out of its own body. The test answers them from the original's records.
pub trait ProbeCalls {
    /// `S8C` (vtable slot 35 of the car): a gate. Nonzero ends the probe with no hit.
    fn s8c(&mut self, car: u32) -> u32;
    /// `E60Flow::M5666b0` on the flow object (`DAT_0165fa90`). Nonzero ends the probe with no hit.
    fn m5666b0(&mut self) -> u32;
    /// `ToWorld(v, out)` on the car: the vector `v` in the world frame (three lanes).
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3;
    /// `Sampler54A0(this + 0x140, big, sm)`: the sampler's result and its two verdict words (`big + 0x44`, `big + 0x48`;
    /// their meaning is open). The asm's buffer is at `E + 0x30`, so the C++ comment's `+0x40`/`+0x44` is off by 4.
    fn sampler(&mut self, car: u32) -> Sample;
}

/// What `ProbeCalls::sampler` returned: `eax`, and the verdict words it wrote to the big buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub found: u32,
    pub first: u32,
    pub second: u32,
}

/// The probe's outcome, with the `out` word and the return value the original gives for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    /// The gate is set: `out = 0`, returns 0.
    Gated,
    /// The car is faster than `SPEED_LIMIT`: `out = 4`, returns 0.
    TooFast,
    /// The sampler's first verdict word is set: `out = 1`, returns 0.
    First,
    /// The sampler's second verdict word is set and the probe points behind the car: `out = 2`, returns 0.
    Second,
    /// Nothing found (the flow says no, or the sampler does not hit, or its words are zero): `out = 0`, returns 1.
    Miss,
}

impl Probe {
    /// The `out` word the original writes.
    pub fn out(self) -> u32 {
        match self {
            Probe::Gated | Probe::Miss => 0,
            Probe::TooFast => 4,
            Probe::First => 1,
            Probe::Second => 2,
        }
    }

    /// The return value (eax).
    pub fn ret(self) -> u32 {
        match self {
            Probe::Miss => 1,
            _ => 0,
        }
    }
}

/// The speed above which the probe is not made (`1.5`, asm 0x72dcdc), in m/s of the velocity's length.
const SPEED_LIMIT: f32 = 1.5;

/// The probe direction in the car frame, `(0, 0, 1)`.
const DIRECTION: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

/// `CCar::MotionProbe`. `neg` is the asm's sign of the dot of the world direction and `DIRECTION`: the second verdict
/// counts only behind the car.
pub fn motion_probe<S: CarStore>(probe: &mut impl ProbeCalls, mem: &S, car: u32) -> Probe {
    if probe.s8c(car) != 0 {
        return Probe::Gated;
    }
    if mem.velocity(car).length3() > SPEED_LIMIT {
        return Probe::TooFast;
    }
    if probe.m5666b0() != 0 {
        return Probe::Miss;
    }
    let world = probe.to_world(car, DIRECTION);
    let dot = world.z * DIRECTION.z + world.y * DIRECTION.y + DIRECTION.x * world.x;
    let behind = dot < 0.0;
    let sample = probe.sampler(car);
    if sample.found == 0 {
        Probe::Miss
    } else if sample.first != 0 {
        Probe::First
    } else if behind && sample.second != 0 {
        Probe::Second
    } else {
        Probe::Miss
    }
}

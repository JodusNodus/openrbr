//! The motion probe: a probe of the surface along the car's `+z` axis, for the car step and the call-for-help path.
//! The order of the tests and the dot product are the original's; the result is the `out` word and the return value
//! (`Probe::out`, `Probe::ret`).

use crate::car_body::store::CarStore;
use crate::math::vec::Vec3;

/// The calls the probe makes out of its own body. The test answers them from the original's records.
pub trait ProbeCalls {
    /// The car's gate: nonzero ends the probe with no hit.
    fn blocked(&mut self, car: u32) -> u32;
    /// The flow object's gate: nonzero ends the probe with no hit.
    fn flow_gate(&mut self) -> u32;
    /// The vector `v` of the car frame in the world frame (three lanes).
    fn to_world(&mut self, car: u32, v: Vec3) -> Vec3;
    /// The surface sample_surface at the car's position: its result and its two verdict words (their meaning is open).
    fn sample_surface(&mut self, car: u32) -> Sample;
}

/// What `ProbeCalls::sample_surface` returned: the result, and the verdict words it wrote.
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
    /// The sample_surface's first verdict word is set: `out = 1`, returns 0.
    First,
    /// The sample_surface's second verdict word is set and the probe points behind the car: `out = 2`, returns 0.
    Second,
    /// Nothing found (the flow says no, or the sample_surface does not hit, or its words are zero): `out = 0`, returns 1.
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

    /// The return value.
    pub fn ret(self) -> u32 {
        match self {
            Probe::Miss => 1,
            _ => 0,
        }
    }
}

/// The speed above which the probe is not made, m/s of the velocity's length.
const SPEED_LIMIT: f32 = 1.5;

/// The probe direction in the car frame, `(0, 0, 1)`.
const DIRECTION: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

/// Probes the surface. `behind` is the sign of the dot of the world direction and `DIRECTION`: the second verdict
/// counts only behind the car.
pub fn motion_probe<S: CarStore>(probe: &mut impl ProbeCalls, mem: &S, car: u32) -> Probe {
    if probe.blocked(car) != 0 {
        return Probe::Gated;
    }
    if mem.velocity(car).length3() > SPEED_LIMIT {
        return Probe::TooFast;
    }
    if probe.flow_gate() != 0 {
        return Probe::Miss;
    }
    let world = probe.to_world(car, DIRECTION);
    let dot = world.z * DIRECTION.z + world.y * DIRECTION.y + DIRECTION.x * world.x;
    let behind = dot < 0.0;
    let sample = probe.sample_surface(car);
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

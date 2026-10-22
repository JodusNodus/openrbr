//! NGP's brake: the torque of a wheel's brake and the heating of its disc. The pad's friction coefficient comes from
//! the original's row lookup (the caller's `friction`, a curve over the disc temperature that also wears the pad); NGP
//! scales it with the disc's rolling speed, multiplies by the clamp force and the effective radius, and steps the
//! disc's two temperatures (the friction layer and the core, microkelvin words) with conduction between them,
//! convection to the air (still air plus forced, as the tyre's) and radiation. 1.02's version
//! (`engine_driveline::drive_curve::wheel_curve_term`) only moves the row's counters. The fields are NGP's `Brake*_NGP`
//! keys (`data/ngp-lsp-keys.md`); the inputs are named by use. All SSE, in doubles where the original uses them.

/// A wheel's brake definition.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Brake {
    /// `BrakeRatio`: the share of the pedal pressure this wheel's brake takes.
    pub ratio: f32,
    /// `BrakePistonArea`: pressure to clamp force.
    pub piston_area: f32,
    /// `BrakeRadiusEff`: clamp force to torque, m.
    pub effective_radius: f32,
    /// `BrakePlateArea`: the disc's surface for convection and radiation, m^2.
    pub plate_area: f32,
    /// `BrakeIntConvec`: the conduction coefficient between the friction layer and the core.
    pub internal_convection: f32,
    /// `BrakeHeatAbsRatio`: the share of the friction power the disc absorbs.
    pub heat_absorption: f32,
    /// `BrakeSpecHeat`, J/(kg K).
    pub specific_heat: f32,
    /// `BrakeMass`, kg.
    pub mass: f32,
    /// `BrakeConvFtr`: the forced convection factor of the friction layer.
    pub convection_factor: f32,
    /// `BrakeLayerMassPct`: the friction layer's share of the mass, as a fraction.
    pub layer_mass_share: f32,
    /// `BrakeVentArea`: the core's convection factor through the vents.
    pub vent_area: f32,
}

/// What the brake step reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrakeIn {
    pub brake: Brake,
    /// The disc's rotation speed, rad/s.
    pub wheel_speed: f32,
    /// The temperatures as words, microkelvin: the friction layer's and the core's.
    pub layer_temperature: u32,
    pub core_temperature: u32,
    /// The pedal pressure and the handbrake pressure of the input (named by use).
    pub pedal_pressure: f32,
    pub handbrake_pressure: f32,
    /// The wheel's own gain of the handbrake term, and NGP's per-car factor of it (1 without a handbrake setting).
    pub handbrake_gain: f32,
    pub handbrake_factor: f32,
    /// A front wheel whose gate word is clear: the input is 0.
    pub released: bool,
    /// The settings' fade of the friction with the disc's rolling speed: the front wheels' or the rear's.
    pub speed_fade: f32,
    /// The car's speed, m/s, and the air temperature, K.
    pub car_speed: f32,
    pub air_temperature: f32,
    pub dt: f32,
    /// Whether part of the handbrake term moves to the wheel before: a rear wheel, centre diff type 3 and the handbrake
    /// at most the centre diff's release.
    pub split_handbrake: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BrakeOut {
    /// The brake torque.
    pub torque: f32,
    pub layer_temperature: u32,
    pub core_temperature: u32,
    /// Fourteen intermediate values NGP keeps for its telemetry, in its order: the input pressure, the clamp force, the
    /// friction coefficient, the disc's rolling speed, the faded coefficient, the convection coefficient, the torque,
    /// the conduction, the layer's heat flow to the air, the core's, the friction power, the layer's and the core's
    /// heat, and the radiation.
    pub telemetry: [f32; 14],
    /// What the split adds to the word before.
    pub transfer: Option<f32>,
}

/// `max(a, b)` with `b` when they are unordered (SSE's `maxss`).
fn max_ss(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// The C runtime's `_ftol`: truncated to 64 bits, the low word (0 for NaN or past 2^63).
fn ftol(x: f32) -> u32 {
    if x.is_nan() || x.abs() >= 2f32.powi(63) {
        0
    } else {
        x as i64 as u32
    }
}

/// A temperature word in kelvin: unsigned, times 1e-6.
fn kelvin(word: u32) -> f32 {
    f64::from(word) as f32 * 1.0e-6
}

/// The exponent of the speed in the forced convection, as the f32 0.78 widened to double (as the tyre's).
const SPEED_EXPONENT: f64 = 0.78_f32 as f64;
/// The Stefan-Boltzmann constant (5.67037e-8) times an emissivity of 0.3.
const RADIATION: f64 = 1.701_111_9e-8;

/// One brake step. `friction` is the original's row lookup on the clamp force: the pad's friction coefficient at the
/// disc temperature.
pub fn brake_step(i: &BrakeIn, friction: impl FnOnce(f32) -> f32) -> BrakeOut {
    let b = &i.brake;
    let handbrake = i.handbrake_gain * i.handbrake_pressure * i.handbrake_factor;
    let pressure = if i.released { 0.0 } else { b.ratio * i.pedal_pressure + handbrake };
    let force = pressure * b.piston_area;
    let mu = friction(force);
    let speed = i.wheel_speed.abs();
    let rolling = speed * b.effective_radius;
    let faded = (1.0 - rolling * i.speed_fade) * mu;
    let airflow = max_ss(i.car_speed.abs(), rolling);
    let convection = (f64::from(airflow).powf(SPEED_EXPONENT) as f32 + 1.0) * b.convection_factor;
    let torque = faded * force * b.effective_radius;
    let (core, layer) = (kelvin(i.core_temperature), kelvin(i.layer_temperature));
    let conduction = b.plate_area * b.internal_convection * (core - layer);
    let to_air = b.plate_area * convection * (i.air_temperature - layer);
    let pow4 = |t: f32| f64::from(t).powf(4.0);
    let radiation = ((pow4(i.air_temperature) - pow4(layer)) * (f64::from(b.plate_area) * RADIATION)) as f32;
    let core_to_air = convection * b.vent_area * (i.air_temperature - core);
    let heat_capacity = b.specific_heat * b.mass;
    let power = torque * speed * b.heat_absorption;
    let layer_heat = (power + to_air + conduction + radiation) * i.dt;
    let core_heat = (core_to_air - conduction) * i.dt;
    let layer_temperature = ftol((layer_heat / (b.layer_mass_share * heat_capacity) + layer) / 1.0e-6);
    let core_temperature = ftol((core_heat / ((1.0 - b.layer_mass_share) * heat_capacity) + core) / 1.0e-6);
    let telemetry = [
        pressure,
        force,
        mu,
        rolling,
        faded,
        convection,
        torque,
        conduction,
        to_air,
        core_to_air,
        power,
        layer_heat,
        core_heat,
        radiation,
    ];
    let transfer = (i.split_handbrake && handbrake > 0.0).then(|| handbrake / pressure * 0.5 * torque);
    BrakeOut {
        torque: transfer.map_or(torque, |t| torque - t),
        layer_temperature,
        core_temperature,
        telemetry,
        transfer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftol_keeps_the_low_word() {
        assert_eq!(ftol(2.5e8), 250_000_000);
        assert_eq!(ftol(3.0e9), 3_000_000_000);
        assert_eq!(ftol(-1.5), u32::MAX);
        assert_eq!(ftol(f32::NAN), 0);
        assert_eq!(ftol(1.0e19), 0);
    }
}

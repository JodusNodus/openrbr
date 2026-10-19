//! NGP's rewrite of `E39_Hook4160` (`0x10046880`, the call at `0x50a2a8` in `Step1E0`): the wheel's curve term with
//! NGP's heat model. Like 1.02's (`engine_driveline::drive_curve::wheel_curve_term`) it runs `row_value` on the speed
//! term, but scales the value by the slope, and steps the two temperature words (`+0x7c`, `+0x8c`, microkelvin) with
//! a conduction term, convection to the air (`10171adc`) and radiation. It stores the words raw (no dirty flags) and
//! fourteen of its terms in a telemetry row. All SSE, in doubles where the asm uses them.

use crate::engine_driveline::layout::HookBlock;

/// The block's words NGP reads beyond 1.02's [`HookBlock`] (names ours): the convection scale (`+0x4c`), the share
/// of the heat that goes to the first word (`+0x5c`) and the lane's convection scale (`+0xac`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlockExtra {
    pub convection: f32,
    pub share: f32,
    pub lane_convection: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveTermIn {
    pub block: HookBlock,
    pub extra: BlockExtra,
    /// The state's slope (`+0x1c`), term gain (`+0xac`) and the two temperature words (`+0x7c`, `+0x8c`).
    pub slope: f32,
    pub term_gain: f32,
    pub pos: u32,
    pub lane: u32,
    /// The input's `+0x60` and `+0x70`.
    pub w60: f32,
    pub w70: f32,
    /// NGP's per-car factor of the gain term (`10171a94`).
    pub gain_factor: f32,
    /// Front wheels (0 and 1) with the drive object's gate (`+0x170`, `+0x80`) clear: the speed term is 0.
    pub gated: bool,
    /// The settings' slope factor: `+0xfe8` for the front wheels, `+0x1168` for the rear.
    pub slope_factor: f32,
    /// The car's speed (`+0x15f8`) and the air temperature (`10171adc`).
    pub speed: f32,
    pub air: f32,
    pub dt: f32,
    /// Whether part of the gain term moves to the wheel before (rear wheels, centre diff type 3, the car's `+0x864`
    /// at most the locks' `+0x198`).
    pub split: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveTermOut {
    pub value: f32,
    pub pos: u32,
    pub lane: u32,
    /// The telemetry row (`1017662c + 0x3c u`), in address order.
    pub telemetry: [f32; 14],
    /// What the split adds to the word before (`state + 200 u - 0x98`).
    pub transfer: Option<f32>,
}

/// `maxss a, b`: `b` when they are unordered.
fn max_ss(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

/// The CRT's `_ftol` (`0x10120630`): truncated to 64 bits, the low word (0 for NaN or past 2^63).
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

/// `0x10046880`. `row_value` is the original `Worker4060` (through NGP's trampoline `0x100466b0`) on the speed term.
pub fn curve_term(i: &CurveTermIn, row_value: impl FnOnce(f32) -> f32) -> CurveTermOut {
    let b = &i.block;
    let gain = i.term_gain * i.w70 * i.gain_factor;
    let input = if i.gated { 0.0 } else { b.e_scale * i.w60 + gain };
    let x = input * b.dt_scale;
    let r = row_value(x);
    let slope = i.slope.abs();
    let slip = slope * b.gain;
    let scaled = (1.0 - slip * i.slope_factor) * r;
    let m = max_ss(i.speed.abs(), slip);
    let conv = (f64::from(m).powf(f64::from(0.78f32)) as f32 + 1.0) * i.extra.convection;
    let value = scaled * x * b.gain;
    let (lane_t, pos_t) = (kelvin(i.lane), kelvin(i.pos));
    let conduction = b.p_scale * b.x1_scale * (lane_t - pos_t);
    let to_air = b.p_scale * conv * (i.air - pos_t);
    let pow4 = |t: f32| f64::from(t).powf(4.0);
    let radiation = ((pow4(i.air) - pow4(pos_t)) * (f64::from(b.p_scale) * 1.701_111_9e-8)) as f32;
    let lane_air = conv * i.extra.lane_convection * (i.air - lane_t);
    let den = b.den_a * b.den_b;
    let friction = value * slope * b.w_scale;
    let heat = (friction + to_air + conduction + radiation) * i.dt;
    let lane_heat = (lane_air - conduction) * i.dt;
    let pos = ftol((heat / (i.extra.share * den) + pos_t) / 1.0e-6);
    let lane = ftol((lane_heat / ((1.0 - i.extra.share) * den) + lane_t) / 1.0e-6);
    let telemetry =
        [input, x, r, slip, scaled, conv, value, conduction, to_air, lane_air, friction, heat, lane_heat, radiation];
    let transfer = (i.split && gain > 0.0).then(|| gain / input * 0.5 * value);
    CurveTermOut { value: transfer.map_or(value, |t| value - t), pos, lane, telemetry, transfer }
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

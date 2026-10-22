//! Spring damper physics (exe E48). The functions here use typed values only;
//! layout is the edge that translates those values to the game's memory.
//! The original asm decides operation order and unordered-comparison behavior.

pub mod layout;
pub mod store;
pub mod suspension;

use crate::fpu::{round24, round_x87};
use crate::math::le_or_unordered;
use std::cmp::Ordering;

const ONE: f32 = f32::from_bits(0x3f80_0000); // 0x795e14
const MERGE_THRESHOLD: f32 = f32::from_bits(0x3586_37bd); // 0x795e70
const RELAX_FLOOR: f32 = f32::from_bits(0x3727_c5ac); // 0x795e8c
/// Kelvin offset of the Celsius scale: 273.15 K.
const KELVIN_OFFSET: f32 = f32::from_bits(0x4388_9333); // 0x73a128
const OUTPUT_SCALE: f32 = f32::from_bits(0x47c3_5000); // 0x73c9f4
const CURVE_LINEAR: f32 = f32::from_bits(0x3fa4_9249); // 0x740264
const CURVE_RECIPROCAL: f32 = f32::from_bits(0x3e92_4925); // 0x740268
const MIN_BUMP_DAMPING: f32 = 400.0;

/// A named CReal value plus its two bookkeeping words. layout owns the offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RealValue {
    pub label: u32,
    pub dirty: u32,
    pub value: f32,
}

/// Spring-damper definition values read by the per-wheel update. Each name is
/// taken from notes/physics.md "Spring-damper"; inferred names remain marked there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Definition {
    pub debug_id: u32,
    pub bump_stop_pos: RealValue,
    pub droop_stop_pos: RealValue,
    pub spring_free_length: RealValue,
    pub spring_rate: RealValue,
    pub helper_min_length: RealValue,
    pub helper_length: RealValue,
    pub helper_rate: RealValue,
    pub bump_stop_rate: RealValue,
    pub droop_stop_rate: RealValue,
    pub damp_bump: RealValue,
    pub damp_rebound: RealValue,
    pub stop_damp: RealValue,
    pub hs_break_bump: RealValue,
    pub hs_break_rebound: RealValue,
    pub hs_damp_bump: RealValue,
    pub hs_damp_rebound: RealValue,
}

impl Definition {
    /// Sets the droop-stop rate to 600000; the edge layer writes the
    /// base and CReal constructor headers as well.
    pub fn new() -> Self {
        Self { droop_stop_rate: RealValue { dirty: 1, value: 600_000.0, ..RealValue::default() }, ..Self::default() }
    }
}

/// Spring-damper setup fields copied into the definition on setup adoption.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SetupValues {
    pub spring_free_length: RealValue,
    pub spring_rate: RealValue,
    pub helper_min_length: RealValue,
    pub helper_length: RealValue,
    pub helper_rate: RealValue,
    pub damp_bump: RealValue,
    pub damp_rebound: RealValue,
    pub hs_break_bump: RealValue,
    pub hs_damp_bump: RealValue,
}

/// Per-wheel effective damping values and the owner used by reset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    pub damp_bump_eff: f32,
    pub damp_rebound_eff: f32,
    pub hs_damp_bump_eff: f32,
    pub owner: u32,
    pub last_deflection: RealValue,
    pub unknown_value: u32,
}

/// Two values used by the fade accumulator.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FadeState {
    pub test_multiplier: f32,
    pub amount: f32,
}

/// Values sent through the damper signal on reset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamperSignal {
    pub owner_id: u32,
    pub peak_velocity: f32,
    pub bump_velocity: f32,
    pub droop_velocity: f32,
    pub valid: i32,
}

/// Mutable per-wheel controller state used by Step and ResetOutputs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controller {
    pub mode: i32,
    pub high_speed_hold: i32,
    pub signal: DamperSignal,
}

/// Strut position, coordinate, and velocity passed to Step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepInput {
    pub travel: f32,
    pub coordinate: f32,
    pub velocity: f32,
}

/// The three output values written by Step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepOutput {
    pub spring_stop_force: f32,
    pub breakpoint_offset: f32,
    pub damping_coefficient: f32,
}

/// A body's two-factor heat capacity: `mass * specific_heat` is the capacity in J/K.
/// The tyre def stores them as raw pairs (surface, tread, carcass); the order is the
/// mass in kg followed by the specific heat in J/(kg·K).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeatCapacity {
    pub mass: f32,
    pub specific_heat: f32,
}

/// The engine's per-block heat accumulator. The divisor joins the shared scale word
/// to form the heat capacity. The running value is named temperature by use: the engine
/// converts it to Celsius. That interpretation is inferred for the other shared blocks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeatAccumulator {
    pub divisor: f32,
    pub temperature: f32,
}

/// Transfer parameters for the clamp. Their physical units are not known.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClampParameters {
    pub transfer_scale: f32,
    pub transfer_divisor: f32,
    pub reserved: f32,
    pub maximum_value: f32,
}

/// A two-word weighted value used by the clamp and the merge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeightedValue {
    pub weight: f32,
    pub value: f32,
}

/// Four related scalar terms with one fixed relation:
/// `response = (numerator / denominator) * scale * gas_constant`.
/// The engine's air cells and the tyre's scratch use it as the ideal-gas law with dry
/// air's R (287 J/(kg·K)); the exhaust blend code reuses the same four words as scratch.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UpdateTerms {
    pub scale: f32,
    pub numerator: f32,
    pub denominator: f32,
    pub response: f32,
}

/// Four-word value block used by the combine. Some meanings remain unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlendTerms {
    pub weight: f32,
    pub value: f32,
    pub response: f32,
    pub scale: f32,
}

/// Four-word state used by the relax and the solve.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Relaxation {
    pub remaining: f32,
    pub accumulated: f32,
    pub response: f32,
    pub scale: f32,
}

/// Four-word accumulator used by the force step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ForceTerms {
    pub multiplier: f32,
    pub accumulated_force: f32,
    pub response: f32,
    pub divisor: f32,
}

/// Sensible heat flow, W: `(hot - cold) * coefficient * area`, with each operation
/// rounded to x87 precision. NGP replaces it with `ngp::heat::heat_flow_to_air` at the live
/// air temperature.
pub fn heat_flow(hot: f32, cold: f32, coefficient: f32, area: f32) -> f64 {
    let difference = round_x87(hot as f64 - cold as f64);
    let scaled = round_x87(difference * coefficient as f64);
    round_x87(scaled * area as f64)
}

/// Conduction through an area, W: `-(gradient * conductivity * area)`, with each product
/// rounded to x87 precision. NGP replaces it with `ngp::heat::conduction`, which runs
/// the same product in f64.
pub fn conduction(gradient: f32, conductivity: f32, area: f32) -> f64 {
    let product = round_x87(gradient as f64 * conductivity as f64);
    -round_x87(product * area as f64)
}

/// Kelvin to Celsius: `kelvin - 273.15`, rounded to x87 precision.
pub fn kelvin_to_celsius(kelvin: f32) -> f64 {
    round_x87(kelvin as f64 - KELVIN_OFFSET as f64)
}

/// Adds heat to a body: `*temperature += heat / (mass * specific_heat)`, in f32 steps.
pub fn accumulate_heat(capacity: HeatCapacity, heat: f32, temperature: &mut f32) {
    let denominator = sse_mul(capacity.specific_heat, capacity.mass);
    let rise = sse_div(heat, denominator);
    *temperature = sse_add(*temperature, rise);
}

/// Adds heat through the engine's shared scale word:
/// `*temperature += heat / (scale * divisor)`, in f32 steps. The scale word lives in
/// the shared parameter block the clamp reads too; the split between the two factors
/// is not known.
pub fn accumulate_heat_scaled(scale: f32, heat: f32, cell: &mut HeatAccumulator) {
    let denominator = sse_mul(scale, cell.divisor);
    let rise = sse_div(heat, denominator);
    cell.temperature = sse_add(cell.temperature, rise);
}

/// Moves the weighted value above the configured limit and returns the transfer.
pub fn clamp_value(parameters: ClampParameters, target: &mut WeightedValue) -> f32 {
    let value = target.value;
    if !(parameters.maximum_value <= value && value != parameters.maximum_value) {
        return 0.0;
    }
    let excess = sse_sub(value, parameters.maximum_value);
    let scaled = sse_mul(excess, parameters.transfer_scale);
    let weighted = sse_mul(scaled, target.weight);
    let mut transfer = sse_div(weighted, parameters.transfer_divisor);
    let remainder = sse_sub(target.weight, transfer);
    target.value = parameters.maximum_value;
    target.weight = remainder;
    if remainder < 0.0 {
        target.weight = 0.0;
        transfer = sse_add(transfer, remainder);
    }
    transfer
}

/// Combines two weighted values, preserving the larger side near zero.
pub fn merge_value(source: WeightedValue, target: &mut WeightedValue) {
    let total_weight = source.weight + target.weight;
    if total_weight > MERGE_THRESHOLD {
        let weighted_sum = source.value * source.weight + target.value * target.weight;
        target.value = weighted_sum / total_weight;
        target.weight = total_weight;
        return;
    }
    if target.weight <= source.weight && source.weight != target.weight {
        target.value = source.value;
    }
    target.weight = total_weight;
}

/// Solves the shared four-term relation for the response:
/// `response = (numerator / denominator) * scale * gas_constant`, or zero for a zero
/// denominator. Every caller passes dry air's R (287 J/(kg·K)).
pub fn solve_response(terms: &mut UpdateTerms, gas_constant: f32) {
    terms.response = if terms.denominator != 0.0 {
        let ratio = sse_div(terms.numerator, terms.denominator);
        let scaled = sse_mul(ratio, terms.scale);
        sse_mul(scaled, gas_constant)
    } else {
        0.0
    };
}

/// Solves the shared four-term relation for the scale:
/// `scale = (response * denominator) / (gas_constant * numerator)`, or zero for a zero
/// numerator. Every caller passes dry air's R (287 J/(kg·K)).
pub fn solve_scale(terms: &mut UpdateTerms, gas_constant: f32) {
    terms.scale = if terms.numerator != 0.0 {
        let numerator = sse_mul(terms.response, terms.denominator);
        let denominator = sse_mul(gas_constant, terms.numerator);
        sse_div(numerator, denominator)
    } else {
        0.0
    };
}

/// Solves the shared four-term relation for the numerator:
/// `numerator = (response * denominator) / (scale * gas_constant)`, or zero for a zero
/// scale. Every caller passes dry air's R (287 J/(kg·K)).
pub fn solve_numerator(terms: &mut UpdateTerms, gas_constant: f32) {
    terms.numerator = if terms.scale != 0.0 {
        let numerator = sse_mul(terms.response, terms.denominator);
        let denominator = sse_mul(terms.scale, gas_constant);
        sse_div(numerator, denominator)
    } else {
        0.0
    };
}

/// Multiplies by the output scale on the x87, returned in st0.
pub fn scale(value: f32) -> f64 {
    round_x87(value as f64 * OUTPUT_SCALE as f64)
}

/// Applies two square roots on the x87 (the fourth root), returned in st0.
/// Non-positive input gives 0.
pub fn sqrt4(value: f32) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    let first = round_x87((value as f64).sqrt());
    round_x87(first.sqrt())
}

/// Shapes a value on the x87, taking the reciprocal branch for unordered input; returned in st0.
pub fn curve(value: f32) -> f64 {
    let x87 = round_x87;
    let value64 = value as f64;
    let linear = CURVE_LINEAR as f64;
    let reciprocal = CURVE_RECIPROCAL as f64;
    if value < ONE {
        // x87 order: scale, subtract, multiply.
        let scaled = x87(value64 * reciprocal);
        let offset = x87(linear - scaled);
        x87(offset * value64)
    } else {
        // x87 order: reciprocal, scale, subtract, multiply, reciprocal.
        let inverse = x87(ONE as f64 / value64);
        let scaled = x87(inverse * reciprocal);
        let offset = x87(linear - scaled);
        let product = x87(inverse * offset);
        x87(ONE as f64 / product)
    }
}

/// Merges two four-value blocks into the first block.
pub fn combine(target: &mut BlendTerms, source: BlendTerms, input: f32) {
    let total_weight = sse_add(target.weight, source.weight);
    let denominator = sse_add(source.scale, target.scale);
    let ratio = sse_div(denominator, target.scale);
    let source_weighted = sse_mul(source.value, source.weight);
    let target_weighted = sse_mul(target.weight, target.value);
    let weighted_sum = sse_add(source_weighted, target_weighted);
    let mut factor = sse_div(weighted_sum, total_weight);
    factor = sse_div(factor, denominator);
    factor = sse_mul(factor, total_weight);
    factor = sse_mul(factor, input);

    // x87 order: (ratio * ratio) / sqrt(ratio) * factor, then store as f32.
    let x87 = round_x87;
    let square = x87(ratio as f64 * ratio as f64);
    let root = x87((ratio as f64).sqrt());
    let shaped = x87(x87(square / root) * factor as f64) as f32;
    target.weight = total_weight;
    target.response = shaped;
    let scaled_weight = sse_mul(total_weight, input);
    target.value = sse_div(sse_mul(target.scale, shaped), scaled_weight);
}

/// Moves the four-value state toward the requested amount.
pub fn relax(state: &mut Relaxation, amount: f32, divisor: f32) {
    if state.remaining > amount {
        let remaining = sse_sub(state.remaining, amount);
        let ratio = sse_div(state.remaining, remaining);
        let fraction = sse_div(ONE, ratio);
        state.remaining = remaining;

        // x87: (fraction * fraction) / sqrt(fraction) * response, stored.
        let x87 = round_x87;
        let f = fraction as f64;
        let shaped = x87(x87(f * f) / x87(f.sqrt()));
        let response = x87(shaped * state.response as f64) as f32;
        state.response = response;
        let numerator = sse_mul(state.scale, response);
        let denominator = sse_mul(remaining, divisor);
        state.accumulated = sse_div(numerator, denominator);
    } else {
        state.remaining = RELAX_FLOOR;
        state.response = RELAX_FLOOR;
        state.accumulated = RELAX_FLOOR;
    }
}

/// Returns the next relaxation amount and updates its state.
/// Its relaxation is the relax code inlined, x87 chain included.
pub fn solve(state: &mut Relaxation, input: f32, limit: f32, divisor: f32) -> f32 {
    if limit.partial_cmp(&state.response) != Some(Ordering::Less) {
        return 0.0;
    }
    let mut ratio = state.response / limit;
    if ratio >= ONE {
        let reciprocal = ONE / ratio;
        ratio = ONE / ((CURVE_LINEAR - reciprocal * CURVE_RECIPROCAL) * reciprocal);
    } else {
        ratio = (CURVE_LINEAR - ratio * CURVE_RECIPROCAL) * ratio;
    }
    let remaining_fraction = ONE - ONE / ratio;
    let threshold = remaining_fraction * state.remaining;
    if input < threshold {
        if input < state.remaining {
            relax(state, input, divisor);
            return input;
        }
        state.remaining = RELAX_FLOOR;
        state.response = RELAX_FLOOR;
        state.accumulated = RELAX_FLOOR;
        return input;
    }
    if threshold > 0.0 {
        relax(state, threshold, divisor);
        return threshold;
    }
    0.0
}

/// Accumulates the input force and updates its response term.
pub fn force(state: &mut ForceTerms, input: f32, divisor: f32, velocity: f32) {
    let accumulated = sse_add(sse_div(input, divisor), state.accumulated_force);
    state.accumulated_force = accumulated;
    state.response = if state.multiplier != 0.0 {
        let ratio = sse_div(state.multiplier, state.divisor);
        let scaled = sse_mul(ratio, accumulated);
        sse_mul(scaled, velocity)
    } else {
        0.0
    };
}

/// Applies fade to the three effective damping values.
pub fn update_forces(fade: FadeState, definition: &Definition, state: &mut State) {
    let amount = fade.amount;
    let bump = (ONE - amount * 0.7) * definition.damp_bump.value;
    state.damp_bump_eff = if bump < MIN_BUMP_DAMPING { MIN_BUMP_DAMPING } else { bump };
    state.damp_rebound_eff = (ONE - amount * 0.3) * definition.damp_rebound.value;
    state.hs_damp_bump_eff = (ONE - amount * 0.8) * definition.hs_damp_bump.value;
}

/// NGP 7.4.778's edits to `Step` (`notes/ngp.md`): the fade's threshold and scale are NGP's per-car words
/// (0x10172ca8 and 0x10172cac in place of 1.02's 27000 at 0x74025c and 0x740258, set by `ngp::layout::car_init`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NgpStep {
    pub fade_threshold: f32,
    pub fade_scale: f32,
}

/// Computes spring, stop, fade and damping outputs. The stops' damping is never limited to
/// the force over the speed (NGP turned the two limiting jumps into no-ops; see the layout).
pub fn step(
    controller: &mut Controller,
    fade: &mut FadeState,
    definition: &Definition,
    state: &mut State,
    input: StepInput,
    ngp: NgpStep,
) -> StepOutput {
    let velocity = input.velocity;
    let mut spring = (definition.spring_free_length.value - (input.travel - definition.helper_min_length.value))
        * definition.spring_rate.value;
    let bump_damping = state.damp_bump_eff;
    let stop_damping = definition.stop_damp.value;
    let bump_rate = definition.bump_stop_rate.value;
    let rebound_damping = state.damp_rebound_eff;
    let high_speed_bump = state.hs_damp_bump_eff;
    let mut breakpoint = 0.0;

    let helper_span =
        (definition.helper_length.value - definition.helper_min_length.value) * definition.helper_rate.value;
    if le_or_unordered(helper_span, spring) {
        controller.mode = 0;
    } else {
        spring = ((definition.spring_rate.value * definition.helper_rate.value)
            / (definition.spring_rate.value + definition.helper_rate.value))
            * ((definition.helper_length.value + definition.spring_free_length.value) - input.travel);
        controller.mode = 1;
    }

    let mut damping = high_speed_bump;
    if le_or_unordered(0.0, velocity) {
        if !le_or_unordered(velocity, definition.hs_break_rebound.value) && definition.hs_break_rebound.value != 0.0 {
            damping = definition.hs_damp_rebound.value;
            breakpoint = (damping - rebound_damping) * definition.hs_break_rebound.value;
            controller.high_speed_hold = 10;
        } else {
            damping = rebound_damping;
            controller.high_speed_hold = controller.high_speed_hold.wrapping_sub(1);
        }
    } else {
        let threshold = 0.0 - definition.hs_break_bump.value;
        if !le_or_unordered(threshold, velocity) && definition.hs_break_bump.value != 0.0 {
            breakpoint = (damping - bump_damping) * threshold;
            controller.high_speed_hold = 10;
        } else {
            damping = bump_damping;
            controller.high_speed_hold = controller.high_speed_hold.wrapping_sub(1);
        }
    }

    let mut stop_force = 0.0;
    let mut stop_coefficient = 0.0;
    if le_or_unordered(definition.bump_stop_pos.value, input.coordinate) {
        if !le_or_unordered(input.coordinate, definition.droop_stop_pos.value) {
            stop_force = (definition.droop_stop_pos.value - input.coordinate) * definition.droop_stop_rate.value;
            if velocity > 0.0 {
                stop_coefficient = stop_damping * 0.3;
                controller.signal.droop_velocity = velocity;
            } else {
                stop_coefficient = stop_damping;
            }
        }
    } else {
        stop_force = (definition.bump_stop_pos.value - input.coordinate) * bump_rate;
        if le_or_unordered(0.0, velocity) {
            stop_coefficient = stop_damping;
        } else {
            stop_coefficient = stop_damping * 0.3;
            controller.signal.bump_velocity = 0.0 - velocity;
        }
    }

    let output = StepOutput {
        spring_stop_force: stop_force + spring,
        breakpoint_offset: breakpoint,
        damping_coefficient: stop_coefficient + damping,
    };

    let nonnegative_stop_force = if stop_force < 0.0 { 0.0 } else { stop_force };
    let total_force = nonnegative_stop_force + (breakpoint - damping * velocity);
    if fade.test_multiplier * total_force > ngp.fade_threshold {
        let scaled = (total_force - ngp.fade_threshold) * ngp.fade_scale;
        let amount = scaled * scaled + fade.amount;
        fade.amount = amount.clamp(0.0, ONE);
        update_forces(*fade, definition, state);
    }

    let abs_velocity = if velocity <= 0.0 { 0.0 - velocity } else { velocity };
    let abs_peak = if controller.signal.peak_velocity <= 0.0 {
        0.0 - controller.signal.peak_velocity
    } else {
        controller.signal.peak_velocity
    };
    if abs_peak < abs_velocity {
        controller.signal.peak_velocity = velocity;
    }
    output
}

/// Returns the old payload for signal dispatch and resets it.
pub fn reset_outputs(controller: &mut Controller, owner_id: u32) -> DamperSignal {
    let emitted = controller.signal;
    controller.signal =
        DamperSignal { owner_id, peak_velocity: 0.0, bump_velocity: 0.0, droop_velocity: 0.0, valid: 1 };
    emitted
}

/// The signal-manager boundary. The local physics port
/// returns the payload to its caller; listener dispatch is owned by the runtime.
pub fn emit_signal(_signal: DamperSignal) {}

/// Clears a tweak value's payload.
pub fn clear_unsigned_value(value: &mut u32) {
    *value = 0;
}

/// Writes the raw bits into effective rebound damping.
pub fn set_rebound_damping_bits(state: &mut State, value: u32) {
    state.damp_rebound_eff = f32::from_bits(value);
}

/// Copies the nine named setup values and their bookkeeping into their definition fields.
pub fn apply_setup(definition: &mut Definition, setup: SetupValues) {
    definition.spring_free_length = setup.spring_free_length;
    definition.spring_rate = setup.spring_rate;
    definition.helper_min_length = setup.helper_min_length;
    definition.helper_length = setup.helper_length;
    definition.helper_rate = setup.helper_rate;
    definition.damp_bump = setup.damp_bump;
    definition.damp_rebound = setup.damp_rebound;
    definition.hs_break_bump = setup.hs_break_bump;
    definition.hs_damp_bump = setup.hs_damp_bump;
}

fn sse_add(a: f32, b: f32) -> f32 {
    round24(a as f64 + b as f64) as f32
}

fn sse_sub(a: f32, b: f32) -> f32 {
    round24(a as f64 - b as f64) as f32
}

fn sse_mul(a: f32, b: f32) -> f32 {
    round24(a as f64 * b as f64) as f32
}

fn sse_div(a: f32, b: f32) -> f32 {
    round24(a as f64 / b as f64) as f32
}

//! Spring-damper game-memory boundary. This file alone knows object offsets.

use super::store::SpringStore;
use super::{
    BlendTerms, ClampParameters, Controller, DamperSignal, Definition, FadeState, ForceTerms, HeatAccumulator,
    HeatCapacity, RealValue, Relaxation, SetupValues, State, UpdateTerms, WeightedValue,
};
use crate::mem::Mem;
use crate::ntweak::layout::{REAL_NAME, REAL_VTABLE as REAL_LABEL, UNSIGNED_NAME, UNSIGNED_VTABLE};
use crate::store::Real;

const DEFINITION_VTABLE: u32 = 0x0074_0124; // 0x52d750
const STATE_VTABLE: u32 = 0x0074_018c; // 0x52da50
const CONTROLLER_VTABLE: u32 = 0x0074_01e8; // 0x52df20
const SCRATCH_VTABLE: u32 = 0x0074_01dc; // 0x52df20
const BASE_SECOND_VTABLE: u32 = 0x0074_0174;

const SETUP_OPTIONS_DICTIONARY: u32 = 0x0165_f16c; // 0x52d710
const DEFINITION_DICTIONARY: u32 = 0x0165_f180; // 0x52d730
const SETUP_DICTIONARY: u32 = 0x0165_f158; // 0x52d9e0

crate::object! {
    /// `CSpringDamper` (the controller; the wheel's strut at `wheel+0xec4`). The size is the end of the last known
    /// field.
    pub struct ControllerObj, size 0x30 {
        vtable @ 0x00: u32,
        definition @ 0x04: u32,
        state @ 0x08: u32,
        scratch_vtable @ 0x0c: u32,
        fade @ 0x10: u32,
        mode @ 0x14: u32,
        high_speed_hold @ 0x18: u32,
        signal_id @ 0x1c: u32,
        signal_peak @ 0x20: f32,
        signal_bump @ 0x24: f32,
        signal_droop @ 0x28: f32,
        signal_valid @ 0x2c: u32,
    }
}

crate::object! {
    /// `CSpringDamper::CState` (rbr-recomp `notes/physics/renames/spring-damper.csv`; the strut's at `wheel+0x670`).
    pub struct StateObj, size 0x3c {
        vtable @ 0x00: u32,
        second_vtable @ 0x04: u32,
        dirty @ 0x08: u32,
        damp_bump_eff @ 0x0c: f32,
        damp_rebound_eff @ 0x10: f32,
        hs_damp_bump_eff @ 0x14: f32,
        owner @ 0x18: u32,
        last_deflection @ 0x1c: Real,
        unsigned_header @ 0x2c: [u32; 3],
        unknown_value @ 0x38: u32,
    }
}

const DEFINITION_BASE: u32 = ControllerObj::definition.at;
const DEFINITION_STATE: u32 = ControllerObj::state.at;
const FADE_POINTER: u32 = ControllerObj::fade.at;
const MODE: u32 = ControllerObj::mode.at;
const HIGH_SPEED_HOLD: u32 = ControllerObj::high_speed_hold.at;
const SIGNAL_ID: u32 = ControllerObj::signal_id.at;
const SIGNAL_PEAK: u32 = ControllerObj::signal_peak.at;
const SIGNAL_BUMP: u32 = ControllerObj::signal_bump.at;
const SIGNAL_DROOP: u32 = ControllerObj::signal_droop.at;
const SIGNAL_VALID: u32 = ControllerObj::signal_valid.at;

/// `CSpringDamper::CDef` members (rbr-recomp `notes/physics/renames/spring-damper.csv`). The reals are CReal headers;
/// each value is at +0xc.
const DEFINITION_DEBUG_ID: u32 = 0x18;
const DEFINITION_BUMP_STOP_POS: u32 = 0x1c;
const DEFINITION_DROOP_STOP_POS: u32 = 0x2c;
const DEFINITION_SPRING_FREE_LENGTH: u32 = 0x3c;
const DEFINITION_SPRING_RATE: u32 = 0x4c;
const DEFINITION_HELPER_MIN_LENGTH: u32 = 0x5c;
const DEFINITION_HELPER_LENGTH: u32 = 0x6c;
const DEFINITION_HELPER_RATE: u32 = 0x7c;
const DEFINITION_BUMP_STOP_RATE: u32 = 0x8c;
const DEFINITION_DROOP_STOP_RATE: u32 = 0x9c;
const DEFINITION_DAMP_BUMP: u32 = 0xac;
const DEFINITION_DAMP_REBOUND: u32 = 0xbc;
const DEFINITION_STOP_DAMP: u32 = 0xcc;
const DEFINITION_HS_BREAK_BUMP: u32 = 0xdc;
const DEFINITION_HS_BREAK_REBOUND: u32 = 0xec;
const DEFINITION_HS_DAMP_BUMP: u32 = 0xfc;
const DEFINITION_HS_DAMP_REBOUND: u32 = 0x10c;

/// `CSpringDamper::CSetup` members (rbr-recomp `notes/physics/renames/spring-damper.csv`), CReal headers as in the
/// definition.
const SETUP_SPRING_FREE_LENGTH: u32 = 0x0c;
const SETUP_SPRING_RATE: u32 = 0x1c;
const SETUP_HELPER_MIN_LENGTH: u32 = 0x2c;
const SETUP_HELPER_LENGTH: u32 = 0x3c;
const SETUP_HELPER_RATE: u32 = 0x4c;
const SETUP_DAMP_BUMP: u32 = 0x5c;
const SETUP_DAMP_REBOUND: u32 = 0x6c;
const SETUP_HS_BREAK_BUMP: u32 = 0x7c;
const SETUP_HS_DAMP_BUMP: u32 = 0x8c;

/// `CSpringDamper::CState` members.
const STATE_DAMP_BUMP_EFF: u32 = StateObj::damp_bump_eff.at;
const STATE_DAMP_REBOUND_EFF: u32 = StateObj::damp_rebound_eff.at;
const STATE_HS_DAMP_BUMP_EFF: u32 = StateObj::hs_damp_bump_eff.at;
const STATE_OWNER: u32 = StateObj::owner.at;
const STATE_LAST_DEFLECTION: u32 = StateObj::last_deflection.at;
const STATE_UNKNOWN: u32 = StateObj::unknown_value.at;

/// Reads a CReal body. The vtable word is left to the constructor and is not copied by setup apply.
fn real_value(mem: &Mem, at: u32) -> RealValue {
    RealValue { label: mem.u32(at), dirty: mem.u32(at + 8), value: mem.f32(at + 12) }
}

/// CSetupOptions::Unk1C returns the setup-options dictionary. 0x52d710.
pub fn setup_options_dictionary() -> u32 {
    SETUP_OPTIONS_DICTIONARY
}

/// CDef::Unk20 returns the definition dictionary. 0x52d730.
pub fn definition_dictionary() -> u32 {
    DEFINITION_DICTIONARY
}

/// CSetup::Unk20 returns the setup dictionary. 0x52d9e0.
pub fn setup_dictionary() -> u32 {
    SETUP_DICTIONARY
}

/// Initializes the complete CDef object, including NTweak headers. 0x52d750.
pub fn construct_definition(mem: &mut Mem, at: u32) -> u32 {
    let value = Definition::new();
    mem.set_u32(at, DEFINITION_VTABLE);
    mem.set_u32(at + 4, BASE_SECOND_VTABLE);
    mem.set_u32(at + 8, 0);
    mem.set_u32(at + 0x0c, UNSIGNED_VTABLE);
    mem.set_u32(at + 0x10, UNSIGNED_NAME);
    mem.set_u32(at + 0x14, 0);
    mem.set_u32(at + DEFINITION_DEBUG_ID, value.debug_id);
    write_real(mem, at + DEFINITION_BUMP_STOP_POS, value.bump_stop_pos, 0);
    write_real(mem, at + DEFINITION_DROOP_STOP_POS, value.droop_stop_pos, 0);
    write_real(mem, at + DEFINITION_SPRING_FREE_LENGTH, value.spring_free_length, 0);
    write_real(mem, at + DEFINITION_SPRING_RATE, value.spring_rate, 0);
    write_real(mem, at + DEFINITION_HELPER_MIN_LENGTH, value.helper_min_length, 0);
    write_real(mem, at + DEFINITION_HELPER_LENGTH, value.helper_length, 0);
    write_real(mem, at + DEFINITION_HELPER_RATE, value.helper_rate, 0);
    write_real(mem, at + DEFINITION_BUMP_STOP_RATE, value.bump_stop_rate, 0);
    write_real(mem, at + DEFINITION_DROOP_STOP_RATE, value.droop_stop_rate, 1);
    write_real(mem, at + DEFINITION_DAMP_BUMP, value.damp_bump, 0);
    write_real(mem, at + DEFINITION_DAMP_REBOUND, value.damp_rebound, 0);
    write_real(mem, at + DEFINITION_STOP_DAMP, value.stop_damp, 0);
    write_real(mem, at + DEFINITION_HS_BREAK_BUMP, value.hs_break_bump, 0);
    write_real(mem, at + DEFINITION_HS_BREAK_REBOUND, value.hs_break_rebound, 0);
    write_real(mem, at + DEFINITION_HS_DAMP_BUMP, value.hs_damp_bump, 0);
    write_real(mem, at + DEFINITION_HS_DAMP_REBOUND, value.hs_damp_rebound, 0);
    at
}

fn write_real(mem: &mut Mem, at: u32, value: RealValue, dirty: u32) {
    mem.set_u32(at, REAL_LABEL);
    mem.set_u32(at + 4, REAL_NAME);
    mem.set_u32(at + 8, dirty);
    mem.set_f32(at + 12, value.value);
}

/// `CSpringDamper::CSetup`'s vtable (0x72e5d4).
const SETUP_VTABLE: u32 = 0x0072_e5d4;

/// `CSpringDamper::CSetup::CSetup` (0x47bc50) on the setup at `at`: an unnamed tweak, its vtable, and the nine reals
/// `CopyStateToDef` (0x52dbd0) copies, from +0x0c, 0x10 apart.
pub fn construct_setup(mem: &mut Mem, at: u32) -> u32 {
    crate::ntweak::construct_base(mem, at, 0);
    mem.set_u32(at, SETUP_VTABLE);
    for real in (0x0c..=0x8c).step_by(0x10) {
        crate::ntweak::construct_real(mem, at + real);
    }
    at
}

/// `CSpringDamper::CSetupOptions`'s vtable (0x72e864).
const OPTIONS_VTABLE: u32 = 0x0072_e864;

/// `CSpringDamper::CSetupOptions::CSetupOptions` (0x47d870) on the options at `at`: an unnamed tweak, its vtable, and
/// eight ranges (`E13RealTriple`s: `Min`, `Max`, `Step`) from +0xc, every real 0x10 apart.
pub fn construct_options(mem: &mut Mem, at: u32) {
    crate::ntweak::construct_base(mem, at, 0);
    mem.set_u32(at, OPTIONS_VTABLE);
    for real in (0x0c..0x18c).step_by(0x10) {
        crate::ntweak::construct_real(mem, at + real);
    }
}

/// Initializes the CState base and its tweak subobjects. 0x52da50.
pub fn construct_state(mem: &mut Mem, at: u32, owner: u32) -> u32 {
    mem.set_u32(at, STATE_VTABLE);
    mem.set_u32(at + 4, BASE_SECOND_VTABLE);
    mem.set_u32(at + 8, 0);
    mem.set_f32(at + STATE_DAMP_BUMP_EFF, 0.0);
    mem.set_f32(at + STATE_DAMP_REBOUND_EFF, 0.0);
    mem.set_f32(at + STATE_HS_DAMP_BUMP_EFF, 0.0);
    mem.set_u32(at + STATE_OWNER, owner);
    write_real(mem, at + STATE_LAST_DEFLECTION, RealValue::default(), 0);
    mem.set_u32(at + 0x2c, UNSIGNED_VTABLE);
    mem.set_u32(at + 0x30, UNSIGNED_NAME);
    mem.set_u32(at + 0x34, 0);
    mem.set_u32(at + STATE_UNKNOWN, 0);
    at
}

/// CState::SetUnk10 writes the raw argument bits at the effective rebound slot. 0x52da40.
pub fn set_rebound_damping<S: SpringStore>(mem: &mut S, at: u32, bits: u32) {
    let mut value = mem.state(at);
    super::set_rebound_damping_bits(&mut value, bits);
    mem.store_rebound_damping(at, value);
}

/// CU32Value::Clear clears its payload word. 0x52d960.
pub fn clear_unsigned_value<S: SpringStore>(mem: &mut S, at: u32) {
    let mut value = mem.unsigned_value(at);
    super::clear_unsigned_value(&mut value);
    mem.store_unsigned_value(at, value);
}

/// Copies a CSetup's nine CReal bodies into the matching CDef fields. 0x52dbd0.
/// Loads only this call's footprint (definition and setup); the owned adoption runs on `Suspension`.
pub fn apply_setup<S: SpringStore>(mem: &mut S, controller_at: u32, setup_at: u32) {
    let mut suspension = super::suspension::Suspension {
        definition: mem.definition(mem.definition_at(controller_at)),
        ..super::suspension::Suspension::default()
    };
    suspension.adopt_setup(mem.setup_values(setup_at));
    suspension.store_definition(mem, controller_at);
}

fn write_real_body(mem: &mut Mem, at: u32, value: RealValue) {
    mem.set_u32(at, value.label);
    mem.set_u32(at + 8, value.dirty);
    mem.set_f32(at + 12, value.value);
}

/// Recomputes the three effective damping values. 0x52dd70.
/// Loads and stores only this call's footprint (definition, state effects, fade).
pub fn update_forces<S: SpringStore>(mem: &mut S, controller_at: u32) {
    let state_at = mem.state_at(controller_at);
    let mut suspension = super::suspension::Suspension {
        definition: mem.definition(mem.definition_at(controller_at)),
        fade: mem.fade_state(mem.fade_at(controller_at)),
        damping: super::suspension::Damping::from_state(mem.state(state_at)),
        ..super::suspension::Suspension::default()
    };
    suspension.update_damping();
    mem.store_effective_damping(state_at, suspension.damping.state());
}

/// Runs one typed spring-damper step and stores its outputs and changed state. 0x52e090.
/// NGP turned the two jumps that limited the stops' damping to the force over the
/// speed (at 0x52e22b and 0x52e279) into no-ops, so the port never limits it.
pub fn step<S: SpringStore>(mem: &mut S, arguments: &[u32], ngp: super::NgpStep) {
    let [controller_at, travel, coordinate, velocity, spring_force, breakpoint, damping, ..] = arguments else {
        panic!("CSpringDamper::Step expects seven recorded arguments");
    };
    let definition_at = mem.definition_at(*controller_at);
    let state_at = mem.state_at(*controller_at);
    let fade_at = mem.fade_at(*controller_at);
    let mut controller = mem.controller(*controller_at);
    let mut fade = mem.fade_state(fade_at);
    let mut state = mem.state(state_at);
    let output = super::step(
        &mut controller,
        &mut fade,
        &mem.definition(definition_at),
        &mut state,
        super::StepInput {
            travel: f32::from_bits(*travel),
            coordinate: f32::from_bits(*coordinate),
            velocity: f32::from_bits(*velocity),
        },
        ngp,
    );

    // The three outputs go to the caller's out pointers.
    mem.set_f32(*spring_force, output.spring_stop_force);
    mem.set_f32(*breakpoint, output.breakpoint_offset);
    mem.set_f32(*damping, output.damping_coefficient);
    mem.store_fade_state(fade_at, fade);
    mem.store_effective_damping(state_at, state);
    mem.store_step_state(*controller_at, controller);
}

/// Resets the signal after handing its previous payload to the runtime. 0x52e010.
/// Loads and stores only this call's footprint (controller, owner link, signal words). Returns the emitted payload
/// for the host's typed damper event.
pub fn reset_outputs<S: SpringStore>(mem: &mut S, controller_at: u32) -> DamperSignal {
    let mut suspension = super::suspension::Suspension {
        controller: mem.controller(controller_at),
        ..super::suspension::Suspension::default()
    };
    let owner_id = mem.owner_id(mem.state_at(controller_at));
    let emitted = suspension.reset_signal(owner_id);
    super::emit_signal(emitted);
    mem.store_signal(controller_at, suspension.controller.signal);
    emitted
}

/// CSpringDamper::CSpringDamper initializes its pointers, scratch header, and counters. 0x52df20.
pub fn construct_controller(mem: &mut Mem, at: u32, definition: u32, state: u32) -> u32 {
    mem.set_u32(at, CONTROLLER_VTABLE);
    mem.set_u32(at + DEFINITION_BASE, definition);
    mem.set_u32(at + DEFINITION_STATE, state);
    mem.set_u32(at + 0x0c, SCRATCH_VTABLE);
    mem.set_u32(at + FADE_POINTER, 0);
    mem.set_u32(at + MODE, 0);
    mem.set_u32(at + HIGH_SPEED_HOLD, 0);
    at
}

/// CDamperSignal::Emit is an external signal-manager call with no spring-owned memory effects.
pub fn emit_signal() {}

fn store_pair(mem: &mut Mem, at: u32, values: [f32; 2]) {
    mem.set_f32(at, values[0]);
    mem.set_f32(at + 4, values[1]);
}

/// Runs DamperAccumA (heat accumulation) on its capacity pair and running temperature. 0x52e3a0.
pub fn accumulate_heat<S: SpringStore>(mem: &mut S, capacity_at: u32, heat: f32, temperature_at: u32) {
    let capacity = mem.load_heat_capacity(capacity_at);
    let mut temperature = mem.f32(temperature_at);
    super::accumulate_heat(capacity, heat, &mut temperature);
    mem.set_f32(temperature_at, temperature);
}

/// Runs DamperAccumB (heat accumulation through the shared scale word) on its scale word
/// and per-block accumulator. 0x52e3d0.
pub fn accumulate_heat_scaled<S: SpringStore>(mem: &mut S, scale_at: u32, heat: f32, cell_at: u32) {
    let mut cell = mem.load_heat_accumulator(cell_at);
    super::accumulate_heat_scaled(mem.f32(scale_at), heat, &mut cell);
    mem.store_heat_accumulator(cell_at, cell);
}

/// Runs DamperClamp on typed transfer values. 0x52e400.
pub fn clamp_value<S: SpringStore>(mem: &mut S, parameters_at: u32, target_at: u32) -> f32 {
    let parameters = mem.load_clamp_parameters(parameters_at);
    let mut target = mem.load_weighted(target_at);
    let result = super::clamp_value(parameters, &mut target);
    mem.store_weighted(target_at, target);
    result
}

/// Runs DamperMerge on typed weighted values. 0x52e460.
pub fn merge_value<S: SpringStore>(mem: &mut S, source_at: u32, target_at: u32) {
    let source = mem.load_weighted(source_at);
    let mut target = mem.load_weighted(target_at);
    super::merge_value(source, &mut target);
    mem.store_weighted(target_at, target);
}

/// Runs DamperUpdateA (solves the response term) on its four-value terms. 0x52e4c0.
pub fn solve_response<S: SpringStore>(mem: &mut S, terms_at: u32, gas_constant: f32) {
    let mut terms = mem.load_update_terms(terms_at);
    super::solve_response(&mut terms, gas_constant);
    mem.store_update_terms(terms_at, terms);
}

/// Runs DamperUpdateB (solves the scale term) on its four-value terms. 0x52e4f0.
pub fn solve_scale<S: SpringStore>(mem: &mut S, terms_at: u32, gas_constant: f32) {
    let mut terms = mem.load_update_terms(terms_at);
    super::solve_scale(&mut terms, gas_constant);
    mem.store_update_terms(terms_at, terms);
}

/// Runs DamperUpdateC (solves the numerator term) on its four-value terms. 0x52e530.
pub fn solve_numerator<S: SpringStore>(mem: &mut S, terms_at: u32, gas_constant: f32) {
    let mut terms = mem.load_update_terms(terms_at);
    super::solve_numerator(&mut terms, gas_constant);
    mem.store_update_terms(terms_at, terms);
}

/// Runs DamperCombine on its target and source blocks. 0x52e630.
pub fn combine<S: SpringStore>(mem: &mut S, target_at: u32, source_at: u32, input: f32) {
    let mut target = mem.load_blend_terms(target_at);
    let source = mem.load_blend_terms(source_at);
    super::combine(&mut target, source, input);
    mem.store_blend_terms(target_at, target);
}

/// Runs DamperRelax on its four-value state. 0x52e6d0.
pub fn relax<S: SpringStore>(mem: &mut S, state_at: u32, amount: f32, divisor: f32) {
    let mut value = mem.load_relaxation(state_at);
    super::relax(&mut value, amount, divisor);
    mem.store_relaxation(state_at, value);
}

/// Runs DamperSolve on its four-value state. 0x52e760.
pub fn solve<S: SpringStore>(mem: &mut S, state_at: u32, input: f32, limit: f32, divisor: f32) -> f32 {
    let mut value = mem.load_relaxation(state_at);
    let result = super::solve(&mut value, input, limit, divisor);
    mem.store_relaxation(state_at, value);
    result
}

/// Runs DamperForce on its four-value accumulator. 0x52e8a0.
pub fn force<S: SpringStore>(mem: &mut S, state_at: u32, input: f32, divisor: f32, velocity: f32) {
    let mut value = mem.load_force_terms(state_at);
    super::force(&mut value, input, divisor, velocity);
    mem.store_force_terms(state_at, value);
}

/// The Mem store: the bodies are the layout's reads and writes.
impl SpringStore for Mem {
    fn definition_at(&self, controller: u32) -> u32 {
        self.u32(controller + DEFINITION_BASE)
    }

    fn state_at(&self, controller: u32) -> u32 {
        self.u32(controller + DEFINITION_STATE)
    }

    fn fade_at(&self, controller: u32) -> u32 {
        self.u32(controller + FADE_POINTER)
    }

    /// Reads the fields needed by the reached spring-damper functions.
    fn definition(&self, at: u32) -> Definition {
        Definition {
            debug_id: self.u32(at + DEFINITION_DEBUG_ID),
            bump_stop_pos: real_value(self, at + DEFINITION_BUMP_STOP_POS),
            droop_stop_pos: real_value(self, at + DEFINITION_DROOP_STOP_POS),
            spring_free_length: real_value(self, at + DEFINITION_SPRING_FREE_LENGTH),
            spring_rate: real_value(self, at + DEFINITION_SPRING_RATE),
            helper_min_length: real_value(self, at + DEFINITION_HELPER_MIN_LENGTH),
            helper_length: real_value(self, at + DEFINITION_HELPER_LENGTH),
            helper_rate: real_value(self, at + DEFINITION_HELPER_RATE),
            bump_stop_rate: real_value(self, at + DEFINITION_BUMP_STOP_RATE),
            droop_stop_rate: real_value(self, at + DEFINITION_DROOP_STOP_RATE),
            damp_bump: real_value(self, at + DEFINITION_DAMP_BUMP),
            damp_rebound: real_value(self, at + DEFINITION_DAMP_REBOUND),
            stop_damp: real_value(self, at + DEFINITION_STOP_DAMP),
            hs_break_bump: real_value(self, at + DEFINITION_HS_BREAK_BUMP),
            hs_break_rebound: real_value(self, at + DEFINITION_HS_BREAK_REBOUND),
            hs_damp_bump: real_value(self, at + DEFINITION_HS_DAMP_BUMP),
            hs_damp_rebound: real_value(self, at + DEFINITION_HS_DAMP_REBOUND),
        }
    }

    /// Reads the nine setup values that CopyStateToDef adopts.
    fn setup_values(&self, at: u32) -> SetupValues {
        SetupValues {
            spring_free_length: real_value(self, at + SETUP_SPRING_FREE_LENGTH),
            spring_rate: real_value(self, at + SETUP_SPRING_RATE),
            helper_min_length: real_value(self, at + SETUP_HELPER_MIN_LENGTH),
            helper_length: real_value(self, at + SETUP_HELPER_LENGTH),
            helper_rate: real_value(self, at + SETUP_HELPER_RATE),
            damp_bump: real_value(self, at + SETUP_DAMP_BUMP),
            damp_rebound: real_value(self, at + SETUP_DAMP_REBOUND),
            hs_break_bump: real_value(self, at + SETUP_HS_BREAK_BUMP),
            hs_damp_bump: real_value(self, at + SETUP_HS_DAMP_BUMP),
        }
    }

    fn store_setup_values(&mut self, definition_at: u32, definition: &Definition) {
        // CopyStateToDef copies each CReal's label, dirty word, and value, in this order.
        write_real_body(self, definition_at + DEFINITION_SPRING_FREE_LENGTH, definition.spring_free_length);
        write_real_body(self, definition_at + DEFINITION_SPRING_RATE, definition.spring_rate);
        write_real_body(self, definition_at + DEFINITION_HELPER_MIN_LENGTH, definition.helper_min_length);
        write_real_body(self, definition_at + DEFINITION_HELPER_LENGTH, definition.helper_length);
        write_real_body(self, definition_at + DEFINITION_HELPER_RATE, definition.helper_rate);
        write_real_body(self, definition_at + DEFINITION_DAMP_BUMP, definition.damp_bump);
        write_real_body(self, definition_at + DEFINITION_DAMP_REBOUND, definition.damp_rebound);
        write_real_body(self, definition_at + DEFINITION_HS_BREAK_BUMP, definition.hs_break_bump);
        write_real_body(self, definition_at + DEFINITION_HS_DAMP_BUMP, definition.hs_damp_bump);
    }

    fn state(&self, at: u32) -> State {
        State {
            damp_bump_eff: self.f32(at + STATE_DAMP_BUMP_EFF),
            damp_rebound_eff: self.f32(at + STATE_DAMP_REBOUND_EFF),
            hs_damp_bump_eff: self.f32(at + STATE_HS_DAMP_BUMP_EFF),
            owner: self.u32(at + STATE_OWNER),
            last_deflection: real_value(self, at + STATE_LAST_DEFLECTION),
            unknown_value: self.u32(at + STATE_UNKNOWN),
        }
    }

    fn store_effective_damping(&mut self, at: u32, value: State) {
        self.set_f32(at + STATE_DAMP_BUMP_EFF, value.damp_bump_eff);
        self.set_f32(at + STATE_DAMP_REBOUND_EFF, value.damp_rebound_eff);
        self.set_f32(at + STATE_HS_DAMP_BUMP_EFF, value.hs_damp_bump_eff);
    }

    fn store_rebound_damping(&mut self, at: u32, value: State) {
        self.set_f32(at + STATE_DAMP_REBOUND_EFF, value.damp_rebound_eff);
    }

    fn owner_id(&self, state_at: u32) -> u32 {
        let owner = self.u32(state_at + STATE_OWNER);
        self.u32(owner + 0xeb4)
    }

    fn controller(&self, at: u32) -> Controller {
        Controller {
            mode: self.u32(at + MODE) as i32,
            high_speed_hold: self.u32(at + HIGH_SPEED_HOLD) as i32,
            signal: DamperSignal {
                owner_id: self.u32(at + SIGNAL_ID),
                peak_velocity: self.f32(at + SIGNAL_PEAK),
                bump_velocity: self.f32(at + SIGNAL_BUMP),
                droop_velocity: self.f32(at + SIGNAL_DROOP),
                valid: self.u32(at + SIGNAL_VALID) as i32,
            },
        }
    }

    fn store_step_state(&mut self, at: u32, value: Controller) {
        self.set_u32(at + MODE, value.mode as u32);
        self.set_u32(at + HIGH_SPEED_HOLD, value.high_speed_hold as u32);
        self.set_f32(at + SIGNAL_PEAK, value.signal.peak_velocity);
        self.set_f32(at + SIGNAL_BUMP, value.signal.bump_velocity);
        self.set_f32(at + SIGNAL_DROOP, value.signal.droop_velocity);
    }

    fn store_signal(&mut self, at: u32, value: DamperSignal) {
        self.set_u32(at + SIGNAL_ID, value.owner_id);
        self.set_f32(at + SIGNAL_PEAK, value.peak_velocity);
        self.set_f32(at + SIGNAL_BUMP, value.bump_velocity);
        self.set_f32(at + SIGNAL_DROOP, value.droop_velocity);
        self.set_u32(at + SIGNAL_VALID, value.valid as u32);
    }

    fn fade_state(&self, at: u32) -> FadeState {
        FadeState { test_multiplier: self.f32(at), amount: self.f32(at + 4) }
    }

    fn store_fade_state(&mut self, at: u32, value: FadeState) {
        self.set_f32(at, value.test_multiplier);
        self.set_f32(at + 4, value.amount);
    }

    fn unsigned_value(&self, at: u32) -> u32 {
        self.u32(at + 4)
    }

    fn store_unsigned_value(&mut self, at: u32, value: u32) {
        self.set_u32(at + 4, value);
    }

    fn load_update_terms(&self, at: u32) -> UpdateTerms {
        UpdateTerms {
            scale: self.f32(at),
            numerator: self.f32(at + 4),
            denominator: self.f32(at + 8),
            response: self.f32(at + 12),
        }
    }

    fn store_update_terms(&mut self, at: u32, terms: UpdateTerms) {
        self.set_f32(at, terms.scale);
        self.set_f32(at + 4, terms.numerator);
        self.set_f32(at + 8, terms.denominator);
        self.set_f32(at + 12, terms.response);
    }

    fn load_blend_terms(&self, at: u32) -> BlendTerms {
        BlendTerms {
            weight: self.f32(at),
            value: self.f32(at + 4),
            response: self.f32(at + 8),
            scale: self.f32(at + 12),
        }
    }

    fn store_blend_terms(&mut self, at: u32, value: BlendTerms) {
        self.set_f32(at, value.weight);
        self.set_f32(at + 4, value.value);
        self.set_f32(at + 8, value.response);
        self.set_f32(at + 12, value.scale);
    }

    fn load_relaxation(&self, at: u32) -> Relaxation {
        Relaxation {
            remaining: self.f32(at),
            accumulated: self.f32(at + 4),
            response: self.f32(at + 8),
            scale: self.f32(at + 12),
        }
    }

    fn store_relaxation(&mut self, at: u32, value: Relaxation) {
        self.set_f32(at, value.remaining);
        self.set_f32(at + 4, value.accumulated);
        self.set_f32(at + 8, value.response);
        self.set_f32(at + 12, value.scale);
    }

    fn load_force_terms(&self, at: u32) -> ForceTerms {
        ForceTerms {
            multiplier: self.f32(at),
            accumulated_force: self.f32(at + 4),
            response: self.f32(at + 8),
            divisor: self.f32(at + 12),
        }
    }

    fn store_force_terms(&mut self, at: u32, value: ForceTerms) {
        self.set_f32(at, value.multiplier);
        self.set_f32(at + 4, value.accumulated_force);
        self.set_f32(at + 8, value.response);
        self.set_f32(at + 12, value.divisor);
    }

    fn load_heat_capacity(&self, at: u32) -> HeatCapacity {
        HeatCapacity { mass: self.f32(at), specific_heat: self.f32(at + 4) }
    }

    fn load_heat_accumulator(&self, at: u32) -> HeatAccumulator {
        HeatAccumulator { divisor: self.f32(at), temperature: self.f32(at + 4) }
    }

    fn store_heat_accumulator(&mut self, at: u32, value: HeatAccumulator) {
        store_pair(self, at, [value.divisor, value.temperature]);
    }

    fn load_clamp_parameters(&self, at: u32) -> ClampParameters {
        let values: [f32; 4] = self.f32s(at);
        ClampParameters {
            transfer_scale: values[0],
            transfer_divisor: values[1],
            reserved: values[2],
            maximum_value: values[3],
        }
    }

    fn load_weighted(&self, at: u32) -> WeightedValue {
        WeightedValue { weight: self.f32(at), value: self.f32(at + 4) }
    }

    fn store_weighted(&mut self, at: u32, value: WeightedValue) {
        store_pair(self, at, [value.weight, value.value]);
    }
}

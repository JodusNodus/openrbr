//! The loads and stores of each tyre kernel call (`ComputeSlip`, `Sign`, `Accumulate`, the contact solve,
//! `Friction`, `HeatStep`, `TableStep`, `RelaxStep`, `ComputeOutputs`, `StepInner`, `Step`, `BlendTable`), and the
//! small tyre calls (the tail getters, the noise signal).

use crate::math::vec::Vec4;
use crate::mem::Mem;

use super::{
    ContactAxes, ContactFrame, Indentation, DEF, DEF_CARCASS_RADIUS, DEF_CARCASS_STIFFNESS_LATERAL,
    DEF_CARCASS_STIFFNESS_TORSIONAL, DEF_CARCASS_WIDTH, DEF_DEBUG_ID, DEF_REFERENCE_FRICTION, DEF_TREAD_COVERAGE,
    DEF_TREAD_DEPTH, DEF_TREAD_WIDTH, LONG_CONTACT_VEL, SLIP_MATRIX, SLIP_RATIO_TERM, SLIP_X, SPEED, SPEED_SQ, SPIN,
    STATE, STATE_EFFECTIVE_RADIUS, STATE_LERP_WEIGHT, STATE_TYRE_TEMPERATURE,
};
use crate::physics_core::layout::SignalHost;
use crate::spring_damper::{ForceTerms, HeatCapacity};
use crate::tyre::forces::RollingInput;
use crate::tyre::friction_limits::{BlendPair, FrictionLimits, LimitsInput};
use crate::tyre::heat::{HeatInput, HeatOutput};
use crate::tyre::hold::HoldInput;
use crate::tyre::material::Material;
use crate::tyre::output_block::OutputBlock;
use crate::tyre::patch_forces::{PatchForces, PatchInput};
use crate::tyre::relax::{RelaxInput, RelaxOutput, SectorInput, SectorOutput};
use crate::tyre::slip::Slip;
use crate::tyre::soil::{SoilDrag, SoilInput};
use crate::tyre::solve::{Coefficients, Contact};
use crate::tyre::store::TyreStore;
use crate::tyre::surface::{SurfaceFriction, SurfaceInput};

use super::frame::{store_floats, Table, WORDS};
use super::frame_types::{Frame, Scratch, Segment, Setup, StateWords};
use crate::{ntweak, physics_core};

/// Loads the two factors of a heat-capacity block at `at`.
fn load_factors(mem: &Mem, at: u32) -> HeatCapacity {
    HeatCapacity { mass: mem.f32(at), specific_heat: mem.f32(at + 4) }
}

/// Loads `DamperForce`'s four words at `at`: the multiplier, the accumulated force, the response and the divisor.
fn load_force_terms(mem: &Mem, at: u32) -> ForceTerms {
    ForceTerms {
        multiplier: mem.f32(at),
        accumulated_force: mem.f32(at + 4),
        response: mem.f32(at + 8),
        divisor: mem.f32(at + 12),
    }
}

/// Stores `DamperForce`'s four words at `at` (see `load_force_terms`).
fn store_force_terms(mem: &mut Mem, at: u32, t: ForceTerms) {
    mem.set_f32(at, t.multiplier);
    mem.set_f32(at + 4, t.accumulated_force);
    mem.set_f32(at + 8, t.response);
    mem.set_f32(at + 12, t.divisor);
}

/// `v * s + base` over the four lanes (the asm's `mulps` by `s`, then `addps` to `base`).
pub fn scaled_sum(v: Vec4, s: f32, base: Vec4) -> Vec4 {
    Vec4 { x: v.x * s + base.x, y: v.y * s + base.y, z: v.z * s + base.z, w: v.w * s + base.w }
}

/// The address of word `index` of the table at `base`. The indices are Locate's `lo`, which is -1 (0xffffffff) when a NaN
/// input walks below the table, so the address wraps as the original's 32-bit one does.
pub fn word_at(base: u32, index: u32) -> u32 {
    base.wrapping_add(index.wrapping_mul(4))
}

/// The sector the relaxation steps: `m_state+0x1c & 7`.
/// The solve coefficients from their eight words.
fn coefficients_from_words(w: [f32; 8]) -> Coefficients {
    Coefficients {
        quadratic: w[0],
        linear: w[1],
        tread_stiffness: w[2],
        sink_factor: w[3],
        tread_area: w[4],
        groove_area: w[5],
        groove_soil_stiffness: w[6],
        tread_soil_stiffness: w[7],
    }
}

fn relax_index(mem: &Mem, state: u32) -> u32 {
    mem.u32(state + 0x1C) & 7
}

/// The blend the friction limits and the patch forces share: the weight at `m_state+0xa4`, and the four setup tables
/// (`+0x244`, `+0x264`, `+0x284`, `+0x2a4`) at the state's indices `m_state+0x9c` and `+0xa0`.
fn load_blend(mem: &Mem, this: u32) -> (f32, [BlendPair; 4]) {
    let state = mem.u32(this + STATE);
    let def = mem.u32(this + DEF);
    let (a, b) = (mem.u32(state + 0x9C), mem.u32(state + 0xA0));
    let pair = |base: u32| BlendPair { at_a: mem.f32(word_at(def + base, a)), at_b: mem.f32(word_at(def + base, b)) };
    (mem.f32(state + 0xA4), [pair(0x244), pair(0x264), pair(0x284), pair(0x2A4)])
}

/// The stack words of the tyre step as typed values, loaded once at entry. `torque_in` is the caller's torque vector,
/// which the forces add to.
#[derive(Clone, Copy, Debug)]
pub struct StepArgs {
    /// The wheel's spin, rad/s, and the time step, s.
    pub spin: f32,
    pub dt: f32,
    /// The height of the hub above the surface, m, and the scale of the slipped area.
    pub hub_height: f32,
    pub slip_area_scale: f32,
    pub material: Material,
    pub debug_words: [u32; 3],
    /// The game's debug flag that copies the debug words into the scratch.
    pub copy_debug_words: bool,
    /// The surface normal and the wheel's axle direction.
    pub normal: Vec4,
    pub axle: Vec4,
    /// The velocity and the position of the contact point.
    pub velocity: Vec4,
    pub contact_point: Vec4,
    pub torque_in: Vec4,
}

/// Where the tyre step stores its outputs.
#[derive(Clone, Copy, Debug)]
pub struct StepPtrs {
    pub force: u32,
    pub torque: u32,
    pub reaction_torque: u32,
    pub sticking_load: u32,
    pub output_block: u32,
}

/// The outputs of the tyre step. The no-contact path sets the force, the torque and the reaction torque to zero and
/// leaves the sticking load and the output block as they were, so those are `None` there.
#[derive(Clone, Copy, Debug)]
pub struct StepOut {
    /// The force and the torque on the wheel.
    pub force: Vec4,
    pub torque: Vec4,
    /// The rolling resistance torque plus the torsion torque, N·m.
    pub reaction_torque: f32,
    /// The normal force scaled by 0.025 and the sticking weight (named by use).
    pub sticking_load: Option<f32>,
    pub block: Option<OutputBlock>,
}

/// `CTyre::CScratch::EmitNoise` (0x535890, thiscall, no arguments): emits the tyre-noise signal with the scratch's signal
/// block (`m_sig`, +0x420), then clears the block's second word (+0x424).
pub fn emit_noise<S: TyreStore>(mem: &mut S, host: &mut impl SignalHost, this: u32) {
    let signal = mem.noise_signal();
    let sig = this + 0x420;
    physics_core::layout::emit(mem.mem_mut(), host, signal, sig);
    mem.set_u32(sig + 4, 0);
}

/// `CU32Value8::Clear` (0x5317f0, thiscall): the eight floats at `this` become 0.0.
pub fn value8_clear<S: TyreStore>(mem: &mut S, this: u32) {
    for k in 0..8 {
        mem.set_f32(this + 4 * k, 0.0);
    }
}

/// The Mem store: the bodies are the layout's reads and writes.
impl TyreStore for Mem {
    /// Loads a [`Segment`] at `at`: `lo`, `hi` and `frac` in three words.
    fn load_segment(&self, at: u32) -> Segment {
        let mem = self;
        Segment { lo: mem.u32(at), hi: mem.u32(at + 4), frac: mem.f32(at + 8) }
    }

    /// Stores a [`Segment`] at `at`.
    fn store_segment(&mut self, at: u32, s: &Segment) {
        let mem = self;
        mem.set_u32(at, s.lo);
        mem.set_u32(at + 4, s.hi);
        mem.set_f32(at + 8, s.frac);
    }

    /// The table of `len` entries at `at`, with the word below it.
    fn read_table(&self, at: u32, len: usize) -> Table {
        let mem = self;
        Table {
            below: mem.f32(at - 4),
            len,
            entries: std::array::from_fn(|k| if k < len { mem.f32(at + 4 * k as u32) } else { 0.0 }),
        }
    }

    /// Loads the whole tyre step footprint of the scratch at `this`: its state, its setup, its wear table and the
    /// contact record's angle. The pointers are read from `this` (`+0x10` setup, `+0x14` state, `+0x410` record,
    /// `+0x414` wear table).
    fn load_frame(&self, this: u32) -> Frame {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let record = mem.u32(this + 0x410);
        let wear = mem.u32(this + 0x414);
        let f = |off: u32| mem.f32(this + off);
        let scratch = Scratch {
            contact_point: Vec4::load(mem, this + 0x160),
            contact_frame: ContactFrame {
                lateral: Vec4::load(mem, this + SLIP_MATRIX),
                rolling: Vec4::load(mem, this + SLIP_MATRIX + 0x10),
                normal: Vec4::load(mem, this + SLIP_MATRIX + 0x20),
            },
            material: mem.load_material(this + 0x1A0),
            static_friction_dirty: mem.u32(this + 0x1E4),
            static_friction: f(0x1E8),
            kinetic_friction_dirty: mem.u32(this + 0x1F4),
            kinetic_friction: f(0x1F8),
            static_friction_soil: f(0x208),
            kinetic_friction_soil: f(0x218),
            sink_factor: f(0x228),
            soil_thickness: f(0x258),
            water_factor: f(0x288),
            rolling_resistance: f(0x298),
            lateral_deflection_ratio: f(0x300),
            camber_radius: f(0x304),
            tread_contact_area: f(0x308),
            patch_length: f(0x30C),
            contact_pressure: f(0x310),
            normal_force: f(0x314),
            carcass_force: f(0x318),
            tread_force: f(0x31C),
            total_force_ratio: f(0x324),
            tread_force_share: f(0x328),
            indentation: Indentation::from_words(mem.f32s(this + 0x32C)),
            soil_drag: f(0x348),
            lateral_soil_force: f(0x34C),
            longitudinal_soil_force: f(0x350),
            lateral_soil_flow: f(0x354),
            soil_flow_aligned: f(0x358),
            soil_flow_opposed: f(0x35C),
            speed: f(SPEED),
            speed_sq: f(SPEED_SQ),
            contact_velocity: mem.f32s(this + SLIP_X),
            long_slip_velocity: f(LONG_CONTACT_VEL),
            spin: f(SPIN),
            spin_slip: f(SLIP_RATIO_TERM),
            low_speed_a: f(0x380),
            low_speed_b: f(0x384),
            slip_phase: f(0x388),
            cornering_stiffness: f(0x38C),
            traction_stiffness: f(0x390),
            lateral_stiffness: f(0x394),
            torsional_stiffness: f(0x398),
            lateral_force: f(0x39C),
            longitudinal_force: f(0x3A0),
            torsion_torque: f(0x3A4),
            hub_height: f(0x3A8),
            inclination_sin: f(0x3AC),
            inclination_cos: f(0x3B0),
            stiffness_factor: f(0x3B4),
            pneumatic_trail: f(0x3B8),
            tread_remaining: f(0x3BC),
            sector: mem.u32(this + 0x3C0),
            wet_term: f(0x3C4),
            rolling_torque: f(0x3C8),
            extra_force: f(0x3CC),
            extra_axle_torque: f(0x3D0),
            extra_term: f(0x3D4),
            slip_area_scale: f(0x3D8),
            sliding_weight: f(0x3DC),
            lateral_phase: f(0x3E0),
            longitudinal_phase: f(0x3E4),
            sliding_deflection: f(0x3E8),
            sliding_torsion: f(0x3EC),
            sticking_weight: f(0x3F0),
            stick_phase_a: f(0x3F4),
            stick_phase_b: f(0x3F8),
            sticking_deflection: f(0x3FC),
            sticking_torsion: f(0x400),
            abrasion_rate: f(0x404),
            contact_angle: mem.f32(record + 0x5AC),
            wear: mem.f32s(wear),
            signal: std::array::from_fn(|k| mem.u32(this + 0x420 + 4 * k as u32)),
            in_contact: mem.u32(this + 0x480),
            sector_copy: mem.u32(this + 0x484),
            contact_point_copy: Vec4::load(mem, this + 0x490),
            motion_copy: Vec4::load(mem, this + 0x4A0),
            soil_flow_copy: mem.u32(this + 0x4B0),
            long_slip_velocity_copy: mem.u32(this + 0x4B4),
            longitudinal_velocity_copy: mem.u32(this + 0x4B8),
            temperature_grip: f(0x4BC),
            lateral_friction_force: f(0x4C0),
            longitudinal_friction_force: f(0x4C4),
            debug_words: std::array::from_fn(|k| mem.u32(this + 0x4C8 + 4 * k as u32)),
            surface_heat_loss: f(0x4E0),
            frictional_power: f(0x4E4),
            wet_damping: f(0x4EC),
            ngp_heat_power: f(0x4E8),
            smoothed_soil_drag: f(0x504),
            smoothed_drag_weight: f(0x508),
            smoothed_furrow_ratio: f(0x50C),
        };
        let st = |off: u32| mem.f32(state + off);
        let entry = |k: u32, word: u32| mem.f32(state + 0xD0 + 12 * k + 4 * word);
        let surface_temperature = std::array::from_fn(|k| entry(k as u32, 0));
        // The entries' second and third words in order; the last third word is the tread temperature.
        let sector_spare = std::array::from_fn(|i| entry(i as u32 / 2, 1 + i as u32 % 2));
        let state_words = StateWords {
            active: mem.u32(state + 0x0C),
            started_dirty: mem.u32(state + 0x28),
            started: mem.u32(state + 0x2C),
            last_point: Vec4::load(mem, state + 0x30),
            lateral_deflection: st(0x40),
            torsion_angle: st(0x44),
            penetration_depth: st(0x48),
            effective_radius: st(STATE_EFFECTIVE_RADIUS),
            vertical_stiffness: st(0x50),
            tread_elasticity: st(0x54),
            tread_coverage: st(STATE_LERP_WEIGHT),
            grip_clamped: mem.u32(state + 0x5C),
            air_pressure: st(0x68),
            tread_engaged: mem.u32(state + 0x70),
            carcass_engaged: mem.u32(state + 0x74),
            hold_direction: Vec4::load(mem, state + 0x80),
            pressure_segment: mem.load_segment(state + 0x90),
            load_segment: mem.load_segment(state + 0x9C),
            surface_temperature_segment: mem.load_segment(state + 0xA8),
            tread_temperature_segment: mem.load_segment(state + 0xB4),
            contact_pressure_segment: mem.load_segment(state + 0xC0),
            surface_temperature,
            sector_spare,
            tread_temperature: entry(7, 2),
            carcass_temperature: st(0x130),
            sector: mem.u32(state + 0x134),
        };
        let setup = Setup {
            debug_id: mem.u32(def + DEF_DEBUG_ID),
            pressure_ratio: mem.f32(def + 0x2C),
            reference_friction: mem.f32(def + DEF_REFERENCE_FRICTION),
            carcass_stiffness_lateral: mem.f32(def + DEF_CARCASS_STIFFNESS_LATERAL),
            carcass_stiffness_torsional: mem.f32(def + DEF_CARCASS_STIFFNESS_TORSIONAL),
            carcass_radius: mem.f32(def + DEF_CARCASS_RADIUS),
            tread_depth: mem.f32(def + DEF_TREAD_DEPTH),
            carcass_thickness: mem.f32(def + 0x9C),
            tread_width: mem.f32(def + DEF_TREAD_WIDTH),
            tread_coverage: mem.f32(def + DEF_TREAD_COVERAGE),
            carcass_width: mem.f32(def + DEF_CARCASS_WIDTH),
            sidewall_height: mem.f32(def + 0xEC),
            wet_wear_factor: mem.f32(def + 0x310),
            inclination_heat_scale: mem.f32(def + 0x10C),
            surface_capacity: mem.f32s(def + 0x110),
            tread_capacity: mem.f32s(def + 0x11C),
            carcass_capacity: mem.f32s(def + 0x128),
            tread_conductivity: mem.f32(def + 0x118),
            pressure_axis: mem.read_table(def + 0x134, 4),
            stiffness_factor_by_pressure: mem.read_table(def + 0x144, 4),
            contact_pressure_axis: mem.read_table(def + 0x154, 6),
            contact_conductance: mem.read_table(def + 0x16C, 6),
            temperature_axis: mem.read_table(def + 0x184, 8),
            rubber_friction_by_temperature: mem.read_table(def + 0x1A4, 8),
            rubber_wear_by_temperature: mem.read_table(def + 0x1C4, 8),
            load_axis: mem.read_table(def + 0x1E4, 8),
            cornering_stiffness_by_load: mem.read_table(def + 0x204, 8),
            traction_stiffness_by_load: mem.read_table(def + 0x224, 8),
            cornering_friction_by_load: mem.read_table(def + 0x244, 8),
            traction_friction_by_load: mem.read_table(def + 0x264, 8),
            cornering_peak_slip_by_load: mem.read_table(def + 0x284, 8),
            traction_peak_slip_by_load: mem.read_table(def + 0x2A4, 8),
        };
        Frame { scratch, state: state_words, setup }
    }

    /// Stores the tyre step footprint back to the scratch at `this` (its state and scratch; the setup and the contact
    /// angle are read only). Every word is written, with the value it was loaded with unless the port changed it.
    fn store_frame(&mut self, this: u32, frame: &Frame) {
        let mem = self;
        let state = mem.u32(this + STATE);
        let wear = mem.u32(this + 0x414);
        let s = &frame.scratch;
        // The material block first: ten of its words are scalars below, which hold the frame's latest values.
        mem.store_material(this + 0x1A0, &s.material);
        mem.set_f32(this + 0x1E8, s.static_friction);
        mem.set_f32(this + 0x1F8, s.kinetic_friction);
        mem.set_f32(this + 0x208, s.static_friction_soil);
        mem.set_f32(this + 0x218, s.kinetic_friction_soil);
        mem.set_f32(this + 0x228, s.sink_factor);
        mem.set_f32(this + 0x258, s.soil_thickness);
        mem.set_f32(this + 0x288, s.water_factor);
        mem.set_f32(this + 0x298, s.rolling_resistance);
        mem.set_f32(this + 0x300, s.lateral_deflection_ratio);
        mem.set_f32(this + 0x304, s.camber_radius);
        mem.set_f32(this + 0x308, s.tread_contact_area);
        mem.set_f32(this + 0x30C, s.patch_length);
        mem.set_f32(this + 0x310, s.contact_pressure);
        mem.set_f32(this + 0x314, s.normal_force);
        mem.set_f32(this + 0x318, s.carcass_force);
        mem.set_f32(this + 0x31C, s.tread_force);
        mem.set_f32(this + 0x324, s.total_force_ratio);
        mem.set_f32(this + 0x328, s.tread_force_share);
        store_floats(mem, this + 0x32C, &s.indentation.to_words());
        mem.set_f32(this + 0x348, s.soil_drag);
        mem.set_f32(this + 0x34C, s.lateral_soil_force);
        mem.set_f32(this + 0x350, s.longitudinal_soil_force);
        mem.set_f32(this + 0x354, s.lateral_soil_flow);
        mem.set_f32(this + 0x358, s.soil_flow_aligned);
        mem.set_f32(this + 0x35C, s.soil_flow_opposed);
        mem.set_f32(this + SPEED, s.speed);
        mem.set_f32(this + SPEED_SQ, s.speed_sq);
        store_floats(mem, this + SLIP_X, &s.contact_velocity);
        mem.set_f32(this + LONG_CONTACT_VEL, s.long_slip_velocity);
        mem.set_f32(this + SPIN, s.spin);
        mem.set_f32(this + SLIP_RATIO_TERM, s.spin_slip);
        mem.set_f32(this + 0x380, s.low_speed_a);
        mem.set_f32(this + 0x384, s.low_speed_b);
        mem.set_f32(this + 0x388, s.slip_phase);
        mem.set_f32(this + 0x38C, s.cornering_stiffness);
        mem.set_f32(this + 0x390, s.traction_stiffness);
        mem.set_f32(this + 0x394, s.lateral_stiffness);
        mem.set_f32(this + 0x398, s.torsional_stiffness);
        mem.set_f32(this + 0x39C, s.lateral_force);
        mem.set_f32(this + 0x3A0, s.longitudinal_force);
        mem.set_f32(this + 0x3A4, s.torsion_torque);
        mem.set_f32(this + 0x3A8, s.hub_height);
        mem.set_f32(this + 0x3AC, s.inclination_sin);
        mem.set_f32(this + 0x3B0, s.inclination_cos);
        mem.set_f32(this + 0x3B4, s.stiffness_factor);
        mem.set_f32(this + 0x3B8, s.pneumatic_trail);
        mem.set_f32(this + 0x3BC, s.tread_remaining);
        mem.set_f32(this + 0x3C4, s.wet_term);
        mem.set_f32(this + 0x3C8, s.rolling_torque);
        mem.set_f32(this + 0x3CC, s.extra_force);
        mem.set_f32(this + 0x3D0, s.extra_axle_torque);
        mem.set_f32(this + 0x3D4, s.extra_term);
        mem.set_f32(this + 0x3D8, s.slip_area_scale);
        mem.set_f32(this + 0x3DC, s.sliding_weight);
        mem.set_f32(this + 0x3E0, s.lateral_phase);
        mem.set_f32(this + 0x3E4, s.longitudinal_phase);
        mem.set_f32(this + 0x3E8, s.sliding_deflection);
        mem.set_f32(this + 0x3EC, s.sliding_torsion);
        mem.set_f32(this + 0x3F0, s.sticking_weight);
        mem.set_f32(this + 0x3F4, s.stick_phase_a);
        mem.set_f32(this + 0x3F8, s.stick_phase_b);
        mem.set_f32(this + 0x3FC, s.sticking_deflection);
        mem.set_f32(this + 0x400, s.sticking_torsion);
        mem.set_f32(this + 0x404, s.abrasion_rate);
        store_floats(mem, wear, &s.wear);
        for (k, word) in s.signal.iter().enumerate() {
            mem.set_u32(this + 0x420 + 4 * k as u32, *word);
        }
        mem.set_u32(this + 0x480, s.in_contact);
        mem.set_u32(this + 0x484, s.sector_copy);
        s.contact_point_copy.store(mem, this + 0x490);
        s.motion_copy.store(mem, this + 0x4A0);
        mem.set_u32(this + 0x4B0, s.soil_flow_copy);
        mem.set_u32(this + 0x4B4, s.long_slip_velocity_copy);
        mem.set_u32(this + 0x4B8, s.longitudinal_velocity_copy);
        mem.set_f32(this + 0x4BC, s.temperature_grip);
        mem.set_f32(this + 0x4C0, s.lateral_friction_force);
        mem.set_f32(this + 0x4C4, s.longitudinal_friction_force);
        for (k, word) in s.debug_words.iter().enumerate() {
            mem.set_u32(this + 0x4C8 + 4 * k as u32, *word);
        }
        mem.set_f32(this + 0x4E0, s.surface_heat_loss);
        mem.set_f32(this + 0x4E4, s.frictional_power);
        mem.set_f32(this + 0x4EC, s.wet_damping);
        mem.set_f32(this + 0x4E8, s.ngp_heat_power);
        mem.set_f32(this + 0x504, s.smoothed_soil_drag);
        mem.set_f32(this + 0x508, s.smoothed_drag_weight);
        mem.set_f32(this + 0x50C, s.smoothed_furrow_ratio);
        s.contact_point.store(mem, this + 0x160);
        s.contact_frame.lateral.store(mem, this + SLIP_MATRIX);
        s.contact_frame.rolling.store(mem, this + SLIP_MATRIX + 0x10);
        s.contact_frame.normal.store(mem, this + SLIP_MATRIX + 0x20);
        mem.set_u32(this + 0x1E4, s.static_friction_dirty);
        mem.set_u32(this + 0x1F4, s.kinetic_friction_dirty);
        mem.set_u32(this + 0x3C0, s.sector);

        let st = &frame.state;
        mem.set_u32(state + 0x0C, st.active);
        mem.set_u32(state + 0x28, st.started_dirty);
        mem.set_u32(state + 0x2C, st.started);
        st.last_point.store(mem, state + 0x30);
        store_floats(
            mem,
            state + 0x40,
            &[
                st.lateral_deflection,
                st.torsion_angle,
                st.penetration_depth,
                st.effective_radius,
                st.vertical_stiffness,
                st.tread_elasticity,
            ],
        );
        mem.set_f32(state + STATE_LERP_WEIGHT, st.tread_coverage);
        mem.set_u32(state + 0x5C, st.grip_clamped);
        mem.set_f32(state + 0x68, st.air_pressure);
        mem.set_u32(state + 0x70, st.tread_engaged);
        mem.set_u32(state + 0x74, st.carcass_engaged);
        st.hold_direction.store(mem, state + 0x80);
        mem.store_segment(state + 0x90, &st.pressure_segment);
        mem.store_segment(state + 0x9C, &st.load_segment);
        mem.store_segment(state + 0xA8, &st.surface_temperature_segment);
        mem.store_segment(state + 0xB4, &st.tread_temperature_segment);
        mem.store_segment(state + 0xC0, &st.contact_pressure_segment);
        for (k, temperature) in st.surface_temperature.iter().enumerate() {
            mem.set_f32(state + 0xD0 + 12 * k as u32, *temperature);
        }
        for (i, word) in st.sector_spare.iter().enumerate() {
            mem.set_f32(state + 0xD0 + 12 * (i as u32 / 2) + 4 + 4 * (i as u32 % 2), *word);
        }
        mem.set_f32(state + 0x12C, st.tread_temperature);
        mem.set_f32(state + 0x130, st.carcass_temperature);
        mem.set_u32(state + 0x134, st.sector);
    }

    /// Loads the copied words of the block at `at`.
    fn load_material(&self, at: u32) -> Material {
        let mem = self;
        std::array::from_fn(|i| mem.u32(at + 4 * WORDS[i]))
    }

    /// Stores the copied words into the block at `at`; the other words are left as they are.
    fn store_material(&mut self, at: u32, material: &Material) {
        let mem = self;
        for (i, word) in material.iter().enumerate() {
            mem.set_u32(at + 4 * WORDS[i], *word);
        }
    }

    /// Loads the effective radius of the tyre at `this`, m.
    fn load_effective_radius(&self, this: u32) -> f32 {
        let mem = self;
        mem.f32(mem.u32(this + STATE) + STATE_EFFECTIVE_RADIUS)
    }

    /// Loads the contact frame's axes (`this+0x170`, rows 0x10 bytes apart).
    fn load_contact_axes(&self, this: u32) -> ContactAxes {
        let mem = self;
        ContactAxes {
            lateral: mem.f32s(this + SLIP_MATRIX),
            rolling: mem.f32s(this + SLIP_MATRIX + 0x10),
            normal: mem.f32s(this + SLIP_MATRIX + 0x20),
        }
    }

    /// Stores the slip.
    fn store_slip(&mut self, this: u32, slip: &Slip) {
        let mem = self;
        for (i, v) in slip.velocity.iter().enumerate() {
            mem.set_f32(this + SLIP_X + 4 * i as u32, *v);
        }
        mem.set_f32(this + SPEED_SQ, slip.speed_sq);
        mem.set_f32(this + SPEED, slip.speed);
        mem.set_f32(this + LONG_CONTACT_VEL, slip.long_slip_velocity);
        mem.set_f32(this + SLIP_RATIO_TERM, slip.spin_slip);
    }

    /// Loads the rolling resistance torque inputs.
    fn load_rolling_input(&self, this: u32) -> RollingInput {
        let mem = self;
        RollingInput {
            patch_length: mem.f32(this + 0x30c),
            rolling_resistance: mem.f32(this + 0x298),
            normal_force: mem.f32(this + 0x314),
            sliding_weight: mem.f32(this + 0x3dc),
            spin: mem.f32(this + SPIN),
        }
    }

    /// Loads the soil drag inputs: the scratch words, the state and the tyre setup (`this+0x10`).
    fn load_soil_input(&self, this: u32) -> SoilInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let f = |off: u32| mem.f32(this + off);
        SoilInput {
            sink_depth: f(0x340),
            carcass_compression: f(0x344),
            tread_coverage: mem.f32(state + STATE_LERP_WEIGHT),
            carcass_width: mem.f32(def + DEF_CARCASS_WIDTH),
            patch_length: f(0x30c),
            normal_force: f(0x314),
            soil_density: f(0x238),
            lateral_velocity: f(SLIP_X),
            longitudinal_velocity: f(SLIP_X + 4),
            long_slip_velocity: f(LONG_CONTACT_VEL),
            lateral_deflection_ratio: f(0x300),
            inclination_cos: f(0x3b0),
            effective_radius: mem.f32(state + STATE_EFFECTIVE_RADIUS),
            smoothed_soil_drag: f(0x504),
            smoothed_drag_weight: f(0x508),
            smoothed_furrow_ratio: f(0x50c),
        }
    }

    /// Stores the soil drag results.
    fn store_soil_drag(&mut self, this: u32, out: &SoilDrag) {
        let mem = self;
        let mut set = |off: u32, value: f32| mem.set_f32(this + off, value);
        set(0x35c, out.flow_opposed);
        set(0x358, out.flow_aligned);
        set(0x350, out.longitudinal_force);
        set(0x314, out.normal_force);
        set(0x348, out.drag);
        set(0x354, out.lateral_flow);
        set(0x34c, out.lateral_force);
        set(0x504, out.smoothed_soil_drag);
        set(0x508, out.smoothed_drag_weight);
        set(0x50c, out.smoothed_furrow_ratio);
    }

    /// Stores the wear table pointer at `this+0x414` (`CScratch::Set414`, 0x532490).
    fn store_wear_table(&mut self, this: u32, table: u32) {
        let mem = self;
        mem.set_u32(this + 0x414, table);
    }

    /// Loads the values a contact solve reads: the indentation (7 words) at `indentation`, the forces (3 words) at
    /// `forces` and the coefficients (8 words) at `coefficients`, the soil thickness at `this+0x258` and the engaged flags.
    fn load_contact(&self, this: u32, indentation: u32, forces: u32, coefficients: u32) -> Contact {
        let mem = self;
        let coefficients = mem.f32s(coefficients);
        mem.load_contact_with(this, indentation, forces, coefficients)
    }

    /// [`load_contact`](TyreStore::load_contact) with the coefficients given, for a caller whose coefficients are in its frame.
    fn load_contact_with(&self, this: u32, indentation: u32, forces: u32, coefficients: [f32; 8]) -> Contact {
        let mem = self;
        let state = mem.u32(this + STATE);
        let [normal_force, carcass_force, tread_force] = mem.f32s(forces);
        Contact {
            soil_thickness: mem.f32(this + 0x258),
            indentation: Indentation::from_words(mem.f32s(indentation)),
            normal_force,
            carcass_force,
            tread_force,
            coefficients: coefficients_from_words(coefficients),
            tread_engaged: mem.u32(state + 0x70),
            carcass_engaged: mem.u32(state + 0x74),
        }
    }

    /// Stores the values a contact solve writes: the indentation, the forces and the engaged flags (`this+0x14`).
    fn store_contact(&mut self, this: u32, indentation: u32, forces: u32, c: &Contact) {
        let mem = self;
        store_floats(mem, indentation, &c.indentation.to_words());
        store_floats(mem, forces, &[c.normal_force, c.carcass_force, c.tread_force]);
        let state = mem.u32(this + STATE);
        mem.set_u32(state + 0x70, c.tread_engaged);
        mem.set_u32(state + 0x74, c.carcass_engaged);
    }

    /// Loads the hold's inputs: the weights, speed and forces of `this`, the contact frame at `this+0x170`, the state's
    /// active flag and the hold direction (`m_state+0x80`). `dt` is the time step.
    fn load_hold_input(&self, this: u32, dt: f32) -> HoldInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        HoldInput {
            sticking_weight: mem.f32(this + 0x3F0),
            active: mem.u32(state + 0xC),
            speed: mem.f32(this + SPEED),
            normal_force: mem.f32(this + 0x314),
            kinetic_friction: mem.f32(this + 0x1F8),
            lateral: Vec4::load(mem, this + SLIP_MATRIX),
            rolling: Vec4::load(mem, this + 0x180),
            normal: Vec4::load(mem, this + 0x190),
            direction: Vec4::load(mem, state + 0x80),
            dt,
        }
    }

    /// Stores the hold direction the hold leaves (`m_state+0x80`).
    fn store_hold_direction(&mut self, this: u32, direction: Vec4) {
        let mem = self;
        let state = mem.u32(this + STATE);
        direction.store(mem, state + 0x80);
    }

    /// Loads the heat step's inputs. The state's indices (`m_state+0xa8`, `+0xac`, `+0xc0`, `+0xc4`) select the
    /// setup entries they name, and `this+0x3c0` selects the sector's temperature and wear. The forces and the time
    /// step are the step's arguments.
    fn load_heat(&self, this: u32, lateral_force: f32, longitudinal_force: f32, dt: f32) -> HeatInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let idx = mem.u32(this + 0x3C0);
        let wear = mem.u32(this + 0x414) + 4 * idx;
        HeatInput {
            lateral_force,
            longitudinal_force,
            dt,
            water_factor: mem.f32(this + 0x288),
            lateral_velocity: mem.f32(this + SLIP_X),
            long_slip_velocity: mem.f32(this + LONG_CONTACT_VEL),
            spin: mem.f32(this + SPIN),
            rolling_torque: mem.f32(this + 0x3C8),
            contact_pressure: mem.f32(this + 0x310),
            speed: mem.f32(this + SPEED),
            slip_phase: mem.f32(this + 0x388),
            tread_contact_area: mem.f32(this + 0x308),
            surface_temperature: mem.f32(state + 0xD0 + 12 * idx),
            tread_temperature: mem.f32(state + 0x12C),
            carcass_temperature: mem.f32(state + 0x130),
            wear_weight: mem.f32(state + 0xB0),
            pressure_weight: mem.f32(state + 0xC8),
            conductance_a: mem.f32(word_at(def + 0x16C, mem.u32(state + 0xC0))),
            conductance_b: mem.f32(word_at(def + 0x16C, mem.u32(state + 0xC4))),
            wear_rate_a: mem.f32(word_at(def + 0x1C4, mem.u32(state + 0xA8))),
            wear_rate_b: mem.f32(word_at(def + 0x1C4, mem.u32(state + 0xAC))),
            tread_conductivity: mem.f32(def + 0x118),
            capacity: [0x110, 0x11C, 0x128].map(|at| load_factors(mem, def + at)),
            wear: mem.f32(wear),
        }
    }

    /// Stores the heat step's results: the scratch words, the sector's temperature, the tread and carcass temperatures,
    /// and the wear.
    fn store_heat(&mut self, this: u32, out: &HeatOutput) {
        let mem = self;
        let state = mem.u32(this + STATE);
        let idx = mem.u32(this + 0x3C0);
        let wear = mem.u32(this + 0x414) + 4 * idx;
        mem.set_f32(this + 0x3C4, out.wet_term);
        mem.set_f32(this + 0x4E4, out.frictional_power);
        mem.set_f32(this + 0x4EC, out.wet_damping);
        mem.set_f32(this + 0x4E0, out.surface_heat_loss);
        mem.set_f32(this + 0x404, out.abrasion_rate);
        mem.set_f32(state + 0xD0 + 12 * idx, out.surface_temperature);
        mem.set_f32(state + 0x12C, out.tread_temperature);
        mem.set_f32(state + 0x130, out.carcass_temperature);
        mem.set_f32(wear, out.wear);
    }

    /// Loads the sector step's inputs for sector `idx`: its surface temperature (`m_state+0xd0+12·idx`), the speed, the
    /// tread temperature (`m_state+0x12c`), the tread's conductivity (`m_def+0x118`) and the capacities of the surface
    /// layer and of the tread (`m_def+0x110`, `+0x11c`).
    fn load_sector_input(&self, this: u32, idx: u32) -> SectorInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        SectorInput {
            surface_temperature: mem.f32(state + 0xD0 + 12 * idx),
            speed: mem.f32(this + SPEED),
            tread_temperature: mem.f32(state + 0x12C),
            tread_conductivity: mem.f32(def + 0x118),
            surface_capacity: load_factors(mem, def + 0x110),
            tread_capacity: load_factors(mem, def + 0x11C),
        }
    }

    /// Stores the sector step's results: the tread's heat loss (`this+0x4f8`), the sector's surface temperature and the
    /// tread temperature.
    fn store_sector(&mut self, this: u32, idx: u32, out: &SectorOutput) {
        let mem = self;
        let state = mem.u32(this + STATE);
        mem.set_f32(this + 0x4F8, out.tread_heat_loss);
        mem.set_f32(state + 0xD0 + 12 * idx, out.surface_temperature);
        mem.set_f32(state + 0x12C, out.tread_temperature);
    }

    /// Loads the relaxation's inputs: the carcass temperature and the inner air's, the carcass radius, the tread
    /// width and the conductivity between the carcass and the tread, the capacities, the inner air's gas
    /// (`m_state+0x60..0x6c`) and the sector input at the sector being stepped (`m_state+0x1c & 7`).
    fn load_relax(&self, this: u32, dt: f32) -> RelaxInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        RelaxInput {
            dt,
            carcass_temperature: mem.f32(state + 0x130),
            inner_air_temperature: mem.f32(state + STATE_TYRE_TEMPERATURE),
            carcass_radius: mem.f32(def + DEF_CARCASS_RADIUS),
            tread_width: mem.f32(def + DEF_TREAD_WIDTH),
            carcass_conductivity: mem.f32(def + 0x124),
            tread_capacity: load_factors(mem, def + 0x11C),
            carcass_capacity: load_factors(mem, def + 0x128),
            inner_air: load_force_terms(mem, state + 0x60),
            sector: mem.load_sector_input(this, relax_index(mem, state)),
        }
    }

    /// Stores the relaxation's results: the carcass temperature, the sidewalls' heat loss (`this+0x4fc`), the inner
    /// air's gas, and the sector.
    fn store_relax(&mut self, this: u32, out: &RelaxOutput) {
        let mem = self;
        let state = mem.u32(this + STATE);
        mem.set_f32(state + 0x130, out.carcass_temperature);
        mem.set_f32(this + 0x4FC, out.sidewall_heat_loss);
        store_force_terms(mem, state + 0x60, out.inner_air);
        mem.store_sector(this, relax_index(mem, state), &out.sector);
    }

    /// Loads the friction limits' inputs. `src` holds the stiffnesses at `+0x36c` and `+0x370`, which pass through; the
    /// shares and the scales are the caller's words.
    fn load_friction_limits(&self, this: u32, src: u32, shares: [f32; 2], scales: [f32; 2]) -> LimitsInput {
        let mem = self;
        let (weight, [cornering_friction, traction_friction, cornering_peak_slip, traction_peak_slip]) =
            load_blend(mem, this);
        LimitsInput {
            weight,
            cornering_friction,
            traction_friction,
            cornering_peak_slip,
            traction_peak_slip,
            lateral_share: shares[0],
            longitudinal_share: shares[1],
            static_scale: scales[0],
            kinetic_scale: scales[1],
            cornering_stiffness: mem.f32(src + 0x36C),
            traction_stiffness: mem.f32(src + 0x370),
        }
    }

    /// Loads the patch forces' inputs from the scratch at `this`.
    fn load_patch_input(&self, this: u32) -> PatchInput {
        let mem = self;
        let state = mem.u32(this + STATE);
        let def = mem.u32(this + DEF);
        let (weight, tables) = load_blend(mem, this);
        PatchInput {
            speed: mem.f32(this + SPEED),
            lateral_velocity: mem.f32(this + SLIP_X),
            longitudinal_velocity: mem.f32(this + SLIP_X + 4),
            spin: mem.f32(this + SPIN),
            effective_radius: mem.f32(state + STATE_EFFECTIVE_RADIUS),
            static_friction: mem.f32(this + 0x1E8),
            kinetic_friction: mem.f32(this + 0x1F8),
            reference_friction: mem.f32(def + DEF_REFERENCE_FRICTION),
            normal_force: mem.f32(this + 0x314),
            cornering_stiffness: mem.f32(this + 0x38C),
            traction_stiffness: mem.f32(this + 0x390),
            inclination_sin: mem.f32(this + 0x3AC),
            patch_length: mem.f32(this + 0x30C),
            sink_depth: mem.f32(this + 0x340),
            weight,
            tables,
        }
    }

    /// Stores the patch forces: `ptrs[0]` the two forces (three floats), `ptrs[1]` zeroed, then the pneumatic trail and
    /// the two slip phases.
    fn store_patch_forces(&mut self, ptrs: [u32; 5], out: &PatchForces) {
        let mem = self;
        for (i, value) in [out.lateral_force, out.longitudinal_force, 0.0].iter().enumerate() {
            mem.set_f32(ptrs[0] + 4 * i as u32, *value);
        }
        Vec4::default().store(mem, ptrs[1]);
        mem.set_f32(ptrs[2], out.pneumatic_trail);
        mem.set_f32(ptrs[3], out.lateral_phase);
        mem.set_f32(ptrs[4], out.longitudinal_phase);
    }

    /// Stores the friction limits through the ten output pointers (the stiffnesses first, then the limits in the
    /// struct's order).
    fn store_friction_limits(&mut self, ptrs: [u32; 10], out: &FrictionLimits) {
        let mem = self;
        mem.set_f32(ptrs[0], out.cornering_stiffness);
        mem.set_f32(ptrs[1], out.traction_stiffness);
        mem.set_f32(ptrs[2], out.lateral_peak);
        mem.set_f32(ptrs[3], out.lateral_kinetic);
        mem.set_f32(ptrs[4], out.longitudinal_peak);
        mem.set_f32(ptrs[5], out.longitudinal_kinetic);
        mem.set_f32(ptrs[6], out.lateral_peak_slip);
        mem.set_f32(ptrs[7], out.lateral_end_slip);
        mem.set_f32(ptrs[8], out.longitudinal_peak_slip);
        mem.set_f32(ptrs[9], out.longitudinal_end_slip);
    }

    /// The game's debug flags for `Step`: the word at `g_1659b70 -> +0x24 -> +0x160e0` (`& 0x20`). When it is set, `Step`
    /// copies its `f` debug_words to `this+0x4c8`. The three words are read here, and the test maps them as node windows.
    fn load_step_flag(&self) -> bool {
        let mem = self;
        let physics = mem.u32(0x0165_9B70);
        let def = mem.u32(physics + 0x24);
        mem.u32(def + 0x160E0) & 0x20 != 0
    }

    /// Stores the output block at `out`.
    fn store_output_block(&mut self, out: u32, b: &OutputBlock) {
        let mem = self;
        b.edge_a.store(mem, out + 0x10);
        b.edge_b.store(mem, out + 0x20);
        b.motion.store(mem, out + 0x50);
        mem.set_f32(out + 0x80, b.mark_a);
        mem.set_f32(out + 0x84, b.mark_b);
        mem.set_f32(out + 0x88, b.soil_flow);
        mem.set_u32(out + 0x90, b.sink_depth_bits);
        mem.set_u32(out + 0x94, b.slip_phase_bits);
        mem.set_f32(out + 0x98, b.slip_area);
        mem.set_u32(out + 0xAC, b.valid);
    }

    /// Loads the sixteen stack words of the tyre step.
    fn load_step(&self, w: &[u32]) -> (StepArgs, StepPtrs) {
        let mem = self;
        let args = StepArgs {
            spin: f32::from_bits(w[0]),
            dt: f32::from_bits(w[1]),
            hub_height: f32::from_bits(w[2]),
            slip_area_scale: f32::from_bits(w[3]),
            material: mem.load_material(w[4]),
            debug_words: std::array::from_fn(|k| mem.u32(w[5] + 4 * k as u32)),
            copy_debug_words: mem.load_step_flag(),
            normal: Vec4::load(mem, w[6]),
            axle: Vec4::load(mem, w[8]),
            velocity: Vec4::load(mem, w[9]),
            contact_point: Vec4::load(mem, w[10]),
            torque_in: Vec4::load(mem, w[12]),
        };
        let ptrs =
            StepPtrs { force: w[11], torque: w[12], reaction_torque: w[13], sticking_load: w[14], output_block: w[15] };
        (args, ptrs)
    }

    /// Stores the outputs of the tyre step (the `None` fields are not written).
    fn store_step(&mut self, p: &StepPtrs, out: &StepOut) {
        let mem = self;
        out.force.store(mem, p.force);
        out.torque.store(mem, p.torque);
        mem.set_f32(p.reaction_torque, out.reaction_torque);
        if let Some(load) = out.sticking_load {
            mem.set_f32(p.sticking_load, load);
        }
        if let Some(block) = &out.block {
            mem.store_output_block(p.output_block, block);
        }
    }

    /// Loads the surface friction's inputs for the surface classes at the indices `i` and `j` with the wetnesses `a` and
    /// `b`: the setup's base frictions (`+0x2c4`), wet slopes (`+0x2e0`) and wear factors (`+0x2f8`) at those indices, its
    /// design wetness (`+0x2dc`), and the wear of the sector (`m_state+0x134`). The indices are words, so the address is
    /// wrapping, as the original's is.
    fn load_surface_input(&self, this: u32, i: u32, j: u32, a: f32, b: f32) -> SurfaceInput {
        let mem = self;
        let def = mem.u32(this + DEF);
        let state = mem.u32(this + STATE);
        let wear = mem.u32(this + 0x414);
        SurfaceInput {
            base_first: mem.f32(word_at(def + 0x2C4, i)),
            base_second: mem.f32(word_at(def + 0x2C4, j)),
            wet_slope_first: mem.f32(word_at(def + 0x2E0, i)),
            wet_slope_second: mem.f32(word_at(def + 0x2E0, j)),
            wear_factor_first: mem.f32(word_at(def + 0x2F8, i)),
            wear_factor_second: mem.f32(word_at(def + 0x2F8, j)),
            design_wetness: mem.f32(def + 0x2DC),
            wetness_first: a,
            wetness_second: b,
            sector_wear: mem.f32(word_at(wear, mem.u32(state + 0x134))),
        }
    }

    /// Stores the surface friction through the caller's pointers `o1` and `o2`, in that order.
    fn store_surface_friction(&mut self, o1: u32, o2: u32, out: SurfaceFriction) {
        let mem = self;
        mem.set_f32(o1, out.first);
        mem.set_f32(o2, out.second);
    }

    /// Stores the `RelaxStep` results, then `RelaxCount`'s counter: `m_state+0x1c` goes up by one and its dirty flag
    /// (`+0x18`) is set.
    fn store_relax_count(&mut self, this: u32, out: &RelaxOutput) {
        let mem = self;
        mem.store_relax(this, out);
        let state = mem.u32(this + STATE);
        let count = mem.u32(state + 0x1C);
        ntweak::set_dirty(mem, state + 0x10);
        mem.set_u32(state + 0x1C, count.wrapping_add(1));
    }

    /// `TyreTail_Get10` (0x5326b0, thiscall): the word at `container+0x10`, the tyre tail of a wheel's container.
    fn load_tyre_tail10(&self, container: u32) -> u32 {
        let mem = self;
        mem.u32(container + 0x10)
    }

    /// `TyreTail_Get14` (0x5326c0, thiscall): the word at `container+0x14`.
    fn load_tyre_tail14(&self, container: u32) -> u32 {
        let mem = self;
        mem.u32(container + 0x14)
    }

    /// `NTweak::CBase::GetName` (0x52db10, thiscall): the name object at `this+4`.
    fn load_name_object(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + 4)
    }

    /// The tyre-noise signal, at `g_signalManager + 0x6268` (`CSignalManager::CMultiSignal1<CSigTyreNoise>`).
    fn noise_signal(&self) -> u32 {
        let mem = self;
        physics_core::layout::signal_manager(mem) + 0x6268
    }
}

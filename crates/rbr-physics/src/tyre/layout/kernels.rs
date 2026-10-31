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

/// The scratch words the tyre step reads and writes, each as `kind offset field`, in the order `store_frame`
/// writes them; `load_frame` reads the same words (reads have no order). The kinds are plain `f32` and `u32`
/// words, four-lane `vec4` vectors, the `material` block at `+0x1a0`, the seven-word `indentation` solve at `+0x32c`,
/// the three-lane `contact_velocity` at the slip velocity, the eight sector `wear` weights at the wear table, the
/// 24-word noise `signal` block and the three `debug_words`.
///
/// The material block comes first: ten of its words alias the scalar fields below, so the block's copy must land
/// before the scalars overwrite them. `wet_damping` (`+0x4ec`) is stored before `ngp_heat_power` (`+0x4e8`), as the
/// original writes them. The contact point, the contact frame and the dirty words are stored last.
macro_rules! scratch_words {
    ($m:ident, $($ctx:tt)*) => {
        $m!($($ctx)*; material);
        $m!($($ctx)*; f32 0x1E8 static_friction);
        $m!($($ctx)*; f32 0x1F8 kinetic_friction);
        $m!($($ctx)*; f32 0x208 static_friction_soil);
        $m!($($ctx)*; f32 0x218 kinetic_friction_soil);
        $m!($($ctx)*; f32 0x228 sink_factor);
        $m!($($ctx)*; f32 0x258 soil_thickness);
        $m!($($ctx)*; f32 0x288 water_factor);
        $m!($($ctx)*; f32 0x298 rolling_resistance);
        $m!($($ctx)*; f32 0x300 lateral_deflection_ratio);
        $m!($($ctx)*; f32 0x304 camber_radius);
        $m!($($ctx)*; f32 0x308 tread_contact_area);
        $m!($($ctx)*; f32 0x30C patch_length);
        $m!($($ctx)*; f32 0x310 contact_pressure);
        $m!($($ctx)*; f32 0x314 normal_force);
        $m!($($ctx)*; f32 0x318 carcass_force);
        $m!($($ctx)*; f32 0x31C tread_force);
        $m!($($ctx)*; f32 0x324 total_force_ratio);
        $m!($($ctx)*; f32 0x328 tread_force_share);
        $m!($($ctx)*; indentation);
        $m!($($ctx)*; f32 0x348 soil_drag);
        $m!($($ctx)*; f32 0x34C lateral_soil_force);
        $m!($($ctx)*; f32 0x350 longitudinal_soil_force);
        $m!($($ctx)*; f32 0x354 lateral_soil_flow);
        $m!($($ctx)*; f32 0x358 soil_flow_aligned);
        $m!($($ctx)*; f32 0x35C soil_flow_opposed);
        $m!($($ctx)*; f32 SPEED speed);
        $m!($($ctx)*; f32 SPEED_SQ speed_sq);
        $m!($($ctx)*; contact_velocity);
        $m!($($ctx)*; f32 LONG_CONTACT_VEL long_slip_velocity);
        $m!($($ctx)*; f32 SPIN spin);
        $m!($($ctx)*; f32 SLIP_RATIO_TERM spin_slip);
        $m!($($ctx)*; f32 0x380 low_speed_a);
        $m!($($ctx)*; f32 0x384 low_speed_b);
        $m!($($ctx)*; f32 0x388 slip_phase);
        $m!($($ctx)*; f32 0x38C cornering_stiffness);
        $m!($($ctx)*; f32 0x390 traction_stiffness);
        $m!($($ctx)*; f32 0x394 lateral_stiffness);
        $m!($($ctx)*; f32 0x398 torsional_stiffness);
        $m!($($ctx)*; f32 0x39C lateral_force);
        $m!($($ctx)*; f32 0x3A0 longitudinal_force);
        $m!($($ctx)*; f32 0x3A4 torsion_torque);
        $m!($($ctx)*; f32 0x3A8 hub_height);
        $m!($($ctx)*; f32 0x3AC inclination_sin);
        $m!($($ctx)*; f32 0x3B0 inclination_cos);
        $m!($($ctx)*; f32 0x3B4 stiffness_factor);
        $m!($($ctx)*; f32 0x3B8 pneumatic_trail);
        $m!($($ctx)*; f32 0x3BC tread_remaining);
        $m!($($ctx)*; f32 0x3C4 wet_term);
        $m!($($ctx)*; f32 0x3C8 rolling_torque);
        $m!($($ctx)*; f32 0x3CC extra_force);
        $m!($($ctx)*; f32 0x3D0 extra_axle_torque);
        $m!($($ctx)*; f32 0x3D4 extra_term);
        $m!($($ctx)*; f32 0x3D8 slip_area_scale);
        $m!($($ctx)*; f32 0x3DC sliding_weight);
        $m!($($ctx)*; f32 0x3E0 lateral_phase);
        $m!($($ctx)*; f32 0x3E4 longitudinal_phase);
        $m!($($ctx)*; f32 0x3E8 sliding_deflection);
        $m!($($ctx)*; f32 0x3EC sliding_torsion);
        $m!($($ctx)*; f32 0x3F0 sticking_weight);
        $m!($($ctx)*; f32 0x3F4 stick_phase_a);
        $m!($($ctx)*; f32 0x3F8 stick_phase_b);
        $m!($($ctx)*; f32 0x3FC sticking_deflection);
        $m!($($ctx)*; f32 0x400 sticking_torsion);
        $m!($($ctx)*; f32 0x404 abrasion_rate);
        $m!($($ctx)*; wear);
        $m!($($ctx)*; signal);
        $m!($($ctx)*; u32 0x480 in_contact);
        $m!($($ctx)*; u32 0x484 sector_copy);
        $m!($($ctx)*; vec4 0x490 contact_point_copy);
        $m!($($ctx)*; vec4 0x4A0 motion_copy);
        $m!($($ctx)*; u32 0x4B0 soil_flow_copy);
        $m!($($ctx)*; u32 0x4B4 long_slip_velocity_copy);
        $m!($($ctx)*; u32 0x4B8 longitudinal_velocity_copy);
        $m!($($ctx)*; f32 0x4BC temperature_grip);
        $m!($($ctx)*; f32 0x4C0 lateral_friction_force);
        $m!($($ctx)*; f32 0x4C4 longitudinal_friction_force);
        $m!($($ctx)*; debug_words);
        $m!($($ctx)*; f32 0x4E0 surface_heat_loss);
        $m!($($ctx)*; f32 0x4E4 frictional_power);
        $m!($($ctx)*; f32 0x4EC wet_damping);
        $m!($($ctx)*; f32 0x4E8 ngp_heat_power);
        $m!($($ctx)*; f32 0x504 smoothed_soil_drag);
        $m!($($ctx)*; f32 0x508 smoothed_drag_weight);
        $m!($($ctx)*; f32 0x50C smoothed_furrow_ratio);
        $m!($($ctx)*; vec4 0x160 contact_point);
        $m!($($ctx)*; contact_frame);
        $m!($($ctx)*; u32 0x1E4 static_friction_dirty);
        $m!($($ctx)*; u32 0x1F4 kinetic_friction_dirty);
        $m!($($ctx)*; u32 0x3C0 sector);
    };
}

/// Assigns one [`scratch_words!`] entry of `target` (`mem` the memory, `base` the scratch, `wear` its wear
/// table). The offsets below appear only here; `load_frame` reads them, `store_frame` writes them back.
macro_rules! load_scratch_into {
    ($mem:tt $base:ident $wear:ident $it:ident ; material) => {
        $it.material = $mem.load_material($base + 0x1A0);
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; f32 $off:tt $field:ident) => {
        $it.$field = $mem.f32($base + $off);
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; u32 $off:tt $field:ident) => {
        $it.$field = $mem.u32($base + $off);
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; vec4 $off:tt $field:ident) => {
        $it.$field = Vec4::load($mem, $base + $off);
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; indentation) => {
        $it.indentation = Indentation::from_words($mem.f32s($base + 0x32C));
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; contact_velocity) => {
        $it.contact_velocity = $mem.f32s($base + SLIP_X);
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; wear) => {
        $it.wear = $mem.f32s($wear);
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; signal) => {
        $it.signal = std::array::from_fn(|k| $mem.u32($base + 0x420 + 4 * k as u32));
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; debug_words) => {
        $it.debug_words = std::array::from_fn(|k| $mem.u32($base + 0x4C8 + 4 * k as u32));
    };
    ($mem:tt $base:ident $wear:ident $it:ident ; contact_frame) => {
        $it.contact_frame = ContactFrame {
            lateral: Vec4::load($mem, $base + SLIP_MATRIX),
            rolling: Vec4::load($mem, $base + SLIP_MATRIX + 0x10),
            normal: Vec4::load($mem, $base + SLIP_MATRIX + 0x20),
        };
    };
}

/// Writes one [`scratch_words!`] entry of `src` (`mem` the memory, `base` the scratch, `wear` its wear table).
macro_rules! store_scratch_word {
    ($mem:tt $base:ident $wear:ident $src:ident ; material) => {
        $mem.store_material($base + 0x1A0, &$src.material);
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; f32 $off:tt $field:ident) => {
        $mem.set_f32($base + $off, $src.$field);
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; u32 $off:tt $field:ident) => {
        $mem.set_u32($base + $off, $src.$field);
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; vec4 $off:tt $field:ident) => {
        $src.$field.store($mem, $base + $off);
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; indentation) => {
        store_floats($mem, $base + 0x32C, &$src.indentation.to_words());
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; contact_velocity) => {
        store_floats($mem, $base + SLIP_X, &$src.contact_velocity);
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; wear) => {
        store_floats($mem, $wear, &$src.wear);
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; signal) => {
        for (k, word) in $src.signal.iter().enumerate() {
            $mem.set_u32($base + 0x420 + 4 * k as u32, *word);
        }
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; debug_words) => {
        for (k, word) in $src.debug_words.iter().enumerate() {
            $mem.set_u32($base + 0x4C8 + 4 * k as u32, *word);
        }
    };
    ($mem:tt $base:ident $wear:ident $src:ident ; contact_frame) => {
        $src.contact_frame.lateral.store($mem, $base + SLIP_MATRIX);
        $src.contact_frame.rolling.store($mem, $base + SLIP_MATRIX + 0x10);
        $src.contact_frame.normal.store($mem, $base + SLIP_MATRIX + 0x20);
    };
}

/// The state words the tyre step reads and writes, each as `kind offset field`, in the order `store_frame` writes
/// them; `load_frame` reads the same words. Besides plain `f32` and `u32` words and four-lane `vec4` vectors, the
/// kinds are the three-word `seg` search results and the two words of each twelve-byte surface-temperature entry
/// (`sector_temps`: the eight surface temperatures, then the entries' other two words in order, without the last
/// entry's second word, which is the tread temperature).
macro_rules! state_words {
    ($m:ident, $($ctx:tt)*) => {
        $m!($($ctx)*; u32 0x0C active);
        $m!($($ctx)*; u32 0x28 started_dirty);
        $m!($($ctx)*; u32 0x2C started);
        $m!($($ctx)*; vec4 0x30 last_point);
        $m!($($ctx)*; f32 0x40 lateral_deflection);
        $m!($($ctx)*; f32 0x44 torsion_angle);
        $m!($($ctx)*; f32 0x48 penetration_depth);
        $m!($($ctx)*; f32 STATE_EFFECTIVE_RADIUS effective_radius);
        $m!($($ctx)*; f32 0x50 vertical_stiffness);
        $m!($($ctx)*; f32 0x54 tread_elasticity);
        $m!($($ctx)*; f32 STATE_LERP_WEIGHT tread_coverage);
        $m!($($ctx)*; u32 0x5C grip_clamped);
        $m!($($ctx)*; f32 0x68 air_pressure);
        $m!($($ctx)*; u32 0x70 tread_engaged);
        $m!($($ctx)*; u32 0x74 carcass_engaged);
        $m!($($ctx)*; vec4 0x80 hold_direction);
        $m!($($ctx)*; seg 0x90 pressure_segment);
        $m!($($ctx)*; seg 0x9C load_segment);
        $m!($($ctx)*; seg 0xA8 surface_temperature_segment);
        $m!($($ctx)*; seg 0xB4 tread_temperature_segment);
        $m!($($ctx)*; seg 0xC0 contact_pressure_segment);
        $m!($($ctx)*; sector_temps);
        $m!($($ctx)*; f32 0x12C tread_temperature);
        $m!($($ctx)*; f32 0x130 carcass_temperature);
        $m!($($ctx)*; u32 0x134 sector);
    };
}

/// Assigns one [`state_words!`] entry of `target` (`mem` the memory, `base` the state, `entry` reads word `word`
/// of entry `k` of the surface-temperature table).
macro_rules! load_state_into {
    ($mem:tt $base:ident $entry:ident $it:ident ; f32 $off:tt $field:ident) => {
        $it.$field = $mem.f32($base + $off);
    };
    ($mem:tt $base:ident $entry:ident $it:ident ; u32 $off:tt $field:ident) => {
        $it.$field = $mem.u32($base + $off);
    };
    ($mem:tt $base:ident $entry:ident $it:ident ; vec4 $off:tt $field:ident) => {
        $it.$field = Vec4::load($mem, $base + $off);
    };
    ($mem:tt $base:ident $entry:ident $it:ident ; seg $off:tt $field:ident) => {
        $it.$field = $mem.load_segment($base + $off);
    };
    ($mem:tt $base:ident $entry:ident $it:ident ; sector_temps) => {
        $it.surface_temperature = std::array::from_fn(|k| $entry(k as u32, 0));
        // The entries' second and third words in order; the last third word is the tread temperature.
        $it.sector_spare = std::array::from_fn(|i| $entry(i as u32 / 2, 1 + i as u32 % 2));
    };
}

/// Writes one [`state_words!`] entry of `src` (`mem` the memory, `base` the state).
macro_rules! store_state_word {
    ($mem:tt $base:ident $src:ident ; f32 $off:tt $field:ident) => {
        $mem.set_f32($base + $off, $src.$field);
    };
    ($mem:tt $base:ident $src:ident ; u32 $off:tt $field:ident) => {
        $mem.set_u32($base + $off, $src.$field);
    };
    ($mem:tt $base:ident $src:ident ; vec4 $off:tt $field:ident) => {
        $src.$field.store($mem, $base + $off);
    };
    ($mem:tt $base:ident $src:ident ; seg $off:tt $field:ident) => {
        $mem.store_segment($base + $off, &$src.$field);
    };
    ($mem:tt $base:ident $src:ident ; sector_temps) => {
        for (k, temperature) in $src.surface_temperature.iter().enumerate() {
            $mem.set_f32($base + 0xD0 + 12 * k as u32, *temperature);
        }
        for (i, word) in $src.sector_spare.iter().enumerate() {
            $mem.set_f32($base + 0xD0 + 12 * (i as u32 / 2) + 4 + 4 * (i as u32 % 2), *word);
        }
    };
}

/// The Mem store: the bodies are the layout's reads and writes.
impl TyreStore for Mem {
    /// Loads a [`Segment`] at `at`: `lo`, `hi` and `frac` in three words.
    fn load_segment(&self, at: u32) -> Segment {
        Segment { lo: self.u32(at), hi: self.u32(at + 4), frac: self.f32(at + 8) }
    }

    /// Stores a [`Segment`] at `at`.
    fn store_segment(&mut self, at: u32, s: &Segment) {
        self.set_u32(at, s.lo);
        self.set_u32(at + 4, s.hi);
        self.set_f32(at + 8, s.frac);
    }

    /// The table of `len` entries at `at`, with the word below it.
    fn read_table(&self, at: u32, len: usize) -> Table {
        Table {
            below: self.f32(at - 4),
            len,
            entries: std::array::from_fn(|k| if k < len { self.f32(at + 4 * k as u32) } else { 0.0 }),
        }
    }

    /// Loads the whole tyre step footprint of the scratch at `this`: its state, its setup, its wear table and the
    /// contact record's angle. The pointers are read from `this` (`+0x10` setup, `+0x14` state, `+0x410` record,
    /// `+0x414` wear table).
    fn load_frame(&self, this: u32) -> Frame {
        let state = self.u32(this + STATE);
        let def = self.u32(this + DEF);
        let record = self.u32(this + 0x410);
        let wear = self.u32(this + 0x414);
        let entry = |k: u32, word: u32| self.f32(state + 0xD0 + 12 * k + 4 * word);
        // The shared word lists (`scratch_words!`, `state_words!`) hold every offset<->field pair once, in the
        // order `store_frame` writes them; the load reads the same words. Only the contact angle (read from the
        // record, never stored) and the read-only setup are assigned here.
        let mut scratch = Scratch::default();
        scratch_words!(load_scratch_into, self this wear scratch);
        scratch.contact_angle = self.f32(record + 0x5AC);
        let mut state_words = StateWords::default();
        state_words!(load_state_into, self state entry state_words);
        let setup = Setup {
            debug_id: self.u32(def + DEF_DEBUG_ID),
            pressure_ratio: self.f32(def + 0x2C),
            reference_friction: self.f32(def + DEF_REFERENCE_FRICTION),
            carcass_stiffness_lateral: self.f32(def + DEF_CARCASS_STIFFNESS_LATERAL),
            carcass_stiffness_torsional: self.f32(def + DEF_CARCASS_STIFFNESS_TORSIONAL),
            carcass_radius: self.f32(def + DEF_CARCASS_RADIUS),
            tread_depth: self.f32(def + DEF_TREAD_DEPTH),
            carcass_thickness: self.f32(def + 0x9C),
            tread_width: self.f32(def + DEF_TREAD_WIDTH),
            tread_coverage: self.f32(def + DEF_TREAD_COVERAGE),
            carcass_width: self.f32(def + DEF_CARCASS_WIDTH),
            sidewall_height: self.f32(def + 0xEC),
            wet_wear_factor: self.f32(def + 0x310),
            inclination_heat_scale: self.f32(def + 0x10C),
            surface_capacity: self.f32s(def + 0x110),
            tread_capacity: self.f32s(def + 0x11C),
            carcass_capacity: self.f32s(def + 0x128),
            tread_conductivity: self.f32(def + 0x118),
            pressure_axis: self.read_table(def + 0x134, 4),
            stiffness_factor_by_pressure: self.read_table(def + 0x144, 4),
            contact_pressure_axis: self.read_table(def + 0x154, 6),
            contact_conductance: self.read_table(def + 0x16C, 6),
            temperature_axis: self.read_table(def + 0x184, 8),
            rubber_friction_by_temperature: self.read_table(def + 0x1A4, 8),
            rubber_wear_by_temperature: self.read_table(def + 0x1C4, 8),
            load_axis: self.read_table(def + 0x1E4, 8),
            cornering_stiffness_by_load: self.read_table(def + 0x204, 8),
            traction_stiffness_by_load: self.read_table(def + 0x224, 8),
            cornering_friction_by_load: self.read_table(def + 0x244, 8),
            traction_friction_by_load: self.read_table(def + 0x264, 8),
            cornering_peak_slip_by_load: self.read_table(def + 0x284, 8),
            traction_peak_slip_by_load: self.read_table(def + 0x2A4, 8),
        };
        Frame { scratch, state: state_words, setup }
    }

    /// Stores the tyre step footprint back to the scratch at `this` (its state and scratch; the setup and the contact
    /// angle are read only). Every word is written, with the value it was loaded with unless the port changed it.
    fn store_frame(&mut self, this: u32, frame: &Frame) {
        let state = self.u32(this + STATE);
        let wear = self.u32(this + 0x414);
        let s = &frame.scratch;
        // Every pair once: `scratch_words!` lists the words in this order (the material block first, then the
        // scalars over it; `+0x4ec` before `+0x4e8`; the contact frame and the dirty words last).
        scratch_words!(store_scratch_word, self this wear s);
        let st = &frame.state;
        state_words!(store_state_word, self state st);
    }

    /// Loads the copied words of the block at `at`.
    fn load_material(&self, at: u32) -> Material {
        std::array::from_fn(|i| self.u32(at + 4 * WORDS[i]))
    }

    /// Stores the copied words into the block at `at`; the other words are left as they are.
    fn store_material(&mut self, at: u32, material: &Material) {
        for (i, word) in material.iter().enumerate() {
            self.set_u32(at + 4 * WORDS[i], *word);
        }
    }

    /// Loads the effective radius of the tyre at `this`, m.
    fn load_effective_radius(&self, this: u32) -> f32 {
        self.f32(self.u32(this + STATE) + STATE_EFFECTIVE_RADIUS)
    }

    /// Loads the contact frame's axes (`this+0x170`, rows 0x10 bytes apart).
    fn load_contact_axes(&self, this: u32) -> ContactAxes {
        ContactAxes {
            lateral: self.f32s(this + SLIP_MATRIX),
            rolling: self.f32s(this + SLIP_MATRIX + 0x10),
            normal: self.f32s(this + SLIP_MATRIX + 0x20),
        }
    }

    /// Stores the slip.
    fn store_slip(&mut self, this: u32, slip: &Slip) {
        for (i, v) in slip.velocity.iter().enumerate() {
            self.set_f32(this + SLIP_X + 4 * i as u32, *v);
        }
        self.set_f32(this + SPEED_SQ, slip.speed_sq);
        self.set_f32(this + SPEED, slip.speed);
        self.set_f32(this + LONG_CONTACT_VEL, slip.long_slip_velocity);
        self.set_f32(this + SLIP_RATIO_TERM, slip.spin_slip);
    }

    /// Loads the rolling resistance torque inputs.
    fn load_rolling_input(&self, this: u32) -> RollingInput {
        RollingInput {
            patch_length: self.f32(this + 0x30c),
            rolling_resistance: self.f32(this + 0x298),
            normal_force: self.f32(this + 0x314),
            sliding_weight: self.f32(this + 0x3dc),
            spin: self.f32(this + SPIN),
        }
    }

    /// Loads the soil drag inputs: the scratch words, the state and the tyre setup (`this+0x10`).
    fn load_soil_input(&self, this: u32) -> SoilInput {
        let state = self.u32(this + STATE);
        let def = self.u32(this + DEF);
        let f = |off: u32| self.f32(this + off);
        SoilInput {
            sink_depth: f(0x340),
            carcass_compression: f(0x344),
            tread_coverage: self.f32(state + STATE_LERP_WEIGHT),
            carcass_width: self.f32(def + DEF_CARCASS_WIDTH),
            patch_length: f(0x30c),
            normal_force: f(0x314),
            soil_density: f(0x238),
            lateral_velocity: f(SLIP_X),
            longitudinal_velocity: f(SLIP_X + 4),
            long_slip_velocity: f(LONG_CONTACT_VEL),
            lateral_deflection_ratio: f(0x300),
            inclination_cos: f(0x3b0),
            effective_radius: self.f32(state + STATE_EFFECTIVE_RADIUS),
            smoothed_soil_drag: f(0x504),
            smoothed_drag_weight: f(0x508),
            smoothed_furrow_ratio: f(0x50c),
        }
    }

    /// Stores the soil drag results.
    fn store_soil_drag(&mut self, this: u32, out: &SoilDrag) {
        let mut set = |off: u32, value: f32| self.set_f32(this + off, value);
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
        self.set_u32(this + 0x414, table);
    }

    /// Loads the values a contact solve reads: the indentation (7 words) at `indentation`, the forces (3 words) at
    /// `forces` and the coefficients (8 words) at `coefficients`, the soil thickness at `this+0x258` and the engaged flags.
    fn load_contact(&self, this: u32, indentation: u32, forces: u32, coefficients: u32) -> Contact {
        let coefficients = self.f32s(coefficients);
        self.load_contact_with(this, indentation, forces, coefficients)
    }

    /// [`load_contact`](TyreStore::load_contact) with the coefficients given, for a caller whose coefficients are in its frame.
    fn load_contact_with(&self, this: u32, indentation: u32, forces: u32, coefficients: [f32; 8]) -> Contact {
        let state = self.u32(this + STATE);
        let [normal_force, carcass_force, tread_force] = self.f32s(forces);
        Contact {
            soil_thickness: self.f32(this + 0x258),
            indentation: Indentation::from_words(self.f32s(indentation)),
            normal_force,
            carcass_force,
            tread_force,
            coefficients: coefficients_from_words(coefficients),
            tread_engaged: self.u32(state + 0x70),
            carcass_engaged: self.u32(state + 0x74),
        }
    }

    /// Stores the values a contact solve writes: the indentation, the forces and the engaged flags (`this+0x14`).
    fn store_contact(&mut self, this: u32, indentation: u32, forces: u32, c: &Contact) {
        store_floats(self, indentation, &c.indentation.to_words());
        store_floats(self, forces, &[c.normal_force, c.carcass_force, c.tread_force]);
        let state = self.u32(this + STATE);
        self.set_u32(state + 0x70, c.tread_engaged);
        self.set_u32(state + 0x74, c.carcass_engaged);
    }

    /// Loads the hold's inputs: the weights, speed and forces of `this`, the contact frame at `this+0x170`, the state's
    /// active flag and the hold direction (`m_state+0x80`). `dt` is the time step.
    fn load_hold_input(&self, this: u32, dt: f32) -> HoldInput {
        let state = self.u32(this + STATE);
        HoldInput {
            sticking_weight: self.f32(this + 0x3F0),
            active: self.u32(state + 0xC),
            speed: self.f32(this + SPEED),
            normal_force: self.f32(this + 0x314),
            kinetic_friction: self.f32(this + 0x1F8),
            lateral: Vec4::load(self, this + SLIP_MATRIX),
            rolling: Vec4::load(self, this + 0x180),
            normal: Vec4::load(self, this + 0x190),
            direction: Vec4::load(self, state + 0x80),
            dt,
        }
    }

    /// Stores the hold direction the hold leaves (`m_state+0x80`).
    fn store_hold_direction(&mut self, this: u32, direction: Vec4) {
        let state = self.u32(this + STATE);
        direction.store(self, state + 0x80);
    }

    /// Loads the heat step's inputs. The state's indices (`m_state+0xa8`, `+0xac`, `+0xc0`, `+0xc4`) select the
    /// setup entries they name, and `this+0x3c0` selects the sector's temperature and wear. The forces and the time
    /// step are the step's arguments.
    fn load_heat(&self, this: u32, lateral_force: f32, longitudinal_force: f32, dt: f32) -> HeatInput {
        let state = self.u32(this + STATE);
        let def = self.u32(this + DEF);
        let idx = self.u32(this + 0x3C0);
        let wear = self.u32(this + 0x414) + 4 * idx;
        HeatInput {
            lateral_force,
            longitudinal_force,
            dt,
            water_factor: self.f32(this + 0x288),
            lateral_velocity: self.f32(this + SLIP_X),
            long_slip_velocity: self.f32(this + LONG_CONTACT_VEL),
            spin: self.f32(this + SPIN),
            rolling_torque: self.f32(this + 0x3C8),
            contact_pressure: self.f32(this + 0x310),
            speed: self.f32(this + SPEED),
            slip_phase: self.f32(this + 0x388),
            tread_contact_area: self.f32(this + 0x308),
            surface_temperature: self.f32(state + 0xD0 + 12 * idx),
            tread_temperature: self.f32(state + 0x12C),
            carcass_temperature: self.f32(state + 0x130),
            wear_weight: self.f32(state + 0xB0),
            pressure_weight: self.f32(state + 0xC8),
            conductance_a: self.f32(word_at(def + 0x16C, self.u32(state + 0xC0))),
            conductance_b: self.f32(word_at(def + 0x16C, self.u32(state + 0xC4))),
            wear_rate_a: self.f32(word_at(def + 0x1C4, self.u32(state + 0xA8))),
            wear_rate_b: self.f32(word_at(def + 0x1C4, self.u32(state + 0xAC))),
            tread_conductivity: self.f32(def + 0x118),
            capacity: [0x110, 0x11C, 0x128].map(|at| load_factors(self, def + at)),
            wear: self.f32(wear),
        }
    }

    /// Stores the heat step's results: the scratch words, the sector's temperature, the tread and carcass temperatures,
    /// and the wear.
    fn store_heat(&mut self, this: u32, out: &HeatOutput) {
        let state = self.u32(this + STATE);
        let idx = self.u32(this + 0x3C0);
        let wear = self.u32(this + 0x414) + 4 * idx;
        self.set_f32(this + 0x3C4, out.wet_term);
        self.set_f32(this + 0x4E4, out.frictional_power);
        self.set_f32(this + 0x4EC, out.wet_damping);
        self.set_f32(this + 0x4E0, out.surface_heat_loss);
        self.set_f32(this + 0x404, out.abrasion_rate);
        self.set_f32(state + 0xD0 + 12 * idx, out.surface_temperature);
        self.set_f32(state + 0x12C, out.tread_temperature);
        self.set_f32(state + 0x130, out.carcass_temperature);
        self.set_f32(wear, out.wear);
    }

    /// Loads the sector step's inputs for sector `idx`: its surface temperature (`m_state+0xd0+12·idx`), the speed, the
    /// tread temperature (`m_state+0x12c`), the tread's conductivity (`m_def+0x118`) and the capacities of the surface
    /// layer and of the tread (`m_def+0x110`, `+0x11c`).
    fn load_sector_input(&self, this: u32, idx: u32) -> SectorInput {
        let state = self.u32(this + STATE);
        let def = self.u32(this + DEF);
        SectorInput {
            surface_temperature: self.f32(state + 0xD0 + 12 * idx),
            speed: self.f32(this + SPEED),
            tread_temperature: self.f32(state + 0x12C),
            tread_conductivity: self.f32(def + 0x118),
            surface_capacity: load_factors(self, def + 0x110),
            tread_capacity: load_factors(self, def + 0x11C),
        }
    }

    /// Stores the sector step's results: the tread's heat loss (`this+0x4f8`), the sector's surface temperature and the
    /// tread temperature.
    fn store_sector(&mut self, this: u32, idx: u32, out: &SectorOutput) {
        let state = self.u32(this + STATE);
        self.set_f32(this + 0x4F8, out.tread_heat_loss);
        self.set_f32(state + 0xD0 + 12 * idx, out.surface_temperature);
        self.set_f32(state + 0x12C, out.tread_temperature);
    }

    /// Loads the relaxation's inputs: the carcass temperature and the inner air's, the carcass radius, the tread
    /// width and the conductivity between the carcass and the tread, the capacities, the inner air's gas
    /// (`m_state+0x60..0x6c`) and the sector input at the sector being stepped (`m_state+0x1c & 7`).
    fn load_relax(&self, this: u32, dt: f32) -> RelaxInput {
        let state = self.u32(this + STATE);
        let def = self.u32(this + DEF);
        RelaxInput {
            dt,
            carcass_temperature: self.f32(state + 0x130),
            inner_air_temperature: self.f32(state + STATE_TYRE_TEMPERATURE),
            carcass_radius: self.f32(def + DEF_CARCASS_RADIUS),
            tread_width: self.f32(def + DEF_TREAD_WIDTH),
            carcass_conductivity: self.f32(def + 0x124),
            tread_capacity: load_factors(self, def + 0x11C),
            carcass_capacity: load_factors(self, def + 0x128),
            inner_air: load_force_terms(self, state + 0x60),
            sector: self.load_sector_input(this, relax_index(self, state)),
        }
    }

    /// Stores the relaxation's results: the carcass temperature, the sidewalls' heat loss (`this+0x4fc`), the inner
    /// air's gas, and the sector.
    fn store_relax(&mut self, this: u32, out: &RelaxOutput) {
        let state = self.u32(this + STATE);
        self.set_f32(state + 0x130, out.carcass_temperature);
        self.set_f32(this + 0x4FC, out.sidewall_heat_loss);
        store_force_terms(self, state + 0x60, out.inner_air);
        self.store_sector(this, relax_index(self, state), &out.sector);
    }

    /// Loads the friction limits' inputs. `src` holds the stiffnesses at `+0x36c` and `+0x370`, which pass through; the
    /// shares and the scales are the caller's words.
    fn load_friction_limits(&self, this: u32, src: u32, shares: [f32; 2], scales: [f32; 2]) -> LimitsInput {
        let (weight, [cornering_friction, traction_friction, cornering_peak_slip, traction_peak_slip]) =
            load_blend(self, this);
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
            cornering_stiffness: self.f32(src + 0x36C),
            traction_stiffness: self.f32(src + 0x370),
        }
    }

    /// Loads the patch forces' inputs from the scratch at `this`.
    fn load_patch_input(&self, this: u32) -> PatchInput {
        let state = self.u32(this + STATE);
        let def = self.u32(this + DEF);
        let (weight, tables) = load_blend(self, this);
        PatchInput {
            speed: self.f32(this + SPEED),
            lateral_velocity: self.f32(this + SLIP_X),
            longitudinal_velocity: self.f32(this + SLIP_X + 4),
            spin: self.f32(this + SPIN),
            effective_radius: self.f32(state + STATE_EFFECTIVE_RADIUS),
            static_friction: self.f32(this + 0x1E8),
            kinetic_friction: self.f32(this + 0x1F8),
            reference_friction: self.f32(def + DEF_REFERENCE_FRICTION),
            normal_force: self.f32(this + 0x314),
            cornering_stiffness: self.f32(this + 0x38C),
            traction_stiffness: self.f32(this + 0x390),
            inclination_sin: self.f32(this + 0x3AC),
            patch_length: self.f32(this + 0x30C),
            sink_depth: self.f32(this + 0x340),
            weight,
            tables,
        }
    }

    /// Stores the patch forces: `ptrs[0]` the two forces (three floats), `ptrs[1]` zeroed, then the pneumatic trail and
    /// the two slip phases.
    fn store_patch_forces(&mut self, ptrs: [u32; 5], out: &PatchForces) {
        for (i, value) in [out.lateral_force, out.longitudinal_force, 0.0].iter().enumerate() {
            self.set_f32(ptrs[0] + 4 * i as u32, *value);
        }
        Vec4::default().store(self, ptrs[1]);
        self.set_f32(ptrs[2], out.pneumatic_trail);
        self.set_f32(ptrs[3], out.lateral_phase);
        self.set_f32(ptrs[4], out.longitudinal_phase);
    }

    /// Stores the friction limits through the ten output pointers (the stiffnesses first, then the limits in the
    /// struct's order).
    fn store_friction_limits(&mut self, ptrs: [u32; 10], out: &FrictionLimits) {
        self.set_f32(ptrs[0], out.cornering_stiffness);
        self.set_f32(ptrs[1], out.traction_stiffness);
        self.set_f32(ptrs[2], out.lateral_peak);
        self.set_f32(ptrs[3], out.lateral_kinetic);
        self.set_f32(ptrs[4], out.longitudinal_peak);
        self.set_f32(ptrs[5], out.longitudinal_kinetic);
        self.set_f32(ptrs[6], out.lateral_peak_slip);
        self.set_f32(ptrs[7], out.lateral_end_slip);
        self.set_f32(ptrs[8], out.longitudinal_peak_slip);
        self.set_f32(ptrs[9], out.longitudinal_end_slip);
    }

    /// The game's debug flags for `Step`: the word at `g_1659b70 -> +0x24 -> +0x160e0` (`& 0x20`). When it is set, `Step`
    /// copies its `f` debug_words to `this+0x4c8`. The three words are read here, and the test maps them as node windows.
    fn load_step_flag(&self) -> bool {
        let physics = self.u32(0x0165_9B70);
        let def = self.u32(physics + 0x24);
        self.u32(def + 0x160E0) & 0x20 != 0
    }

    /// Stores the output block at `out`.
    fn store_output_block(&mut self, out: u32, b: &OutputBlock) {
        b.edge_a.store(self, out + 0x10);
        b.edge_b.store(self, out + 0x20);
        b.motion.store(self, out + 0x50);
        self.set_f32(out + 0x80, b.mark_a);
        self.set_f32(out + 0x84, b.mark_b);
        self.set_f32(out + 0x88, b.soil_flow);
        self.set_u32(out + 0x90, b.sink_depth_bits);
        self.set_u32(out + 0x94, b.slip_phase_bits);
        self.set_f32(out + 0x98, b.slip_area);
        self.set_u32(out + 0xAC, b.valid);
    }

    /// Loads the sixteen stack words of the tyre step.
    fn load_step(&self, w: &[u32]) -> (StepArgs, StepPtrs) {
        let args = StepArgs {
            spin: f32::from_bits(w[0]),
            dt: f32::from_bits(w[1]),
            hub_height: f32::from_bits(w[2]),
            slip_area_scale: f32::from_bits(w[3]),
            material: self.load_material(w[4]),
            debug_words: std::array::from_fn(|k| self.u32(w[5] + 4 * k as u32)),
            copy_debug_words: self.load_step_flag(),
            normal: Vec4::load(self, w[6]),
            axle: Vec4::load(self, w[8]),
            velocity: Vec4::load(self, w[9]),
            contact_point: Vec4::load(self, w[10]),
            torque_in: Vec4::load(self, w[12]),
        };
        let ptrs =
            StepPtrs { force: w[11], torque: w[12], reaction_torque: w[13], sticking_load: w[14], output_block: w[15] };
        (args, ptrs)
    }

    /// Stores the outputs of the tyre step (the `None` fields are not written).
    fn store_step(&mut self, p: &StepPtrs, out: &StepOut) {
        out.force.store(self, p.force);
        out.torque.store(self, p.torque);
        self.set_f32(p.reaction_torque, out.reaction_torque);
        if let Some(load) = out.sticking_load {
            self.set_f32(p.sticking_load, load);
        }
        if let Some(block) = &out.block {
            self.store_output_block(p.output_block, block);
        }
    }

    /// Loads the surface friction's inputs for the surface classes at the indices `i` and `j` with the wetnesses `a` and
    /// `b`: the setup's base frictions (`+0x2c4`), wet slopes (`+0x2e0`) and wear factors (`+0x2f8`) at those indices, its
    /// design wetness (`+0x2dc`), and the wear of the sector (`m_state+0x134`). The indices are words, so the address is
    /// wrapping, as the original's is.
    fn load_surface_input(&self, this: u32, i: u32, j: u32, a: f32, b: f32) -> SurfaceInput {
        let def = self.u32(this + DEF);
        let state = self.u32(this + STATE);
        let wear = self.u32(this + 0x414);
        SurfaceInput {
            base_first: self.f32(word_at(def + 0x2C4, i)),
            base_second: self.f32(word_at(def + 0x2C4, j)),
            wet_slope_first: self.f32(word_at(def + 0x2E0, i)),
            wet_slope_second: self.f32(word_at(def + 0x2E0, j)),
            wear_factor_first: self.f32(word_at(def + 0x2F8, i)),
            wear_factor_second: self.f32(word_at(def + 0x2F8, j)),
            design_wetness: self.f32(def + 0x2DC),
            wetness_first: a,
            wetness_second: b,
            sector_wear: self.f32(word_at(wear, self.u32(state + 0x134))),
        }
    }

    /// Stores the surface friction through the caller's pointers `o1` and `o2`, in that order.
    fn store_surface_friction(&mut self, o1: u32, o2: u32, out: SurfaceFriction) {
        self.set_f32(o1, out.first);
        self.set_f32(o2, out.second);
    }

    /// Stores the `RelaxStep` results, then `RelaxCount`'s counter: `m_state+0x1c` goes up by one and its dirty flag
    /// (`+0x18`) is set.
    fn store_relax_count(&mut self, this: u32, out: &RelaxOutput) {
        self.store_relax(this, out);
        let state = self.u32(this + STATE);
        let count = self.u32(state + 0x1C);
        ntweak::set_dirty(self, state + 0x10);
        self.set_u32(state + 0x1C, count.wrapping_add(1));
    }

    /// `TyreTail_Get10` (0x5326b0, thiscall): the word at `container+0x10`, the tyre tail of a wheel's container.
    fn load_tyre_tail10(&self, container: u32) -> u32 {
        self.u32(container + 0x10)
    }

    /// `TyreTail_Get14` (0x5326c0, thiscall): the word at `container+0x14`.
    fn load_tyre_tail14(&self, container: u32) -> u32 {
        self.u32(container + 0x14)
    }

    /// `NTweak::CBase::GetName` (0x52db10, thiscall): the name object at `this+4`.
    fn load_name_object(&self, this: u32) -> u32 {
        self.u32(this + 4)
    }

    /// The tyre-noise signal, at `g_signalManager + 0x6268` (`CSignalManager::CMultiSignal1<CSigTyreNoise>`).
    fn noise_signal(&self) -> u32 {
        physics_core::layout::signal_manager(self) + 0x6268
    }
}

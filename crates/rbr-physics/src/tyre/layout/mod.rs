//! Typed views of the memory the tyre functions read and write: the scratch (`CScratch`, `this`), its
//! state (`m_state`, the pointer at `this+0x14`) and the tyre setup (`m_def`, at `this+0x10`). The offsets
//! are the original's (rbr-recomp `notes/physics/tyre.md` 2a-2c, `notes/physics/renames/tyre.csv`). The `load` and
//! `store` functions below are the only code that knows them; the physics works on the typed values.

mod construct;
pub mod entries;
mod frame;
mod frame_inputs;
mod frame_types;
mod kernels;
mod objects;
mod scratch_ctor;

pub use construct::{
    construct_cdef, construct_options, construct_setup, construct_state, construct_tyre_type, copy_def_from,
    set_setup_type,
};
pub use frame::{copy_material, Outside, Table};
pub use frame_inputs::{
    accumulate_input, apply_accumulate, apply_contact, apply_heat, contact_input, friction_input, heat_input,
    sign_input, step_inner_input, table_at,
};
pub use frame_types::{sig, Frame, Scratch, Segment, Setup, StateWords, TableEntry, SCALE_238};
pub use kernels::{
    emit_noise, scaled_sum, value8_clear, word_at, OutputBlock, SignInput, Slip, State, StepArgs, StepOut, StepPtrs,
};
pub use objects::ScratchObj;
pub use scratch_ctor::{
    cbase_headers_17, cbase_headers_3, construct_scratch, create_scratch, delete_container, delete_scratch,
    destroy_scratch, destroy_scratch_body, init_scratch, SCRATCH_SIZE,
};

// The `CScratch` (`this`) words, its state (`m_state`, `this+0x14`) and its setup (`m_def`, `this+0x10`), as the rename
// map (rbr-recomp `notes/physics/renames/tyre.csv`) and rbr-recomp `notes/physics/tyre.md` 2a-2c name them.

/// `this+0x10`: the tyre setup, `m_def` (a CDef; its words are the `DEF_*` consts).
pub const DEF: u32 = 0x10;
/// `this+0x14`: the tyre state, `m_state` (a CState; its words are the `STATE_*` consts).
pub const STATE: u32 = 0x14;
/// `this+0x170`: the 3x3 slip matrix, rows 0x10 bytes apart (`ComputeSlip`: `s = M·dir`).
pub const SLIP_MATRIX: u32 = 0x170;
/// `this+0x340` and `+0x344`: the lerp pair `Accumulate` blends (`blend_hi`, `blend_lo`, inferred).
pub const BLEND_HI: u32 = 0x340;
pub const BLEND_LO: u32 = 0x344;
/// `this+0x360`: the contact-plane slip speed `sqrt(s0² + s1²)`, m/s (`m_speed`).
pub const SPEED: u32 = 0x360;
/// `this+0x364`: `s0² + s1²` (`m_speedSq`).
pub const SPEED_SQ: u32 = 0x364;
/// `this+0x368`, `+0x36c`, `+0x370`: the three slip velocity components (`m_slipX`, `m_slipY`, `m_slipZ`).
pub const SLIP_X: u32 = 0x368;
pub const SLIP_Y: u32 = 0x36C;
pub const SLIP_Z: u32 = 0x370;
/// `this+0x374`: `R·ω + s1`, the longitudinal velocity of the contact point (`m_contactLongVel`).
pub const LONG_CONTACT_VEL: u32 = 0x374;
/// `this+0x378`: the wheel's spin rate ω, rad/s, stored as float bits (`m_spin`).
pub const SPIN: u32 = 0x378;
/// `this+0x37c`: `-(1/R)·s1 - ω` (`m_slipRatioTerm`).
pub const SLIP_RATIO_TERM: u32 = 0x37C;

/// `m_state+0x4c`: the effective radius `R`, m (`m_effectiveRadius`).
pub const STATE_EFFECTIVE_RADIUS: u32 = 0x4C;
/// `m_state+0x58`: the weight of `Accumulate`'s lerp (`m_lerpWeight`).
pub const STATE_LERP_WEIGHT: u32 = 0x58;
/// `m_state+0x64`: the tyre temperature (`m_tyreTemperature`; `HeatStep` subtracts 273.15 from it).
pub const STATE_TYRE_TEMPERATURE: u32 = 0x64;

// The CDef words (`m_def`), named by the tweak keys of `CTyre::CDef` (rbr-recomp `notes/physics/tyre.md` 2c).
/// `m_def+0x1c`: the tweak key `DebugId`, read as the signal owner id.
pub const DEF_DEBUG_ID: u32 = 0x1C;
/// `m_def+0x3c`: the tweak key `ReferenceFriction`.
pub const DEF_REFERENCE_FRICTION: u32 = 0x3C;
/// `m_def+0x4c`: the tweak key `CarcassStiffnessLateral`.
pub const DEF_CARCASS_STIFFNESS_LATERAL: u32 = 0x4C;
/// `m_def+0x5c`: the tweak key `CarcassStiffnessTorsional`.
pub const DEF_CARCASS_STIFFNESS_TORSIONAL: u32 = 0x5C;
/// `m_def+0x7c`: the tweak key `CarcassRadius`.
pub const DEF_CARCASS_RADIUS: u32 = 0x7C;
/// `m_def+0x8c`: the tweak key `TreadDepth`.
pub const DEF_TREAD_DEPTH: u32 = 0x8C;
/// `m_def+0x9c`: the carcass thickness (no tweak key; named by NGP's use, 5 mm in its tyre types).
pub const DEF_CARCASS_THICKNESS: u32 = 0x9C;
/// `m_def+0xac`: the tweak key `TreadWidth`.
pub const DEF_TREAD_WIDTH: u32 = 0xAC;
/// `m_def+0xbc`: the tweak key `TreadCoverage`.
pub const DEF_TREAD_COVERAGE: u32 = 0xBC;
/// `m_def+0xcc`: the tweak key `CarcassWidth`.
pub const DEF_CARCASS_WIDTH: u32 = 0xCC;
/// `m_def+0xec`: the sidewall height, tread surface to rim (no tweak key; named by NGP's use, 0.09 m in its tyre
/// types).
pub const DEF_SIDEWALL_HEIGHT: u32 = 0xEC;

/// The 3x3 slip matrix at `this+0x170..0x19c`: row `r` maps the contact direction to slip component `r`.
pub type SlipMatrix = [[f32; 3]; 3];

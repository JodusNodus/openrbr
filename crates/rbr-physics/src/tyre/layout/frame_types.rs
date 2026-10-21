//! The tyre step's footprint as typed values: the scratch, state and setup words, and the `Frame` that holds them.
//! Field docs give the original's offsets (`+0x..` of the scratch `this`, of `m_state` at `this+0x14`, of `m_def` at
//! `this+0x10`) and the evidence for the name. A name marked "by use" comes from how the step uses the word.

use crate::math::vec::Vec4;

use super::frame::Table;
use crate::tyre::material::Material;

/// A table search result (`Locate`, see `locate.rs`): the two indices and the fraction, stored as three words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub lo: u32,
    pub hi: u32,
    pub frac: f32,
}

/// The contact frame at `+0x170`, `+0x180` and `+0x190` (four lanes each; the w lane is stale). The wheel gives the
/// surface normal and the axle, and `contact_frame::build` completes the frame: `rolling` is `normal × axle`
/// normalised, `lateral` is `rolling × normal`. The slip velocity components are the dot products of the contact
/// velocity with these axes, in the order lateral, longitudinal (rolling), normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactFrame {
    pub lateral: Vec4,
    pub rolling: Vec4,
    pub normal: Vec4,
}

/// The seven words at `+0x32c` that a contact solve reads and writes (`Contact` in `solve.rs` is built from them).
/// The soil is the surface material's layer of depth `soil_thickness`; the tyre is the carcass with its tread.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Indentation {
    /// How far the tyre's outer radius reaches below the surface, m (outer radius minus the hub height).
    pub penetration: f32,
    /// The tread's deflection, m: the root the solve finds.
    pub deflection: f32,
    /// The carcass radius, m (a copy for the solve).
    pub carcass_radius: f32,
    /// The hub height above the surface, m (a copy for the solve).
    pub hub_height: f32,
    /// The tread depth left after wear, m (a copy for the solve).
    pub tread_remaining: f32,
    /// How far the tyre sinks into the soil, m, at most `soil_thickness`.
    pub sink_depth: f32,
    /// The carcass radius minus the hub height minus the tread deflection, m (by use: its sign picks the solve case).
    pub carcass_compression: f32,
}

impl Indentation {
    /// The words as stored, in field order.
    pub fn from_words(w: [f32; 7]) -> Self {
        Self {
            penetration: w[0],
            deflection: w[1],
            carcass_radius: w[2],
            hub_height: w[3],
            tread_remaining: w[4],
            sink_depth: w[5],
            carcass_compression: w[6],
        }
    }

    pub fn to_words(self) -> [f32; 7] {
        [
            self.penetration,
            self.deflection,
            self.carcass_radius,
            self.hub_height,
            self.tread_remaining,
            self.sink_depth,
            self.carcass_compression,
        ]
    }
}

/// The scratch words (`CScratch`, `this`) the tyre step reads and writes. Each field is one address (the doc says
/// which); the overlapping words are one field. The state, the setup and the arguments are separate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scratch {
    /// `+0x160`: the contact point the wheel passes in (by use: the patch edges are this point plus or minus half the
    /// tread width along the lateral axis).
    pub contact_point: Vec4,
    /// `+0x170`: the contact frame.
    pub contact_frame: ContactFrame,
    /// `+0x1a0`: the surface material's words, in `material::WORDS` order. `+0x238` is word 17 (`SOIL_DENSITY`).
    pub material: Material,
    /// `+0x1e4`: the dirty word `SetDirty(+0x1dc)` writes (the tweak object of the static friction).
    pub static_friction_dirty: u32,
    /// `+0x1e8`: `myStatic`, the static friction coefficient of the surface (the peak of the friction curve). The
    /// step scales it by `temperature_grip`, and `contact_patch` blends it with the soil's.
    pub static_friction: f32,
    /// `+0x1f4`: the dirty word of the kinetic friction.
    pub kinetic_friction_dirty: u32,
    /// `+0x1f8`: `myKinetic`, the kinetic friction coefficient (the level the friction curve falls to).
    pub kinetic_friction: f32,
    /// `+0x208`: `myStaticSoil`; `+0x218`: `myKineticSoil`.
    pub static_friction_soil: f32,
    pub kinetic_friction_soil: f32,
    /// `+0x228`: `SinkFactor`: how easily the soil gives way (the tread area divided by it is the soil's stiffness).
    pub sink_factor: f32,
    /// `+0x258`: `SoilThickness`, m.
    pub soil_thickness: f32,
    /// `+0x288`: `WaterFactor`: how wet the surface is.
    pub water_factor: f32,
    /// `+0x298`: `RollingResistance`.
    pub rolling_resistance: f32,
    /// `+0x300`: the lateral deflection as a fraction of the sidewall height, at most 1.
    pub lateral_deflection_ratio: f32,
    /// `+0x304`: hub height over the sine of the inclination, m (1e7 when the sine is under 1e-4); the radius of the
    /// arc the contact moves on under camber (by use: `step` turns the longitudinal velocity into an angle with it).
    pub camber_radius: f32,
    /// `+0x308`: the area of the tread in contact, m².
    pub tread_contact_area: f32,
    /// `+0x30c`: the contact patch's length, m.
    pub patch_length: f32,
    /// `+0x310`: the contact pressure, Pa (the total force over the tread contact area).
    pub contact_pressure: f32,
    /// `+0x314`: the normal force, N.
    pub normal_force: f32,
    /// `+0x318`, `+0x31c`: the carcass's and the tread's share of the vertical force, N (the solve's outputs; both
    /// are the carcass's force when the soil does not give way).
    pub carcass_force: f32,
    pub tread_force: f32,
    /// `+0x324`: the total force over the normal force. `+0x328`: the tread's share of the total force.
    pub total_force_ratio: f32,
    pub tread_force_share: f32,
    /// `+0x32c`: the solve's words.
    pub indentation: Indentation,
    /// `+0x348`: the soil's drag force, N.
    pub soil_drag: f32,
    /// `+0x34c`: the soil's drag along the lateral axis, N (added to the lateral friction force).
    pub lateral_soil_force: f32,
    /// `+0x350`: the soil's longitudinal term (always ±0: the original multiplies it by 0.0).
    pub longitudinal_soil_force: f32,
    /// `+0x354`: the volume the lateral slip sweeps through the soil per second, m³/s.
    pub lateral_soil_flow: f32,
    /// `+0x358`, `+0x35c`: the volume the longitudinal slip moves per second, m³/s, where the slip velocity has the
    /// same sign as the contact point's, and where it has the opposite sign.
    pub soil_flow_aligned: f32,
    pub soil_flow_opposed: f32,
    /// `+0x360`: the slip speed in the contact plane, m/s. `+0x364`: its square.
    pub speed: f32,
    pub speed_sq: f32,
    /// `+0x368`: the contact point's velocity in the contact frame: lateral, longitudinal, normal, m/s.
    pub contact_velocity: [f32; 3],
    /// `+0x374`: the slip velocity along the rolling axis, m/s (`R·ω + v`). `+0x378`: the wheel's spin, rad/s. `+0x37c`:
    /// the spin the contact's longitudinal velocity does not explain, rad/s (`-v/R - ω`).
    pub long_slip_velocity: f32,
    pub spin: f32,
    pub spin_slip: f32,
    /// `+0x380`, `+0x384`: zeroed when the slip speed is at most 1 m/s (by use; nothing in the step reads them).
    pub low_speed_a: f32,
    pub low_speed_b: f32,
    /// `+0x388`: the slip phase, 0 to 2: 1 at the friction peak, 2 sliding.
    pub slip_phase: f32,
    /// `+0x38c`, `+0x390`: the cornering and traction stiffness at the load, times the stiffness factor.
    pub cornering_stiffness: f32,
    pub traction_stiffness: f32,
    /// `+0x394`: the lateral carcass stiffness (`CarcassStiffnessLateral`) times the stiffness factor, N/m.
    /// `+0x398`: the torsional one, N·m/rad, which the step also scales with the speed.
    pub lateral_stiffness: f32,
    pub torsional_stiffness: f32,
    /// `+0x39c`: the lateral force, N. `+0x3a0`: the longitudinal force, N. `+0x3a4`: the torsion torque, N·m.
    pub lateral_force: f32,
    pub longitudinal_force: f32,
    pub torsion_torque: f32,
    /// `+0x3a8`: the hub height above the surface, m (step argument `c`).
    pub hub_height: f32,
    /// `+0x3ac`: the sine of the wheel's inclination to the surface normal (`-normal·axle`). `+0x3b0`: its cosine
    /// (`lateral·axle`).
    pub inclination_sin: f32,
    pub inclination_cos: f32,
    /// `+0x3b4`: the stiffness factor at the air pressure (`StfFct`).
    pub stiffness_factor: f32,
    /// `+0x3b8`: the pneumatic trail, m (by use: the lever of the lateral force about the normal axis).
    pub pneumatic_trail: f32,
    /// `+0x3bc`: the tread depth left after wear, m.
    pub tread_remaining: f32,
    /// `+0x3c0`: the tread sector in contact, 0 to 7.
    pub sector: u32,
    /// `+0x3c4`: `2 + 3·water_factor` (by use: it enters the heat step's damping).
    pub wet_term: f32,
    /// `+0x3c8`: the rolling resistance torque, N·m.
    pub rolling_torque: f32,
    /// `+0x3cc`, `+0x3d0`, `+0x3d4`: zeroed by `step` (by use: an extra force, an extra torque about the axle and a
    /// third term; they only ever add zero).
    pub extra_force: f32,
    pub extra_axle_torque: f32,
    pub extra_term: f32,
    /// `+0x3d8`: step argument `d`, the scale of the slipped-area output.
    pub slip_area_scale: f32,
    /// `+0x3dc`: the weight of the sliding model: 0 at a slip speed of 1 m/s or less, 1 at 2 m/s or more.
    pub sliding_weight: f32,
    /// `+0x3e0`, `+0x3e4`: the lateral and longitudinal slip phases of the sliding model.
    pub lateral_phase: f32,
    pub longitudinal_phase: f32,
    /// `+0x3e8`, `+0x3ec`: the sliding model's deflection and torsion.
    pub sliding_deflection: f32,
    pub sliding_torsion: f32,
    /// `+0x3f0`: the weight of the sticking model (`1 - sliding_weight`).
    pub sticking_weight: f32,
    /// `+0x3f4`, `+0x3f8`: the sticking model's slip phases (2 once it clamps, else 0).
    pub stick_phase_a: f32,
    pub stick_phase_b: f32,
    /// `+0x3fc`, `+0x400`: the sticking model's deflection and torsion.
    pub sticking_deflection: f32,
    pub sticking_torsion: f32,
    /// `+0x404`: the abrasion rate of the heat step (0 below 0.05).
    pub abrasion_rate: f32,
    /// `*(+0x410) + 0x5ac`: the contact angle (read only; the record is not part of the frame).
    pub contact_angle: f32,
    /// `*(+0x414)[0..8]`: the wear of each tread sector, 0 to 1.
    pub wear: [f32; 8],
    /// `+0x420` to `+0x47c`: the tyre-noise signal block: 24 words, read and written as dwords.
    pub signal: [u32; 24],
    /// `+0x480`: 1 when the tyre is in contact. `+0x484`: the sector copy.
    pub in_contact: u32,
    pub sector_copy: u32,
    /// `+0x490`, `+0x4a0`: the contact point and the motion vector of the output block. `+0x4b0`, `+0x4b4`, `+0x4b8`:
    /// the soil flow, the longitudinal slip velocity and the longitudinal velocity, as the block holds them.
    pub contact_point_copy: Vec4,
    pub motion_copy: Vec4,
    pub soil_flow_copy: u32,
    pub long_slip_velocity_copy: u32,
    pub longitudinal_velocity_copy: u32,
    /// `+0x4bc`: the temperature grip factor (the rubber's friction at the tread temperatures, with the wet wear).
    pub temperature_grip: f32,
    /// `+0x4c0`, `+0x4c4`: the lateral and longitudinal friction forces, N.
    pub lateral_friction_force: f32,
    pub longitudinal_friction_force: f32,
    /// `+0x4c8` to `+0x4d0`: the three debug words, copied when the game flag is set.
    pub debug_words: [u32; 3],
    /// `+0x4e0`: the heat the tread surface loses, W. `+0x4e4`: the frictional power, W. `+0x4ec`: the heat step's
    /// damping.
    pub surface_heat_loss: f32,
    pub frictional_power: f32,
    pub wet_damping: f32,
    /// `+0x4e8`: NGP's heat power term (`ngp::inclination`), unused by 1.02.
    pub ngp_heat_power: f32,
    /// `+0x504`, `+0x508`, `+0x50c`: the soil drag, the lateral weight and the furrow ratio, each averaged with the
    /// factor 0.9 (`Accumulate`).
    pub smoothed_soil_drag: f32,
    pub smoothed_drag_weight: f32,
    pub smoothed_furrow_ratio: f32,
}

/// The state words the tyre step reads and writes (`m_state`, at `this+0x14`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateWords {
    /// `+0x0c`: 1 once `step` has run with contact (`sti[3]`); a zero leaves `friction` at once.
    pub active: u32,
    /// `+0x28`: the dirty word `SetDirty(+0x20)` writes.
    pub started_dirty: u32,
    /// `+0x2c`: set on the first step.
    pub started: u32,
    /// `+0x30` to `+0x3c`: the contact point `step` leaves when there is no contact.
    pub last_point: Vec4,
    /// `+0x40`: the lateral deflection of the carcass, m. `+0x44`: its torsion, rad.
    pub lateral_deflection: f32,
    pub torsion_angle: f32,
    /// `+0x48`: how far the outer radius reaches below a surface at a given height (`set_penetration`).
    pub penetration_depth: f32,
    /// `+0x4c`: the effective radius, m.
    pub effective_radius: f32,
    /// `+0x50`: `CarcassStiffnessVertical`, N/m. `+0x54`: `TreadElasticity`.
    pub vertical_stiffness: f32,
    pub tread_elasticity: f32,
    /// `+0x58`: `TreadCoverage` (the fraction of the contact patch that is tread). `+0x5c`: set while the sticking
    /// model clamps to the friction circle.
    pub tread_coverage: f32,
    pub grip_clamped: u32,
    /// `+0x68`: the air pressure in the tyre, Pa (the response of the inner air's gas state at `+0x60`).
    pub air_pressure: f32,
    /// `+0x70`, `+0x74`: the flags the last contact solve left (by use: the tread is loaded, the carcass is
    /// compressed); the next solve starts from them.
    pub tread_engaged: u32,
    pub carcass_engaged: u32,
    /// `+0x80` to `+0x8c`: the hold direction `friction` keeps.
    pub hold_direction: Vec4,
    /// `+0x90` (the pressure search), `+0x9c` (the load search), `+0xa8` (the search at the sector's surface
    /// temperature), `+0xb4` (the search at the tread temperature) and `+0xc0` (the contact pressure search). Each is
    /// `lo`, `hi` and `frac`.
    pub pressure_segment: Segment,
    pub load_segment: Segment,
    pub surface_temperature_segment: Segment,
    pub tread_temperature_segment: Segment,
    pub contact_pressure_segment: Segment,
    /// `+0xd0`: the surface temperature of each tread sector, K, in twelve-byte entries.
    pub surface_temperature: [f32; 8],
    /// The other two words of the eight entries, in order, without the last entry's second word (that is
    /// `tread_temperature`).
    pub sector_spare: [f32; 15],
    /// `+0x12c`: the temperature of the tread under the surface layer, K (the last entry's third word).
    pub tread_temperature: f32,
    /// `+0x130`: the carcass temperature, K.
    pub carcass_temperature: f32,
    /// `+0x134`: the sector `contact_patch` finds.
    pub sector: u32,
}

/// The setup words (`m_def`, `this+0x10`), read only. The names are the tweak keys of the tyre type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    /// `+0x1c`: `DebugId`, the signal's owner id.
    pub debug_id: u32,
    /// `+0x2c`: `PressureRatio`. `+0x3c`: `ReferenceFriction`.
    pub pressure_ratio: f32,
    pub reference_friction: f32,
    /// `+0x4c`: `CarcassStiffnessLateral`. `+0x5c`: `CarcassStiffnessTorsional`.
    pub carcass_stiffness_lateral: f32,
    pub carcass_stiffness_torsional: f32,
    /// `+0x7c`: `CarcassRadius`. `+0x8c`: `TreadDepth`. `+0x9c`: the carcass thickness. `+0xac`: `TreadWidth`.
    /// `+0xbc`: `TreadCoverage`. `+0xcc`: `CarcassWidth`. `+0xec`: `SideWallHeight`.
    pub carcass_radius: f32,
    pub tread_depth: f32,
    pub carcass_thickness: f32,
    pub tread_width: f32,
    pub tread_coverage: f32,
    pub carcass_width: f32,
    pub sidewall_height: f32,
    /// `+0x310`: `WetnessWearFtr`.
    pub wet_wear_factor: f32,
    /// `+0x10c`: NGP's scale of the inclination heat factor (`ngp::inclination::heat_scale`).
    pub inclination_heat_scale: f32,
    /// `+0x110`, `+0x11c`, `+0x128`: the mass and specific heat of the surface layer, the tread under it and the
    /// carcass (`accumulate_a` divides by their product, the heat capacity, J/K).
    pub surface_capacity: [f32; 2],
    pub tread_capacity: [f32; 2],
    pub carcass_capacity: [f32; 2],
    /// `+0x118`: the tread's thermal conductivity, W/(m·K).
    pub tread_conductivity: f32,
    /// The tables, by their entries: `+0x134` (4, the air pressure axis, Pa), `+0x144` (4, `StfFct_P`, the stiffness
    /// factor), `+0x154` (6, the contact pressure axis, Pa), `+0x16c` (6, the contact conductance factor), `+0x184`
    /// (8, the temperature axis, K), `+0x1a4` (8, `RubMyFct_T`), `+0x1c4` (8, `RubWear_T`), `+0x1e4` (8, the load
    /// axis, N), `+0x204` (`CrnStf_L`), `+0x224` (`TrctStf_L`), `+0x244` (`CrnMyFct_L`), `+0x264` (`TrctMyFct_L`),
    /// `+0x284` (`SlpPkCrn_L`) and `+0x2a4` (`SlpPkTrct_L`), 8 each.
    pub pressure_axis: Table,
    pub stiffness_factor_by_pressure: Table,
    pub contact_pressure_axis: Table,
    pub contact_conductance: Table,
    pub temperature_axis: Table,
    pub rubber_friction_by_temperature: Table,
    pub rubber_wear_by_temperature: Table,
    pub load_axis: Table,
    pub cornering_stiffness_by_load: Table,
    pub traction_stiffness_by_load: Table,
    pub cornering_friction_by_load: Table,
    pub traction_friction_by_load: Table,
    pub cornering_peak_slip_by_load: Table,
    pub traction_peak_slip_by_load: Table,
}

/// The whole tyre step footprint: loaded once by `load_frame`, written back once by `store_frame`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub scratch: Scratch,
    pub state: StateWords,
    pub setup: Setup,
}

/// The index of `this+0x238` (`SoilDensity`) in the material block (`material::WORDS`).
pub const SOIL_DENSITY: usize = 17;

/// The words of `Scratch::signal` (the `+0x420` block), named for what `fill_outputs` writes there. Word `k` is `+0x420 +
/// 4k`. The rest are copied by `write_outputs` or read by the signal's owner.
pub mod sig {
    /// `+0x420`: the owner's debug id (raw). `+0x424`: the running maximum. `+0x428`: the force sum times 1000.
    pub const DEBUG_ID: usize = 0;
    pub const MAX: usize = 1;
    pub const FORCE_SUM: usize = 2;
    /// `+0x42c`: `|the slip difference|`. `+0x430`: the scaled maximum. `+0x434`: the normal force bits (raw).
    pub const SLIP_DIFF: usize = 3;
    pub const SCALED_MAX: usize = 4;
    pub const NORMAL_FORCE: usize = 5;
    /// `+0x438`: the constant 320. `+0x43c`: the spin bits (raw). `+0x440`: `|spin slip|`. `+0x444`: the speed bits.
    pub const CONST_320: usize = 6;
    pub const SPIN: usize = 7;
    pub const SPIN_SLIP: usize = 8;
    pub const SPEED: usize = 9;
    /// `+0x450`: the contact point (raw). `+0x460`: the contact velocity Step passes in (raw).
    pub const CONTACT_POINT: usize = 12;
    pub const CONTACT_VELOCITY: usize = 16;
    /// `+0x470..+0x478`: the debug words (raw). `+0x47c`: the flag, 1 once `fill_outputs` has run.
    pub const DEBUG_WORDS: usize = 20;
    pub const FLAG: usize = 23;
}

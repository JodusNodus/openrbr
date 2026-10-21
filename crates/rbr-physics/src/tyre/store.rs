//! The tyre's store (`crate::store`): the layout's load and store groups. `Mem` implements it in `layout/kernels.rs`.

use super::forces::RollingInput;
use super::friction_limits::{FrictionLimits, LimitsInput};
use super::heat::{HeatInput, HeatOutput};
use super::hold::HoldInput;
use super::layout::{ContactAxes, Frame, Segment, StepArgs, StepOut, StepPtrs, Table};
use super::material::Material;
use super::output_block::OutputBlock;
use super::patch_forces::{PatchForces, PatchInput};
use super::relax::{RelaxInput, RelaxOutput, SectorInput, SectorOutput};
use super::slip::Slip;
use super::soil::{SoilDrag, SoilInput};
use super::solve::Contact;
use super::surface::{SurfaceFriction, SurfaceInput};
use crate::math::vec::Vec4;
use crate::store::Raw;

pub trait TyreStore: Raw {
    /// Loads a [`Segment`] at `at`: `lo`, `hi` and `frac` in three words.
    fn load_segment(&self, at: u32) -> Segment;
    /// Stores a [`Segment`] at `at`.
    fn store_segment(&mut self, at: u32, s: &Segment);
    /// The table of `len` entries at `at`, with the word below it.
    fn read_table(&self, at: u32, len: usize) -> Table;
    /// Loads the whole tyre step footprint of the scratch at `this`: its state, its setup, its wear table and the
    /// contact record's angle. The pointers are read from `this` (`+0x10` setup, `+0x14` state, `+0x410` record,
    /// `+0x414` wear table).
    fn load_frame(&self, this: u32) -> Frame;
    /// Stores the tyre step footprint back to the scratch at `this` (its state and scratch; the setup and the contact
    /// angle are read only). Every word is written, with the value it was loaded with unless the port changed it.
    fn store_frame(&mut self, this: u32, frame: &Frame);
    /// Loads the copied words of the block at `at`.
    fn load_material(&self, at: u32) -> Material;
    /// Stores the copied words into the block at `at`; the other words are left as they are.
    fn store_material(&mut self, at: u32, material: &Material);
    /// Loads the effective radius of the tyre at `this`, m.
    fn load_effective_radius(&self, this: u32) -> f32;
    /// Loads the contact frame's axes (`this+0x170`, rows 0x10 bytes apart).
    fn load_contact_axes(&self, this: u32) -> ContactAxes;
    /// Stores the slip.
    fn store_slip(&mut self, this: u32, slip: &Slip);
    /// Loads the rolling resistance torque inputs.
    fn load_rolling_input(&self, this: u32) -> RollingInput;
    /// Loads the soil drag inputs: the scratch words, the state and the tyre setup (`this+0x10`).
    fn load_soil_input(&self, this: u32) -> SoilInput;
    /// Stores the soil drag results.
    fn store_soil_drag(&mut self, this: u32, out: &SoilDrag);
    /// Stores the wear table pointer (`this+0x414`): the eight sector wear words the heat step reads and writes.
    fn store_wear_table(&mut self, this: u32, table: u32);
    /// Loads the values a contact solve reads: the indentation (7 words) at `indentation`, the forces (3 words) at
    /// `forces` and the coefficients (8 words) at `coefficients`, the soil thickness at `this+0x258` and the engaged flags.
    fn load_contact(&self, this: u32, indentation: u32, forces: u32, coefficients: u32) -> Contact;
    /// [`load_contact`](TyreStore::load_contact) with the coefficients given, for a caller whose coefficients are in its frame.
    fn load_contact_with(&self, this: u32, indentation: u32, forces: u32, coefficients: [f32; 8]) -> Contact;
    /// Stores the values a contact solve writes: the indentation, the forces and the engaged flags (`this+0x14`).
    fn store_contact(&mut self, this: u32, indentation: u32, forces: u32, c: &Contact);
    /// Loads the hold's inputs: the weights, speed and forces of `this`, the contact frame at `this+0x170`, the state's
    /// active flag and the hold direction (`m_state+0x80`). `dt` is the time step.
    fn load_hold_input(&self, this: u32, dt: f32) -> HoldInput;
    /// Stores the hold direction the hold leaves (`m_state+0x80`).
    fn store_hold_direction(&mut self, this: u32, direction: Vec4);
    /// Loads the heat step's inputs. The state's indices (`m_state+0xa8`, `+0xac`, `+0xc0`, `+0xc4`) select the
    /// setup entries they name, and `this+0x3c0` selects the sector's temperature and wear. The forces and the time
    /// step are the step's arguments.
    fn load_heat(&self, this: u32, lateral_force: f32, longitudinal_force: f32, dt: f32) -> HeatInput;
    /// Stores the heat step's results: the scratch words, the sector's temperature, the tread and carcass temperatures,
    /// and the wear.
    fn store_heat(&mut self, this: u32, out: &HeatOutput);
    /// Loads the sector step's inputs for sector `idx`: its surface temperature (`m_state+0xd0+12·idx`), the speed, the
    /// tread temperature (`m_state+0x12c`), the tread's conductivity (`m_def+0x118`) and the capacities of the surface
    /// layer and of the tread (`m_def+0x110`, `+0x11c`).
    fn load_sector_input(&self, this: u32, idx: u32) -> SectorInput;
    /// Stores the sector step's results: the tread's heat loss (`this+0x4f8`), the sector's surface temperature and the
    /// tread temperature.
    fn store_sector(&mut self, this: u32, idx: u32, out: &SectorOutput);
    /// Loads the relaxation's inputs: the carcass temperature and the inner air's, the carcass radius, the tread
    /// width and the conductivity between the carcass and the tread, the capacities, the inner air's gas
    /// (`m_state+0x60..0x6c`) and the sector input at the sector being stepped (`m_state+0x1c & 7`).
    fn load_relax(&self, this: u32, dt: f32) -> RelaxInput;
    /// Stores the relaxation's results: the carcass temperature, the sidewalls' heat loss (`this+0x4fc`), the inner
    /// air's gas, and the sector.
    fn store_relax(&mut self, this: u32, out: &RelaxOutput);
    /// Loads the friction limits' inputs. `src` holds the stiffnesses at `+0x36c` and `+0x370`, which pass through; the
    /// shares and the scales are the caller's words.
    fn load_friction_limits(&self, this: u32, src: u32, shares: [f32; 2], scales: [f32; 2]) -> LimitsInput;
    /// Loads the patch forces' inputs from the scratch at `this`.
    fn load_patch_input(&self, this: u32) -> PatchInput;
    /// Stores the patch forces: `ptrs[0]` the two forces (three floats), `ptrs[1]` zeroed, then the pneumatic trail and
    /// the two slip phases.
    fn store_patch_forces(&mut self, ptrs: [u32; 5], out: &PatchForces);
    /// Stores the friction limits through the ten output pointers (the stiffnesses first, then the limits in the
    /// struct's order).
    fn store_friction_limits(&mut self, ptrs: [u32; 10], out: &FrictionLimits);
    /// The game's debug flags for `Step`: the word at `g_1659b70 -> +0x24 -> +0x160e0` (`& 0x20`). When it is set, `Step`
    /// copies its `f` debug_words to `this+0x4c8`. The three words are read here, and the test maps them as node windows.
    fn load_step_flag(&self) -> bool;
    /// Stores the output block at `out`.
    fn store_output_block(&mut self, out: u32, b: &OutputBlock);
    /// Loads the sixteen stack words of the tyre step.
    fn load_step(&self, w: &[u32]) -> (StepArgs, StepPtrs);
    /// Stores the outputs of the tyre step (the `None` fields are not written).
    fn store_step(&mut self, p: &StepPtrs, out: &StepOut);
    /// Loads the surface friction's inputs for the surface classes at the indices `i` and `j` with the wetnesses `a` and
    /// `b`: the setup's base frictions (`+0x2c4`), wet slopes (`+0x2e0`) and wear factors (`+0x2f8`) at those indices, its
    /// design wetness (`+0x2dc`), and the wear of the sector (`m_state+0x134`). The indices are words, so the address is
    /// wrapping, as the original's is.
    fn load_surface_input(&self, this: u32, i: u32, j: u32, a: f32, b: f32) -> SurfaceInput;
    /// Stores the surface friction through the caller's pointers `o1` and `o2`, in that order.
    fn store_surface_friction(&mut self, o1: u32, o2: u32, out: SurfaceFriction);
    /// Stores the `RelaxStep` results, then `RelaxCount`'s counter: `m_state+0x1c` goes up by one and its dirty flag
    /// (`+0x18`) is set.
    fn store_relax_count(&mut self, this: u32, out: &RelaxOutput);
    /// `TyreTail_Get10` (0x5326b0, thiscall): the word at `container+0x10`, the tyre tail of a wheel's container.
    fn load_tyre_tail10(&self, container: u32) -> u32;
    /// `TyreTail_Get14` (0x5326c0, thiscall): the word at `container+0x14`.
    fn load_tyre_tail14(&self, container: u32) -> u32;
    /// `NTweak::CBase::GetName` (0x52db10, thiscall): the name object at `this+4`.
    fn load_name_object(&self, this: u32) -> u32;
    /// The tyre-noise signal, at `g_signalManager + 0x6268` (`CSignalManager::CMultiSignal1<CSigTyreNoise>`).
    fn noise_signal(&self) -> u32;
}

//! The tyre's store (`crate::store`): the layout's load and store groups. `Mem` implements it in `layout/kernels.rs`.

use super::force::{AccumulateInput, Accumulated};
use super::friction::FrictionInput;
use super::heat::{HeatInput, HeatOutput};
use super::layout::{
    Frame, OutputBlock, Segment, SignInput, Slip, SlipMatrix, State, StepArgs, StepOut, StepPtrs, Table,
};
use super::material::Material;
use super::outputs::{ComputeInput, ComputeOutputs};
use super::relax::{BlendInput, BlendOut, RelaxInput, RelaxOutput, TableInput, TableOutput};
use super::solve::Contact;
use super::step_inner::{StepInput, StepOutput};
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
    /// Loads the state the tyre functions read. The state pointer is at `this+0x14`.
    fn load_state(&self, this: u32) -> State;
    /// Loads the slip matrix (`this+0x170`, rows 0x10 bytes apart).
    fn load_slip_matrix(&self, this: u32) -> SlipMatrix;
    /// Stores the `ComputeSlip` results.
    fn store_slip(&mut self, this: u32, slip: &Slip);
    /// Loads the `Sign` inputs.
    fn load_sign_input(&self, this: u32) -> SignInput;
    /// Loads the `Accumulate` inputs: the scratch words, the state and the tyre setup (`this+0x10`, `m_def`).
    fn load_accumulate(&self, this: u32) -> AccumulateInput;
    /// Stores the `Accumulate` results.
    fn store_accumulate(&mut self, this: u32, out: &Accumulated);
    /// Stores the `Set414` value (`CScratch::Set414`, 0x532490).
    fn store_414(&mut self, this: u32, value: u32);
    /// Loads the values a contact solve reads: `p` (7 words), `qb` (3 words) and `r` (8 words) at the given
    /// addresses, the contact offset at `this+0x258` and the state words at `+0x70` and `+0x74`.
    fn load_contact(&self, this: u32, p: u32, qb: u32, r: u32) -> Contact;
    /// `load_contact` with the `r` words given, for a caller whose `r` is in its frame (`ContactPatch`).
    fn load_contact_with(&self, this: u32, p: u32, qb: u32, r: [f32; 8]) -> Contact;
    /// Stores the values a contact solve writes: `p`, `qb` and the state words (`this+0x14`).
    fn store_contact(&mut self, this: u32, p: u32, qb: u32, c: &Contact);
    /// Loads the inputs of `Friction`: the gate, speed and factors of `this`, the vectors at `this+0x170`,
    /// `+0x180`, `+0x190`, the state's flag (`m_state+0xc`) and `sv` (`m_state+0x80`). `k` is its stack argument.
    fn load_friction(&self, this: u32, k: f32) -> FrictionInput;
    /// Stores the state vector `Friction` leaves (`m_state+0x80`).
    fn store_friction(&mut self, this: u32, sv: Vec4);
    /// Loads the inputs of `HeatStep`. The state's indices (`m_state+0xa8`, `+0xac`, `+0xc0`, `+0xc4`) select the
    /// def entries they name, and `this+0x3c0` selects the table entry and the wear word.
    fn load_heat(&self, this: u32, a: f32, b: f32, c: f32) -> HeatInput;
    /// Stores the `HeatStep` results: the scratch words, the table entry and the two damper states, and the wear.
    fn store_heat(&mut self, this: u32, out: &HeatOutput);
    /// Loads the `TableStep` inputs for table index `idx`: the entry at `m_state+0xd0+12·idx`, the speed, the
    /// state's `m_state+0x12c`, the time constant (`m_def+0x118`) and the two damper blocks (`m_def+0x110`, `+0x11c`).
    fn load_table(&self, this: u32, idx: u32) -> TableInput;
    /// Stores the `TableStep` results: `this+0x4f8`, the table entry at `idx`, and `m_state+0x12c`.
    fn store_table(&mut self, this: u32, idx: u32, out: &TableOutput);
    /// Loads the `RelaxStep(dt)` inputs: the carcass and tread damper states, the factors of the spring term
    /// (`m_def+0x7c`, `+0xac`, `+0x124`), the damper blocks, `DamperForce`'s state (`m_state+0x60..0x6c`) and the
    /// `TableStep` input at the tyre index (`m_state+0x1c & 7`).
    fn load_relax(&self, this: u32, dt: f32) -> RelaxInput;
    /// Stores the `RelaxStep` results: `m_state+0x130`, `this+0x4fc`, `DamperForce`'s state, and the table step.
    fn store_relax(&mut self, this: u32, out: &RelaxOutput);
    /// Loads the `ComputeOutputs` inputs. `src` is stack word 6, and its words `+0x36c` and `+0x370` are copied through.
    /// `p1`, `p2`, `p4` and `p5` are stack words 1, 2, 4 and 5.
    fn load_compute_outputs(&self, this: u32, src: u32, p1: f32, p2: f32, p4: f32, p5: f32) -> ComputeInput;
    /// Loads the `StepInner` inputs: the record (`args[0]`), and the CScratch, state and def words the step reads.
    fn load_step_inner(&self, this: u32, rec: u32) -> StepInput;
    /// Stores the `StepInner` results: `args[1]` (three floats), `args[2]` (zeroed, as `E09Vec4::Clear` does), and
    /// `args[3]` to `args[5]`. `ptrs` holds `args[1]` to `args[5]`.
    fn store_step_inner(&mut self, ptrs: [u32; 5], out: &StepOutput);
    /// Stores the `ComputeOutputs` results through the ten output pointers, stack words 7 to 16 (`ptrs[0]` is word 7).
    fn store_compute_outputs(&mut self, ptrs: [u32; 10], out: &ComputeOutputs);
    /// The game's debug flags for `Step`: the word at `g_1659b70 -> +0x24 -> +0x160e0` (`& 0x20`). When it is set, `Step`
    /// copies its `f` pair to `this+0x4c8`. The three words are read here, and the test maps them as node windows.
    fn load_step_flag(&self) -> bool;
    /// Stores the output block at `out`.
    fn store_output_block(&mut self, out: u32, b: &OutputBlock);
    /// Loads the sixteen stack words of `Step` (`a` to `p`).
    fn load_step(&self, w: &[u32]) -> (StepArgs, StepPtrs);
    /// Stores the outputs of `Step` (the `None` fields are not written).
    fn store_step(&mut self, p: &StepPtrs, out: &StepOut);
    /// Loads the inputs of `BlendTable` for the indices `i` and `j` and the fractions `a` and `b`: the def's bases (`+0x2c4`),
    /// slopes (`+0x2e0`) and wear factors (`+0x2f8`) at those indices, its `+0x2dc`, and the wear factor at the tyre index
    /// `m_state+0x134`. The indices are words, so the address is wrapping, as the asm's is.
    fn load_blend_table(&self, this: u32, i: u32, j: u32, a: f32, b: f32) -> BlendInput;
    /// Stores the two outputs of `BlendTable` through the caller's pointers `o1` and `o2`, in that order.
    fn store_blend(&mut self, o1: u32, o2: u32, out: BlendOut);
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

//! The wheel's store (`crate::store`): the layout's loads and stores. `Mem` implements it in `fields.rs`.

use crate::math::vec::Vec4;
use crate::store::Raw;
use crate::wheel::axes::{AxleWobbleIn, HubDamageIn, HubDamageUpdate, Inclination, InclinedAxle, TopMountDeformIn};
use crate::wheel::contact::RayBlock;
use crate::wheel::refresh::SuspensionDef;

pub trait WheelStore: Raw {
    /// Loads the ray block at `at` (the gate word is read first, as the original does).
    fn load_ray_block(&self, at: u32) -> RayBlock;
    /// The tyre tail object of the wheel: the pointer at `this+0xf10`.
    fn tail(&self, this: u32) -> u32;
    /// The tyre's `HubOffset` (`tail+0x6c`).
    fn load_hub_offset(&self, this: u32) -> f32;
    /// The tyre's `CarcassWidth` (`tail+0xcc`).
    fn load_carcass_width(&self, this: u32) -> f32;
    /// The car pointer (`this+0x7f4`).
    fn load_car(&self, this: u32) -> u32;
    /// A wheel vector at `this + offset`.
    fn load_at(&self, this: u32, offset: u32) -> Vec4;
    /// Stores a wheel vector at `this + offset`.
    fn store_at(&mut self, this: u32, offset: u32, v: Vec4);
    /// The CDef pointer (`this+0x34`).
    fn load_def(&self, this: u32) -> u32;
    /// The car `M3A8A0` reads (`this+0x44`), before its flag test.
    fn load_xform_car(&self, this: u32) -> u32;
    /// The flags byte of a car (`car+0x60`).
    fn car_flags(&self, car: u32) -> u8;
    /// A float of the CDef at `off`.
    fn load_def_f32(&self, this: u32, off: u32) -> f32;
    /// The damage record pointer (`this+0x7f0`).
    fn load_block(&self, this: u32) -> u32;
    /// A float of the damage record at `off` (`BLOCK_*`).
    fn load_block_f32(&self, this: u32, off: u32) -> f32;
    /// The wheel's angle of rotation (`this+0x5ac`).
    fn load_angle(&self, this: u32) -> f32;
    /// A float of this at `at` (the wheel's scalars).
    fn load_f32(&self, this: u32, off: u32) -> f32;
    /// Stores a float of this at `off`.
    fn store_f32(&mut self, this: u32, off: u32, value: f32);
    /// Stores a float of the damage record at `off` (`BLOCK_*`).
    fn store_block_f32(&mut self, this: u32, off: u32, value: f32);
    /// Stores an int at `this + off`.
    fn store_u32(&mut self, this: u32, off: u32, value: u32);
    /// A vector of the CDef at `off` (`DEF_*`).
    fn load_def_vec(&self, this: u32, off: u32) -> Vec4;
    /// The CDef the CPhysics global points at.
    fn world_def(&self) -> u32;
    fn load_axle_wobble(&self, this: u32) -> AxleWobbleIn;
    fn store_wobbled_axle(&mut self, this: u32, v: Vec4);
    fn load_inclination(&self, this: u32) -> Inclination;
    fn store_inclined_axle(&mut self, this: u32, axle: &InclinedAxle);
    fn load_top_mount_deform(&self, this: u32) -> TopMountDeformIn;
    fn store_deformed_top_mount(&mut self, this: u32, v: Vec4);
    fn load_hub_damage(&self, this: u32, torque: f32) -> HubDamageIn;
    /// Stores the damage update and sets the changed flag. Nothing is written when `update` is `None`.
    fn store_hub_damage(&mut self, this: u32, update: Option<&HubDamageUpdate>);
    /// The CDef words `M3B900` reads (`this+0x34`).
    fn load_suspension_def(&self, this: u32) -> SuspensionDef;
    /// The debug flag word, `[[0x1659b70] + 0x24] + 0x160e0`: bit 2 is the debug flag `M3B900` tests, bit 0x400 gates the
    /// travel in `M3B880`.
    fn flag_word(&self) -> u32;
    fn debug_flag(&self) -> bool;
}

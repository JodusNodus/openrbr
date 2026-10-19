//! The wheel's store (`crate::store`): the layout's loads and stores. `Mem` implements it in `layout/fields.rs`.

use super::layout::{AngleIn, AxisPair, Frame, InputSet, SetInputIn, StateLerpIn, TurnIn};
use super::refresh::DefPose;
use crate::math::vec::Vec4;
use crate::store::Raw;

pub trait WheelStore: Raw {
    /// Loads the frame record at `at` (the flag word is read first, as the asm does).
    fn load_frame(&self, at: u32) -> Frame;
    /// The tyre tail object of the wheel: the pointer at `this+0xf10`.
    fn tail(&self, this: u32) -> u32;
    /// `tail+0x6c`, the scale `Copy100` multiplies the scaled input by.
    fn load_copy_scale(&self, this: u32) -> f32;
    /// `tail+0xcc`, the scale `Blend4` halves.
    fn load_blend_scale(&self, this: u32) -> f32;
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
    /// The tyre block pointer (`this+0x7f0`).
    fn load_block(&self, this: u32) -> u32;
    /// A float of the tyre block at `off` (`BLOCK_*`).
    fn load_block_f32(&self, this: u32, off: u32) -> f32;
    /// The angle `M3A9E0` reads (`this+0x5ac`).
    fn load_angle(&self, this: u32) -> f32;
    /// A float of this at `at` (the wheel's scalars).
    fn load_f32(&self, this: u32, off: u32) -> f32;
    /// Stores a float of this at `off`.
    fn store_f32(&mut self, this: u32, off: u32, value: f32);
    /// Stores a float of the tyre block at `off` (`BLOCK_*`).
    fn store_block_f32(&mut self, this: u32, off: u32, value: f32);
    /// Stores an int at `this + off`.
    fn store_u32(&mut self, this: u32, off: u32, value: u32);
    /// A vector of the CDef at `off` (`DEF_*`).
    fn load_def_vec(&self, this: u32, off: u32) -> Vec4;
    /// The CDef the CPhysics global points at.
    fn world_def(&self) -> u32;
    fn load_turn(&self, this: u32) -> TurnIn;
    fn store_turn(&mut self, this: u32, v: Vec4);
    fn load_angle_in(&self, this: u32) -> AngleIn;
    fn store_axis_pair(&mut self, this: u32, p: &AxisPair);
    fn load_state_lerp(&self, this: u32) -> StateLerpIn;
    fn store_state_lerp(&mut self, this: u32, v: Vec4);
    fn load_set_input(&self, this: u32, v: f32) -> SetInputIn;
    /// Stores the setter's result and sets the changed flag. Nothing is written when `set` is `None`.
    fn store_set_input(&mut self, this: u32, set: Option<&InputSet>);
    /// The CDef words `M3B900` reads (`this+0x34`).
    fn load_refresh_def(&self, this: u32) -> DefPose;
    /// The debug flag word, `[[0x1659b70] + 0x24] + 0x160e0`: bit 2 is the debug flag `M3B900` tests, bit 0x400 gates the
    /// travel in `M3B880`.
    fn flag_word(&self) -> u32;
    fn debug_flag(&self) -> bool;
}

//! The VCU's store (`crate::store`): the layout's load and store groups. `Mem` implements it in `layout.rs`.

use super::layout::{
    Def, GearRatios, Gearbox, Knot, KnotMaps, Locks, Outputs, Packet, Pedals, Reals, Scratch, SetupFlags, SpeedKnot,
    State,
};
use crate::store::Raw;

pub trait VcuStore: Raw {
    /// Reads a speed map of `n` records at `at`.
    fn speed_map(&self, at: u32, n: usize) -> Vec<SpeedKnot>;
    /// Reads a pedal map of `n` knots at `at`.
    fn pedal_map(&self, at: u32, n: usize) -> Vec<Knot>;
    /// Reads the setup at `at`. The memory must cover the whole object (0xcac bytes).
    fn def(&self, at: u32) -> Def;
    /// Reads the maps of the setup at `setup`.
    fn knot_maps(&self, setup: u32) -> KnotMaps;
    /// Writes the maps into the CDef at `def`.
    fn store_knot_maps(&mut self, def: u32, value: &KnotMaps);
    fn setup_flags(&self, setup: u32) -> SetupFlags;
    fn state(&self, at: u32) -> State;
    fn store_state(&mut self, at: u32, value: &State);
    fn gearbox(&self, at: u32) -> Gearbox;
    fn store_gearbox(&mut self, at: u32, value: &Gearbox);
    /// Gear count of the drive's own definition (`[drive + 4] + 0x654`), which `CDrive::Set47B0` clamps to.
    fn gear_count(&self, drive: u32) -> u32;
    fn reals(&self, drive: u32) -> Reals;
    fn store_reals(&mut self, drive: u32, value: &Reals);
    fn outputs(&self, drive: u32) -> Outputs;
    fn store_outputs(&mut self, drive: u32, value: &Outputs);
    fn gear_ratios(&self, drive: u32) -> GearRatios;
    fn locks(&self, drive: u32) -> Locks;
    /// Writes the locks back. Only the torques change; the ratios and the maximum torques are inputs here.
    fn store_locks(&mut self, drive: u32, value: &Locks);
    fn pedals(&self, scratch: u32) -> Pedals;
    fn store_pedals(&mut self, scratch: u32, value: &Pedals);
    fn packet(&self, at: u32) -> Packet;
    fn scratch(&self, at: u32) -> Scratch;
    fn store_scratch(&mut self, at: u32, value: &Scratch);
}

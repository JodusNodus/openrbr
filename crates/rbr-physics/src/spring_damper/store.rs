//! The spring-damper's store (`crate::store`): the layout's load and store groups. `Mem` implements it in `layout.rs`.

use super::{
    BlendTerms, ClampParameters, Controller, DamperSignal, Definition, FadeState, ForceTerms, HeatAccumulator,
    HeatCapacity, Relaxation, SetupValues, State, UpdateTerms, WeightedValue,
};
use crate::store::Raw;

pub trait SpringStore: Raw {
    /// The controller's definition, state and fade pointers.
    fn definition_at(&self, controller: u32) -> u32;
    fn state_at(&self, controller: u32) -> u32;
    fn fade_at(&self, controller: u32) -> u32;

    fn definition(&self, at: u32) -> Definition;
    /// The nine setup reals a definition adopts (`CopyStateToDef`), and their store into the definition.
    fn setup_values(&self, at: u32) -> SetupValues;
    fn store_setup_values(&mut self, definition_at: u32, definition: &Definition);

    fn state(&self, at: u32) -> State;
    fn store_effective_damping(&mut self, at: u32, value: State);
    fn store_rebound_damping(&mut self, at: u32, value: State);
    /// The signal owner id of the state's car.
    fn owner_id(&self, state_at: u32) -> u32;

    fn controller(&self, at: u32) -> Controller;
    fn store_step_state(&mut self, at: u32, value: Controller);
    fn store_signal(&mut self, at: u32, value: DamperSignal);

    fn fade_state(&self, at: u32) -> FadeState;
    fn store_fade_state(&mut self, at: u32, value: FadeState);

    /// A `CU32Value`'s payload word.
    fn unsigned_value(&self, at: u32) -> u32;
    fn store_unsigned_value(&mut self, at: u32, value: u32);

    // The damper kernels' blocks (in the engine's and the tyre's objects).
    fn load_update_terms(&self, at: u32) -> UpdateTerms;
    fn store_update_terms(&mut self, at: u32, terms: UpdateTerms);
    fn load_blend_terms(&self, at: u32) -> BlendTerms;
    fn store_blend_terms(&mut self, at: u32, value: BlendTerms);
    fn load_relaxation(&self, at: u32) -> Relaxation;
    fn store_relaxation(&mut self, at: u32, value: Relaxation);
    fn load_force_terms(&self, at: u32) -> ForceTerms;
    fn store_force_terms(&mut self, at: u32, value: ForceTerms);
    fn load_heat_capacity(&self, at: u32) -> HeatCapacity;
    fn load_heat_accumulator(&self, at: u32) -> HeatAccumulator;
    fn store_heat_accumulator(&mut self, at: u32, value: HeatAccumulator);
    fn load_clamp_parameters(&self, at: u32) -> ClampParameters;
    fn load_weighted(&self, at: u32) -> WeightedValue;
    fn store_weighted(&mut self, at: u32, value: WeightedValue);
}

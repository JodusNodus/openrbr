//! The surface material's values that the tyre step copies into the scratch.

/// The copied words: 13 values of three words each. The value words are the material's `myStatic`, `myKinetic`,
/// `myStaticSoil`, `myKineticSoil`, `SinkFactor`, `SoilDensity`, `SoilThickness`, `WaterFactor` and
/// `RollingResistance`, and four the tyre step does not read. `layout::WORDS` has their order.
pub type Material = [u32; 39];

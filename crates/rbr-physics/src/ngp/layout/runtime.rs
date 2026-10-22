//! Standalone settings that the original NGP stores in patched executable instructions.
//! Snapshot adapters can still decode those instructions; a boot supplies these values directly.

use std::sync::OnceLock;

/// Reference address space used by standalone bindings, without loading any plugin code.
pub const BASE: u32 = 0x791a_0000;

/// Values owned by one standalone boot, shared by its memory views.
#[derive(Debug)]
pub struct Runtime {
    pub collision_systems: [u32; 3],
    pub head_gasket: Option<[f32; 2]>,
    damage_gate_limit: OnceLock<f32>,
}

impl Runtime {
    pub fn new(collision_systems: [u32; 3], durability: i32) -> Self {
        Self { collision_systems, head_gasket: head_gasket(durability), damage_gate_limit: OnceLock::new() }
    }

    /// Set at the original's per-car initialization point, after the setup has been applied.
    pub(super) fn init_damage_gate(&self, factor: f32) {
        self.damage_gate_limit.set(factor * 11000.0).expect("NGP car initialized once per boot");
    }

    pub fn damage_gate_limit(&self) -> f32 {
        *self.damage_gate_limit.get().expect("NGP car initialized before stepping")
    }
}

/// NGP's stall-draw scale and bias for the configured head-gasket durability.
pub fn head_gasket(durability: i32) -> Option<[f32; 2]> {
    (durability > 1).then(|| {
        let scale = 1.0 / durability as f32;
        [scale, (scale - 1.0) * f32::from_bits(0x3c23_d70a)]
    })
}

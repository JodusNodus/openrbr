//! The contact loop's functions, which are `physics_core::contact_loop` over the store, and the frame and constants they
//! use.

pub use crate::physics_core::contact_loop::{advance_contact_bodies, advance_contact_bodies_remainder, collide_window};

use crate::store::Raw;

/// A collision event's two solids (`+0xa0`, then `+0x98`).
pub const EVENT_SOLIDS_AT: [u32; 2] = [0xa0, 0x98];
/// The solid's slot 0x80 (its vtable entry), which the damper workers ask first.
pub const SLOT_80: u32 = 0x80;
/// The profiler samples of the contact loop (0x38 bytes each from 0x165afc0) and its name.
pub const COLLIDE_SAMPLES: u32 = 0x0165_afc0;
pub const COLLIDE_SAMPLE_SIZE: u32 = 0x38;
pub const COLLIDE_NAME: u32 = 0x0073_f858;

/// The stack words of `CollideWork` (0x526d20): the entry esp, and the esp each call is entered at.
#[derive(Clone, Copy, Debug)]
pub struct CollideStack {
    entry: u32,
}

impl CollideStack {
    pub fn new(entry: u32) -> Self {
        Self { entry }
    }

    /// The profiler channel.
    pub fn channel(self) -> u32 {
        self.entry - 0x14
    }

    /// The channel's `Enter`.
    pub fn enter_sp(self) -> u32 {
        self.entry - 0x40
    }

    /// The detect pass, the telemetry event hook and the workers `Work5640`/`Work5760`.
    pub fn worker_sp(self) -> u32 {
        self.entry - 0x38
    }

    /// The channel's exit, the solver's calls and the entity slots in the pass.
    pub fn call_sp(self) -> u32 {
        self.entry - 0x34
    }

    /// The entity slots after the passes.
    pub fn slot_sp(self) -> u32 {
        self.entry - 0x30
    }

    /// Writes the fraction `CollideWork` leaves in its argument slot (`fst` reuses it for the clamped fraction).
    pub fn record_fraction(self, store: &mut impl Raw, fraction: f32) {
        store.set_f32(self.entry + 4, fraction);
    }
}

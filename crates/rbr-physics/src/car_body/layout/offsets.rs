//! Addresses inside the car's neighbours that the model modules name by role: the signal emitters' words, the signal
//! manager's emitters, the car's members and the damage handler's words.

use crate::physics_core::layout::SIGNAL_MANAGER;
use crate::store::Raw;

/// The emitter's busy word (+0), its source pointer (+4: to the word holding the signal's index), the list of the
/// receivers of every signal (+0x208) and the lists per index (+8, sixteen bytes each).
pub fn emitter_busy_at(emitter: u32) -> u32 {
    emitter
}

pub fn emitter_source_at(emitter: u32) -> u32 {
    emitter + 4
}

pub fn all_receivers_at(emitter: u32) -> u32 {
    emitter + 0x208
}

pub fn index_receivers_at(emitter: u32, index: u32) -> u32 {
    emitter + 8 + 16 * index
}

/// A receiver node's next pointer (+4).
pub fn next_receiver_at(node: u32) -> u32 {
    node + 4
}

/// The signal manager's emitter at `offset`, found through the global pointer.
pub fn signal_at<R: Raw + ?Sized>(mem: &R, offset: u32) -> u32 {
    mem.u32(SIGNAL_MANAGER) + offset
}

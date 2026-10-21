//! The signal emitters the car uses: one template, the same code for every signal type. An emitter holds a pointer to a
//! word that is the signal's index (below 32). The receivers of every signal are one list, the receivers of this index
//! another; each receiver node is called with the signal, which the caller answers. The walk runs the all-signal list,
//! then the index list, and the emitter's busy flag is 1 while it runs. The signal is an address: each type's payload
//! is read by its receivers. An emit while busy logs and returns; an index at or past the limit logs and traps in the
//! original, which the port asserts.

use super::layout::{all_receivers_at, emitter_busy_at, emitter_source_at, index_receivers_at, next_receiver_at};
use crate::car_body::store::CarStore;

/// The signal index limit: the original logs an error at 32 and traps.
const INDEX_LIMIT: u32 = 0x20;

/// The receiver call (one argument: the signal) on a list node, and the log of an emit while busy: answered by the
/// caller.
pub trait Receive {
    fn receive(&mut self, node: u32, signal: u32);
    /// The error log of an emit while the emitter is emitting.
    fn log_recursive(&mut self);
}

/// Emits `signal` on `emitter`. Returns when the emitter has no source, and after the log when it is emitting (the busy
/// flag stays set).
pub fn emit<S: CarStore>(sink: &mut impl Receive, mem: &mut S, emitter: u32, signal: u32) {
    let source = mem.u32(emitter_source_at(emitter));
    if source == 0 {
        return;
    }
    if mem.u32(emitter_busy_at(emitter)) != 0 {
        sink.log_recursive();
        return;
    }
    mem.set_u32(emitter_busy_at(emitter), 1);
    let index = mem.u32(source);
    assert!(index < INDEX_LIMIT, "signal emit: index {index:#x} is out of range");
    for head in [mem.u32(all_receivers_at(emitter)), mem.u32(index_receivers_at(emitter, index))] {
        let mut node = head;
        while node != 0 {
            sink.receive(node, signal);
            node = mem.u32(next_receiver_at(node));
        }
    }
    mem.set_u32(emitter_busy_at(emitter), 0);
}

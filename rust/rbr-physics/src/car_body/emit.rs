//! `CMultiSignal1<T>::Emit`: the signal emitters the car uses (exe 0x4e81b0, 0x4e8270, 0x4e8330, 0x4e83f0, 0x4e84b0,
//! 0x4e88f0, 0x4e8b30, 0x4e8bf0, 0x4e8cb0; one template, the same code in all nine). The emitter `this` holds a pointer at +4
//! to a word that is the signal's index (below 0x20). The receivers of every signal are the list at +0x208; the receivers of
//! this index are the list at +8 + 16 * index. Each list node's next is at +4, and its vtable slot 1 is `Receive(signal)`,
//! which the caller answers. The walk runs the +0x208 list, then the index list, and the emitter's flag at +0 is 1 while
//! it runs. The signal is an address: each `T`'s payload is read by its receivers.

use crate::car_body::store::CarStore;

/// The signal index limit: the original logs an error at 0x20 and goes on (the drive never reaches it; not ported).
const INDEX_LIMIT: u32 = 0x20;

/// `Receive(signal)` on a list node (vtable slot 1, one stack argument): answered by the caller.
pub trait Receive {
    fn receive(&mut self, node: u32, signal: u32);
}

/// `CMultiSignal1<T>::Emit(signal)` on `emitter` (thiscall, `ret 4`). Returns when the source pointer at +4 is null.
pub fn emit<S: CarStore>(sink: &mut impl Receive, mem: &mut S, emitter: u32, signal: u32) {
    let source = mem.u32(emitter + 4);
    if source == 0 {
        return;
    }
    assert_eq!(mem.u32(emitter), 0, "CMultiSignal1::Emit: the emitter is already emitting (the original's error path)");
    mem.set_u32(emitter, 1);
    let index = mem.u32(source);
    assert!(index < INDEX_LIMIT, "CMultiSignal1::Emit: index {index:#x} is out of range");
    for head in [mem.u32(emitter + 0x208), mem.u32(emitter + 8 + 16 * index)] {
        let mut node = head;
        while node != 0 {
            sink.receive(node, signal);
            node = mem.u32(node + 4);
        }
    }
    mem.set_u32(emitter, 0);
}

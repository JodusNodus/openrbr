//! Signal dispatch in the physics core, over the physics core's store. The connections' invoke and the two log
//! messages are the caller's (`SignalHost`).

use super::layout::signal_emit::{OwnerHost, SignalHost};
use super::layout::{signal_overflow, signal_slot};
use super::signal::SLOT_COUNT;
use super::store::CoreStore;

/// Emits `sig` on the signal at `signal`: `sig` goes to each connection of the overflow list, then of the slot the
/// count selects. A null count pointer returns silently. A set guard logs and returns. An out-of-range count logs and
/// returns with the guard still set.
pub fn emit<S: CoreStore>(store: &mut S, host: &mut impl SignalHost<S>, signal: u32, sig: u32) {
    let count_at = store.signal_count_at(signal);
    if count_at == 0 {
        return;
    }
    if store.signal_guard(signal) != 0 {
        host.log_recursive();
        return;
    }
    store.set_signal_guard(signal, 1);
    let count = store.signal_count(count_at);
    if count >= SLOT_COUNT {
        host.log_range(count);
        return;
    }
    let slot = signal_slot(signal, count);
    let overflow = signal_overflow(signal);
    if store.list_head(slot) != 0 || store.list_head(overflow) != 0 {
        for list in [overflow, slot] {
            let mut conn = store.list_head(list);
            while conn != 0 {
                host.invoke(store, conn, sig);
                conn = store.link_next(conn);
            }
        }
    }
    store.set_signal_guard(signal, 0);
}

/// The pad/exit notify on the connection at `conn`: the connection calls its owner with the signal. Both notifies
/// are the same code.
pub fn notify_owner<S: CoreStore>(store: &mut S, host: &mut impl OwnerHost<S>, conn: u32, sig: u32) {
    let owner = store.conn_owner(conn);
    host.call_owner(store, owner, sig);
}

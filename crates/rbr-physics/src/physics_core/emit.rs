//! Signal dispatch in the physics core (`RBR_E46_Conn.cpp`): `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580), over
//! the physics core's store. The connections' `Invoke` and the two `E01Log` messages are the caller's (`SignalHost`).

use super::layout::signal_emit::{OwnerHost, SignalHost};
use super::layout::{signal_overflow, signal_slot};
use super::signal::SLOT_COUNT;
use super::store::CoreStore;

/// `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580) on the signal at `signal`: `sig` goes to each connection of
/// the overflow list, then of the slot the count selects. A null count pointer returns silently. A set guard logs and
/// returns. An out-of-range count logs and returns with the guard still set; the original traps there (`traps`).
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
    let count = store.u32(count_at);
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

/// `E46FuncConn::NotifyPad` (0x527b40) and `NotifyExitGame` (0x527b90), which are the same code: the connection at `conn`
/// calls its owner (+0x10) with the signal.
pub fn notify_owner<S: CoreStore>(store: &mut S, host: &mut impl OwnerHost<S>, conn: u32, sig: u32) {
    let owner = store.conn_owner(conn);
    host.call_owner(store, owner, sig);
}

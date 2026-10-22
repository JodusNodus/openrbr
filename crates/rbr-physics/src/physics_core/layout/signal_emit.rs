//! Signal dispatch in the physics core (`RBR_E46_Conn.cpp`): `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580).
//! The connections' `Invoke` and the two `E01Log` messages are the caller's (`SignalHost`).

use super::{
    conn_owner, link_next, list_head, set_signal_guard, signal_count_at, signal_guard, signal_overflow, signal_slot,
};
use crate::mem::Mem;
use crate::physics_core::signal::SLOT_COUNT;

/// The caller's side of `Emit`, in the original's call order.
pub trait SignalHost {
    /// `Invoke(sig)` on the connection `conn` (vtable slot 1, thiscall).
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32);
    /// `E01Log(6, 0, ...)`: an emission while the signal is already emitting.
    fn log_recursive(&mut self);
    /// `E01Log(7, 0, ...)`: the slot count is out of range.
    fn log_range(&mut self, count: u32);
}

/// `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580) on the signal at `signal`: `sig` goes to each connection of
/// the overflow list, then of the slot the count selects. A null count pointer returns silently. A set guard logs and
/// returns. An out-of-range count logs and returns with the guard still set; the original traps there (`traps`).
pub fn emit(mem: &mut Mem, host: &mut impl SignalHost, signal: u32, sig: u32) {
    let count_at = signal_count_at(mem, signal);
    if count_at == 0 {
        return;
    }
    if signal_guard(mem, signal) != 0 {
        host.log_recursive();
        return;
    }
    set_signal_guard(mem, signal, 1);
    let count = mem.u32(count_at);
    if count >= SLOT_COUNT {
        host.log_range(count);
        return;
    }
    let slot = signal_slot(signal, count);
    let overflow = signal_overflow(signal);
    if list_head(mem, slot) != 0 || list_head(mem, overflow) != 0 {
        for list in [overflow, slot] {
            let mut conn = list_head(mem, list);
            while conn != 0 {
                host.invoke(mem, conn, sig);
                conn = link_next(mem, conn);
            }
        }
    }
    set_signal_guard(mem, signal, 0);
}

/// The caller's side of a connection's owner call: the owner is a cdecl callback with the signal.
pub trait OwnerHost {
    /// `owner(sig)` (cdecl, no `this`).
    fn call_owner(&mut self, mem: &mut Mem, owner: u32, sig: u32);
}

/// `E46FuncConn::NotifyPad` (0x527b40) and `NotifyExitGame` (0x527b90), which are the same code: the connection at `conn`
/// calls its owner (+0x10) with the signal.
pub fn notify_owner(mem: &mut Mem, host: &mut impl OwnerHost, conn: u32, sig: u32) {
    let owner = conn_owner(mem, conn);
    host.call_owner(mem, owner, sig);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNAL: u32 = 0x1000;
    const COUNT: u32 = 0x1400;

    /// Records the caller's calls in order.
    #[derive(Default)]
    struct Log(Vec<String>);

    impl SignalHost for Log {
        fn invoke(&mut self, _: &mut Mem, conn: u32, sig: u32) {
            self.0.push(format!("invoke {conn:#x} {sig:#x}"));
        }

        fn log_recursive(&mut self) {
            self.0.push("recursive".to_string());
        }

        fn log_range(&mut self, count: u32) {
            self.0.push(format!("range {count}"));
        }
    }

    /// A signal (0x218 bytes) whose slot count is `count`, with no connections yet.
    fn signal(count: u32) -> Mem {
        let mut mem = Mem::new();
        mem.map(SIGNAL, &[0; 0x218]);
        mem.map(COUNT, &[0; 4]);
        mem.set_u32(SIGNAL + 4, COUNT);
        mem.set_u32(COUNT, count);
        mem
    }

    /// Links the connections (0x18 bytes each, next link at +4) into the list at `head`, in order.
    fn link(mem: &mut Mem, head: u32, conns: &[u32]) {
        mem.set_u32(head, conns.first().copied().unwrap_or(0));
        for (i, &conn) in conns.iter().enumerate() {
            mem.map(conn, &[0; 0x18]);
            mem.set_u32(conn + 4, conns.get(i + 1).copied().unwrap_or(0));
        }
    }

    #[test]
    fn overflow_goes_before_the_slot() {
        let mut mem = signal(3);
        link(&mut mem, SIGNAL + 8 + 16 * 3, &[0x2000, 0x2100]);
        link(&mut mem, SIGNAL + 0x208, &[0x3000]);
        let mut log = Log::default();
        emit(&mut mem, &mut log, SIGNAL, 0x4000);
        assert_eq!(log.0, ["invoke 0x3000 0x4000", "invoke 0x2000 0x4000", "invoke 0x2100 0x4000"]);
        assert_eq!(mem.u32(SIGNAL), 0);
    }

    #[test]
    fn empty_lists_call_nothing() {
        let mut mem = signal(0);
        let mut log = Log::default();
        emit(&mut mem, &mut log, SIGNAL, 0x4000);
        assert!(log.0.is_empty());
        assert_eq!(mem.u32(SIGNAL), 0);
    }

    #[test]
    fn null_count_pointer_returns_silently() {
        let mut mem = signal(0);
        mem.set_u32(SIGNAL + 4, 0);
        let mut log = Log::default();
        emit(&mut mem, &mut log, SIGNAL, 0x4000);
        assert!(log.0.is_empty());
        assert_eq!(mem.u32(SIGNAL), 0);
    }

    #[test]
    fn set_guard_logs_and_returns() {
        let mut mem = signal(0);
        mem.set_u32(SIGNAL, 1);
        let mut log = Log::default();
        emit(&mut mem, &mut log, SIGNAL, 0x4000);
        assert_eq!(log.0, ["recursive"]);
        assert_eq!(mem.u32(SIGNAL), 1);
    }

    #[test]
    fn out_of_range_count_keeps_the_guard() {
        let mut mem = signal(0x20);
        let mut log = Log::default();
        emit(&mut mem, &mut log, SIGNAL, 0x4000);
        assert_eq!(log.0, ["range 32"]);
        assert_eq!(mem.u32(SIGNAL), 1);
    }
}

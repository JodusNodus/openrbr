//! Signal dispatch in the physics core (`RBR_E46_Conn.cpp`): `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580).
//! The connections' `Invoke` and the two `E01Log` messages are the caller's (`SignalHost`).

use super::layout;
use crate::mem::Mem;

/// The slot count's limit: a `CMultiSignal1` has 32 slots.
const SLOTS: u32 = 0x20;

/// The caller's side of `Emit`, in the original's call order.
pub trait SignalHost {
    /// `Invoke(sig)` on the connection `conn` (vtable slot 1, thiscall).
    fn invoke(&mut self, mem: &mut Mem, conn: u32, sig: u32);
    /// `E01Log(6, 0, ...)`: an emission while the signal is already emitting.
    fn log_recursive(&mut self);
    /// `E01Log(7, 0, ...)`: the slot count is out of range.
    fn log_range(&mut self, count: u32);
}

/// Whether `emit` returns without logging, from the signal's count pointer, its guard and its count (`layout::emits_quietly`
/// reads them): it either has no slots or runs. A guard that is set or a count out of range is the original's `E01Log`,
/// which the caller's host answers (a binding declines it instead).
pub fn quiet(count_at: u32, guard: u32, count: u32) -> bool {
    count_at == 0 || (guard == 0 && count < SLOTS)
}

/// `CMultiSignal1<const CSigLocalTime&>::Emit` (0x526580) on the signal at `signal`: `sig` goes to each connection of
/// the overflow list, then of the slot the count selects. A null count pointer returns silently. A set guard logs and
/// returns. An out-of-range count logs and returns with the guard still set, as the original does.
pub fn emit(mem: &mut Mem, host: &mut impl SignalHost, signal: u32, sig: u32) {
    let count_at = layout::signal_count_at(mem, signal);
    if count_at == 0 {
        return;
    }
    if layout::signal_guard(mem, signal) != 0 {
        host.log_recursive();
        return;
    }
    layout::set_signal_guard(mem, signal, 1);
    let count = mem.u32(count_at);
    if count >= SLOTS {
        host.log_range(count);
        return;
    }
    let slot = layout::signal_slot(signal, count);
    let overflow = layout::signal_overflow(signal);
    if layout::list_head(mem, slot) != 0 || layout::list_head(mem, overflow) != 0 {
        for list in [overflow, slot] {
            let mut conn = layout::list_head(mem, list);
            while conn != 0 {
                host.invoke(mem, conn, sig);
                conn = layout::link_next(mem, conn);
            }
        }
    }
    layout::set_signal_guard(mem, signal, 0);
}

/// The caller's side of a connection's owner call: the owner is a cdecl callback with the signal.
pub trait OwnerHost {
    /// `owner(sig)` (cdecl, no `this`).
    fn call_owner(&mut self, mem: &mut Mem, owner: u32, sig: u32);
}

/// `E46FuncConn::NotifyPad` (0x527b40) and `NotifyExitGame` (0x527b90), which are the same code: the connection at `conn`
/// calls its owner (+0x10) with the signal.
pub fn notify_owner(mem: &mut Mem, host: &mut impl OwnerHost, conn: u32, sig: u32) {
    let owner = layout::conn_owner(mem, conn);
    host.call_owner(mem, owner, sig);
}

/// The registry entry `CPhysics::E46_Notify67d0` (0x5267d0) makes a placement's object from: the placement's object
/// type, unless its collision grid's name (`+0x684`, `None` without a grid or a name) holds `CONE` (0x19) or else
/// `BANNER_STAND` (0x15).
pub fn stuff_kind(object_type: u32, grid_name: Option<&[u8]>) -> u32 {
    let holds = |name: &[u8], part: &[u8]| name.windows(part.len()).any(|w| w == part);
    match grid_name {
        Some(name) if holds(name, b"CONE") => 0x19,
        Some(name) if holds(name, b"BANNER_STAND") => 0x15,
        _ => object_type,
    }
}

/// How `CState::RegWall` (0x5263d0) files an object: the answers of its slots 0x8c, 0x80 and 0x88.
pub type Filing = (bool, bool, bool);

/// What `E37SigEmit9B30` (0x4f9b30), the first thing `E46_Notify67d0` does with a placement, does with its descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshEmit {
    /// A static placement (its flags' bits 0x30 clear, or 0x20 alone): nothing.
    None,
    /// A dynamic one with 0x10: the collision-mesh signal on its descriptor.
    Emit,
    /// A dynamic one with 0x30: first the next free collision id and flag 4 on its descriptor, then the signal.
    NewId,
}

pub fn mesh_emit(flags: u32) -> MeshEmit {
    match flags & 0x30 {
        0x10 => MeshEmit::Emit,
        0x30 => MeshEmit::NewId,
        _ => MeshEmit::None,
    }
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

    #[test]
    fn stuff_kind_takes_cones_and_banner_stands_by_their_grids_name() {
        assert_eq!(stuff_kind(7, None), 7);
        assert_eq!(stuff_kind(7, Some(b"POLE")), 7);
        assert_eq!(stuff_kind(7, Some(b"X_TRAFFIC_CONE_1")), 0x19);
        assert_eq!(stuff_kind(7, Some(b"BANNER_STAND_A")), 0x15);
    }
}

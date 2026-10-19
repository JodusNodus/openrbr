//! `CPhysics::Vf10` (`src/rbr/RBR_E46_Physics.cpp`, 0x524f80): the per-step debug hook. Its tools are a keyboard
//! camera (`GetAsyncKeyState`), the orbit and its sin/cos, a collision probe (`E45_M40C0`), wall visitors and the
//! debug draws. They run only when a debug flag of the CDef's `DbgFlags` (+0x160e0) is set. This port does not run
//! them; it covers the case where none is set, and `is_idle` says which case a call is.

/// The `DbgFlags` bits that make `Vf10` run a tool: the orbit (`0x2000000`, sin/cos and the keyboard), the second
/// keyboard block (`0x4000000`), the collision probe (`0x2`), the wall visitor (`0x800`), the draw (`0x1000`), bit 15
/// (`(char)(flags >> 8) < 0`, a debug draw) and `0x10000` (`E38_004FE4C0`).
pub const TOOL_FLAGS: u32 =
    0x0200_0000 | 0x0400_0000 | 0x0000_0002 | 0x0000_0800 | 0x0000_1000 | 0x0000_8000 | 0x0001_0000;

/// True when `CPhysics::Vf10` does nothing for these `DbgFlags`: no tool gate is set. The port covers exactly these calls.
pub fn is_idle(dbg_flags: u32) -> bool {
    dbg_flags & TOOL_FLAGS == 0
}

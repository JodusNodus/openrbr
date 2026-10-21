//! The physics step's per-step debug hook. Its tools are a keyboard camera, the orbit camera, a collision probe, wall
//! visitors and debug draws. They run only when one of the physics definition's debug flags is set. This port does not
//! run them; it covers the case where none is set, and `is_idle` says which case a call is.

/// The debug flag bits that make the hook run a tool: the orbit camera (bit 25), the second keyboard block (bit 26),
/// the collision probe (bit 1), the wall visitor (bit 11), the draw (bit 12), the debug draw (bit 15) and bit 16.
pub const TOOL_FLAGS: u32 = 1 << 25 | 1 << 26 | 1 << 1 | 1 << 11 | 1 << 12 | 1 << 15 | 1 << 16;

/// True when the hook does nothing for these debug flags: no tool bit is set. The port covers exactly these calls.
pub fn is_idle(dbg_flags: u32) -> bool {
    dbg_flags & TOOL_FLAGS == 0
}

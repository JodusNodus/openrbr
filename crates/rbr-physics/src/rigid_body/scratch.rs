//! `CSolid::E43Vf74` (slot 29, 0x529ea0): the scratch accumulate. The body's scale (`+0xd0`) times the vector `pp` is
//! added to the scratch (`+0x310`), lane by lane (`mulps`, then `addps`). When the gate word has bit 0x4000 the call
//! runs the body's slot 0 (`CEntity::Vf00`, 0x519950: a read that returns the name, so it changes nothing), which the
//! host makes. The gate is the caller's: the original reads `q` and `b` from the app globals and tests `(q | b)`.

use super::host::HostUnavailable;
use super::layout::{Body, Vec4};

/// The gate bit that runs the body's slot 0.
pub const FLUSH_BIT: u32 = 0x4000;

/// The body's slot-0 call the gate runs. The original's result is not used.
pub trait FlushHost {
    fn flush(&mut self) -> Result<(), HostUnavailable>;
}

/// `E43Vf74`: adds `pp * scratch_scale` to the scratch, then runs the flush when `gate` has `FLUSH_BIT`. A host error
/// stops the call after the scratch is written, as the original's calls before it would have.
pub fn accumulate(body: &mut Body, pp: Vec4, gate: u32, host: &mut impl FlushHost) -> Result<(), HostUnavailable> {
    let k = body.scratch_scale;
    let s = body.scratch;
    body.scratch = Vec4 { x: s.x + pp.x * k, y: s.y + pp.y * k, z: s.z + pp.z * k, w: s.w + pp.w * k };
    if gate & FLUSH_BIT != 0 {
        host.flush()?;
    }
    Ok(())
}

//! The CEntity slot that dispatches to the entity's own slots, gated by the global flags (`CEntity::E43VfD4`, 0x519ae0).
//! The memory it reads (the flags) is in `layout`; the dispatch is here.

/// The calls `CEntity::E43VfD4` makes on the entity `this`. The flags are read at each test, since the calls may change
/// them. The callees get the arguments `(a, c, e, f, g)` as the original pushes them (`a` and `c` are the addresses of
/// the caller's argument slots, which the callees may write).
pub trait D4Calls {
    /// The flags object (`g_1659b70`'s +0x24) and its flag byte (+0x160cc).
    fn flags(&mut self) -> (u32, u8);
    /// Slot 0xd8 (`VfD8`) on `this`; its eax.
    fn vf_d8(&mut self, args: [u32; 5]) -> u32;
    /// Slot 0x114 (`Vf114`) on `e`; its eax.
    fn vf114(&mut self, e: u32) -> u32;
    /// Slot 0xdc (`VfDC`) on `this`; its eax.
    fn vf_dc(&mut self, args: [u32; 5]) -> u32;
}

/// `CEntity::E43VfD4` (0x519ae0) with the arguments `[a, c, e, f, g]`: flag 1 calls `VfD8`, and its nonzero answer ends the
/// call; flag 2 calls `Vf114` on `e`, and a nonzero answer calls `VfDC`. Returns eax as the original leaves it.
pub fn vf_d4(calls: &mut impl D4Calls, args: [u32; 5]) -> u32 {
    if calls.flags().1 & 1 != 0 {
        let eax = calls.vf_d8(args);
        if eax != 0 {
            return eax;
        }
    }
    let (flags, bits) = calls.flags();
    if bits & 2 == 0 {
        return flags;
    }
    if calls.vf114(args[2]) == 0 {
        return 0;
    }
    calls.vf_dc(args)
}

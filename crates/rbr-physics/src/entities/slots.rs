//! The entity slot that dispatches to the entity's own slots, gated by the global flags. The memory it reads (the flags)
//! is in `layout`; the dispatch is here.

/// The calls the dispatch makes on the entity. The flags are read at each test, since the calls may change them. The callees
/// get the arguments `[a, c, e, f, g]` (`a` and `c` are the addresses of the caller's argument slots, which the callees may
/// write).
pub trait FlagDispatch {
    /// The flags object and its flag byte.
    fn flags(&mut self) -> (u32, u8);
    /// The first call (flag bit 1) on the entity; its result.
    fn first_call(&mut self, args: [u32; 5]) -> u32;
    /// The check (flag bit 2) on `e`; its result.
    fn check(&mut self, e: u32) -> u32;
    /// The second call (flag bit 2, when the check is nonzero) on the entity; its result.
    fn second_call(&mut self, args: [u32; 5]) -> u32;
}

/// The dispatch with the arguments `[a, c, e, f, g]`: flag bit 1 makes the first call, and its nonzero answer ends the call;
/// flag bit 2 makes the check on `e`, and a nonzero answer makes the second call. Returns what the original returns.
pub fn dispatch_by_flags(calls: &mut impl FlagDispatch, args: [u32; 5]) -> u32 {
    if calls.flags().1 & 1 != 0 {
        let eax = calls.first_call(args);
        if eax != 0 {
            return eax;
        }
    }
    let (flags, bits) = calls.flags();
    if bits & 2 == 0 {
        return flags;
    }
    if calls.check(args[2]) == 0 {
        return 0;
    }
    calls.second_call(args)
}

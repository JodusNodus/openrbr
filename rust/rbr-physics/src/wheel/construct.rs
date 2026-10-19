//! `E51Wheel::M3D1C0` (0x53d1c0, thiscall, `ret 8`): allocates the wheel's 0x1420-byte sub-object and constructs it from
//! the two arguments. The original's SEH frame restores FS:[0] on both paths and leaves nothing else, so the port has no
//! part for it.

/// `operator new(0x1420)`'s size.
const OBJECT_SIZE: u32 = 0x1420;

/// The calls `M3D1C0` makes: `operator new` (cdecl, 0x70bec2) and the sub-object's constructor (thiscall, 0x53b6e0, with
/// four stack words: the outer object, the two arguments, and 0).
pub trait ConstructCalls {
    /// `operator new(size)`: the block's address, or 0 when the allocation fails.
    fn allocate(&mut self, size: u32) -> u32;
    /// The constructor (0x53b6e0) on the new object `obj`; the result is the eax it returns.
    fn construct(&mut self, obj: u32, outer: u32, a: u32, b: u32) -> u32;
}

/// `E51Wheel::M3D1C0`: a failed allocation returns 0 and constructs nothing; otherwise the constructor's result.
pub fn construct(host: &mut impl ConstructCalls, outer: u32, a: u32, b: u32) -> u32 {
    let obj = host.allocate(OBJECT_SIZE);
    if obj == 0 {
        return 0;
    }
    host.construct(obj, outer, a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records the calls, and answers the allocation with `block`.
    struct Log {
        block: u32,
        calls: Vec<String>,
    }

    impl ConstructCalls for Log {
        fn allocate(&mut self, size: u32) -> u32 {
            self.calls.push(format!("new {size:#x}"));
            self.block
        }

        fn construct(&mut self, obj: u32, outer: u32, a: u32, b: u32) -> u32 {
            self.calls.push(format!("ctor {obj:#x} {outer:#x} {a:#x} {b:#x}"));
            0x77
        }
    }

    #[test]
    fn constructs_the_new_block() {
        let mut log = Log { block: 0x9000, calls: Vec::new() };
        assert_eq!(construct(&mut log, 0x1000, 2, 3), 0x77);
        assert_eq!(log.calls, ["new 0x1420", "ctor 0x9000 0x1000 0x2 0x3"]);
    }

    #[test]
    fn failed_allocation_returns_zero() {
        let mut log = Log { block: 0, calls: Vec::new() };
        assert_eq!(construct(&mut log, 0x1000, 2, 3), 0);
        assert_eq!(log.calls, ["new 0x1420"]);
    }
}

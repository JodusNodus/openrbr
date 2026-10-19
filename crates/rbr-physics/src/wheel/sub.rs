//! `CWheel::Unk97a0` (0x5397a0): the two sub-objects at `+0xd00` and `+0xc90`, each called with the vector the wheel
//! builds. The edge is `layout::update_sub_objects`; its one host call is `SubCalls::sub_unk94d0`.

use crate::math::vec::Vec4;

/// The call `CWheel::Unk97a0` makes on a gated-on sub-object.
pub trait SubCalls {
    /// `CWheel::Unk94d0` (0x5394d0) as the asm calls it: `wheel` is the `this` register, the first stack word is the
    /// sub-object `sub`, then `v` (a stack vector), `a`, `b` and `c`. Working name `sub_unk94d0`.
    fn sub_unk94d0(&mut self, wheel: u32, sub: u32, v: Vec4, a: u32, b: u32, c: u32);
}

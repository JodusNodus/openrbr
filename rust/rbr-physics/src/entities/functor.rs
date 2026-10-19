//! The unary functors `CPlainUnaryFunction::Vf04x` (0x51a120) and `Vf00x` (0x51a130), the same code: the argument is
//! pushed, the callback stored at `this+4` is called with `this` in ecx, and the caller pops the argument. The callback
//! `E43_Tail40` (0x5194a0) is a tail jump to the argument's slot 16, with the argument as `this`; the port makes that
//! call itself, one frame up, so the pure step has no tail call.

/// `E43_Tail40` (0x5194a0): the callback that jumps to its argument's slot 16.
pub const TAIL40: u32 = 0x5194a0;
/// Byte offset of slot 16 in a vtable.
const SLOT16: u32 = 0x40;

/// The reads and calls the functor makes.
pub trait Calls {
    fn word(&self, at: u32) -> u32;
    /// A call of `callee` with ecx `this` and one argument; returns eax.
    fn call(&mut self, callee: u32, this: u32, arg: u32) -> u32;
}

/// `CPlainUnaryFunction::Vf04x` (and `Vf00x`): calls the stored callback with `arg`; returns the callee's eax.
pub fn call_stored(calls: &mut impl Calls, this: u32, arg: u32) -> u32 {
    let callback = calls.word(this + 4);
    if callback == TAIL40 {
        let slot16 = calls.word(calls.word(arg) + SLOT16);
        calls.call(slot16, arg, arg)
    } else {
        calls.call(callback, this, arg)
    }
}

//! `CHingedRigidBody::SimE4F0` (0x51e4f0, vtable slot 0x138). With no owner car (`+0x3d0`) it forwards its six arguments
//! to `CSolid::E43Vf138` (`rigid_body::layout::e43vf138`). With a car it runs, for `a`, `b` and `c` in turn, the car's
//! slot 12 into a buffer (the pointer `u`), the body's slot 12 with `(v, u)` (the walker `x`), then `ComposeA` for `a` and
//! `E43PairWalk::MB560` for `b` and `c` on `x` with `(v, u, buffer)`. Last, the car's slot 0x138 gets the six arguments.
//!
//! The calls are the caller's (`E4F0Calls`), in the original's order. The buffers are the caller's frame (three 16-byte
//! locals). The asm's stack leftovers (the slot-12 calls leave their pushed argument) are the arguments the calls take, so
//! the port passes them explicitly.

/// The calls on the car path. Each takes the object as the original does (the car's or the body's vtable, the walker).
pub trait E4F0Calls {
    /// The car's slot 12 (0x30) with the buffer `buf`: returns the pointer `u` the transform uses.
    fn car_transform(&mut self, buf: u32) -> u32;
    /// The body's slot 12 (0x30) with `(v, u)`: returns the walker `x` (eax) the walk runs on.
    fn body_walker(&mut self, v: u32, u: u32) -> u32;
    /// `E33Mat34::ComposeA` (0x51b4c0) on `x` with `(a, b, c)`.
    fn compose_a(&mut self, x: u32, a: u32, b: u32, c: u32);
    /// `E43PairWalk::MB560` (0x51b560) on `x` with `(a, b, c)`.
    fn walk(&mut self, x: u32, a: u32, b: u32, c: u32);
    /// The car's slot 0x138 with the six arguments. Returns eax.
    fn car_e138(&mut self, args: [u32; 6]) -> u32;
}

/// `SimE4F0`'s car path. `args` are its six stack words (`a`, `b`, `c` are vector addresses), `buffers` are the three
/// 16-byte locals (`ComposeA`'s and `MB560`'s `c`). Returns the car's eax.
pub fn car_path<C: E4F0Calls>(calls: &mut C, args: [u32; 6], buffers: [u32; 3]) -> u32 {
    let [a, b, c, ..] = args;
    let [buf_a, buf_b, buf_c] = buffers;
    let u = calls.car_transform(buf_a);
    let x = calls.body_walker(a, u);
    calls.compose_a(x, a, u, buf_a);
    walk_at(calls, b, buf_b);
    walk_at(calls, c, buf_c);
    calls.car_e138(args)
}

/// One `MB560` leg of the car path: the car's transform into `buf`, the body's walker, then the walk.
fn walk_at<C: E4F0Calls>(calls: &mut C, v: u32, buf: u32) {
    let u = calls.car_transform(buf);
    let x = calls.body_walker(v, u);
    calls.walk(x, v, u, buf);
}

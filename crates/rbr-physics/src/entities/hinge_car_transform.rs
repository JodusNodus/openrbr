//! A hinged body's six-argument call forwarded through its owner car. With no car the call goes straight to the plain
//! solid body. With a car, for each of the first three arguments `a`, `b` and `c` (vectors) in turn: the car's frame goes
//! into a buffer, the body's frame is taken from `(vector, car frame)`, and the vector is then carried in that frame, in the
//! pose form for `a` and the plain form for `b` and `c`; last, the car gets the six arguments. What the call does to the car is
//! not known, so its pieces are named by use.
//!
//! The calls are the caller's ([`CarPathCalls`]), in the original's order. The buffers are the caller's (three 16-byte
//! scratch vectors). //! them explicitly.

/// The calls on the car path. Each takes the object as the original does (the car, the body or the frame taken from it).
pub trait CarPathCalls {
    /// The car's frame, written into the buffer `buf`; returns the handle the next call uses.
    fn car_frame(&mut self, buf: u32) -> u32;
    /// The body's frame for `(vector, car_frame)`; returns the frame the carry runs on.
    fn body_frame(&mut self, vector: u32, car_frame: u32) -> u32;
    /// Carries the vector `a` through `frame` in the pose form (the first leg): `(a, car_frame, out)`.
    fn carry_pose(&mut self, frame: u32, a: u32, car_frame: u32, out: u32);
    /// Carries the vector `a` through `frame` (the other legs): `(a, car_frame, out)`.
    fn carry(&mut self, frame: u32, a: u32, car_frame: u32, out: u32);
    /// The car's six-argument call (named by use). Returns what the original leaves in its result register.
    fn car_forward(&mut self, args: [u32; 6]) -> u32;
}

/// The car path. `args` are the six arguments (`a`, `b`, `c` are vector handles), `buffers` the three 16-byte scratch
/// vectors (the outputs of the compose and the carries). Returns the car's result.
pub fn forward_through_car<C: CarPathCalls>(calls: &mut C, args: [u32; 6], buffers: [u32; 3]) -> u32 {
    let [a, b, c, ..] = args;
    let [buf_a, buf_b, buf_c] = buffers;
    let car_frame = calls.car_frame(buf_a);
    let frame = calls.body_frame(a, car_frame);
    calls.carry_pose(frame, a, car_frame, buf_a);
    carry_at(calls, b, buf_b);
    carry_at(calls, c, buf_c);
    calls.car_forward(args)
}

/// One carried leg of the car path: the car's frame into `buf`, the body's frame, then the carry.
fn carry_at<C: CarPathCalls>(calls: &mut C, vector: u32, buf: u32) {
    let car_frame = calls.car_frame(buf);
    let frame = calls.body_frame(vector, car_frame);
    calls.carry(frame, vector, car_frame, buf);
}

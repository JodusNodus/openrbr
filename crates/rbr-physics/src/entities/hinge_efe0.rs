//! `CHingedRigidBody::SimEFE0` (0x51efe0): the hinged body's frame update. It copies the definition's angle block and
//! limits into the body, sets the angle from the definition's limit ratio, runs two `VfE070` vector builds into the
//! definition, `CCar::BodyCompose`, `SimD160`, the `SimD860` vector, and `E37GridOps::AddEdge` on the grid.
//!
//! The calls are the caller's (`EfeCalls`). Each one can change the body (`SimD160`, the slot-36 call), so the port reads the
//! body back after every call (`load`) and writes its own values before every call (`store`). The fourth lane of the
//! `SimD860` copy (+0x5ec) is the original's stack garbage (`E01Vec4` temp, which `SimD860` leaves unset); the port writes 0.
//! The angle is stored as float bits: the asm's `movss` after SetDirty. The C++ casts it to int, which the asm does not do.

/// The body and definition words the port reads or writes, as bits (the copies are word copies).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EfeState {
    /// The definition's gate word (+0x4ec): zero takes the `SetDirty` branch, nonzero the slot-36 call.
    pub def_gate: u32,
    /// The definition's angle block (+0x494..+0x49c, the last word is the reference angle).
    pub def_angle_block: [u32; 3],
    /// The definition's limit words (+0x47c and +0x48c) for the angle's ratio.
    pub def_47c: u32,
    pub def_48c: u32,
    /// The definition's words at +0x20c and +0x26c (the body's limit words at +0x3e8 and +0x3ec).
    pub def_20c: u32,
    pub def_26c: u32,
    /// The definition's context vector (+0x3c0, 4 words): `AddEdge`'s vector.
    pub def_context: [u32; 4],
    /// The link at +0x650 points to the flag word; the port clears it on the slot-36 branch.
    pub flag_addr: u32,
    pub flag_word: u32,
    /// The grid pointer (+0x64): `AddEdge`'s receiver, when nonzero.
    pub grid: u32,
    /// `SetDirty` on the CBase at +0x1d0 stores 1 at its +8 (+0x1d8); +0x1dc is a counter the branch clears (meaning unknown).
    pub dirty_1d0: u32,
    pub field_1dc: u32,
    /// The angle block the body keeps (+0x3f4..+0x3fc): the definition's copy, with the middle word set by `SetDirty` and
    /// the last word (the angle) from the limit ratio on the flag-zero branch.
    pub angle_block: [u32; 3],
    /// The body's limit words (+0x3e8 and +0x3ec).
    pub limit_words: [u32; 2],
    /// The body's position (+0x140, 4 words) and its copy at +0x330.
    pub position: [u32; 4],
    pub position_copy: [u32; 4],
    /// The body's rotation words (+0x100, 4 words, read as the asm reads them: `this+0xf0+0x10`) and their copy at +0x2a0.
    pub body_rotation: [u32; 4],
    pub rotation_copy: [u32; 4],
    /// The body's offset (+0x5c0, 4 words) and its copy at +0x5d0; the `SimD860` copy at +0x5e0.
    pub offset: [u32; 4],
    pub offset_copy: [u32; 4],
    pub out_copy: [u32; 4],
}

/// The addresses the port passes to the calls (the definition's vectors and the body's own pointers).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EfeFrame {
    /// The body (`this`).
    pub this: u32,
    /// The owner car (+0x3d0): `BodyCompose`'s receiver.
    pub car: u32,
    /// `VfE070`'s six pointers for the first build (the definition's +0x300, +0x2c0, +0x2e0, +0x310, +0x3e0, +0x404).
    pub vf_first: [u32; 6],
    /// `VfE070`'s six pointers for the second build (+0x3c0, +0x380, +0x3a0, +0x3d0, +0x44c, +0x428).
    pub vf_second: [u32; 6],
    /// `BodyCompose`'s `a` (+0x300) and its output (+0x320).
    pub body_a: u32,
    pub body_out: u32,
    /// `SimD860`'s output, a stack vector (the caller's frame).
    pub d860_out: u32,
}

/// What `SimEFE0` needs from the game. Every call takes the body as it is in memory (the port stores first).
pub trait EfeCalls {
    /// The body and definition words as they are now.
    fn load(&self) -> EfeState;
    /// Writes the port's values back to the body (the words the port changes).
    fn store(&mut self, state: &EfeState);
    /// The virtual call through slot 36 (`E47D36y`) on the body.
    fn e47_d36(&mut self, this: u32);
    /// `CHingedRigidBody::VfE070` (0x51e070) on the body, with its six pointers.
    fn vf_e070(&mut self, this: u32, args: [u32; 6]);
    /// `CCar::BodyCompose` (0x4e6a30) on `car` with `a` and the output `out`.
    fn body_compose(&mut self, car: u32, a: u32, out: u32);
    /// `CHingedRigidBody::SimD160` (0x51d160) on the body.
    fn sim_d160(&mut self, this: u32);
    /// `CHingedRigidBody::SimD860` (0x51d860) on the body, writing its output to `out`; returns the output's three floats.
    fn sim_d860(&mut self, this: u32, out: u32) -> [f32; 3];
    /// `E37GridOps::AddEdge` (0x4f65b0) on `grid` with the vector `vec`.
    fn add_edge(&mut self, grid: u32, vec: [f32; 4]);
}

/// `CHingedRigidBody::SimEFE0`: the body's frame update, in the original's call order.
pub fn hinge_frame_update<C: EfeCalls>(calls: &mut C, frame: &EfeFrame) {
    let mut s = calls.load();
    if s.def_gate == 0 {
        // SetDirty on the CBase at +0x1d0, then the counter at +0x1dc is cleared
        s.dirty_1d0 = 1;
        s.field_1dc = 0;
    } else {
        s.flag_word = 0;
        calls.store(&s);
        calls.e47_d36(frame.this);
        s = calls.load();
    }
    s.angle_block = s.def_angle_block;
    if s.flag_word == 0 {
        let d47c = f32::from_bits(s.def_47c);
        let d48c = f32::from_bits(s.def_48c);
        let d49c = f32::from_bits(s.def_angle_block[2]);
        // the ratio of the reference angle between the two limits: past the middle, the factor is 0.9 (0x3f666666)
        let f7 = if 0.5 < (d49c - d48c) / (d47c - d48c) { 0.9 } else { 0.1 };
        let f6 = 1.0 - f7;
        // SetDirty on the CBase at +0x3f0 (its +8 is the angle block's middle word)
        s.angle_block[1] = 1;
        s.angle_block[2] = ((f6 + f7) * d47c).to_bits();
    }
    calls.store(&s);
    calls.vf_e070(frame.this, frame.vf_first);
    calls.vf_e070(frame.this, frame.vf_second);
    s = calls.load();
    calls.store(&s);
    calls.body_compose(frame.car, frame.body_a, frame.body_out);
    s = calls.load();
    calls.store(&s);
    calls.sim_d160(frame.this);
    s = calls.load();
    s.position_copy = s.position;
    s.rotation_copy = s.body_rotation;
    calls.store(&s);
    let out = calls.sim_d860(frame.this, frame.d860_out);
    s = calls.load();
    s.offset_copy = s.offset;
    s.out_copy = [out[0].to_bits(), out[1].to_bits(), out[2].to_bits(), 0];
    if s.grid != 0 {
        calls.store(&s);
        calls.add_edge(s.grid, s.def_context.map(f32::from_bits));
        s = calls.load();
    }
    s.limit_words = [s.def_20c, s.def_26c];
    calls.store(&s);
}

//! The ground entity (`CGround`): the flags that report the ground present, the contact the box probe reports, and the
//! calls the probe and the replacement of the ground singleton make. The classes' constants and the construction are
//! layout (`layout::classes`, `layout::construct_ground_def`, `layout::ground_make`).

use super::store::EntityStore;

/// The ground's flag slots report it present (four slots, one answer).
pub fn present() -> bool {
    true
}

/// The call the ground's make (`layout::ground_make`) makes on the ground it replaces: its deleting destructor.
pub trait Replace {
    fn delete(&mut self, ground: u32);
}

/// The part of the record the box probe writes on a hit that the model stores itself, from the host's 13 floats `out`:
/// words 4..15 (the first four are the vector copy of `out[0..4]`, which the model makes through [`ProbeCalls`]). `owner`
/// is the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// Words 4..8: `out[4..8]`.
    pub second: [f32; 4],
    /// Words 8..11: `out[10]`, `out[11]`, `out[9]` (the order the original stores them in).
    pub point: [f32; 3],
    /// Word 13: `out[12]`. Words 11 and 12 are cleared.
    pub depth: f32,
    /// Word 14: the ground.
    pub owner: u32,
}

/// The scatter of the box probe on a hit, with `owner` the ground.
pub fn contact(out: &[f32; 13], owner: u32) -> Contact {
    Contact { second: [out[4], out[5], out[6], out[7]], point: [out[10], out[11], out[9]], depth: out[12], owner }
}

/// The calls the box probe makes: the transform of the ground (a walk of its own), the host's box query (on `host`, writes
/// the 13 floats to `out`; returns whether it hit), and the vector copy.
pub trait ProbeCalls {
    fn transform(&mut self, this: u32, out: u32, arg: u32) -> u32;
    fn query(&mut self, host: u32, first: u32, second: u32, out: u32) -> u32;
    fn copy_vector(&mut self, to: u32, from: u32);
}

/// The scratch the box probe keeps on its stack: the two matrices (`first`, `second`, 16 bytes each) and the host's 13-float
/// output (`out`). Their addresses are the original's stack; the model takes them from its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeScratch {
    pub first: u32,
    pub second: u32,
    pub out: u32,
}

/// `CGround::VfE8` (0x51c6f0) on the ground `ground`, with transforms `a` and `b` and the record `d`: `FetchX` of both
/// transforms into `scratch`, the host's query on their matrices (its 13 floats go to `scratch.out`), and on a hit the
/// record: `CopyFrom` of `out[0..4]`, then the scatter ([`contact`]). Returns eax.
pub fn box_probe<S: EntityStore>(
    store: &mut S,
    ground: u32,
    a: u32,
    b: u32,
    d: u32,
    scratch: ProbeScratch,
    calls: &mut impl ProbeCalls,
) -> u32 {
    let identity = store.identity();
    calls.transform(a, scratch.first, identity);
    calls.transform(b, scratch.second, identity);
    let host = store.probe_host();
    if calls.query(host, scratch.first, scratch.second, scratch.out) == 0 {
        return 0;
    }
    let out = store.probe_output(scratch.out);
    calls.copy_vector(d, scratch.out);
    store.store_probe_contact(d, &contact(&out, ground));
    1
}

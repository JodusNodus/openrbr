//! `CSolid::Vf81x` (0x52c510): the contact vote. The word at `CSolid+0x274` is read as a pointer to the contact list
//! (count at +4, 0x60-byte records from +0x10). With no list (the word is 0) the call returns 0 and writes nothing.
//! With a list the original clears the two outputs and, for each record, adds the impulse pair of a `BigA` hit
//! (`contact::contact_step`) to them, lanes x, y, z in that order. eax is 1 when a record hits. The word is also the
//! sleep flag `Vf24x` reads (`README.md`, unknown meaning).

use super::contact::Impulse;
use super::host::HostUnavailable;
use super::layout::Vec4;

/// The vote's call of `BigA` on record `k` (0-based): the pair on a hit, `None` on a miss.
pub trait ContactVoteHost {
    fn contact(&mut self, k: u32) -> Result<Option<Impulse>, HostUnavailable>;
}

/// The two outputs of a vote over a list (cleared, then summed) and whether a record hit (the original's eax).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactVote {
    pub a: Vec4,
    pub b: Vec4,
    pub hit: bool,
}

/// `a + d` in lanes x, y, z (the asm's three `addss`); lane w is kept.
fn add3(a: Vec4, d: Vec4) -> Vec4 {
    Vec4 { x: a.x + d.x, y: a.y + d.y, z: a.z + d.z, w: a.w }
}

/// The vote over the list of `count` records, `None` when the body has no list (no call, no write).
pub fn vote(count: Option<u32>, host: &mut impl ContactVoteHost) -> Result<Option<ContactVote>, HostUnavailable> {
    let Some(count) = count else {
        return Ok(None);
    };
    let mut out = ContactVote::default();
    for k in 0..count {
        if let Some(i) = host.contact(k)? {
            out.a = add3(out.a, i.a);
            out.b = add3(out.b, i.b);
            out.hit = true;
        }
    }
    Ok(Some(out))
}

/// The vote's answer when it votes: the index (`local+0x20`) and the scale (`local+0x30`) the caller takes from the
/// output buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vote {
    pub index: u32,
    pub scale: f32,
}

/// The vote hook the table-vote worker calls (`S00` of the global at 0x7c755c, 0x4fe810): the position and the point
/// (the position with its z lowered by the argument). `None` when it does not vote (eax 0).
pub trait VoteHost {
    fn vote(&mut self, pos: Vec4, point: Vec4) -> Result<Option<Vote>, HostUnavailable>;
}

/// What `Step77` returns and writes: its eax, the int at `*a` and the float at `*b`, after the call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step77 {
    pub ret: i32,
    pub a: u32,
    pub b: f32,
}

/// `CSolid::Step77` (0x529d30) (name ours): votes for the position `pos` (the body's `+0x140`) with its z lowered by `b`. When the
/// vote is taken, `*a` is the vote's index and `*b` is scaled by its scale; otherwise `*a` is 1.0 and `*b` is unchanged.
pub fn probe_ground(pos: Vec4, b: f32, host: &mut impl VoteHost) -> Result<Step77, HostUnavailable> {
    let point = Vec4 { z: pos.z - b, ..pos };
    Ok(match host.vote(pos, point)? {
        Some(v) => Step77 { ret: 1, a: v.index, b: b * v.scale },
        None => Step77 { ret: 0, a: 1.0f32.to_bits(), b },
    })
}

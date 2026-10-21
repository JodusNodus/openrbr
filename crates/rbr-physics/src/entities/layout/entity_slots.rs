//! The entities' small slots: the `CEntity` hooks and flags, `CDummyObject`'s box and span test, the exclusion set
//! and `CCar::Clear480`.

use super::collide_init::IDENTITY_GLOBAL;
use crate::entities::dummy;
use crate::entities::exclusion::{self, Excluder, Heap, Members};
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use crate::physics_core::layout::PHYSICS_GLOBAL;

/// The global that holds the flags object (`g_1659b70`); the object's +0x24 word is the flags object, whose byte at
/// +0x160cc holds the flags `CEntity::E43VfD4` tests.
const FLAGS_OBJECT: u32 = 0x24;
const FLAGS_BYTE: u32 = 0x160CC;

/// The flags object and its flag byte, as `CEntity::E43VfD4` reads them (`layout` reads them again after each call).
pub fn d4_flags(mem: &Mem) -> (u32, u8) {
    let object = mem.u32(mem.u32(PHYSICS_GLOBAL) + FLAGS_OBJECT);
    (object, mem.u8(object + FLAGS_BYTE))
}

/// The calls `CDummyObject::VfCC` makes, each with the callee's entry stack pointer `sp`: `E33Mat34::FetchX` (0x51b6c0)
/// on the transform `this` into `out` with `arg`, and the hit handler (0x601470, not named) on the group.
pub trait DummyCalls {
    fn fetch_x(&mut self, sp: u32, this: u32, out: u32, arg: u32);
    fn group_on_hit(&mut self, sp: u32, group: u32);
}

/// The dummy's centre (+0x94) and half extents (+0xa0), as stored (x, z, y: see `dummy::box_corners`).
const DUMMY_CENTER: u32 = 0x94;
const DUMMY_HALF: u32 = 0xa0;

/// `CDummyObject::Vf11C` (0x600b20) on the dummy at `this`: the box's low and high corners, three floats each, to `lo`
/// and `hi`.
pub fn dummy_box(mem: &mut Mem, this: u32, lo: u32, hi: u32) {
    let (low, high) = dummy::box_corners(Vec3::load(mem, this + DUMMY_CENTER), Vec3::load(mem, this + DUMMY_HALF));
    for (at, v) in [(lo, low), (hi, high)] {
        mem.set_f32(at, v.x);
        mem.set_f32(at + 4, v.y);
        mem.set_f32(at + 8, v.z);
    }
}

/// The dummy's span group (+0xb0), its span segment (+0x7c) and its post segment (+0x80), and the flag word their
/// `+0x5c` keeps (bit 3, `8`).
const DUMMY_GROUP: u32 = 0xb0;
const DUMMY_SPAN: u32 = 0x7c;
const DUMMY_POST: u32 = 0x80;
const SEGMENT_FLAGS: u32 = 0x5c;
const SEGMENT_HIT: u32 = 0x8;

/// `CDummyObject::VfCC(a, b)` (0x6009b0) on the dummy at `this`, with `frame` the base of the original's frame (the
/// stack below its aligned entry esp, `FetchX`'s outputs at `frame + 0x1c` and `frame + 0xc`): the two origins, the span
/// and the overlap test. On a hit the group's handler runs and both segments get the hit flag (the original stores to
/// the post without a null check). Always returns 0 (eax).
pub fn dummy_span_test(mem: &mut Mem, this: u32, a: u32, b: u32, frame: u32, calls: &mut impl DummyCalls) {
    let identity = mem.u32(IDENTITY_GLOBAL);
    let (origin_a, origin_b) = (frame + 0x1c, frame + 0xc);
    calls.fetch_x(frame - 0x10, a, origin_a, identity);
    calls.fetch_x(frame - 0x10, b, origin_b, identity);
    let (c, d) = dummy::span(Vec3::load(mem, origin_a), Vec3::load(mem, origin_b));
    let hit = dummy::overlaps(Vec3::load(mem, this + DUMMY_CENTER), Vec3::load(mem, this + DUMMY_HALF), c, d);
    if !hit {
        return;
    }
    calls.group_on_hit(frame - 0x8, mem.u32(this + DUMMY_GROUP));
    let span = mem.u32(this + DUMMY_SPAN);
    if span != 0 {
        set_hit_flag(mem, span);
    }
    let post = mem.u32(this + DUMMY_POST);
    set_hit_flag(mem, post);
}

/// `CEntity::E43_ExclA060` (0x51a060), removing the first occurrence from the inline count-and-members array.
pub fn remove_exclusion(mem: &mut Mem, this: u32, value: u32) {
    let array = mem.u32(this + 0x2c);
    if array == 0 {
        return;
    }
    let count = mem.u32(array);
    let mut found = false;
    for i in 0..count {
        if found {
            mem.set_u32(array + i * 4, mem.u32(array + (i + 1) * 4));
        }
        if mem.u32(array + (i + 1) * 4) == value {
            found = true;
        }
    }
    if found {
        mem.set_u32(array + count * 4, 0);
        mem.set_u32(array, count.wrapping_sub(1));
    }
}

/// Calls made by `CCar::Clear480` (0x4e58f0).
pub trait Clear480Calls {
    fn sim_fd00(&mut self, sp: u32, body: u32) -> u32;
}

/// `CCar::Clear480` checks all four tags and clears a matching slot only when its body's SimFD00 returns nonzero.
pub fn clear_car_bodies(mem: &mut Mem, car: u32, body: u32, frame: u32, calls: &mut impl Clear480Calls) {
    for i in 0..4 {
        let slot = car.wrapping_add(0x480 + i * 4);
        if mem.u32(slot) == body && calls.sim_fd00(frame - 0x10, body) != 0 {
            mem.set_u32(slot, 0);
        }
    }
}

fn set_hit_flag(mem: &mut Mem, segment: u32) {
    let flags = mem.u32(segment + SEGMENT_FLAGS);
    mem.set_u32(segment + SEGMENT_FLAGS, flags | SEGMENT_HIT);
}

/// A hook that answers 0 (`CEntity` 0x519ac0: `xor eax, eax; ret`).
pub fn zero_hook() -> u32 {
    0
}

/// `CEntity::VfCC` (0x519e90, slot 51, ret 8): the entity answers 1 whatever its two arguments are.
pub fn vf_cc() -> u32 {
    1
}

/// `CEntity::VfD0` (0x519ea0, slot 52, ret 12): the entity answers 0 whatever its three arguments are.
pub fn vf_d0() -> u32 {
    0
}

pub(super) fn load_vec4(mem: &Mem, at: u32) -> [f32; 4] {
    mem.f32s(at)
}

/// `E44Shell::M51C8C0` (0x51c8c0), thiscall `(a, out)`: `out[i] = this[i] * a[i]` over four floats
/// (`Vec4::mul_lanes`). Both inputs are read before `out` is written, so `out` may alias either one.
pub fn componentwise_mul(mem: &mut Mem, this: u32, a: u32, out: u32) {
    let p = vec4_of(load_vec4(mem, this)).mul_lanes(vec4_of(load_vec4(mem, a)));
    for (i, v) in [p.x, p.y, p.z, p.w].into_iter().enumerate() {
        mem.set_f32(out + 4 * i as u32, v);
    }
}

pub(super) fn vec4_of(v: [f32; 4]) -> Vec4 {
    Vec4 { x: v[0], y: v[1], z: v[2], w: v[3] }
}

/// The exclusion set (`+0x2c`, `[count, members...]`) and the parent (`+0x44`) of a CEntity (`ExclusionTest`).
pub(super) const EXCL_SET: u32 = 0x2c;
pub(super) const PARENT: u32 = 0x44;

/// The entity at `at` as the exclusion test sees it.
pub fn load_excluder(mem: &Mem, at: u32) -> Excluder<SetInMem<'_>> {
    let set = mem.u32(at + EXCL_SET);
    let members = (set != 0).then(|| SetInMem { mem, set, count: mem.u32(set) });
    Excluder { handle: at, parent: mem.u32(at + PARENT), members }
}

/// An exclusion set read where it is (`[count, members...]` at `set`) instead of copied: the step tests dozens.
pub struct SetInMem<'a> {
    mem: &'a Mem,
    set: u32,
    count: u32,
}

impl Members for SetInMem<'_> {
    fn count(&self) -> u32 {
        self.count
    }

    fn holds(&self, entity: u32) -> bool {
        (1..=self.count).any(|i| self.mem.u32(self.set + 4 * i) == entity)
    }
}

/// `CEntity::E43_ExclA000` at `at` (0x51a000): the set's array from `heap` (the array's pointer, then its
/// count 0). Returns eax: 1 when it allocated, 0 when the entity had a set.
pub fn alloc_exclusion(mem: &mut Mem, at: u32, n: u32, heap: &mut impl Heap) -> u32 {
    let has_set = mem.u32(at + EXCL_SET) != 0;
    match exclusion::alloc_exclusion(heap, has_set, n) {
        Some(array) => {
            mem.set_u32(at + EXCL_SET, array);
            mem.set_u32(array, 0);
            1
        }
        None => 0,
    }
}

/// `CEntity::E43VfEC` at `at` (0x51a040, slot 59): appends `member` to the set (it does not check the array's size).
/// Returns eax, 1.
pub fn add_exclusion(mem: &mut Mem, at: u32, member: u32) -> u32 {
    let set = mem.u32(at + EXCL_SET);
    let count = mem.u32(set);
    mem.set_u32(set + 4 * (count + 1), member);
    mem.set_u32(set, count + 1);
    1
}

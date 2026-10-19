//! `E51Wheel::M3D520` (0x53d520), the ground probe, and the wheel tuning NGP retunes.

use super::fields::{BLOCK_DIFF, BLOCK_TARGET, CONTACT_A80, SUB_A, SUB_B, TYRE_SCRATCH};
use crate::car_body::layout as car_layout;
use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::physics_core::layout::COLLISION_GLOBAL;
use crate::stage::layout::COLLISION_SYSTEM;
use crate::wheel::store::WheelStore;

/// The probe's scratch, as offsets from the body's frame `S` (its `esp` after the prologue, see `PROBE_PROLOGUE`): the
/// E46 output `rayWork` at `PROBE_RAYS` (the list `E45_M46A0` reads), the eight ray objects of `PROBE_RAY_SIZE` bytes
/// from `PROBE_RAY_BLOCKS` (the constructor and destructor iterators' array, 0x30 past the list), the hit buffer
/// `PROBE_HIT` (0x40 bytes: the hit's words, its distance at `PROBE_HIT_DIST`), the sweep's 16-byte output and its
/// distance, and the vectors the casts take by pointer.
pub const PROBE_FV: u32 = 0x60;
pub const PROBE_V1: u32 = 0x80;
pub const PROBE_V2: u32 = 0x70;
pub const PROBE_V3: u32 = 0x50;
pub const PROBE_HIT: u32 = 0x90;
pub const PROBE_HIT_DIST: u32 = 0x30;
pub const PROBE_SWEEP_OUT: u32 = 0xD0;
pub const PROBE_SWEEP_DIST: u32 = 0x40;
pub const PROBE_RAYS: u32 = 0xE0;
pub const PROBE_RAY_BLOCKS: u32 = 0x110;
pub const PROBE_RAY_SIZE: u32 = 0x6B0;
/// The bytes of the scratch the probe uses: up to the end of the eight rays.
pub const PROBE_SCRATCH: u32 = PROBE_RAY_BLOCKS + 8 * PROBE_RAY_SIZE;
/// The body's frame below the entry's aligned `ebp`: the SEH record and its try level (16 bytes), the `_chkstk`
/// allocation (0x3694) and the three saved registers (12 bytes). `S` is that `esp`; the callees run below it.
pub const PROBE_PROLOGUE: u32 = 0x36B0;

/// A block's gate word (`*block`): the ray's `M_D760X` result, then 1 on a hit.
pub const PROBE_GATE: u32 = 0x0;
/// The block's second word (`block[1]`, `+4`): cleared on a miss, and read by the blend as the block's "valid" flag.
pub const PROBE_CLEAR: u32 = 0x4;
/// The block's point (`block + 8` words): the source `LoadBinary` reads.
pub const PROBE_POINT: u32 = 0x20;
/// The block at `+0xc20`, the middle one (its target is `+0xc30`, its point `+0xc40`).
const PROBE_BLOCK_2: u32 = 0xC20;
/// An int the probe copies into `+0xce0` (`*0x739594`).
const PROBE_INT: u32 = 0x0073_9594;
/// The tyre tail's scale word the probe reads (`+0x4c`).
const TAIL_SCALE: u32 = 0x4C;
/// The copied block's dwords (`rep movsd` of `0x1c`).
const PROBE_BLOCK_WORDS: u32 = 0x1C;
/// The first ray limit (`2.0`), the test vector's fill (`0.4`), the pose's second term (`0.2`), the blend's average
/// (`0.5`), and the rate the probe writes into the `+0xd00` block (`0.1`).
const PROBE_LIMIT: f32 = 2.0;
const PROBE_FILL: f32 = 0.4;
const PROBE_POSE: f32 = 0.2;
const HALF: f32 = 0.5;
const PROBE_RATE: f32 = 0.1;

/// The wheel constants NGP makes per-car values (`10171300`, `10171304`, `10171308`; `notes/ngp.md`): the ground
/// probe's test vector fill, the table lerp's first weight, and the pose (the probe's middle block, the lerps' base).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelTuning {
    pub fill: f32,
    pub weight: f32,
    pub pose: f32,
}

impl WheelTuning {
    /// 1.02's: 0.4 (`0x72f664`), 0.15 (`0x72d154`) and 0.2 (`0x72f428`, and the probe's push).
    pub const STOCK: Self = WheelTuning { fill: PROBE_FILL, weight: 0.15, pose: PROBE_POSE };
}

/// What NGP retunes in the ground probe: the collision system its casts run on (NGP repoints `g_7c755c` at its own
/// copy) and the wheel constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeTuning {
    pub collision: u32,
    pub wheel: WheelTuning,
}

impl ProbeTuning {
    /// 1.02's: the system at `g_7c755c`.
    pub fn stock(mem: &Mem) -> Self {
        ProbeTuning { collision: mem.u32(COLLISION_SYSTEM), wheel: WheelTuning::STOCK }
    }
}

/// The original callees the probe calls, each with the arguments the asm passes: the object `ecx` names first, then the
/// stack words in order. The callees with an `eax` return it.
pub trait GroundCalls {
    /// `TyreTail_Get14` (0x5326c0) on the tyre container: the tail pointer.
    fn tyre_tail14(&mut self, container: u32) -> u32;
    /// `CWheel::Unk96e0` (0x5396e0) on the wheel (name ours).
    fn table_lerp_pair(&mut self, wheel: u32);
    /// `E39_SegTest170` (0x4ff170) on `object` with the segment's start and test vector, and a null output.
    fn seg_test170(&mut self, object: u32, a: u32, b: u32);
    /// The eight rays' constructor iterator (`eh vector constructor iterator`, 0x70c408) over `blocks`.
    fn ray_blocks_init(&mut self, blocks: u32);
    /// The eight rays' destructor iterator (0x70c4da) over `blocks`.
    fn ray_blocks_fini(&mut self, blocks: u32);
    /// `E46_Kernel5bd0` (0x525bd0, stdcall, with ECX = the world): the rays from `a` to `b`, mask 8, no exclusion, to `out`.
    fn kernel5bd0(&mut self, world: u32, a: u32, b: u32, out: u32);
    /// `E38Ops::M_D760X` (0x4fd760) on `object` with `(b, c, d, 0)`: its result.
    fn m_d760x(&mut self, object: u32, b: u32, c: u32, d: u32) -> u32;
    /// `E45_M46A0` (0x5246a0) on the world with `(a, b, list, out)`: the hit object's pointer, or 0.
    fn m46a0(&mut self, world: u32, a: u32, b: u32, list: u32, out: u32) -> u32;
    /// The hit object's virtual at `+0x154` (ECX = `hit`): its eax.
    fn hit_flag(&mut self, hit: u32) -> u32;
    /// `E51Wheel::M3D460` (0x53d460) on the wheel with `(block, b, c)`.
    fn pose_flag(&mut self, wheel: u32, block: u32, b: f32, c: f32);
    /// `E38_004FAD90` (0x4fad90, stdcall, with ECX = `object`): `(a, b, c, d)`, its eax.
    fn segment_sweep(&mut self, object: u32, a: u32, b: u32, c: u32, d: u32) -> u32;
    /// `CWheel::Unk9290` (0x539290) on the wheel with `(a, b, out)`.
    fn pick(&mut self, wheel: u32, a: u32, b: u32, out: u32);
    /// `CWheel::Unk9900` (0x539900) on the wheel with `(x, y, z)`.
    fn blend_tags(&mut self, wheel: u32, x: u32, y: u32, z: f32);
    /// `NTweak::CFloat3::LoadBinary` (0x4f3d20) on `tmp` with the data at `data`.
    fn load_binary(&mut self, tmp: u32, data: u32);
}

/// `E01Vec4::CopyFrom` (0x43c5b0): the 16 bytes at `src` go to `dst`.
fn copy_vec<S: WheelStore>(mem: &mut S, dst: u32, src: u32) {
    let v = Vec4::load(mem, src);
    v.store(mem, dst);
}

/// `count` words from `src` to `dst`, in order (the `rep movsd` copies).
fn copy_words<S: WheelStore>(mem: &mut S, dst: u32, src: u32, count: u32) {
    for k in 0..count {
        let word = mem.u32(src + 4 * k);
        mem.set_u32(dst + 4 * k, word);
    }
}

/// `a * s + b` in each lane (the asm's `mulps` then `addps`).
fn scaled_plus(a: Vec4, s: f32, b: Vec4) -> Vec4 {
    Vec4 { x: a.x * s + b.x, y: a.y * s + b.y, z: a.z * s + b.z, w: a.w * s + b.w }
}

/// The miss test of a ray cast, in the asm's order: no hit; the hit's `+0x154` says so; or the block already holds a
/// hit whose distance is not below its target. The compare is false for a NaN, so a NaN distance misses.
fn ray_misses<S: WheelStore>(mem: &S, calls: &mut impl GroundCalls, block: u32, buf: u32, obj: u32) -> bool {
    obj == 0
        || calls.hit_flag(obj) != 0
        || (mem.u32(block + PROBE_GATE) != 0 && !(mem.f32(block + BLOCK_TARGET) > mem.f32(buf + PROBE_HIT_DIST)))
}

/// Whether a hit takes the best so far: its flag byte has bit 4, and its distance is below `limit`.
fn hit_takes<S: WheelStore>(mem: &S, obj: u32, dist: f32, limit: f32) -> bool {
    mem.u8(obj + 0x60) & 4 != 0 && dist < limit
}

/// `E51Wheel::M3D520` (0x53d520, thiscall, `ret 8`): the ground probe. Three ray casts (from `+0xa20`, `+0xa40` and the
/// strut's `+0xa30`), each into its block (`+0xc90`, `+0xd00`, `+0xc20`) with `M3D460`, then the blend of the hits into
/// `out` (the contact point). Returns 1 when the blend runs, 0 when no ray found a block to blend (the early return).
/// `frame` is the body's `S` (the scratch is `PROBE_SCRATCH` bytes above it); `tmp` is the second stack argument.
/// `tuning` is [`ProbeTuning::stock`] for 1.02.
pub fn ground_probe<S: WheelStore>(
    mem: &mut S,
    calls: &mut impl GroundCalls,
    this: u32,
    frame: u32,
    out: u32,
    tmp: u32,
    tuning: ProbeTuning,
) -> u32 {
    let o = this;
    let pf = o + 0xA30;
    let v367c = o + CONTACT_A80;
    let pi1 = o + SUB_B;
    let pi2 = o + PROBE_BLOCK_2;
    let pi3 = o + SUB_A;
    let list = frame + PROBE_RAYS;
    let blocks = frame + PROBE_RAY_BLOCKS;
    let buf = frame + PROBE_HIT;
    let world = mem.u32(car_layout::WORLD);
    let object = tuning.collision;
    let sweep = mem.u32(COLLISION_GLOBAL);

    let f5 = mem.f32(calls.tyre_tail14(o + TYRE_SCRATCH) + TAIL_SCALE);
    let int = mem.u32(PROBE_INT);
    mem.set_u32(o + 0xCE0, int);
    mem.set_f32(o + 0xC70, 0.0);
    mem.set_f32(o + 0xD50, PROBE_RATE);
    mem.set_u32(o + 0xD7C, 0);
    let mut best = PROBE_LIMIT;
    calls.table_lerp_pair(o);
    let fv = frame + PROBE_FV;
    for k in 0..3 {
        mem.set_f32(fv + 4 * k, tuning.wheel.fill);
    }
    calls.seg_test170(object, pf, fv);
    calls.ray_blocks_init(blocks);
    calls.kernel5bd0(world, pf, fv, list);

    // Ray 1: from +0xa20, its test vector is the contact vector plus +0xa20; the hit goes to block 1.
    let v1 = frame + PROBE_V1;
    (scaled_plus(Vec4::load(mem, v367c), 1.0, Vec4::load(mem, o + 0xA20))).store(mem, v1);
    mem.set_u32(pi1 + PROBE_GATE, calls.m_d760x(object, o + 0xA20, v1, o + 0xCA0));
    let obj = calls.m46a0(world, o + 0xA20, v1, list, buf);
    if !ray_misses(mem, calls, pi1, buf, obj) {
        mem.set_u32(pi1 + PROBE_GATE, 1);
        copy_words(mem, o + 0xCA0, buf, 16);
        let dist = mem.f32(buf + PROBE_HIT_DIST);
        if hit_takes(mem, obj, dist, PROBE_LIMIT) {
            mem.set_u32(o + 0xD7C, obj);
            best = dist;
        }
        calls.pose_flag(o, pi1, f5, mem.f32(o + 0xD74));
    } else if mem.u32(pi1 + PROBE_GATE) != 0 {
        calls.pose_flag(o, pi1, f5, mem.f32(o + 0xD74));
    } else {
        mem.set_u32(pi1 + PROBE_CLEAR, 0);
    }

    // Ray 2: from +0xa40 (the same test vector form); the hit goes to block 3, its point at +0xd10.
    let v2 = frame + PROBE_V2;
    (scaled_plus(Vec4::load(mem, v367c), 1.0, Vec4::load(mem, o + 0xA40))).store(mem, v2);
    mem.set_u32(pi3 + PROBE_GATE, calls.m_d760x(object, o + 0xA40, v2, o + 0xD10));
    let obj = calls.m46a0(world, o + 0xA40, v2, list, buf);
    if !ray_misses(mem, calls, pi3, buf, obj) {
        mem.set_u32(pi3 + PROBE_GATE, 1);
        copy_words(mem, o + 0xD10, buf, 16);
        let dist = mem.f32(buf + PROBE_HIT_DIST);
        if hit_takes(mem, obj, dist, best) {
            mem.set_u32(o + 0xD7C, obj);
            best = dist;
        }
        calls.pose_flag(o, pi3, f5, mem.f32(o + 0xD74));
    } else if mem.u32(pi3 + PROBE_GATE) != 0 {
        calls.pose_flag(o, pi3, f5, mem.f32(o + 0xD74));
    } else {
        mem.set_u32(pi3 + PROBE_CLEAR, 0);
    }

    // Ray 3: from the strut's point +0xa30; the hit goes to block 2, its point at +0xc30.
    let v3 = frame + PROBE_V3;
    (scaled_plus(Vec4::load(mem, v367c), 1.0, Vec4::load(mem, pf))).store(mem, v3);
    copy_vec(mem, o + 0xDB0, pf);
    copy_vec(mem, o + 0xDC0, v3);
    copy_vec(mem, o + 0xE20, v367c);
    mem.set_u32(pi2 + PROBE_GATE, calls.m_d760x(object, pf, v3, o + 0xC30));
    let obj = calls.m46a0(world, pf, v3, list, buf);
    let miss = ray_misses(mem, calls, pi2, buf, obj);
    if !miss {
        mem.set_u32(pi2 + PROBE_GATE, 1);
        copy_words(mem, o + 0xC30, buf, 16);
        if hit_takes(mem, obj, mem.f32(buf + PROBE_HIT_DIST), best) {
            mem.set_u32(o + 0xD7C, obj);
        }
    } else if mem.u32(pi2 + PROBE_GATE) == 0 {
        mem.set_u32(pi2 + PROBE_CLEAR, 0);
        mem.set_f32(o + 0xD78, 0.0);
    }
    if !miss || mem.u32(pi2 + PROBE_GATE) != 0 {
        // The sweep path: block 2's pose at 0.2, then the sweep from the strut to the contact vector.
        calls.pose_flag(o, pi2, f5, tuning.wheel.pose);
        let hv = calls.segment_sweep(sweep, pf, v3, frame + PROBE_SWEEP_OUT, frame + PROBE_SWEEP_DIST);
        let dist = mem.f32(frame + PROBE_SWEEP_DIST);
        let residue = if hv != 0 && f5 > dist { f5 - dist } else { 0.0 };
        mem.set_f32(o + 0xD78, residue);
    }

    // The blend. Without a block the probe clears its blocks and returns 0.
    let b11 = mem.u32(pi1 + PROBE_CLEAR) != 0 || mem.u32(pi3 + PROBE_CLEAR) != 0;
    if mem.u32(pi2 + PROBE_CLEAR) == 0 && !b11 {
        calls.ray_blocks_fini(blocks);
        return 0;
    }
    // The asm stores the cross's three lanes only; lane 3 (+0xA9C) keeps its old value, and the stack copies read it.
    let cross = Vec4::load(mem, o + 0xA70).cross3(Vec4::load(mem, v367c));
    (crate::math::vec::Vec3 { x: cross.x, y: cross.y, z: cross.z }).store(mem, o + 0xA90);
    let v369c = if mem.u32(pi1 + PROBE_CLEAR) != 0 && mem.u32(pi3 + PROBE_CLEAR) != 0 {
        let average = (mem.f32(pi3 + BLOCK_DIFF) + mem.f32(pi1 + BLOCK_DIFF)) * HALF;
        if mem.u32(pi2 + PROBE_CLEAR) == 0 || !(average > mem.f32(pi2 + BLOCK_DIFF)) {
            calls.pick(o, pi1, pi3, tmp);
            average
        } else {
            calls.load_binary(tmp, pi2 + PROBE_POINT);
            mem.f32(pi2 + BLOCK_DIFF)
        }
    } else {
        if mem.u32(pi1 + PROBE_CLEAR) == 0 {
            copy_words(mem, pi1, pi3, PROBE_BLOCK_WORDS);
        }
        if mem.u32(pi2 + PROBE_CLEAR) == 0 {
            mem.set_f32(pi2 + BLOCK_DIFF, f5);
        }
        let c84 = mem.f32(pi2 + BLOCK_DIFF);
        let cf4 = mem.f32(pi1 + BLOCK_DIFF);
        if !b11 {
            calls.load_binary(tmp, pi2 + PROBE_POINT);
            c84
        } else if mem.u32(pi2 + PROBE_CLEAR) == 0 {
            calls.load_binary(tmp, pi1 + PROBE_POINT);
            (c84 + cf4) * HALF
        } else if c84 > cf4 {
            calls.pick(o, pi2, pi1, tmp);
            (c84 + cf4) * HALF
        } else {
            calls.load_binary(tmp, pi2 + PROBE_POINT);
            c84
        }
    };
    mem.set_f32(o + 0xAA0, v369c);
    (scaled_plus(Vec4::load(mem, v367c), v369c, Vec4::load(mem, pf))).store(mem, out);
    if mem.u32(pi2 + PROBE_GATE) == 0 {
        let source = if mem.u32(pi1 + PROBE_GATE) != 0 { pi1 } else { pi3 };
        copy_words(mem, pi2, source, PROBE_BLOCK_WORDS);
    }
    calls.blend_tags(o, mem.u32(pi2 + 0x38), mem.u32(pi2 + 0x3C), mem.f32(pi2 + 0x34));
    if mem.u32(pi2 + PROBE_GATE) != 0 {
        copy_vec(mem, o + 0xDD0, out);
    }
    if mem.u32(pi1 + PROBE_GATE) != 0 {
        copy_vec(mem, o + 0xE10, o + 0xCA0);
    }
    if mem.u32(pi3 + PROBE_GATE) != 0 {
        copy_vec(mem, o + 0xDF0, o + 0xD10);
    }
    calls.ray_blocks_fini(blocks);
    1
}

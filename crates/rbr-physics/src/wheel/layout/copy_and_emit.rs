//! `E51Wheel::CopyCF30`, `CWheel::Unk9900`/`Unk9660`/`Unk96e0`, the embedded dispatch and `E51MultiTire::Emit`.

use super::fields::{
    BLEND_OPS, BLOCK_LERP_A, BLOCK_LERP_B, CF_DEF_234, CF_DEF_238, CF_DEF_23C, CF_DEF_414, CF_DEF_418, CF_DEF_41C,
    CF_DEF_900, CF_DEF_BASE, CF_DEF_COS, CF_DEF_F1, CF_DEF_SIN, CF_DEF_SUM, CF_DEF_VEC, CF_DEF_ZERO, CF_SRC_20,
    CF_SRC_44, CF_SRC_48, CF_SRC_4C, CF_SRC_54, CF_SRC_58, CF_SRC_5C, CF_SRC_ANG, CF_SRC_VEC, DEF, DEF_MATERIALS,
    EMBEDDED_OBJ, LERP_OUT_A, LERP_OUT_B, STRUT, TYRE_SCRATCH, UNK_C04, UNK_C08, UNK_C0C,
};
use super::host::{BlendCalls, EmbeddedCalls, EmitCalls, SubBlockInit};
use super::probe::WheelTuning;
use super::store::WheelStore;
use crate::math::vec::Vec4;
use crate::mem::Mem;
use crate::physics_core::material;
use crate::rigid_body::host::HostUnavailable;
use crate::wheel::axes::Trig;
use crate::wheel::sector;
use crate::{ntweak, spring_damper, tyre};

/// Copies the dword at `from` to `to`.
fn copy_word(mem: &mut Mem, from: u32, to: u32) {
    let v = mem.u32(from);
    mem.set_u32(to, v);
}

/// The calls `CopyCF30` makes outside the wheel, one method each (the sine and cosine are `axes::Trig`). The
/// objects are addresses, `this`-relative.
pub trait DefCopyCalls {
    /// `CSpringDamper::CopyStateToDef(b)` (0x52dbd0, thiscall, one stack word) on the strut object `strut` (`this+0xec4`).
    fn copy_state_to_def(&mut self, mem: &mut Mem, strut: u32, b: u32);
    /// `CTyre::CScratch::CopyDefFrom(c)` (0x534760, thiscall, one stack word) on the tyre scratch `scratch` (`this+0xf00`).
    fn copy_def_from(&mut self, mem: &mut Mem, scratch: u32, c: u32);
    /// `NTweak::CBase::SetDirty` (0x52f450, thiscall, no arguments) on the CDef's base object `base` (`def+0x3f0`).
    fn set_dirty(&mut self, mem: &mut Mem, base: u32);
}

/// The three calls as the ports make them: `spring_damper::layout::apply_setup`, `tyre::layout::copy_def_from` and
/// `ntweak::set_dirty`.
pub struct Ported;

impl DefCopyCalls for Ported {
    fn copy_state_to_def(&mut self, mem: &mut Mem, strut: u32, b: u32) {
        spring_damper::layout::apply_setup(mem, strut, b);
    }

    fn copy_def_from(&mut self, mem: &mut Mem, scratch: u32, c: u32) {
        tyre::layout::copy_def_from(mem, scratch, c);
    }

    fn set_dirty(&mut self, mem: &mut Mem, base: u32) {
        ntweak::set_dirty(mem, base);
    }
}

/// `E51Wheel::CopyCF30` (0x53cf30, thiscall, stack `a`, `b`, `c`). The strut copies its state from `b` and the tyre
/// scratch copies its CDef from `c` (host calls); then `a`'s words go to the CDef, the vector at `a+0x30` (null when `a`
/// is -0x20, as the asm tests `a+0x20`), `SetDirty` on the CDef's base (a host call), `f1 + a+0x5c` to `+0x3fc`, and the
/// sine and cosine of `a+0x6c` to `+0x3d8` and `+0x3d0`. `+0x3d4` is zeroed and `a+0x6c` is copied to `+0x900`.
pub fn copy_cf30(host: &mut impl DefCopyCalls, trig: &mut impl Trig, mem: &mut Mem, this: u32, a: u32, b: u32, c: u32) {
    host.copy_state_to_def(mem, this + STRUT, b);
    host.copy_def_from(mem, this + TYRE_SCRATCH, c);
    let def = mem.u32(this + DEF);
    copy_word(mem, a + CF_SRC_44, def + CF_DEF_234);
    copy_word(mem, a + CF_SRC_48, def + CF_DEF_238);
    copy_word(mem, a + CF_SRC_4C, def + CF_DEF_23C);
    let source = if a.wrapping_add(CF_SRC_20) == 0 { 0 } else { a + CF_SRC_VEC };
    Vec4::load(mem, source).store(mem, def + CF_DEF_VEC);
    copy_word(mem, a + CF_SRC_54, def + CF_DEF_414);
    copy_word(mem, a + CF_SRC_58, def + CF_DEF_418);
    copy_word(mem, a + CF_SRC_5C, def + CF_DEF_41C);
    let sum = mem.f32(def + CF_DEF_F1) + mem.f32(a + CF_SRC_5C);
    host.set_dirty(mem, def + CF_DEF_BASE);
    mem.set_f32(def + CF_DEF_SUM, sum);
    let angle = f32::from_bits(mem.u32(a + CF_SRC_ANG));
    let sine = trig.sin(angle) as f32;
    let cosine = trig.cos(angle) as f32;
    mem.set_f32(def + CF_DEF_COS, cosine);
    mem.set_f32(def + CF_DEF_ZERO, 0.0);
    mem.set_f32(def + CF_DEF_SIN, sine);
    copy_word(mem, a + CF_SRC_ANG, def + CF_DEF_900);
}

/// `CWheel::Unk9900` (0x539900, `blend_tags`): the material entries of the two tags `x` and `y` (from the CDef's manager),
/// the scratch's coefficients `BlendTable` returns, the blend of the two entries on the blend ops, then the stores of `x`,
/// `y` and `z`. The material entries are read before the host calls, as the asm reads them.
pub fn blend_tags<S: WheelStore>(host: &mut impl BlendCalls, mem: &mut S, this: u32, x: u32, y: u32, z: f32) {
    let manager = mem.world_def() + DEF_MATERIALS;
    let s1 = material::tag_entry(mem.mem(), manager, x);
    let s2 = material::tag_entry(mem.mem(), manager, y);
    let (a, b) = host.blend_coefficients(
        this + TYRE_SCRATCH,
        mem.u32(s1 + 0x148),
        mem.u32(s2 + 0x148),
        mem.f32(s1 + 0xE8),
        mem.f32(s2 + 0xE8),
    );
    host.blend_materials(this + BLEND_OPS, s1, s2, a, b, z);
    mem.set_u32(this + UNK_C04, x);
    mem.set_u32(this + UNK_C08, y);
    mem.set_f32(this + UNK_C0C, z);
}

/// `CWheel::Unk9660` (0x539660, `uint_select`, thiscall with `p` and `n`): `sector::pick_sector` of `this+0x5ac`, written
/// to `p` in the asm's order (`p[0]`, the float fraction at `p[2]`, then `p[1]`). Returns the wrap count.
pub fn uint_select<S: WheelStore>(mem: &mut S, this: u32, p: u32, n: u32) -> u32 {
    let pick = sector::pick_sector(mem.load_angle(this), n);
    mem.set_u32(p, pick.first);
    mem.set_f32(p + 8, pick.first_weight);
    mem.set_u32(p + 4, pick.next);
    pick.next_wraps
}

/// `CWheel::Unk96e0` (0x5396e0, thiscall, plain `ret`): `Init` on the local `li` (a host call), `uint_select` with `n = 4`
/// into `li`, then the two lerps of the tyre block's tables. Column A goes to `this+0xd70` (weight 0.15), column B to
/// `this+0xd74` (weight 0); both from the base 0.2. NGP makes the base and the first weight per-car (`tuning`).
/// Returns the tyre block pointer, the asm's last `eax`.
pub fn table_lerp_pair<S: WheelStore>(
    init: &mut impl SubBlockInit,
    mem: &mut S,
    this: u32,
    li: u32,
    tuning: WheelTuning,
) -> u32 {
    init.init_sub_block(li);
    uint_select(mem, this, li, 4);
    let block = mem.load_block(this);
    let first = mem.u32(li);
    let next = mem.u32(li + 4);
    let first_weight = mem.f32(li + 8);
    let a = sector::interpolate(
        mem.f32(block + BLOCK_LERP_A + 4 * first),
        mem.f32(block + BLOCK_LERP_A + 4 * next),
        first_weight,
    );
    mem.set_f32(this + LERP_OUT_A, sector::mix_toward(a, tuning.pose, tuning.weight));
    let b = sector::interpolate(
        mem.f32(block + BLOCK_LERP_B + 4 * first),
        mem.f32(block + BLOCK_LERP_B + 4 * next),
        first_weight,
    );
    mem.set_f32(this + LERP_OUT_B, sector::mix_toward(b, tuning.pose, 0.0));
    block
}

/// `E51Wheel::M3B5B0` (0x53b5b0, the wheel's slot 16, thiscall, no stack words): `CSolid::Vf40` on the wheel, then a tail
/// jump to slot 17 of the embedded object at `this+0x3d0`. Returns that tail call's eax, which the wheel returns.
pub fn embedded_dispatch(host: &mut impl EmbeddedCalls, this: u32) -> Result<u32, HostUnavailable> {
    host.solid_vf40(this)?;
    host.embedded_slot17(this + EMBEDDED_OBJ)
}

/// `CMultiSignal1<CSigTireParticles>` of `E51MultiTire::Emit` (0x53d0d0): the guard word, the count pointer, the slots
/// (16 bytes each, from +8, one per count) and the overflow list head at +0x208. The template layout is the physics
/// core's signal layout (`physics_core::signal`).
const TIRE_GUARD: u32 = 0x0;
const TIRE_COUNT: u32 = 0x4;
const TIRE_SLOTS: u32 = 0x8;
const TIRE_OVERFLOW: u32 = 0x208;
/// A signal has 32 slots; the count's limit.
const TIRE_SLOT_LIMIT: u32 = 0x20;
/// A connection's `Invoke` is vtable slot 1 (+4); its list link is at +4.
const INVOKE_SLOT: u32 = 4;
const CONN_NEXT: u32 = 4;

/// `E51MultiTire::Emit` (0x53d0d0, thiscall, `ret 4`): `sig` goes to each connection of the overflow list, then of the
/// slot the count selects. A null count pointer returns silently. A set guard logs and returns. An out-of-range count logs
/// and returns with the guard still set. The original's log calls leave their five arguments on the stack, so they do not
/// return into the emit; the port stops after each log for the same reason.
pub fn tire_emit<S: WheelStore>(mem: &mut S, host: &mut impl EmitCalls, this: u32, sig: u32) {
    let count_at = mem.u32(this + TIRE_COUNT);
    if count_at == 0 {
        return;
    }
    if mem.u32(this + TIRE_GUARD) != 0 {
        host.log_recursive();
        return;
    }
    mem.set_u32(this + TIRE_GUARD, 1);
    let count = mem.u32(count_at);
    if count >= TIRE_SLOT_LIMIT {
        host.log_range(count);
        return;
    }
    let slot = this + TIRE_SLOTS + 16 * count;
    let overflow = this + TIRE_OVERFLOW;
    if mem.u32(slot) != 0 || mem.u32(overflow) != 0 {
        for head in [overflow, slot] {
            let mut conn = mem.u32(head);
            while conn != 0 {
                let target = mem.u32(mem.u32(conn) + INVOKE_SLOT);
                host.invoke(target, conn, sig);
                conn = mem.u32(conn + CONN_NEXT);
            }
        }
    }
    mem.set_u32(this + TIRE_GUARD, 0);
}

//! `E51Wheel::M3B900` (0x53b900), the pose refresh, and the CDef words it reads.

use super::fields::{
    AXIS_OUT, COPY_BLOCK, COPY_OFFSET, SCALED_VEC, STATE_LERP, TYRE_SCRATCH, WALK_OFFSET, WALK_RATIO_DEN,
    WALK_RATIO_NUM,
};
pub use crate::car_body::layout::CURRENT_CAR as CAR_GLOBAL;
use crate::car_body::store::CarStore;
use crate::math::quat;
use crate::math::vec::{Vec3, Vec4};
pub use crate::physics_core::layout::PHYSICS_GLOBAL as DEBUG_GLOBAL;
use crate::wheel::refresh::{self, RefreshIn};
use crate::wheel::store::WheelStore;

// The refresh (`M3B900`, `refresh.rs`): the CDef words it reads, and its inputs from the wheel and the tyre block.
/// The CDef words of the refresh (`+0x23c` height, `+0x244`, `+0x248`, `+0x24c` bits, `+0x880` point, `+0x890` axis).
pub const DEF_HEIGHT: u32 = 0x23C;
pub const DEF_BITS_244: u32 = 0x244;
pub const DEF_BITS_248: u32 = 0x248;
pub const DEF_BITS_24C: u32 = 0x24C;
pub const DEF_POINT: u32 = 0x880;
pub const DEF_AXIS: u32 = 0x890;
/// `+0x36c` (the sine gain), `+0x3fc`, `+0x3ec` (the terms of `A` and `B`) and `+0x35c` (the last scale).
pub const DEF_GAIN_36C: u32 = 0x36C;
pub const DEF_OFFSET_3FC: u32 = 0x3FC;
pub const DEF_TERM_3EC: u32 = 0x3EC;
pub const DEF_SCALE_35C: u32 = 0x35C;
/// `+0x380` and `+0x3a0`: the two weight vectors (three lanes each).
pub const DEF_WEIGHTS_380: u32 = 0x380;
pub const DEF_WEIGHTS_3A0: u32 = 0x3A0;
/// `+0x21c` (the flag for `GetB0C`), `+0x42c` (the contact scale), `+0x8b0`, `+0x8c0` (the vectors), `+0x8c4`, `+0x8c8`
/// (the rates) and `+0x8d0` (the offset).
pub const DEF_HAS_B0C: u32 = 0x21C;
pub const DEF_SCALE_42C: u32 = 0x42C;
pub const DEF_V8B0: u32 = 0x8B0;
pub const DEF_V8C0: u32 = 0x8C0;
pub const DEF_RATE_8C4: u32 = 0x8C4;
pub const DEF_RATE_8C8: u32 = 0x8C8;
pub const DEF_OFFSET_8D0: u32 = 0x8D0;
/// The tyre block's `+8` word (the factor of the strut's first offset).
pub const BLOCK_8: u32 = 0x8;

/// `this+0x5d0`: the object `SetDirty` runs on (`EBX` in the asm after the prologue).
pub const DIRTY_OBJECT: u32 = 0x5D0;
/// `this+0x5dc`: the dirty height (`E+0x18`), stored after `SetDirty`.
pub const DIRTY_HEIGHT: u32 = 0x5DC;
/// `this+0x5e4`: the three CDef words the refresh copies (`+0x244`, `+0x248`, `+0x24c`).
pub const REFRESH_COPY: u32 = 0x5E4;
/// `this+0x990`: `-c`, the third row of the frame (`E+0x100`) times `-1`.
pub const NEG_THIRD_ROW: u32 = 0x990;
/// `this+0x9b0`: the contact point (`E+0x40`).
pub const CONTACT: u32 = 0x9B0;
/// `this+0x9d0`: the normalised `+0x9e0 * d42c + +0x140` less the CDef's `+0x8d0`.
pub const UNIT_OFFSET: u32 = 0x9D0;
/// `this+0xd80`, `+0xd90`, `+0xda0`: the three frame rows, copied under the debug flag.
pub const DEBUG_ROWS: u32 = 0xD80;
/// `this+0xe50`: the frame point `E+0x1f0` (`E+0xc0` plus the contact).
pub const POINT_E50: u32 = 0xE50;
/// `this+0xe60`: the offset `(B0 - contact) - E+0xc0`.
pub const OFFSET_E60: u32 = 0xE60;

/// The original calls `E51Wheel::M3B900` makes: `SetDirty` (0x52f450), `TyreTail_Get10` (0x5326b0) and the wheel's
/// vtable slot 0x40 (`Vf40`, thiscall, no arguments).
pub trait RefreshCalls {
    /// `NTweak::CBase::SetDirty` (0x52f450) on `object` (no arguments).
    fn set_dirty(&mut self, object: u32);
    /// `TyreTail_Get10` (0x5326b0) on the tyre container: the tail pointer.
    fn tyre_tail(&mut self, container: u32) -> u32;
    /// The wheel's vtable slot 0x40, called on `wheel`; returns eax.
    fn vf40(&mut self, wheel: u32) -> u32;
}

/// `E51Wheel::M3B900` (0x53b900) after its prologue calls: `state` (`M3AB90`) is stored at `+0x570`, `axis` (`M3A9E0`)
/// at `+0x5f0`, and both are read back here. The sequence is the asm's: the stores and host calls in order. Returns the
/// eax of the final virtual call.
pub fn refresh<S: WheelStore + CarStore>(mem: &mut S, calls: &mut impl RefreshCalls, this: u32, travel: f32) -> u32 {
    let input = RefreshIn {
        travel,
        def: mem.load_refresh_def(this),
        block_8: mem.load_block_f32(this, BLOCK_8),
        state: mem.load_at(this, STATE_LERP),
        axis: mem.load_at(this, AXIS_OUT),
    };
    let f = refresh::strut_frame(&input);
    calls.set_dirty(this + DIRTY_OBJECT);
    mem.store_f32(this, DIRTY_HEIGHT, f.dirty_height);
    mem.store_u32(this, REFRESH_COPY, input.def.bits_244);
    mem.store_u32(this, REFRESH_COPY + 4, input.def.bits_248);
    mem.store_u32(this, REFRESH_COPY + 8, input.def.bits_24c);
    let lift = refresh::lift_frame(&input, &f);
    let k = if input.def.has_b0c { mem.target_b0c(mem.u32(CAR_GLOBAL)) } else { 0.0 };
    let fr = refresh::frame(&input, &f, &lift, k, f.dirty_height);
    mem.store_at(this, COPY_OFFSET, fr.pos);
    (Vec3 { x: fr.rot[0][0], y: fr.rot[0][1], z: fr.rot[0][2] }).store(mem, this + SCALED_VEC);
    let tail = calls.tyre_tail(this + TYRE_SCRATCH);
    let s = 0.0 - mem.f32(tail + 0x6C);
    let (pos, unit) =
        refresh::after_tyre(mem.load_at(this, SCALED_VEC), s, fr.pos, input.def.scale_42c, input.def.offset_8d0);
    mem.store_at(this, COPY_OFFSET, pos);
    mem.store_at(this, NEG_THIRD_ROW, fr.c_neg);
    mem.store_at(this, WALK_OFFSET, fr.offset);
    mem.store_at(this, CONTACT, fr.contact);
    mem.store_at(this, POINT_E50, fr.v1f0);
    mem.store_at(this, OFFSET_E60, fr.dv);
    mem.store_f32(this, WALK_RATIO_NUM, fr.inv_a);
    mem.store_f32(this, WALK_RATIO_DEN, fr.inv_b);
    mem.store_at(this, UNIT_OFFSET, unit);
    if mem.debug_flag() {
        for (k, row) in fr.rows.into_iter().enumerate() {
            mem.store_at(this, DEBUG_ROWS + 0x10 * k as u32, row);
        }
    }
    let q = quat::to_quat(&fr.rot);
    mem.store_at(this, COPY_BLOCK, Vec4 { x: q.x, y: q.y, z: q.z, w: q.w });
    calls.vf40(this)
}

// The ground probe (`M3D520`): three ray casts from the wheel's points, each with its block, then the blend of the hits.

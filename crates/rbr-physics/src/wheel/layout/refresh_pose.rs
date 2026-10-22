//! `E51Wheel::M3B900` (0x53b900), the pose refresh, and the CDef words it reads.

use super::fields::{
    AXLE, DEFORMED_TOP_MOUNT, HUB_POSITION, INV_ARM, INV_RACK, ORIENTATION, STRUT_DIR, TAIL_HUB_OFFSET, TYRE_SCRATCH,
    WOBBLED_AXLE,
};
use super::store::WheelStore;
pub use crate::car_body::layout::CURRENT_CAR as CAR_GLOBAL;
use crate::car_body::store::CarStore;
use crate::math::quat;
use crate::math::vec::{Vec3, Vec4};
pub use crate::physics_core::layout::PHYSICS_GLOBAL as DEBUG_GLOBAL;
use crate::wheel::refresh::{self, PoseIn};

// The refresh (`M3B900`, `refresh.rs`): the CDef words it reads, and its inputs from the wheel and the tyre block.
/// The CDef words of the refresh (`+0x23c` `SteeringRodLength`, `+0x244`, `+0x248` the header words and `+0x24c` the value of
/// `WishboneLength`, `+0x880` the joint mount, `+0x890` the joint axis).
pub const DEF_STEERING_ROD_LENGTH: u32 = 0x23C;
pub const DEF_WISHBONE_HEADER: u32 = 0x244;
pub const DEF_WISHBONE_LENGTH: u32 = 0x24C;
pub const DEF_JOINT_MOUNT: u32 = 0x880;
pub const DEF_JOINT_AXIS: u32 = 0x890;
/// `+0x36c` (`WishboneSign`), `+0x3fc` (the real after `StrutDimX`), `+0x3ec` (`StrutDimX`) and `+0x35c` (`SteeringSign`).
pub const DEF_WISHBONE_SIGN: u32 = 0x36C;
pub const DEF_STRUT_DIM_Z: u32 = 0x3FC;
pub const DEF_STRUT_DIM_X: u32 = 0x3EC;
pub const DEF_STEERING_SIGN: u32 = 0x35C;
/// `+0x380` and `+0x3a0`: `vecStrutRodMount` and `vecStrutHubPoint` (three lanes each).
pub const DEF_STRUT_ROD_MOUNT: u32 = 0x380;
pub const DEF_STRUT_HUB_POINT: u32 = 0x3A0;
/// `+0x21c` (`IsSteered`), `+0x42c` (`DriveAxleMountOffset`), `+0x8b0` and `+0x8c0` (the steering mount and axis; the
/// axis' y and z are the words `+0x8c4` and `+0x8c8`) and `+0x8d0` (the drive axle's joint).
pub const DEF_IS_STEERED: u32 = 0x21C;
pub const DEF_DRIVE_AXLE_MOUNT_OFFSET: u32 = 0x42C;
pub const DEF_STEERING_MOUNT: u32 = 0x8B0;
pub const DEF_STEERING_AXIS: u32 = 0x8C0;
pub const DEF_DRIVE_AXLE_JOINT: u32 = 0x8D0;
/// The damage record's `+8` word (the steering damage: 0.015 of it shortens the steering rod).
pub const BLOCK_STEERING_DAMAGE: u32 = 0x8;

/// `this+0x5d0`: the object `SetDirty` runs on (the live copy of `SteeringRodLength`).
pub const STEERING_ROD_TWEAK: u32 = 0x5D0;
/// `this+0x5dc`: the steering rod's length after damage, stored after `SetDirty`.
pub const STEERING_ROD_LENGTH: u32 = 0x5DC;
/// `this+0x5e4`: the three words of `WishboneLength` the refresh copies (`+0x244`, `+0x248`, `+0x24c`).
pub const WISHBONE_COPY: u32 = 0x5E4;
/// `this+0x990`: the knuckle's third axis, negated.
pub const NEG_KNUCKLE_Z: u32 = 0x990;
/// `this+0x9b0`: the wishbone's tip.
pub const TIP: u32 = 0x9B0;
/// `this+0x9d0`: the drive axle's direction.
pub const DRIVE_AXLE_DIR: u32 = 0x9D0;
/// `this+0xd80`, `+0xd90`, `+0xda0`: the knuckle's three axes, copied under the debug flag.
pub const DEBUG_ROWS: u32 = 0xD80;
/// `this+0xe50`: the rod mount in the world.
pub const ROD_MOUNT: u32 = 0xE50;
/// `this+0xe60`: the steering rod.
pub const STEERING_ROD: u32 = 0xE60;

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

/// `E51Wheel::M3B900` (0x53b900) after its prologue calls: the deformed top mount (`M3AB90`) is stored at `+0x570`, the
/// wobbled axle (`M3A9E0`) at `+0x5f0`, and both are read back here. The sequence is the original's: the stores and host
/// calls in order. Returns the eax of the final virtual call.
pub fn refresh<S: WheelStore + CarStore>(mem: &mut S, calls: &mut impl RefreshCalls, this: u32, travel: f32) -> u32 {
    let input = PoseIn {
        travel,
        def: mem.load_suspension_def(this),
        steering_damage: mem.load_block_f32(this, BLOCK_STEERING_DAMAGE),
        top_mount: mem.load_at(this, DEFORMED_TOP_MOUNT),
        axle: mem.load_at(this, WOBBLED_AXLE),
    };
    let def = &input.def;
    let strut = refresh::strut_geometry(&input);
    calls.set_dirty(this + STEERING_ROD_TWEAK);
    mem.store_f32(this, STEERING_ROD_LENGTH, strut.steering_rod_length);
    mem.store_u32(this, WISHBONE_COPY, def.wishbone_header[0]);
    mem.store_u32(this, WISHBONE_COPY + 4, def.wishbone_header[1]);
    mem.store_u32(this, WISHBONE_COPY + 8, def.wishbone_length.to_bits());
    let linkage = refresh::linkage(&input, &strut);
    let steer_target = if def.is_steered { mem.steer_target(mem.u32(CAR_GLOBAL)) } else { 0.0 };
    let frame = refresh::hub_frame(&input, &strut, &linkage, steer_target, strut.steering_rod_length);
    mem.store_at(this, HUB_POSITION, frame.hub_position);
    (Vec3 { x: frame.rotation[0][0], y: frame.rotation[0][1], z: frame.rotation[0][2] }).store(mem, this + AXLE);
    let tail = calls.tyre_tail(this + TYRE_SCRATCH);
    let (hub_position, drive_axle_dir) = refresh::place_hub(
        mem.load_at(this, AXLE),
        mem.f32(tail + TAIL_HUB_OFFSET),
        frame.hub_position,
        def.drive_axle_mount_offset,
        def.drive_axle_joint,
    );
    mem.store_at(this, HUB_POSITION, hub_position);
    mem.store_at(this, NEG_KNUCKLE_Z, frame.neg_knuckle_z);
    mem.store_at(this, STRUT_DIR, frame.strut_dir);
    mem.store_at(this, TIP, frame.tip);
    mem.store_at(this, ROD_MOUNT, frame.rod_mount);
    mem.store_at(this, STEERING_ROD, frame.steering_rod);
    mem.store_f32(this, INV_ARM, frame.inv_arm);
    mem.store_f32(this, INV_RACK, frame.inv_rack);
    mem.store_at(this, DRIVE_AXLE_DIR, drive_axle_dir);
    if mem.debug_flag() {
        for (k, axis) in frame.knuckle_axes.into_iter().enumerate() {
            mem.store_at(this, DEBUG_ROWS + 0x10 * k as u32, axis);
        }
    }
    let q = quat::to_quat(&frame.rotation);
    mem.store_at(this, ORIENTATION, Vec4 { x: q.x, y: q.y, z: q.z, w: q.w });
    calls.vf40(this)
}

// The ground probe (`M3D520`): three ray casts from the wheel's points, each with its block, then the blend of the hits.

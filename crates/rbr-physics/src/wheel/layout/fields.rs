//! The wheel's member offsets (`CWheel`, its CDef, its tyre block and tail) and the small loaders and stores over
//! them.

use super::refresh_pose::{
    DEBUG_GLOBAL, DEF_DRIVE_AXLE_JOINT, DEF_DRIVE_AXLE_MOUNT_OFFSET, DEF_IS_STEERED, DEF_JOINT_AXIS, DEF_JOINT_MOUNT,
    DEF_STEERING_AXIS, DEF_STEERING_MOUNT, DEF_STEERING_ROD_LENGTH, DEF_STEERING_SIGN, DEF_STRUT_DIM_X,
    DEF_STRUT_DIM_Z, DEF_STRUT_HUB_POINT, DEF_STRUT_ROD_MOUNT, DEF_WISHBONE_HEADER, DEF_WISHBONE_LENGTH,
    DEF_WISHBONE_SIGN,
};
use super::store::WheelStore;
use crate::math::vec::{Vec3, Vec4};
use crate::mem::Mem;
use crate::wheel::axes::{AxleWobbleIn, HubDamageIn, HubDamageUpdate, Inclination, InclinedAxle, TopMountDeformIn};
use crate::wheel::contact::RayBlock;
use crate::wheel::refresh::SuspensionDef;

/// `CWheel::m_car` (`+0x7f4`): the car this wheel belongs to (the receiver of `Xform6870`).
pub const CAR: u32 = 0x7F4;
/// The tyre container (`+0xf00`) and its tail pointer, read by `TyreTail_Get10` (`*(container + 0x10)`, 0x5326b0).
pub const TYRE_TAIL: u32 = 0xF10;
/// Vector `this+0x140`: the hub position (`M3B900` stores it; `Copy100` moves it along the axle by the tyre's hub offset).
pub const HUB_POSITION: u32 = 0x140;
/// Vector `this+0x100`: the wheel's rotation as a quaternion (`M3B900` stores it; `Copy100` copies it to its output).
pub const ORIENTATION: u32 = 0x100;
/// Vector `this+0xa90`: the axis the frame records are dotted with (lanes 0..2 are `a90`, `a94`, `a98`), and the vector
/// `Unk9290` scales by `m` into its output. Working name.
pub const FRAME_AXIS: u32 = 0xA90;
/// Vector `this+0xa70`: a contact vector `Unk9290` adds (scaled by its `second` term). Working name.
pub const CONTACT_A70: u32 = 0xA70;
/// Vector `this+0xa80`: a contact vector `Unk9290` adds (scaled by its `first` term). Working name.
pub const CONTACT_A80: u32 = 0xA80;
/// Vector `this+0xa20`: a copy of Blend4's `d` (0x539180). Working name.
pub const BLEND_D_COPY_A20: u32 = 0xA20;
/// Vector `this+0xa40`: a copy of Blend4's `c`. Working name.
pub const BLEND_C_COPY_A40: u32 = 0xA40;
/// Vector `this+0xde0`: a copy of Blend4's `c`. Working name.
pub const BLEND_C_COPY_DE0: u32 = 0xDE0;
/// Vector `this+0xe00`: a copy of Blend4's `d`. Working name.
pub const BLEND_D_COPY_E00: u32 = 0xE00;

/// The tyre's `HubOffset` (`tail+0x6c`), which `Copy100` moves the hub along the axle by.
pub const TAIL_HUB_OFFSET: u32 = 0x6C;
/// The tyre's `CarcassWidth` (`tail+0xcc`), which `Blend4` halves.
pub const TAIL_CARCASS_WIDTH: u32 = 0xCC;

/// `this+0x34`: the CDef, the wheel's setup (`CWheel::m_def`, `RBR_E51_WheelA.cpp:41`).
pub const DEF: u32 = 0x34;
/// `this+0x44`: the car `M3A8A0` makes its transforms on (`RBR_E51_WheelA.cpp:41`), kept when its flags carry bit 3.
pub const XFORM_CAR: u32 = 0x44;
/// `car+0x60`: the car's flags byte; bit 3 (`CAR_FLAG_XFORM`) lets `M3A8A0` use the car (`RBR_E51_WheelA.cpp:43`).
pub const CAR_FLAGS: u32 = 0x60;
/// The bit of `car+0x60` that `M3A8A0` tests (`testb $0x8`).
pub const CAR_FLAG_XFORM: u8 = 0x8;
/// The car transforms `M3A8A0` makes: for each pair, the guard (`def + guard` is null when zero), the source and the output
/// offsets from the CDef, in the order of the calls (`RBR_E51_WheelA.cpp:50-67`). The transform kinds are in `xform.rs`.
pub const XFORM_PAIRS: [(u32, u32, u32); 8] = [
    (0x310, 0x320, 0x8E0),
    (0x330, 0x340, 0x8F0),
    (0x250, 0x260, 0x880),
    (0x270, 0x280, 0x890),
    (0x290, 0x2A0, 0x8A0),
    (0x2B0, 0x2C0, 0x8B0),
    (0x2D0, 0x2E0, 0x8C0),
    (0x2F0, 0x300, 0x8D0),
];

// The wheel's damage record (`this+0x7f0`, a float block: `RBR_E51_WheelA.cpp:74`, `RBR_E51_WheelB.cpp:73`,
// `RBR_E50_WheelSim.cpp:570`). The step raises `+0` with the force on the top mount, and `M3AC50` moves `+0x10` with the
// hub torque, clamped to [-1, 1]; the other words are named by use.
/// `this+0x7f0`: the damage record pointer.
pub const TYRE_BLOCK: u32 = 0x7F0;
/// `block+0x0`: the top mount's damage along the up axis, the factor of the `+0x570` lerp (`RBR_E51_WheelA.cpp:128`).
pub const BLOCK_UP_DAMAGE: u32 = 0x0;
/// `block+0x4`: the top mount's damage along the side axis, the factor of the second `+0x570` lerp
/// (`RBR_E51_WheelA.cpp:138`).
pub const BLOCK_SIDE_DAMAGE: u32 = 0x4;
/// `block+0x10`: the hub damage the axis angle and the setter use (`RBR_E51_WheelA.cpp:74`).
pub const BLOCK_INPUT: u32 = 0x10;

/// `this+0x570`: the vector `M3AB90` lerps from the CDef's `+0x8e0`, `+0x8a0` and `+0x8f0` (`RBR_E51_WheelA.cpp:124`).
pub const DEFORMED_TOP_MOUNT: u32 = 0x570;
/// `this+0x5ac`: the angle `M3A9E0` takes the sine and cosine of (`RBR_E51_WheelA.cpp:77`).
pub const WHEEL_ANGLE: u32 = 0x5AC;
/// `this+0x5f0`: the vector `M3A9E0` writes and normalises (`RBR_E51_WheelA.cpp:78-89`); lane 3 is kept from the old value.
pub const WOBBLED_AXLE: u32 = 0x5F0;
/// `this+0x600`: the axis base `M3A9E0` adds to its scaled turn vector, and `M3AB00` writes (cos, 0, sin).
pub const INCLINED_AXLE: u32 = 0x600;
/// `this+0x610`: the turn vector `M3A9E0` scales by the cosine term (`RBR_E51_WheelA.cpp:82`); `M3AB00` writes (-sin, 0, cos).
pub const INCLINED_SIDE: u32 = 0x610;
/// `this+0x9c0`: a float of the walk's scale (`M3AEF0`, `RBR_E51_WheelA.cpp:229-236`). Working name `walk_ratio_num`.
pub const INV_ARM: u32 = 0x9C0;
/// `this+0x9c4`: the denominator of `INV_ARM` and the magnitude `M3AEF0` tests against 0.001. Working name.
pub const INV_RACK: u32 = 0x9C4;
/// `this+0x9a0`: the vector `M3AEF0` adds, scaled by its gain, to `this+0xc10` (`RBR_E51_WheelA.cpp:241-244`).
pub const STRUT_DIR: u32 = 0x9A0;
/// `this+0xc10`: the vector `M3AEF0` walks into (`MB560`'s `c`) and then adds to; `M3AFB0` reads it. Working name.
pub const WALK_OUT: u32 = 0xC10;
/// `this+0xa50`: the spin rate `M3AFB0` scales the vectors by (`RBR_E51_WheelA.cpp:247-250`).
pub const SPIN_RATE: u32 = 0xA50;
/// `this+0x9e0`: the wheel axle, the first row of the wheel's rotation (`M3AFB0` scales it by the spin, `Copy100` by the hub offset).
pub const AXLE: u32 = 0x9E0;

/// CDef `+0x8e0`, `+0x8a0`, `+0x8f0`: the lerp inputs of `M3AB90` (`RBR_E51_WheelA.cpp:126-137`).
pub const DEF_TOP_MOUNT_UP_AXIS: u32 = 0x8E0;
pub const DEF_TOP_MOUNT: u32 = 0x8A0;
pub const DEF_TOP_MOUNT_SIDE_AXIS: u32 = 0x8F0;
/// CDef `+0x920` and `+0x910`: the scales of the two `M3AB90` lerps (`RBR_E51_WheelA.cpp:128`, `:138`).
pub const DEF_MAX_DEFORM_UP: u32 = 0x920;
pub const DEF_MAX_DEFORM_SIDE: u32 = 0x910;
/// CDef `+0x950` and `+0x900`: the scale and offset of the axis angle in `M3AB00` (`RBR_E51_WheelA.cpp:106-109`).
pub const DEF_INCLINATION_DAMAGE_ANGLE: u32 = 0x950;
pub const DEF_AXIS_INCLINATION: u32 = 0x900;
/// CDef `+0x960` and `+0x970`: the setter's lower bound and range end in `M3AC50` (`RBR_E51_WheelA.cpp:147-151`).
pub const DEF_HUB_DAMAGE_THRESHOLD: u32 = 0x960;
pub const DEF_HUB_DAMAGE_MAX_TORQUE: u32 = 0x970;

/// The tyre scratch (`CScratch`, `this+0xf00`) and the blend ops (`E50BlendOps`, `this+0xaa4`) that `Unk9900` uses.
pub const TYRE_SCRATCH: u32 = 0xF00;
pub const BLEND_OPS: u32 = 0xAA4;
/// `Unk9900`'s stored tags and weight (`m_unkC04`, `m_unkC08`, `m_unkC0C`): working names.
pub const UNK_C04: u32 = 0xC04;
pub const UNK_C08: u32 = 0xC08;
pub const UNK_C0C: u32 = 0xC0C;
/// `Vf20`'s two cleared vectors (`E09Vec4::Clear`, 0x465d80): working names.
pub const CLEAR_1C0: u32 = 0x1C0;
pub const CLEAR_190: u32 = 0x190;
/// `this+0xeb4`: the car's index of this wheel (`m_wheelIndex`, inferred). The car's calls take it, and the spring
/// damper's signal owner id is the same value.
pub const WHEEL_INDEX: u32 = 0xEB4;
/// `this+0xebc`: 1 when the ground probe found a contact, else 0 (`m_onGround`, inferred).
pub const ON_GROUND: u32 = 0xEBC;
/// `this+0xec0`: an int the input setters set to 1 (`M3AC50`, `RBR_E51_WheelA.cpp:159`) and the step clears through
/// `M3B880` (`RBR_E51_WheelB.cpp:74`). Working name `input_changed`.
pub const INPUT_CHANGED: u32 = 0xEC0;
/// `E43Vf5C`'s two vectors (passed to the car, then cleared) and the int zeroed after them (0x539a90). Working names.
pub const FORWARD_VEC_9F0: u32 = 0x9F0;
pub const FORWARD_VEC_A00: u32 = 0xA00;
pub const FORWARD_INT_A10: u32 = 0xA10;
/// `CopyCF30`'s source `a` (the words it copies) and the CDef words it writes (0x53cf30). Working names.
pub const CF_SRC_20: u32 = 0x20;
pub const CF_SRC_VEC: u32 = 0x30;
pub const CF_SRC_44: u32 = 0x44;
pub const CF_SRC_48: u32 = 0x48;
pub const CF_SRC_4C: u32 = 0x4C;
pub const CF_SRC_54: u32 = 0x54;
pub const CF_SRC_58: u32 = 0x58;
pub const CF_SRC_5C: u32 = 0x5C;
pub const CF_SRC_ANG: u32 = 0x6C;
pub const CF_DEF_234: u32 = 0x234;
pub const CF_DEF_238: u32 = 0x238;
pub const CF_DEF_23C: u32 = 0x23C;
pub const CF_DEF_VEC: u32 = 0x2A0;
pub const CF_DEF_414: u32 = 0x414;
pub const CF_DEF_418: u32 = 0x418;
pub const CF_DEF_41C: u32 = 0x41C;
pub const CF_DEF_F1: u32 = 0x40C;
pub const CF_DEF_BASE: u32 = 0x3F0;
pub const CF_DEF_SUM: u32 = 0x3FC;
pub const CF_DEF_COS: u32 = 0x3D0;
pub const CF_DEF_ZERO: u32 = 0x3D4;
pub const CF_DEF_SIN: u32 = 0x3D8;
pub const CF_DEF_900: u32 = 0x900;
/// `Unk97a0`'s two sub-objects (`+0xd00` and `+0xc90`), each gated on by a flag and a scale (`+0xd04`/`+0xd5c` and
/// `+0xc94`/`+0xcec`).
pub const SUB_A: u32 = 0xD00;
pub const SUB_A_FLAG: u32 = 0xD04;
pub const SUB_A_SCALE: u32 = 0xD5C;
pub const SUB_B: u32 = 0xC90;
pub const SUB_B_FLAG: u32 = 0xC94;
pub const SUB_B_SCALE: u32 = 0xCEC;
/// The three lanes `Unk97a0` adds to its scaled cross product (`+0xa60`, `+0xa64`, `+0xa68`).
pub const CROSS_ADD: u32 = 0xA60;

/// The tyre block's offsets as `M3D230` and `M3D460` read them (the block is the argument itself; `RBR_E51_WheelB.cpp:151`).
/// `BLOCK_INPUT` (+0x10) is the first sine's input there, and `PROBE_CLEAR` (+0x4) is the flag `M3D460` sets.
pub const BLOCK_PHASE: u32 = 0x14;
/// The two 4-entry tables `Unk96e0` lerps, in the tyre block (`BLOCK_LERP_A` is `BLOCK_PHASE`).
pub const EMBEDDED_OBJ: u32 = 0x3D0;
pub const BLOCK_LERP_A: u32 = 0x14;
pub const BLOCK_LERP_B: u32 = 0x24;
/// The two outputs of `Unk96e0`, in the wheel.
pub const LERP_OUT_A: u32 = 0xD70;
pub const LERP_OUT_B: u32 = 0xD74;
pub const BLOCK_WEIGHT: u32 = 0x34;
pub const BLOCK_TAG_1: u32 = 0x38;
pub const BLOCK_TAG_2: u32 = 0x3C;
pub const BLOCK_TARGET: u32 = 0x40;
pub const BLOCK_BLEND: u32 = 0x54;
pub const BLOCK_TOTAL: u32 = 0x58;
pub const BLOCK_CLAMP_A: u32 = 0x5C;
pub const BLOCK_CLAMP_B: u32 = 0x60;
pub const BLOCK_DIFF: u32 = 0x64;

/// The CDef's inverse mass (`+0x16c`, `CSolid::CDef`'s InverseMass, `m_inverseMass`): the strut step's `m`.
pub const DEF_INVERSE_MASS: u32 = 0x16C;

/// The CPhysics global (`g_1659b70`, `car_body::layout::WORLD`) points at the CPhysics; its `+0x24` is the CDef
/// (`RBR_E51_WheelB.cpp:151`, `:203`).
pub const WORLD_DEF: u32 = 0x24;
/// The CDef's material manager (`+0x18`, `RBR_E51_WheelB.cpp:152`).
pub const DEF_MATERIALS: u32 = 0x18;
/// The CDef's flags word (`+0x160cc`) and the tag it gives the block's pair (`+0x1614c`, `RBR_E51_WheelB.cpp:203`).
pub const DEF_FLAGS: u32 = 0x160CC;
pub const DEF_PAIR_TAG: u32 = 0x1614C;
/// The flag bit that makes `M3D460` take the pair tag.
pub const FLAG_PAIR_TAG: u32 = 0x40000;
/// A material's fields `M3D230` reads: the divisor (`+0xd8`), the gain (`+0xc8`) and the two lerped values (`+0x18`,
/// `+0xb8`). The tag entry is `physics_core::layout::material::tag_entry`.
pub const MAT_PERIOD: u32 = 0xD8;
pub const MAT_GAIN: u32 = 0xC8;
pub const MAT_LERP_A: u32 = 0x18;
pub const MAT_LERP_B: u32 = 0xB8;

/// The wheel's strut, a `CSpringDamper` at `this+0xec4` (`RBR_E51_WheelB.cpp:131`, `M3D030`); its name object is read
/// through `CBase`'s `m_name` (+4, `RBR_E48_NTweak.h:183`).
pub const STRUT: u32 = 0xEC4;
pub const CBASE_NAME: u32 = 0x4;
/// The `CBase` whose dirty flag `M3D030` sets (`this+0x5b0`).
pub const STRUT_TWEAK: u32 = 0x5B0;
/// `this+0x5bc`: the strut's length: `M3D030` stores the blend of the name's floats here, and `M3DD30` and `M3B900`
/// read it as the strut's travel (`m_strutLength`, rbr-recomp `notes/physics/renames/wheel.csv`).
pub const STRUT_LENGTH: u32 = 0x5BC;
/// `this+0x5cc`: the strut's velocity, clamped to +-100 (`m_strutVel`, rbr-recomp `notes/physics/wheel.md` 2a).
pub const STRUT_VEL: u32 = 0x5CC;
/// The strut definition's `+0x28` (`InnerBumpRubberPos`, the bump stop) and `+0x38` (`OuterBumpRubberPos`, the droop stop).
pub const NAME_F28: u32 = 0x28;
pub const NAME_F38: u32 = 0x38;
// The typed inputs and outputs of the ports. Each `load_*` reads the game memory a port takes, and each `store_*` writes
// what the port returns; the ports never touch `Mem`.

/// The Mem store: the bodies are the layout's reads and writes.
impl WheelStore for Mem {
    /// Loads the ray block at `at` (the gate word is read first, as the original does).
    fn load_ray_block(&self, at: u32) -> RayBlock {
        let mem = self;
        RayBlock {
            point: (mem.u32(at) != 0).then(|| Vec4::load(mem, at + 0x20)),
            coord_a: mem.f32(at + 0x50),
            difference: mem.f32(at + 0x64),
        }
    }

    /// The tyre tail object of the wheel: the pointer at `this+0xf10`.
    fn tail(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + TYRE_TAIL)
    }

    /// The tyre's `HubOffset` (`tail+0x6c`).
    fn load_hub_offset(&self, this: u32) -> f32 {
        let mem = self;
        mem.f32(mem.tail(this) + TAIL_HUB_OFFSET)
    }

    /// The tyre's `CarcassWidth` (`tail+0xcc`).
    fn load_carcass_width(&self, this: u32) -> f32 {
        let mem = self;
        mem.f32(mem.tail(this) + TAIL_CARCASS_WIDTH)
    }

    /// The car pointer (`this+0x7f4`).
    fn load_car(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + CAR)
    }

    /// A wheel vector at `this + offset`.
    fn load_at(&self, this: u32, offset: u32) -> Vec4 {
        let mem = self;
        Vec4::load(mem, this + offset)
    }

    /// Stores a wheel vector at `this + offset`.
    fn store_at(&mut self, this: u32, offset: u32, v: Vec4) {
        let mem = self;
        v.store(mem, this + offset);
    }

    /// The CDef pointer (`this+0x34`).
    fn load_def(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + DEF)
    }

    /// The car `M3A8A0` reads (`this+0x44`), before its flag test.
    fn load_xform_car(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + XFORM_CAR)
    }

    /// The flags byte of a car (`car+0x60`).
    fn car_flags(&self, car: u32) -> u8 {
        let mem = self;
        mem.u8(car + CAR_FLAGS)
    }

    /// A float of the CDef at `off`.
    fn load_def_f32(&self, this: u32, off: u32) -> f32 {
        let mem = self;
        mem.f32(mem.load_def(this) + off)
    }

    /// The tyre block pointer (`this+0x7f0`).
    fn load_block(&self, this: u32) -> u32 {
        let mem = self;
        mem.u32(this + TYRE_BLOCK)
    }

    /// A float of the tyre block at `off` (`BLOCK_*`).
    fn load_block_f32(&self, this: u32, off: u32) -> f32 {
        let mem = self;
        mem.f32(mem.load_block(this) + off)
    }

    /// The angle `M3A9E0` reads (`this+0x5ac`).
    fn load_angle(&self, this: u32) -> f32 {
        let mem = self;
        mem.f32(this + WHEEL_ANGLE)
    }

    /// A float of this at `at` (the wheel's scalars).
    fn load_f32(&self, this: u32, off: u32) -> f32 {
        let mem = self;
        mem.f32(this + off)
    }

    /// Stores a float of this at `off`.
    fn store_f32(&mut self, this: u32, off: u32, value: f32) {
        let mem = self;
        mem.set_f32(this + off, value);
    }

    /// Stores a float of the tyre block at `off` (`BLOCK_*`).
    fn store_block_f32(&mut self, this: u32, off: u32, value: f32) {
        let mem = self;
        mem.set_f32(mem.load_block(this) + off, value);
    }

    /// Stores an int at `this + off`.
    fn store_u32(&mut self, this: u32, off: u32, value: u32) {
        let mem = self;
        mem.set_u32(this + off, value);
    }

    /// A vector of the CDef at `off` (`DEF_*`).
    fn load_def_vec(&self, this: u32, off: u32) -> Vec4 {
        let mem = self;
        Vec4::load(mem, mem.load_def(this) + off)
    }

    /// The CDef the CPhysics global points at.
    fn world_def(&self) -> u32 {
        let mem = self;
        mem.u32(mem.u32(crate::car_body::layout::WORLD) + WORLD_DEF)
    }

    fn load_axle_wobble(&self, this: u32) -> AxleWobbleIn {
        let mem = self;
        AxleWobbleIn {
            hub_damage: mem.load_block_f32(this, BLOCK_INPUT),
            axle: mem.load_at(this, INCLINED_AXLE),
            side: mem.load_at(this, INCLINED_SIDE),
            wheel_angle: mem.load_angle(this),
            previous_w: mem.f32(this + WOBBLED_AXLE + 12),
        }
    }

    fn store_wobbled_axle(&mut self, this: u32, v: Vec4) {
        let mem = self;
        mem.store_at(this, WOBBLED_AXLE, v);
    }

    fn load_inclination(&self, this: u32) -> Inclination {
        let mem = self;
        Inclination {
            hub_damage: mem.load_block_f32(this, BLOCK_INPUT),
            damage_angle: mem.load_def_f32(this, DEF_INCLINATION_DAMAGE_ANGLE),
            axis_inclination: mem.load_def_f32(this, DEF_AXIS_INCLINATION),
        }
    }

    fn store_inclined_axle(&mut self, this: u32, axle: &InclinedAxle) {
        let mem = self;
        axle.axle.store(mem, this + INCLINED_AXLE);
        axle.side.store(mem, this + INCLINED_SIDE);
    }

    fn load_top_mount_deform(&self, this: u32) -> TopMountDeformIn {
        let mem = self;
        TopMountDeformIn {
            up_damage: mem.load_block_f32(this, BLOCK_UP_DAMAGE),
            side_damage: mem.load_block_f32(this, BLOCK_SIDE_DAMAGE),
            max_up: mem.load_def_f32(this, DEF_MAX_DEFORM_UP),
            max_side: mem.load_def_f32(this, DEF_MAX_DEFORM_SIDE),
            up_axis: mem.load_def_vec(this, DEF_TOP_MOUNT_UP_AXIS),
            mount: mem.load_def_vec(this, DEF_TOP_MOUNT),
            side_axis: mem.load_def_vec(this, DEF_TOP_MOUNT_SIDE_AXIS),
        }
    }

    fn store_deformed_top_mount(&mut self, this: u32, v: Vec4) {
        let mem = self;
        mem.store_at(this, DEFORMED_TOP_MOUNT, v);
    }

    fn load_hub_damage(&self, this: u32, torque: f32) -> HubDamageIn {
        let mem = self;
        HubDamageIn {
            torque,
            threshold: mem.load_def_f32(this, DEF_HUB_DAMAGE_THRESHOLD),
            max_torque: mem.load_def_f32(this, DEF_HUB_DAMAGE_MAX_TORQUE),
            hub_damage: mem.load_block_f32(this, BLOCK_INPUT),
            inclination: mem.load_inclination(this),
        }
    }

    /// Stores the damage update and sets the changed flag. Nothing is written when `update` is `None`.
    fn store_hub_damage(&mut self, this: u32, update: Option<&HubDamageUpdate>) {
        let mem = self;
        if let Some(update) = update {
            mem.store_block_f32(this, BLOCK_INPUT, update.hub_damage);
            mem.store_inclined_axle(this, &update.axle);
            mem.store_u32(this, INPUT_CHANGED, 1);
        }
    }

    /// The CDef words `M3B900` reads (`this+0x34`).
    fn load_suspension_def(&self, this: u32) -> SuspensionDef {
        let mem = self;
        let def = mem.load_def(this);
        SuspensionDef {
            steering_rod_length: mem.f32(def + DEF_STEERING_ROD_LENGTH),
            wishbone_header: [mem.u32(def + DEF_WISHBONE_HEADER), mem.u32(def + DEF_WISHBONE_HEADER + 4)],
            wishbone_length: mem.f32(def + DEF_WISHBONE_LENGTH),
            joint_mount: Vec3::load(mem, def + DEF_JOINT_MOUNT),
            joint_axis: Vec4::load(mem, def + DEF_JOINT_AXIS),
            wishbone_sign: mem.f32(def + DEF_WISHBONE_SIGN),
            strut_dim_x: mem.f32(def + DEF_STRUT_DIM_X),
            strut_dim_z: mem.f32(def + DEF_STRUT_DIM_Z),
            steering_sign: mem.f32(def + DEF_STEERING_SIGN),
            strut_rod_mount: Vec3::load(mem, def + DEF_STRUT_ROD_MOUNT),
            strut_hub_point: Vec3::load(mem, def + DEF_STRUT_HUB_POINT),
            is_steered: mem.u32(def + DEF_IS_STEERED) != 0,
            drive_axle_mount_offset: mem.f32(def + DEF_DRIVE_AXLE_MOUNT_OFFSET),
            steering_mount: Vec4::load(mem, def + DEF_STEERING_MOUNT),
            steering_axis: Vec4::load(mem, def + DEF_STEERING_AXIS),
            drive_axle_joint: Vec3::load(mem, def + DEF_DRIVE_AXLE_JOINT),
        }
    }

    /// The debug flag word, `[[0x1659b70] + 0x24] + 0x160e0`: bit 2 is the debug flag `M3B900` tests, bit 0x400 gates the
    /// travel in `M3B880`.
    fn flag_word(&self) -> u32 {
        let mem = self;
        let global = mem.u32(DEBUG_GLOBAL);
        mem.u32(mem.u32(global + 0x24) + 0x160E0)
    }

    fn debug_flag(&self) -> bool {
        let mem = self;
        mem.flag_word() & 4 != 0
    }
}

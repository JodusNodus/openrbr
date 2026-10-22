//! The game memory of a wheel (`this`) and of its tyre tail, as the wheel's functions read it. The offsets are the only
//! raw ones in the wheel module: the ported functions take the typed values these loaders return.

mod copy_and_emit;
mod ctors;
mod fields;
mod host;
mod probe;
mod refresh_pose;
mod step;
mod store;
mod wheel_calls;

pub use copy_and_emit::{
    blend_tags, copy_cf30, embedded_dispatch, table_lerp_pair, tire_emit, uint_select, DefCopyCalls, Ported,
};
pub use ctors::{
    cdef_ctor, cdef_unk20, cdef_unk4c, construct_options, damage_zero, finish, new_cdef, new_wheel, refresh_travel,
    scratch_ctor, scratch_vf04, select_top_mount, setup_ctor, setup_vf20, setup_vf28, state_ctor, tyre_tail_setup,
    wheel_ctor, FinishCalls, WHEEL_SIZE,
};
pub use fields::{
    AXLE, BLEND_C_COPY_A40, BLEND_C_COPY_DE0, BLEND_D_COPY_A20, BLEND_D_COPY_E00, BLEND_OPS, BLOCK_BLEND,
    BLOCK_CLAMP_A, BLOCK_CLAMP_B, BLOCK_DIFF, BLOCK_INPUT, BLOCK_LERP_A, BLOCK_LERP_B, BLOCK_PHASE, BLOCK_SIDE_DAMAGE,
    BLOCK_TAG_1, BLOCK_TAG_2, BLOCK_TARGET, BLOCK_TOTAL, BLOCK_UP_DAMAGE, BLOCK_WEIGHT, CAR, CAR_FLAGS, CAR_FLAG_XFORM,
    CBASE_NAME, CF_DEF_234, CF_DEF_238, CF_DEF_23C, CF_DEF_414, CF_DEF_418, CF_DEF_41C, CF_DEF_900, CF_DEF_BASE,
    CF_DEF_COS, CF_DEF_F1, CF_DEF_SIN, CF_DEF_SUM, CF_DEF_VEC, CF_DEF_ZERO, CF_SRC_20, CF_SRC_44, CF_SRC_48, CF_SRC_4C,
    CF_SRC_54, CF_SRC_58, CF_SRC_5C, CF_SRC_ANG, CF_SRC_VEC, CLEAR_190, CLEAR_1C0, CONTACT_A70, CONTACT_A80, CROSS_ADD,
    DEF, DEFORMED_TOP_MOUNT, DEF_AXIS_INCLINATION, DEF_FLAGS, DEF_HUB_DAMAGE_MAX_TORQUE, DEF_HUB_DAMAGE_THRESHOLD,
    DEF_INCLINATION_DAMAGE_ANGLE, DEF_INVERSE_MASS, DEF_MATERIALS, DEF_MAX_DEFORM_SIDE, DEF_MAX_DEFORM_UP,
    DEF_PAIR_TAG, DEF_TOP_MOUNT, DEF_TOP_MOUNT_SIDE_AXIS, DEF_TOP_MOUNT_UP_AXIS, EMBEDDED_OBJ, FLAG_PAIR_TAG,
    FORWARD_INT_A10, FORWARD_VEC_9F0, FORWARD_VEC_A00, FRAME_AXIS, HUB_POSITION, INCLINED_AXLE, INCLINED_SIDE,
    INPUT_CHANGED, INV_ARM, INV_RACK, LERP_OUT_A, LERP_OUT_B, MAT_GAIN, MAT_LERP_A, MAT_LERP_B, MAT_PERIOD, NAME_F28,
    NAME_F38, ON_GROUND, ORIENTATION, SPIN_RATE, STRUT, STRUT_DIR, STRUT_LENGTH, STRUT_TWEAK, STRUT_VEL, SUB_A,
    SUB_A_FLAG, SUB_A_SCALE, SUB_B, SUB_B_FLAG, SUB_B_SCALE, TYRE_BLOCK, TYRE_SCRATCH, TYRE_TAIL, UNK_C04, UNK_C08,
    UNK_C0C, WALK_OUT, WHEEL_ANGLE, WHEEL_INDEX, WOBBLED_AXLE, WORLD_DEF, XFORM_CAR, XFORM_PAIRS,
};
pub use host::{
    blend_ops, construct, forward_vf6c, hook, transform_call, BlendCalls, CarSlotCalls, CarXform, ConstructCalls,
    EmbeddedCalls, EmitCalls, HookCalls, SlotCalls, SolidCalls, StrutCalls, SubBlockInit, SubCalls, WalkCalls,
};
pub use probe::{
    ground_probe, GroundCalls, ProbeTuning, WheelTuning, PROBE_CLEAR, PROBE_FV, PROBE_GATE, PROBE_HIT, PROBE_HIT_DIST,
    PROBE_POINT, PROBE_PROLOGUE, PROBE_RAYS, PROBE_RAY_BLOCKS, PROBE_RAY_SIZE, PROBE_SCRATCH, PROBE_SWEEP_DIST,
    PROBE_SWEEP_OUT, PROBE_V1, PROBE_V2, PROBE_V3,
};
pub use refresh_pose::{
    refresh, RefreshCalls, BLOCK_STEERING_DAMAGE, CAR_GLOBAL, DEBUG_GLOBAL, DEBUG_ROWS, DEF_DRIVE_AXLE_JOINT,
    DEF_DRIVE_AXLE_MOUNT_OFFSET, DEF_IS_STEERED, DEF_JOINT_AXIS, DEF_JOINT_MOUNT, DEF_STEERING_AXIS,
    DEF_STEERING_MOUNT, DEF_STEERING_ROD_LENGTH, DEF_STEERING_SIGN, DEF_STRUT_DIM_X, DEF_STRUT_DIM_Z,
    DEF_STRUT_HUB_POINT, DEF_STRUT_ROD_MOUNT, DEF_WISHBONE_HEADER, DEF_WISHBONE_LENGTH, DEF_WISHBONE_SIGN,
    DRIVE_AXLE_DIR, NEG_KNUCKLE_Z, ROD_MOUNT, STEERING_ROD, STEERING_ROD_LENGTH, STEERING_ROD_TWEAK, TIP,
    WISHBONE_COPY,
};
pub use step::{wheel_step, StepCalls};
pub use store::WheelStore;
pub use wheel_calls::{
    blend_pair, car_frames, clear_and_forward, forward_to_car_and_clear, pick_contact, pose_flag, set_input,
    spin_cross, split_on_sub, state_lerp, transform_and_copy, turn_axis, update_strut, update_sub_objects, walk_result,
};

//! The game memory of a wheel (`this`) and of its tyre tail, as the wheel's functions read it. The offsets are the only
//! raw ones in the wheel module: the ported functions take the typed values these loaders return.

mod copy_and_emit;
mod ctors;
mod fields;
mod probe;
mod refresh_pose;
mod step;
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
    Frame, AXIS_BASE, AXIS_OUT, AXIS_TURN, BLEND_C_COPY_A40, BLEND_C_COPY_DE0, BLEND_D_COPY_A20, BLEND_D_COPY_E00,
    BLEND_OPS, BLOCK_ACC, BLOCK_B, BLOCK_BLEND, BLOCK_CLAMP_A, BLOCK_CLAMP_B, BLOCK_DIFF, BLOCK_INPUT, BLOCK_LERP_A,
    BLOCK_LERP_B, BLOCK_PHASE, BLOCK_TAG_1, BLOCK_TAG_2, BLOCK_TARGET, BLOCK_TOTAL, BLOCK_WEIGHT, CAR, CAR_FLAGS,
    CAR_FLAG_XFORM, CBASE_NAME, CF_DEF_234, CF_DEF_238, CF_DEF_23C, CF_DEF_414, CF_DEF_418, CF_DEF_41C, CF_DEF_900,
    CF_DEF_BASE, CF_DEF_COS, CF_DEF_F1, CF_DEF_SIN, CF_DEF_SUM, CF_DEF_VEC, CF_DEF_ZERO, CF_SRC_20, CF_SRC_44,
    CF_SRC_48, CF_SRC_4C, CF_SRC_54, CF_SRC_58, CF_SRC_5C, CF_SRC_ANG, CF_SRC_VEC, CLEAR_190, CLEAR_1C0, CONTACT_A70,
    CONTACT_A80, COPY_BLOCK, COPY_OFFSET, CROSS_ADD, DEF, DEF_ANGLE_OFFSET, DEF_ANGLE_SCALE, DEF_FLAGS, DEF_INPUT_MAX,
    DEF_INPUT_MIN, DEF_INVERSE_MASS, DEF_LERP_A, DEF_LERP_A_SCALE, DEF_LERP_B, DEF_LERP_B_SCALE, DEF_LERP_C,
    DEF_MATERIALS, DEF_PAIR_TAG, EMBEDDED_OBJ, FLAG_PAIR_TAG, FORWARD_INT_A10, FORWARD_VEC_9F0, FORWARD_VEC_A00,
    FRAME_AXIS, INPUT_CHANGED, LERP_OUT_A, LERP_OUT_B, MAT_GAIN, MAT_LERP_A, MAT_LERP_B, MAT_PERIOD, NAME_F28,
    NAME_F38, NAME_WEIGHT_28, NAME_WEIGHT_38, ON_GROUND, SCALED_VEC, SPIN_RATE, STATE_ANGLE, STATE_LERP, STRUT,
    STRUT_LENGTH, STRUT_TWEAK, STRUT_VEL, SUB_A, SUB_A_FLAG, SUB_A_SCALE, SUB_B, SUB_B_FLAG, SUB_B_SCALE, TYRE_BLOCK,
    TYRE_SCRATCH, TYRE_TAIL, UNK_C04, UNK_C08, UNK_C0C, WALK_OFFSET, WALK_OUT, WALK_RATIO_DEN, WALK_RATIO_NUM,
    WHEEL_INDEX, WORLD_DEF, XFORM_CAR, XFORM_PAIRS,
};
pub use probe::{
    ground_probe, GroundCalls, ProbeTuning, WheelTuning, PROBE_CLEAR, PROBE_FV, PROBE_GATE, PROBE_HIT, PROBE_HIT_DIST,
    PROBE_POINT, PROBE_PROLOGUE, PROBE_RAYS, PROBE_RAY_BLOCKS, PROBE_RAY_SIZE, PROBE_SCRATCH, PROBE_SWEEP_DIST,
    PROBE_SWEEP_OUT, PROBE_V1, PROBE_V2, PROBE_V3,
};
pub use refresh_pose::{
    refresh, RefreshCalls, BLOCK_8, CAR_GLOBAL, CONTACT, DEBUG_GLOBAL, DEBUG_ROWS, DEF_AXIS, DEF_BITS_244,
    DEF_BITS_248, DEF_BITS_24C, DEF_GAIN_36C, DEF_HAS_B0C, DEF_HEIGHT, DEF_OFFSET_3FC, DEF_OFFSET_8D0, DEF_POINT,
    DEF_RATE_8C4, DEF_RATE_8C8, DEF_SCALE_35C, DEF_SCALE_42C, DEF_TERM_3EC, DEF_V8B0, DEF_V8C0, DEF_WEIGHTS_380,
    DEF_WEIGHTS_3A0, DIRTY_HEIGHT, DIRTY_OBJECT, NEG_THIRD_ROW, OFFSET_E60, POINT_E50, REFRESH_COPY, UNIT_OFFSET,
};
pub use step::{wheel_step, StepCalls, STEP_SLIP};
pub use wheel_calls::{
    blend_pair, car_frames, clear_and_forward, forward_to_car_and_clear, pick_contact, pose_flag, set_input,
    spin_cross, split_on_sub, state_lerp, tag_blend, transform_and_copy, turn_axis, update_strut, update_sub_objects,
    walk_result, AngleIn, AxisPair, InputSet, SetInputIn, StateLerpIn, TurnIn,
};

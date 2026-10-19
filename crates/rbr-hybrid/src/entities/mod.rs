//! Bindings of the entity ports (`rbr_physics::entities`, exe 0x519530-0x522100 and 0x6009b0-0x606090 mostly): the
//! tree and pair walks, the transform fetches, the frame hooks and entity slots, the hinged body's simulation, the
//! `E90Group` build and `CollideInit`. This file holds the addresses and the binding table; the bindings are in the
//! submodules by concern.

use crate::Binding;

mod collide_init;
mod e90;
mod entity_slots;
mod frame;
mod hinge;
mod matrices;
mod walks;

use crate::collision::E01_VEC4_COPY_FROM;
use crate::rigid_body::VF81X;
use collide_init::{collide_init_run, copy_grid_run};
use e90::{
    d3dx_normalize_run, e90_add_bend_run, e90_add_fixed_run, e90_add_particle_run, e90_add_pin_run, e90_add_spring_run,
    e90_bend_pool_init_run, e90_build_post_run, e90_build_run, e90_build_span_run, e90_part_ctor_run,
    e90_part_pool_init_run, e90_pin_pool_init_run, e90_ptr_tab_init_run, e90_seg_init_run, e90_seg_pool_init_run,
};
use entity_slots::{
    clear_acc_run, dummy_present, dummy_vf11c, dummy_vf_cc, e43_vf_d4, entity_vf140, entity_vf80_run, entity_vf9c_run,
    entity_vf_cc, entity_vf_d0, entity_vf_e8, entity_vf_f0, entity_vfa0_run, entity_vfb0_run, exclusion_test_run,
    ground_present, ground_vf_cc, ground_vf_e8, init_bd0_run, limit_run, old_frame_of_run, remove_exclusion_run,
    sample_indexed_run, sample_slot_run, vf118, vf120_run, vf_ac,
};
pub(crate) use entity_slots::{empty_hook_run, exclusion_test_direct, ENTITY_VF120};
use frame::{
    frame_vf04_run, frame_vf08_run, frame_vf0c_run, frame_vf10_run, frame_vf14_run, old_frame_vf04_run,
    old_frame_vf08_run, old_frame_vf0c_run, old_frame_vf10_run,
};
use hinge::{
    hinge_sim_d860, hinge_vf_e070, set_650_run, sim_c960_run, sim_cf10_run, sim_cff0_run, sim_d160_run, sim_da60_run,
    sim_e1d0_run, sim_e4f0_run, sim_ea80_run, sim_ead0_run, sim_efe0_run, sim_f350_run, sim_f950_run, sim_fd00_run,
    state_emit_run, step_big_run, SIM_D160, SIM_D860, VF_E070,
};
use matrices::{compose_b_run, copy16_walk_run, copy_blocks_run, copy_blocks_three_four_run, fetch_x_run, fetch_y_run};
pub(crate) use matrices::{fetch_x_direct, transform_direct};
use walks::{
    functor_run, grid_hooks_hi_run, grid_hooks_lo_run, integrate_run, mb560_run, mul4_run, name_walk_run, node_of_run,
    pair_combine_run, pair_echo_run, pair_five_six_deeper_run, pair_five_six_run, pair_five_six_shallower_run,
    pair_grids_deeper_run, pair_grids_run, pair_grids_shallower_run, pair_one_two_deeper_run, pair_one_two_run,
    pair_one_two_shallower_run, pair_three_four_deeper_run, pair_three_four_run, pair_three_four_shallower_run,
    route_pair_five_six_run, route_pair_grids_run, route_pair_one_two_run, route_pair_three_four_run, vf20, vf48, vf58,
    vf5c, vf64, walk_pre_order_run,
};

pub const VF58: u32 = 0x51a250;
pub const VF5C: u32 = 0x51a290;
pub const VF64: u32 = 0x51a2c0;
pub const WALK_PRE_ORDER: u32 = 0x51a1f0;
pub const INTEGRATE: u32 = 0x51a330;
pub const VF20: u32 = 0x51a3a0;
pub const NAME_WALK: u32 = 0x51a4b0;
pub const MB560: u32 = 0x51b560;
pub const PAIR_COMBINE: u32 = 0x51ab90;
pub const PAIR_COMBINE_TWIN: u32 = 0x51ab70;
pub const PAIR_MAB10: u32 = 0x51ab10;
pub const PAIR_MAB20: u32 = 0x51ab20;
pub const PAIR_MAB30: u32 = 0x51ab30;
pub const PAIR_MAB40: u32 = 0x51ab40;
pub const NODE_OF: u32 = 0x519c00;
pub const FUNCTOR_VF04X: u32 = 0x51a120;
pub const MUL4: u32 = 0x51c8c0;
pub const PAIR_ONE_TWO_DEEPER: u32 = 0x51afe0;
pub const PAIR_ONE_TWO: u32 = 0x51a9d0;
pub const PAIR_THREE_FOUR: u32 = 0x51aa20;
pub const PAIR_ONE_TWO_SHALLOWER: u32 = 0x51b050;
pub const PAIR_THREE_FOUR_DEEPER: u32 = 0x51b0c0;
pub const PAIR_THREE_FOUR_SHALLOWER: u32 = 0x51b130;
pub const PAIR_FIVE_SIX: u32 = 0x51aa70;
pub const PAIR_FIVE_SIX_DEEPER: u32 = 0x51b1a0;
pub const PAIR_FIVE_SIX_SHALLOWER: u32 = 0x51b210;
pub const ROUTE_PAIR_FIVE_SIX: u32 = 0x51b3c0;
pub const ROUTE_PAIR_ONE_TWO: u32 = 0x51b320;
pub const ROUTE_PAIR_THREE_FOUR: u32 = 0x51b370;
pub const FRAME_VF04: u32 = 0x519530;
pub const FRAME_VF08: u32 = 0x519560;
pub const FRAME_VF0C: u32 = 0x519590;
pub const FRAME_VF10: u32 = 0x5195c0;
pub const FRAME_VF14: u32 = 0x5195f0;
pub const OLD_FRAME_VF04: u32 = 0x5196d0;
pub const OLD_FRAME_VF08: u32 = 0x519720;
pub const OLD_FRAME_VF0C: u32 = 0x519770;
pub const OLD_FRAME_VF10: u32 = 0x5197c0;
pub const ENTITY_VF80: u32 = 0x519a20;
pub const ENTITY_VF9C: u32 = 0x51a700;
pub const ENTITY_VFA0: u32 = 0x51a710;
pub const ENTITY_VFB0: u32 = 0x51a750;
pub const EMPTY_HOOK: u32 = 0x519b90;
pub const COPY_BLOCKS: u32 = 0x51b460;
pub const COPY_BLOCKS_THREE_FOUR: u32 = 0x51b500;
pub const COPY16_WALK: u32 = 0x51b4c0;
pub const LIMIT_UPDATE: u32 = 0x51f280;
pub const SAMPLE_SLOT: u32 = 0x520010;
pub const SAMPLE_INDEXED: u32 = 0x520240;
pub const FETCH_X: u32 = 0x51b6c0;
pub const FETCH_Y: u32 = 0x51b780;
pub const COMPOSE_B: u32 = 0x51b600;
const GRID_WALK: u32 = 0x51b410;
pub const SIM_F950: u32 = 0x51f950;
pub const STATE_EMIT: u32 = 0x51f700;
pub const SIM_C960: u32 = 0x51c960;
pub const SIM_CF10: u32 = 0x51cf10;
pub const SIM_CFF0: u32 = 0x51cff0;
pub const SIM_E1D0: u32 = 0x51e1d0;
pub const SIM_EA80: u32 = 0x51ea80;
pub const SIM_EAD0: u32 = 0x51ead0;
pub const SIM_EFE0: u32 = 0x51efe0;
pub const SIM_E4F0: u32 = 0x51e4f0;
pub const SIM_F350: u32 = 0x51f350;
pub const SIM_FD00: u32 = 0x51fd00;
pub const SIM_DA60: u32 = 0x51da60;
pub const CLEAR_ACC: u32 = 0x529e60;
pub const INIT_BD0: u32 = 0x519bd0;
pub const E90_BUILD: u32 = 0x601470;
pub const E90_SEG_POOL_INIT: u32 = 0x601270;
pub const E90_BUILD_SPAN: u32 = 0x6029b0;
pub const E90_BUILD_POST: u32 = 0x602fe0;
pub const REMOVE_EXCLUSION: u32 = 0x51a060;
pub const SET_650: u32 = 0x51da50;
pub(crate) const BODY_COMPOSE: u32 = 0x4e6a30;
const XFORM_8130: u32 = 0x4e8130;
const TO_QUAT: u32 = 0x4e4d10;
pub(crate) const EXCLUSION_TEST: u32 = 0x519f80;

pub(crate) const ENTITY_VF118: u32 = 0x519ac0;

pub static BINDINGS: [Binding; 118] = [
    bind(VF58, "CEntity::E43Vf58", vf58),
    bind(VF5C, "CEntity::E43Vf5C", vf5c),
    bind(ENTITY_VF80, "CEntity::E43Vf80", entity_vf80_run),
    bind(ENTITY_VF118, "CEntity::Vf118", vf118),
    bind(0x519d60, "CEntity::Vf114", vf118),
    bind(0x519970, "CEntity::VfE8", entity_vf_e8),
    bind(0x600b20, "CDummyObject::Vf11C", dummy_vf11c),
    bind(0x6009b0, "CDummyObject::VfCC", dummy_vf_cc),
    bind(0x519e90, "CEntity::VfCC", entity_vf_cc),
    bind(0x519ea0, "CEntity::VfD0", entity_vf_d0),
    bind(0x600c80, "CDummyObject::Vf110", dummy_present),
    bind(0x519ae0, "CEntity::E43VfD4", e43_vf_d4),
    bind(VF64, "CEntity::E43Vf64", vf64),
    bind(WALK_PRE_ORDER, "CEntity::E43_Walk1F0", walk_pre_order_run),
    bind(INTEGRATE, "CEntity::E43Vf6C", integrate_run),
    bind(VF20, "CEntity::E43Vf20", vf20),
    bind(NAME_WALK, "CEntity::Vf40", name_walk_run),
    bind(MB560, "E43PairWalk::MB560", mb560_run),
    bind(PAIR_COMBINE, "E33Mat34::Combine", pair_combine_run),
    bind(PAIR_COMBINE_TWIN, "E33Mat34::CombineTwin", pair_combine_run),
    bind(PAIR_MAB10, "E43PairWalk::MAB10", pair_echo_run),
    bind(PAIR_MAB20, "E43PairWalk::MAB20", pair_echo_run),
    bind(PAIR_MAB30, "E43PairWalk::MAB30", pair_echo_run),
    bind(PAIR_MAB40, "E43PairWalk::MAB40", pair_echo_run),
    bind(NODE_OF, "E43PairWalk::MFc00", node_of_run),
    bind(FUNCTOR_VF04X, "CPlainUnaryFunction::Vf04x", functor_run),
    bind(MUL4, "E44Shell::M51C8C0", mul4_run),
    bind(PAIR_ONE_TWO_DEEPER, "E43PairWalk::WFE0", pair_one_two_deeper_run),
    bind(PAIR_THREE_FOUR_DEEPER, "E43PairWalk::WB0C0", pair_three_four_deeper_run),
    bind(PAIR_THREE_FOUR_SHALLOWER, "E43PairWalk::WB130", pair_three_four_shallower_run),
    bind(ROUTE_PAIR_ONE_TWO, "E43PairWalk::WB320", route_pair_one_two_run),
    bind(ROUTE_PAIR_THREE_FOUR, "E43PairWalk::WB370", route_pair_three_four_run),
    bind(PAIR_ONE_TWO, "E43PairWalk::W9D0", pair_one_two_run),
    bind(PAIR_FIVE_SIX, "E43PairWalk::WA70", pair_five_six_run),
    bind(PAIR_FIVE_SIX_DEEPER, "E43PairWalk::WB1A0", pair_five_six_deeper_run),
    bind(PAIR_FIVE_SIX_SHALLOWER, "E43PairWalk::WB210", pair_five_six_shallower_run),
    bind(ROUTE_PAIR_FIVE_SIX, "E43PairWalk::WB3C0", route_pair_five_six_run),
    bind(PAIR_THREE_FOUR, "E43PairWalk::WA20", pair_three_four_run),
    bind(0x51aac0, "E43PairWalk::WAC0", pair_grids_run),
    bind(0x51b280, "E43PairWalk::WB280", pair_grids_deeper_run),
    bind(0x51b2d0, "E43PairWalk::WB2D0", pair_grids_shallower_run),
    bind(0x51b410, "E43PairWalk::WB410", route_pair_grids_run),
    bind(0x4f5e40, "E37Walk5E40", grid_hooks_lo_run),
    bind(0x4f5eb0, "E37Walk5EB0", grid_hooks_hi_run),
    bind(PAIR_ONE_TWO_SHALLOWER, "E43PairWalk::WB050", pair_one_two_shallower_run),
    bind(FRAME_VF04, "CEntityFrame::Vf04", frame_vf04_run),
    bind(FRAME_VF08, "CEntityFrame::Vf08", frame_vf08_run),
    bind(FRAME_VF0C, "CEntityFrame::Vf0C", frame_vf0c_run),
    bind(FRAME_VF10, "CEntityFrame::Vf10", frame_vf10_run),
    bind(FRAME_VF14, "CEntityFrame::Vf14", frame_vf14_run),
    bind(0x519eb0, "CEntity::VfF0", entity_vf_f0),
    bind(0x519e20, "CEntity::E43Vf140", entity_vf140),
    bind(OLD_FRAME_VF04, "CEntityOldFrame::Vf04", old_frame_vf04_run),
    bind(OLD_FRAME_VF08, "CEntityOldFrame::Vf08", old_frame_vf08_run),
    bind(OLD_FRAME_VF0C, "CEntityOldFrame::Vf0C", old_frame_vf0c_run),
    bind(OLD_FRAME_VF10, "CEntityOldFrame::Vf10", old_frame_vf10_run),
    bind(ENTITY_VF9C, "CEntity::E43Vf9C", entity_vf9c_run),
    bind(ENTITY_VFA0, "CEntity::E43VfA0", entity_vfa0_run),
    bind(ENTITY_VFB0, "CEntity::E43VfB0", entity_vfb0_run),
    bind(EMPTY_HOOK, "CEntity::EmptyHook (ours)", empty_hook_run),
    bind(FETCH_X, "E33Mat34::FetchX", fetch_x_run),
    bind(FETCH_Y, "E33Mat34::FetchY", fetch_y_run),
    bind(COMPOSE_B, "E33Mat34::ComposeB", compose_b_run),
    bind(COPY_BLOCKS, "E43PairWalk::M51B460 (ours)", copy_blocks_run),
    bind(COPY_BLOCKS_THREE_FOUR, "E43PairWalk::M51B500 (ours)", copy_blocks_three_four_run),
    bind(COPY16_WALK, "E33Mat34::M51B4C0 (ours)", copy16_walk_run),
    bind(LIMIT_UPDATE, "CEntity::M51F280 (ours)", limit_run),
    bind(SAMPLE_SLOT, "E44MatTable::M520010 (ours)", sample_slot_run),
    bind(SAMPLE_INDEXED, "E44MatTable::M520240 (ours)", sample_indexed_run),
    bind(SIM_F950, "CHingedRigidBody::M51F950 (ours)", sim_f950_run),
    bind(STATE_EMIT, "CMultiSignal1<CSigMrRBodyState>::Emit (ours)", state_emit_run),
    bind(SIM_C960, "CHingedRigidBody::SimC960", sim_c960_run),
    bind(SIM_CF10, "CHingedRigidBody::SimCF10", sim_cf10_run),
    bind(SIM_CFF0, "CHingedRigidBody::SimCFF0", sim_cff0_run),
    bind(SIM_D160, "CHingedRigidBody::SimD160", sim_d160_run),
    bind(SIM_E1D0, "CHingedRigidBody::SimE1D0", sim_e1d0_run),
    bind(SIM_EA80, "CHingedRigidBody::SimEA80", sim_ea80_run),
    bind(SIM_EAD0, "CHingedRigidBody::SimEAD0", sim_ead0_run),
    bind(SIM_E4F0, "CHingedRigidBody::SimE4F0", sim_e4f0_run),
    // The mounted body's slot 0x138: the same asm as SimE4F0's.
    bind(0x521a50, "CMountedRigidBody::Vf138", sim_e4f0_run),
    bind(SIM_F350, "CHingedRigidBody::SimF350", sim_f350_run),
    bind(SIM_FD00, "CHingedRigidBody::SimFD00", sim_fd00_run),
    bind(SIM_DA60, "CHingedRigidBody::SimDA60", sim_da60_run),
    bind(0x522100, "CMountedRigidBody::StepBig", step_big_run),
    bind(CLEAR_ACC, "CSolid::E43ClearAcc (ours)", clear_acc_run),
    bind(INIT_BD0, "CEntity::E43_InitBD0", init_bd0_run),
    bind(E90_BUILD, "E90Group::Build", e90_build_run),
    bind(E90_SEG_POOL_INIT, "E90SegPool::Init", e90_seg_pool_init_run),
    bind(E90_BUILD_SPAN, "CFenceHandler::BuildSpan", e90_build_span_run),
    bind(E90_BUILD_POST, "CFenceHandler::BuildPost", e90_build_post_run),
    bind(0x605f30, "E90Seg::AddParticle", e90_add_particle_run),
    bind(0x605fb0, "E90Seg::AddSpring", e90_add_spring_run),
    bind(0x606070, "E90Seg::AddFixed", e90_add_fixed_run),
    bind(0x606090, "E90Seg::AddPin", e90_add_pin_run),
    bind(0x6060b0, "E90Seg::AddBend", e90_add_bend_run),
    bind(0x605b60, "E90Seg::Init", e90_seg_init_run),
    bind(0x6026a0, "E90PartPool::Init", e90_part_pool_init_run),
    bind(0x6027a0, "E90PtrTab::Init", e90_ptr_tab_init_run),
    bind(0x6028b0, "E90BendPool::Init", e90_bend_pool_init_run),
    bind(0x602930, "E90PinPool::Init", e90_pin_pool_init_run),
    bind(0x602220, "E90_PartCtor", e90_part_ctor_run),
    bind(0x627235, "_D3DXVec3Normalize@8", d3dx_normalize_run),
    bind(REMOVE_EXCLUSION, "CEntity::E43_ExclA060", remove_exclusion_run),
    bind(SIM_EFE0, "CHingedRigidBody::SimEFE0", sim_efe0_run),
    bind(SET_650, "CHingedRigidBody::Set650", set_650_run),
    bind(0x51a440, "CEntity::E43Vf48", vf48),
    bind(EXCLUSION_TEST, "CEntity::E43_ExclF80", exclusion_test_run),
    bind(0x519c10, "E43PairWalk::MFc10", old_frame_of_run),
    bind(ENTITY_VF120, "CEntity::Vf120", vf120_run),
    bind(0x51bb70, "CGround::Vf110", ground_present),
    bind(0x51bb80, "CGround::Vf114", ground_present),
    bind(0x51bd30, "E33Mat34::CollideInit", collide_init_run),
    bind(0x4f6770, "E37GridOps::Copy6770", copy_grid_run),
    bind(0x51bc70, "CGround::VfCC", ground_vf_cc),
    bind(0x51c6f0, "CGround::VfE8", ground_vf_e8),
    bind(0x51a740, "CEntity::E43VfAC", vf_ac),
    bind(VF_E070, "CHingedRigidBody::VfE070", hinge_vf_e070),
    bind(SIM_D860, "CHingedRigidBody::SimD860", hinge_sim_d860),
];

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "entities", run }
}

#[cfg(test)]
mod tests {
    use super::hinge::sim_e4f0_run;
    use crate::tests::Script;
    use crate::{Ctx, Regs, Return};
    use rbr_physics::mem::Mem;

    /// An unowned hinge returns `E43Vf138`'s eax, the fifth stack word `e` (the asm's `mov eax, [esp+0x20]`, 0x52b9d0),
    /// through both `SimE4F0` and `CSolid::E43Vf138`'s bindings. `dp` is zero, so no dot product is taken.
    #[test]
    fn unowned_e4f0_returns_the_e_pointer() {
        let host = Script::default();
        let mut ctx = Ctx { mem: Mem::new(), host: &host };
        let (this, def) = (0x1000, 0x2000);
        ctx.mem.map(this, &[0; 0x400]);
        ctx.mem.set_u32(this + 0x34, def);
        ctx.mem.map(def, &[0; 0xcac]);
        for at in [0x3000, 0x3010, 0x3020, 0x3030, 0x3040] {
            ctx.mem.map(at, &[0; 0x10]);
        }
        let mut r = Regs { ecx: this, esp: 0x9000, ..Regs::default() };
        r.stack[..6].copy_from_slice(&[0x3000, 0x3010, 0x3020, 0, 0x3030, 0x3040]);
        let want = Ok(Return::void(0x3030, 0x18));
        assert_eq!(sim_e4f0_run(&mut ctx, &r), want);
        assert_eq!(crate::rigid_body::e43vf138(&mut ctx, &r), want);
    }
}

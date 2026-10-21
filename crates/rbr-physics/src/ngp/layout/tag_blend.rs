//! NGP's rewrite of `E51Wheel::M3D230` (`0x1005e8a0`, in `M3D460`'s call at `0x53d49d`): the blend of the tyre block's
//! two material tags, [`crate::ngp::tag_blend`].

use crate::mem::Mem;
use crate::ngp::tag_blend::{blend, Material};
use crate::physics_core::material::tag_entry;
use crate::wheel::layout::{
    BLOCK_BLEND, BLOCK_INPUT, BLOCK_PHASE, BLOCK_TAG_1, BLOCK_TAG_2, BLOCK_TOTAL, BLOCK_WEIGHT, DEF_MATERIALS,
    MAT_GAIN, MAT_LERP_A, MAT_LERP_B, MAT_PERIOD,
};

/// The physics definition (`g_1659b6c`); NGP skips the blend while it is 0.
const CDEF_GLOBAL: u32 = 0x0165_9b6c;
/// The second tag NGP replaces with the first.
const TAG_SAME: u32 = 0xfe;

/// A material's entry: the period (`+0xd8`) and gain (`+0xc8`), and the two lerped values (`+0x18`, `+0xb8`).
fn load_material(mem: &Mem, entry: u32) -> Material {
    Material {
        period: mem.f32(entry + MAT_PERIOD),
        gain: mem.f32(entry + MAT_GAIN),
        blend_value: mem.f32(entry + MAT_LERP_A),
        total_value: mem.f32(entry + MAT_LERP_B),
    }
}

/// `0x1005e8a0` on the block at `blk`: the tag remap, the materials' lookup (the exe's `Tag520940`) and the blend,
/// stored at `+0x54` and `+0x58`. Its eax (the C runtime's leftovers) is not modelled.
pub fn update(mem: &mut Mem, blk: u32) {
    let def = mem.u32(CDEF_GLOBAL);
    if def == 0 {
        return;
    }
    let manager = def + DEF_MATERIALS;
    let tag_1 = mem.u32(blk + BLOCK_TAG_1);
    let mut tag_2 = mem.u32(blk + BLOCK_TAG_2);
    if tag_2 == TAG_SAME {
        tag_2 = tag_1;
        mem.set_u32(blk + BLOCK_TAG_2, tag_2);
    }
    let materials = [tag_1, tag_2].map(|tag| load_material(mem, tag_entry(mem, manager, tag)));
    let (blend, total) =
        blend(materials, mem.f32(blk + BLOCK_WEIGHT), mem.f32(blk + BLOCK_INPUT), mem.f32(blk + BLOCK_PHASE));
    mem.set_f32(blk + BLOCK_BLEND, blend);
    mem.set_f32(blk + BLOCK_TOTAL, total);
}

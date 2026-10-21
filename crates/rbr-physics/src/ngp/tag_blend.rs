//! NGP's rewrite of `E51Wheel::M3D230` (`0x1005e8a0`, in `M3D460`'s call at `0x53d49d`): the blend of the tyre
//! block's two material tags, in SSE. Unlike 1.02's (`wheel::layout::tag_blend`), the sine terms take the same weights
//! as the lerps (1.02 weighs the first material's sines by `w` and the second's by `1 - w`, and its lerps the other way),
//! the sines are the CRT's `sin` in f64 rounded to f32, and a second tag of `0xfe` takes the first's (written back).

use crate::mem::Mem;
use crate::physics_core::layout::material::tag_entry;
use crate::wheel::layout::{
    BLOCK_BLEND, BLOCK_INPUT, BLOCK_PHASE, BLOCK_TAG_1, BLOCK_TAG_2, BLOCK_TOTAL, BLOCK_WEIGHT, DEF_MATERIALS,
    MAT_GAIN, MAT_LERP_A, MAT_LERP_B, MAT_PERIOD,
};

const TWO_PI: f32 = f32::from_bits(0x40C9_0FDB); // 10148ae4

/// The physics definition (`g_1659b6c`); NGP skips the blend while it is 0.
const CDEF_GLOBAL: u32 = 0x0165_9b6c;
/// The second tag NGP replaces with the first.
const TAG_SAME: u32 = 0xfe;

/// What the blend reads of a material's entry: the phase's period (`+0xd8`) and gain (`+0xc8`), and the two lerped
/// values (`+0x18`, `+0xb8`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub period: f32,
    pub gain: f32,
    pub lerp_a: f32,
    pub lerp_b: f32,
}

impl Material {
    pub fn load(mem: &Mem, entry: u32) -> Self {
        Material {
            period: mem.f32(entry + MAT_PERIOD),
            gain: mem.f32(entry + MAT_GAIN),
            lerp_a: mem.f32(entry + MAT_LERP_A),
            lerp_b: mem.f32(entry + MAT_LERP_B),
        }
    }
}

/// `sin(phase_b * r) + sin(phase_a * r)` for the material's ratio `r = 2 pi / period`, each sine rounded to f32.
fn sines(period: f32, phase_a: f32, phase_b: f32) -> f32 {
    let r = TWO_PI / period;
    let s1 = f64::from(r * phase_b).sin() as f32;
    let s2 = f64::from(r * phase_a).sin() as f32;
    s1 + s2
}

/// The blend and the total for the two materials and the weight `w` (the second material's share).
pub fn blend([m1, m2]: [Material; 2], w: f32, phase_a: f32, phase_b: f32) -> (f32, f32) {
    let inv = 1.0 - w;
    let mut acc = 0.0;
    // `!= 0.0` takes a NaN period too, as the asm's unordered compare does
    if m1.period != 0.0 {
        acc = sines(m1.period, phase_a, phase_b) * (inv * m1.gain) + 0.0;
    }
    if m2.period != 0.0 {
        acc += sines(m2.period, phase_a, phase_b) * (w * m2.gain);
    }
    let blend = (inv * m1.lerp_a + w * m2.lerp_a) + acc;
    let total = (inv * m1.lerp_b + w * m2.lerp_b) + blend;
    (blend, total)
}

/// `0x1005e8a0` on the block at `blk`: the tag remap, the materials' lookup (the exe's `Tag520940`) and the blend,
/// stored at `+0x54` and `+0x58`.
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
    let materials = [tag_1, tag_2].map(|tag| Material::load(mem, tag_entry(mem, manager, tag)));
    let (blend, total) =
        blend(materials, mem.f32(blk + BLOCK_WEIGHT), mem.f32(blk + BLOCK_INPUT), mem.f32(blk + BLOCK_PHASE));
    mem.set_f32(blk + BLOCK_BLEND, blend);
    mem.set_f32(blk + BLOCK_TOTAL, total);
}

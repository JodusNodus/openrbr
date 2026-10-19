//! 0x5320c0 `CScratch::ComputeOutputs`: blends the four `CDef` tables at the state's two indices, and writes the
//! results through the ten output pointers (stack words 7 to 16). The original is thiscall with 17 stack words
//! (`ret 0x44`). The stack words are read at their asm offsets: 1, 2, 4 and 5 are scale factors, 6 is the `src`
//! pointer whose two words at `+0x36c` and `+0x370` are copied through.
//!
//! The names are ours: the original has no strings or RTTI for these tables.

use crate::Variant;

/// The two entries a blend reads: `table[a]` and `table[b]`, at the state's indices `m_state+0x9c` and `+0xa0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendPair {
    pub at_a: f32,
    pub at_b: f32,
}

/// `(1 - weight) * at_a + at_b * weight`, in the asm's single-precision order.
fn blend(pair: BlendPair, weight: f32) -> f32 {
    (1.0 - weight) * pair.at_a + pair.at_b * weight
}

/// The inputs of `ComputeOutputs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComputeInput {
    /// `m_state+0xa4`: the blend weight.
    pub weight: f32,
    /// The tables at `m_def+0x244`, `+0x264`, `+0x284` and `+0x2a4`.
    pub table_244: BlendPair,
    pub table_264: BlendPair,
    pub table_284: BlendPair,
    pub table_2a4: BlendPair,
    /// Stack words 1, 2, 4 and 5: the scale factors.
    pub scale_1: f32,
    pub scale_2: f32,
    pub scale_4: f32,
    pub scale_5: f32,
    /// The two dwords at `src+0x36c` and `src+0x370`. They are copied as they are: their type is not known.
    pub raw: [u32; 2],
}

/// What `ComputeOutputs` writes: `raw` to stack words 7 and 8, and the floats to stack words 9 to 16.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComputeOutputs {
    pub raw: [u32; 2],
    pub out9: f32,
    pub out10: f32,
    pub out11: f32,
    pub out12: f32,
    pub out13: f32,
    pub out14: f32,
    pub out15: f32,
    pub out16: f32,
}

/// `scale * x` clamped to 1 from above (`comiss` then `ja`: a NaN keeps the product).
fn scaled_clamped(x: f32, scale: f32) -> f32 {
    let v = x * scale;
    if v > 1.0 {
        1.0
    } else {
        v
    }
}

/// 0x5320c0 `CScratch::ComputeOutputs`. The scale of outputs 14 and 16 is 3 (`0x72b2a0`); NGP repoints it to its 1
/// (`1014fbc4`, the operand at `0x5321d8`).
pub fn compute_outputs(x: &ComputeInput, variant: Variant) -> ComputeOutputs {
    let scale = match variant {
        Variant::Stock102 => 3.0,
        Variant::Ngp778 => 1.0,
    };
    let w = x.weight;
    let at_244 = blend(x.table_244, w);
    let at_264 = blend(x.table_264, w);
    let at_284 = blend(x.table_284, w);
    let at_2a4 = blend(x.table_2a4, w);
    let scaled = at_244 * x.scale_5;
    ComputeOutputs {
        raw: x.raw,
        out9: at_244 * x.scale_4,
        out10: scaled * x.scale_1,
        out11: at_264 * x.scale_4,
        out12: scaled * x.scale_2,
        out13: at_284,
        out14: scaled_clamped(at_284, scale),
        out15: at_2a4,
        out16: scaled_clamped(at_2a4, scale),
    }
}

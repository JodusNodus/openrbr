//! The tyre step's footprint as typed values: the scratch, state and setup words, and the `Frame` that holds them.

use crate::math::vec::Vec4;

use super::frame::Table;
use crate::tyre::material::Material;

/// A table search result (`E46_IdxWrite`, see `locate.rs`): the two indices and the fraction, stored as three words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub lo: u32,
    pub hi: u32,
    pub frac: f32,
}

/// A 12-byte entry of the state's table (`m_state+0xd0`): the value and two damper words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableEntry {
    pub value: f32,
    pub a: f32,
    pub b: f32,
}

/// The scratch words (`CScratch`, `this`) the tyre step reads and writes. Each field is one address (the doc says
/// which); the overlapping words are one field. The state, the setup and the arguments are separate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scratch {
    /// `+0x160`: the `k` vector Step copies in; FillOutputs reads it back.
    pub k_vec: Vec4,
    /// `+0x170`, `+0x180`, `+0x190`: the contact frame rows `c`, `n` and `a` (Align). The slip matrix is their x, y, z.
    pub contact_frame: [Vec4; 3],
    /// `+0x1a0`: the `CTyreMatData` words, in `material::WORDS` order. `+0x238` is word 17 (`SCALE_238`).
    pub material: Material,
    /// `+0x1e4`: the dirty word `SetDirty(+0x1dc)` writes.
    pub dirty_1e4: u32,
    /// `+0x1e8`, `+0x1f4`, `+0x1f8`, `+0x208`, `+0x218`: `ContactPatch` and `SolvePatch` outputs (unknown meaning).
    pub f_1e8: f32,
    pub flag_1f4: u32,
    pub factor_1f8: f32,
    pub f_208: f32,
    pub f_218: f32,
    /// `+0x228`: `G`, the gate of the solve path.
    pub g_228: f32,
    /// `+0x258`: `H`, the contact offset (`f96`).
    pub f96: f32,
    /// `+0x288`: the heat source (`F[0xA2]`), the wear gate.
    pub heat_source: f32,
    /// `+0x298`: a tyre factor (`Sign`).
    pub factor_298: f32,
    /// `+0x300`, `+0x304`, `+0x308`, `+0x30c`, `+0x310`: the `ContactPatch` ratios and factors.
    pub scale_300: f32,
    pub r304: f32,
    pub f308: f32,
    pub tyre_factor: f32,
    pub f310: f32,
    /// `+0x314`, `+0x318`, `+0x31c`: the contact outputs and the `qb` words of the solve (`tyre_factor_2` is `+0x314`).
    pub tyre_factor_2: f32,
    pub f318: f32,
    pub f31c: f32,
    /// `+0x324`, `+0x328`: the weight ratios.
    pub ratio_324: f32,
    pub ratio_328: f32,
    /// `+0x32c` to `+0x344`: the seven `p` words of the solve (`+0x340` is `blend_hi`, `+0x344` `blend_lo`).
    pub contact_p: [f32; 7],
    /// `+0x348`, `+0x34c`, `+0x350`, `+0x354`, `+0x358`, `+0x35c`: the force sums of `Accumulate`.
    pub force_sum: f32,
    pub nh: f32,
    pub zero_350: f32,
    pub force_t1: f32,
    pub force_b: f32,
    pub force_a: f32,
    /// `+0x360` (`F[0xD8]`): the slip speed. `+0x364`: `v0² + v1²`. `+0x368` to `+0x370`: the slip velocity.
    pub speed: f32,
    pub speed_sq: f32,
    pub slip_v: [f32; 3],
    /// `+0x374`: `R·ω + v1`. `+0x378`: the spin (float bits). `+0x37c`: the slip-ratio term.
    pub long_contact_vel: f32,
    pub spin: f32,
    pub slip_ratio_term: f32,
    /// `+0x380`, `+0x384`: zeroed by `SplitSlip` above the threshold.
    pub f380: f32,
    pub f384: f32,
    /// `+0x388`: `F[0xE2]`, the contact weight.
    pub f_e2: f32,
    /// `+0x38c`, `+0x390`: the heat-flow terms.
    pub heat_flow_e3: f32,
    pub heat_flow_e4: f32,
    /// `+0x394`, `+0x398`: `def+0x4c` and `def+0x5c` times the blend (`+0x398` is then scaled by the speed weight in Step).
    pub f394: f32,
    pub f398: f32,
    /// `+0x39c`, `+0x3a0`, `+0x3a4`: the `SolveForces` outputs (`a3`, `a6`, `a5`).
    pub f39c: f32,
    pub f3a0: f32,
    pub f3a4: f32,
    /// `+0x3a8`: `c` (float bits). `+0x3ac`: the dot product of the align. `+0x3b0`: `scale_3b0`.
    pub arg_c: f32,
    pub f3ac: f32,
    pub scale_3b0: f32,
    /// `+0x3b4`: the blend of the table entries. `+0x3b8`: the `StepInner` result (`arg3`).
    pub blend: f32,
    pub f3b8: f32,
    /// `+0x3bc`: `p3bc`, `(1 - table) · d8c`.
    pub p3bc: f32,
    /// `+0x3c0`: the tyre index, `trunc(angle) & 7`.
    pub tyre_index: u32,
    /// `+0x3c4`: the heat temperature.
    pub heat_t: f32,
    /// `+0x3c8`: `F[0xF2]`, the sign term.
    pub heat_f2: f32,
    /// `+0x3cc`, `+0x3d0`, `+0x3d4`: zeroed at the start of the force terms.
    pub f3cc: f32,
    pub f3d0: f32,
    pub f3d4: f32,
    /// `+0x3d8`: `d` (float bits). `+0x3dc`: the first weight of `SplitSlip`. `+0x3e0`, `+0x3e4`: `StepInner` arg4 and arg5.
    pub arg_d: f32,
    pub f3dc: f32,
    pub f3e0: f32,
    pub f3e4: f32,
    /// `+0x3e8`, `+0x3ec`: the two `SolvePatch` clamps. `+0x3f0`: the second weight (the gate of `SolvePair`).
    pub f3e8: f32,
    pub f3ec: f32,
    pub gate: f32,
    /// `+0x3f4`, `+0x3f8`: the `ClampGrip` flags. `+0x3fc`, `+0x400`: the grip outputs.
    pub f3f4: f32,
    pub f3f8: f32,
    pub f3fc: f32,
    pub f400: f32,
    /// `+0x404`: the heat flow (`HeatStep`).
    pub flow: f32,
    /// `*(+0x410) + 0x5ac`: the contact angle (read only; the record is not part of the frame).
    pub contact_angle: f32,
    /// `*(+0x414)[0..8]`: the wear table, one word per tyre index.
    pub wear: [f32; 8],
    /// `+0x420` to `+0x47c`: the signal block (`m_sig`): 24 words, read and written as dwords.
    pub signal: [u32; 24],
    /// `+0x480`: the contact flag. `+0x484`: the tyre index copy.
    pub contact_flag: u32,
    pub tyre_index_copy: u32,
    /// `+0x490`, `+0x4a0`: the copies `WriteOutputs` makes. `+0x4b0`, `+0x4b4`, `+0x4b8`: the copied dwords.
    pub copy_490: Vec4,
    pub copy_4a0: Vec4,
    pub copy_4b0: u32,
    pub copy_4b4: u32,
    pub copy_4b8: u32,
    /// `+0x4bc`: `e3`. `+0x4c0`, `+0x4c4`: the `SolvePatch` x and y.
    pub e3: f32,
    pub x4c0: f32,
    pub y4c4: f32,
    /// `+0x4c8` to `+0x4d0`: the `f` pair, copied when the game flag is set.
    pub pair: [u32; 3],
    /// `+0x4e0`, `+0x4e4`, `+0x4ec`: the `HeatStep` outputs `r`, `sum_ab` and `t_inv`.
    pub heat_r: f32,
    pub heat_sum_ab: f32,
    pub heat_t_inv: f32,
    /// `+0x4e8`: NGP's heat term (`ngp::inclination`), unused by 1.02.
    pub ngp_4e8: f32,
    /// `+0x504`, `+0x508`, `+0x50c`: the three `Accumulate` accumulators (decayed by 0.9).
    pub acc_504: f32,
    pub acc_508: f32,
    pub acc_50c: f32,
}

/// The state words the tyre step reads and writes (`m_state`, at `this+0x14`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateWords {
    /// `+0x0c`: the flag `Step` sets to 1 at its end (`sti[3]`); a zero leaves `Friction` at once.
    pub flag_0c: u32,
    /// `+0x28`: the dirty word `SetDirty(+0x20)` writes.
    pub dirty_28: u32,
    /// `+0x2c`: the first-call flag of `Step`.
    pub flag_2c: u32,
    /// `+0x30` to `+0x3c`: the vector `NoContact` copies `k` to.
    pub vec_30: Vec4,
    /// `+0x40`, `+0x44`: the grip factors `k40` and `k44` (raw bits in `ClampGrip`). `+0x48`: the tread (`SetState48`).
    pub k40: f32,
    pub k44: f32,
    pub tread_48: f32,
    /// `+0x4c`: the effective radius `R`. `+0x50`, `+0x54`: the lateral and torsional state values.
    pub radius: f32,
    pub s50: f32,
    pub s54: f32,
    /// `+0x58`: the lerp weight of `Accumulate`. `+0x5c`: the grip flag (`ClampGrip`).
    pub lerp_weight: f32,
    pub grip_flag: u32,
    /// `+0x68`: the lerp input of the first table search.
    pub s68: f32,
    /// `+0x70`, `+0x74`: the contact state flags.
    pub contact_70: u32,
    pub contact_74: u32,
    /// `+0x80` to `+0x8c`: the friction vector `sv`.
    pub sv: Vec4,
    /// `+0x90` (the blend search), `+0x9c` (the heat search), `+0xa8` (the shape search, table `+0x184`), `+0xb4`
    /// (the second shape search) and `+0xc0` (the mix search, table `+0x154`). Each is `lo`, `hi` and `frac`.
    pub seg_blend: Segment,
    pub seg_heat: Segment,
    pub seg_shape: Segment,
    pub seg_shape2: Segment,
    pub seg_mix: Segment,
    /// `+0xd0`: the eight tyre-index entries (12 bytes each). The last entry's `b` is `+0x12c`.
    pub table: [TableEntry; 8],
    /// `+0x130`: the second damper state.
    pub state_130: f32,
    /// `+0x134`: the tyre index `ContactPatch` writes.
    pub tyre_index: u32,
}

/// The setup words (`m_def`, `this+0x10`), read only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    /// `+0x1c`: the signal's owner id (`m_debugId`).
    pub debug_id: u32,
    /// `+0x2c`, `+0x3c`, `+0x4c`, `+0x5c`: the factors `z5`, `def_3c`, the blend factor `+0x4c` and `+0x5c`.
    pub f2c: f32,
    pub f3c: f32,
    pub f4c: f32,
    pub f5c: f32,
    /// `+0x7c`, `+0x8c`, `+0x9c`, `+0xac`, `+0xbc`, `+0xcc`, `+0xec`: the scalars of the contact and output terms.
    pub f7c: f32,
    pub f8c: f32,
    pub f9c: f32,
    pub fac: f32,
    pub fbc: f32,
    pub fcc: f32,
    pub fec: f32,
    /// `+0x310`: the wear factor of `e3`.
    pub f310: f32,
    /// `+0x10c`: NGP's scale of the inclination heat factor (`ngp::inclination::heat_scale`).
    pub f10c: f32,
    /// `+0x110`, `+0x11c`, `+0x128`: the three `DamperAccumA` blocks. `+0x118`: the time constant.
    pub damper_110: [f32; 2],
    pub damper_11c: [f32; 2],
    pub damper_128: [f32; 2],
    pub def_118: f32,
    /// The tables: `+0x134` (4 entries, the blend search), `+0x144` (4, the blend values), `+0x154` (6, the mix search),
    /// `+0x16c` (6, the mix factors), `+0x184` (8, the shape search), `+0x1a4` (8, the `e3` values), `+0x1c4` (8, the
    /// shape factors), `+0x1e4` (8, the heat search), `+0x204` and `+0x224` (8 each, the heat lerps), `+0x244`,
    /// `+0x264`, `+0x284` and `+0x2a4` (8 each, the blend pairs).
    pub t134: Table,
    pub t144: Table,
    pub t154: Table,
    pub t16c: Table,
    pub t184: Table,
    pub t1a4: Table,
    pub t1c4: Table,
    pub t1e4: Table,
    pub t204: Table,
    pub t224: Table,
    pub t244: Table,
    pub t264: Table,
    pub t284: Table,
    pub t2a4: Table,
}

/// The whole tyre step footprint: loaded once by `load_frame`, written back once by `store_frame`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub scratch: Scratch,
    pub state: StateWords,
    pub setup: Setup,
}

/// The index of `this+0x238` in the material block (`material::WORDS`): the `scale_238` of `Accumulate`.
pub const SCALE_238: usize = 17;

/// The words of `Scratch::signal` (the `+0x420` block), named for what `FillOutputs` writes there. Word `k` is `+0x420 +
/// 4k`. The rest are copied by `WriteOutputs` or read by the signal's owner.
pub mod sig {
    /// `+0x420`: the owner's debug id (raw). `+0x424`: the running maximum. `+0x428`: the force sum times 1000.
    pub const DEBUG_ID: usize = 0;
    pub const MAX: usize = 1;
    pub const FORCE_SUM: usize = 2;
    /// `+0x42c`: `|the slip difference|`. `+0x430`: the scaled maximum. `+0x434`: the `tyre_factor_2` bits (raw).
    pub const SLIP_DIFF: usize = 3;
    pub const SCALED_MAX: usize = 4;
    pub const FACTOR: usize = 5;
    /// `+0x438`: the constant 320. `+0x43c`: the spin bits (raw). `+0x440`: `|slip ratio term|`. `+0x444`: the speed bits.
    pub const CONST_320: usize = 6;
    pub const SPIN: usize = 7;
    pub const SLIP_RATIO: usize = 8;
    pub const SPEED: usize = 9;
    /// `+0x450`: the `k` vector (raw). `+0x460`: the vector Step passes in (raw).
    pub const K_VEC: usize = 12;
    pub const SRC_VEC: usize = 16;
    /// `+0x470..+0x478`: the `pair` dwords (raw). `+0x47c`: the flag, 1 once `FillOutputs` has run.
    pub const PAIR: usize = 20;
    pub const FLAG: usize = 23;
}

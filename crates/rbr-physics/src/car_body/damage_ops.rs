//! Pure descriptions of the two fixed damage tables and the level fractions used by E36DamageOps.
//! Memory access and the calls made while building these tables live in `layout.rs`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctorAppend {
    /// Entry-array offset in the E36 object; its count is at `table + 0x80`.
    pub table: u32,
    pub function: u32,
    pub value: u32,
    pub scale: u32,
    pub index: u32,
}

/// The appends the original inlines in `E36DamageOps::InitTables` (0x4ef910).
pub const INLINE_APPENDS: &[FunctorAppend] = &[
    FunctorAppend { table: 0x0aa8, function: 0x4eca00, value: 0x453b8000, scale: 0x391d4952, index: 0 },
    FunctorAppend { table: 0x0888, function: 0x4eca00, value: 0x453b8000, scale: 0x391d4952, index: 1 },
    FunctorAppend { table: 0x0c40, function: 0x4eca00, value: 0x453b8000, scale: 0x391d4952, index: 2 },
    FunctorAppend { table: 0x0a20, function: 0x4eca00, value: 0x453b8000, scale: 0x391d4952, index: 3 },
    FunctorAppend { table: 0x0dd8, function: 0x4ecf20, value: 0x44fa0000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0e60, function: 0x4ecf20, value: 0x44fa0000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x0ee8, function: 0x4ecf20, value: 0x44fa0000, scale: 0x3f800000, index: 2 },
    FunctorAppend { table: 0x0f70, function: 0x4ecf20, value: 0x44fa0000, scale: 0x3f800000, index: 3 },
    FunctorAppend { table: 0x0dd8, function: 0x4ec9a0, value: 0x453b8000, scale: 0x3a03126f, index: 0 },
    FunctorAppend { table: 0x0e60, function: 0x4ec9a0, value: 0x453b8000, scale: 0x3a03126f, index: 1 },
    FunctorAppend { table: 0x0ee8, function: 0x4ec9a0, value: 0x453b8000, scale: 0x3a03126f, index: 2 },
    FunctorAppend { table: 0x0f70, function: 0x4ec9a0, value: 0x453b8000, scale: 0x3a03126f, index: 3 },
    FunctorAppend { table: 0x1108, function: 0x4ec7b0, value: 0x44af0000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x1190, function: 0x4ec7b0, value: 0x44af0000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x1218, function: 0x4ec7b0, value: 0x44af0000, scale: 0x3f800000, index: 2 },
    FunctorAppend { table: 0x0338, function: 0x4ec660, value: 0x44fa0000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x03c0, function: 0x4ec660, value: 0x44bb8000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x0448, function: 0x4ec660, value: 0x44bb8000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x04d0, function: 0x4ec660, value: 0x44bb8000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x0558, function: 0x4ec660, value: 0x44fa0000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x1ba8, function: 0x4ed020, value: 0x44bb8000, scale: 0x38d1b717, index: 0 },
    FunctorAppend { table: 0x04d0, function: 0x4ed020, value: 0x44fa0000, scale: 0x38d1b717, index: 0 },
    FunctorAppend { table: 0x1b20, function: 0x4ed020, value: 0x44bb8000, scale: 0x38d1b717, index: 0 },
    FunctorAppend { table: 0x0448, function: 0x4ed020, value: 0x44fa0000, scale: 0x38d1b717, index: 0 },
    FunctorAppend { table: 0x1c30, function: 0x4ed020, value: 0x44bb8000, scale: 0x38d1b717, index: 0 },
    FunctorAppend { table: 0x1108, function: 0x4ed020, value: 0x447a0000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x1190, function: 0x4ed020, value: 0x447a0000, scale: 0x3951b717, index: 0 },
    FunctorAppend { table: 0x04d0, function: 0x4ef5f0, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x1108, function: 0x4ef5f0, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0448, function: 0x4ec820, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x1108, function: 0x4eceb0, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x1a98, function: 0x4eceb0, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0558, function: 0x4ed300, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0338, function: 0x4ec930, value: 0x453b8000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0558, function: 0x4ec890, value: 0x459c4000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0b30, function: 0x4ecaa0, value: 0x45fa0000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0bb8, function: 0x4ecb20, value: 0x45fa0000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0910, function: 0x4ecaa0, value: 0x45fa0000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x0998, function: 0x4ecb20, value: 0x45fa0000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x16e0, function: 0x4ecaa0, value: 0x461c4000, scale: 0x3f800000, index: 2 },
    FunctorAppend { table: 0x1658, function: 0x4ecb20, value: 0x461c4000, scale: 0x3f800000, index: 2 },
    FunctorAppend { table: 0x05e0, function: 0x4ecc30, value: 0x42480000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x02b0, function: 0x4ecc30, value: 0x42480000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x05e0, function: 0x4ecbb0, value: 0x42480000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x02b0, function: 0x4ecbb0, value: 0x42480000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x0668, function: 0x4ecd30, value: 0x42480000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x06f0, function: 0x4ecd30, value: 0x42480000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x0668, function: 0x4ecdb0, value: 0x42480000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x06f0, function: 0x4ecdb0, value: 0x42480000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x0668, function: 0x4eccb0, value: 0x42480000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x06f0, function: 0x4eccb0, value: 0x42480000, scale: 0x3f800000, index: 1 },
    FunctorAppend { table: 0x0118, function: 0x4ed200, value: 0x457a0000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x01a0, function: 0x4ed200, value: 0x457a0000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x02b0, function: 0x4ed200, value: 0x451c4000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x05e0, function: 0x4ed200, value: 0x451c4000, scale: 0x3f800000, index: 0 },
    FunctorAppend { table: 0x0668, function: 0x4ed200, value: 0x44fa0000, scale: 0x3f800000, index: 1 },
];

/// Appends emitted as real calls to `E36DamageOps::AddFunctor` in the tail of 0x4ef910.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddFunctorCall {
    pub table: u32,
    pub index: u32,
    pub scale: u32,
    pub value: u32,
    pub tag: u32,
}

pub const BOUND_APPENDS: &[AddFunctorCall] = &[
    AddFunctorCall { table: 0x1328, index: 5, scale: 0x3f800000, value: 0x42c80000, tag: 0 },
    AddFunctorCall { table: 0x13b0, index: 5, scale: 0x3f800000, value: 0x42c80000, tag: 1 },
    AddFunctorCall { table: 0x1438, index: 5, scale: 0x3f800000, value: 0x42c80000, tag: 2 },
    AddFunctorCall { table: 0x14c0, index: 5, scale: 0x3f800000, value: 0x42c80000, tag: 3 },
    AddFunctorCall { table: 0x1768, index: 6, scale: 0x3f800000, value: 0x41200000, tag: 0 },
    AddFunctorCall { table: 0x17f0, index: 6, scale: 0x3f800000, value: 0x41200000, tag: 1 },
    AddFunctorCall { table: 0x1878, index: 6, scale: 0x3f800000, value: 0x41200000, tag: 2 },
    AddFunctorCall { table: 0x1900, index: 6, scale: 0x3f800000, value: 0x41200000, tag: 3 },
    AddFunctorCall { table: 0x1988, index: 6, scale: 0x3f800000, value: 0x41200000, tag: 4 },
    AddFunctorCall { table: 0x0800, index: 27, scale: 0x3f800000, value: 0x44fa0000, tag: 1 },
    AddFunctorCall { table: 0x15d0, index: 27, scale: 0x3f800000, value: 0x44480000, tag: 2 },
    AddFunctorCall { table: 0x06f0, index: 27, scale: 0x3f800000, value: 0x44480000, tag: 2 },
    AddFunctorCall { table: 0x0778, index: 27, scale: 0x3f800000, value: 0x44480000, tag: 2 },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HandlerFill {
    pub table: u32,
    pub id: u32,
    pub value: u32,
}

/// Fixed fills performed by `CCarDamageHandler::InitTables` (0x4ed370), called by the E36 constructor. The four
/// entries at `(0x2b30, 0x1a)`, `(0x2b88, 0x1b)`, `(0x2ad8, 0x1d)` and `(0x2a80, 0x1c)` use 0.1f from the original's
/// 0x72b068 constant; the reconstructed C++ source labels that constant 0.7f.
pub const HANDLER_FILLS: &[HandlerFill] = &[
    HandlerFill { table: 0x2488, id: 0x0c, value: 0x3f333333 },
    HandlerFill { table: 0x2488, id: 0x09, value: 0x3f333333 },
    HandlerFill { table: 0x2488, id: 0x08, value: 0x3f333333 },
    HandlerFill { table: 0x2488, id: 0x0a, value: 0x3f000000 },
    HandlerFill { table: 0x24e0, id: 0x0e, value: 0x3f333333 },
    HandlerFill { table: 0x24e0, id: 0x0a, value: 0x3f333333 },
    HandlerFill { table: 0x24e0, id: 0x07, value: 0x3ecccccd },
    HandlerFill { table: 0x24e0, id: 0x01, value: 0x3ecccccd },
    HandlerFill { table: 0x2278, id: 0x0d, value: 0x3f333333 },
    HandlerFill { table: 0x2278, id: 0x0b, value: 0x3f333333 },
    HandlerFill { table: 0x2278, id: 0x08, value: 0x3f333333 },
    HandlerFill { table: 0x2278, id: 0x0a, value: 0x3f000000 },
    HandlerFill { table: 0x25e8, id: 0x01, value: 0x3f333333 },
    HandlerFill { table: 0x25e8, id: 0x0a, value: 0x3f000000 },
    HandlerFill { table: 0x25e8, id: 0x1b, value: 0x3f000000 },
    HandlerFill { table: 0x25e8, id: 0x08, value: 0x3ecccccd },
    HandlerFill { table: 0x2590, id: 0x08, value: 0x3f333333 },
    HandlerFill { table: 0x2590, id: 0x09, value: 0x3e99999a },
    HandlerFill { table: 0x2590, id: 0x0b, value: 0x3e99999a },
    HandlerFill { table: 0x2538, id: 0x07, value: 0x3f333333 },
    HandlerFill { table: 0x2538, id: 0x0a, value: 0x3f000000 },
    HandlerFill { table: 0x2538, id: 0x1a, value: 0x3f000000 },
    HandlerFill { table: 0x2538, id: 0x08, value: 0x3ecccccd },
    HandlerFill { table: 0x2298, id: 0x05, value: 0x3e99999a },
    HandlerFill { table: 0x2298, id: 0x06, value: 0x3e99999a },
    HandlerFill { table: 0x2298, id: 0x36, value: 0x3e99999a },
    HandlerFill { table: 0x2608, id: 0x05, value: 0x3e99999a },
    HandlerFill { table: 0x2608, id: 0x06, value: 0x3e99999a },
    HandlerFill { table: 0x2608, id: 0x36, value: 0x3e99999a },
    HandlerFill { table: 0x24a8, id: 0x0b, value: 0x3e99999a },
    HandlerFill { table: 0x24a8, id: 0x0a, value: 0x3e99999a },
    HandlerFill { table: 0x24a8, id: 0x32, value: 0x3e99999a },
    HandlerFill { table: 0x2558, id: 0x0b, value: 0x3e99999a },
    HandlerFill { table: 0x2558, id: 0x0a, value: 0x3e99999a },
    HandlerFill { table: 0x2558, id: 0x32, value: 0x3e99999a },
    HandlerFill { table: 0x2500, id: 0x07, value: 0x3e99999a },
    HandlerFill { table: 0x2500, id: 0x08, value: 0x3e99999a },
    HandlerFill { table: 0x2500, id: 0x09, value: 0x3e99999a },
    HandlerFill { table: 0x25b0, id: 0x34, value: 0x3e99999a },
    HandlerFill { table: 0x25b0, id: 0x36, value: 0x3e99999a },
    HandlerFill { table: 0x25b0, id: 0x20, value: 0x3e99999a },
    HandlerFill { table: 0x25b0, id: 0x21, value: 0x3e99999a },
    HandlerFill { table: 0x2b30, id: 0x14, value: 0x3e99999a },
    HandlerFill { table: 0x2b30, id: 0x1a, value: 0x3dcccccd },
    HandlerFill { table: 0x2b30, id: 0x18, value: 0x3e99999a },
    HandlerFill { table: 0x2b88, id: 0x10, value: 0x3e99999a },
    HandlerFill { table: 0x2b88, id: 0x1b, value: 0x3dcccccd },
    HandlerFill { table: 0x2b88, id: 0x19, value: 0x3e99999a },
    HandlerFill { table: 0x2ad8, id: 0x1f, value: 0x3ecccccd },
    HandlerFill { table: 0x2ad8, id: 0x1d, value: 0x3dcccccd },
    HandlerFill { table: 0x2ad8, id: 0x13, value: 0x3ecccccd },
    HandlerFill { table: 0x2a80, id: 0x1e, value: 0x3ecccccd },
    HandlerFill { table: 0x2a80, id: 0x1c, value: 0x3dcccccd },
    HandlerFill { table: 0x2a80, id: 0x13, value: 0x3ecccccd },
    HandlerFill { table: 0x2a28, id: 0x22, value: 0x3ecccccd },
    HandlerFill { table: 0x2978, id: 0x22, value: 0x3f19999a },
    HandlerFill { table: 0x29d0, id: 0x22, value: 0x3ecccccd },
];

pub const HANDLER_ONES: &[u32] =
    &[0x2470, 0x24c8, 0x2260, 0x25d0, 0x2578, 0x2520, 0x2b50, 0x2af8, 0x2628, 0x26d8, 0x2680];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelFractions {
    pub wheel: f32,
    pub body: f32,
    pub scale: f32,
    pub level_three: u32,
}

/// The scalar branch in `E36DamageOps::Slot4` (0x4f1000).
pub fn level_fractions(level: u32) -> LevelFractions {
    match level {
        1 => LevelFractions { wheel: 0.5, body: 0.5, scale: 0.2, level_three: 0 },
        2 => LevelFractions { wheel: 0.8, body: 0.95, scale: 0.6, level_three: 0 },
        3 => LevelFractions { wheel: 1.0, body: 1.0, scale: 1.0, level_three: 1 },
        _ => LevelFractions { wheel: 0.0, body: 0.0, scale: 0.0, level_three: 0 },
    }
}

/// `Slot4`'s ordered `comiss` clamps preserve NaNs and signed zero.
pub fn clamp_damage(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

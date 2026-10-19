//! `CEngine`'s objects: its definition, state, option block and tuning words.

use super::drive_objects::Lane;
use crate::engine_driveline::store::EngineStore;
use crate::ngp::atmosphere::Atmosphere;
use crate::Variant;

/// `CEngine`'s object pointers (`notes/physics/engine-driveline.md` 2).
pub(super) const ENGINE_DEF_PTR: u32 = 0x04;
pub(super) const ENGINE_STATE_PTR: u32 = 0x08;
pub(super) const ENGINE_OPTIONS_PTR: u32 = 0x48;

/// The three objects a `CEngine` points at: the definition (`this+4`), the state (`this+8`) and the option block
/// (`this+0x48`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineObjects {
    pub def: u32,
    pub state: u32,
    pub options: u32,
}

impl EngineObjects {
    pub fn at<S: EngineStore>(mem: &S, engine: u32) -> Self {
        mem.engine_objects(engine)
    }
}

/// The engine stage's air and tuning words, which NGP repoints at its `.data` (`notes/ngp.md`, "Engine"): the air
/// at the car's height (`ngp::atmosphere`, updated before every engine step) and per-car words it rescales at load.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineTuning {
    pub variant: Variant,
    pub air: Atmosphere,
    /// `Sim7D30`'s second load band around the curve value, below and above: 10000 (`0x72f420`) and 20000
    /// (`0x73b824`); NGP's `10171ac4` and `10171ac8`.
    pub band: [f32; 2],
    /// `SimE470`'s rev gate: the floor rpm 500 (`0x731cd4`), the start rpm 3500 (`0x73b700`) and the scale 1/4500
    /// (`0x731cfc`); NGP's `10171aa8`, `10171aac` and `10171ab0`.
    pub rev_gate: [f32; 3],
    /// `Sim80D0`'s misfire speed gate, 80 (`0x73a1b0`); NGP's `10171ab8`.
    pub misfire_speed: f32,
    /// `Sim80D0`'s misfire heat, 2215000 (`0x73cab4`); NGP's `10171ac0`.
    pub misfire_heat: f32,
    /// `SimECF0`'s stall guard speed, 40 rad/s (`0x73089c`); NGP's `10171abc`.
    pub stall_speed: f32,
}

impl EngineTuning {
    /// 1.02's constants. Its air words: 293.14 (`0x73b5c8`), 101300 (`0x73b5cc` and the pushed `0x47c5da00`),
    /// 1.2040719 (`0x73cac0`), 1/101300 (`0x73cab0`), 116495 (`0x73b5d0`) and 100000 (`0x73c9f4`).
    pub const STOCK: Self = EngineTuning {
        variant: Variant::Stock102,
        air: Atmosphere {
            temperature: 293.140_01,
            pressure: 101_300.0,
            density: f32::from_bits(0x3f9a_1f07),
            inv_pressure: 9.871_668_2e-6,
            pressure_115: 116_495.0,
            pressure_0987: 100_000.0,
        },
        band: [10_000.0, 20_000.0],
        rev_gate: [500.0, 3500.0, 0.000_222_222_22],
        misfire_speed: 80.0,
        misfire_heat: 2_215_000.0,
        stall_speed: 40.0,
    };
}

/// `CEngine::CDef` members (`notes/physics/renames/engine-driveline.csv`).
pub(super) const ENGINE_DEF_ABS_SCALE: u32 = 0x282c;

/// The engine definition's fields the ported kernels read (`CEngine::CDef`, `this+4`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineDef {
    /// Scale of the absolute rpm term (`Sim410`), `+0x282c`.
    pub abs_scale: f32,
    /// Low and high ends of the four compliance stages (`SimE9F0`): `+0x118`/`+0x128`, `+0x158`/`+0x168`,
    /// `+0x198`/`+0x1a8`, `+0x1d8`/`+0x1e8`.
    pub stage_low: [f32; 4],
    pub stage_high: [f32; 4],
    /// The fifth stage, used when `enable` (`+0x26f8`) is set: `+0xd8` (low) and `+0xe8` (high).
    pub extra_stage_low: f32,
    pub extra_stage_high: f32,
    pub enable: u32,
    /// Torque curve coefficients (`SimED70`, names not known): `+0x221c`, `+0x2218`, `+0x2214` with the scale
    /// `+0x21e0`; `+0x2204`, `+0x2200`, `+0x21fc` with `+0x21dc`; `+0x2234`, `+0x2230`, `+0x222c` with `+0x21e4`.
    pub torque_a: [f32; 3],
    pub torque_a_scale: f32,
    pub torque_b: [f32; 3],
    pub torque_b_scale: f32,
    pub torque_c: [f32; 3],
    pub torque_c_scale: f32,
}

/// The definition's fields `SimE660` reads past the 0x2830 bytes the other kernels read (names ours): the four load
/// denominators (`+0x138`, `+0x178`, `+0x1b8`, `+0x1f8`), the span of each load term (`+0x284c`), the threshold
/// term's slope (`+0x285c`) and the threshold (`+0x286c`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LoadDef {
    pub load_den: [f32; 4],
    pub load_span: f32,
    pub threshold_slope: f32,
    pub threshold: f32,
}

impl LoadDef {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_load_def(at)
    }
}

impl EngineDef {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_engine_def(at)
    }
}

/// `CEngine`'s own pedal: the `CReal` header at +0x14 (its value at +0x20).
pub(super) const ENGINE_PEDAL: u32 = 0x14;

/// The engine object's own fields (`CEngine`, not its objects): the pedal (`+0x14` is its `CReal` header, dirty at
/// `+0x1c`, the value at `+0x20`) and the misfire level (`+0x194`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineThis {
    pub pedal: Lane,
    pub misfire_level: f32,
}

impl EngineThis {
    pub fn load<S: EngineStore>(mem: &S, engine: u32) -> Self {
        mem.load_engine_this(engine)
    }

    pub fn store<S: EngineStore>(&self, mem: &mut S, engine: u32) {
        mem.store_engine_this(engine, self)
    }
}

/// `CEngine::CState` members (`notes/physics/renames/engine-driveline.csv`). A `CReal`'s dirty word is named on its own;
/// `ENGINE_STATE_SMOOTHED` is a `Lane` header (its value at +0xc).
pub(super) const ENGINE_STATE_CRANK_ANGLE: u32 = 0x178;
pub(super) const ENGINE_STATE_CRANK_COUNT: u32 = 0x17c;
pub(super) const ENGINE_STATE_CRANK_COUNT_PREV: u32 = 0x180;
pub(super) const ENGINE_STATE_ENGINE_SPEED: u32 = 0x188;
pub(super) const ENGINE_STATE_TORQUE_ACCUM: u32 = 0x238;
pub(super) const ENGINE_STATE_ABS_VALUE_DIRTY: u32 = 0x248;
pub(super) const ENGINE_STATE_ABS_VALUE: u32 = 0x24c;
pub(super) const ENGINE_STATE_SCALED_VALUE_DIRTY: u32 = 0x258;
pub(super) const ENGINE_STATE_SCALED_VALUE: u32 = 0x25c;
pub(super) const ENGINE_STATE_COMPLIANCE_DIRTY: u32 = 0x268;
pub(super) const ENGINE_STATE_COMPLIANCE_INVERSE: u32 = 0x26c;
pub(super) const ENGINE_STATE_SMOOTHED: u32 = 0x1e4;

/// The engine state (`CEngine::CState`, `this+8`): the fields the ported kernels read or write. The values are
/// carried for every kernel, so `load` and `store` cover the same bytes (0x274).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineState {
    /// Crank angle, rad, wrapped at 4 pi (`+0x178`).
    pub crank_angle: f32,
    /// Crank position, ticks of 256 per 4 pi (`+0x17c`), and the previous one (`+0x180`).
    pub crank_count: u32,
    pub crank_count_prev: u32,
    /// Misfire state (`+0x10`, `+0x14`, `+0x18`, and the output at `+0x3c`). The meanings are not known.
    pub misfire_rate: f32,
    pub misfire_time: f32,
    pub misfire_count: f32,
    pub misfire_out: f32,
    /// Series compliance (`SimE9F0`): the four stage values (`+0xb8`, `+0xf0`, `+0x128`, `+0x160`), the fifth at
    /// `+0x80`, and the combined value (`+0x26c`, a `CReal` whose dirty flag is at `+0x268`).
    pub stage: [f32; 4],
    pub extra_stage: f32,
    pub compliance_dirty: u32,
    pub compliance_inverse: f32,
    /// Engine speed, rad/s (`+0x188`).
    pub speed: f32,
    /// The absolute rpm value (`+0x24c`, dirty `+0x248`).
    pub abs_value_dirty: u32,
    pub abs_value: f32,
    /// Scaled compliance (`Sim4A0`, not ported yet): `+0x25c` (dirty `+0x258`).
    pub scaled_value_dirty: u32,
    pub scaled_value: f32,
    /// The rate `+0x184` (its sign picks the torque sign). The four loads (`+0xbc`, `+0xf4`, `+0x12c`, `+0x164`) are
    /// each divided by a definition denominator in `SimE660`; `threshold_value` (`+0x23c`) is compared with the
    /// definition's threshold. The torque (`+0x270`) and its accumulation (`+0x238`) are written there. The names
    /// are ours.
    pub signed_rate: f32,
    pub load_bc: f32,
    pub load_f4: f32,
    pub load_12c: f32,
    pub load_164: f32,
    pub threshold_value: f32,
    pub torque_accum: f32,
    pub torque: f32,
    /// Slot `i` (`SimD870`, `i < 4`): `+0x94 + 0x38 * i` (`slot_a`) and `+0xac + 0x38 * i` (`slot_b`). The names
    /// are ours.
    pub slot_a: [f32; 4],
    pub slot_b: [f32; 4],
    /// The smoothed damper value (`SimE060`, a CReal at `+0x1e4`: dirty `+0x1ec`, value `+0x1f0`).
    pub smoothed: Lane,
    /// The solved damper value (`SimE1F0`, a CReal at `+0x1f4`: dirty `+0x1fc`, value `+0x200`).
    pub solved: Lane,
}

impl EngineState {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_engine_state(at)
    }

    /// Writes every field back (the ones a kernel did not change keep their bytes).
    pub fn store<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_engine_state(at, self)
    }
}

/// The engine's option block (`this+0x48`). `gate_58` (`+0x58`) enables the absolute rpm term (`Sim410`) and is
/// the first stall gate (`SimECF0`), whose second is `gate_5c`. `stall_terms` (`+0x8`, `+0x18`, `+0x28`, `+0x38`)
/// count in `SimECF0`; `stage_t` (`+0xc`, `+0x1c`, `+0x2c`, `+0x3c`) and `fifth_t` (`+0x70`) are the stage fractions
/// of `SimE9F0`; `rev_gate` (`+0x74`) gates `SimE470`. The meanings beyond the uses are not known.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineOptions {
    pub gate_58: f32,
    pub gate_5c: f32,
    pub stall_terms: [f32; 4],
    pub stage_t: [f32; 4],
    pub fifth_t: f32,
    pub rev_gate: u32,
    /// Four option words that `SimE660` scales by 400 (`+0x14`, `+0x24`, `+0x34`, `+0x44`); their meaning is not known.
    pub load_terms: [f32; 4],
    /// The roll gate of `SimD870` (`+0x4`): zero returns no pointer. The roll works on the stall terms
    /// (`stall_terms[i]`, the words at `+0x8 + 0x10 * i`).
    pub roll_gate: u32,
}

impl EngineOptions {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_engine_options(at)
    }

    /// Writes the words `SimD870` changes (the stall terms); the rest of the block keeps its bytes.
    pub fn store_stall_terms<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_stall_terms(at, self)
    }
}

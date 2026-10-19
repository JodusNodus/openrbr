//! The drive definition's curve rows and tables and the curve workers' state (`Worker4060`, `Hook4160`).

use super::drive_objects::IntLane;
use super::worker::{DriveWorker, GearWork};
use crate::engine_driveline::drive_curve;
use crate::engine_driveline::store::EngineStore;
use crate::Variant;

/// The E39 curve words of the def (`n`): the rows at `+0x4ec`, `0xb0` apart, and the three ten-knot tables at `+0x59c`,
/// `+0x5c4` and `+0x5ec`. The last knot of the third table is the value at `+0x610`. Names are ours.
const CURVE_ROW_AT: u32 = 0x4EC;
const CURVE_ROW_STRIDE: u32 = 0xB0;
pub(super) const CURVE_TABLE_A_AT: u32 = 0x59C;
pub(super) const CURVE_TABLE_B_AT: u32 = 0x5C4;
pub(super) const CURVE_TABLE_C_AT: u32 = 0x5EC;

/// The ratio ring of the drive state at `state` (`+0x388`, `E39_RatioCA0`).
pub fn ratio_ring_at(state: u32) -> u32 {
    state + 0x388
}

/// The curve state of block `k` of the drive state at `state` (`+0x48`, 200 bytes a block): `Hook4160`'s first
/// argument, and the block's slip words.
pub fn curve_state_at(state: u32, k: u32) -> u32 {
    state + 0x48 + 200 * k
}

/// The def's curve block `k` (`+0x12c`, 240 bytes a block): `Hook4160`'s second argument.
pub fn hook_block_at(def: u32, k: u32) -> u32 {
    def + 0x12c + 0xf0 * k
}

/// The curve input of the drive `this` (`+0x10`): `Hook4160`'s fourth argument.
pub fn curve_input_at(this: u32) -> u32 {
    this + 0x10
}

/// The address of row `index` of the def at `def`.
pub fn curve_row_at(def: u32, index: u32) -> u32 {
    def + CURVE_ROW_AT + CURVE_ROW_STRIDE * index
}

/// A curve row: the breaks (`+0xc`, `+0x1c`, `+0x2c`, `+0x3c`, 0x10 apart), the values (`+0x6c` to `+0x9c`, 0x10 apart),
/// the gain (`+0x4c`), the rate (`+0x5c`) and the counter's limit (`+0xac`). The row's breaks are `breaks[0]` (`+0xc`),
/// `breaks[2]` (`+0x2c`) and `breaks[1]` (`+0x1c`, the upper one), and `breaks[3]` (`+0x3c`) is the lower break `WorkerF60`
/// uses. The names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveRow {
    pub breaks: [u32; 4],
    pub values: [f32; 4],
    pub gain: f32,
    pub rate: f32,
    pub limit: u32,
}

impl CurveRow {
    /// The row at `at` (see [`curve_row_at`]).
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_curve_row(at)
    }
}

/// The three ten-knot tables of the def: `a` (`+0x59c`), `b` (`+0x5c4`) and `c` (`+0x5ec`). `c[9]` is the value at `+0x610`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveTables {
    pub a: [f32; 10],
    pub b: [f32; 10],
    pub c: [f32; 10],
}

impl CurveTables {
    pub fn load<S: EngineStore>(mem: &S, def: u32) -> Self {
        mem.load_curve_tables(def)
    }
}

/// The drive state's words the curve workers read and update (`b`, also Hook4160's `a1`): the position (an `IntLane` at
/// `+0x70`), the slope (`+0x1c`), the term's gain (`+0xac`), the lane (an `IntLane` at `+0x80`) and the counter (an
/// `IntLane` at `+0xb8`, its value at `+0xc4`). Names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveState {
    pub pos: IntLane,
    pub slope: f32,
    pub term_gain: f32,
    pub lane: IntLane,
    pub counter: IntLane,
}

impl CurveState {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_curve_state(at)
    }

    pub fn store<S: EngineStore>(&self, mem: &mut S, at: u32) {
        mem.store_curve_state(at, self)
    }
}

/// The input block of Hook4160 (`x`): the words at `+0x60` and `+0x70`. Names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CurveInput {
    pub w60: f32,
    pub w70: f32,
}

impl CurveInput {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_curve_input(at)
    }
}

/// The car block's words Hook4160 reads (`c`): the row index (`+0xdc`), the gain (`+0x6c`), the speed term's scales
/// (`+0xec` and `+0xcc`), the position term's scales (`+0x9c`, `+0xbc` and `+0x8c`) and the denominators (`+0x7c` and
/// `+0x3c`). Names are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HookBlock {
    pub index: u32,
    pub gain: f32,
    pub e_scale: f32,
    pub dt_scale: f32,
    pub p_scale: f32,
    pub x1_scale: f32,
    pub w_scale: f32,
    pub den_a: f32,
    pub den_b: f32,
}

impl HookBlock {
    pub fn load<S: EngineStore>(mem: &S, at: u32) -> Self {
        mem.load_hook_block(at)
    }
}

/// Runs `port` on a fresh load of the worker at `this` and stores it back.
pub fn with_worker<S: EngineStore, T>(mem: &mut S, this: u32, port: impl FnOnce(&mut DriveWorker) -> T) -> T {
    let mut w = DriveWorker::load(mem, this);
    let out = port(&mut w);
    w.store(mem, this);
    out
}

/// `with_worker` with the gear's words too (the gear workers load them).
pub fn with_gear<S: EngineStore, T>(
    mem: &mut S,
    this: u32,
    port: impl FnOnce(&mut DriveWorker, &mut GearWork) -> T,
) -> T {
    let mut w = DriveWorker::load(mem, this);
    let mut g = GearWork::load(mem, this);
    let out = port(&mut w, &mut g);
    w.store(mem, this);
    g.store(mem, this);
    out
}

/// The edge of `Hook4160` as the step calls it: the block's curve state at `state_at` and the def's block at `block_at`
/// are loaded, the term runs with the def's row and tables and the drive's input, and the state is stored back. `this`
/// is the drive (its gate and input), `def` the definition.
pub fn curve_term_at<S: EngineStore>(
    mem: &mut S,
    this: u32,
    def: u32,
    state_at: u32,
    block_at: u32,
    dt: f32,
    variant: Variant,
) -> f32 {
    let gate = mem.obj_gate(this);
    let block = HookBlock::load(mem, block_at);
    let row = CurveRow::load(mem, curve_row_at(def, block.index));
    let tables = CurveTables::load(mem, def);
    let input = CurveInput::load(mem, curve_input_at(this));
    let mut state = CurveState::load(mem, state_at);
    let aval = drive_curve::wheel_curve_term(gate, &block, &row, &tables, &mut state, &input, dt, variant);
    state.store(mem, state_at);
    aval
}

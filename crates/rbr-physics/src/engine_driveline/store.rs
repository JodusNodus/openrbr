//! The engine's and the drive's store (`crate::store`): the layout's records (`load_*`/`store_*`, which the records'
//! `load` and `store` forward to) and its table and word readers. `Mem` implements it in `layout/mem_store.rs`.

use super::drive_line::DriveLinePoint;
use super::layout::{
    BlendKnot, CurveInput, CurveKnot, CurveRow, CurveState, CurveTables, Drive, DriveDef, DriveObjects, DriveState,
    DriveWorker, EngineDef, EngineObjects, EngineOptions, EngineState, EngineThis, GearBand, GearWork, HookBlock,
    LaneScale, LoadDef, RatioRing, UnitSplit, WorkerDef,
};
use super::sim::{BlendRec, Bracket};
use crate::math::vec::Vec4;
use crate::spring_damper::store::SpringStore;
use crate::spring_damper::{BlendTerms, Relaxation};

pub trait EngineStore: SpringStore {
    fn load_drive(&self, at: u32) -> Drive;
    fn store_drive(&mut self, at: u32, value: &Drive);
    fn load_drive_def(&self, at: u32) -> DriveDef;
    fn load_drive_state(&self, at: u32) -> DriveState;
    fn store_drive_state(&mut self, at: u32, value: &DriveState);
    fn store_commanded_gear(&mut self, at: u32, value: &DriveState);
    fn drive_objects(&self, drive: u32) -> DriveObjects;
    /// The row at `at` (see [`curve_row_at`]).
    fn load_curve_row(&self, at: u32) -> CurveRow;
    fn load_curve_tables(&self, def: u32) -> CurveTables;
    fn load_curve_state(&self, at: u32) -> CurveState;
    fn store_curve_state(&mut self, at: u32, value: &CurveState);
    fn load_curve_input(&self, at: u32) -> CurveInput;
    fn load_hook_block(&self, at: u32) -> HookBlock;
    fn engine_objects(&self, engine: u32) -> EngineObjects;
    fn load_load_def(&self, at: u32) -> LoadDef;
    fn load_engine_def(&self, at: u32) -> EngineDef;
    fn load_engine_this(&self, engine: u32) -> EngineThis;
    fn store_engine_this(&mut self, engine: u32, value: &EngineThis);
    fn load_engine_state(&self, at: u32) -> EngineState;
    /// Writes every field back (the ones a kernel did not change keep their bytes).
    fn store_engine_state(&mut self, at: u32, value: &EngineState);
    fn load_engine_options(&self, at: u32) -> EngineOptions;
    /// Writes the words the stall roll changes (the stall terms); the rest of the block keeps its bytes.
    fn store_stall_terms(&mut self, at: u32, value: &EngineOptions);
    /// The band of `gear` (any value, read at the band's offset as the original does).
    fn load_gear_band(&self, def: u32, gear: u32) -> GearBand;
    /// The definition at `def`.
    fn load_worker_def(&self, def: u32) -> WorkerDef;
    fn load_drive_worker(&self, this: u32) -> DriveWorker;
    fn store_drive_worker(&mut self, this: u32, value: &DriveWorker);
    fn load_gear_work(&self, this: u32) -> GearWork;
    /// Writes the gear's scale and time step (the state's gear is the one `load` read).
    fn store_gear_work(&mut self, this: u32, value: &GearWork);
    /// The ring at `at`.
    fn load_ratio_ring(&self, at: u32) -> RatioRing;
    fn store_ratio_ring(&mut self, at: u32, value: &RatioRing);
    fn load_lane_scale(&self, at: u32) -> LaneScale;
    /// Writes the record at `at` (the four words; the rest of the object is left alone).
    fn store_unit_split(&mut self, at: u32, value: &UnitSplit);
    /// The block's gain that the curve workers scale by.
    fn curve_gain(&self, block: u32) -> f32;
    /// The block's row index that `drive_curve::row_value` picks the row by.
    fn curve_index(&self, block: u32) -> u32;
    /// The gate word of the drive object that the curve term reads.
    fn obj_gate(&self, this: u32) -> u32;
    fn drive_record_f32(&self, this: u32, a: u32, at: u32) -> f32;
    /// The manager's second word (the table manager's `mgr_b` record).
    fn mgr_b_word8(&self, this: u32) -> u32;
    /// Entry `idx` of the table the manager points to.
    fn mgr_b_entry(&self, this: u32, idx: u32) -> u32;
    /// The drive line's stores: the entries, the count, and entries `n..10000` with zero position and tangent and distance -1. The
    /// tags of the filled entries are left as they were. The log lines are skipped; a count over 10000 traps in the
    /// original.
    fn store_drive_line(&mut self, this: u32, entries: &[DriveLinePoint]);
    /// The lerp of the drive line entry `idx`'s tangent and entry `idx + 1`'s, at `t`. The count check is the caller's
    /// (the original traps past it).
    fn lerp_at(&self, this: u32, idx: i32, t: f32) -> Vec4;
    /// Row `i` of the definition's exhaust table: eight floats.
    fn exhaust_row(&self, def: u32, i: u32) -> [f32; 8];
    /// The `n` knots of a curve at `at`.
    fn curve_knots(&self, at: u32, n: usize) -> Vec<CurveKnot>;
    /// The keys of a bracket table (the argument of `find_bracket`): each record's key.
    fn bracket_keys(&self, at: u32) -> Vec<f32>;
    /// The knots of a two-level blend table at `at`: each outer record has a key and a sub-table, which is read with its
    /// own count.
    fn blend_table(&self, at: u32) -> Vec<BlendKnot>;
    /// The four words of a `BlendTerms` block at `at`.
    fn load_blend_terms(&self, at: u32) -> BlendTerms;
    fn store_blend_terms(&mut self, at: u32, block: BlendTerms);
    /// The four words of a `Relaxation` block at `at`.
    fn load_relaxation(&self, at: u32) -> Relaxation;
    fn store_relaxation(&mut self, at: u32, block: Relaxation);
    /// The four outputs of `find_bracket`, written through the pointers the caller passed (`hi`, `lo` as ints).
    fn store_bracket(&mut self, b: &Bracket, out: [u32; 4]);
    /// `n` consecutive floats at `at`.
    fn f32_table(&self, at: u32, n: usize) -> Vec<f32>;
    /// The `count` records of a blend table (the argument of `blend_tables`), described at `desc`. The count is the first
    /// table's, and the second table is read with the same count.
    fn blend_records(&self, desc: u32, count: usize) -> Vec<BlendRec>;
    /// The drive's definition, which the drive setup fills.
    fn drive_def(&self, drive: u32) -> u32;
    fn load_entry(&self, at: u32) -> DriveLinePoint;
    fn store_entry(&mut self, at: u32, e: DriveLinePoint);
}

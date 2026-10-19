//! Replays the original's E33/E36 collision calls through the Rust ports.
//!
//! Generate files with `python3 harness/vcu_vectors.py <snapshot> <out>` and set
//! `RBR_COLLISION_VECTORS` to one or more output paths separated by `:`. The tests
//! skip when that variable is absent or the selected files contain no calls for a
//! function.

mod common;

use common::shared_mem;
use rbr_physics::collision::{
    body, collide_big, dictionary, e36, e38, edge_edge, elem, event, grid, layout, links, list, math, sort, state,
    vec_set, wall,
};
use rbr_physics::math::vec::Vec3;
use rbr_physics::mem::Mem;
use rbr_physics::Variant;

const MIN_MAX_A: u32 = 0x4e0ae0;
const MIN: u32 = 0x4e0750;
const MAX: u32 = 0x4e0780;
const SCALED: u32 = 0x4e0700;
const ALL_BELOW_ZERO: u32 = 0x4e07b0;
const BOX_HIT: u32 = 0x4e0e40;
const BODY_HAS_GRID: u32 = 0x4e11b0;
const BODY_GRID_ACTIVE: u32 = 0x4e11c0;
const BODY_HAS_GRID_TWIN: u32 = 0x4e11e0;
const BODY_GRID_PTR: u32 = 0x4e11f0;
const BODY_COPY_BOUNDS: u32 = 0x4e1200;
const GRID_FLAG_68A: u32 = 0x4e0820;
const GRID_FLAG_68F: u32 = 0x4e0830;
const GRID_READ_CLEAR: u32 = 0x4e0a20;
const GRID_STORE_VEC: u32 = 0x4e0a50;
const GRID_EDGE_AT: u32 = 0x4e0a60;
const GRID_EDGE_COUNT: u32 = 0x4e0a70;
const GRID_ACTIVE: u32 = 0x4e0a80;
const GRID_EDGE_FOR_A: u32 = 0x4e0a90;
const GRID_EDGE_FOR_B: u32 = 0x4e0ab0;
const GRID_PAIR_COUNT: u32 = 0x4e0ad0;
const GRID_SAMPLE_A: u32 = 0x4e0840;
const GRID_SAMPLE_B: u32 = 0x4e0950;
const BODY_LINK_HEAD: u32 = 0x4e0000;
const BODY_LINK_CTOR: u32 = 0x4e0010;
const BODY_LINK_UNLINK: u32 = 0x4e0020;
const BODY_LINK_NOTIFY_FILTERED: u32 = 0x4e0080;
const CELL_LINK_CTOR: u32 = 0x4e01d0;
const CELL_GRID_CTOR: u32 = 0x4e0550;
const CELL_GRID_LOOKUP_TWIN: u32 = 0x4e03b0;
const DICT_COUNT: u32 = 0x4e0bd0;
const DICT_ENTRY: u32 = 0x4e0c00;
const DICT_LOOKUP: u32 = 0x4e0b70;
const MAKE_EVENT: u32 = 0x4e0d00;
const BODY_DEF_CTOR: u32 = 0x4e0fd0;
const BODY_DEF_DTOR: u32 = 0x4e1000;
const BODY_CTOR: u32 = 0x4e3460;
const ENTITY_CTOR: u32 = 0x51a640;
const BODY_DEF_VF4C: u32 = 0x4e0ff0;
const BODY_STATE_VF20: u32 = 0x4e1050;
const BODY_STATE_VF1C: u32 = 0x4e1060;
const BODY_STATE_CTOR: u32 = 0x4e1070;
const BODY_STATE_UNINIT: u32 = 0x4e10e0;
const BODY_STATE_GET_OWNER: u32 = 0x4e1100;
const BODY_VF40: u32 = 0x4e1120;
const BODY_VF44: u32 = 0x4e1140;
const BODY_VFCC: u32 = 0x4e1230;
const VFCC_SCRATCH: u32 = 0x7000_0000;
const RAY_SEEDS: u32 = 0x4f6550;
const RAY_CLEAR: u32 = 0x4f6740;
const GRID_ADD_EDGE: u32 = 0x4f65b0;
const E09_VEC4_CLEAR: u32 = 0x465d80;
const BODY_CLEAR_GRID: u32 = 0x4e11a0;
const BODY_STATE_UNKA44: u32 = 0x4e33e0;
const E36_SELECT_TABLE: u32 = 0x4f5ca0;
const ENTITY_VF44: u32 = 0x519e60;
const MAT_FETCH_X: u32 = 0x51b6c0;
const BODY_SCRATCH_VF04: u32 = 0x4e1110;
const BODY_SCRATCH_CTOR: u32 = 0x4e3440;
const BRAKE_WALL_VF88: u32 = 0x4e38e0;
const BRAKE_WALL_VF110: u32 = 0x4e3860;
const BRAKE_WALL_VF11C: u32 = 0x4e3870;
const BRAKE_WALL_QUERY_POINT: u32 = 0x4e3dc0;
const BRAKE_WALL_QUERY_BOX: u32 = 0x4e3f10;
const BRAKE_WALL_VF15C: u32 = 0x4e4070;
const BRAKE_WALL_ADD_COUNT: u32 = 0x4e3810;
const E36_SPHERE_BOX: u32 = 0x4f2ce0;
const E36_GET_680: u32 = 0x4f2c30;
const E36_GET_68A: u32 = 0x4f2c40;
const E36_COLLIDE_INIT: u32 = 0x4f2c50;
const E36_NOP: u32 = 0x4f2cd0;
const E36_OBB_TEST: u32 = 0x4f3030;
const E36_COLLECT: u32 = 0x4f3950;
/// E36Collect's frame (the test's own memory; the binding passes the original's frame below the call).
const COLLECT_FRAME: u32 = 0x7000_0800;
const E36_ALLOC_EVENT: u32 = 0x4f4f10;
const E37_REFRESH_4A20: u32 = 0x4f4a20;
const E37_STORE_5570: u32 = 0x4f5570;
const E37_WALK_5FF0: u32 = 0x4f5ff0;
const E37_M74A0: u32 = 0x4f74a0;
const E37_EDGE_PAIR: u32 = 0x4f8560;
const E37_TEST_4B20: u32 = 0x4f4b20;
const COLLISION_EVENT_GET_94: u32 = 0x4f3f80;
const E37_GET_F7F0: u32 = 0x4f4070;
const E37_ZERO_4090: u32 = 0x4f4090;
const E37_NOP_40A0: u32 = 0x4f40a0;
const E37_ZERO_40B0: u32 = 0x4f40b0;
const E37_GET_795E14A: u32 = 0x4f40c0;
const E37_GET_795E14B: u32 = 0x4f40d0;
const E37_GET_72AFCC: u32 = 0x4f40e0;
const E37_CMP_INT: u32 = 0x4f3f90;
const E37_CMP_FLOAT: u32 = 0x4f3fb0;
const E37_FIND_INT: u32 = 0x4f4030;
const E37_INIT_70: u32 = 0x4f4080;
const E37_PLANE_SCALE: u32 = 0x4f4190;
const E37_ELEM_T68A: u32 = 0x4f5890;
const E37_ELEM_AT_E8: u32 = 0x4f58a0;
const E37_ELEM_AT_E10: u32 = 0x4f58b0;
const E37_ELEM_IDX7: u32 = 0x4f58c0;
const E37_ELEM_GET_684: u32 = 0x4f58e0;
const E37_ELEM_SET_68A: u32 = 0x4f58f0;
const E37_ELEM_T158_2: u32 = 0x4f5920;
const E37_ELEM_T158_4: u32 = 0x4f5930;
const E37_ELEM_GET_30: u32 = 0x4f5940;
const E37_ELEM_GET_04: u32 = 0x4f5950;
const E37_ELEM_SET_04: u32 = 0x4f5960;
const E37_ELEM_GET_00: u32 = 0x4f5970;
const E37_ELEM_T04_2: u32 = 0x4f5980;
const E37_ELEM_VALID_5990: u32 = 0x4f5990;
const E37_ELEM_GET_08: u32 = 0x4f59b0;
const E37_ELEM_GET_0C_W: u32 = 0x4f59c0;
const E37_ELEM_GET_10: u32 = 0x4f59d0;
const E37_ELEM_GET_0E_W: u32 = 0x4f59e0;
const E37_ELEM_GET_14: u32 = 0x4f59f0;
const E37_ELEM_GET_18: u32 = 0x4f5a00;
const E37_ELEM_GET_1C: u32 = 0x4f5a10;
const E37_ELEM_GET_20_W: u32 = 0x4f5a20;
const E37_ELEM_GET_22_W: u32 = 0x4f5a30;
const E37_ELEM_GET_24: u32 = 0x4f5a40;
const E37_ELEM_GET_28: u32 = 0x4f5a50;
const E37_ELEM_GET_2C: u32 = 0x4f5a60;
const E37_ZERO_5A70: u32 = 0x4f5a70;
const E37_GET_5A80: u32 = 0x4f5a80;
const E37_NOP_5CF0: u32 = 0x4f5cf0;
const E37_COMPOSE_9: u32 = 0x4f5d00;
const E37_TRI_5A90: u32 = 0x4f5a90;
const E37_RUN_56C0: u32 = 0x4f56c0;
const E37_ELEM_5F20: u32 = 0x4f5f20;
const E38_VEC_SCALE: u32 = 0x4fcc70;
const E38_VEC_CCB0: u32 = 0x4fccb0;
const E38_VEC_CD10: u32 = 0x4fcd10;
const E38_VEC_CDD0: u32 = 0x4fcdd0;
const E38_VEC_CD60: u32 = 0x4fcd60;
const E38_VEC_CEE0: u32 = 0x4fcee0;
const E38_VEC_CF30: u32 = 0x4fcf30;
const E38_VEC_CFA0: u32 = 0x4fcfa0;
const E38_VEC_D030: u32 = 0x4fd030;
const E38_NORMALIZE_D0F0: u32 = 0x4fd0f0;
const E38_SAT_D230: u32 = 0x4fd230;
const E38_SAT_D2C0: u32 = 0x4fd2c0;
const E38_RAY_TRIANGLE_D480: u32 = 0x4fd480;
const E38_MESH_AC50: u32 = 0x4fac50;
const E38_CLEAR_MESH_SCRATCH: u32 = 0x4fad10;
const E38_SOLVER_DEF_UNK20: u32 = 0x4fb330;
const E38_SOLVER_MATERIAL_TAG: u32 = 0x4fb720;
const E36_SUBBLOCK_INIT: u32 = 0x4f3e80;
const MATERIAL_TAG_520940: u32 = 0x520940;
const E37_COPY_80: u32 = 0x4f4200;
const E37_CMP_15: u32 = 0x4f4570;
const E37_ZERO_3: u32 = 0x4f4b40;
const E37_FIRST: u32 = 0x4f4b50;
const E37_FIRST_TWIN: u32 = 0x4f4b60;
const E37_AT_8: u32 = 0x4f4b70;
const E37_ZERO_4: u32 = 0x4f4b80;
const E37_LIST_INSERT: u32 = 0x4f4b90;
const E37_UNLINK: u32 = 0x4f4bc0;
const E37_RATE_4C70: u32 = 0x4f4c70;
const E37_LIST_RELEASE: u32 = 0x4f4ea0;
const E37_RELEASE_4F50: u32 = 0x4f4f50;
const E37_RELEASE_4F60: u32 = 0x4f4f60;
const E37_LIST_SET_1C: u32 = 0x4f5470;
const E37_VEC16_MIN: u32 = 0x4f5480;
const E37_VEC16_MAX: u32 = 0x4f54b0;
const E37_ALL_NON_NEG: u32 = 0x4f54e0;
const E37_ALL_NON_POS: u32 = 0x4f5510;
const E37_DOT_X: u32 = 0x4f5530;
const E37_STORE_5630: u32 = 0x4f5630;
const E37_PUSH_FRONT: u32 = 0x4f4c30;
const E37_EVENT_DTOR_BODY: u32 = 0x4f52c0;
const E37_EVENT_VF1CX: u32 = 0x4f52f0;
const E37_LIST_DRAIN: u32 = 0x4f4ec0;
const E37_COLLISION_LIST_VF08X: u32 = 0x4f4f40;
const E37_DRAIN_4F70: u32 = 0x4f4f70;
const E45_CELL_QUERY: u32 = 0x5240c0;
const E43_COMPOSE_A: u32 = 0x51b4c0;
const E43_MB560: u32 = 0x51b560;
const BODY_VF118: u32 = 0x4e11f0;
const ENTITY_S30: u32 = 0x519c00;
const FLOAT3_LENGTH: u32 = 0x473b30;

struct Window {
    index: usize,
    at: u32,
    len: usize,
    before: Vec<u8>,
    after: Vec<u8>,
}

struct Call {
    index: usize,
    addr: u32,
    args: Vec<u32>,
    windows: Vec<Window>,
    eax: u32,
    st0: u64,
    branch: String,
    inner: Vec<Inner>,
    after: Vec<AfterWindow>,
}

struct Inner {
    addr: u32,
    args: Vec<u32>,
    eax: u32,
    st0: u64,
    windows: Vec<Window>,
    after: Vec<AfterWindow>,
}

struct AfterWindow {
    at: u32,
    len: usize,
    bytes: Vec<u8>,
}

fn hex_bytes(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

fn parse(text: &str) -> Vec<Call> {
    let mut calls = Vec::<Call>::new();
    let mut pending = Vec::<(usize, u32, usize, Vec<u8>)>::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.first().copied() {
            Some("call") => calls.push(Call {
                index: fields[1].parse().unwrap(),
                addr: u32::from_str_radix(fields[2].trim_start_matches("0x"), 16).unwrap(),
                args: Vec::new(),
                windows: Vec::new(),
                eax: 0,
                st0: 0,
                branch: String::new(),
                inner: Vec::new(),
                after: Vec::new(),
            }),
            Some("args") => {
                calls.last_mut().unwrap().args =
                    fields[1..].iter().map(|word| u32::from_str_radix(word, 16).unwrap()).collect();
            }
            Some("win") => {
                let index = fields[1].parse().unwrap();
                let at = u32::from_str_radix(fields[2].trim_start_matches("0x"), 16).unwrap();
                let len = fields[3].parse().unwrap();
                let bytes = if fields.len() > 5 { hex_bytes(fields[5]) } else { Vec::new() };
                if fields[4] == "pre" {
                    assert_eq!(bytes.len(), len, "window pre length");
                    pending.push((index, at, len, bytes));
                } else {
                    let (before_index, before_at, before_len, before) = pending.pop().unwrap();
                    assert_eq!((before_index, before_at, before_len), (index, at, len));
                    assert_eq!(bytes.len(), len, "window post length");
                    calls.last_mut().unwrap().windows.push(Window { index, at, len, before, after: bytes });
                }
            }
            Some("ret") => {
                let call = calls.last_mut().unwrap();
                call.eax = u32::from_str_radix(fields[1], 16).unwrap();
                call.st0 = u64::from_str_radix(fields[2], 16).unwrap();
            }
            Some("inner") => calls.last_mut().unwrap().inner.push(Inner {
                addr: u32::from_str_radix(fields[1].trim_start_matches("0x"), 16).unwrap(),
                eax: u32::from_str_radix(fields[2], 16).unwrap(),
                st0: u64::from_str_radix(fields[3], 16).unwrap(),
                args: fields[4..].iter().map(|word| u32::from_str_radix(word, 16).unwrap()).collect(),
                windows: Vec::new(),
                after: Vec::new(),
            }),
            Some("iwin") => {
                let index = fields[1].parse().unwrap();
                let at = u32::from_str_radix(fields[2].trim_start_matches("0x"), 16).unwrap();
                let len = fields[3].parse().unwrap();
                let (before, after) =
                    if len == 0 { (Vec::new(), Vec::new()) } else { (hex_bytes(fields[4]), hex_bytes(fields[5])) };
                assert_eq!(before.len(), len, "inner window pre length");
                assert_eq!(after.len(), len, "inner window post length");
                calls.last_mut().unwrap().inner.last_mut().unwrap().windows.push(Window {
                    index,
                    at,
                    len,
                    before,
                    after,
                });
            }
            Some("iawin") => {
                let at = u32::from_str_radix(fields[1].trim_start_matches("0x"), 16).unwrap();
                let len = fields[2].parse().unwrap();
                let bytes = hex_bytes(fields[3]);
                assert_eq!(bytes.len(), len, "inner after window length");
                calls.last_mut().unwrap().inner.last_mut().unwrap().after.push(AfterWindow { at, len, bytes });
            }
            Some("awin") => {
                let at = u32::from_str_radix(fields[1].trim_start_matches("0x"), 16).unwrap();
                let len = fields[2].parse().unwrap();
                let bytes = hex_bytes(fields[3]);
                assert_eq!(bytes.len(), len, "after window length");
                calls.last_mut().unwrap().after.push(AfterWindow { at, len, bytes });
            }
            Some("branch") => calls.last_mut().unwrap().branch = fields[1].to_string(),
            _ => {}
        }
    }
    assert!(pending.is_empty(), "unpaired pre windows");
    calls
}

fn recorded_calls(addr: u32) -> Option<Vec<Call>> {
    let paths = std::env::var("RBR_COLLISION_VECTORS").ok()?;
    let mut found = Vec::new();
    for path in paths.split(':') {
        let text = std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{path}: {error}"));
        found.extend(parse(&text).into_iter().filter(|call| call.addr == addr));
    }
    (!found.is_empty()).then_some(found)
}

fn replay(addr: u32, name: &str, eax_mask: u32, port: impl Fn(&mut Mem, &[u32]) -> u32) {
    let Some(calls) = recorded_calls(addr) else {
        eprintln!("skipping {name}: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let eax = port(&mut mem, &call.args);
        assert_eq!(eax & eax_mask, call.eax & eax_mask, "{name} call {} ({}) EAX", call.index, call.branch);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "{name} call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("{name}: {} calls match", calls.len());
}

fn replay_st0(addr: u32, name: &str, port: impl Fn(&Mem, &[u32]) -> f32) {
    let Some(calls) = recorded_calls(addr) else {
        eprintln!("skipping {name}: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        assert_eq!(
            port(&mem, &call.args).to_bits(),
            (f64::from_bits(call.st0) as f32).to_bits(),
            "{name} call {} ({}) ST0",
            call.index,
            call.branch
        );
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.before,
                "{name} call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("{name}: {} calls match ST0", calls.len());
}

fn replay_st0_writes(addr: u32, name: &str, port: impl Fn(&mut Mem, &[u32]) -> f32) {
    let Some(calls) = recorded_calls(addr) else {
        eprintln!("skipping {name}: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        assert_eq!(
            port(&mut mem, &call.args).to_bits(),
            (f64::from_bits(call.st0) as f32).to_bits(),
            "{name} call {} ({}) ST0",
            call.index,
            call.branch
        );
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "{name} call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("{name}: {} calls match ST0 and writes", calls.len());
}

struct EdgeEdgeMemGrid<'a> {
    mem: &'a Mem,
    grid: u32,
}

impl edge_edge::GridData for EdgeEdgeMemGrid<'_> {
    fn bounds(&mut self) -> edge_edge::Bounds {
        layout::edge_edge_bounds(self.mem, self.grid)
    }

    fn face_count(&mut self) -> u32 {
        layout::edge_edge_face_count(self.mem, self.grid)
    }

    fn face_is_valid(&mut self, index: u32) -> bool {
        layout::edge_edge_face_is_valid(self.mem, self.grid, index)
    }

    fn face(&mut self, index: u32) -> edge_edge::Face {
        layout::edge_edge_face(self.mem, self.grid, index)
    }

    fn sample_indices(&mut self, index: u32) -> [u8; 3] {
        layout::edge_edge_sample_indices(self.mem, self.grid, index)
    }

    fn vertices(&mut self, indices: [u8; 3]) -> [Vec3; 3] {
        layout::edge_edge_vertices(self.mem, self.grid, indices)
    }

    fn hit_word(&mut self) -> u32 {
        layout::edge_edge_hit_word(self.mem, self.grid)
    }

    fn sample_dimensions(&mut self) -> Option<(u16, u16)> {
        layout::edge_edge_sample_dimensions(self.mem, self.grid)
    }

    fn vertex_samples(&mut self, indices: [u8; 3]) -> [[u8; 2]; 3] {
        layout::edge_edge_vertex_samples(self.mem, self.grid, indices)
    }

    fn sample_bytes(&mut self, first: i32, second: i32) -> [u8; 2] {
        layout::edge_edge_sample_bytes(self.mem, self.grid, first, second)
    }
}

#[test]
fn edge_pair_matches_original_calls() {
    replay(E37_EDGE_PAIR, "E37GridOps::EdgePair", u32::MAX, |mem, args| {
        let (grid, words) = args.split_first().expect("EdgePair has its grid");
        let words: [u32; 10] = words.try_into().expect("EdgePair has ten stack words");
        layout::edge_pair(mem, *grid, words, Variant::Stock102)
    });
}

#[test]
fn walk_5ff0_matches_original_calls() {
    replay_st0_writes(E37_WALK_5FF0, "E37GridOps::Walk5FF0", |mem, args| {
        let words: [u32; 7] = args[2..9].try_into().expect("Walk5FF0 has eight stack words");
        layout::walk_5ff0(mem, args[0], args[1], words, Variant::Stock102)
    });
}

#[test]
fn m74a0_matches_original_calls() {
    replay_st0_writes(E37_M74A0, "E37GridOps::M74A0", |mem, args| {
        layout::edge_normal(mem, args[0], args[1], args[2], args[3], args[4])
    });
}

#[test]
fn store_5570_matches_original_calls() {
    replay_st0(E37_STORE_5570, "E37VecSet::Store5570", |mem, args| {
        layout::vec_set_gap(mem, args[0], args[1], args[2], args[3], Variant::Stock102) as f32
    });
}

#[test]
fn edge_edge_matches_original_calls() {
    let Some(calls) = recorded_calls(0x4f8280) else {
        eprintln!("skipping E37GridOps::EdgeEdge: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut hits = 0;
    let mut sampled_hits = 0;
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let [grid_at, start_at, end_at, point_at, distance_at, normal_at, word_at] = call.args[..] else {
            panic!("EdgeEdge call {} args", call.index);
        };
        let start = layout::Vec4::load(&mem, start_at);
        let end = Vec3::load(&mem, end_at);
        let result = edge_edge::edge_edge(&mut EdgeEdgeMemGrid { mem: &mem, grid: grid_at }, start, end);
        let eax = if let Some(hit) = result {
            hits += 1;
            sampled_hits += usize::from(hit.samples.is_some());
            layout::store_edge_edge_hit(
                &mut mem,
                grid_at,
                start_at,
                end_at,
                point_at,
                distance_at,
                normal_at,
                word_at,
                hit,
            );
            1
        } else {
            0
        };
        assert_eq!(eax, call.eax, "EdgeEdge call {} ({}) EAX", call.index, call.branch);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "EdgeEdge call {} ({}) window {} at {:#x}",
                call.index,
                call.branch,
                window.index,
                window.at
            );
        }
    }
    assert!(hits > 0, "direct fuzz must record EdgeEdge hits");
    assert!(sampled_hits > 0, "direct fuzz must exercise sampled-surface hits");
    eprintln!("E37GridOps::EdgeEdge: {} calls match; {hits} hits, {sampled_hits} sampled", calls.len());
}

struct DictReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
}

impl DictReplayHost<'_> {
    fn call(&mut self, mem: &Mem, object: u32, slot: u32, args: &[u32]) -> u32 {
        let call = self
            .inner
            .get(self.next)
            .unwrap_or_else(|| panic!("unexpected dictionary virtual call {} on {object:#x}", self.next));
        let target = mem.u32(mem.u32(object) + slot * 4);
        assert_eq!(call.addr, target, "dictionary virtual slot {slot}");
        let mut expected_args = vec![object];
        expected_args.extend_from_slice(args);
        assert_eq!(call.args, expected_args, "dictionary virtual slot {slot} args");
        self.next += 1;
        call.eax
    }
}

impl dictionary::DictHost for DictReplayHost<'_> {
    fn count(&mut self, mem: &Mem, parent: u32) -> u32 {
        self.call(mem, parent, 3, &[])
    }

    fn entry(&mut self, mem: &Mem, parent: u32, index: i32) -> u32 {
        self.call(mem, parent, 4, &[index as u32])
    }

    fn lookup_start(&mut self, mem: &Mem, dictionary: u32) -> u32 {
        self.call(mem, dictionary, 0, &[])
    }

    fn find(&mut self, mem: &Mem, dictionary: u32, name: u32, index_out: u32) -> u32 {
        self.call(mem, dictionary, 5, &[name, index_out])
    }
}

fn replay_dict(addr: u32, name: &str, port: impl Fn(&Mem, &[u32], &mut DictReplayHost<'_>) -> u32) {
    let Some(calls) = recorded_calls(addr) else {
        eprintln!("skipping {name}: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = DictReplayHost { inner: &call.inner, next: 0 };
        let eax = port(&mem, &call.args, &mut host);
        assert_eq!(eax, call.eax, "{name} call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "{name} call {} virtual count", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "{name} call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("{name}: {} calls match", calls.len());
}

struct EventReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
}

struct AllocEventReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
}

impl AllocEventReplayHost<'_> {
    fn take(&mut self, addr: u32, args: &[u32], name: &str) -> &Inner {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected {name} call {}", self.next));
        assert_eq!(call.addr, addr, "{name} address");
        assert_eq!(call.args, args, "{name} arguments");
        self.next += 1;
        call
    }
}

struct MaterialTagReplayHost<'a> {
    call: &'a Inner,
    expected_manager: u32,
    expected_tag: u32,
    called: bool,
}

impl e38::MaterialTagHost for MaterialTagReplayHost<'_> {
    fn material_tag(&mut self, manager: u32, tag: u32) -> u32 {
        assert!(!self.called, "unexpected repeated Tag520940 call");
        assert_eq!(self.call.addr, MATERIAL_TAG_520940);
        assert_eq!(manager, self.expected_manager);
        assert_eq!(tag, self.expected_tag);
        assert_eq!(self.call.args, [manager, tag]);
        self.called = true;
        self.call.eax
    }
}

fn replay_solver_material_tag() {
    let Some(calls) = recorded_calls(E38_SOLVER_MATERIAL_TAG) else {
        eprintln!("skipping E38_004FB720: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut branches = std::collections::BTreeSet::new();
    let mut exact_half = 0;
    let mut unordered = 0;
    for call in &calls {
        assert_eq!(call.args.len(), 2, "E38_004FB720 stack arguments");
        assert_eq!(call.inner.len(), 2, "E38_004FB720 nested calls");
        let init = &call.inner[0];
        assert_eq!(init.addr, E36_SUBBLOCK_INIT, "temporary sub-block initializer");
        assert_eq!(init.args.len(), 1, "temporary sub-block this pointer");
        assert_eq!(init.windows.len(), 1, "temporary sub-block write window");
        assert_eq!(init.windows[0].at, init.args[0], "temporary sub-block address");
        assert_eq!(init.windows[0].len, 12, "temporary sub-block size");
        assert_eq!(init.windows[0].after, [0; 12], "temporary sub-block zeroing");

        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let object = call.args[0];
        let output_tag = call.args[1];
        let table = if mem.u32(object + 0xe4) & 0x800 == 0 { 0xac } else { 0x80 };
        let fraction = mem.f32(object + table + 8);
        let expected_tag = if 0.5_f32 < fraction { mem.u32(object + table + 4) } else { mem.u32(object + table) };
        exact_half += usize::from(fraction.to_bits() == 0x3f00_0000);
        unordered += usize::from(fraction.is_nan());
        branches.insert(call.branch.as_str());

        let application = mem.u32(0x0165_9b70);
        let material_manager = mem.u32(application + 0x24) + 0x18;
        let tag_call = &call.inner[1];
        assert_eq!(tag_call.addr, MATERIAL_TAG_520940, "material tag callee");
        assert_eq!(tag_call.args, [material_manager, expected_tag]);
        let mut host =
            MaterialTagReplayHost { call: tag_call, expected_manager: material_manager, expected_tag, called: false };
        // The asm stores the tag before it reads the material manager.
        let tag = e38::pick_material_tag(layout::load_material_pair(&mem, object));
        mem.set_u32(output_tag, tag);
        let manager = layout::material_manager(&mem);
        let eax = e38::MaterialTagHost::material_tag(&mut host, manager, tag);
        assert!(host.called, "Tag520940 was not called");
        assert_eq!(eax, call.eax, "E38_004FB720 call {} EAX", call.index);
        assert_eq!(mem.u32(output_tag), expected_tag, "selected material tag");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E38_004FB720 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    assert_eq!(branches, ["primary_high", "primary_low", "secondary_high", "secondary_low"].into_iter().collect());
    assert!(exact_half > 0, "strict 0.5 threshold was not recorded");
    assert!(unordered > 0, "NaN threshold behavior was not recorded");
    eprintln!("E38_004FB720: {} calls match", calls.len());
}

impl EventReplayHost<'_> {
    fn take(&mut self, addr: u32, args: &[u32], name: &str) -> &Inner {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected {name} call {}", self.next));
        assert_eq!(call.addr, addr, "{name} address");
        assert_eq!(call.args, args, "{name} arguments");
        self.next += 1;
        call
    }
}

impl event::AllocEventHost for AllocEventReplayHost<'_> {
    fn operator_new(&mut self, size: u32) -> u32 {
        self.take(0x70bec2, &[size], "operator new").eax
    }
}

struct StateReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    /// The test's memory (a clone of a shared backend), so the host reads and writes the windows at each call.
    mem: Mem,
}

impl<'a> StateReplayHost<'a> {
    fn take(&mut self, addr: u32, args: &[u32], name: &str) -> &'a Inner {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected {name} call {}", self.next));
        assert_eq!(call.addr, addr, "{name} address");
        assert_eq!(call.args, args, "{name} arguments");
        self.next += 1;
        call
    }
}

impl state::StateHost for StateReplayHost<'_> {
    fn owner_vf30(&mut self, owner: u32) -> u32 {
        let call = self.take(0x519c00, &[owner], "CBody owner slot 12");
        call.eax
    }

    fn wall_sync(&mut self, wall: u32, body: u32, vec_a: u32, vec_b: u32) -> u32 {
        let call = self.take(0x4f7090, &[wall, body, vec_a, vec_b], "E37 wall sync");
        for window in &call.windows {
            assert_eq!(self.mem.read(window.at, window.len), window.before, "E37 wall sync input at {:#x}", window.at);
        }
        for window in &call.windows {
            self.mem.map(window.at, &window.after);
        }
        call.eax
    }
}

impl body::ClearGridHost for StateReplayHost<'_> {
    fn sync_grid(&mut self, grid: u32, source: u32, lower: u32, upper: u32) -> u32 {
        state::StateHost::wall_sync(self, grid, source, lower, upper)
    }
}

struct BodyVf40ReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    mem: &'a mut Mem,
}

impl<'a> BodyVf40ReplayHost<'a> {
    fn take(&mut self, addr: u32, args: &[u32], name: &str) -> &'a Inner {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected {name} call {}", self.next));
        assert_eq!(call.addr, addr, "{name} address");
        assert_eq!(call.args, args, "{name} arguments");
        self.next += 1;
        call
    }
}

struct BodyVf44ReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    /// The test's memory (a clone of a shared backend), so the host reads the state at the moment of each call.
    mem: Mem,
}

struct BodyVfccReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    /// The test's memory (a clone of a shared backend): the host reads the matrices and writes each corner.
    mem: Mem,
}

struct CellGridReplayHost {
    allocation: u32,
    calls: usize,
}

struct CellGridCtorReplayHost {
    allocation: u32,
    bytes: u32,
    calls: usize,
}

struct NotifyFilteredReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
}

struct BodyCtorReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
}

struct CollectReplayHost<'a> {
    inner: &'a [Inner],
    used: Vec<bool>,
}

impl<'a> BodyVf44ReplayHost<'a> {
    fn take(&mut self, addr: u32, args: &[u32], name: &str) -> &'a Inner {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected {name} call {}", self.next));
        assert_eq!(call.addr, addr, "{name} address");
        assert_eq!(call.args, args, "{name} arguments");
        self.next += 1;
        call
    }
}

impl body::VfCcHost for BodyVfccReplayHost<'_> {
    fn fetch_x(&mut self, matrix: u32, out: u32, worker: u32) -> u32 {
        let call =
            self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected E33Mat34::FetchX call {}", self.next));
        assert_eq!(call.addr, MAT_FETCH_X, "E33Mat34::FetchX target");
        assert_eq!(call.args.len(), 3, "E33Mat34::FetchX arguments");
        assert_eq!(call.args[0], matrix, "E33Mat34::FetchX matrix");
        assert_eq!(call.args[2], worker, "E33Mat34::FetchX worker");
        assert_eq!(call.eax, call.args[1], "E33Mat34::FetchX result pointer");
        for index in 0..=1 {
            let window = call
                .windows
                .iter()
                .find(|window| window.index == index)
                .unwrap_or_else(|| panic!("missing E33Mat34::FetchX input window {index}"));
            assert_eq!(self.mem.read(window.at, window.len), window.before, "E33Mat34::FetchX input window {index}");
        }
        let output =
            call.windows.iter().find(|window| window.index == 2).expect("missing E33Mat34::FetchX output window");
        if self.mem.u32(matrix + 4) == worker {
            let source = self.mem.u32(matrix);
            assert_eq!(output.after, self.mem.read(source, 16), "identity E33Mat34::FetchX output");
        }
        // The corner lands at the caller's `out`, which may differ from the recorded frame.
        self.mem.map(out, &output.after);
        self.next += 1;
        out
    }
}

/// The E09Vec4::Clear calls of E38RaySeeds: each clears its vector in the shared memory (the recorded after bytes).
struct RayReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    mem: Mem,
}

impl e38::RayHost for RayReplayHost<'_> {
    fn vec4_clear(&mut self, at: u32) -> u32 {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected E09Vec4::Clear call {}", self.next));
        assert_eq!(call.addr, E09_VEC4_CLEAR, "E09Vec4::Clear address");
        assert_eq!(call.args, [at], "E09Vec4::Clear arguments");
        for window in &call.windows {
            self.mem.map(window.at, &window.after);
        }
        self.next += 1;
        call.eax
    }
}

impl CollectReplayHost<'_> {
    fn take(&mut self, addr: u32, name: &str) -> &Inner {
        let index = self
            .inner
            .iter()
            .enumerate()
            .find_map(|(index, call)| (call.addr == addr && !self.used[index]).then_some(index))
            .unwrap_or_else(|| panic!("missing {name} call to {addr:#x}"));
        self.used[index] = true;
        &self.inner[index]
    }

    fn assert_consumed(&self) {
        for (index, call) in self.inner.iter().enumerate() {
            if matches!(call.addr, BODY_VF118 | ENTITY_S30 | E43_COMPOSE_A | E43_MB560 | E45_CELL_QUERY | FLOAT3_LENGTH)
            {
                assert!(self.used[index], "unconsumed E36Collect host call {:#x}", call.addr);
            }
        }
    }
}

impl e36::CollectHost for CollectReplayHost<'_> {
    fn virtual_call(&mut self, _mem: &mut Mem, _sp: u32, target: u32, args: &[u32]) -> u32 {
        let call = self.take(target, "virtual");
        assert_eq!(call.args.first(), args.first(), "E36Collect virtual this argument");
        call.eax
    }

    fn transform(&mut self, mem: &mut Mem, _sp: u32, target: u32, this: u32, a: u32, b: u32, c: u32) {
        let call = self.take(target, "transform");
        assert_eq!(call.args.len(), 4, "E36Collect transform arguments");
        assert_eq!(call.args[0], this, "E36Collect transform receiver");
        assert_eq!(call.args[2], b, "E36Collect transform matrix");
        if target == E43_COMPOSE_A {
            let source = call.windows.iter().find(|window| window.index == 0).expect("ComposeA source window");
            let matrix = call.windows.iter().find(|window| window.index == 1).expect("ComposeA matrix window");
            assert_eq!(mem.read(a, source.len), source.before, "ComposeA source bytes");
            assert_eq!(mem.read(b, matrix.len), matrix.before, "ComposeA matrix bytes");
            let output = call.after.first().expect("ComposeA output bytes");
            assert!(output.len >= 0x10, "ComposeA output length");
            mem.map(c, &output.bytes);
        } else {
            assert_eq!(target, E43_MB560, "E36Collect transform target");
            let source = call.windows.iter().find(|window| window.index == 0).expect("MB560 source window");
            let matrix = call.windows.iter().find(|window| window.index == 1).expect("MB560 matrix window");
            let output = call.windows.iter().find(|window| window.index == 2).expect("MB560 output window");
            assert_eq!(mem.read(a, source.len), source.before, "MB560 source bytes");
            assert_eq!(mem.read(b, matrix.len), matrix.before, "MB560 matrix bytes");
            mem.map(c, &output.after);
        }
    }

    fn query_cells(&mut self, mem: &mut Mem, _sp: u32, lo: u32, hi: u32, cells: u32) {
        let call = self.take(E45_CELL_QUERY, "E45 grid query");
        let lower = call.windows.iter().find(|window| window.index == 0).expect("E45 lower bounds");
        let upper = call.windows.iter().find(|window| window.index == 1).expect("E45 upper bounds");
        let output = call.windows.iter().find(|window| window.index == 2).expect("E45 cells output");
        assert_eq!(mem.read(lo, lower.len), lower.before, "E45 lower bounds");
        assert_eq!(mem.read(hi, upper.len), upper.before, "E45 upper bounds");
        mem.map(cells, &output.after);
        for index in 0..4 {
            let cell = mem.u32(cells + index * 4);
            if cell != 0 && !call.windows.iter().any(|window| window.at == cell && window.len >= 4) {
                mem.map(cell, &[0; 0x10]);
            }
        }
        for window in &call.windows {
            if window.index >= 3 {
                mem.map(window.at, &window.after);
            }
        }
    }

    fn length(&mut self, mem: &Mem, _sp: u32, vector: u32) -> f32 {
        let call = self.take(FLOAT3_LENGTH, "CFloat3::Length");
        let window = call.windows.first().expect("CFloat3::Length vector bytes");
        assert_eq!(mem.read(vector, window.len), window.before, "CFloat3::Length input");
        f64::from_bits(call.st0) as f32
    }
}

impl grid::AllocHost for CellGridReplayHost {
    fn allocate(&mut self, bytes: u32) -> u32 {
        assert_eq!(bytes, 0x644, "CCellGrid lazy block allocation size");
        self.calls += 1;
        self.allocation
    }
}

impl grid::AllocHost for CellGridCtorReplayHost {
    fn allocate(&mut self, bytes: u32) -> u32 {
        assert_eq!(bytes, self.bytes, "CCellGrid constructor allocation size");
        self.calls += 1;
        self.allocation
    }
}

impl links::NotifyFilteredHost for NotifyFilteredReplayHost<'_> {
    fn vf04(&mut self, mem: &mut Mem, visitor: u32, body: u32) -> u32 {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected visitor Vf04 call {}", self.next));
        assert_eq!(call.addr, 0x4e3590, "visitor slot-1 target");
        assert_eq!(call.args, [visitor, body], "visitor slot-1 arguments");
        assert_eq!(call.eax, visitor, "visitor slot-1 EAX");
        for window in &call.windows {
            assert_eq!(mem.read(window.at, window.len), window.before, "visitor slot-1 input at {:#x}", window.at);
        }
        for window in &call.windows {
            mem.map(window.at, &window.after);
        }
        self.next += 1;
        call.eax
    }
}

impl layout::CtorHost for BodyCtorReplayHost<'_> {
    fn entity_ctor(
        &mut self,
        mem: &mut Mem,
        body_ptr: u32,
        definition: u32,
        frame: u32,
        parent: u32,
        flags: u32,
    ) -> u32 {
        let call =
            self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected CEntity constructor call {}", self.next));
        assert_eq!(call.addr, ENTITY_CTOR, "CEntity base constructor target");
        assert_eq!(call.args, [body_ptr, definition, frame, parent, flags], "CEntity base constructor arguments");
        assert_eq!(call.eax, body_ptr, "CEntity base constructor EAX");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.before,
                "CEntity base constructor input at {:#x}",
                window.at
            );
        }
        for window in &call.windows {
            mem.map(window.at, &window.after);
        }
        self.next += 1;
        call.eax
    }
}

impl body::Vf40Host for BodyVf40ReplayHost<'_> {
    fn state_unka44(&mut self, state_ptr: u32) -> u32 {
        let call = self.take(BODY_STATE_UNKA44, &[state_ptr], "CBody::CState::Unk44");
        for window in &call.windows {
            assert_eq!(
                self.mem.read(window.at, window.len),
                window.before,
                "CBody::CState::Unk44 input at {:#x}",
                window.at
            );
        }
        for window in &call.windows {
            self.mem.map(window.at, &window.after);
        }
        call.eax
    }

    fn entity_vf40(&mut self, body_ptr: u32) -> u32 {
        let call = self.take(0x51a4b0, &[body_ptr], "CEntity::Vf40");
        for window in &call.windows {
            assert_eq!(self.mem.read(window.at, window.len), window.before, "CEntity::Vf40 input at {:#x}", window.at);
        }
        for window in &call.windows {
            self.mem.map(window.at, &window.after);
        }
        call.eax
    }
}

impl body::Vf44Host for BodyVf44ReplayHost<'_> {
    fn select_table(&mut self, object: u32) -> u32 {
        let call = self.take(E36_SELECT_TABLE, &[object], "E36SelectTable");
        let mut input = Mem::new();
        for window in &call.windows {
            if window.index == 0 {
                assert_eq!(self.mem.read(window.at, window.len), window.before, "E36SelectTable input");
            }
            input.map(window.at, &window.before);
        }
        let selected = layout::select_table(&input, object);
        assert_eq!(selected, call.eax, "E36SelectTable result");
        selected
    }

    fn entity_vf44(&mut self, body: u32, object: u32) -> u32 {
        let call = self.take(ENTITY_VF44, &[body, object], "CEntity::Vf44");
        for window in &call.windows {
            assert_eq!(self.mem.read(window.at, window.len), window.before, "CEntity::Vf44 input at {:#x}", window.at);
        }
        for window in &call.windows {
            self.mem.map(window.at, &window.after);
        }
        call.eax
    }
}

impl event::MakeEventHost for EventReplayHost<'_> {
    fn body_grid(&mut self, mem: &Mem, body_ptr: u32) -> u32 {
        let call = self.take(0x4e11f0, &[body_ptr], "CBody::Vf118");
        assert_eq!(call.eax, layout::Body::new(body_ptr).grid(mem), "CBody::Vf118 result");
        call.eax
    }

    fn alloc_event(&mut self, size: u32) -> u32 {
        let event = self.take(0x4f4f10, &[size], "E36AllocEvent").eax;
        if self.inner.get(self.next).map(|call| call.addr) == Some(0x70bec2) {
            let allocated = self.take(0x70bec2, &[size], "operator new").eax;
            assert_eq!(allocated, event, "operator new result");
        }
        event
    }

    fn init_full(
        &mut self,
        mem: &mut Mem,
        event_ptr: u32,
        kind: u32,
        body_a: u32,
        aux_a: u32,
        aux_b: u32,
        data: &[u32; 16],
    ) -> u32 {
        let data_ptr = self.inner[self.next].args[5];
        let args = [event_ptr, kind, body_a, aux_a, aux_b, data_ptr, 0];
        let call = self.take(0x4f50e0, &args, "CCollisionEvent::InitFull");
        let payload = data.iter().flat_map(|word| word.to_le_bytes()).collect::<Vec<_>>();
        let source = call
            .windows
            .iter()
            .find(|window| window.at == data_ptr && window.len == 0x40)
            .unwrap_or_else(|| panic!("InitFull did not capture payload at {data_ptr:#x}"));
        assert_eq!(source.before, payload, "InitFull payload bytes");
        let object = call
            .windows
            .iter()
            .find(|window| window.at == event_ptr && window.len == 0xf0)
            .unwrap_or_else(|| panic!("InitFull did not capture event at {event_ptr:#x}"));
        mem.map(event_ptr, &object.after);
        call.eax
    }
}

#[test]
fn min_matches_original_calls() {
    replay(MIN, "E33Vec4::Min", u32::MAX, |mem, args| {
        let [this, other, out] = args[..] else {
            panic!("unexpected Min args: {}", args.len());
        };
        let value = layout::Vec4::load(mem, this).min_lanes(layout::Vec4::load(mem, other));
        value.store(mem, out);
        out
    });
}

#[test]
fn max_matches_original_calls() {
    replay(MAX, "E33Vec4::Max", u32::MAX, |mem, args| {
        let [this, other, out] = args[..] else {
            panic!("unexpected Max args: {}", args.len());
        };
        let value = layout::Vec4::load(mem, this).max_lanes(layout::Vec4::load(mem, other));
        value.store(mem, out);
        out
    });
}

#[test]
fn scaled_matches_original_calls() {
    replay(SCALED, "E33Vec4::Scaled", u32::MAX, |mem, args| {
        let [this, out, scale_bits] = args[..] else {
            panic!("unexpected Scaled args: {}", args.len());
        };
        layout::scaled(mem, this, out, f32::from_bits(scale_bits));
        out
    });
}

#[test]
fn all_below_zero_matches_original_calls() {
    replay(ALL_BELOW_ZERO, "E33Vec4::AllBelowZero", u32::MAX, |mem, args| {
        let [this] = args[..] else {
            panic!("unexpected AllBelowZero args: {}", args.len());
        };
        let value = layout::Vec4 { x: mem.f32(this), y: mem.f32(this + 4), z: mem.f32(this + 8), w: 0.0 };
        u32::from(math::all_below_zero(value))
    });
}

#[test]
fn min_max_a_matches_original_calls() {
    replay(MIN_MAX_A, "E33Vec4::MinMaxA", u32::MAX, |mem, args| {
        let [this, lower_source_and_upper_source, lower_out, upper_out] = args[..] else {
            panic!("unexpected MinMaxA args: {}", args.len());
        };
        layout::min_max_a(mem, this, lower_source_and_upper_source, lower_out, upper_out)
    });
}

#[test]
fn box_hit_matches_original_calls() {
    // The original only writes AL for its bool return; the upper 24 EAX bits are
    // leftover values (a vector pointer on early exits, a stack address on late ones).
    replay(BOX_HIT, "E33Vec4::BoxHit", 0xff, |mem, args| {
        let [a, b, extents, tolerance] = args[..] else {
            panic!("unexpected BoxHit args: {}", args.len());
        };
        u32::from(math::boxes_overlap(
            layout::Vec4::load(mem, a),
            layout::Vec4::load(mem, b),
            layout::Vec4::load(mem, extents),
            layout::Vec4::load(mem, tolerance),
        ))
    });
}

#[test]
fn body_grid_queries_match_original_calls() {
    replay(BODY_HAS_GRID, "CBody::Vf10C", u32::MAX, |mem, args| {
        u32::from(body::has_grid(layout::Body::new(args[0]).grid(mem)))
    });
    replay(BODY_GRID_ACTIVE, "CBody::Vf110", u32::MAX, |mem, args| {
        let grid = layout::Body::new(args[0]).grid(mem);
        let active = (grid != 0).then(|| layout::ColGrid::new(grid).active(mem));
        u32::from(body::grid_active(active))
    });
    replay(BODY_HAS_GRID_TWIN, "CBody::Vf114", u32::MAX, |mem, args| {
        u32::from(body::has_grid(layout::Body::new(args[0]).grid(mem)))
    });
    replay(BODY_GRID_PTR, "CBody::Vf118", u32::MAX, |mem, args| layout::Body::new(args[0]).grid(mem));
}

#[test]
fn body_bounds_copy_matches_original_calls() {
    replay(BODY_COPY_BOUNDS, "CBody::Vf11C", 0, |mem, args| {
        let [body_ptr, out_a, out_b] = args[..] else {
            panic!("unexpected Vf11C args: {}", args.len());
        };
        let bounds = layout::Body::new(body_ptr).bounds(mem);
        bounds.lower.store(mem, out_a);
        bounds.upper.store(mem, out_b);
        0
    });
}

#[test]
fn colgrid_flags_match_original_calls() {
    replay(GRID_FLAG_68A, "CColGrid::Flag68A", u32::MAX, |mem, args| {
        grid::status_flag_bit1_clear(layout::ColGrid::new(args[0]).status_flag(mem))
    });
    replay(GRID_FLAG_68F, "CColGrid::Flag68F", 0xff, |mem, args| u32::from(layout::ColGrid::new(args[0]).tag(mem)));
    replay(GRID_EDGE_COUNT, "CColGrid::Flag68D", u32::MAX, |mem, args| {
        u32::from(layout::ColGrid::new(args[0]).edge_count(mem))
    });
    replay(GRID_ACTIVE, "CColGrid::Flag68B", u32::MAX, |mem, args| {
        u32::from(layout::ColGrid::new(args[0]).active(mem))
    });
    replay(GRID_PAIR_COUNT, "CColGrid::Flag68C", u32::MAX, |mem, args| {
        u32::from(layout::ColGrid::new(args[0]).pair_count(mem))
    });
}

#[test]
fn colgrid_read_clear_matches_original_calls() {
    replay(GRID_READ_CLEAR, "CColGrid::ReadClear", 0, |mem, args| {
        let [grid_ptr, out_a, out_b] = args[..] else {
            panic!("unexpected ReadClear args: {}", args.len());
        };
        let flags = layout::ColGrid::new(grid_ptr).take_event_flags(mem);
        let (first, second) = grid::read_clear(flags);
        mem.set_u32(out_a, first);
        mem.set_u32(out_b, second);
        0
    });
}

#[test]
fn colgrid_vector_and_edge_helpers_match_original_calls() {
    replay(GRID_STORE_VEC, "CColGrid::StoreVec", 0, |mem, args| {
        let [grid_ptr, out] = args[..] else {
            panic!("unexpected StoreVec args: {}", args.len());
        };
        layout::ColGrid::new(grid_ptr).vector(mem).store(mem, out);
        0
    });
    replay(GRID_EDGE_AT, "CColGrid::EdgeAt", u32::MAX, |_mem, args| {
        let [grid_ptr, index] = args[..] else {
            panic!("unexpected EdgeAt args: {}", args.len());
        };
        layout::ColGrid::new(grid_ptr).edge(index)
    });
    replay(GRID_EDGE_FOR_A, "CColGrid::EdgeForA", u32::MAX, |mem, args| {
        let [grid_ptr, index] = args[..] else {
            panic!("unexpected EdgeForA args: {}", args.len());
        };
        let grid = layout::ColGrid::new(grid_ptr);
        grid::edge_for_a(grid, grid.pair(mem, index as usize))
    });
    replay(GRID_EDGE_FOR_B, "CColGrid::EdgeForB", u32::MAX, |mem, args| {
        let [grid_ptr, index] = args[..] else {
            panic!("unexpected EdgeForB args: {}", args.len());
        };
        let grid = layout::ColGrid::new(grid_ptr);
        grid::edge_for_b(grid, grid.pair(mem, index as usize))
    });
    replay(GRID_SAMPLE_A, "CColGrid::SampleA", 0, |mem, args| {
        let [grid_ptr, index, t_bits, out_a, out_b] = args[..] else {
            panic!("unexpected SampleA args: {}", args.len());
        };
        let (first, second) = layout::sample_a(mem, grid_ptr, index as usize, f32::from_bits(t_bits));
        mem.set_u32(out_a, first);
        mem.set_u32(out_b, second);
        0
    });
    // The direct records (g-colgrid) cover the grid with and without data.
    replay(GRID_SAMPLE_B, "CColGrid::SampleB", 0, |mem, args| {
        let [grid_ptr, index, out_a, out_b] = args[..] else {
            panic!("unexpected SampleB args: {}", args.len());
        };
        let (first, second) = layout::sample_b(mem, grid_ptr, index as usize);
        mem.set_u32(out_a, first);
        mem.set_u32(out_b, second);
        0
    });
}

/// E01Vec4::CopyFrom (0x43c5b0): the four words at `src` are copied to `dst`, and the copy returns `dst`.
#[test]
fn vec4_copy_from_moves_the_sixteen_bytes() {
    let mut mem = Mem::new();
    mem.map(0x1000, &[0; 32]);
    for (k, value) in [1.5f32, -2.0, 0.25, 8.0].into_iter().enumerate() {
        mem.set_f32(0x1010 + 4 * k as u32, value);
    }
    assert_eq!(layout::vec4_copy_from(&mut mem, 0x1000, 0x1010), 0x1000);
    for k in 0..4 {
        assert_eq!(mem.f32(0x1000 + 4 * k), mem.f32(0x1010 + 4 * k));
    }
    assert_eq!(mem.f32(0x1000), 1.5);
}

#[test]
fn intrusive_link_helpers_match_original_calls() {
    replay(BODY_LINK_HEAD, "CBodyLink::Head", u32::MAX, |mem, args| links::head(mem, args[0]));
    replay(BODY_LINK_CTOR, "CBodyLink::CBodyLink", u32::MAX, |mem, args| links::body_link_ctor(mem, args[0]));
    // eax is not compared: an unattached link leaves the caller's eax, which the port does not model (rust_step checks it
    // on the drive).
    replay(BODY_LINK_UNLINK, "CBodyLink::Unlink", 0, |mem, args| layout::unlink(mem, args[0]));
    replay(CELL_LINK_CTOR, "CCellLink::CCellLink", u32::MAX, |mem, args| links::cell_link_ctor(mem, args[0]));
}

#[test]
fn dictionary_parent_helpers_match_original_calls() {
    replay_dict(DICT_LOOKUP, "E33Dict::Unk18", |mem, args, host| {
        dictionary::lookup(mem, args[0], args[1], args[2], host)
    });
    replay_dict(DICT_COUNT, "E33Dict::Unk0C", |mem, args, host| dictionary::count(mem, args[0], host));
    replay_dict(DICT_ENTRY, "E33Dict::Unk10", |mem, args, host| dictionary::entry(mem, args[0], args[1] as i32, host));
}

#[test]
fn make_event_matches_original_calls() {
    let Some(calls) = recorded_calls(MAKE_EVENT) else {
        eprintln!("skipping E33MakeEvent: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let stack_tail =
            mem.u32(call.windows.iter().find(|window| window.index == 6).expect("MakeEvent stack-tail window").at);
        let [kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2] = call.args[..] else {
            panic!("unexpected E33MakeEvent args: {}", call.args.len());
        };
        let args = event::MakeEventArgs { kind, vector_a, p3, body_a, body_b, aux_a, aux_b, vector_b, z2 };
        let mut host = EventReplayHost { inner: &call.inner, next: 0 };
        let eax = event::make_event(&mut mem, args, stack_tail, &mut host);
        assert_eq!(eax, call.eax, "E33MakeEvent call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "E33MakeEvent inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E33MakeEvent call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        for window in &call.after {
            assert_eq!(
                mem.read(window.at, window.len),
                window.bytes,
                "E33MakeEvent call {} returned object at {:#x}",
                call.index,
                window.at
            );
        }
    }
    eprintln!("E33MakeEvent: {} calls match", calls.len());
}

#[test]
fn body_state_accessors_match_original_calls() {
    replay(BODY_DEF_VF4C, "CBody::CDef::Vf4c", u32::MAX, |_mem, _args| state::body_def_name());
    replay(BODY_STATE_VF20, "CBody::CState::Vf20", u32::MAX, |_mem, _args| state::state_table_vf20());
    replay(BODY_STATE_VF1C, "CBody::CState::Vf1C", u32::MAX, |_mem, _args| state::state_table_vf1c());
    replay(BODY_STATE_GET_OWNER, "CBody::CState::GetOwner", u32::MAX, |mem, args| state::state_owner(mem, args[0]));
}

#[test]
fn body_state_uninit_and_scratch_noop_match_original_calls() {
    replay(BODY_STATE_UNINIT, "CBody::CState::Uninit", 0, |mem, args| {
        state::state_uninit(mem, args[0]);
        0
    });
    replay(BODY_SCRATCH_VF04, "CBody::CScratch::Vf04", 0, |mem, args| {
        state::scratch_vf04(mem, args[0]);
        0
    });
}

#[test]
fn body_state_constructors_match_original_calls() {
    replay(BODY_DEF_CTOR, "CBody::CDef::CDef", u32::MAX, |mem, args| state::body_def_ctor(mem, args[0], args[1]));
    replay(BODY_STATE_CTOR, "CBody::CState::CState", u32::MAX, |mem, args| {
        layout::state_ctor(mem, args[0], args[1], args[2])
    });
    replay(BODY_SCRATCH_CTOR, "CBody::CScratch::CScratch", u32::MAX, |mem, args| state::scratch_ctor(mem, args[0]));
}

#[test]
fn body_def_destructor_matches_original_calls() {
    replay(BODY_DEF_DTOR, "CBody::CDef::~CDef", u32::MAX, |mem, args| state::body_def_dtor(mem, args[0]));
}

#[test]
fn body_state_unka44_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_STATE_UNKA44) else {
        eprintln!("skipping CBody::CState::Unk44: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        for inner in &call.inner {
            if inner.addr == 0x4f7090 {
                for window in &inner.windows {
                    mem.map(window.at, &window.before);
                }
            }
        }
        let mut host = StateReplayHost { inner: &call.inner, next: 0, mem: mem.clone() };
        let eax = layout::state_unka44(&mut host, &mut mem, call.args[0]);
        assert_eq!(eax, call.eax, "CBody::CState::Unk44 call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "CBody::CState::Unk44 inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBody::CState::Unk44 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBody::CState::Unk44: {} calls match", calls.len());
}

#[test]
fn body_vf40_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_VF40) else {
        eprintln!("skipping CBody::Vf40: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = BodyVf40ReplayHost { inner: &call.inner, next: 0, mem: &mut mem };
        let eax = layout::reset_state_and_base(&mut host, call.args[0]);
        assert_eq!(eax, call.eax, "CBody::Vf40 call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "CBody::Vf40 inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBody::Vf40 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBody::Vf40: {} calls match", calls.len());
}

#[test]
fn body_vf44_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_VF44) else {
        eprintln!("skipping CBody::Vf44: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = BodyVf44ReplayHost { inner: &call.inner, next: 0, mem: mem.clone() };
        let eax = layout::select_grid(&mut host, &mut mem, call.args[0], call.args[1]);
        assert_eq!(eax, call.eax, "CBody::Vf44 call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "CBody::Vf44 inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBody::Vf44 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBody::Vf44: {} calls match", calls.len());
}

#[test]
fn body_vfcc_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_VFCC) else {
        eprintln!("skipping CBody::VfCC: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        // The corners go to the caller's frame, which the recording does not map.
        mem.map(VFCC_SCRATCH, &[0; 32]);
        let mut host = BodyVfccReplayHost { inner: &call.inner, next: 0, mem: mem.clone() };
        let eax = layout::aabb_pretest(&mut host, &mut mem, call.args[0], call.args[1], call.args[2], VFCC_SCRATCH);
        assert_eq!(eax, call.eax, "CBody::VfCC call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "CBody::VfCC inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBody::VfCC call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBody::VfCC: {} calls match", calls.len());
}

#[test]
fn body_clear_grid_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_CLEAR_GRID) else {
        eprintln!("skipping CBody::ClearGrid: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        for inner in &call.inner {
            if inner.addr == 0x4f7090 {
                for window in &inner.windows {
                    mem.map(window.at, &window.before);
                }
            }
        }
        let mut host = StateReplayHost { inner: &call.inner, next: 0, mem: mem.clone() };
        let body_ptr = call.args[0];
        let grid = mem.u32(body_ptr + 0x64);
        let eax = layout::clear_grid(&mut host, &mem, body_ptr, call.args[1], call.args[2], call.args[3]);
        assert_eq!(eax, if grid == 0 { u32::MAX } else { call.eax }, "CBody::ClearGrid call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "CBody::ClearGrid inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBody::ClearGrid call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBody::ClearGrid: {} calls match", calls.len());
}

#[test]
fn body_ctor_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_CTOR) else {
        eprintln!("skipping CBody::CBody: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        for window in &call.after {
            mem.map(window.at, &vec![0; window.len]);
        }
        let mut host = BodyCtorReplayHost { inner: &call.inner, next: 0 };
        let eax =
            layout::ctor(&mut host, &mut mem, call.args[0], call.args[1], call.args[2], call.args[3], call.args[4]);
        assert_eq!(eax, call.eax, "CBody::CBody call {} EAX", call.index);
        assert_eq!(host.next, 1, "CEntity base constructor call count");
        assert_eq!(call.inner.len(), 2, "CBody member constructor calls");
        let state_call = &call.inner[1];
        assert_eq!(state_call.addr, BODY_STATE_CTOR, "CBody::CState constructor target");
        let name = if call.args[1] == 0 { 0 } else { mem.u32(call.args[1] + 4) };
        assert_eq!(
            state_call.args,
            [call.args[0] + 0x70, call.args[0], name, call.args[3]],
            "CBody::CState constructor arguments"
        );
        assert_eq!(state_call.eax, call.args[0] + 0x70, "CBody::CState constructor EAX");
        for window in &state_call.windows {
            if window.at == call.args[0] {
                // The enclosing CBody constructor clears m_grid at +0x64 after CState returns.
                assert_eq!(mem.read(window.at, 0x64), window.after[..0x64]);
            } else {
                assert_eq!(
                    mem.read(window.at, window.len),
                    window.after,
                    "CBody::CState constructor output at {:#x}",
                    window.at
                );
            }
        }
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBody::CBody call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBody::CBody: {} calls match", calls.len());
}

#[test]
fn body_link_notify_filtered_matches_original_calls() {
    let Some(calls) = recorded_calls(BODY_LINK_NOTIFY_FILTERED) else {
        eprintln!("skipping CBodyLink::NotifyFiltered: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = NotifyFilteredReplayHost { inner: &call.inner, next: 0 };
        links::notify_filtered(&mut mem, call.args[0], call.args[1], &mut host);
        assert_eq!(host.next, call.inner.len(), "visitor slot-1 call count");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBodyLink::NotifyFiltered call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBodyLink::NotifyFiltered: {} calls match", calls.len());
}

#[test]
fn cell_grid_ctor_matches_original_calls() {
    let Some(calls) = recorded_calls(CELL_GRID_CTOR) else {
        eprintln!("skipping CCellGrid::CCellGrid: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let allocation = call.after.first();
        if let Some(output) = allocation {
            mem.map(output.at, &vec![0; output.len]);
        }
        let width = f32::from_bits(call.args[3]);
        let height = f32::from_bits(call.args[4]);
        let columns = (rbr_physics::fpu::trunc_i32(width as f64) + 99) / 100;
        let rows = (rbr_physics::fpu::trunc_i32(height as f64) + 99) / 100;
        let bytes = rows * columns * 4 + 4;
        let mut host = CellGridCtorReplayHost {
            allocation: allocation.map_or(0, |window| window.at),
            bytes: bytes as u32,
            calls: 0,
        };
        let eax = layout::cell_grid_ctor(
            &mut host,
            &mut mem,
            call.args[0],
            f32::from_bits(call.args[1]),
            f32::from_bits(call.args[2]),
            width,
            height,
        );
        assert_eq!(eax, call.eax, "CCellGrid::CCellGrid call {} EAX", call.index);
        assert_eq!(host.calls, 1, "CCellGrid constructor allocation call");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CCellGrid::CCellGrid call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        if let Some(output) = allocation {
            assert_eq!(output.len, bytes as usize);
            assert_eq!(mem.read(output.at, output.len), output.bytes);
            assert_eq!(mem.u32(call.args[0] + 0x10), output.at + 4);
            assert_eq!(call.inner.len(), (rows * columns) as usize);
            for (index, inner) in call.inner.iter().enumerate() {
                let cell_ref = output.at + 4 + index as u32 * 4;
                assert_eq!(inner.addr, 0x4dffb0, "CCellRef ctor target");
                assert_eq!(inner.args, [cell_ref], "CCellRef ctor arguments");
                assert_eq!(inner.eax, cell_ref, "CCellRef ctor EAX");
                assert_eq!(inner.windows.len(), 1, "CCellRef ctor window");
                assert_eq!(inner.windows[0].at, cell_ref);
                assert_eq!(inner.windows[0].after, [0; 4]);
                assert_eq!(mem.read(cell_ref, 4), [0; 4]);
            }
        } else {
            assert_eq!(mem.u32(call.args[0] + 0x10), 0);
            assert!(call.inner.is_empty());
        }
    }
    eprintln!("CCellGrid::CCellGrid: {} calls match", calls.len());
}

#[test]
fn cell_grid_lookup_twin_matches_original_calls() {
    let Some(calls) = recorded_calls(CELL_GRID_LOOKUP_TWIN) else {
        eprintln!("skipping CCellGrid::LookupTwin: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let output = call.after.first().expect("missing CCellGrid block output");
        if !call.windows.iter().any(|window| window.at == output.at && window.len == output.len) {
            mem.map(output.at, &vec![0; output.len]);
        }
        let mut host = CellGridReplayHost { allocation: output.at, calls: 0 };
        let eax = layout::lookup_twin(
            &mut host,
            &mut mem,
            call.args[0],
            f32::from_bits(call.args[1]),
            f32::from_bits(call.args[2]),
        );
        assert_eq!(eax, call.eax, "CCellGrid::LookupTwin call {} EAX", call.index);
        assert_eq!(host.calls, usize::from(!call.inner.is_empty()));
        assert!(call.inner.is_empty() || call.inner.len() == 100);
        for (index, inner) in call.inner.iter().enumerate() {
            let address = output.at + 4 + index as u32 * 0x10;
            assert_eq!(inner.addr, CELL_LINK_CTOR, "CCellLink ctor target");
            assert_eq!(inner.args, [address], "CCellLink ctor args");
            assert_eq!(inner.eax, address, "CCellLink ctor EAX");
            for window in &inner.windows {
                assert_eq!(mem.read(window.at, window.len), window.after);
            }
        }
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CCellGrid::LookupTwin call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        for window in &call.after {
            assert_eq!(
                mem.read(window.at, window.len),
                window.bytes,
                "CCellGrid::LookupTwin call {} allocation at {:#x}",
                call.index,
                window.at
            );
        }
    }
    eprintln!("CCellGrid::LookupTwin: {} calls match", calls.len());
}

#[test]
fn select_table_matches_original_calls() {
    replay(E36_SELECT_TABLE, "E36SelectTable", u32::MAX, |mem, args| layout::select_table(mem, args[0]));
}

#[test]
fn brake_wall_queries_match_original_calls() {
    replay(BRAKE_WALL_VF88, "CBrakeWall::Vf88", u32::MAX, |_mem, _args| wall::is_available());
    replay(BRAKE_WALL_VF110, "CBrakeWall::Vf110", u32::MAX, |_mem, _args| wall::is_active());

    let Some(calls) = recorded_calls(BRAKE_WALL_VF11C) else {
        eprintln!("skipping CBrakeWall::Vf11C: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        wall::copy_root_bounds(&mut mem, call.args[0], call.args[1], call.args[2]);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBrakeWall::Vf11C call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBrakeWall::Vf11C: {} calls match", calls.len());
}

#[test]
fn brake_wall_quadtree_queries_match_original_calls() {
    replay(BRAKE_WALL_QUERY_POINT, "CBrakeWall::QueryPoint", u32::MAX, |mem, args| {
        wall::query_point(mem, args[0], args[1], args[2])
    });
    replay(BRAKE_WALL_QUERY_BOX, "CBrakeWall::QueryBox", u32::MAX, |mem, args| {
        wall::query_box(mem, args[0], args[1], args[2])
    });
}

#[test]
fn brake_wall_vf15c_matches_original_calls() {
    let Some(calls) = recorded_calls(BRAKE_WALL_VF15C) else {
        eprintln!("skipping CBrakeWall::Vf15C: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let eax = wall::query_xy_box(&mem, call.args[0], call.args[1]);
        assert_eq!(eax, call.eax, "CBrakeWall::Vf15C call {} EAX", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBrakeWall::Vf15C call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        assert_eq!(call.inner.len(), 1, "CBrakeWall::Vf15C QueryBox call count");
        let inner = &call.inner[0];
        assert_eq!(inner.addr, BRAKE_WALL_QUERY_BOX, "CBrakeWall::Vf15C QueryBox target");
        assert_eq!(inner.args[0], call.args[0], "CBrakeWall::Vf15C QueryBox wall");
        assert_eq!(inner.eax, call.eax, "CBrakeWall::Vf15C QueryBox result");
        let shape = inner.windows.iter().find(|window| window.index == 1).expect("missing Vf15C query bounds");
        assert_eq!(&shape.before[..8], &mem.read(call.args[1], 8), "CBrakeWall::Vf15C center");
        assert_eq!(
            &shape.before[8..16],
            &[0x00, 0x00, 0x20, 0x41, 0x00, 0x00, 0x20, 0x41],
            "CBrakeWall::Vf15C ten-unit extents"
        );

        let mut inner_mem = Mem::new();
        for window in &inner.windows {
            inner_mem.map(window.at, &window.before);
        }
        let inner_eax = wall::query_box(&mut inner_mem, inner.args[0], inner.args[1], inner.args[2]);
        assert_eq!(inner_eax, inner.eax, "nested QueryBox EAX");
        for window in &inner.windows {
            assert_eq!(inner_mem.read(window.at, window.len), window.after);
        }
    }
    eprintln!("CBrakeWall::Vf15C: {} calls match", calls.len());
}

/// The fuzzed direct calls (`harness/vector_specs/collision.py`, counts 0 to 40). With no points eax keeps the caller's
/// upper bytes, which a direct call does not set, so only its low byte is compared.
#[test]
fn grid_add_edge_matches_original_calls() {
    let Some(calls) = recorded_calls(GRID_ADD_EDGE) else {
        eprintln!("skipping E37GridOps::AddEdge: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let moved = layout::grid_add_edge(&mut mem, call.args[0], call.args[1]);
        match moved {
            0 => assert_eq!(call.eax & 0xff, 0, "E37GridOps::AddEdge call {} EAX", call.index),
            n => assert_eq!(call.eax, call.args[0] + 0x48 + 16 * n, "E37GridOps::AddEdge call {} EAX", call.index),
        }
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E37GridOps::AddEdge call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("E37GridOps::AddEdge: {} calls match", calls.len());
}

#[test]
fn brake_wall_add_count_matches_original_calls() {
    let Some(calls) = recorded_calls(BRAKE_WALL_ADD_COUNT) else {
        eprintln!("skipping CBrakeWall::AddCount: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        wall::add_count(&mut mem, call.args[0], call.args[1], call.args[2]);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "CBrakeWall::AddCount call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("CBrakeWall::AddCount: {} calls match", calls.len());
}

#[test]
fn e36_sphere_box_matches_original_calls() {
    let Some(calls) = recorded_calls(E36_SPHERE_BOX) else {
        eprintln!("skipping E36SphereBox: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let eax = layout::sphere_box(&mut mem, call.args[0], call.args[1], call.args[2]);
        assert_eq!(eax, call.eax, "E36SphereBox call {} EAX", call.index);
        for window in &call.windows {
            let actual = mem.read(window.at, window.len);
            if actual != window.after {
                let offset = actual.iter().zip(&window.after).position(|(got, expected)| got != expected).unwrap();
                panic!(
                    "E36SphereBox call {} window {} at {:#x}: first mismatch +{:#x}, got {:?}, expected {:?}",
                    call.index,
                    window.index,
                    window.at,
                    offset,
                    &actual[offset.saturating_sub(4)..(offset + 12).min(actual.len())],
                    &window.after[offset.saturating_sub(4)..(offset + 12).min(actual.len())]
                );
            }
        }
    }
    eprintln!("E36SphereBox: {} calls match", calls.len());
}

#[test]
fn e36_obb_test_matches_original_calls() {
    let Some(calls) = recorded_calls(E36_OBB_TEST) else {
        eprintln!("skipping E36ObbTest: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let eax = layout::obb_test(&mut mem, call.args[0], call.args[1], call.args[2]);
        assert_eq!(eax, call.eax, "E36ObbTest call {} EAX", call.index);
        for window in &call.windows {
            let actual = mem.read(window.at, window.len);
            if actual != window.after {
                let offset = actual.iter().zip(&window.after).position(|(got, expected)| got != expected).unwrap();
                panic!(
                    "E36ObbTest call {} window {} at {:#x}: first mismatch +{:#x}, got {:?}, expected {:?}",
                    call.index,
                    window.index,
                    window.at,
                    offset,
                    &actual[offset.saturating_sub(4)..(offset + 12).min(actual.len())],
                    &window.after[offset.saturating_sub(4)..(offset + 12).min(window.after.len())]
                );
            }
        }
    }
    eprintln!("E36ObbTest: {} calls match", calls.len());
}

#[test]
fn e36_accessors_and_init_match_original_calls() {
    replay(E36_GET_680, "E36Get680", u32::MAX, |mem, args| e36::active_shape(mem, args[0]));
    replay(E36_GET_68A, "E36Get68a", u32::MAX, |mem, args| e36::flag_bit2(mem, args[0]));
    replay(E36_COLLIDE_INIT, "E36CollideInit", u32::MAX, |mem, args| e36::collide_init(mem, args[0]));
}

#[test]
fn e36_alloc_event_matches_original_calls() {
    let Some(calls) = recorded_calls(E36_ALLOC_EVENT) else {
        eprintln!("skipping E36AllocEvent: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut branches = [0usize; 3];
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let branch = match call.branch.as_str() {
            "new" => &mut branches[0],
            "pop" => &mut branches[1],
            "detached" => &mut branches[2],
            other => panic!("unexpected E36AllocEvent branch {other}"),
        };
        *branch += 1;

        let mut host = AllocEventReplayHost { inner: &call.inner, next: 0 };
        let eax = event::alloc_event(&mut mem, call.args[0], &mut host);
        assert_eq!(eax, call.eax, "E36AllocEvent call {} EAX", call.index);

        match call.branch.as_str() {
            "new" => {
                assert_eq!(call.inner.len(), 1, "operator new call count");
                assert_eq!(call.inner[0].addr, 0x70bec2, "operator new target");
                assert_eq!(call.inner[0].args, [call.args[0]], "operator new size");
                assert_eq!(host.next, 1, "operator new host calls");
            }
            "pop" => {
                assert_eq!(call.inner.len(), 1, "E37Unlink call count");
                assert_eq!(call.inner[0].addr, E37_UNLINK, "E37Unlink target");
                assert_eq!(call.inner[0].args, [call.eax + 0x0c], "E37Unlink link");
                assert_eq!(host.next, 0, "unexpected allocator host call");
            }
            "detached" => {
                assert!(call.inner.is_empty(), "detached head has no nested calls");
                assert_eq!(host.next, 0, "unexpected allocator host call");
            }
            _ => unreachable!(),
        }

        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E36AllocEvent call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    assert!(branches.iter().all(|count| *count != 0), "missing branch coverage: {branches:?}");
    eprintln!("E36AllocEvent: {} calls match ({branches:?})", calls.len());
}

#[test]
fn e36_collect_matches_original_calls() {
    let Some(calls) = recorded_calls(E36_COLLECT) else {
        eprintln!("skipping E36Collect: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        for inner in call.inner.iter().filter(|inner| inner.addr == E45_CELL_QUERY) {
            for window in inner.windows.iter().filter(|window| window.index >= 3) {
                mem.map(window.at, &window.before);
            }
        }
        let mut host = CollectReplayHost { inner: &call.inner, used: vec![false; call.inner.len()] };
        let eax =
            e36::collect(&mut mem, COLLECT_FRAME, call.args[0], call.args[1], call.args[2], call.args[3], &mut host);
        host.assert_consumed();
        assert_eq!(eax, call.eax, "E36Collect call {} EAX", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E36Collect call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("E36Collect: {} calls match", calls.len());
}

#[test]
fn e36_nop_matches_original_calls() {
    let Some(calls) = recorded_calls(E36_NOP) else {
        eprintln!("skipping E36Nop: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        assert!(call.args.is_empty(), "E36Nop arguments");
        assert!(call.windows.is_empty(), "E36Nop memory windows");
        assert!(call.inner.is_empty(), "E36Nop nested calls");
        e36::nop();
    }
    eprintln!("E36Nop: {} no-op calls matched", calls.len());
}

#[test]
fn e37_test_4b20_matches_original_calls() {
    replay(E37_TEST_4B20, "E37Test4B20", u32::MAX, |mem, args| u32::from(event::is_suppressed(mem, args[0])));
}

#[test]
fn e37_refresh_4a20_matches_original_calls() {
    let Some(calls) = recorded_calls(E37_REFRESH_4A20) else {
        eprintln!("skipping E37Refresh4A20: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut branches = [0usize; 4];
    for call in &calls {
        assert_eq!(call.args.len(), 1, "E37Refresh4A20 arguments");
        assert!(call.inner.is_empty(), "E37Refresh4A20 external calls");
        let branch = match call.branch.as_str() {
            "none" => &mut branches[0],
            "first" => &mut branches[1],
            "second" => &mut branches[2],
            "both" => &mut branches[3],
            other => panic!("unexpected E37Refresh4A20 branch {other}"),
        };
        *branch += 1;

        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        event::refresh_materials(&mut mem, call.args[0]);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E37Refresh4A20 call {} branch {} window {} at {:#x}",
                call.index,
                call.branch,
                window.index,
                window.at
            );
        }
    }
    assert!(branches.iter().all(|&count| count > 0), "branch coverage {branches:?}");
    eprintln!("E37Refresh4A20: {} calls match ({branches:?})", calls.len());
}

#[test]
fn e37_sort_scalar_helpers_match_original_calls() {
    replay_st0(COLLISION_EVENT_GET_94, "CCollisionEvent::Get94", |mem, args| sort::event_get_94(mem, args[0]));
    replay(E37_GET_F7F0, "E37GetF7F0", u32::MAX, |mem, _args| sort::live_event_count(mem));
    replay(E37_ZERO_4090, "E37Zero4090", u32::MAX, |_mem, _args| sort::zero_4090());
    replay(E37_NOP_40A0, "E37Nop40A0", 0, |_mem, _args| {
        sort::nop_40a0();
        0
    });
    replay(E37_ZERO_40B0, "E37Zero40B0", u32::MAX, |_mem, _args| sort::zero_40b0());
}

#[test]
fn e37_sort_constant_getters_match_original_calls() {
    replay_st0(E37_GET_795E14A, "E37Get795E14a", |_mem, _args| sort::get_795e14a());
    replay_st0(E37_GET_795E14B, "E37Get795E14b", |_mem, _args| sort::get_795e14b());
    replay_st0(E37_GET_72AFCC, "E37Get72AFCC", |_mem, _args| sort::get_72afcc());
}

#[test]
fn e37_sort_comparators_match_original_calls() {
    replay(E37_CMP_INT, "E37CmpInt", u32::MAX, |mem, args| sort::cmp_int(mem, args[0], args[1]));
    replay(E37_CMP_FLOAT, "E37CmpFloat", u32::MAX, |mem, args| sort::cmp_float(mem, args[0], args[1]));
}

#[test]
fn e37_find_int_matches_original_calls() {
    replay(E37_FIND_INT, "E37FindInt", u32::MAX, |mem, args| sort::find_int(mem, args[0], args[1], args[2]));
}

#[test]
fn e37_init_70_matches_original_calls() {
    replay(E37_INIT_70, "E37Init70Obj::Init70", u32::MAX, |mem, args| sort::init_70(mem, args[0], args[1]));
}

#[test]
fn e37_plane_scale_matches_original_calls() {
    replay(E37_PLANE_SCALE, "E37EvOps::PlaneScale", u32::MAX, |mem, args| event::plane_scale(mem, args[0], args[1]));
}

#[test]
fn e37_element_accessors_match_original_calls() {
    replay(E37_ELEM_T68A, "E37GridOps::T68A", u32::MAX, |mem, args| elem::t68a(mem, args[0]));
    replay(E37_ELEM_AT_E8, "E37GridOps::AtE8", u32::MAX, |_mem, args| elem::at_e8(args[0], args[1] as i32));
    replay(E37_ELEM_AT_E10, "E37GridOps::AtE10", u32::MAX, |_mem, args| elem::at_e10(args[0], args[1] as i32));
    replay(E37_ELEM_IDX7, "E37GridOps::Idx7", u32::MAX, |mem, args| elem::idx7(mem, args[0], args[1] as i32));
    replay(E37_ELEM_GET_684, "E37GridOps::Get684", u32::MAX, |mem, args| elem::get_684(mem, args[0]));
    replay(E37_ELEM_SET_68A, "E37GridOps::Set68A", u32::MAX, |mem, args| elem::set_68a(mem, args[0], args[1]));
    replay(E37_ELEM_T158_2, "E37GridOps::T158_2", u32::MAX, |mem, args| elem::t158_2(mem, args[0]));
    replay(E37_ELEM_T158_4, "E37GridOps::T158_4", u32::MAX, |mem, args| elem::t158_4(mem, args[0]));
    replay(E37_ELEM_GET_30, "E37GridOps::Get30", 0xff, |mem, args| elem::get_30(mem, args[0]));
}

#[test]
fn e37_element_header_fields_match_original_calls() {
    replay(E37_ELEM_GET_04, "E37GridOps::Get04", u32::MAX, |mem, args| elem::get_04(mem, args[0]));
    replay(E37_ELEM_SET_04, "E37GridOps::Set04", 0, |mem, args| {
        elem::set_04(mem, args[0], args[1]);
        0
    });
    replay(E37_ELEM_GET_00, "E37GridOps::Get00", u32::MAX, |mem, args| elem::get_00(mem, args[0]));
    replay(E37_ELEM_T04_2, "E37GridOps::T04_2", u32::MAX, |mem, args| elem::t04_2(mem, args[0]));
    replay(E37_ELEM_VALID_5990, "E37GridOps::Valid5990", u32::MAX, |mem, args| elem::valid_5990(mem, args[0]));
    replay(E37_ELEM_GET_08, "E37GridOps::Get08", u32::MAX, |mem, args| elem::get_08(mem, args[0]));
    replay(E37_ELEM_GET_0C_W, "E37GridOps::Get0Cw", 0xffff, |mem, args| elem::get_0c_w(mem, args[0]));
    replay(E37_ELEM_GET_10, "E37GridOps::Get10", u32::MAX, |mem, args| elem::get_10(mem, args[0]));
    replay(E37_ELEM_GET_0E_W, "E37GridOps::Get0Ew", 0xffff, |mem, args| elem::get_0e_w(mem, args[0]));
    replay(E37_ELEM_GET_14, "E37GridOps::Get14", u32::MAX, |mem, args| elem::get_14(mem, args[0]));
    replay(E37_ELEM_GET_18, "E37GridOps::Get18", u32::MAX, |mem, args| elem::get_18(mem, args[0]));
    replay(E37_ELEM_GET_1C, "E37GridOps::Get1C", u32::MAX, |mem, args| elem::get_1c(mem, args[0]));
    replay(E37_ELEM_GET_20_W, "E37GridOps::Get20w", 0xffff, |mem, args| elem::get_20_w(mem, args[0]));
    replay(E37_ELEM_GET_22_W, "E37GridOps::Get22w", 0xffff, |mem, args| elem::get_22_w(mem, args[0]));
    replay(E37_ELEM_GET_24, "E37GridOps::Get24", u32::MAX, |mem, args| elem::get_24(mem, args[0]));
    replay(E37_ELEM_GET_28, "E37GridOps::Get28", u32::MAX, |mem, args| elem::get_28(mem, args[0]));
    replay(E37_ELEM_GET_2C, "E37GridOps::Get2C", u32::MAX, |mem, args| elem::get_2c(mem, args[0]));
}

#[test]
fn e37_element_cell_global_accessors_match_original_calls() {
    replay(E37_ZERO_5A70, "E37Zero5A70", 0, |mem, _args| {
        elem::zero_5a70(mem);
        0
    });
    replay(E37_GET_5A80, "E37Get5A80", u32::MAX, |mem, _args| elem::get_5a80(mem));
}

#[test]
fn e37_nop_5cf0_matches_original_calls() {
    replay(E37_NOP_5CF0, "E37Nop5CF0", 0, |_mem, _args| {
        elem::nop_5cf0();
        0
    });
}

#[test]
fn e37_compose_9_matches_original_calls() {
    replay(E37_COMPOSE_9, "E37Compose9", 0, |mem, args| {
        let terms = elem::triangle_plane_terms(
            Vec3::load(mem, args[0]),
            Vec3::load(mem, args[1]),
            Vec3::load(mem, args[2]),
            Vec3::load(mem, args[3]),
        );
        mem.set_f32(args[5], terms.dot_p4_n1);
        mem.set_f32(args[6], terms.dot_p4_n2);
        mem.set_f32(args[7], terms.dot_p4_n3);
        mem.set_f32(args[8], terms.dot_p3_n1);
        terms.cross_sum.store(mem, args[4]);
        0
    });
}

#[test]
fn e37_tri_5a90_matches_original_calls() {
    replay(E37_TRI_5A90, "E37Tri5A90", 0xff, |mem, args| {
        let tri = elem::segment_triangle(
            Vec3::load(mem, args[0]),
            Vec3::load(mem, args[1]),
            Vec3::load(mem, args[2]),
            Vec3::load(mem, args[3]),
            Vec3::load(mem, args[4]),
        );
        layout::store_segment_triangle(mem, args[5], &tri);
        u32::from(tri.hit)
    });
}

#[test]
fn e37_run_56c0_matches_original_calls() {
    replay(E37_RUN_56C0, "E37Run56C0", u32::MAX, |mem, args| {
        let distance = elem::ray_triangle_distance(
            Vec3::load(mem, args[0]),
            Vec3::load(mem, args[1]),
            Vec3::load(mem, args[2]),
            Vec3::load(mem, args[3]),
            Vec3::load(mem, args[4]),
        );
        match distance {
            Some(distance) => {
                mem.set_f32(args[5], distance);
                1
            }
            None => 0,
        }
    });
}

#[test]
fn e38_mesh_ac50_matches_original_calls() {
    replay(E38_MESH_AC50, "E38Ops::M_AC50", u32::MAX, |mem, args| {
        let base = layout::mesh_vertices(mem, args[0]);
        let hit = e38::mesh_ray_test(
            Vec3::load(mem, args[1]),
            Vec3::load(mem, args[2]),
            |index| Vec3::load(mem, base.wrapping_add(index as u32 * 16)),
            |index| layout::mesh_tag(mem, args[0], index),
        );
        match hit {
            Some(hit) => {
                mem.set_f32(args[4], hit.distance);
                layout::store_mesh_hit_tags(mem, args[3], &hit);
                1
            }
            None => 0,
        }
    });
}

#[test]
fn e38_clear_mesh_scratch_matches_original_calls() {
    replay(E38_CLEAR_MESH_SCRATCH, "E38_004FAD10", 0, |mem, args| {
        layout::store_zero_mesh_scratch(mem, args[0]);
        0
    });
}

#[test]
fn e38_solver_def_unk20_matches_original_calls() {
    replay(E38_SOLVER_DEF_UNK20, "CCollisionSolver::CDef::Unk20", u32::MAX, |_mem, _args| e38::shared_solver_table());
}

#[test]
fn e38_solver_material_tag_matches_original_calls() {
    replay_solver_material_tag();
}

#[test]
fn e37_element_5f20_matches_original_calls() {
    replay_st0(E37_ELEM_5F20, "E37GridOps::Elem5F20", |mem, args| {
        elem::elem_5f20(mem, args[0], args[1], args[2], args[3])
    });
}

#[test]
fn e38_vector_leaves_match_original_calls() {
    replay(E38_VEC_SCALE, "E38Vec::Scale", u32::MAX, |mem, args| {
        let scaled = Vec3::load(mem, args[0]).scale(f32::from_bits(args[1]));
        scaled.store(mem, args[0]);
        args[0]
    });
    replay(E38_VEC_CCB0, "E38Vec::M_CCB0", u32::MAX, |mem, args| {
        layout::vec3_add_sub(mem, args[0], args[1], args[2], args[3])
    });
    replay(E38_VEC_CD10, "E38Vec::M_CD10", u32::MAX, |mem, args| layout::vec3_sub(mem, args[0], args[1], args[2]));
    replay(E38_VEC_CDD0, "E38Vec::M_CDD0", u32::MAX, |mem, args| layout::vec3_add(mem, args[0], args[1], args[2]));
    replay_st0_writes(E38_VEC_CD60, "E38Vec::M_CD60", |mem, args| {
        layout::vec3_sub_dot(mem, args[0], args[1], args[2], args[3])
    });
    replay_st0(E38_VEC_CEE0, "E38Vec::M_CEE0", |mem, args| layout::vec3_dot(mem, args[0], args[1]));
    replay(E38_VEC_CF30, "E38Vec::M_CF30", u32::MAX, |mem, args| layout::vec3_cross(mem, args[0], args[1], args[2]));
    replay_st0_writes(E38_VEC_CFA0, "E38Vec::M_CFA0", |mem, args| {
        layout::vec3_cross_dot(mem, args[0], args[1], args[2], args[3])
    });
    replay(E38_VEC_D030, "E38Vec::M_D030", u32::MAX, |mem, args| {
        layout::vec3_scale(mem, args[0], f32::from_bits(args[1]), args[2])
    });
    replay(E38_NORMALIZE_D0F0, "E38_004FD0F0", u32::MAX, |mem, args| layout::vec3_normalize(mem, args[0]));
    replay(E38_SAT_D230, "E38Ops::M_D230", u32::MAX, |mem, args| {
        layout::world_axes_overlap(mem, args[0], args[1], args[2])
    });
    replay(E38_SAT_D2C0, "E38Ops::M_D2C0", u32::MAX, |mem, args| {
        layout::box_sat_overlap(mem, args[0], args[1], args[2])
    });
    replay(E38_RAY_TRIANGLE_D480, "E38Ops::M_D480", u32::MAX, |mem, args| {
        layout::ray_triangle(mem, args[1], args[2], args[3], args[4], args[5], args[6])
    });
}

#[test]
fn e37_copy_80_matches_original_calls() {
    replay(E37_COPY_80, "E37EvOps::Copy80", 0, |mem, args| {
        event::copy_80(mem, args[0], args[1]);
        0
    });
}

#[test]
fn e37_cmp_15_matches_original_calls() {
    replay(E37_CMP_15, "E37EvOps::Cmp15", u32::MAX, |mem, args| event::cmp_15(mem, args[0], args[1]));
}

#[test]
fn e37_list_primitives_match_original_calls() {
    replay(E37_ZERO_3, "E37Zero3", 0, |mem, args| {
        list::zero_3(mem, args[0]);
        0
    });
    replay(E37_FIRST, "E37First", u32::MAX, |mem, args| list::first(mem, args[0]));
    replay(E37_FIRST_TWIN, "E37FirstTwin", u32::MAX, |mem, args| list::first_twin(mem, args[0]));
    replay(E37_AT_8, "E37At8", u32::MAX, |mem, args| list::at_8(mem, args[0]));
    replay(E37_ZERO_4, "E37Zero4", 0, |mem, args| {
        list::zero_4(mem, args[0]);
        0
    });
}

#[test]
fn e37_list_insert_matches_original_calls() {
    replay(E37_LIST_INSERT, "E37ListOps::Insert", 0, |mem, args| {
        list::insert(mem, args[0], args[1]);
        0
    });
}

#[test]
fn e37_unlink_matches_original_calls() {
    replay(E37_UNLINK, "E37Unlink", 0, |mem, args| {
        list::unlink(mem, args[0]);
        0
    });
}

#[test]
fn e37_rate_4c70_matches_original_calls() {
    replay_st0(E37_RATE_4C70, "E37Rate4C70", |mem, args| list::link_rate(mem, args[0]));
}

#[test]
fn e37_release_owner_twins_match_original_calls() {
    replay(E37_RELEASE_4F50, "E37Release4F50", 0, |mem, args| {
        list::release_4f50(mem, args[0]);
        0
    });
    replay(E37_RELEASE_4F60, "E37Release4F60", 0, |mem, args| {
        list::release_4f60(mem, args[0]);
        0
    });
}

#[test]
fn e37_list_release_matches_original_calls() {
    replay(E37_LIST_RELEASE, "E37ListOps::Release", 0, |mem, args| {
        list::release(mem, args[0], args[1]);
        0
    });
}

#[test]
fn e37_list_set_1c_matches_original_calls() {
    replay(E37_LIST_SET_1C, "E37ListOps::Set1C", 0, |mem, args| {
        list::set_1c(mem, args[0], args[1]);
        0
    });
}

#[test]
fn e37_vec16_minmax_match_original_calls() {
    replay(E37_VEC16_MIN, "E37Vec16::Min", u32::MAX, |mem, args| layout::vec16_min(mem, args[0], args[1]));
    replay(E37_VEC16_MAX, "E37Vec16::Max", u32::MAX, |mem, args| layout::vec16_max(mem, args[0], args[1]));
}

#[test]
fn e37_sign_vector_predicates_match_original_calls() {
    replay(E37_ALL_NON_NEG, "E37AllNonNeg", u32::MAX, |mem, args| list::all_non_neg(mem, args[0]));
    replay(E37_ALL_NON_POS, "E37AllNonPos", u32::MAX, |mem, args| list::all_non_pos(mem, args[0]));
}

#[test]
fn e37_vec_set_store_5630_matches_original_calls() {
    replay(E37_STORE_5630, "E37VecSet::Store5630", u32::MAX, |mem, args| {
        let cross =
            vec_set::cross_from_origin(Vec3::load(mem, args[0]), Vec3::load(mem, args[1]), Vec3::load(mem, args[2]));
        cross.store(mem, args[3]);
        args[3]
    });
}

#[test]
fn e37_vec_set_dot_x_matches_original_calls() {
    replay_st0(E37_DOT_X, "E37VecSet::DotX", |mem, args| {
        vec_set::cross_then_dot(Vec3::load(mem, args[0]), Vec3::load(mem, args[1]), Vec3::load(mem, args[2]))
    });
}

#[test]
fn e37_push_front_matches_original_calls() {
    replay(E37_PUSH_FRONT, "E37PushFront", 0, |mem, args| {
        list::push_front(mem, args[0]);
        0
    });
}

#[test]
fn e37_event_dtor_body_matches_original_calls() {
    replay(E37_EVENT_DTOR_BODY, "CCollisionEvent::DtorBody", 0, |mem, args| {
        event::dtor_body(mem, args[0]);
        0
    });
}

#[test]
fn e37_event_vf1cx_matches_original_calls() {
    replay(E37_EVENT_VF1CX, "CCollisionEvent::Vf1Cx", u32::MAX, |mem, args| event::vf1cx(mem, args[0], args[1]));
}

#[test]
fn e37_list_drain_matches_original_calls() {
    replay(E37_LIST_DRAIN, "E37ListOps::Drain", 0, |mem, args| {
        list::drain(mem, args[0], args[1]);
        0
    });
}

#[test]
fn e37_drain_wrappers_match_original_calls() {
    replay(E37_COLLISION_LIST_VF08X, "CCollisionList::Vf08x", 0, |mem, args| {
        list::collision_list_vf08x(mem, args[0]);
        0
    });
    replay(E37_DRAIN_4F70, "E37Drain4F70", 0, |mem, args| {
        list::drain_by_flag(mem, args[0]);
        0
    });
}

#[test]
fn ray_object_matches_original_calls() {
    let Some(calls) = recorded_calls(RAY_SEEDS) else {
        eprintln!("skipping E38RaySeeds: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = RayReplayHost { inner: &call.inner, next: 0, mem: mem.clone() };
        let eax = layout::ray_seeds(&mut host, &mut mem, call.args[0]);
        assert_eq!(eax, call.eax, "E38RaySeeds call {} EAX", call.index);
        assert_eq!(host.next, call.inner.len(), "E38RaySeeds inner calls");
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E38RaySeeds call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    eprintln!("E38RaySeeds: {} calls match", calls.len());

    // The original leaves eax as the caller had it; the recorded callers' eax is not modelled, so it is not compared.
    replay(RAY_CLEAR, "E38RayClear", 0, |mem, args| {
        layout::ray_clear(mem, args[0]);
        0
    });
}

/// The host of CColGrid::SyncGrid's two callees. `copy_transform` is the identity transform (the worker is the identity
/// object, which the test does not model further), and `copy_vec4` copies a vector.
struct GridSyncTestHost;

impl layout::GridSyncHost for GridSyncTestHost {
    fn copy_transform(&mut self, mem: &mut Mem, _a: u32, edges: u32, worker: u32, out: u32, count: u32) {
        assert_eq!(worker, GRID_SYNC_WORKER, "SyncGrid passes the identity worker");
        let bytes = mem.read(edges, 16 * count as usize);
        mem.map(out, &bytes);
    }

    fn copy_vec4(&mut self, mem: &mut Mem, dst: u32, src: u32) {
        let bytes = mem.read(src, 16);
        mem.map(dst, &bytes);
    }
}

const GRID_SYNC_WORKER: u32 = 0x7000_0100;

fn vec_lanes(mem: &Mem, at: u32) -> [f32; 4] {
    std::array::from_fn(|i| mem.f32(at + 4 * i as u32))
}

#[test]
fn grid_sync_caller_logic() {
    // The caller's logic only: rust_step checks the real M_B460 under the binding (its walker is a host call).
    let (grid, a, b, c, scratch) = (0x1000_0000, 0x1000_3000, 0x1000_1000, 0x1000_1010, 0x1000_2000);
    let vectors: [[f32; 4]; 3] = [[1.0, -2.0, 3.0, 0.0], [-4.0, 5.0, 0.5, 9.0], [2.0, 0.0, -1.0, 7.0]];
    let mut mem = shared_mem();
    mem.map(layout::IDENTITY_WORKER_SLOT, &GRID_SYNC_WORKER.to_le_bytes());
    let edges: Vec<u8> = vectors.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
    mem.map(grid + 0x40, &edges);
    mem.map(grid + 0x68d, &[3]);
    mem.map(b, &[0; 16]);
    mem.map(c, &[0; 16]);
    mem.map(scratch, &[0; 48]);
    let mut host = GridSyncTestHost;
    assert_eq!(layout::sync_grid(&mut host, &mut mem, grid, a, b, c, scratch), 1);
    assert_eq!(vec_lanes(&mem, b), [-4.0, -2.0, -1.0, 0.0], "SyncGrid b: lanewise minimum");
    assert_eq!(vec_lanes(&mem, c), [2.0, 5.0, 3.0, 9.0], "SyncGrid c: lanewise maximum");

    // No edges: nothing is written and the result is 0.
    mem.map(grid + 0x68d, &[0]);
    mem.map(b, &[0; 16]);
    let mut host = GridSyncTestHost;
    assert_eq!(layout::sync_grid(&mut host, &mut mem, grid, a, b, c, scratch), 0);
    assert_eq!(vec_lanes(&mem, b), [0.0; 4], "SyncGrid b untouched without edges");
}

/// The host of M_D760X's table lookup: a fixed surface byte.
struct SampleTestHost;

impl e38::SampleHost for SampleTestHost {
    fn sample(&mut self, _byte: u32, _low: f32, _high: f32) -> u32 {
        0x2a
    }
}

fn map_floats(mem: &mut Mem, at: u32, values: &[f32]) {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    mem.map(at, &bytes);
}

/// One grid with one record (centre (0.25, 0.25, 0), half 2) and one face (vertices (0,0,0), (1,0,0), (0,1,0) at z = 0,
/// flags set in the two 16-bit words). The ray runs from `start_z` to `end_z` at x = y = 0.25.
fn grid_ray_scene(mem: &mut Mem, start_z: f32, end_z: f32) -> (u32, u32, u32, u32) {
    let (grid, start, end, out): (u32, u32, u32, u32) = (0x1000_0000, 0x1000_6000, 0x1000_6010, 0x1000_6100);
    let (list, record, face, base): (u32, u32, u32, u32) = (0x1000_1014, 0x1000_3000, 0x1000_4000, 0x1000_5000);
    map_floats(mem, start, &[0.25, 0.25, start_z]);
    map_floats(mem, end, &[0.25, 0.25, end_z]);
    mem.map(out, &[0; 0x40]);
    mem.map(list, &1u32.to_le_bytes());
    mem.map(list + 4, &record.to_le_bytes());
    map_floats(mem, record, &[0.25, 0.25, 0.0, 2.0, 2.0, 2.0]);
    mem.map(record + 0x18, &((1u32 << 22) | 1).to_le_bytes());
    mem.map(record + 0x1c, &face.to_le_bytes());
    mem.map(grid + 0x10, &base.to_le_bytes());
    map_floats(mem, base, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let mut face_bytes = [0u8; 18];
    face_bytes[2] = 1;
    face_bytes[4] = 2;
    face_bytes[6..8].copy_from_slice(&0x8000u16.to_le_bytes());
    face_bytes[8..10].copy_from_slice(&0x8000u16.to_le_bytes());
    mem.map(face, &face_bytes);
    (grid, start, end, out)
}

#[test]
fn grid_ray_hits_nearest_face() {
    // The ray from z = -1 to z = 1 meets the face at z = 0, halfway along (ray parameter 0.5).
    let mut mem = shared_mem();
    let (grid, start, end, out) = grid_ray_scene(&mut mem, -1.0, 1.0);
    let mut host = SampleTestHost;
    assert_eq!(layout::mesh_ray_query(&mut host, &mut mem, grid, start, end, out, 0), 1);
    assert_eq!(
        [mem.f32(out), mem.f32(out + 4), mem.f32(out + 8)],
        [0.25, 0.25, 0.0],
        "M_D760X position: start + direction * t"
    );
    // The normal is normalised with `rsqrtss` (about 12 bits), so it is close to (0, 0, -1), not exactly.
    let normal = [mem.f32(out + 0x10), mem.f32(out + 0x14), mem.f32(out + 0x18)];
    assert!(normal[0] == 0.0 && normal[1] == 0.0, "M_D760X normal: x and y");
    assert!((normal[2] + 1.0).abs() < 1e-3, "M_D760X normal: z near -1");
    assert_eq!(mem.f32(out + 0x30), 0.5, "M_D760X t");
    assert_eq!(mem.u32(out + 0x28), 0x2a, "M_D760X surface byte, low sample");
    assert_eq!(mem.u32(out + 0x2c), 0x2a, "M_D760X surface byte, high sample");
    assert_eq!(mem.u32(out + 0x34), 1, "M_D760X flag word +6 bit 15");
    assert_eq!(mem.u32(out + 0x38), 1, "M_D760X flag word +8 bit 15");

    // The hit at parameter 1.5 is past the ray's end: no hit, and the record is not written.
    let mut mem = shared_mem();
    let (grid, start, end, out) = grid_ray_scene(&mut mem, -3.0, -1.0);
    assert_eq!(layout::mesh_ray_query(&mut host, &mut mem, grid, start, end, out, 0), 0);
    assert_eq!(mem.u32(out + 0x30), 0, "M_D760X leaves the record on a miss");
}

/// A node block: the box (centre, then half extents at +12), the faces word at +0x18 and the child at +0x1c, zero to
/// 0x40.
fn node_bytes(center: [f32; 3], half: [f32; 3], faces: u32, child: u32) -> Vec<u8> {
    let mut bytes: Vec<u8> = center.iter().chain(half.iter()).flat_map(|v| v.to_le_bytes()).collect();
    bytes.extend(faces.to_le_bytes());
    bytes.extend(child.to_le_bytes());
    bytes.resize(0x40, 0);
    bytes
}

#[test]
fn node_walk_collects_overlapping_leaves() {
    // The root has no faces and one child, a leaf. The child's bit 21 at +0x38 blocks its second child (+0x20).
    let (query, root, child, out): (u32, u32, u32, u32) = (0x1000_6000, 0x1000_7000, 0x1000_8000, 0x1000_6100);
    for (center, expected) in [([0.0, 0.0, 0.0], 1), ([10.0, 0.0, 0.0], 0)] {
        let mut mem = shared_mem();
        map_floats(&mut mem, query, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        let mut leaf = node_bytes(center, [1.0; 3], 2, 0);
        leaf[0x38..0x3c].copy_from_slice(&0x20_0000u32.to_le_bytes());
        mem.map(child, &leaf);
        mem.map(root, &node_bytes([0.0; 3], [1.0; 3], 0, child));
        mem.map(out, &[0; 0x40]);
        layout::node_walk(&mut mem, query, root, out);
        assert_eq!(mem.u32(out), expected, "NodeWalk count for centre {center:?}");
        if expected == 1 {
            assert_eq!(mem.u32(out + 4), child, "NodeWalk leaf");
        }
    }
}

/// The cell query of SegmentSweep's test: one list head (`head`), and the other three slots empty.
struct CellTestHost {
    head: u32,
}

impl e38::CellQueryHost for CellTestHost {
    fn query_cells(&mut self, mem: &mut Mem, _lo: u32, _hi: u32, cells: u32) {
        mem.set_u32(cells, self.head);
    }
}

#[test]
fn segment_sweep_tests_flagged_meshes() {
    // A quad in z = 0 (triangles (v0, v2, v1) and (v0, v3, v2)) in one object with flag bit 9 set. The segment runs up
    // through its middle at ray parameter 0.5. Its tags are 1..4.
    let (start, end, out, distance): (u32, u32, u32, u32) = (0x1000_6000, 0x1000_6010, 0x1000_6100, 0x1000_6200);
    let (scratch, head, node, object, block): (u32, u32, u32, u32, u32) =
        (0x1000_9000, 0x1000_6300, 0x1000_6400, 0x1000_6500, 0x1000_6600);
    for (flag, start_xy, expected) in [(0x200u32, 0.25f32, Some(0.5f32)), (0, 0.25, None), (0x200, 2.0, None)] {
        let mut mem = shared_mem();
        map_floats(&mut mem, start, &[start_xy, start_xy, -1.0]);
        map_floats(&mut mem, end, &[start_xy, start_xy, 1.0]);
        mem.map(out, &[0; 0x10]);
        mem.map(distance, &9.0f32.to_le_bytes());
        mem.map(scratch, &[0; 0x40]);
        mem.map(head, &node.to_le_bytes());
        let mut node_bytes = vec![0u8; 0x10];
        node_bytes[0xc..].copy_from_slice(&object.to_le_bytes());
        mem.map(node, &node_bytes);
        let mut object_bytes = vec![0u8; 0xc0];
        object_bytes[0x60..0x64].copy_from_slice(&flag.to_le_bytes());
        object_bytes[0xa0..0xa4].copy_from_slice(&block.to_le_bytes());
        for (index, tag) in [1u32, 2, 3, 4].iter().enumerate() {
            object_bytes[0xb0 + 4 * index..0xb4 + 4 * index].copy_from_slice(&tag.to_le_bytes());
        }
        mem.map(object, &object_bytes);
        map_floats(&mut mem, block, &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
        let mut host = CellTestHost { head };
        layout::segment_sweep(&mut host, &mut mem, scratch, start, end, out, distance);
        match expected {
            Some(t) => {
                assert_eq!(mem.f32(distance), t, "SegmentSweep distance");
                let tags: Vec<u32> = (0..4).map(|i| mem.u32(out + 4 * i)).collect();
                assert_eq!(tags, [1, 2, 3, 4], "SegmentSweep tags");
            }
            None => {
                assert_eq!(mem.f32(distance), 9.0, "SegmentSweep leaves the distance on a miss");
                assert_eq!(mem.u32(out), 0, "SegmentSweep leaves the tags on a miss");
            }
        }
    }
}

const GRID_RAY: u32 = 0x4FD760;
const SAMPLE_LOOKUP: u32 = 0x520240;
const NODE_WALK: u32 = 0x4FDD60;

/// The sample lookups of M_D760X: each takes the next recorded `FUN_00520240` call (its arguments: the table, the byte
/// word, and the two nibble sums). The byte is the call's low byte; the rest of that stack word is not read by the
/// callee.
struct SampleReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    issued: usize,
}

impl e38::SampleHost for SampleReplayHost<'_> {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32 {
        // The other inner calls are the Rust-ported vector edges, which the port runs itself.
        let index = self.next
            + self.inner[self.next..]
                .iter()
                .position(|call| call.addr == SAMPLE_LOOKUP)
                .unwrap_or_else(|| panic!("unexpected sample lookup {}", self.issued));
        let call = &self.inner[index];
        assert_eq!(call.args[1] & 0xFF, byte, "sample byte");
        assert_eq!(call.args[2], low.to_bits(), "sample low sum");
        assert_eq!(call.args[3], high.to_bits(), "sample high sum");
        self.next = index + 1;
        self.issued += 1;
        call.eax
    }
}

#[test]
fn grid_ray_matches_original_calls() {
    let Some(calls) = recorded_calls(GRID_RAY) else {
        eprintln!("skipping M_D760X: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut hits = 0;
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let &[grid, start, end, out, list] = call.args.as_slice() else {
            panic!("M_D760X call {}: five arguments", call.index)
        };
        let mut host = SampleReplayHost { inner: &call.inner, next: 0, issued: 0 };
        let eax = layout::mesh_ray_query(&mut host, &mut mem, grid, start, end, out, list);
        assert_eq!(eax, call.eax, "M_D760X call {} EAX", call.index);
        let lookups = call.inner.iter().filter(|inner| inner.addr == SAMPLE_LOOKUP).count();
        assert_eq!(host.issued, lookups, "M_D760X call {} sample lookups", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "M_D760X call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        hits += eax;
    }
    eprintln!("M_D760X: {} calls match, {hits} hit", calls.len());
}

#[test]
fn node_walk_matches_original_calls() {
    let Some(calls) = recorded_calls(NODE_WALK) else {
        eprintln!("skipping E38NodeWalk: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut with_leaves = 0;
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let &[query, root, out] = call.args.as_slice() else {
            panic!("E38NodeWalk call {}: three arguments", call.index)
        };
        layout::node_walk(&mut mem, query, root, out);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "E38NodeWalk call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        with_leaves += usize::from(mem.u32(out) != 0);
    }
    eprintln!("E38NodeWalk: {} calls match, {with_leaves} with leaves", calls.len());
}

const FACE_QUERY: u32 = 0x4FEBC0;
const POINT_TRIANGLE: u32 = 0x4FE2D0;
const SEGMENT_TEST: u32 = 0x4FF070;
/// Where the replays put E39_SegTest70's query box (its frame is not in the records).
const SEGMENT_QUERY: u32 = 0x0012_0000 - 0x18;

#[test]
fn face_query_matches_original_calls() {
    let Some(calls) = recorded_calls(FACE_QUERY) else {
        eprintln!("skipping E38Ops::M_EBC0: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut hits = 0;
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let &[this, point, out, list] = call.args.as_slice() else {
            panic!("M_EBC0 call {}: four arguments", call.index)
        };
        let mut host = SampleReplayHost { inner: &call.inner, next: 0, issued: 0 };
        let eax = layout::face_query(&mut host, &mut mem, this, point, out, list, Variant::Stock102);
        assert_eq!(eax, call.eax, "M_EBC0 call {} EAX", call.index);
        let lookups = call.inner.iter().filter(|inner| inner.addr == SAMPLE_LOOKUP).count();
        assert_eq!(host.issued, lookups, "M_EBC0 call {} sample lookups", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "M_EBC0 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        hits += eax;
    }
    eprintln!("M_EBC0: {} calls match, {hits} hit", calls.len());
}

#[test]
fn point_triangle_matches_original_calls() {
    replay(POINT_TRIANGLE, "E38Ops::M_E2D0", u32::MAX, |mem, args| {
        let &[_, point, v0, v1, v2, out] = args else { panic!("M_E2D0: six arguments, got {}", args.len()) };
        layout::point_triangle(mem, point, v0, v1, v2, out, Variant::Stock102)
    });
}

#[test]
fn segment_test_matches_original_calls() {
    replay(SEGMENT_TEST, "E39_SegTest70", u32::MAX, |mem, args| {
        let &[this, a, b, out] = args else { panic!("E39_SegTest70: four arguments, got {}", args.len()) };
        layout::segment_test(mem, this, a, b, out, SEGMENT_QUERY)
    });
}

#[test]
fn seg_test170_walks_the_segment_box() {
    // The ends are the box's centre (`a`) and half extents (`b`), as E38NodeWalk reads them. With `out` 0 the records go
    // to the grid's own list (this+0x1014), and the result is 1 when the list has a record.
    let (this, root, child, a, b, tmp): (u32, u32, u32, u32, u32, u32) =
        (0x1000_9000, 0x1000_b000, 0x1000_c000, 0x1000_d000, 0x1000_f000, 0x1000_e000);
    for (centre, expected) in [([0.0, 0.0, 0.0], 1), ([10.0, 0.0, 0.0], 0)] {
        let mut mem = shared_mem();
        map_floats(&mut mem, a, &centre);
        map_floats(&mut mem, b, &[1.0; 3]);
        mem.map(tmp, &[0; 0x18]);
        let mut leaf = node_bytes([0.0; 3], [1.0; 3], 2, 0);
        leaf[0x38..0x3c].copy_from_slice(&0x20_0000u32.to_le_bytes());
        mem.map(child, &leaf);
        mem.map(root, &node_bytes([0.0; 3], [1.0; 3], 0, child));
        mem.map(this, &[0; 0x1020]);
        mem.set_u32(this + 8, root);
        assert_eq!(layout::seg_test170(&mut mem, this, a, b, 0, tmp), expected, "SegTest170 for the centre {centre:?}");
        assert_eq!(mem.u32(this + 0x1014), expected, "the grid's list count");
    }
}

/// The E38 solver passes' host calls replayed from the record: each call must be the next recorded one of these
/// addresses (the passes' other callees are ported inline), and its windows and returned runs are applied.
const SOLVER_HOST_CALLS: [u32; 7] = [0x5297d0, 0x5293d0, 0x5293e0, 0x4fb820, 0x4fbd30, 0x4fb890, 0x4fc1a0];

/// M_BD30's entry stack pointer in the replays (the pass records do not give it; `ComposeFromRecord` ignores where its
/// call enters).
const SOLVER_STACK: u32 = 0x0012_0000;

struct SolverReplayHost<'a> {
    inner: Vec<&'a Inner>,
    next: usize,
    mem: Mem,
    /// With `Some`, the ports of `M_BD30` and `E38_004FB890` run on each of their recorded calls first (the count of
    /// each checked).
    ports: Option<[usize; 2]>,
}

/// `ComposeA` in a replayed `M_BD30`: its output is the composed vector the recorded row holds after the call (the row
/// the port stores into, read from the recorded windows after the call).
struct ComposeFromRecord {
    mem: Mem,
    after: Vec<(u32, Vec<u8>)>,
}

impl e38::ImpulseHost for ComposeFromRecord {
    fn relative_velocity(&mut self, _: u32, _: u32, _: u32) {
        panic!("M_BD30 calls no PlaneDiff");
    }

    fn response(&mut self, _: u32, _: u32, _: u32, _: u32, _: u32, _: u32, _: u32) {
        panic!("M_BD30 calls no Query4390");
    }

    fn apply_impulse(&mut self, _: u32, _: u32, _: f32, _: f32, _: u32, _: u32) {
        panic!("M_BD30 calls no Emit4DC0");
    }

    fn compose(&mut self, _sp: u32, _frame: u32, _point: u32, body: u32, out: u32) {
        let mem = &mut self.mem;
        let rows = mem.u32(body + 0x274);
        let count = mem.u32(rows + 4);
        let at = rows + 0x10 + 0x60 * count + 0x40;
        // A body with its 8 rows stores none: the output is not used.
        let composed =
            match self.after.iter().find(|(start, bytes)| *start <= at && at + 16 <= *start + bytes.len() as u32) {
                Some((start, bytes)) if count < 8 => bytes[(at - start) as usize..][..16].to_vec(),
                _ => vec![0; 16],
            };
        mem.map(out, &composed);
    }
}

impl SolverReplayHost<'_> {
    /// The state before the next call: the replay so far (the pass's memory) with the call's input windows.
    fn state_before(&self) -> (Mem, &Inner) {
        let call = self.inner[self.next];
        let mut mem = self.mem.clone();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        (mem, call)
    }

    /// Checks `mem` after a port against the next call's windows and returned runs.
    fn check_after(&self, name: &str, mem: &Mem, call: &Inner) {
        let expected = call.windows.iter().map(|w| (w.at, &w.after)).chain(call.after.iter().map(|w| (w.at, &w.bytes)));
        for (at, bytes) in expected {
            let got = mem.read(at, bytes.len());
            if let Some(i) = (0..bytes.len()).find(|&i| got[i] != bytes[i]) {
                panic!(
                    "{name} (host call {}): byte {:#x} is {:#04x}, the original's {:#04x}",
                    self.next,
                    at + i as u32,
                    got[i],
                    bytes[i]
                );
            }
        }
    }

    fn take(&mut self, addr: u32, args: &[u32]) -> (u32, u64) {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected host call {addr:#x} {args:x?}"));
        assert_eq!((call.addr, &call.args[..]), (addr, args), "host call {}", self.next);
        for window in &call.windows {
            self.mem.map(window.at, &window.after);
        }
        for window in &call.after {
            self.mem.map(window.at, &window.bytes);
        }
        self.next += 1;
        (call.eax, call.st0)
    }
}

impl e38::SolverHost for SolverReplayHost<'_> {
    fn inverse_mass(&mut self, target: u32, body: u32) -> f64 {
        f64::from_bits(self.take(target, &[body]).1)
    }

    fn body_test(&mut self, target: u32, body: u32) -> u32 {
        self.take(target, &[body]).0
    }

    fn body_wake(&mut self, target: u32, body: u32) {
        self.take(target, &[body]);
    }

    fn contact_rows(&mut self, solver: u32, body: u32, event: u32, side: u32) {
        if let Some(checked) = self.ports {
            let (mut mem, call) = self.state_before();
            let after = call.windows.iter().map(|w| (w.at, w.after.clone())).collect();
            let mut host = ComposeFromRecord { mem: mem.clone(), after };
            // The frame global `ComposeA` is called on; the record's replay of the call does not use it.
            mem.map(0x7cbb00, &[0; 4]);
            let eax = layout::contact_rows(&mut host, &mut mem, solver, body, event, side, SOLVER_STACK);
            assert_eq!(eax, call.eax & 0xff, "M_BD30 (host call {}) EAX", self.next);
            self.check_after("M_BD30", &mem, call);
            self.ports = Some([checked[0] + 1, checked[1]]);
        }
        self.take(0x4fbd30, &[solver, body, event, side]);
    }

    fn apply_offsets(&mut self, row: u32) {
        if let Some(checked) = self.ports {
            let (mut mem, call) = self.state_before();
            layout::apply_offsets(&mut mem, row, Variant::Stock102);
            self.check_after("E38_004FB890", &mem, call);
            self.ports = Some([checked[0], checked[1] + 1]);
        }
        self.take(0x4fb890, &[row]);
    }

    fn solve_event(&mut self, solver: u32, event: u32) {
        self.take(0x4fc1a0, &[solver, event]);
    }
}

/// The E38 solver passes on harness/vector_specs/collision.py's `_e38_solver_direct_fuzz` solvers (direct records):
/// eax, the host calls in order, and every window.
#[test]
fn e38_solver_passes_match_original_calls() {
    type Pass = fn(&mut SolverReplayHost, &mut Mem, u32) -> u32;
    let passes: [(u32, &str, Pass); 5] = [
        (0x4fb7f0, "E38_004FB7F0", |_, mem, solver| layout::clear_contact_rows(mem, solver)),
        (0x4fbc20, "E38_004FBC20", |host, mem, solver| {
            layout::refresh_events(host, mem, solver);
            u32::MAX
        }),
        (0x4fc0b0, "CCollisionSolver::Vf08", |_, mem, solver| {
            layout::drain_events(mem, solver).expect("CCollisionEvent events");
            0
        }),
        (0x4fc590, "E38_004FC590X", |host, mem, solver| {
            layout::correct_penetrations(host, mem, solver, Variant::Stock102)
        }),
        (0x4fca20, "E38_004FCA20", |host, mem, solver| {
            layout::solve_due_events(host, mem, solver);
            u32::MAX
        }),
    ];
    for (addr, name, pass) in passes {
        let Some(calls) = recorded_calls(addr) else {
            eprintln!("skipping {name}: no matching RBR_COLLISION_VECTORS calls");
            continue;
        };
        let mut host_calls = 0;
        for call in &calls {
            let mut mem = shared_mem();
            for window in &call.windows {
                mem.map(window.at, &window.before);
            }
            let mut host = SolverReplayHost {
                inner: call.inner.iter().filter(|inner| SOLVER_HOST_CALLS.contains(&inner.addr)).collect(),
                next: 0,
                mem: mem.clone(),
                ports: None,
            };
            let eax = pass(&mut host, &mut mem, call.args[0]);
            if eax != u32::MAX {
                assert_eq!(eax, call.eax, "{name} call {} EAX", call.index);
            }
            assert_eq!(host.next, host.inner.len(), "{name} call {} host calls", call.index);
            host_calls += host.next;
            for window in &call.windows {
                assert_eq!(
                    mem.read(window.at, window.len),
                    window.after,
                    "{name} call {} window {} at {:#x}",
                    call.index,
                    window.index,
                    window.at
                );
            }
        }
        eprintln!("{name}: {} calls match ({host_calls} host calls)", calls.len());
    }
}

/// E38Ops::M_BD30 (0x4fbd30) and E38_004FB890 (0x4fb890) on their calls inside the recorded E38_004FC590X passes: each
/// port runs on the replayed state with the call's input windows, and must leave the recorded windows (the solver's
/// rows, the body's +0x274; the row, the body) and eax. `ComposeA`'s output is the recorded row's.
#[test]
fn e38_contact_rows_and_offsets_match_original_calls() {
    let Some(calls) = recorded_calls(0x4fc590) else {
        eprintln!("skipping E38_004FC590X's inner calls: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut checked = [0; 2];
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = SolverReplayHost {
            inner: call.inner.iter().filter(|inner| SOLVER_HOST_CALLS.contains(&inner.addr)).collect(),
            next: 0,
            mem: mem.clone(),
            ports: Some([0; 2]),
        };
        layout::correct_penetrations(&mut host, &mut mem, call.args[0], Variant::Stock102);
        let [rows, offsets] = host.ports.unwrap();
        checked = [checked[0] + rows, checked[1] + offsets];
    }
    assert!(checked[0] > 0 && checked[1] > 0, "no M_BD30 or E38_004FB890 call checked");
    eprintln!("M_BD30: {} calls match, E38_004FB890: {} calls match", checked[0], checked[1]);
}

/// M_C1A0's calls replayed from a direct record: each must be the next recorded one with the recorded arguments, and
/// the vectors it takes must hold what the original passed (its windows before); then its outputs are applied.
struct ImpulseReplayHost<'a> {
    inner: Vec<&'a Inner>,
    next: usize,
    /// The port's memory (a clone of the shared map).
    mem: Mem,
}

/// The calls of M_C1A0 the port leaves to its host (the record also lists their own inner calls).
const IMPULSE_HOST_CALLS: [u32; 3] = [0x4f40f0, 0x4f4390, 0x4f4dc0];

impl ImpulseReplayHost<'_> {
    fn take(&mut self, addr: u32, args: &[u32], outputs: usize) {
        let mem = &mut self.mem;
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected call {addr:#x} {args:x?}"));
        assert_eq!((call.addr, &call.args[..]), (addr, args), "call {}", self.next);
        for (k, window) in call.windows.iter().enumerate() {
            if k < call.windows.len() - outputs {
                assert_eq!(
                    mem.read(window.at, window.len),
                    window.before,
                    "call {} ({addr:#x}) input at {:#x}",
                    self.next,
                    window.at
                );
            }
            mem.map(window.at, &window.after);
        }
        for window in &call.after {
            mem.map(window.at, &window.bytes);
        }
        self.next += 1;
    }
}

impl e38::ImpulseHost for ImpulseReplayHost<'_> {
    fn relative_velocity(&mut self, _sp: u32, event: u32, out: u32) {
        self.take(0x4f40f0, &[event, out], 1);
    }

    fn response(&mut self, _sp: u32, event: u32, direction: u32, other: u32, use_other: u32, along: u32, across: u32) {
        let args = [event, direction, other, use_other, along, across];
        self.take(0x4f4390, &args, 2);
    }

    fn apply_impulse(&mut self, _sp: u32, event: u32, impulse: f32, slip_speed: f32, vector: u32, slip: u32) {
        let args = [event, impulse.to_bits(), slip_speed.to_bits(), vector, slip];
        self.take(0x4f4dc0, &args, 0);
    }

    fn compose(&mut self, _: u32, _: u32, _: u32, _: u32, _: u32) {
        panic!("M_C1A0 calls no ComposeA");
    }
}

/// E38Ops::M_C1A0 (0x4fc1a0) on harness/vector_specs/collision.py's `_e38_impulse_direct_fuzz` events (direct
/// records): the impulse (st0), its calls in order with their arguments and input vectors, and the event's windows.
/// The entry stack pointer is the one whose frame holds `PlaneDiff`'s output where the original's did.
#[test]
fn e38_impulse_matches_original_calls() {
    let Some(calls) = recorded_calls(0x4fc1a0) else {
        eprintln!("skipping E38Ops::M_C1A0: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut solved = 0;
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let inner: Vec<&Inner> = call.inner.iter().filter(|inner| IMPULSE_HOST_CALLS.contains(&inner.addr)).collect();
        let velocity_at = inner[0].args[1];
        let esp = velocity_at - 0x50 + 0x94;
        let mut host = ImpulseReplayHost { inner, next: 0, mem: mem.clone() };
        let impulse = layout::solve_event(&mut host, &mut mem, call.args[1], esp, rbr_physics::Variant::Stock102);
        assert_eq!(f64::from(impulse).to_bits(), call.st0, "M_C1A0 call {} impulse {impulse}", call.index);
        assert_eq!(host.next, host.inner.len(), "M_C1A0 call {} calls", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "M_C1A0 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        solved += usize::from(host.next > 1);
    }
    eprintln!("M_C1A0: {} calls match, {solved} solved", calls.len());
}

/// CBody::VfD0's calls replayed from a record (harness/vector_specs/body_vfd0.py): each must be the next recorded one
/// with the recorded arguments (a slot call's target is the record's), and the inputs it reads in VfD0's frame must
/// hold what the original left there; then its outputs are applied and its eax returned.
struct VfD0ReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    mem: Mem,
    /// The caller's frame, `[frame, frame + size)`.
    frame: u32,
    size: u32,
    /// The events VfDC made: an input there (the event Append takes) is checked too.
    events: Vec<u32>,
}

impl VfD0ReplayHost<'_> {
    fn take(&mut self, addr: Option<u32>, args: &[u32], outputs: usize) -> u32 {
        let call = self.inner.get(self.next).unwrap_or_else(|| panic!("unexpected call {addr:x?} {args:x?}"));
        assert_eq!((addr.unwrap_or(call.addr), &call.args[..]), (call.addr, args), "call {}", self.next);
        let checked = |at: u32| (self.frame..self.frame + self.size).contains(&at) || self.events.contains(&at);
        for (k, window) in call.windows.iter().enumerate() {
            if k + outputs < call.windows.len() && checked(window.at) {
                assert_eq!(
                    self.mem.read(window.at, window.len),
                    window.before,
                    "call {} ({:#x}) input at {:#x}",
                    self.next,
                    call.addr,
                    window.at
                );
            }
            self.mem.map(window.at, &window.after);
        }
        for window in &call.after {
            self.mem.map(window.at, &window.bytes);
        }
        self.next += 1;
        call.eax
    }
}

impl collide_big::VfD0Host for VfD0ReplayHost<'_> {
    fn ray_seeds(&mut self, _: u32, grid: u32) {
        self.take(Some(0x4f6550), &[grid], 1);
    }

    fn ray_clear(&mut self, _: u32, grid: u32) {
        self.take(Some(0x4f6740), &[grid], 1);
    }

    fn collide_init(&mut self, _: u32, other: u32, body: u32, list: u32) -> u32 {
        self.take(Some(0x51bd30), &[other, body, 0, list], 0)
    }

    fn transform(&mut self, _: u32, entity: u32) -> u32 {
        self.take(None, &[entity], 0)
    }

    fn grid_active(&mut self, _: u32, entity: u32) -> u32 {
        self.take(None, &[entity], 0)
    }

    fn grid(&mut self, _: u32, entity: u32) -> u32 {
        self.take(None, &[entity], 0)
    }

    fn vf80(&mut self, _: u32, entity: u32) -> u32 {
        self.take(None, &[entity], 0)
    }

    fn vf_f0(&mut self, _: u32, entity: u32, args: [u32; 8]) -> u32 {
        let mut words = vec![entity];
        words.extend(args);
        self.take(None, &words, 5)
    }

    fn append(&mut self, _: u32, list: u32, event: u32) {
        self.take(None, &[list, event], 0);
    }

    fn fetch_y(&mut self, _: u32, matrix: u32, out: u32, transform: u32) {
        self.take(Some(0x51b780), &[matrix, out, transform], 1);
    }

    fn fetch_x(&mut self, _: u32, matrix: u32, out: u32, transform: u32) {
        self.take(Some(0x51b6c0), &[matrix, out, transform], 1);
    }

    fn combine(&mut self, _: u32, desc: u32, point: u32, transform: u32) {
        self.take(Some(0x51ab90), &[desc, point, transform], 1);
    }

    fn combine_twin(&mut self, _: u32, desc: u32, point: u32, transform: u32) {
        self.take(Some(0x51ab70), &[desc, point, transform], 1);
    }

    fn compose_a(&mut self, _: u32, transform: u32, point: u32, other: u32, out: u32) {
        self.take(Some(0x51b4c0), &[transform, point, other, out], 1);
    }

    fn compose_b(&mut self, _: u32, transform: u32, point: u32, other: u32, out: u32) {
        self.take(Some(0x51b600), &[transform, point, other, out], 1);
    }

    fn grid_copy(&mut self, _: u32, grid: u32, src: u32, a: u32, b: u32) {
        self.take(Some(0x4f6610), &[grid, src, a, b], 1);
    }

    fn edge_edge(
        &mut self,
        _: u32,
        grid: u32,
        from: u32,
        to: u32,
        hit: u32,
        t: u32,
        normal: u32,
        material: u32,
    ) -> u32 {
        let args = [grid, from, to, hit, t, normal, material];
        self.take(Some(0x4f8280), &args, 4)
    }

    fn normalize(&mut self, _: u32, vector: u32) {
        self.take(Some(0x43c6a0), &[vector], 0);
    }

    fn add(&mut self, _: u32, a: u32, out: u32, b: u32) -> u32 {
        self.take(Some(0x4a19e0), &[a, out, b], 1)
    }

    fn sub(&mut self, _: u32, a: u32, out: u32, b: u32) -> u32 {
        self.take(Some(0x465da0), &[a, out, b], 1)
    }

    fn make_event(
        &mut self,
        _: u32,
        kind: collide_big::ContactKind,
        hit: u32,
        body: u32,
        other: u32,
        at: u32,
        normal: u32,
    ) -> u32 {
        let args = [kind as u32, hit, 0, body, other, at, 0x8ef7e0, normal, 0];
        self.take(Some(0x4e0d00), &args, 0)
    }

    fn sample_b(&mut self, _: u32, grid: u32, index: u32, out_a: u32, out_b: u32) {
        self.take(Some(0x4e0950), &[grid, index, out_a, out_b], 2);
    }

    fn sample_a(&mut self, _: u32, grid: u32, index: u32, t: f32, out_a: u32, out_b: u32) {
        let args = [grid, index, t.to_bits(), out_a, out_b];
        self.take(Some(0x4e0840), &args, 2);
    }
}

/// CBody::VfD0 (0x4e1eb0) on the fuzzed in-step records of harness/vector_specs/body_vfd0.py: eax, and its calls in
/// order with their arguments and the inputs it leaves in its frame. The frame is the record's last window.
#[test]
fn body_vf_d0_matches_original_calls() {
    let Some(calls) = recorded_calls(0x4e1eb0) else {
        eprintln!("skipping CBody::VfD0: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut events = 0;
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let frame = call.windows.last().unwrap().at;
        let mut host =
            VfD0ReplayHost { inner: &call.inner, next: 0, mem: mem.clone(), frame, size: 0xe90, events: Vec::new() };
        let [this, matrix, other, list] = call.args[..] else { panic!("VfD0 call {} args", call.index) };
        let eax = layout::body_vf_d0(&mut host, &mut mem, this, matrix, other, list, frame + 0xe94);
        assert_eq!(eax, call.eax, "VfD0 call {} eax", call.index);
        assert_eq!(host.next, call.inner.len(), "VfD0 call {} calls", call.index);
        events += call.inner.iter().filter(|inner| inner.addr == 0x4e0d00).count();
    }
    eprintln!("CBody::VfD0: {} calls match, {events} events", calls.len());
}

impl collide_big::VfDcHost for VfD0ReplayHost<'_> {
    fn vf34(&mut self, _: u32, entity: u32) -> u32 {
        self.take(None, &[entity], 0)
    }

    fn edge_pair(&mut self, _: u32, grid: u32, args: [u32; 10]) -> u32 {
        let mut words = vec![grid];
        words.extend(args);
        self.take(Some(0x4f8560), &words, 3)
    }

    fn pair_event(&mut self, _: u32, hit: u32, t: u32, body: u32, other: u32, at: u32, normal: u32) -> u32 {
        let args = [1, hit, t, body, other, at, 0x8ef7e0, normal, 0];
        let event = self.take(Some(0x4e0d00), &args, 0);
        self.events.push(event);
        event
    }
}

/// CBody::VfDC (0x4e2850) on the fuzzed in-step records of harness/vector_specs/g_vfdc.py: eax, its calls in order with
/// their arguments and inputs (each appended event with the grid's flag bytes in it), and the grid's flag bytes after.
/// The frame is the record's last window, the grid's flag bytes its second.
#[test]
fn body_vf_dc_matches_original_calls() {
    let Some(calls) = recorded_calls(0x4e2850) else {
        eprintln!("skipping CBody::VfDC: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let (mut pairs, mut events) = (0, 0);
    for call in &calls {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let frame = call.windows.last().unwrap().at;
        let mut host =
            VfD0ReplayHost { inner: &call.inner, next: 0, mem: mem.clone(), frame, size: 0x14e0, events: Vec::new() };
        let [this, a, b, other, _, list] = call.args[..] else { panic!("VfDC call {} args", call.index) };
        let eax = layout::body_vf_dc(&mut host, &mut mem, this, a, b, other, list, frame + 0x14e4);
        assert_eq!(eax, call.eax, "VfDC call {} eax", call.index);
        assert_eq!(host.next, call.inner.len(), "VfDC call {} calls", call.index);
        if call.windows.len() == 4 {
            let flags = &call.windows[1];
            assert_eq!(mem.read(flags.at, flags.len), flags.after, "VfDC call {} grid flags", call.index);
        }
        let count = |addr: u32| call.inner.iter().filter(|inner| inner.addr == addr).count();
        pairs += count(0x4f8560);
        events += count(0x4e0d00);
    }
    eprintln!("CBody::VfDC: {} calls match, {pairs} edge pairs, {events} events", calls.len());
}

/// PlaneDiff's slot 0x140 calls replayed from a direct record: the next recorded call, its target and arguments, the
/// point it reads; then the velocity it wrote.
struct PlaneDiffReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    mem: Mem,
}

impl event::PlaneDiffHost for PlaneDiffReplayHost<'_> {
    fn point_velocity(&mut self, _: u32, target: u32, body: u32, point: u32, out: u32) {
        let call = &self.inner[self.next];
        assert_eq!((call.addr, &call.args[..]), (target, &[body, point, out][..]), "call {}", self.next);
        assert_eq!(self.mem.read(point, 0x10), call.windows[0].before, "point");
        self.mem.map(out, &call.windows[1].after);
        self.next += 1;
    }
}

/// E37EvOps::PlaneDiff (0x4f40f0) on direct records (harness/vector_specs/body_vfd0.py): eax, the slot calls and
/// `out` (lane w kept).
#[test]
fn plane_diff_matches_original_calls() {
    let Some(calls) = recorded_calls(0x4f40f0) else {
        eprintln!("skipping E37EvOps::PlaneDiff: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut one_sided = 0;
    for call in calls.iter().filter(|call| call.inner.iter().all(|inner| inner.windows.len() == 2)) {
        let mut mem = shared_mem();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let mut host = PlaneDiffReplayHost { inner: &call.inner, next: 0, mem: mem.clone() };
        // The original's frame, from the first side's output (the body's at +0x10, the partner's at +0x20).
        let frame = call
            .inner
            .first()
            .map_or(0x7f_f000, |side| side.args[2] - if mem.u32(call.args[0] + 0xa0) != 0 { 0x10 } else { 0x20 });
        let eax = layout::relative_point_velocity(&mut host, &mut mem, call.args[0], call.args[1], frame + 0x34);
        assert_eq!(eax, call.eax, "PlaneDiff call {} eax", call.index);
        assert_eq!(host.next, call.inner.len(), "PlaneDiff call {} calls", call.index);
        let out = &call.windows[1];
        assert_eq!(mem.read(out.at, out.len), out.after, "PlaneDiff call {} out", call.index);
        one_sided += usize::from(call.inner.len() < 2);
    }
    eprintln!("E37EvOps::PlaneDiff: {} calls match, {one_sided} one-sided", calls.len());
}

/// The mesh's bounds copy and activity (0x4fb070, 0x4fb0b0) and the partner slot of Emit4DC0 (0x4f4dc0): unit checks.
#[test]
fn mesh_bounds_and_partner_slot_unit() {
    let mut mem = shared_mem();
    let mesh = 0x0100_0000;
    let (a, b) = (0x0100_1000, 0x0100_1010);
    for (i, value) in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0].into_iter().enumerate() {
        mem.set_u32(mesh + 0x70 + 4 * i as u32, value.to_bits());
    }
    assert_eq!(layout::mesh_copy_bounds(&mut mem, mesh, a, b), b);
    assert_eq!(mem.f32(a + 12), 4.0);
    assert_eq!(mem.f32(b), 5.0);
    assert_eq!(mem.f32(b + 12), 8.0);
    assert_eq!(e38::mesh_is_active(), 1);
    assert_eq!(event::partner_slot(0, 9, 7, 8), (7, 8));
    assert_eq!(event::partner_slot(3, 9, 7, 8), (9, 3));
}

/// Records the calls of Query4390 and Emit4DC0 with their entry stack pointers, and answers each virtual call with 7.
#[derive(Default)]
struct BodyRecorder {
    calls: Vec<(&'static str, u32)>,
}

impl layout::EventBodyHost for BodyRecorder {
    fn vf30(&mut self, sp: u32, _target: u32, body: u32) -> u32 {
        self.calls.push(("vf30", sp));
        body
    }
    fn mb560(&mut self, sp: u32, _event: u32, _a: u32, _b: u32, _c: u32) {
        self.calls.push(("mb560", sp));
    }
    fn compose(&mut self, sp: u32, _frame: u32, _point: u32, _body: u32, _out: u32) {
        self.calls.push(("compose", sp));
    }
    fn vf138x(&mut self, sp: u32, _target: u32, _body: u32, _args: [u32; 6]) -> u32 {
        self.calls.push(("vf138x", sp));
        7
    }
    fn vf128x(&mut self, sp: u32, _target: u32, _body: u32, _args: [u32; 8]) -> u32 {
        self.calls.push(("vf128x", sp));
        7
    }
}

/// Query4390 and Emit4DC0 call their bodies in the original's order, each entered at its frame's stack pointer.
#[test]
fn event_body_calls_follow_the_original_frame() {
    let mut mem = shared_mem();
    let (event, esp) = (0x0100_0000, 0x0200_0000 + 0x3c);
    let frame = (esp - 4) & !0xf;
    let (body_a, body_b, vtable) = (0x0100_2000, 0x0100_3000, 0x0100_4000);
    mem.set_u32(event + 0xa0, body_a);
    mem.set_u32(event + 0x98, body_b);
    for word in [0x60, 0x80, 0x8c, 0x90, 0xa4, 0xa8, 0xe4] {
        mem.set_u32(event + word, 0);
    }
    for body in [body_a, body_b] {
        mem.set_u32(body, vtable);
    }
    for slot in [0x30, 0x128, 0x138] {
        mem.set_u32(vtable + slot, 0x0500_0000);
    }
    let vectors = [0x0100_5000, 0x0100_5010, 0x0100_5020, 0x0100_5030, 0x0100_5040];
    for offset in (0..0x60).step_by(4) {
        mem.set_u32(0x0100_5000 + offset, 0);
    }
    let mut host = BodyRecorder::default();
    let eax = layout::query_event_bodies(&mut host, &mut mem, event, vectors, esp);
    assert_eq!(eax, 7);
    let per_body = [
        ("vf30", frame - 0x48),
        ("mb560", frame - 0x50),
        ("vf30", frame - 0x48),
        ("mb560", frame - 0x50),
        ("vf30", frame - 0x48),
        ("compose", frame - 0x50),
        ("vf138x", frame - 0x5c),
    ];
    let expected: Vec<_> = per_body.iter().chain(per_body.iter()).copied().collect();
    assert_eq!(host.calls, expected);

    let mut host = BodyRecorder::default();
    mem.set_u32(event + 0xe4, 0x2000);
    assert_eq!(layout::apply_event_impulse(&mut host, &mut mem, event, vectors[..4].try_into().unwrap(), esp), 0x2000);
    mem.set_u32(event + 0xe4, 0);
    assert_eq!(layout::apply_event_impulse(&mut host, &mut mem, event, vectors[..4].try_into().unwrap(), esp), 7);
    assert_eq!(host.calls, vec![("vf128x", frame - 0x54), ("vf128x", frame - 0x54)]);
}

/// The leaves of the contact hit paths: E09Vec4::Sub and E21Vec3::Add copy the stale word below
/// their aligned frame as w, Normalize scales all four lanes, SampleB reads the byte pair the grid's +0x690 byte names.
#[test]
fn hit_path_leaves_unit() {
    let mut mem = shared_mem();
    let (a, b, out, esp) = (0x0100_0000, 0x0100_0010, 0x0100_0020, 0x0200_0048);
    let stale = ((esp - 4) & !0xf) - 4;
    for (at, v) in [(a, [1.0f32, 2.0, 4.0, 9.0]), (b, [0.5, 8.0, -1.0, 7.0])] {
        layout::Vec4 { x: v[0], y: v[1], z: v[2], w: v[3] }.store(&mut mem, at);
    }
    mem.set_u32(stale, 0xdead_beef);
    assert_eq!(layout::vec4_sub_stale(&mut mem, a, out, b, esp), out);
    let words = |mem: &Mem| [0, 4, 8, 12].map(|k| mem.u32(out + k));
    let f = |x: f32| x.to_bits();
    assert_eq!(words(&mem), [f(0.5), f(-6.0), f(5.0), 0xdead_beef]);
    assert_eq!(layout::vec3_add_stale(&mut mem, a, out, b, esp), out);
    assert_eq!(words(&mem), [f(1.5), f(10.0), f(3.0), 0xdead_beef]);

    layout::Vec4 { x: 3.0, y: 0.0, z: 4.0, w: 10.0 }.store(&mut mem, out);
    assert_eq!(layout::vec4_normalize(&mut mem, out), out);
    let v = layout::Vec4::load(&mem, out);
    assert!((v.x - 0.6).abs() < 1e-3 && (v.z - 0.8).abs() < 1e-3 && (v.w - 2.0).abs() < 1e-3);

    let grid = 0x0110_0000;
    let (data, table_a, table_b) = (0x0120_0000, 0x0130_0000, 0x0140_0000);
    mem.set_u32(grid + 0x400, 0);
    assert_eq!(layout::sample_b(&mem, grid, 0), (0, 0));
    mem.set_u32(grid + 0x400, data);
    mem.set_u32(grid + 0x404, 4 | (8 << 16)); // width 4, height 8
    mem.set_u32(grid + 0x408, table_a);
    mem.set_u32(grid + 0x40c, table_b);
    mem.set_u8(grid + 0x690 + 3, 5);
    mem.map(data + 10, &[128, 192]); // (x, y) = (0.5, 0.75)
    mem.set_u8(table_a + 3 * 4 + 2, 0x11);
    mem.set_u8(table_b + 6 * 8 + 4, 0x22);
    assert_eq!(layout::sample_b(&mem, grid, 3), (0x22, 0x11));
    layout::store_samples(&mut mem, a, b, (0x22, 0x11));
    assert_eq!((mem.u32(a), mem.u32(b)), (0x22, 0x11));
}

/// CCollisionEvent::InitFull (0x4f50e0): the fields, the payload copy (over the +0x80 words it cleared first), the two
/// transforms of the body (each entered 0x90 below the aligned frame), the root identity's handle, and the event count.
#[test]
fn init_full_unit() {
    let mut mem = shared_mem();
    let (event, payload, body, vtable, root) = (0x0100_0000, 0x0100_1000, 0x0100_2000, 0x0100_3000, 0x0100_4000);
    let esp = 0x0200_0044;
    for k in (0..0xf0).step_by(4) {
        mem.set_u32(event + k, 0xffff_ffff);
    }
    for k in 0..16 {
        mem.set_u32(payload + 4 * k, 0x100 + k);
    }
    mem.set_u32(body, vtable);
    mem.set_u32(vtable + 0x30, 0x0500_0000);
    mem.set_u32(0x007c_bb00, root);
    mem.set_u32(root + 8, 6);
    mem.set_u32(0x008e_f7f0, 41);
    let mut host = BodyRecorder::default();
    let args = [2, body, 0x0100_5000, 0x0100_5010, payload, 9];
    assert_eq!(layout::init_full(&mut host, &mut mem, event, args, esp), event);
    let sp = ((esp - 4) & !0xf) - 0x90;
    assert_eq!(host.calls, [("vf30", sp), ("compose", sp), ("vf30", sp), ("mb560", sp)]);
    assert_eq!([0, 4, 8].map(|k| mem.u32(event + k)), [0x739558, root, 7]);
    for k in [0xc, 0x10, 0x14, 0x40, 0x4c, 0x50, 0x5c, 0xa4, 0xa8, 0xac, 0xb0, 0xb4] {
        assert_eq!(mem.u32(event + k), 0, "+{k:#x}");
    }
    assert_eq!((0..16).map(|k| mem.u32(event + 0x60 + 4 * k)).collect::<Vec<_>>(), (0x100..0x110).collect::<Vec<_>>());
    assert_eq!([0xa0, 0xe0, 0xe4].map(|k| mem.u32(event + k)), [body, 9, 2]);
    assert_eq!(mem.u32(0x008e_f7f0), 42);
}

const SYSTEM_VF00: u32 = 0x4FE810;
const MESH_RAY: u32 = 0x4FDE40;

/// The host calls of `CCollisionSystem::Vf00` answered from the record: the table lookups, in order. The mesh ray is
/// Rust's (`layout::mesh_ray_worklist`), so its recorded call only gives the frame.
struct SystemReplayHost<'a> {
    inner: &'a [Inner],
    next: usize,
    lookups: usize,
}

impl SystemReplayHost<'_> {
    fn take(&mut self, addr: u32) -> &Inner {
        let index = self.next
            + self.inner[self.next..]
                .iter()
                .position(|call| call.addr == addr)
                .unwrap_or_else(|| panic!("no inner call to {addr:#x} left"));
        self.next = index + 1;
        &self.inner[index]
    }
}

impl e38::SampleHost for SystemReplayHost<'_> {
    fn sample(&mut self, byte: u32, low: f32, high: f32) -> u32 {
        let call = self.take(SAMPLE_LOOKUP);
        assert_eq!(call.args[1] & 0xFF, byte, "sample byte");
        assert_eq!(call.args[2], low.to_bits(), "sample low sum");
        assert_eq!(call.args[3], high.to_bits(), "sample high sum");
        let eax = call.eax;
        self.lookups += 1;
        eax
    }
}

#[test]
fn system_vf00_matches_original_calls() {
    let Some(calls) = recorded_calls(SYSTEM_VF00) else {
        eprintln!("skipping CCollisionSystem::Vf00: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut hits = 0;
    for call in &calls {
        let mut mem = Mem::new();
        for window in call.windows.iter().chain(call.inner.iter().flat_map(|inner| &inner.windows)) {
            mem.map(window.at, &window.before);
        }
        let &[system, origin, end, out] = call.args.as_slice() else {
            panic!("Vf00 call {}: four arguments", call.index)
        };
        // The mesh ray's direction is the frame's +8 word: the entry esp is 0x44 above it.
        let mesh = call
            .inner
            .iter()
            .find(|inner| inner.addr == MESH_RAY)
            .unwrap_or_else(|| panic!("Vf00 call {}: no mesh ray", call.index));
        let frame = layout::system_frame(mesh.args[2] + 0x44);
        let mut host = SystemReplayHost { inner: &call.inner, next: 0, lookups: 0 };
        let eax = layout::system_vf00(&mut host, &mut mem, system, origin, end, out, frame);
        assert_eq!(eax, call.eax, "Vf00 call {} EAX", call.index);
        hits += eax;
        let lookups = call.inner.iter().filter(|inner| inner.addr == SAMPLE_LOOKUP).count();
        assert_eq!(host.lookups, lookups, "Vf00 call {} sample lookups", call.index);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "Vf00 call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
    }
    assert!(hits > 0, "no Vf00 call hit: the records do not reach the hit path");
    eprintln!("CCollisionSystem::Vf00: {} calls, {hits} hits", calls.len());
}

/// E38Ops::M_DE40X (0x4fde40) from the direct-call records (`--set g-de40`): the tree and the table are in each record,
/// and the hit record (window 2) must match. A hit is a call whose hit record changed.
#[test]
fn mesh_ray_matches_original_calls() {
    let Some(calls) = recorded_calls(MESH_RAY) else {
        eprintln!("skipping E38Ops::M_DE40X: no matching RBR_COLLISION_VECTORS calls");
        return;
    };
    let mut hits = 0;
    for call in &calls {
        let mut mem = Mem::new();
        for window in &call.windows {
            mem.map(window.at, &window.before);
        }
        let &[system, origin, direction, root, out] = call.args.as_slice() else {
            panic!("M_DE40X call {}: five arguments", call.index)
        };
        layout::mesh_ray_worklist(&mut mem, system, origin, direction, root, out);
        for window in &call.windows {
            assert_eq!(
                mem.read(window.at, window.len),
                window.after,
                "M_DE40X call {} window {} at {:#x}",
                call.index,
                window.index,
                window.at
            );
        }
        let hit = call.windows.iter().find(|window| window.index == 2).expect("hit record");
        hits += usize::from(hit.before != hit.after);
    }
    assert!(hits > 0, "no M_DE40X call hit: the records do not reach the hit path");
    eprintln!("E38Ops::M_DE40X: {} calls, {hits} hits", calls.len());
}

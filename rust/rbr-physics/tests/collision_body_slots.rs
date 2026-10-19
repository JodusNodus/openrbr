//! CBody::VfD8 (0x4e3070) against a scripted host: the calls' order and arguments, the hit path (compose, event, sample,
//! append) and the miss path. The game's own runs are checked by `harness/rust_step.py` (rbr-hybrid's bound.csv).

use rbr_physics::collision::body_slots::{self, D8Calls, D8Frame, E8Calls, F4Calls, F4Frame};

const FRAME: D8Frame = D8Frame {
    a: 0x100,
    b: 0x104,
    ray_a: 0x200,
    ray_b: 0x300,
    payload: 0x400,
    sub_block: 0x420,
    aux: 0x440,
    desc: 0x450,
    twin: 0x460,
};

/// Logs every call; the scripted answers are a grid of `count` edges, the pairs that hit (`hits`), and whether an
/// event can be allocated.
struct Script {
    count: u32,
    hits: Vec<u32>,
    allocate: bool,
    edge: u32,
    log: Vec<String>,
}

impl D8Calls for Script {
    fn ray_seeds(&mut self, ray: u32) {
        self.log.push(format!("seeds {ray:#x}"));
    }

    fn fetch_y(&mut self, matrix: u32, out: u32, worker: u32) {
        self.log.push(format!("fetch_y {matrix:#x} {out:#x} {worker:#x}"));
    }

    fn other_vf34(&mut self) -> u32 {
        self.log.push("vf34".into());
        0x34
    }

    fn other_vf30(&mut self) -> u32 {
        self.log.push("vf30".into());
        0x30
    }

    fn this_vf30(&mut self) -> u32 {
        self.log.push("this_vf30".into());
        0x3
    }

    fn combine(&mut self, out: u32, src: u32, worker: u32) {
        self.log.push(format!("combine {out:#x} {src:#x} {worker:#x}"));
    }

    fn sub_block_init(&mut self, block: u32) {
        self.log.push(format!("sub_block {block:#x}"));
    }

    fn grid_count(&mut self) -> u32 {
        self.count
    }

    fn grid_hit(&mut self, twin: u32, desc: u32, payload: u32) -> bool {
        let edge = self.edge;
        self.edge += 1;
        self.log.push(format!("grid_hit {twin:#x} {desc:#x} {payload:#x}"));
        self.hits.contains(&edge)
    }

    fn compose_a(&mut self, out: u32, payload: u32, worker: u32) {
        self.log.push(format!("compose {out:#x} {payload:#x} {worker:#x}"));
    }

    fn alloc_event(&mut self) -> u32 {
        self.log.push("alloc".into());
        if self.allocate {
            0x900
        } else {
            0
        }
    }

    fn init_full(&mut self, event: u32, aux: u32, payload: u32) -> u32 {
        self.log.push(format!("init_full {event:#x} {aux:#x} {payload:#x}"));
        event + 1
    }

    fn sample_b(&mut self, index: u32, event: u32) {
        self.log.push(format!("sample_b {index} {event:#x}"));
    }

    fn append(&mut self, event: u32) {
        self.log.push(format!("append {event:#x}"));
    }

    fn ray_clear(&mut self, ray: u32) {
        self.log.push(format!("clear {ray:#x}"));
    }
}

fn script(count: u32, hits: &[u32], allocate: bool) -> Script {
    Script { count, hits: hits.to_vec(), allocate, edge: 0, log: Vec::new() }
}

#[test]
fn vf_d8_without_edges_seeds_fetches_and_clears() {
    let mut calls = script(0, &[], true);
    assert_eq!(body_slots::record_edge_hits(&mut calls, &FRAME), 0);
    assert_eq!(
        calls.log,
        [
            "seeds 0x200",
            "seeds 0x300",
            "vf34",
            "fetch_y 0x100 0x200 0x34",
            "vf30",
            "fetch_y 0x104 0x300 0x30",
            "sub_block 0x420",
            "clear 0x300",
            "clear 0x200",
        ]
    );
}

/// The calls before the edge loop, and the clears after it.
const HEAD: [&str; 7] = [
    "seeds 0x200",
    "seeds 0x300",
    "vf34",
    "fetch_y 0x100 0x200 0x34",
    "vf30",
    "fetch_y 0x104 0x300 0x30",
    "sub_block 0x420",
];
const TAIL: [&str; 2] = ["clear 0x300", "clear 0x200"];

/// One edge's calls before the other body's test: the descriptors (`desc` from ray b's edge record, `twin` from ray
/// a's) and the test itself.
fn edge(offset: u32) -> Vec<String> {
    let b = 0x300 + offset;
    let a = 0x200 + offset;
    [
        "vf30".to_string(),
        format!("combine 0x450 {b:#x} 0x30"),
        "vf34".to_string(),
        format!("combine 0x460 {a:#x} 0x34"),
        "grid_hit 0x460 0x450 0x400".to_string(),
    ]
    .into()
}

fn lines(parts: &[&[&str]]) -> Vec<String> {
    parts.iter().flat_map(|p| p.iter().map(|l| l.to_string())).collect()
}

#[test]
fn vf_d8_records_a_hit_with_its_event_and_returns_one() {
    let mut calls = script(2, &[1], true);
    assert_eq!(body_slots::record_edge_hits(&mut calls, &FRAME), 1);
    let mut expected = lines(&[&HEAD]);
    expected.extend(edge(0x40));
    expected.extend(edge(0x50));
    expected.extend(lines(&[
        &[
            "this_vf30",
            "compose 0x440 0x400 0x3",
            "alloc",
            "init_full 0x900 0x440 0x400",
            "sample_b 1 0x901",
            "append 0x901",
        ],
        &TAIL,
    ]));
    assert_eq!(calls.log, expected);
}

#[test]
fn vf_d8_without_an_event_appends_null_and_returns_one() {
    let mut calls = script(1, &[0], false);
    assert_eq!(body_slots::record_edge_hits(&mut calls, &FRAME), 1);
    let mut expected = lines(&[&HEAD]);
    expected.extend(edge(0x40));
    expected
        .extend(lines(&[&["this_vf30", "compose 0x440 0x400 0x3", "alloc", "sample_b 0 0x0", "append 0x0"], &TAIL]));
    assert_eq!(calls.log, expected);
}

#[test]
fn vf_d8_misses_every_pair_and_returns_zero() {
    let mut calls = script(2, &[], true);
    assert_eq!(body_slots::record_edge_hits(&mut calls, &FRAME), 0);
    let mut expected = lines(&[&HEAD]);
    expected.extend(edge(0x40));
    expected.extend(edge(0x50));
    expected.extend(lines(&[&TAIL]));
    assert_eq!(calls.log, expected);
}

/// CBody::VfE8 (0x4e2f50) against a scripted body: the eight arguments of its slot 0xf4, the flags it copies on a hit,
/// and the two early returns (a zero record or last argument).
struct E8Script {
    answer: u32,
    flags: (u32, u32),
    log: Vec<String>,
}

impl E8Calls for E8Script {
    fn box_edge_hit(&mut self, a: u32, b: u32, record: u32) -> u32 {
        self.log.push(format!("box_edge_hit {a:#x} {b:#x} {record:#x}"));
        self.answer
    }

    fn take_grid_flags(&mut self) -> (u32, u32) {
        self.log.push("take_flags".into());
        self.flags
    }

    fn store_hit(&mut self, record: u32, flags: (u32, u32)) {
        self.log.push(format!("store {record:#x} {flags:?}"));
    }
}

#[test]
fn vf_e8_calls_the_slot_then_copies_the_grid_flags_on_a_hit() {
    let mut calls = E8Script { answer: 1, flags: (3, 5), log: Vec::new() };
    assert_eq!(body_slots::grid_hit(&mut calls, 0x100, 0x104, 0x400, 1), 1);
    assert_eq!(calls.log, ["box_edge_hit 0x100 0x104 0x400", "take_flags", "store 0x400 (3, 5)",]);
}

#[test]
fn vf_e8_miss_and_null_records_leave_the_grid_alone() {
    let mut calls = E8Script { answer: 0, flags: (3, 5), log: Vec::new() };
    assert_eq!(body_slots::grid_hit(&mut calls, 0x100, 0x104, 0x400, 1), 0);
    assert_eq!(body_slots::grid_hit(&mut calls, 0x100, 0x104, 0, 1), 0);
    assert_eq!(body_slots::grid_hit(&mut calls, 0x100, 0x104, 0x400, 0), 0);
    assert_eq!(calls.log.len(), 1, "only the miss calls the slot");
}

/// CBody::VfF4 (0x4e2e90) against a scripted body: the calls in order, the hit's record words, and the miss.
struct F4Script {
    hit: bool,
    log: Vec<String>,
}

impl F4Calls for F4Script {
    fn vf34(&mut self) -> u32 {
        self.log.push("vf34".into());
        0x500
    }

    fn vf30(&mut self) -> u32 {
        self.log.push("vf30".into());
        0x600
    }

    fn fetch_x(&mut self, matrix: u32, out: u32, t: u32) {
        self.log.push(format!("fetch_x {matrix:#x} {out:#x} {t:#x}"));
    }

    fn edge_edge(&mut self, from: u32, to: u32, hit: u32, t: u32, normal: u32, material: u32) -> bool {
        self.log.push(format!("edge_edge {from:#x} {to:#x} {hit:#x} {t:#x} {normal:#x} {material:#x}"));
        self.hit
    }

    fn store_hit(&mut self, f: u32, g: u32, h: u32) {
        self.log.push(format!("store_hit {f:#x} {g:#x} {h:#x}"));
    }

    fn compose_a(&mut self, transform: u32, point: u32, out: u32) {
        self.log.push(format!("compose_a {transform:#x} {point:#x} {out:#x}"));
    }

    fn compose_b(&mut self, transform: u32, point: u32, out: u32) {
        self.log.push(format!("compose_b {transform:#x} {point:#x} {out:#x}"));
    }
}

const F4_FRAME: F4Frame =
    F4Frame { a: 0x100, b: 0x104, c: 0x108, d: 0x10c, e: 0x110, f: 0x114, g: 0x118, h: 0x11c, from: 0x200, to: 0x210 };

#[test]
fn vf_f4_fetches_both_boxes_then_composes_on_a_hit() {
    let mut calls = F4Script { hit: true, log: Vec::new() };
    assert_eq!(body_slots::box_edge_hit(&mut calls, &F4_FRAME), 1);
    assert_eq!(
        calls.log,
        [
            "vf34",
            "fetch_x 0x100 0x200 0x500",
            "vf30",
            "fetch_x 0x104 0x210 0x600",
            "edge_edge 0x200 0x210 0x108 0x10c 0x110 0x118",
            "store_hit 0x114 0x118 0x11c",
            "vf30",
            "compose_a 0x600 0x108 0x108",
            "vf30",
            "compose_b 0x600 0x110 0x110",
        ]
    );
}

#[test]
fn vf_f4_miss_fetches_and_tests_only() {
    let mut calls = F4Script { hit: false, log: Vec::new() };
    assert_eq!(body_slots::box_edge_hit(&mut calls, &F4_FRAME), 0);
    assert_eq!(calls.log.len(), 5, "vf34, two fetches, the edge test");
    assert!(calls.log.last().unwrap().starts_with("edge_edge"));
}

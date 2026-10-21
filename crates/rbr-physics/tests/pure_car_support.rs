use rbr_physics::car_body::store::CarStore;
use rbr_physics::car_body::{layout, scratch_dispatch};
use rbr_physics::collision::face_sweep::{self, Bounds, Face, FaceGrid};
use rbr_physics::math::vec::{Vec3, Vec4};
use rbr_physics::mem::Mem;
use rbr_physics::rigid_body::host::{HostUnavailable, Vf54Host};

#[test]
fn get_30_reads_the_word_at_plus_30() {
    let mut mem = Mem::new();
    mem.set_u32(0x1030, 0xdead_beef);

    assert_eq!(mem.sort_key(0x1000), 0xdead_beef);
}

#[test]
fn time_keeper_diff_selects_current_or_finish_clock() {
    let mut mem = Mem::new();
    let keeper = 0x1000;
    let card = 0x2000;
    let current = 0x3000;
    mem.set_u32(keeper + 4, card);
    mem.set_u32(card + 0x58, 0);
    mem.map(card + 0x38, &10.25f64.to_le_bytes());
    mem.map(card + 0x50, &98.5f64.to_le_bytes());
    mem.map(current, &42.75f64.to_le_bytes());

    assert_eq!(mem.time_keeper_diff(keeper, current), layout::TimeKeeperDiff { origin: 10.25, end: 42.75 });

    mem.set_u32(card + 0x58, 1);
    assert_eq!(mem.time_keeper_diff(keeper, current), layout::TimeKeeperDiff { origin: 10.25, end: 98.5 });
}

struct Vf54Calls {
    car: u32,
    events: Vec<&'static str>,
}

impl Vf54Host for Vf54Calls {
    fn entity_scratch_tail(&mut self) -> Result<(), HostUnavailable> {
        self.events.push("entity");
        Ok(())
    }
}

impl scratch_dispatch::ScratchDispatchCalls for Vf54Calls {
    fn scratch_slot1(&mut self, scratch: u32) -> u32 {
        assert_eq!(scratch, self.car + 0x1690);
        self.events.push("scratch slot 1");
        0x7654_3210
    }
}

#[test]
fn vf54x_resets_the_scratch_then_dispatches_slot_one() {
    let car = 0x1000;
    let block = car + 0x280;
    let mut mem = Mem::new();
    mem.map(block, &[0xa5; 0x140]);
    let mut calls = Vf54Calls { car, events: Vec::new() };

    assert_eq!(layout::reset_scratch_and_dispatch(&mut calls, &mut mem, car), Ok(0x7654_3210));
    assert_eq!(calls.events, ["entity", "scratch slot 1"]);
    for offset in [0x20, 0x60, 0x70, 0x80, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0x100, 0x120] {
        assert_eq!(mem.read(block + offset, 0x10), [0; 0x10], "scratch +{offset:#x}");
    }
    for offset in [0x110, 0x130, 0x134] {
        assert_eq!(mem.u32(block + offset), 0, "scratch +{offset:#x}");
    }
    assert_eq!(mem.u32(block + 0x10), 0xa5a5_a5a5);
}

struct TriangleGrid {
    bounds: Bounds,
    vertices: [Vec3; 3],
}

impl FaceGrid for TriangleGrid {
    fn bounds(&mut self) -> Bounds {
        self.bounds
    }

    fn face_count(&mut self) -> u32 {
        1
    }

    fn face_is_valid(&mut self, index: u32) -> bool {
        assert_eq!(index, 0);
        true
    }

    fn face(&mut self, index: u32) -> Face {
        assert_eq!(index, 0);
        Face { vertices: [0, 1, 2] }
    }

    fn sample_indices(&mut self, _index: u32) -> [u8; 3] {
        unreachable!("the test grid has no sampled surface")
    }

    fn vertices(&mut self, indices: [u8; 3]) -> [Vec3; 3] {
        assert_eq!(indices, [0, 1, 2]);
        self.vertices
    }

    fn material(&mut self) -> u32 {
        0x5a
    }

    fn sample_dimensions(&mut self) -> Option<(u16, u16)> {
        None
    }

    fn vertex_samples(&mut self, _indices: [u8; 3]) -> [[u8; 2]; 3] {
        unreachable!("the test grid has no sampled surface")
    }

    fn sample_bytes(&mut self, _first: i32, _second: i32) -> [u8; 2] {
        unreachable!("the test grid has no sampled surface")
    }
}

fn triangle_grid() -> TriangleGrid {
    TriangleGrid {
        bounds: Bounds { centre: Vec3::default(), half: Vec3 { x: 2.0, y: 2.0, z: 2.0 } },
        vertices: [
            Vec3 { x: -1.0, y: -1.0, z: 0.0 },
            Vec3 { x: 1.0, y: -1.0, z: 0.0 },
            Vec3 { x: 0.0, y: 1.0, z: 0.0 },
        ],
    }
}

#[test]
fn edge_edge_reports_a_segment_triangle_hit() {
    let mut grid = triangle_grid();
    let hit =
        face_sweep::sweep_faces(&mut grid, Vec4 { x: 0.0, y: 0.0, z: 1.0, w: 8.0 }, Vec3 { x: 0.0, y: 0.0, z: -1.0 })
            .expect("segment crosses the triangle");

    assert!((hit.distance - 0.5).abs() < 1.0e-6);
    assert!((hit.point.z).abs() < 1.0e-6);
    assert_eq!(hit.point.w, 8.0);
    assert_eq!(hit.material, 0x5a);
    assert!(hit.samples.is_none());
}

#[test]
fn edge_edge_rejects_a_segment_outside_grid_bounds() {
    let mut grid = triangle_grid();
    let hit =
        face_sweep::sweep_faces(&mut grid, Vec4 { x: 5.0, y: 0.0, z: 1.0, w: 0.0 }, Vec3 { x: 5.0, y: 0.0, z: -1.0 });

    assert!(hit.is_none());
}

#[test]
fn get_15f8_reads_the_float_at_plus_15f8() {
    let mut mem = Mem::new();
    mem.map(0x1000, &[0; 0x1600]);
    mem.set_f32(0x15f8 + 0x1000, 27.5);

    assert_eq!(mem.forward_speed(0x1000), 27.5);
}

#[test]
fn damage_select_reads_the_host_word_or_the_global() {
    let mut mem = Mem::new();
    let (flow, host) = (0x1000, 0x2000);
    mem.map(flow, &[0; 0xd00]);
    mem.map(host, &[0; 0x17860]);
    mem.set_u32(flow + 0xC98, host);
    mem.set_u32(host + 0x17854, 0xdead_beef);
    mem.set_u32(0x0166_084C, 0x1234);

    mem.set_u8(host, 0x7B);
    assert_eq!(mem.damage_select(flow), 0xdead_beef);
    mem.set_u8(host + 1, 1);
    assert_eq!(mem.damage_select(flow), 0x1234);
}

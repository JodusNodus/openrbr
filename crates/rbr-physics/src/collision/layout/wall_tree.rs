//! Brake-wall query helpers from E33.

use crate::mem::Mem;

/// CBrakeWall::Vf88 (0x4e38e0) (name ours): the wall reports itself as available.
pub const fn is_available() -> u32 {
    1
}

/// CBrakeWall::Vf110 (0x4e3860) (name ours): the wall reports itself as active.
pub const fn is_active() -> u32 {
    1
}

/// CBrakeWall::Vf11C (0x4e3870) (name ours): copy the inline root node's horizontal bounds
/// into two four-lane vectors, preserving the retail store order and W lanes.
pub fn copy_root_bounds(mem: &mut Mem, wall: u32, lower: u32, upper: u32) {
    let min_x = mem.f32(wall + 0x64);
    let min_y = mem.f32(wall + 0x68);
    let max_x = mem.f32(wall + 0x6c);
    let max_y = mem.f32(wall + 0x70);

    mem.set_f32(lower + 4, min_y - max_y);
    mem.set_f32(lower + 8, -999_999.0);
    mem.set_f32(lower, min_x - max_x);
    mem.set_f32(upper + 4, max_y + min_y);
    mem.set_f32(upper + 8, 999_999.0);
    mem.set_f32(upper, min_x + max_x);
}

fn query(mem: &mut Mem, wall: u32, shape: u32, collector: u32, box_query: bool) -> u32 {
    let mut stack = [0u32; 257];
    stack[1] = wall + 0x64;
    mem.set_u32(collector + 0x190, 0);
    let mut top = 1usize;

    loop {
        let node = stack[top];
        let mut next = top - 1;
        if mem.u32(node + 0x10) == 0 {
            let children = mem.u32(node + 0x14);
            if children != 0 {
                if child_hit(mem, shape, children, box_query) {
                    stack[top] = children;
                    next = top;
                }
                let second = children + 0x18;
                if child_hit(mem, shape, second, box_query) {
                    stack[next + 1] = second;
                    next += 1;
                }
            }
        } else {
            let count = mem.u32(collector + 0x190);
            mem.set_u32(collector + count * 4, node);
            mem.set_u32(collector + 0x190, count + 1);
        }

        if next == 0 {
            break;
        }
        top = next;
    }
    u32::from(mem.u32(collector + 0x190) != 0)
}

fn child_hit(mem: &Mem, shape: u32, child: u32, box_query: bool) -> bool {
    let dx = (mem.f32(shape) - mem.f32(child)).abs();
    let dy = (mem.f32(shape + 4) - mem.f32(child + 4)).abs();
    if box_query {
        dx < mem.f32(child + 8) + mem.f32(shape + 8) && dy < mem.f32(child + 12) + mem.f32(shape + 12)
    } else {
        dx < mem.f32(child + 8) && dy < mem.f32(child + 12)
    }
}

fn query_any(mem: &Mem, wall: u32, shape: [f32; 4]) -> bool {
    let mut stack = [0u32; 257];
    stack[1] = wall + 0x64;
    let mut top = 1usize;
    let mut found = false;

    loop {
        let node = stack[top];
        let mut next = top - 1;
        if mem.u32(node + 0x10) == 0 {
            let children = mem.u32(node + 0x14);
            if children != 0 {
                if child_hit_values(mem, shape, children) {
                    stack[top] = children;
                    next = top;
                }
                let second = children + 0x18;
                if child_hit_values(mem, shape, second) {
                    stack[next + 1] = second;
                    next += 1;
                }
            }
        } else {
            found = true;
        }

        if next == 0 {
            break;
        }
        top = next;
    }
    found
}

fn child_hit_values(mem: &Mem, shape: [f32; 4], child: u32) -> bool {
    (shape[0] - mem.f32(child)).abs() < mem.f32(child + 8) + shape[2]
        && (shape[1] - mem.f32(child + 4)).abs() < mem.f32(child + 12) + shape[3]
}

/// CBrakeWall::QueryPoint (0x4e3dc0): collect leaf nodes containing a point.
pub fn query_point(mem: &mut Mem, wall: u32, point: u32, collector: u32) -> u32 {
    query(mem, wall, point, collector, false)
}

/// CBrakeWall::QueryBox (0x4e3f10): collect leaf nodes overlapping a box.
pub fn query_box(mem: &mut Mem, wall: u32, bounds: u32, collector: u32) -> u32 {
    query(mem, wall, bounds, collector, true)
}

/// CBrakeWall::Vf15C (0x4e4070) (name ours): query the wall with a ten-unit box centered
/// at the point's X/Y coordinates.
pub fn query_xy_box(mem: &Mem, wall: u32, point: u32) -> u32 {
    let bounds = [mem.f32(point), mem.f32(point + 4), 10.0, 10.0];
    u32::from(query_any(mem, wall, bounds))
}

/// CBrakeWall::AddCount (0x4e3810): relocate the relative pointers in a
/// serialized wall quadtree using the data block's base address as the delta.
pub fn add_count(mem: &mut Mem, wall: u32, node: u32, delta: u32) {
    let _ = wall;
    let mut current = node;
    let mut items = mem.u32(current + 0x10);
    loop {
        if items != 0 {
            let relative = mem.u32(current + 0x14);
            mem.set_u32(current + 0x14, relative.wrapping_add(delta));
            return;
        }
        let relative_child = mem.u32(current + 0x14);
        if relative_child == 0 {
            break;
        }
        let child = relative_child.wrapping_add(delta);
        mem.set_u32(current + 0x14, child);
        add_count(mem, wall, child, delta);
        items = mem.u32(child + 0x28);
        current = child + 0x18;
    }
}

/// The calls `E34WallVf::VfCC` (0x4e42f0) makes, each at its callee's entry stack pointer `sp`: the wall's slot 12
/// (`Vf30`, its frame), `E33Mat34::FetchX` (0x51b6c0, thiscall on a corner with `(out, frame)`) and
/// `CBrakeWall::QueryBox` (0x4e3f10, thiscall on the wall with `(bounds, collector)`).
pub trait WallVfCcHost {
    fn frame(&mut self, sp: u32, wall: u32) -> u32;
    fn fetch_x(&mut self, sp: u32, corner: u32, out: u32, frame: u32);
    fn query_box(&mut self, sp: u32, wall: u32, bounds: u32, collector: u32) -> u32;
}

/// The call `CBrakeWall::VfF0` (0x4e3970) makes at its callee's entry stack pointer `sp`: the wall's slot 0xe8 (`VfE8`)
/// with `(a, b, 0, out, 1)`, which fills the four vectors at `out` on a hit (eax non-zero).
pub trait VfF0Host {
    fn vf_e8(&mut self, sp: u32, wall: u32, a: u32, b: u32, out: u32) -> u32;
}

/// The original's call out of `E34WallVf::Raycast` (0x4e4570): the penetration signal's emit (0x4e4490, thiscall on
/// the signal manager's emit object, one stack word: the buffer).
pub trait RaycastCalls {
    /// `CMultiSignal1<const CSigCarBrakeWallPenetration&>::Emit` (0x4e4490) on `this`, with the 28-byte buffer at
    /// `out`.
    fn emit_penetration(&mut self, this: u32, out: u32);
}

/// The calls `CBrakeWall::VfE8` (0x4e3a10) makes at its callee's entry stack pointer `sp`: `E33Mat34::FetchX`
/// (0x51b6c0) on each end point's matrix, into `out`, with the identity worker.
pub trait VfE8Host {
    fn fetch_x(&mut self, sp: u32, matrix: u32, out: u32, worker: u32);
}

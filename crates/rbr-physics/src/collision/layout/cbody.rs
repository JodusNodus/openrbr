//! `CBody`'s views and calls: the body, its state, the constructors, the grid selection and the hit-record frames
//! of its slots.

use crate::mem::Mem;

use super::body_hosts::{self as body, ClearGridHost, Vf40Host, Vf44Host, VfCcHost};
use super::body_slots;
use super::col_grids::ColGrid;
use super::state::{
    StateHost, BODY_SCRATCH_VTABLE, BODY_STATE_VTABLE, CREAL_NAME, CREAL_VTABLE, CSTATE_OWNER_OFFSET,
    CSTATE_REAL_OFFSET,
};
use super::vec_ops::Vec4Pair;
use crate::collision::overlap;
pub use crate::entities::layout::ROOT_IDENTITY as IDENTITY_WORKER_SLOT;
use crate::math::vec::Vec4;
pub use crate::physics_core::layout::vtable_target as vtable_slot;

/// CBody's vtable, which CBody::CBody (0x4e3460) stores at the body's first word.
const CBODY_VTABLE: u32 = 0x737c9c;

/// The xyz sentinels that CState::Unk44 writes to its two vectors (`0x4974_23fe` is about +1e6, `0xc974_23fe` about
/// -1e6).
const CSTATE_WALL_RESET_POSITIVE: u32 = 0x4974_23fe;
const CSTATE_WALL_RESET_NEGATIVE: u32 = 0xc974_23fe;

/// The `CBody` members `Body` reads (`collision/README.md` (git 9c28c0b8)): the grid pointer that `CBody::Vf44` stores, and the bounds.
pub(in crate::collision) const BODY_GRID: u32 = 0x64;
const BODY_LOWER: u32 = 0x80;
const BODY_UPPER: u32 = 0x90;

/// A CBody view. The game stores its collision grid and state bounds separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    base: u32,
}

impl Body {
    pub const fn new(base: u32) -> Self {
        Self { base }
    }

    pub fn state(self) -> u32 {
        self.base + 0x70
    }

    pub fn grid(self, mem: &Mem) -> u32 {
        mem.u32(self.base + BODY_GRID)
    }

    pub fn set_grid(self, mem: &mut Mem, grid: u32) {
        mem.set_u32(self.base + BODY_GRID, grid);
    }

    /// The CState bounds at CBody+0x80 and CBody+0x90.
    pub fn bounds(self, mem: &Mem) -> Vec4Pair {
        Vec4Pair::load(mem, self.base + BODY_LOWER)
    }

    /// Where the bounds are: CBody+0x80 (lower) and CBody+0x90 (upper).
    pub fn bounds_at(self) -> (u32, u32) {
        (self.base + BODY_LOWER, self.base + BODY_UPPER)
    }
}

/// CBody::Vf10C (0x4e11b0): whether the body has a grid.
pub fn body_has_grid(mem: &Mem, body: u32) -> bool {
    body::has_grid(Body::new(body).grid(mem))
}

/// CBody::Vf110 (0x4e11c0): whether the body's grid is active (its byte +0x68b, read only when there is a grid).
pub fn body_grid_active(mem: &Mem, body: u32) -> bool {
    let grid = Body::new(body).grid(mem);
    body::grid_active((grid != 0).then(|| ColGrid::new(grid).active(mem)))
}

/// CBody::Vf40 (0x4e1120) (name ours): the E33 state reset on the body's state, then CEntity's base walk. Both are host
/// calls.
pub fn reset_state_and_base(host: &mut impl Vf40Host, body: u32) -> u32 {
    let _ = host.state_unka44(Body::new(body).state());
    host.entity_vf40(body)
}

/// CBody::CState::Unk44 (0x4e33e0): reset the xyz sentinels in both vectors at `state + 0x10` and `state + 0x20`, then
/// the owner's slot-12 transform (host), and the wall sync (host) when the owner has a wall.
pub fn state_unka44(host: &mut impl StateHost, mem: &mut Mem, state: u32) -> u32 {
    let vec_a = state + 0x10;
    let vec_b = state + 0x20;
    for lane in 0..3 {
        mem.set_u32(vec_a + lane * 4, CSTATE_WALL_RESET_POSITIVE);
        mem.set_u32(vec_b + lane * 4, CSTATE_WALL_RESET_NEGATIVE);
    }

    let owner = mem.u32(state + CSTATE_OWNER_OFFSET);
    let eax = host.owner_vf30(owner);
    let wall = mem.u32(owner + 0x64);
    if wall == 0 {
        eax
    } else {
        host.wall_sync(wall, eax, vec_a, vec_b)
    }
}

/// CBody::CState::CState (0x4e1070): the state's name, its owner (`body`, when `body`'s flags have bit 2), and the
/// embedded default CReal.
pub fn state_ctor(mem: &mut Mem, state: u32, body: u32, name: u32) -> u32 {
    mem.set_u32(state + 4, name);
    mem.set_u32(state + 8, 0);
    mem.set_u32(state, BODY_STATE_VTABLE);

    mem.set_u32(state + CSTATE_REAL_OFFSET, CREAL_VTABLE);
    mem.set_u32(state + CSTATE_REAL_OFFSET + 4, CREAL_NAME);
    mem.set_u32(state + CSTATE_REAL_OFFSET + 8, 0);
    mem.set_u32(state + CSTATE_REAL_OFFSET + 12, 0);

    let owner = if body != 0 && mem.u32(body + 0x60) & 2 != 0 { body } else { 0 };
    mem.set_u32(state + CSTATE_OWNER_OFFSET, owner);
    state
}

/// Calls outside CBody::CBody (0x4e3460): the E43 base constructor, which the body's flags reach with bit 2 set.
pub trait CtorHost {
    fn entity_ctor(&mut self, mem: &mut Mem, body: u32, definition: u32, frame: u32, parent: u32, flags: u32) -> u32;
}

/// CBody::CBody (0x4e3460): the E43 base constructor (host) with bit 2 set in the flags, then the state, the scratch
/// vtable, and a cleared grid.
pub fn ctor(
    host: &mut impl CtorHost,
    mem: &mut Mem,
    body: u32,
    definition: u32,
    frame: u32,
    parent: u32,
    flags: u32,
) -> u32 {
    let _ = host.entity_ctor(mem, body, definition, frame, parent, flags | 2);
    let name = if definition == 0 { 0 } else { mem.u32(definition + 4) };
    state_ctor(mem, Body::new(body).state(), body, name);
    mem.set_u32(body, CBODY_VTABLE);
    mem.set_u32(body + 0xc0, BODY_SCRATCH_VTABLE);
    Body::new(body).set_grid(mem, 0);
    body
}

/// CBody::ClearGrid (0x4e11a0): synchronize bounds only when a grid is attached (`u32::MAX` otherwise).
pub fn clear_grid(host: &mut impl ClearGridHost, mem: &Mem, body: u32, source: u32, lower: u32, upper: u32) -> u32 {
    let grid = Body::new(body).grid(mem);
    if grid == 0 {
        u32::MAX
    } else {
        host.sync_grid(grid, source, lower, upper)
    }
}

/// CBody::VfCC (0x4e1230): fetches both corners (host, into `scratch` and `scratch + 16`), then tests them against the
/// body's bounds. Returns the flag in eax.
pub fn aabb_pretest(host: &mut impl VfCcHost, mem: &mut Mem, body: u32, first: u32, second: u32, scratch: u32) -> u32 {
    let worker = mem.u32(IDENTITY_WORKER_SLOT);
    host.fetch_x(first, scratch, worker);
    host.fetch_x(second, scratch + 16, worker);
    let bounds = Body::new(body).bounds(mem);
    let first = Vec4::load(mem, scratch);
    let second = Vec4::load(mem, scratch + 16);
    u32::from(overlap::corners_overlap_bounds(bounds.lower, bounds.upper, first, second))
}

/// CBody::Vf44 (0x4e1140) (name ours): selects the grid (host), stores it at +0x64, then runs CEntity's slot-17
/// implementation (host).
pub fn select_grid(host: &mut impl Vf44Host, mem: &mut Mem, body: u32, object: u32) -> u32 {
    let grid = host.select_table(object);
    Body::new(body).set_grid(mem, grid);
    host.entity_vf44(body, object)
}

/// The frame of CBody::VfD8 (`body_slots::record_edge_hits`) below its entry stack pointer `esp`, 16-byte aligned: the
/// two ray buffers (0x6b0 bytes each) and the payload, aux matrix and descriptors, all above the callees (the host
/// places them 0x1000 below `esp`).
pub fn record_edge_hits_frame(esp: u32, a: u32, b: u32) -> body_slots::D8Frame {
    let base = esp & !0xf;
    let payload = base - 0x80;
    body_slots::D8Frame {
        a,
        b,
        ray_a: base - 0xde0,
        ray_b: base - 0x730,
        payload,
        sub_block: payload + 0x20,
        aux: base - 0x40,
        desc: base - 0x30,
        twin: base - 0x20,
    }
}

/// The eight arguments CBody::VfE8 pushes for its slot 0xf4 (`VfF4`), in the original's order: `a`, `b`, the record
/// and the record's words at +0x34, +0x10, +0x28, +0x20 and +0x24.
pub fn grid_hit_args(a: u32, b: u32, record: u32) -> [u32; 8] {
    [a, b, record, record + 0x34, record + 0x10, record + 0x28, record + 0x20, record + 0x24]
}

/// The grid's flag bytes (+0x688, +0x689) of the body's grid, read and cleared (CBody::VfE8's hit path).
pub fn take_grid_flags(mem: &mut Mem, body: u32) -> (u32, u32) {
    let grid = Body::new(body).grid(mem);
    let flags = (u32::from(mem.u8(grid + 0x688)), u32::from(mem.u8(grid + 0x689)));
    mem.set_u8(grid + 0x688, 0);
    mem.set_u8(grid + 0x689, 0);
    flags
}

/// CBody::VfE8's hit record: the two grid flags at +0x2c and +0x30 and the body at +0x38.
pub fn store_grid_hit(mem: &mut Mem, record: u32, flags: (u32, u32), body: u32) {
    mem.set_u32(record + 0x2c, flags.0);
    mem.set_u32(record + 0x30, flags.1);
    mem.set_u32(record + 0x38, body);
}

/// The frame of CBody::VfF4 (`body_slots::box_edge_hit`) below its entry stack pointer `esp`: the two box temporaries,
/// above the callees (which run below `box_edge_hit_sp`).
pub fn box_edge_hit_frame(esp: u32, args: [u32; 8]) -> body_slots::F4Frame {
    let base = (esp - 4) & !0xf;
    let [a, b, c, d, e, f, g, h] = args;
    body_slots::F4Frame { a, b, c, d, e, f, g, h, from: base - 0x10, to: base - 0x20 }
}

/// The stack pointer below CBody::VfF4's saved registers (`esi`, `edi`, `ebx`) for the entry stack pointer `esp`: the
/// callees' return slots are below it (FetchX at -12, EdgeEdge at -0x1c, ComposeA and ComposeB at -0x10).
pub fn box_edge_hit_sp(esp: u32) -> u32 {
    ((esp - 4) & !0xf) - 0x30
}

/// CBody::VfF4's hit record: `h` takes the word at `g`, and `f` is cleared to 0.0.
pub fn store_vf_f4_hit(mem: &mut Mem, f: u32, g: u32, h: u32) {
    let word = mem.u32(g);
    mem.set_u32(h, word);
    mem.set_f32(f, 0.0);
}

/// The grid's edge count (+0x68d) of the body's grid (+0x64), which CBody::VfD8 walks.
pub fn grid_edge_count(mem: &Mem, body: u32) -> u32 {
    let grid = Body::new(body).grid(mem);
    u32::from(mem.u8(grid + 0x68d))
}

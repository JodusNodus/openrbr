//! The host calls of the body-contact walks (`body_vf_d0`, `body_vf_dc`).

use crate::collision::grid_contact::ContactKind;

/// Calls out of CBody::VfD0 (0x4e1eb0), each entered at `sp` as the original's. The vectors they take and write are
/// in VfD0's frame; the slot calls are on the entity's vtable.
pub trait VfD0Host {
    /// `E38RaySeeds` (0x4f6550) on a grid in the frame.
    fn ray_seeds(&mut self, sp: u32, grid: u32);
    /// `E38RayClear` (0x4f6740) on a grid in the frame.
    fn ray_clear(&mut self, sp: u32, grid: u32);
    /// `E33Mat34::CollideInit` (0x51bd30), thiscall on the other entity with `(body, 0, list)`: eax.
    fn collide_init(&mut self, sp: u32, other: u32, body: u32, list: u32) -> u32;
    /// Slot 0x30 (`CEntity::Vf30`), thiscall on an entity: its transform.
    fn transform(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0x110 (`CBody::Vf110` for a body), thiscall on an entity: eax, non-zero when its grid is active.
    fn grid_active(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0x118 (`CBody::Vf118` for a body), thiscall on an entity: its grid.
    fn grid(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0x80 (`E43Vf80`), thiscall on an entity: eax.
    fn vf80(&mut self, sp: u32, entity: u32) -> u32;
    /// Slot 0xf0, thiscall on the other entity with `(desc_a, desc_b, hit, u, normal, t, material, material)`: eax,
    /// non-zero for a contact.
    fn vf_f0(&mut self, sp: u32, entity: u32, args: [u32; 8]) -> u32;
    /// Slot 1 (`Append`), thiscall on the event list with the event.
    fn append(&mut self, sp: u32, list: u32, event: u32);
    /// `E33Mat34::FetchY` (0x51b780), thiscall on `matrix` with `(out, transform)`.
    fn fetch_y(&mut self, sp: u32, matrix: u32, out: u32, transform: u32);
    /// `E33Mat34::FetchX` (0x51b6c0), thiscall on `matrix` with `(out, transform)`.
    fn fetch_x(&mut self, sp: u32, matrix: u32, out: u32, transform: u32);
    /// `E33Mat34::Combine` (0x51ab90), thiscall on `desc` with `(point, transform)`.
    fn combine(&mut self, sp: u32, desc: u32, point: u32, transform: u32);
    /// `E33Mat34::CombineTwin` (0x51ab70), thiscall on `desc` with `(point, transform)`.
    fn combine_twin(&mut self, sp: u32, desc: u32, point: u32, transform: u32);
    /// `ComposeA` (0x51b4c0), thiscall on `transform` with `(point, other, out)`.
    fn compose_a(&mut self, sp: u32, transform: u32, point: u32, other: u32, out: u32);
    /// `ComposeB` (0x51b600), thiscall on `transform` with `(point, other, out)`.
    fn compose_b(&mut self, sp: u32, transform: u32, point: u32, other: u32, out: u32);
    /// `E37GridOps::M6610` (0x4f6610), thiscall on the frame's grid with `(src, a, b)`.
    fn grid_copy(&mut self, sp: u32, grid: u32, src: u32, a: u32, b: u32);
    /// `E37GridOps::EdgeEdge` (0x4f8280), thiscall on the frame's grid with `(from, to, hit, t, normal, material)`:
    /// eax, non-zero for a hit.
    #[allow(clippy::too_many_arguments)]
    fn edge_edge(
        &mut self,
        sp: u32,
        grid: u32,
        from: u32,
        to: u32,
        hit: u32,
        t: u32,
        normal: u32,
        material: u32,
    ) -> u32;
    /// `E01Vec4::Normalize` (0x43c6a0), thiscall on the vector.
    fn normalize(&mut self, sp: u32, vector: u32);
    /// `E21Vec3::Add` (0x4a19e0), thiscall on `a` with `(out, b)`: eax (`out`).
    fn add(&mut self, sp: u32, a: u32, out: u32, b: u32) -> u32;
    /// `E09Vec4::Sub` (0x465da0), thiscall on `a` with `(out, b)`: eax (`out`).
    fn sub(&mut self, sp: u32, a: u32, out: u32, b: u32) -> u32;
    /// `E33MakeEvent` (0x4e0d00), cdecl `(kind, hit, 0, body, other, at, &g_8ef7e0, normal, 0)`: the event.
    #[allow(clippy::too_many_arguments)]
    fn make_event(&mut self, sp: u32, kind: ContactKind, hit: u32, body: u32, other: u32, at: u32, normal: u32) -> u32;
    /// `CColGrid::SampleB` (0x4e0950), thiscall on the grid with `(index, out_a, out_b)`.
    fn sample_b(&mut self, sp: u32, grid: u32, index: u32, out_a: u32, out_b: u32);
    /// `CColGrid::SampleA` (0x4e0840), thiscall on the grid with `(index, t, out_a, out_b)`.
    fn sample_a(&mut self, sp: u32, grid: u32, index: u32, t: f32, out_a: u32, out_b: u32);
}

/// The calls CBody::VfDC (0x4e2850) makes beyond VfD0's, each entered at `sp` as the original's.
pub trait VfDcHost: VfD0Host {
    /// Slot 0x34 (`Vf34`), thiscall on an entity: its second transform.
    fn vf34(&mut self, sp: u32, entity: u32) -> u32;
    /// `E37GridOps::EdgePair` (0x4f8560), thiscall on the frame's grid with ten words (two edges' ends, the hit, t, the
    /// normal, a scratch word, the grid again and 0): eax, non-zero for a hit.
    fn edge_pair(&mut self, sp: u32, grid: u32, args: [u32; 10]) -> u32;
    /// `E33MakeEvent` (0x4e0d00), cdecl `(1, hit, t, body, other, at, &g_8ef7e0, normal, 0)`: the event. `t` is the
    /// word EdgePair left.
    #[allow(clippy::too_many_arguments)]
    fn pair_event(&mut self, sp: u32, hit: u32, t: u32, body: u32, other: u32, at: u32, normal: u32) -> u32;
}

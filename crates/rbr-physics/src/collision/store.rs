//! The collision subsystem's store (`crate::store`): the layout's load and store groups. `Mem` implements it in
//! `layout/mem_store.rs`.

use super::brake_wall::{EdgeCross, SegmentHit};
use super::contact_solver::{ContactMaterial, ContactRow, RowEvent};
use super::face_sweep::{Bounds, Face};
use super::obb::{ObbContact, ObbFrame, SphereContact};
use super::overlap::SatBox;
use crate::math::vec::{Vec3, Vec4};
use crate::store::Raw;

/// A collision event's two sides, named by the body they belong to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The event's body (`+0xa0`), material ids at `+0xac`.
    Body,
    /// The event's partner (`+0x98`), material ids at `+0x80`.
    Partner,
}

/// A tree node's flag word (`+0x18`: the face count in bits 0 to 20, the table index from bit 22) and its link
/// (`+0x1c`: the children, or the first face).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub word: u32,
    pub link: u32,
}

impl TreeNode {
    pub fn faces(self) -> u32 {
        self.word & 0x1f_ffff
    }

    pub fn table(self) -> u32 {
        self.word >> 22
    }
}

/// The record a mesh query writes: the point, the normal, the plane offset, the two channel sums, the two surface
/// bytes, the ray parameter and the two face flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceRecord {
    pub point: Vec3,
    pub normal: Vec3,
    pub plane_offset: f32,
    pub channels: [f32; 2],
    pub surfaces: [u32; 2],
    pub t: f32,
    pub flags: [u32; 2],
}

/// The nearest face of a mesh ray: its normal, face, vertex table, ray parameter and barycentric coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub normal: Vec3,
    pub face: u32,
    pub table: u32,
    pub t: f32,
    pub u: f32,
    pub v: f32,
}

/// An event's id words: the first pair (`+0x8c`, `+0x90`) and the second (`+0xa4`, `+0xa8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventIds {
    pub first: [u32; 2],
    pub second: [u32; 2],
}

/// What `correct_penetrations` reads of an event: its flags, plane and point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventPlane {
    pub flags: u32,
    pub plane: Vec4,
    pub point: Vec3,
}

/// The solver (`CCollisionSolver`), its event list, contact rows and offset rows, and the bodies and events they
/// name. `solver`, `event`, `body`, `rows` and `row` are object addresses.
pub trait CollisionStore: Raw {
    /// A 16-byte vector at `at` (the caller's frame or an out pointer), and its store.
    fn vec4(&self, at: u32) -> Vec4;
    fn store_vec4(&mut self, at: u32, value: Vec4);
    /// The function at `object`'s vtable byte offset `slot`.
    fn vtable_slot(&self, object: u32, slot: u32) -> u32;

    // The solver's event list.
    fn first_event(&self, solver: u32) -> u32;
    fn event_list_len(&self, solver: u32) -> u32;
    fn event_count(&self, solver: u32) -> u32;
    fn next_event(&self, event: u32) -> u32;
    /// Whether the event's slot 0x1c is `CCollisionEvent::Vf1Cx`, which the release and drain ports assume.
    fn event_releases_to_pool(&self, event: u32) -> bool;
    /// `list::drain` of the solver's event list with the pool flag.
    fn drain_event_list(&mut self, solver: u32);
    /// `list::release`: counts the event off the solver's list and unlinks it when it is on it.
    fn release_event(&mut self, solver: u32, event: u32);
    /// `event_pool::vf1cx` with the pool flag: the event goes back to the free list.
    fn pool_event(&mut self, event: u32);

    // An event's fields.
    fn event_body(&self, event: u32) -> u32;
    fn event_partner(&self, event: u32) -> u32;
    fn event_flags(&self, event: u32) -> u32;
    fn store_event_flags(&mut self, event: u32, flags: u32);
    fn event_suppressed(&self, event: u32) -> bool;
    /// `CCollisionEvent::Get94`.
    fn event_time(&self, event: u32) -> f32;
    fn event_plane(&self, event: u32) -> EventPlane;
    /// The event's plane vector alone (`+0x70`).
    fn event_normal(&self, event: u32) -> Vec4;
    /// The event's material refresh from its bodies' grid tags.
    fn refresh_event_materials(&mut self, event: u32);
    /// Whether the material of the event's id on `side` has flag bit 3.
    fn side_suppresses(&self, event: u32, side: Side) -> bool;
    /// What `contact_rows` reads of the event.
    fn row_event(&self, event: u32) -> RowEvent;
    /// The tag of the event's material and the reals of its entry.
    fn event_material(&self, event: u32) -> (u32, ContactMaterial);
    /// The address of the event's contact point `contact_rows` composes for `side` (0 the partner's, else the body's).
    fn event_other_point(&self, event: u32, side: u32) -> u32;

    // A body.
    /// Whether the body takes part in the solve (flag byte `+0x60`, bit 2).
    fn body_active(&self, body: u32) -> bool;
    fn body_progress(&self, body: u32) -> f32;
    fn body_position(&self, body: u32) -> Vec4;
    fn store_body_position(&mut self, body: u32, value: Vec4);
    fn body_contact_rows(&self, body: u32) -> u32;
    fn set_body_contact_rows(&mut self, body: u32, rows: u32);
    fn body_offset_row(&self, body: u32) -> u32;
    fn set_body_offset_row(&mut self, body: u32, row: u32);
    /// The root frame `ComposeA` is called on.
    fn frame_root(&self) -> u32;

    // The solver's contact rows.
    fn solver_contact_count(&self, solver: u32) -> u32;
    fn set_solver_contact_count(&mut self, solver: u32, count: u32);
    /// The body of the solver's contact row `index`.
    fn solver_contact_body(&self, solver: u32, index: u32) -> u32;
    /// The address of the solver's contact row `index`.
    fn solver_contact_row(&self, solver: u32, index: u32) -> u32;
    /// Starts a contact row for `body` at `rows`: its body, an empty count, and the body's link to it.
    fn open_contact_row(&mut self, rows: u32, body: u32);
    fn contact_row_count(&self, rows: u32) -> u32;
    fn set_contact_row_count(&mut self, rows: u32, count: u32);
    fn store_contact_row(&mut self, rows: u32, index: u32, row: &ContactRow);

    // The solver's offset rows.
    fn solver_offset_count(&self, solver: u32) -> u32;
    fn set_solver_offset_count(&mut self, solver: u32, count: u32);
    /// The address of the solver's offset row `index`.
    fn solver_offset_row(&self, solver: u32, index: u32) -> u32;
    fn offset_row_body(&self, row: u32) -> u32;
    fn offset_row_count(&self, row: u32) -> u32;
    fn set_offset_row_count(&mut self, row: u32, count: u32);
    fn offset(&self, row: u32, index: u32) -> Vec4;
    fn store_offset(&mut self, row: u32, index: u32, value: Vec4);
    /// Starts an offset row for `body` with its one offset in place: the body, the body's link and the count of 1.
    fn open_offset_row(&mut self, row: u32, body: u32);

    // The body contact walks (`CBody::VfD0`, `VfDC`): the other entity, the world flags and the grids.
    fn entity_flags(&self, entity: u32) -> u8;
    /// The world settings' contact flags (bit 2 point contacts, bit 3 edge contacts).
    fn world_flags(&self) -> u32;
    fn body_grid(&self, body: u32) -> u32;
    fn grid_edge_count(&self, grid: u32) -> u8;
    fn grid_pair_count(&self, grid: u32) -> u8;
    fn grid_status_flag(&self, grid: u32) -> u8;
    /// The address of the grid's edge vector `index`.
    fn grid_edge(&self, grid: u32, index: u32) -> u32;
    /// The six bytes of the grid's edge record `index`: the two vertices and the sample indices.
    fn grid_pair(&self, grid: u32, index: u32) -> [u8; 6];
    /// The body's grid flag bytes (`+0x688`, `+0x689`), read and cleared.
    fn take_body_grid_flags(&mut self, body: u32) -> (u32, u32);
    /// The addresses of the event's two sample words, which `SampleA` and `SampleB` write.
    fn event_sample_slots(&self, event: u32) -> (u32, u32);
    fn store_event_samples(&mut self, event: u32, first: u32, second: u32);

    // The brake wall: its quadtree, the leaf collector and the segment table.
    /// The quadtree's root node.
    fn wall_root(&self, wall: u32) -> u32;
    /// A quadtree node's item count and link (the children, or the leaf's data).
    fn wall_node(&self, node: u32) -> (u32, u32);
    /// The two children a node's link names.
    fn wall_children(&self, link: u32) -> [u32; 2];
    /// A node's box: the centre x and y, then the half extents.
    fn wall_box(&self, node: u32) -> [f32; 4];
    fn clear_collector(&mut self, collector: u32);
    fn collector_count(&self, collector: u32) -> u32;
    fn collector_push(&mut self, collector: u32, node: u32);
    fn collector_leaf(&self, collector: u32, index: u32) -> u32;
    fn wall_latch(&self, wall: u32) -> u32;
    fn set_wall_latch(&mut self, wall: u32, value: u32);
    /// The collector `QueryBox` fills (the wall's leaf list).
    fn wall_leaves(&self, wall: u32) -> u32;
    fn wall_mesh(&self, wall: u32) -> u32;
    /// The four vertices of segment record `index`, at the 8-byte stride.
    fn wall_segment(&self, mesh: u32, index: u32) -> [[f32; 2]; 4];
    /// A leaf's segment count, and the index word of its segment `j`.
    fn leaf_count(&self, leaf: u32) -> u32;
    fn leaf_word(&self, leaf: u32, j: u32) -> u32;
    /// The penetration signal's emit object.
    fn penetration_emitter(&self) -> u32;
    /// What `Raycast` writes to its record: the normal, penetration, signed distance and flags.
    fn store_wall_probe(&mut self, out: u32, hit: &SegmentHit, flags: u32);
    fn store_wall_flags(&mut self, out: u32, flags: u32);
    /// What `VfE8` writes to its record on a crossing.
    fn store_wall_crossing(&mut self, out: u32, wall: u32, hit: &EdgeCross);

    // The collision mesh: the node tree, the face lists and the hit records.
    fn sat_box(&self, at: u32) -> SatBox;
    fn tree_node(&self, node: u32) -> TreeNode;
    /// The two children a node's link names: each one's address and flag word.
    fn child_pair(&self, link: u32) -> [(u32, u32); 2];
    /// The tree root of a collider or system.
    fn tree_root(&self, owner: u32) -> u32;
    /// The collider's own list of records, used when the caller gives none.
    fn own_list(&self, owner: u32) -> u32;
    fn list_len(&self, list: u32) -> u32;
    fn list_record(&self, list: u32, index: u32) -> u32;
    /// The vertex table `index` of a collider or system.
    fn mesh_table(&self, owner: u32, index: u32) -> u32;
    /// The vertex in slot `slot` of a vertex table.
    fn mesh_vertex(&self, table: u32, slot: u16) -> Vec3;
    /// The face record at `at`.
    fn face_bytes(&self, at: u32) -> [u8; 0x12];
    fn mesh_flags(&self, object: u32) -> u32;
    /// A mesh's vertex block, vertex `index` of it (16-byte vertices) and its tags.
    fn mesh_block(&self, object: u32) -> u32;
    fn mesh_point(&self, block: u32, index: u32) -> Vec3;
    fn mesh_tag(&self, object: u32, index: u32) -> u32;
    /// A ray object's seeds cleared, and the addresses of its two vectors.
    fn clear_ray_seeds(&mut self, object: u32);
    fn ray_vectors(&self, object: u32) -> (u32, u32);
    fn clear_ray(&mut self, object: u32);
    /// The surface record a mesh query returns.
    fn store_surface_record(&mut self, out: u32, record: &SurfaceRecord);
    /// The nearest-face record the node worklist keeps.
    fn ray_hit(&self, hit: u32) -> RayHit;
    fn store_ray_hit(&mut self, hit: u32, value: &RayHit);
    fn ray_hit_t(&self, hit: u32) -> f32;
    fn set_ray_hit_t(&mut self, hit: u32, t: f32);
    fn store_ray_hit_normal(&mut self, hit: u32, normal: Vec3);

    // The collision grid's face and edge-pair tables (`E37GridOps`).
    fn grid_bounds(&self, grid: u32) -> Bounds;
    fn grid_face_count(&self, grid: u32) -> u32;
    /// Whether the material byte of face `index` is not the skipped `0xff`.
    fn grid_face_is_valid(&self, grid: u32, index: u32) -> bool;
    fn grid_face(&self, grid: u32, index: u32) -> Face;
    fn grid_face_samples(&self, grid: u32, index: u32) -> [u8; 3];
    /// The triangle vertices of a face's three vertex indices.
    fn grid_face_vertices(&self, grid: u32, indices: [u8; 3]) -> [Vec3; 3];
    /// The grid's material tag, which a hit copies.
    fn grid_material(&self, grid: u32) -> u32;
    /// The sample atlas' width and height, `None` for a grid without one.
    fn grid_sample_dimensions(&self, grid: u32) -> Option<(u16, u16)>;
    fn grid_vertex_samples(&self, grid: u32, indices: [u8; 3]) -> [[u8; 2]; 3];
    fn grid_sample_bytes(&self, grid: u32, first: i32, second: i32) -> [u8; 2];
    /// The grid's sample result bytes (`+0x688`, `+0x689`).
    fn store_grid_sample_bytes(&mut self, grid: u32, bytes: [u8; 2]);
    /// The vertex in slot `slot` of the grid's vertex table.
    fn grid_vertex(&self, grid: u32, slot: u8) -> Vec3;
    /// The two vertex slots of edge-pair row `index`.
    fn grid_pair_row(&self, grid: u32, index: u32) -> [u8; 2];
    fn grid_sample_slot(&self, grid: u32, slot: u8) -> u8;
    fn grid_sample_pair(&self, grid: u32, sample: u8) -> [u8; 2];
    /// Makes `grid` a copy of `src`.
    fn copy_grid(&mut self, grid: u32, src: u32);
    /// The addresses of the grid's two point blocks.
    fn grid_blocks(&self, grid: u32) -> (u32, u32);
    /// The scale word the half extent takes (the w lane of the half vector).
    fn grid_half_scale(&self, grid: u32) -> f32;
    /// The grid's bounds: low, high, centre and half extent.
    fn store_grid_bounds(&mut self, grid: u32, low: Vec4, high: Vec4, centre: Vec4, half: Vec4);

    // Event creation and the events' calls on their bodies.
    /// A three-lane vector at `at`, and its store (the fourth lane is not written).
    fn vec3(&self, at: u32) -> Vec3;
    fn store_vec3(&mut self, at: u32, value: Vec3);
    /// The address of the event's contact point (`+0x60`), the vector its bodies' calls take.
    fn event_point(&self, event: u32) -> u32;
    /// The address of the event's material ids on `side`.
    fn event_materials_at(&self, event: u32, side: Side) -> u32;
    fn event_ids(&self, event: u32) -> EventIds;
    /// Where `InitFull` has the body's transforms write the event's two contact points.
    fn event_contact_outputs(&self, event: u32) -> (u32, u32);
    /// `InitFull`'s field writes, in the original's order, before its calls: the links cleared, the handle on the root
    /// identity, the base vtable, the vectors and ids cleared, the tag, the 16 payload words at `payload`, the body and
    /// the kind in the flags.
    fn init_event(&mut self, event: u32, kind: u32, body: u32, tag: u32, payload: u32);
    /// The global event count, one more.
    fn count_event(&mut self);
    /// The head of the free list of events.
    fn free_event(&self) -> u32;
    /// Takes a free event off the list when it is on it: the count one less, the event unlinked.
    fn take_free_event(&mut self, event: u32);
    /// A grid's two hit flag bytes, which are cleared after the read.
    fn take_grid_hit_flags(&mut self, grid: u32) -> [u32; 2];
    fn grid_tag(&self, grid: u32) -> u32;
    /// Sets all three of the event's body-side material words to `tag` (the last cleared).
    fn store_event_tag(&mut self, event: u32, tag: u32);

    // E36's collision entry (`object`): an OBB frame and four contact records, filled from the bodies in the cells.
    fn obb_frame(&self, object: u32) -> ObbFrame;
    /// The sphere query at `at`: its centre and radius.
    fn sphere_input(&self, at: u32) -> (Vec4, f32);
    /// The five vectors an OBB query takes at `at`: the centre, the half extents and the three axes.
    fn obb_input(&self, at: u32) -> [Vec4; 5];
    /// The w lane of the slot's previous normal, which the contact's normalisation scales.
    fn contact_normal_w(&self, object: u32, index: u32) -> f32;
    /// Writes contact `index` of the entry; the OBB contact keeps the position's w lane.
    fn store_sphere_contact(&mut self, object: u32, index: u32, input: Vec4, contact: &SphereContact);
    fn store_obb_contact(&mut self, object: u32, index: u32, input: Vec4, contact: &ObbContact);
    /// The surface tag byte of contact `index`.
    fn store_contact_tag(&mut self, object: u32, index: u32, tag: u8);
    /// Where the entry's frame goes in and out of the bodies' transforms.
    fn entry_frame(&self, object: u32) -> EntryFrame;
    /// The centre the transform last wrote.
    fn entry_centre(&self, object: u32) -> [f32; 3];
    /// The address the entry's contact records are listed at.
    fn contact_list(&self, object: u32) -> u32;
    /// The matrix address `E36Collect` composes with.
    fn collide_matrix(&self) -> u32;
    /// A body's collision entry: its active shape, whether that shape is a sphere, and its tag.
    fn collision_shape(&self, collision: u32) -> u32;
    fn collision_is_sphere(&self, collision: u32) -> bool;
    fn collision_tag(&self, collision: u32) -> u8;
    /// A shape's OBB axis `axis` (0 to 2) and half-length.
    fn shape_axis(&self, shape: u32, axis: u32) -> Vec4;
    fn shape_extent(&self, shape: u32, axis: u32) -> f32;
    /// A cell list's first node, a node's next and its owning body.
    fn cell_first(&self, cell: u32) -> u32;
    fn node_next(&self, node: u32) -> u32;
    fn node_owner(&self, node: u32) -> u32;
}

/// The addresses of an E36 entry's frame: the point and axes the bodies' transforms read (`*_in`) and the places they
/// write (`*_out`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntryFrame {
    pub centre_in: u32,
    pub centre_out: u32,
    pub axes_in: [u32; 3],
    pub axes_out: [u32; 3],
}

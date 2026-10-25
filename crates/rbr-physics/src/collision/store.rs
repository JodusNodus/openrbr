//! The collision subsystem's store (`crate::store`): the layout's load and store groups. `Mem` implements it in
//! `layout/mem_store.rs`.

use super::contact_solver::{ContactMaterial, ContactRow, RowEvent};
use super::obb::{ObbContact, ObbFrame, SphereContact};
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

//! The stage's physics world from its retail files, as the load builds it (step 5 of the load path,
//! `notes/rust-plan.md` milestone L):
//! - the `.col` image, read whole and relocated in place (the relocation itself is
//!   `rbr_formats::col::relocate`), the collision system's vertex slots and tree root, the brake wall its root
//!   names, the cell grid (`physics_core::detect::build_cell_grid`) and the root's water meshes;
//! - the `.mat` image and the material table (`entities::material_table::MaterialTable`);
//! - the `.trk` file: the drive line (kind 0x14 records as drive-line entries) and the collision meshes
//!   (kind 0x16): a descriptor per mesh, the collision grid of each static one (`collision::col_grid`),
//!   and the placements ([`Stuff`]), which the physics turns into objects;
//! - the placements' objects: per placement an object of the registry entry its object type names, placed (pose,
//!   extent, grid cell, collision grid, rotation and bounds), filed with the physics state and put to sleep.
//!
//! The pure parts are here; [`layout`] runs them on game memory in load order.

pub mod layout;
pub mod store;

use rbr_formats::trk;

use crate::engine_driveline::drive_line::DriveLinePoint;
use crate::math::vec::Vec4;
use crate::physics_core::detect::Xy;

/// The `.col` root object's header, as the relocation's mode-3 path reads it: offsets in the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColRoot {
    /// The root tree, which becomes the collision system's tree root.
    pub tree: u32,
    /// The brake-wall definition (none when 0).
    pub wall_def: Option<u32>,
    /// The two water-mesh lists: wet surfaces (built only in wet weather) and water.
    pub wet: MeshList,
    pub water: MeshList,
}

/// A run of 0x40-byte mesh records: their count and offset in the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshList {
    pub count: u32,
    pub at: u32,
}

/// Bytes per water-mesh record: four corners of four floats.
pub const MESH_RECORD: u32 = 0x40;

fn word(image: &[u8], at: u32) -> u32 {
    let at = at as usize;
    u32::from_le_bytes(image[at..at + 4].try_into().unwrap())
}

/// The root object's header, from the file header.
pub fn col_root(image: &[u8]) -> ColRoot {
    let root = word(image, 4);
    let field = |off| word(image, root + off);
    let def = field(0x14);
    ColRoot {
        tree: root + field(0x10),
        wall_def: (def != 0).then_some(root + def),
        wet: MeshList { count: field(0x18), at: root + field(0x1c) },
        water: MeshList { count: field(0x20), at: root + field(0x24) },
    }
}

/// The vertex slots a relocated image's objects took: the root, then the cells in table order, each with vertices
/// holding its slot. Each gives `(slot, offset of its vertices)`, which the collision system's slot table keeps.
pub fn vertex_slots(image: &[u8]) -> Vec<(u32, u32)> {
    let (count, table) = (word(image, 8), word(image, 0xc));
    std::iter::once(word(image, 4))
        .chain((0..count).map(|i| word(image, table + 4 * i)))
        .filter(|&object| word(image, object + 8) != 0)
        .map(|object| {
            let slot = u16::from_le_bytes([image[object as usize + 2], image[object as usize + 3]]);
            (u32::from(slot), object + word(image, object + 0xc))
        })
        .collect()
}

/// The water mesh's corner bounds: the lanewise bounds of the record's four corners (the min and max over corners
/// 1 to 3 from corner 0), and the stored word, `(y + x) * 0 - z` of corner 0 (in that order).
pub fn water_box(corners: [Vec4; 4]) -> (Vec4, Vec4, f32) {
    let (low, high) =
        corners[1..].iter().fold((corners[0], corners[0]), |(low, high), &c| (low.min_lanes(c), high.max_lanes(c)));
    let c = corners[0];
    (low, high, (c.y + c.x) * 0.0 - c.z)
}

/// The point the water is filed under, the centre of its bounds: `(high + low) * half` per axis in f32 arithmetic,
/// stored as floats.
pub fn water_centre(low: Vec4, high: Vec4, half: f32) -> (f32, f32) {
    ((high.x + low.x) * half, (high.y + low.y) * half)
}

/// The corners (low, high) of the wall's root box (centre x, centre y, half x, half y), which the cell grid spans
/// over.
pub fn wall_corners([cx, cy, hx, hy]: [f32; 4]) -> [Xy; 2] {
    [Xy { x: cx - hx, y: cy - hy }, Xy { x: cx + hx, y: hy + cy }]
}

/// One run of the chosen `.mat` entry as the material loader hands it on: the low bytes of its width and height
/// words and the offset of its 256 cell bytes in the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialMap {
    pub width: u8,
    pub height: u8,
    pub cells: u32,
}

/// On a `.mat` image: the runs of the first entry whose name holds both `wear` and `wetness` (the `wear` and
/// `wetness` words), or `None` when no entry does (then nothing is emitted). Each entry is a NUL-terminated name,
/// a run count and 0x108-byte runs (width word, height word, cells).
pub fn select_maps(image: &[u8], wear: &[u8], wetness: &[u8]) -> Option<Vec<MaterialMap>> {
    let contains = |name: &[u8], word: &[u8]| name.windows(word.len()).any(|w| w == word);
    let mut at = 4;
    for _ in 0..word(image, 0) {
        let len = image[at as usize..].iter().position(|&b| b == 0).unwrap() as u32;
        let name = &image[at as usize..(at + len) as usize];
        let runs = word(image, at + len + 1);
        at += len + 5;
        let maps: Vec<_> = (0..runs)
            .map(|j| {
                let run = at + j * 0x108;
                MaterialMap { width: image[run as usize], height: image[run as usize + 4], cells: run + 8 }
            })
            .collect();
        at += runs * 0x108;
        if contains(name, wear) && contains(name, wetness) {
            return Some(maps);
        }
    }
    None
}

fn vec4(v: trk::Vec3) -> Vec4 {
    Vec4 { x: v.x, y: v.y, z: v.z, w: 0.0 }
}

/// The kind-0x14 records as drive-line entries: the position, the tangent, the arc length, and the two words as the
/// tag (the first in its high half). The array leaves the vectors' `w` lanes unset in memory; here they are 0.
pub fn drive_line_entries(data: &trk::DriveLineData) -> Vec<DriveLinePoint> {
    data.records
        .iter()
        .map(|r| DriveLinePoint {
            position: vec4(r.vectors[0]),
            tangent: vec4(r.vectors[1]),
            distance: r.value,
            tag: u32::from(r.unknown_u16[0]) << 16 | u32::from(r.unknown_u16[1]),
        })
        .collect()
}

/// A placement as the mesh loader emits it (0x50 bytes; the words not named here are 0). Its receiver makes the
/// object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stuff {
    /// The position, values and extent.
    pub position: Vec4,
    pub values: [f32; 4],
    pub extent: Vec4,
    /// The mesh's collision id, or for a dynamic mesh its descriptor.
    pub collision: u32,
    /// The object type.
    pub object_type: u32,
    /// The interactive object's handle (0 for mesh kinds 0 and 5).
    pub handle: u32,
    /// The dynamic-mesh flag (0x30 for a dynamic mesh).
    pub flags: u32,
}

/// The placement `object` of `mesh`, with `collision` as its id or descriptor and no handle yet.
pub fn stuff(mesh: &trk::CollisionMesh, object: &trk::CollisionObject, collision: u32) -> Stuff {
    Stuff {
        position: vec4(object.position),
        values: object.values,
        extent: vec4(object.extent),
        collision,
        object_type: mesh.game_object_type(),
        handle: 0,
        flags: if mesh.is_static() { 0 } else { 0x30 },
    }
}

/// Whether the mesh loader asks the interactive objects for a handle: for every mesh kind but 0 and 5.
pub fn wants_handle(mesh: &trk::CollisionMesh) -> bool {
    !matches!(mesh.kind, 0 | 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f32, y: f32, z: f32) -> Vec4 {
        Vec4 { x, y, z, w: 0.0 }
    }

    #[test]
    fn water_box_takes_lanewise_bounds() {
        let (low, high, level) =
            water_box([v(1.0, 5.0, -2.0), v(3.0, 4.0, -2.0), v(0.0, 6.0, -1.0), v(2.0, 5.5, -3.0)]);
        assert_eq!(low, v(0.0, 4.0, -3.0));
        assert_eq!(high, v(3.0, 6.0, -1.0));
        assert_eq!(level, 2.0);
    }

    #[test]
    fn wall_corners_span_the_root_box() {
        let [low, high] = wall_corners([100.0, 50.0, 20.0, 10.0]);
        assert_eq!((low.x, low.y, high.x, high.y), (80.0, 40.0, 120.0, 60.0));
    }

    #[test]
    fn select_maps_takes_the_first_match() {
        let mut image = 2u32.to_le_bytes().to_vec();
        for (name, width) in [(&b"x dry new\0"[..], 7u8), (b"x damp new\0", 16)] {
            image.extend_from_slice(name);
            image.extend_from_slice(&1u32.to_le_bytes());
            image.extend_from_slice(&u32::from(width).to_le_bytes());
            image.extend_from_slice(&16u32.to_le_bytes());
            image.extend_from_slice(&[0; 256]);
        }
        let maps = select_maps(&image, b"new", b"damp").unwrap();
        assert_eq!(maps, [MaterialMap { width: 16, height: 16, cells: 4 + 10 + 4 + 264 + 11 + 4 + 8 }]);
        assert_eq!(select_maps(&image, b"worn", b"dry"), None);
    }
}

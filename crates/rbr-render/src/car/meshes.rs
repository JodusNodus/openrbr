//! Building the car's meshes from the `.sgc` model: draw order, crack overlays, damageable panels and placement.

use super::shading::{draw_state, FaceStyle, Order};
use super::UNDAMAGED;
use crate::damage::{self, Crack, Dents, MeshDamage, Panel};
use crate::stage::{Anchor, Bounds, Draw, Layer, Lod, Mesh, Vertex};
use glam::{Mat3, Mat4, Vec3};
use rbr_formats::sgc;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// The car's meshes in the game's draw order. Blending makes order matter from the first blended shader on: a draw
/// without z write is covered by any later draw behind it. Meshes before it are solid (their order does not
/// matter); the rest are drawn in the blended layer in order, so a solid part whose shader comes later (COROLLA's
/// spare wheel after the `pc_e_window_*` panes) still covers a pane in front of it, as in the game.
pub(super) fn in_draw_order<T>(mut groups: Vec<(Order, Mesh, T)>) -> Vec<(Mesh, T)> {
    groups.sort_by_key(|(order, _, _)| *order);
    let first_blended =
        groups.iter().find(|(_, mesh, _)| mesh.draws.iter().any(|d| d.state.blend)).map(|(order, _, _)| *order);
    groups
        .into_iter()
        .map(|(order, mut mesh, damage)| {
            if first_blended.is_some_and(|first| order >= first) {
                mesh.layer = Layer::Blended;
            }
            (mesh, damage)
        })
        .collect()
}

/// The crack overlays of window part `mesh`, slot `window`: its first face (the `CFaceBlock` HH 0x100161e0 draws) once
/// per projection the viewer offers ([`damage::crack_variants`]), with the crack uvs of its vertices in the car's
/// model space (`transform`, before `placement`) about their mean. The car's mirror bits are taken as clear (the game
/// draws them by `rand()`).
pub(super) fn crack_overlays(
    mesh: &sgc::Mesh,
    window: usize,
    transform: Mat4,
    placement: Mat4,
    style: FaceStyle,
) -> Vec<(Order, Mesh, MeshDamage)> {
    let model: Vec<Vec3> = mesh.vertices.iter().map(|v| transform.transform_point3(Vec3::from(v.position))).collect();
    let Some(centre) = (!model.is_empty()).then(|| model.iter().sum::<Vec3>() / model.len() as f32) else {
        return Vec::new();
    };
    let to_model = placement.inverse();
    let built = build_meshes(mesh, placement * transform, &[style]);
    let mut overlays = Vec::new();
    for (order, base, damage) in built.meshes {
        for &high in damage::crack_variants(window) {
            let crack = Crack { window, high };
            let mut overlay = base.clone();
            for vertex in &mut overlay.vertices {
                let p = to_model.transform_point3(Vec3::from(vertex.position));
                vertex.uv0 = damage::crack_uv(crack, false, centre, p);
            }
            overlays.push((
                order,
                overlay,
                MeshDamage { dents: damage.dents.clone(), crack: Some(crack), ..MeshDamage::default() },
            ));
        }
    }
    overlays
}

/// One model mesh in stage space, split by draw order, with the triangles its faces dropped.
pub(super) struct BuiltMeshes {
    pub(super) meshes: Vec<(Order, Mesh, MeshDamage)>,
    pub(super) bad_triangles: usize,
}

/// A damageable panel's deformation data, when it has a morph target and a non-empty zone list. `UpdateDeformation`
/// writes nothing for a panel without a target (8 interior panels of COROLLA), and moves nothing without lists (the
/// `pc_e_window_*` panes; rbr-recomp `notes/car-damage-visuals.md`, section 6).
fn panel_of(mesh: &sgc::Mesh) -> Option<(Arc<Panel>, &[[f32; 3]])> {
    let target = mesh.morph_targets.first().filter(|_| mesh.damageable)?;
    if mesh.morph_lists.iter().all(Vec::is_empty) {
        return None;
    }
    let panel = Panel { vertex_count: mesh.vertices.len(), lists: mesh.morph_lists.clone() };
    Some((Arc::new(panel), &target.deltas))
}

/// Transforms one mesh's vertices by `matrix` and its faces into draws, one mesh per draw order (each with the
/// vertices it uses). The triangles keep their order under a mirroring `matrix`, as the game draws a node through its
/// world matrix with the class's fixed cull mode: the mirror turns which side is culled (`coords::is_mirror`).
pub(super) fn build_meshes(mesh: &sgc::Mesh, matrix: Mat4, faces: &[FaceStyle]) -> BuiltMeshes {
    let panel = panel_of(mesh);
    let mut by_order: BTreeMap<Order, (Vec<u32>, Vec<Draw>)> = BTreeMap::new();
    let mut bad_triangles = 0;
    for (face, style) in mesh.faces.iter().zip(faces) {
        let start = face.start_index as usize;
        let count = face.prim_count as usize;
        let triangles = start.is_multiple_of(3).then(|| mesh.triangles.get(start / 3..start / 3 + count)).flatten();
        let Some(triangles) = triangles else {
            bad_triangles += count;
            continue;
        };
        let (indices, draws) = by_order.entry(style.order).or_default();
        let first = indices.len();
        for tri in triangles {
            let tri = tri.map(usize::from);
            if tri.iter().any(|&i| i >= mesh.vertices.len()) {
                bad_triangles += 1;
                continue;
            }
            indices.extend(tri.map(|i| i as u32));
        }
        if indices.len() > first {
            draws.push(Draw {
                indices: first..indices.len(),
                state: draw_state(style.texture, style.scrape, style.pass, style.shade),
                lod: Lod::Always,
            });
        }
    }
    let meshes = by_order
        .into_iter()
        .filter(|(_, (_, draws))| !draws.is_empty())
        .map(|(order, (indices, draws))| {
            let (placed, sources) = placed_mesh(mesh, matrix, indices, draws);
            let dents = panel.as_ref().map(|(panel, deltas)| Dents {
                panel: panel.clone(),
                vertices: sources
                    .iter()
                    .map(|&i| (i, matrix.transform_vector3(Vec3::from(deltas[i as usize]))))
                    .collect(),
            });
            (order, placed, MeshDamage { dents, ..MeshDamage::default() })
        })
        .collect();
    BuiltMeshes { meshes, bad_triangles }
}

/// The vertices of `mesh` that `indices` use, placed by `matrix`, with the indices renumbered to them, and each one's
/// index in `mesh`. The second uv is the first, for the scrape texture.
fn placed_mesh(mesh: &sgc::Mesh, matrix: Mat4, indices: Vec<u32>, draws: Vec<Draw>) -> (Mesh, Vec<u32>) {
    // Normals go by the inverse transpose, which keeps them outward through scales and mirrors.
    let normals = Mat3::from_mat4(matrix).inverse().transpose();
    let mut remap: HashMap<u32, u32> = HashMap::new();
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut sources: Vec<u32> = Vec::new();
    let indices: Vec<u32> = indices
        .into_iter()
        .map(|i| {
            *remap.entry(i).or_insert_with(|| {
                let v = i as usize;
                let uv = mesh.uvs.get(v).copied().unwrap_or([0.0; 2]);
                sources.push(i);
                vertices.push(Vertex {
                    position: matrix.transform_point3(Vec3::from(mesh.vertices[v].position)).to_array(),
                    normal: (normals * Vec3::from(mesh.vertices[v].normal)).normalize_or_zero().to_array(),
                    color: UNDAMAGED,
                    uv0: uv,
                    uv1: uv,
                    uv2: [0.0; 2],
                    specular: [0.0; 3],
                    occlusion: 0.0,
                });
                vertices.len() as u32 - 1
            })
        })
        .collect();
    let (min, max) = vertices.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
        let p = Vec3::from(v.position);
        (lo.min(p), hi.max(p))
    });
    let layer = if draws.iter().any(|d| d.state.blend) { Layer::Blended } else { Layer::Solid };
    let placed = Mesh {
        layer,
        anchor: Anchor::World,
        bounds: Some(Bounds { center: (min + max) * 0.5, half_extent: (max - min) * 0.5 }),
        cell: None,
        vertices,
        indices,
        draws,
    };
    (placed, sources)
}

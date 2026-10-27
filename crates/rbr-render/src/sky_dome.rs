//! Sky domes among the backdrop's meshes (ours): some RSF stages draw their sky as a world-anchored sphere or box
//! kilometres across, a sky face like the backdrop's terrain (519 Bois de Bie, 580 and 592 Helette, 602 and 603,
//! 450 Gabiria and its versions, 126, 312, 332, 470, 482, 484, 489, 517, 558, 570, 631, 641; 469 and 473, the Longyou
//! stages 574 to 577: 39 of the 659 readable variants). As a caster it roofs the whole stage: the sky's occlusion found
//! every vertex shut in, and the stage went black under `lighting`. A dome is told from terrain by covering
//! the stage from above: the backdrop's terrain lies under or around the stage, never over its middle. The casters
//! leave domes out (`renderer::shadows`), and under `Enhance::lighting` the renderer leaves them out altogether: the
//! physically based sky and its clouds take their place, following the weather (user decision 2026-10-27).

use crate::stage::{Anchor, Layer, Mesh};
use glam::{Vec2, Vec3, Vec3Swizzles};

/// Where a dome must pass over the stage: the middle of the stage's world meshes' footprint and the corners of its
/// central half, each above every one of them.
pub(crate) struct Over {
    points: [Vec2; 5],
    above: f32,
}

impl Over {
    /// Over the world-anchored meshes with bounds (the stage, not its sky); `None` without any.
    pub(crate) fn of(meshes: &[Mesh]) -> Option<Over> {
        let (low, high) = meshes
            .iter()
            .filter(|m| m.anchor == Anchor::World && m.layer != Layer::Sky)
            .filter_map(|m| m.bounds)
            .map(|b| (b.center - b.half_extent, b.center + b.half_extent))
            .reduce(|(l, h), (bl, bh)| (l.min(bl), h.max(bh)))?;
        let (centre, quarter) = (((low + high) * 0.5).xz(), ((high - low) * 0.25).xz());
        let corner = |x: f32, z: f32| centre + quarter * Vec2::new(x, z);
        Some(Over {
            points: [centre, corner(-1.0, -1.0), corner(1.0, -1.0), corner(-1.0, 1.0), corner(1.0, 1.0)],
            above: high.y,
        })
    }

    /// Whether `mesh` is a world-anchored sky face with a triangle over each point, higher than the stage.
    pub(crate) fn roofed_by(&self, mesh: &Mesh) -> bool {
        if mesh.anchor != Anchor::World || mesh.layer != Layer::Sky {
            return false;
        }
        let position = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
        let triangles: Vec<[Vec3; 3]> =
            mesh.indices.chunks_exact(3).map(|t| [position(t[0]), position(t[1]), position(t[2])]).collect();
        self.points.iter().all(|&p| triangles.iter().any(|t| height_over(t, p).is_some_and(|y| y > self.above)))
    }
}

/// The height of triangle `t` over the point `p` (stage x and z), if it lies over it.
fn height_over(t: &[Vec3; 3], p: Vec2) -> Option<f32> {
    let [a, b, c] = t.map(|v| v.xz());
    let area = (b - a).perp_dot(c - a);
    if area.abs() < 1e-6 {
        return None;
    }
    let u = (b - p).perp_dot(c - p) / area;
    let v = (c - p).perp_dot(a - p) / area;
    let w = 1.0 - u - v;
    (u >= 0.0 && v >= 0.0 && w >= 0.0).then(|| u * t[0].y + v * t[1].y + w * t[2].y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{Bounds, Vertex};
    use bytemuck::Zeroable;

    fn mesh(layer: Layer, positions: &[[f32; 3]], indices: Vec<u32>) -> Mesh {
        let vertices = positions.iter().map(|&position| Vertex { position, ..Vertex::zeroed() }).collect();
        let bounds =
            (layer != Layer::Sky).then_some(Bounds { center: Vec3::ZERO, half_extent: Vec3::new(500.0, 20.0, 500.0) });
        Mesh { layer, anchor: Anchor::World, bounds, cell: None, vertices, indices, draws: Vec::new() }
    }

    #[test]
    fn a_roof_over_the_stage_is_a_dome_and_terrain_around_it_is_not() {
        let stage = mesh(Layer::Solid, &[], Vec::new());
        let over = Over::of(std::slice::from_ref(&stage)).unwrap();
        // A square roof 2 km up, 6 km across.
        let roof = mesh(
            Layer::Sky,
            &[[-3e3, 2e3, -3e3], [3e3, 2e3, -3e3], [3e3, 2e3, 3e3], [-3e3, 2e3, 3e3]],
            vec![0, 1, 2, 0, 2, 3],
        );
        assert!(over.roofed_by(&roof));
        // The same square as terrain under the stage, and a ring of hills around it.
        let ground = mesh(
            Layer::Sky,
            &[[-3e3, -50.0, -3e3], [3e3, -50.0, -3e3], [3e3, -50.0, 3e3], [-3e3, -50.0, 3e3]],
            vec![0, 1, 2, 0, 2, 3],
        );
        assert!(!over.roofed_by(&ground));
        let hill = mesh(Layer::Sky, &[[2e3, 0.0, -3e3], [3e3, 800.0, 0.0], [2e3, 0.0, 3e3]], vec![0, 1, 2]);
        assert!(!over.roofed_by(&hill));
        // Not a sky face.
        assert!(!over.roofed_by(&Mesh { layer: Layer::Solid, ..roof }));
    }
}

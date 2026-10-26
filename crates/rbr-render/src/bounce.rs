//! The light the stage bounces onto itself, computed from its geometry (ours, with
//! [`Enhance::occlusion`](crate::renderer::Enhance::occlusion)): what each vertex sees, the ground beside and under
//! walls, trunks, banks and people, and the sunlit slopes and walls across from a shaded one, lit by the sun and the
//! sky. The stage's tiles are drawn (as the sky's occlusion draws them, [`crate::sky_occlusion`]) along [`DIRECTIONS`]
//! directions below the horizon and as many above it, from the side the vertices look toward, as each surface's linear
//! albedo times its share of the sun (facing it, past the stage's shadow along the sun) and of the sky
//! ([`Vertex::occlusion`](crate::stage::Vertex::occlusion)), only its faces turned toward the vertices (from below the
//! nearest is the ground the vertex sees; from above, a wall or slope's side facing it, not a crown's or roof's top).
//! Each vertex sums the light of the surfaces it sees, weighed by how squarely it faces them, into
//! [`Vertex::bounce`](crate::stage::Vertex::bounce): the light it gets per unit of the sun's and of the sky's light on
//! the ground, which the shader times by the frame's (`own_light` in `shader.wgsl`), so only the sun's direction needs
//! it drawn again.
//!
//! The car and the people move, so they look the ground up where they are instead: each tile drawn from above without
//! the cut-out draws (the floor under the trees, not their tops), [`GROUND`] texels across, the same two colours
//! (`ground_bounce` in `shader.wgsl`).

use glam::Vec3;

pub(crate) const SHADER: &str = include_str!("bounce.wgsl");
pub(crate) const SURFACE_SHADER: &str = include_str!("bounce_surface.wgsl");
/// Directions in each hemisphere, spread evenly (equal solid angles).
pub(crate) const DIRECTIONS: usize = 24;
/// Texels across a tile's map of the lit surfaces (a metre each): the bounce varies slowly.
pub(crate) const SIZE: u32 = 1024;
/// Texels across a tile's map of the ground's light seen from above (4 metres each).
pub(crate) const GROUND: u32 = 256;

/// Directions below the horizon, then above it: the sky's occlusion's spiral ([`crate::sky_occlusion::spiral`]) turned
/// over, then as it is.
pub(crate) fn directions() -> Vec<Vec3> {
    let above = crate::sky_occlusion::spiral(DIRECTIONS);
    above.iter().map(|d| Vec3::new(d.x, -d.y, d.z)).chain(above.iter().copied()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directions_cover_the_ground_evenly() {
        let dirs = directions();
        assert_eq!(dirs.len(), 2 * DIRECTIONS);
        assert!(dirs.iter().all(|d| (d.length() - 1.0).abs() < 1e-5));
        let (below, above) = dirs.split_at(DIRECTIONS);
        assert!(below.iter().all(|d| d.y < 0.0) && above.iter().all(|d| d.y > 0.0));
        let mean = below.iter().map(|d| d.y).sum::<f32>() / DIRECTIONS as f32;
        assert!((mean + 0.5).abs() < 1e-4);
    }

    #[test]
    fn the_bakes_step_the_vertex_buffer_by_a_vertex() {
        let words = std::mem::size_of::<crate::stage::Vertex>() / 4;
        for shader in [SHADER, crate::sky_occlusion::SHADER] {
            assert!(shader.contains(&format!("const STRIDE: u32 = {words}u;")), "STRIDE is not {words} words");
        }
    }
}

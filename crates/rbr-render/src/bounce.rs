//! The light the stage bounces onto itself, computed from its geometry (ours, with
//! [`Enhance::occlusion`](crate::renderer::Enhance::occlusion)): what each vertex sees below the horizon, the ground
//! beside and under walls, trunks, banks and people, lit by the sun and the sky. The stage's tiles are drawn (as the
//! sky's occlusion draws them, [`crate::sky_occlusion`]) along [`DIRECTIONS`] directions below the horizon, from
//! below, as each surface's linear albedo times its share of the sun (facing it, past the stage's shadow along the sun)
//! and of the sky ([`Vertex::occlusion`](crate::stage::Vertex::occlusion)); from below the nearest surface is the ground
//! the vertex sees. Each vertex sums the light of the surfaces it sees, weighed by how squarely it faces them, into
//! [`Vertex::bounce`](crate::stage::Vertex::bounce): the light it gets per unit of the sun's and of the sky's light on
//! the ground, which the shader times by the frame's (`own_light` in `shader.wgsl`), so only the sun's direction needs
//! it drawn again.
//!
//! What a vertex sees above the horizon (a canopy's underside, an overhang) counts as dark: a map drawn from above
//! holds only the side facing away from the vertex.

use glam::Vec3;

pub(crate) const SHADER: &str = include_str!("bounce.wgsl");
pub(crate) const SURFACE_SHADER: &str = include_str!("bounce_surface.wgsl");
/// Directions below the horizon, spread evenly over the lower hemisphere (equal solid angles).
pub(crate) const DIRECTIONS: usize = 24;
/// Texels across a tile's map of the lit surfaces (a metre each): the bounce varies slowly.
pub(crate) const SIZE: u32 = 1024;
/// The sun's direction may move this far, radians, before the bounce is drawn again (about 8 minutes of the clock).
pub(crate) const RESUN: f32 = 0.035;

/// Directions below the horizon: the sky's occlusion's ([`crate::sky_occlusion::directions`]) turned over.
pub(crate) fn directions() -> Vec<Vec3> {
    crate::sky_occlusion::spiral(DIRECTIONS).into_iter().map(|d| Vec3::new(d.x, -d.y, d.z)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directions_cover_the_ground_evenly() {
        let dirs = directions();
        assert_eq!(dirs.len(), DIRECTIONS);
        assert!(dirs.iter().all(|d| (d.length() - 1.0).abs() < 1e-5 && d.y < 0.0));
        let mean = dirs.iter().map(|d| d.y).sum::<f32>() / DIRECTIONS as f32;
        assert!((mean + 0.5).abs() < 1e-4);
    }
}

//! The car's shadow as the game draws it: `CFenceRenderer::RenderShadows` (0x605670) and `EmitShadows` (0x603fe0),
//! `Textures\car_dpshad2.tga` laid on the stage's collision triangles under the car (`notes/physics.md` "Scenery"
//! 3.6). Traced from the asm (the C++ reconstruction's centre and alpha arguments are wrong).
//!
//! Faithful:
//! - the rectangle: `QueryGround` (0x601a60) with the centre `(0, 2.3, -0.3)` (0x73b9bc, 0x72f668) of the car's frame,
//!   2.0 m wide and 4.2 m long (`x` -1..1, `y` -1.9..2.3); none when the car's `z` axis points down;
//! - the triangles: every triangle of the `.col` leaves whose boxes meet the box over the rectangle's four corners,
//!   10 m (0x72b154) above and below the car, at most 512 (`E39_Query7F0`, [`StageProbe::triangles_in`]);
//! - the height window (`GroundSpan` 0x6019d0, four times): segments from 0.5 m above to 10 m below the car's
//!   origin ± 2 `y` and ± 1 `x`; a triangle is drawn when the mean height of its corners is within 0.5 m of the lowest
//!   and highest of those hits and the rectangle's centre;
//! - the mapping: each corner relative to the rectangle's centre, turned by the car's heading (the frame from the
//!   world `x` axis in the car's axes, flattened), `u = x / 2.0 + 0.5`, `v = y / 4.2 + 1` (the `side` term is 0);
//! - the alpha: `Car_Deep_Shadow_Alpha` (the sky block's word 21, `[[0x7ea990]+0x54]+0x54`) times
//!   `asin(z.z) / (pi / 2)`, times `clamp(1 - 2 (h - 0.5), 0, 1)` for the height `h` of the car's origin over the
//!   ground 50 m (0x7304a0) below it, as a byte (`ftol`);
//! - white vertices and the texture's black, alpha-blended, with a depth bias.
//!
//! Ours: the car's frame is the recorded body pose; the game adds an offset the car body holds (`CCarBody::Get34`
//! `+0x130`, `OnCarState` 0x601fb0), not traced. An alpha over 1 is clamped (the game's byte would wrap). The texture
//! is clamped at its (clear) edges; the game's address mode is not traced.

use glam::{Mat4, Vec3};

use crate::stage_probe::StageProbe;

/// The rectangle's centre in the car's frame, its width (`x`) and length (`y`, from the centre back).
pub const CENTRE: Vec3 = Vec3::new(0.0, 2.3, -0.3);
pub const WIDTH: f32 = 2.0;
pub const LENGTH: f32 = 4.2;
/// The triangle query's reach above and below the car, and the window probes' depth below their points, m.
const REACH: f32 = 10.0;
/// The height probe's depth below the car, m.
const DROP: f32 = 50.0;
/// The query buffer's size (0x8000 bytes of 0x40-byte triangles).
const MOST: usize = 512;
/// `2 / pi` as the exe holds it (0x74f7e4).
const TWO_OVER_PI: f32 = 0.636_619_75;
/// A third as the exe holds it (0x74f7dc).
const THIRD: f32 = 0.3333;

/// The texture's file.
pub const TEXTURE: &str = "Textures/car_dpshad2.tga";

/// One corner of a shadow triangle: its stage position (the renderer's y-up axes), texture coordinates and alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: f32,
}

/// The shadow's triangles, three corners each, for the car at `frame` (its body frame in the physics world, z up) on
/// `probe`'s ground, with the section's `Car_Deep_Shadow_Alpha` `deep`.
pub fn decal(probe: &StageProbe, frame: Mat4, deep: f32) -> Vec<Corner> {
    let (x, y, z, origin) =
        (frame.x_axis.truncate(), frame.y_axis.truncate(), frame.z_axis.truncate(), frame.w_axis.truncate());
    // QueryGround: no shadow for a car on its roof.
    if z.z < 0.0 {
        return Vec::new();
    }
    let local = [
        Vec3::new(CENTRE.x - WIDTH * 0.5, CENTRE.y - LENGTH, CENTRE.z),
        Vec3::new(CENTRE.x + WIDTH * 0.5, CENTRE.y, CENTRE.z),
        Vec3::new(CENTRE.x + WIDTH * 0.5, CENTRE.y - LENGTH, CENTRE.z),
        Vec3::new(CENTRE.x - WIDTH * 0.5, CENTRE.y, CENTRE.z),
    ]
    .map(|p| frame.transform_point3(p));
    let lo = local.iter().fold(Vec3::splat(f32::MAX), |a, &p| a.min(p));
    let hi = local.iter().fold(Vec3::splat(f32::MIN), |a, &p| a.max(p));
    let triangles =
        probe.triangles_in(Vec3::new(lo.x, lo.y, origin.z - REACH), Vec3::new(hi.x, hi.y, origin.z + REACH), MOST);

    // EmitShadows.
    // On the x87 stack, stored as a float.
    let mut alpha = (f64::from(z.z.clamp(-1.0, 1.0)).asin() * f64::from(deep) * f64::from(TWO_OVER_PI)) as f32;
    let heading = Vec3::new(x.x, y.x, z.x);
    let right = Vec3::Z.cross(heading).normalize_or_zero();
    let forward = right.cross(Vec3::Z).normalize_or_zero();
    let centre = frame.transform_point3(CENTRE);
    let (mut low, mut high) = (centre.z, centre.z);
    for p in [origin + y * 2.0, origin + y * -2.0, origin + x, origin - x] {
        let Some(hit) = probe.segment(p + Vec3::Z * 0.5, p - Vec3::Z * REACH) else { continue };
        if hit.point.z < low {
            low = hit.point.z;
        } else if hit.point.z > high {
            high = hit.point.z;
        }
    }
    if let Some(hit) = probe.segment(origin, origin - Vec3::Z * DROP) {
        alpha *= (1.0 - (origin.z - hit.point.z - 0.5) * 2.0).clamp(0.0, 1.0);
    }
    let (low, high) = (low - 0.5, high + 0.5);
    let alpha = (alpha.clamp(0.0, 1.0) * 255.0) as u8 as f32 / 255.0;
    let (across, along) = (1.0 / WIDTH, 1.0 / LENGTH);
    triangles
        .iter()
        .filter(|t| (low..=high).contains(&((t[0].z + t[1].z + t[2].z) * THIRD)))
        .flatten()
        .map(|&p| {
            let d = p - centre;
            let turned = forward * d.x + right * d.y + Vec3::Z * d.z;
            Corner { position: [p.x, p.z, p.y], uv: [turned.x * across + 0.5, turned.y * along + 1.0], alpha }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Quat;

    /// A floor at height 0 facing up, 40 m square about the origin, in two triangles.
    fn floor() -> StageProbe {
        let (a, b, c, d) = (
            Vec3::new(-20.0, -20.0, 0.0),
            Vec3::new(20.0, -20.0, 0.0),
            Vec3::new(20.0, 20.0, 0.0),
            Vec3::new(-20.0, 20.0, 0.0),
        );
        StageProbe::from_triangles(&[[a, c, b], [a, d, c]])
    }

    #[test]
    fn a_level_car_low_over_the_floor_casts_the_full_alpha_mapped_by_its_frame() {
        let frame = Mat4::from_rotation_translation(Quat::from_rotation_z(0.7), Vec3::new(3.0, -2.0, 0.45));
        let corners = decal(&floor(), frame, 1.0);
        assert_eq!(corners.len(), 6);
        assert!(corners.iter().all(|c| c.position[1] == 0.0), "{corners:?}");
        // A yawed frame's z.z is a hair under 1 in floats; asin is steep there.
        assert!(corners.iter().all(|c| c.alpha >= 254.0 / 255.0), "{corners:?}");
        // The rectangle's corners map to the texture's corners.
        let inverse = frame.inverse();
        for c in &corners {
            let local = inverse.transform_point3(Vec3::new(c.position[0], c.position[2], c.position[1]));
            let u = (local.x - CENTRE.x) / WIDTH + 0.5;
            let v = (local.y - CENTRE.y) / LENGTH + 1.0;
            assert!((c.uv[0] - u).abs() < 1e-4 && (c.uv[1] - v).abs() < 1e-4, "{c:?} {local}");
        }
    }

    #[test]
    fn the_alpha_fades_with_height_and_tilt_and_a_car_on_its_roof_casts_none() {
        let probe = floor();
        let at =
            |h: f32, tilt: f32| Mat4::from_rotation_translation(Quat::from_rotation_y(tilt), Vec3::new(0.0, 0.0, h));
        // Half way up the fade: 1 - 2 (0.75 - 0.5) = 0.5, as a byte.
        assert_eq!(decal(&probe, at(0.75, 0.0), 1.0)[0].alpha, 127.0 / 255.0);
        // 1 m up and over: none; the triangles stay (the window holds the rectangle's centre).
        assert!(decal(&probe, at(1.2, 0.0), 1.0).iter().all(|c| c.alpha == 0.0));
        // Tilted 45 degrees: asin(cos 45) / (pi / 2) = 0.5.
        assert_eq!(decal(&probe, at(0.3, std::f32::consts::FRAC_PI_4), 1.0)[0].alpha, 127.0 / 255.0);
        assert_eq!(decal(&probe, at(0.3, 0.0), 0.5)[0].alpha, 127.0 / 255.0);
        assert!(decal(&probe, at(0.3, 3.0), 1.0).is_empty());
    }

    #[test]
    fn faces_far_above_or_below_the_ground_under_the_car_are_left_out() {
        let roof = [Vec3::new(-5.0, -5.0, 3.0), Vec3::new(5.0, 5.0, 3.0), Vec3::new(5.0, -5.0, 3.0)];
        let floor = [Vec3::new(-5.0, -5.0, 0.0), Vec3::new(5.0, 5.0, 0.0), Vec3::new(5.0, -5.0, 0.0)];
        let probe = StageProbe::from_triangles(&[floor, roof]);
        let corners = decal(&probe, Mat4::from_translation(Vec3::new(0.0, 0.0, 0.4)), 1.0);
        assert_eq!(corners.len(), 3);
        assert!(corners.iter().all(|c| c.position[1] == 0.0));
    }
}

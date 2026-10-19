//! The cracked-glass decal (ours), drawn in place of `Textures\WS_Broken.dds` with the same projection
//! ([`crate::damage::crack_uv`]): a white texture whose alpha holds two spider-web cracks, one in each half, at the
//! places `WS_Broken.dds` has its stars (the left half's at `(0.06, 0.17)`, the right half's at `(0.77, 0.62)`), so
//! the windows the game cracks crack at the same spots. The cracks are drawn in metres on the glass: `ProjectUVs` maps
//! 1 m across to 0.6 of `u` and 1 m up to 2 of `v` (front and rear windows), so a round crack is an ellipse here.
//!
//! The pattern: a crushed core, radial cracks that wander and fork, and ring cracks between neighbouring rays, as
//! laminated glass breaks round an impact. Deterministic.

use crate::particles::sim::Rng;
use glam::Vec2;

/// Texels across and up.
pub const SIZE: usize = 1024;
/// The impacts, in uv: the stars of `WS_Broken.dds`.
const IMPACTS: [[f32; 2]; 2] = [[0.06, 0.17], [0.77, 0.62]];
/// Uv per metre across and up the glass (`ProjectUVs`' front-window scales; `v` runs down).
const UV_PER_METRE: Vec2 = Vec2::new(0.6, -2.0);
/// Radial cracks per impact, their length (m), and the step a crack takes before it turns (m).
const RAYS: [usize; 2] = [14, 20];
const RAY_LENGTH: [f32; 2] = [0.18, 0.55];
const RAY_STEP: f32 = 0.012;
/// The chance a ray forks at a step, and a fork's share of what is left of its ray.
const FORK: f32 = 0.06;
const FORK_SHARE: f32 = 0.45;
/// The ring cracks' radii (m), the chance each span between two rays has one, and how far it sags inward.
const RINGS: [f32; 5] = [0.018, 0.04, 0.07, 0.11, 0.16];
const RING_CHANCE: f32 = 0.65;
const RING_SAG: f32 = 0.12;
/// The crushed core's radius (m) and its short cracks.
const CORE: f32 = 0.012;
const CORE_CRACKS: usize = 60;

/// The decal's alpha, `SIZE` x `SIZE`, row by row (0 clear, 1 a crack).
pub fn alpha() -> Vec<f32> {
    let mut canvas = Canvas { alpha: vec![0.0; SIZE * SIZE] };
    let mut rng = Rng::new(0x0c4a_c7ed);
    for impact in IMPACTS {
        let centre = Vec2::from(impact);
        let to_uv = |p: Vec2| centre + p * UV_PER_METRE;
        // The rays: their directions round the circle with jitter, sorted for the rings.
        let count = RAYS[0] + (rng.unit() * (RAYS[1] - RAYS[0]) as f32) as usize;
        let mut rays: Vec<Vec<Vec2>> = Vec::new();
        for k in 0..count {
            let angle = (k as f32 + rng.unit() * 0.7) / count as f32 * std::f32::consts::TAU;
            let length = RAY_LENGTH[0] + (RAY_LENGTH[1] - RAY_LENGTH[0]) * rng.unit();
            let ray = crack(&mut canvas, &mut rng, Vec2::ZERO, angle, length, 1.0, &to_uv);
            rays.push(ray);
        }
        for &radius in &RINGS {
            for k in 0..rays.len() {
                if rng.unit() > RING_CHANCE {
                    continue;
                }
                let (a, b) = (at_radius(&rays[k], radius), at_radius(&rays[(k + 1) % rays.len()], radius));
                let (Some(a), Some(b)) = (a, b) else { continue };
                // A ring span bows inward a little.
                let mid = (a + b) * 0.5 * (1.0 - RING_SAG * rng.unit());
                canvas.line(to_uv(a), to_uv(mid), 0.7);
                canvas.line(to_uv(mid), to_uv(b), 0.7);
            }
        }
        for _ in 0..CORE_CRACKS {
            let from = Vec2::from_angle(rng.unit() * std::f32::consts::TAU) * CORE * rng.unit();
            let to = from + Vec2::from_angle(rng.unit() * std::f32::consts::TAU) * CORE * 0.6;
            canvas.line(to_uv(from), to_uv(to), 0.9);
        }
    }
    canvas.alpha
}

/// A crack from `from` (m from the impact) heading `angle` for `length` metres, wandering and sometimes forking;
/// returns its points.
fn crack(
    canvas: &mut Canvas,
    rng: &mut Rng,
    from: Vec2,
    mut angle: f32,
    length: f32,
    strength: f32,
    to_uv: &impl Fn(Vec2) -> Vec2,
) -> Vec<Vec2> {
    let mut points = vec![from];
    let mut at = from;
    let mut gone = 0.0;
    while gone < length {
        angle += (rng.unit() - 0.5) * 0.35;
        let step = RAY_STEP * (0.5 + rng.unit());
        let next = at + Vec2::from_angle(angle) * step;
        // Fainter toward the end.
        canvas.line(to_uv(at), to_uv(next), strength * (1.0 - 0.6 * gone / length));
        at = next;
        gone += step;
        points.push(at);
        if rng.unit() < FORK && strength > 0.5 {
            let side = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
            let turn = side * (0.3 + 0.4 * rng.unit());
            crack(canvas, rng, at, angle + turn, (length - gone) * FORK_SHARE, 0.7, to_uv);
        }
    }
    points
}

/// The first point of a ray at least `radius` from the impact.
fn at_radius(ray: &[Vec2], radius: f32) -> Option<Vec2> {
    ray.iter().copied().find(|p| p.length() >= radius)
}

struct Canvas {
    alpha: Vec<f32>,
}

impl Canvas {
    /// An antialiased line about a texel and a half wide between two uv points, at most `strength`.
    fn line(&mut self, a: Vec2, b: Vec2, strength: f32) {
        let scale = SIZE as f32;
        let (a, b) = (a * scale, b * scale);
        let (lo, hi) = (a.min(b) - 2.0, a.max(b) + 2.0);
        let (x0, y0) = (lo.x.max(0.0) as usize, lo.y.max(0.0) as usize);
        let (x1, y1) = ((hi.x.max(0.0) as usize).min(SIZE - 1), (hi.y.max(0.0) as usize).min(SIZE - 1));
        let d = b - a;
        let len2 = d.length_squared().max(1e-6);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let t = ((p - a).dot(d) / len2).clamp(0.0, 1.0);
                let distance = p.distance(a + d * t);
                let cover = (1.0 - (distance - 0.25) / 1.0).clamp(0.0, 1.0) * strength;
                let texel = &mut self.alpha[y * SIZE + x];
                *texel = texel.max(cover);
            }
        }
    }
}

/// The decal as a `DXT3` `.dds` with its full mip chain: white, with the cracks in the 4-bit alpha.
pub fn dds() -> Vec<u8> {
    white_dxt3(SIZE, alpha(), true)
}

/// A white `DXT3` `.dds` of `size` x `size` texels (a power of two, at least 4) with `alpha` (0 to 1, row by row) and its
/// mip chain down to 4 x 4. With `keep_thin`, each mip texel is halfway between the mean and the largest of its four, so
/// that thin lines stay visible further off; else the mean.
pub fn white_dxt3(size: usize, alpha: Vec<f32>, keep_thin: bool) -> Vec<u8> {
    let mut levels = vec![alpha];
    while levels.last().map_or(0, Vec::len) > 16 {
        let last = levels.last().unwrap();
        let size = (last.len() as f32).sqrt() as usize;
        let half = size / 2;
        let next = (0..half * half)
            .map(|i| {
                let (x, y) = (i % half * 2, i / half * 2);
                // The largest of the four keeps thin cracks visible further off; a mean would fade them to nothing.
                let texels = [
                    last[y * size + x],
                    last[y * size + x + 1],
                    last[(y + 1) * size + x],
                    last[(y + 1) * size + x + 1],
                ];
                let mean = texels.iter().sum::<f32>() / 4.0;
                if keep_thin {
                    0.5 * mean + 0.5 * texels.iter().copied().fold(0.0, f32::max)
                } else {
                    mean
                }
            })
            .collect();
        levels.push(next);
    }
    let mut bytes = vec![0u8; 128];
    bytes[..4].copy_from_slice(b"DDS ");
    let put = |bytes: &mut Vec<u8>, at: usize, v: u32| bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
    put(&mut bytes, 4, 124);
    put(&mut bytes, 8, 0x2_1007);
    put(&mut bytes, 12, size as u32);
    put(&mut bytes, 16, size as u32);
    put(&mut bytes, 28, levels.len() as u32);
    put(&mut bytes, 76, 32);
    put(&mut bytes, 80, 4);
    bytes[84..88].copy_from_slice(b"DXT3");
    for level in &levels {
        let size = (level.len() as f32).sqrt() as usize;
        for by in 0..size / 4 {
            for bx in 0..size / 4 {
                // Explicit 4-bit alpha, row by row, the first texel in the low nibble; then a white colour block.
                for row in 0..4 {
                    let nibbles = (0..4).fold(0u16, |n, i| {
                        let a = level[(by * 4 + row) * size + bx * 4 + i];
                        n | ((a.clamp(0.0, 1.0) * 15.0).round() as u16) << (4 * i)
                    });
                    bytes.extend_from_slice(&nibbles.to_le_bytes());
                }
                bytes.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
            }
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cracks_centre_on_the_stars_and_the_dds_parses() {
        let alpha = alpha();
        let covered = |u: f32, v: f32, r: usize| {
            let (x, y) = ((u * SIZE as f32) as usize, (v * SIZE as f32) as usize);
            let mut sum = 0.0;
            for j in y.saturating_sub(r)..(y + r).min(SIZE) {
                for i in x.saturating_sub(r)..(x + r).min(SIZE) {
                    sum += alpha[j * SIZE + i];
                }
            }
            sum / (4 * r * r) as f32
        };
        // Dense at the impacts, clear between them.
        assert!(covered(0.06, 0.17, 12) > 0.3);
        assert!(covered(0.77, 0.62, 12) > 0.3);
        assert!(covered(0.45, 0.95, 12) < 0.05);
        let bytes = dds();
        let dds = rbr_formats::dds::parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.mip_count), (SIZE as u32, SIZE as u32, 9));
        assert_eq!(alpha, super::alpha(), "deterministic");
    }
}

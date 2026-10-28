//! The spray's sprites, drawn here rather than taken from the game's `particles.dat`: a soft lumpy puff (dust, mist,
//! snow powder) and a drop (water, drawn streaked along its motion), each white, the pool's colour tinting it. RGBA,
//! [`SIZE`] square, straight alpha.

use crate::particles::gpu::SPRITE_SIZE as SIZE;

/// The sprite ids the pools name, past the game's (`particletextures.ini` has a few dozen).
pub const PUFF: u32 = 0x1000;
pub const DROP: u32 = 0x1001;
pub const WISP: u32 = 0x1002;

/// Every sprite of the spray, by id.
pub fn all() -> [(u32, Vec<u8>); 3] {
    [(PUFF, draw(puff)), (DROP, draw(drop)), (WISP, draw(wisp))]
}

/// A sprite of `texel` at each texel centre, `x` and `y` from -1 to 1 (y up).
fn draw(texel: fn(f32, f32) -> [f32; 4]) -> Vec<u8> {
    let n = SIZE as usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for row in 0..n {
        for col in 0..n {
            let x = (col as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let y = 1.0 - (row as f32 + 0.5) / n as f32 * 2.0;
            rgba.extend(texel(x, y).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    rgba
}

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6b343) ^ (y as u32).wrapping_mul(0xd8163841);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h & 0xffff) as f32 / 65535.0
}

/// Value noise of period `period` cells over the sprite, wrapping so the sprite has no seam.
fn noise(x: f32, y: f32, cells: f32) -> f32 {
    let (u, v) = ((x * 0.5 + 0.5) * cells, (y * 0.5 + 0.5) * cells);
    let (i, j) = (u.floor(), v.floor());
    let (fu, fv) = (u - i, v - j);
    let (su, sv) = (fu * fu * (3.0 - 2.0 * fu), fv * fv * (3.0 - 2.0 * fv));
    let c = cells as i32;
    let at = |a: f32, b: f32| hash((a as i32).rem_euclid(c), (b as i32).rem_euclid(c));
    let top = at(i, j) + (at(i + 1.0, j) - at(i, j)) * su;
    let bottom = at(i, j + 1.0) + (at(i + 1.0, j + 1.0) - at(i, j + 1.0)) * su;
    top + (bottom - top) * sv
}

/// A soft ball whose edge breaks into lumps.
fn puff(x: f32, y: f32) -> [f32; 4] {
    let r2 = x * x + y * y;
    let lumps = 0.55 * noise(x, y, 4.0) + 0.3 * noise(x, y, 8.0) + 0.15 * noise(x, y, 16.0);
    let body = (1.0 - r2).max(0.0).powi(2);
    let alpha = (body * (0.55 + 0.9 * lumps) - 0.08).max(0.0);
    [1.0, 1.0, 1.0, alpha.min(1.0)]
}

/// A puff of spray: the same ball, but its noise cuts clear gaps, so overlapping ones leave tendrils and holes.
fn wisp(x: f32, y: f32) -> [f32; 4] {
    let r2 = x * x + y * y;
    let lumps = 0.5 * noise(x, y, 3.0) + 0.3 * noise(x, y, 6.0) + 0.2 * noise(x, y, 12.0);
    let alpha = (1.0 - r2).max(0.0).powf(1.5) * smooth(0.3, 0.7, lumps);
    [1.0, 1.0, 1.0, alpha]
}

/// A round drop, bright at its middle.
fn drop(x: f32, y: f32) -> [f32; 4] {
    let r = (x * x + y * y).sqrt();
    let alpha = 1.0 - smooth(0.55, 1.0, r);
    let core = 1.0 - smooth(0.0, 0.5, r);
    let shade = 0.8 + 0.2 * core;
    [shade, shade, shade, alpha]
}

fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprites_are_clear_at_their_corners_and_solid_inside() {
        for (id, rgba) in all() {
            assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
            assert_eq!(rgba[3], 0, "sprite {id:#x}'s corner");
            let solid = rgba.chunks(4).map(|texel| texel[3]).max().unwrap();
            assert!(solid > 128, "sprite {id:#x}'s solid part");
        }
    }
}

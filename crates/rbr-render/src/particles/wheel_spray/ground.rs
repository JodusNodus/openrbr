//! What the ground under a wheel is made of, by its surface id, and where the renderer's puddles lie.

use std::sync::LazyLock;

use rbr_formats::lsp_keys::{Class, KEYS};

/// The kind of ground a wheel throws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// Bushes, trees, rocks, walls: nothing thrown.
    #[default]
    None,
    Gravel,
    Dirt,
    Grass,
    Tarmac,
    Snow,
    Water,
}

/// A surface as the spray sees it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Material {
    pub kind: Kind,
    /// 0 dry, 0.5 damp, 1 wet.
    pub wet: f32,
    /// How much loose stuff lies on it, 0 to 1: deeper gravel, softer dirt, gravel on tarmac.
    pub loose: f32,
}

/// [`Material`] by the exe's `MaterialManager` name (`Gravel_Coarse_Shallow_Wet`, `BR2_Dirt_Hard_Damp`, ...).
pub fn by_name(name: &str) -> Material {
    let name = name.trim_start_matches("US2_").trim_start_matches("BR2_");
    let has = |part: &str| name.contains(part);
    let wet = if has("_Wet") {
        1.0
    } else if has("_Damp") {
        0.5
    } else {
        0.0
    };
    let pick = |pairs: &[(&str, f32)], default: f32| pairs.iter().find(|(p, _)| has(p)).map_or(default, |&(_, v)| v);
    let (kind, loose) = if name.starts_with("Gravel_") {
        (Kind::Gravel, pick(&[("_Deeper", 1.0), ("_Deep", 0.8)], 0.6))
    } else if name.starts_with("Dirt_") {
        (Kind::Dirt, pick(&[("_Soft", 0.9), ("_Medium", 0.6)], 0.4))
    } else if name.starts_with("Rough_") {
        (Kind::Dirt, 0.5)
    } else if name.starts_with("Grass_") {
        (Kind::Grass, pick(&[("_Soft", 0.6), ("_Medium", 0.45)], 0.3))
    } else if name.starts_with("Tarmac_") {
        (Kind::Tarmac, pick(&[("_Covered", 0.5), ("_Sprinkled", 0.25)], 0.0))
    } else if name.starts_with("SnowOn") {
        (Kind::Snow, pick(&[("_Deep", 1.0), ("_Medium", 0.7)], 0.4))
    } else if name == "Water_Surface" {
        (Kind::Water, 0.0)
    } else {
        (Kind::None, 0.0)
    };
    let wet = if kind == Kind::Water { 1.0 } else { wet };
    Material { kind, wet, loose }
}

static MATERIALS: LazyLock<[Material; 256]> = LazyLock::new(|| {
    let mut by_id = [Material::default(); 256];
    for key in KEYS.iter().filter(|k| k.class == Class::MaterialManager) {
        if let Some(m) = by_id.get_mut(key.offset as usize) {
            *m = by_name(key.key);
        }
    }
    by_id
});

/// The material of surface id `id`: [`Kind::None`] for an id the exe's table lacks.
pub fn material(id: u32) -> Material {
    MATERIALS.get(id as usize).copied().unwrap_or_default()
}

pub fn smoothstep([from, to]: [f32; 2], x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `water.wgsl`'s puddles: one in `PUDDLE_SHARE` of the `PUDDLE_CELL` squares, an ellipse of `PUDDLE_SIZE` of the
/// square, on ground no steeper than `PUDDLE_GRADE`.
const PUDDLE_CELL: f32 = 4.0;
const PUDDLE_SHARE: f32 = 0.7;
const PUDDLE_SIZE: [f32; 2] = [0.18, 0.3];
pub const PUDDLE_GRADE: [f32; 2] = [0.08, 0.15];

/// `water.wgsl`'s `puddle_hash`.
fn puddle_hash(n: u32) -> [f32; 3] {
    let x = n.wrapping_mul(747796405).wrapping_add(2891336453);
    [x, x.wrapping_mul(1664525).wrapping_add(1013904223), x.wrapping_mul(22695477).wrapping_add(1)].map(|v| {
        let v = (v ^ (v >> 16)).wrapping_mul(2246822519);
        let v = (v ^ (v >> 13)).wrapping_mul(3266489917);
        ((v ^ (v >> 16)) & 0xffffff) as f32 / 16777216.0
    })
}

/// `water.wgsl`'s `puddle_shape` at stage `(x, z)` (physics x, y), on level ground, without its rim noise and the
/// hollows' growth (a splash may start a little off a drawn rim).
pub fn puddle_cover(x: f32, z: f32) -> f32 {
    let q = [x / PUDDLE_CELL, z / PUDDLE_CELL];
    let first = q.map(|c| (c - 0.5).floor());
    let mut cover = 0.0f32;
    for k in 0..4 {
        let cell = [first[0] + (k % 2) as f32, first[1] + (k / 2) as f32];
        let n = (cell[0] as i32 as u32).wrapping_mul(73856093) ^ (cell[1] as i32 as u32).wrapping_mul(19349663);
        let h = puddle_hash(n);
        if h[0] > PUDDLE_SHARE {
            continue;
        }
        let g = puddle_hash(n ^ 0x9e3779b9);
        let d = [q[0] - cell[0] - g[0], q[1] - cell[1] - g[1]];
        let c = 2.0 * g[2] - 1.0;
        let turn = [c, (1.0 - c * c).sqrt()];
        let across = [turn[0] * d[0] + turn[1] * d[1], (turn[0] * d[1] - turn[1] * d[0]) * (1.0 + 0.6 * h[2])];
        let radius = PUDDLE_SIZE[0] + (PUDDLE_SIZE[1] - PUDDLE_SIZE[0]) * h[1];
        let length = (across[0] * across[0] + across[1] * across[1]).sqrt();
        cover = cover.max(1.0 - smoothstep([0.7, 1.0], length / radius));
    }
    cover
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_by_name() {
        let m = by_name("BR2_Gravel_Coarse_Deep_Wet");
        assert_eq!((m.kind, m.wet, m.loose), (Kind::Gravel, 1.0, 0.8));
        assert_eq!(by_name("Dirt_Hard_Damp").wet, 0.5);
        assert_eq!(by_name("Tarmac_Fine_Clean_Dry").loose, 0.0);
        assert_eq!(by_name("Tarmac_Medium_Covered_Wet").loose, 0.5);
        assert_eq!(by_name("SnowOnIce_Deep").kind, Kind::Snow);
        assert_eq!(by_name("Water_Surface").wet, 1.0);
        assert_eq!(by_name("Tree_Large").kind, Kind::None);
        assert!(MATERIALS.iter().any(|m| m.kind == Kind::Gravel && m.wet == 1.0), "the exe's table names wet gravel");
    }

    #[test]
    fn puddles_cover_part_of_the_road() {
        let points = (0..200).flat_map(|i| (0..200).map(move |j| (i as f32 * 0.37, j as f32 * 0.41)));
        let covered = points.filter(|&(x, z)| puddle_cover(x, z) > 0.5).count() as f32 / 40000.0;
        // 0.7 of the squares hold an ellipse about a quarter of the square across: a few percent of the ground.
        assert!((0.02..0.25).contains(&covered), "{covered}");
    }
}

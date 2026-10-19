//! How directional a stage's baked vertex light is (explore/time-of-day, ours). Loads two variants of one stage,
//! pairs their world solid vertices by position, and fits each variant's vertex luminance against the facing of its
//! own `SunDir` and of the other's. If the bake holds the sun, a variant follows its own sun better than the other's,
//! and the two variants' light differs where the facings differ.
//!
//! RBR_DATA=<install> cargo run --release -p rbr-render --example baked_light -- track-54_M track-54_E

use glam::Vec3;
use rbr_formats::data::Data;
use rbr_render::stage::{self, Anchor, Layer, Selection, Stage};
use std::collections::HashMap;
use std::error::Error;

/// Per vertex: luminance 0-1, area-weighted face normal (unit), and whether it is on a shadow-mapped ground draw.
struct Sample {
    luminance: f32,
    normal: Vec3,
    ground: bool,
    /// On an alpha-tested draw (foliage, fences, cut-outs).
    cutout: bool,
}

fn key(p: [f32; 3]) -> (i32, i32, i32) {
    let q = |v: f32| (v * 100.0).round() as i32;
    (q(p[0]), q(p[1]), q(p[2]))
}

fn samples(stage: &Stage) -> HashMap<(i32, i32, i32), Sample> {
    let mut out = HashMap::new();
    for mesh in stage.meshes.iter().filter(|m| m.layer == Layer::Solid && m.anchor == Anchor::World) {
        let mut normals = vec![Vec3::ZERO; mesh.vertices.len()];
        let mut ground = vec![false; mesh.vertices.len()];
        let mut cutout = vec![false; mesh.vertices.len()];
        for draw in mesh.draws.iter().filter(|d| d.state.shade.is_none()) {
            for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                let p = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
                let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
                for &i in t {
                    normals[i as usize] += n;
                    ground[i as usize] |= draw.state.shadow.is_some();
                    cutout[i as usize] |= draw.state.alpha_ref.is_some();
                }
            }
        }
        for (((v, n), g), c) in mesh.vertices.iter().zip(normals).zip(ground).zip(cutout) {
            if n.length_squared() == 0.0 {
                continue;
            }
            // D3DCOLOR order: b, g, r, a.
            let [b, g_, r, _] = v.color.map(|c| c as f32 / 255.0);
            let luminance = 0.2126 * r + 0.7152 * g_ + 0.0722 * b;
            out.insert(key(v.position), Sample { luminance, normal: n.normalize(), ground: g, cutout: c });
        }
    }
    out
}

/// Least squares `y = a + b x`: returns (a, b, r).
fn fit(xs: &[f32], ys: &[f32]) -> (f32, f32, f32) {
    let n = xs.len() as f64;
    let (sx, sy) = (xs.iter().map(|&x| x as f64).sum::<f64>(), ys.iter().map(|&y| y as f64).sum::<f64>());
    let (mx, my) = (sx / n, sy / n);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (&x, &y) in xs.iter().zip(ys) {
        let (dx, dy) = (x as f64 - mx, y as f64 - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let b = sxy / sxx;
    ((my - b * mx) as f32, b as f32, (sxy / (sxx * syy).sqrt()) as f32)
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (a_name, b_name) = (args.next().ok_or("two stage names")?, args.next().ok_or("two stage names")?);
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let a = stage::load(&maps, &a_name, Selection::default())?;
    let b = stage::load(&maps, &b_name, Selection::default())?;
    let (sa, sb) = (samples(&a), samples(&b));
    // Toward the sun; `SunDir` is the direction the light travels.
    let (to_a, to_b) = (-a.atmosphere.sun_direction, -b.atmosphere.sun_direction);
    println!("{a_name}: {} vertices, to sun {to_a:.3}", sa.len());
    println!("{b_name}: {} vertices, to sun {to_b:.3}", sb.len());

    // The winding sign: most ground faces point up.
    let up = sa.values().filter(|s| s.ground).map(|s| s.normal.y.signum()).sum::<f32>().signum();
    let kind = |s: &Sample| {
        if s.ground {
            0
        } else if s.cutout {
            2
        } else {
            1
        }
    };
    for k in 0..3 {
        let label = ["ground (shadow-mapped) draws", "other opaque unlit draws", "alpha-tested draws"][k];
        let total = sa.values().filter(|s| kind(s) == k).count();
        let total_b = sb.values().filter(|s| kind(s) == k).count();
        println!("\n{label}: {total} / {total_b} vertices in the variants");
        let pairs: Vec<(&Sample, &Sample)> =
            sa.iter().filter(|(_, s)| kind(s) == k).filter_map(|(k, s)| Some((s, sb.get(k)?))).collect();
        if pairs.len() < 100 {
            println!("  {} paired, skipped", pairs.len());
            continue;
        }
        let facing = |s: &Sample, to: Vec3| (up * s.normal).dot(to).max(0.0);
        let col = |f: &dyn Fn(&(&Sample, &Sample)) -> f32| pairs.iter().map(f).collect::<Vec<f32>>();
        let (la, lb) = (col(&|p| p.0.luminance), col(&|p| p.1.luminance));
        let (fa_a, fa_b) = (col(&|p| facing(p.0, to_a)), col(&|p| facing(p.0, to_b)));
        let diff = col(&|p| p.0.luminance - p.1.luminance);
        let fdiff = col(&|p| facing(p.0, to_a) - facing(p.0, to_b));
        let same = pairs.iter().filter(|p| (p.0.luminance - p.1.luminance).abs() < 1.5 / 255.0).count();
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        let mad = diff.iter().map(|d| d.abs()).sum::<f32>() / diff.len() as f32;
        println!("  {} paired", pairs.len());
        println!(
            "  mean luminance {:.3} / {:.3}; mean |diff| {:.3}; identical {:.1}%",
            mean(&la),
            mean(&lb),
            mad,
            100.0 * same as f32 / pairs.len() as f32
        );
        for (name, l, fo, fx) in [(&a_name, &la, &fa_a, &fa_b), (&b_name, &lb, &fa_b, &fa_a)] {
            let (c0, c1, r_own) = fit(fo, l);
            let (_, _, r_other) = fit(fx, l);
            println!("  {name}: L = {c0:.3} + {c1:.3}*facing(own sun), r {r_own:.3}; r vs other sun {r_other:.3}");
        }
        let (_, k, r) = fit(&fdiff, &diff);
        println!("  L_a - L_b vs facing_a - facing_b: slope {k:.3}, r {r:.3}");
        if label.starts_with("other") {
            // The shader's relight of a's light to b's sun (`relight.rs`, `relit` in `shader.wgsl`), scored against b's
            // own bake: raw, and with each side's mean taken out (the relight does not move the overall level).
            let relight = rbr_render::relight::fit(&a);
            let to_a = relight.bake_sun;
            println!(
                "  fitted bake sun {to_a:.3} (r {:.3}); {a_name}'s SunDir gives {:.3}",
                relight.r, -a.atmosphere.sun_direction
            );
            let relit: Vec<f32> = pairs
                .iter()
                .map(|p| {
                    let l = p.0.luminance;
                    (l + relight.strength * (facing(p.0, to_b) - facing(p.0, to_a))).max(0.35 * l)
                })
                .collect();
            let score = |x: &[f32]| {
                let (mx, mb) = (mean(x), mean(&lb));
                let raw = x.iter().zip(&lb).map(|(x, y)| (x - y).abs()).sum::<f32>() / x.len() as f32;
                let centred = x.iter().zip(&lb).map(|(x, y)| ((x - mx) - (y - mb)).abs()).sum::<f32>() / x.len() as f32;
                let (_, _, r) = fit(x, &lb);
                format!("mean |error| {raw:.3}, centred {centred:.3}, r {r:.3}")
            };
            println!("  as baked ({a_name}) vs {b_name}: {}", score(&la));
            println!("  relit (b {:.3}) vs {b_name}:   {}", relight.strength, score(&relit));
        }
    }
    Ok(())
}

#![allow(clippy::needless_range_loop)]
//! Mean linear albedo of a stage's world surfaces (measurement for the colour calibration, ours): each solid world
//! triangle's texture sampled at a few points (5x5 texel boxes, wrapped), weighted by area, binned by the texel's
//! colour into grey (road, rock, snow), brown (dirt, gravel, sand) and green (grass, foliage), for the terrain cells
//! (`cell`) and the objects (plants, rocks, houses) apart.
//!
//! RBR_DATA=<install> cargo run --release -p rbr-render --example albedo_survey -- track-41_M

use rbr_formats::data::Data;
use rbr_render::stage::{self, Selection};
use rbr_render::start_board::Rgba;
use std::collections::HashMap;

fn lin(v: u8) -> f64 {
    (f64::from(v) / 255.0).powf(2.2)
}

#[derive(Default, Clone, Copy)]
struct Bin {
    area: f64,
    rgb: [f64; 3],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = Data::from_env().ok_or("RBR_DATA")?;
    let maps = data.install().join("Maps");
    let name = std::env::args().nth(1).unwrap();
    let mut s = stage::load(&maps, &name, Selection::default())?;
    s.load_textures(&maps)?;
    let mut decoded: HashMap<_, Option<Rgba>> = HashMap::new();
    // bins[terrain?][class]
    let mut bins = [[Bin::default(); 3]; 2];
    for m in s.meshes.iter().filter(|m| m.anchor == stage::Anchor::World && m.layer == stage::Layer::Solid) {
        let terrain = usize::from(m.cell.is_some());
        for d in &m.draws {
            let Some(t) = d.state.textures[0] else { continue };
            let tex = decoded.entry(t).or_insert_with(|| match s.textures.get(&t)?.image.as_ref()? {
                stage::Image::Dds(b) => Some(Rgba::decode(&rbr_formats::dds::parse(b).ok()?)),
                _ => None,
            });
            let Some(tex) = tex else { continue };
            for tri in m.indices[d.indices.clone()].chunks_exact(3) {
                let v = |i: u32| &m.vertices[i as usize];
                let p = |i: u32| glam::Vec3::from(v(i).position);
                let area = f64::from((p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0])).length()) * 0.5;
                if area <= 0.0 {
                    continue;
                }
                for w in [[1.0 / 3.0; 3], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2], [0.2, 0.2, 0.6]] {
                    let uv = |c: usize| tri.iter().zip(w).map(|(&i, w)| v(i).uv0[c] * w).sum::<f32>();
                    let (u, vv) = (uv(0).rem_euclid(1.0), uv(1).rem_euclid(1.0));
                    let (cx, cy) = ((u * tex.width as f32) as i32, (vv * tex.height as f32) as i32);
                    let (mut sum, mut n) = ([0.0f64; 3], 0.0);
                    for dy in -2..=2 {
                        for dx in -2..=2 {
                            let x = (cx + dx).rem_euclid(tex.width as i32) as usize;
                            let y = (cy + dy).rem_euclid(tex.height as i32) as usize;
                            let i = (y * tex.width as usize + x) * 4;
                            if tex.pixels[i + 3] < 128 {
                                continue;
                            }
                            for c in 0..3 {
                                sum[c] += lin(tex.pixels[i + c]);
                            }
                            n += 1.0;
                        }
                    }
                    if n == 0.0 {
                        continue;
                    }
                    let c = sum.map(|v| v / n);
                    let class = if c[1] > c[0] * 1.12 && c[1] > c[2] * 1.4 {
                        2
                    } else if c[0] > c[2] * 1.35 {
                        1
                    } else {
                        0
                    };
                    let b = &mut bins[terrain][class];
                    b.area += area / 4.0;
                    for k in 0..3 {
                        b.rgb[k] += c[k] * area / 4.0;
                    }
                }
            }
        }
    }
    for (t, label) in [(1, "terrain"), (0, "objects")] {
        let total: f64 = bins[t].iter().map(|b| b.area).sum();
        for (c, class) in ["grey", "brown", "green"].iter().enumerate() {
            let b = bins[t][c];
            if b.area == 0.0 {
                continue;
            }
            let m = b.rgb.map(|v| v / b.area);
            let (mx, mn) = (m[0].max(m[1]).max(m[2]), m[0].min(m[1]).min(m[2]));
            let y = 0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2];
            println!(
                "{name} {label:<8} {class:<6} {:5.1}%  lin rgb {:.3} {:.3} {:.3}  Y {:.3}  sat {:.2}",
                100.0 * b.area / total,
                m[0],
                m[1],
                m[2],
                y,
                (mx - mn) / mx.max(1e-6)
            );
        }
    }
    Ok(())
}

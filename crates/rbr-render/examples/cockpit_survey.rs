//! The cockpit eye of each retail car and, with `RBR_RSF`, each RSF sample car: how far it sits behind `Cam_internal`'s
//! `Pos`, its pitch, and the nearest interior surface the default frame shows from it and from it [`DIP`] lower (as
//! `views::cockpit_back` checks it, on a finer grid), with what lies nearer than [`NEAR`]. Usage:
//! `cockpit_survey [CAR...]` (RSF cars by id or pack), or `cockpit_survey --shake DRIVE...` for the seat shake's
//! extremes over recorded drives.

use glam::Vec3;
use rbr_formats::car_parts;
use rbr_formats::data::Data;
use rbr_render::car;
use rbr_render::coords;
use rbr_render::playback::{Drive, CAMERA_SHAKE};
use rbr_render::stage::Mesh;
use rbr_render::views::{self, Mount, Mounts};
use std::error::Error;
use std::path::PathBuf;

/// `views::FRAME_CLEAR` and `views::SHAKE_DIP`.
const NEAR: f32 = 0.35;
const DIP: f32 = 0.03;
/// The default frame's half height, degrees (50 degrees across at 16:9).
const HALF_VERTICAL: f32 = 14.7;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--shake") {
        return shake(&args[1..]);
    }
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let mut cars: Vec<(String, Data, String)> = Vec::new();
    let retail = ["XSARA", "ACCENT", "ZR", "LANCER_EVO_VII", "PEUGEOT_206", "IMPREZA03", "COROLLA", "IMPREZA00"];
    for name in retail {
        if args.is_empty() || args.iter().any(|w| w.eq_ignore_ascii_case(name)) {
            cars.push((name.to_owned(), data.clone(), name.to_owned()));
        }
    }
    if let Some(root) = std::env::var_os("RBR_RSF").map(PathBuf::from) {
        let install = rbr_formats::rsf::Install::open(&root)?;
        for car in &install.cars {
            let key = car.id.to_string();
            if !args.is_empty() && !args.contains(&key) && !args.contains(&car.pack) {
                continue;
            }
            match rbr_formats::rsf::select(data.clone(), &key, rbr_formats::rsf::SLOT, Some(&root)) {
                Ok(selected) if selected.model.fallback.is_none() => {
                    let folder = selected.folder().to_owned();
                    cars.push((format!("rsf-{} {}", car.id, car.name), selected.data, folder))
                }
                Ok(_) => println!("rsf-{} {}: no model of its own", car.id, car.name),
                Err(e) if e.contains("no pack") => {}
                Err(e) => println!("rsf-{} {}: {e}", car.id, car.name),
            }
        }
    }
    for (label, data, folder) in cars {
        let interior = car::load_interior(&data, &folder, car::placement_at(None))?;
        let exterior = car::load(&data, &folder, car::placement_at(None))?;
        let (lo, hi) = exterior.bounds().ok_or("no geometry")?;
        let mounts = Mounts::load(&data, &folder, lo, hi, &exterior.meshes, &interior.meshes)?;
        let sections = car::camera_sections(&data, &folder)?;
        let internal =
            car_parts::section(&sections, "Cam_internal").and_then(Mount::from_section).ok_or("no camera")?;
        let m = mounts.cockpit;
        let look = m.target - m.eye;
        let pitch = (-look.y).atan2(look.z.hypot(look.x)).to_degrees();
        let hits = frame_hits(&interior.meshes, m);
        let nearest = hits.iter().min_by(|a, b| a.d.total_cmp(&b.d));
        let nearest = nearest.map_or("nothing".into(), |h| format!("{:.2} m at ({:+.0}, {:+.0})", h.d, h.across, h.up));
        println!("{label} [{folder}]: back {:.2} m, pitch {pitch:.1} deg, nearest {nearest}", m.eye.z - internal.eye.z);
        let mut near: std::collections::BTreeMap<usize, Vec<&Hit>> = Default::default();
        for h in hits.iter().filter(|h| h.d < NEAR) {
            near.entry(h.mesh).or_default().push(h);
        }
        for (mesh, hits) in near {
            let range = |f: fn(&Hit) -> f32| {
                hits.iter().map(|&h| f(h)).fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)))
            };
            let (across, up, d) = (range(|h| h.across), range(|h| h.up), range(|h| h.d));
            println!(
                "    mesh {mesh}: {} lines, {:.2} to {:.2} m, across {:+.0} to {:+.0} deg, up {:+.0} to {:+.0} deg",
                hits.len(),
                d.0,
                d.1,
                across.0,
                across.1,
                up.0,
                up.1
            );
        }
    }
    Ok(())
}

/// The first shown surface along a line through the default frame: the line's angles from the view axis (right and
/// up, degrees), its distance from the eye, and its mesh.
struct Hit {
    across: f32,
    up: f32,
    d: f32,
    mesh: usize,
}

/// Along 17 by 11 lines across the default frame, from `mount`'s eye and from it [`DIP`] lower, the first `interior`
/// surface drawn (not blended, facing the eye where culled at its back, beyond the near plane).
fn frame_hits(interior: &[Mesh], mount: Mount) -> Vec<Hit> {
    let ahead = (mount.target - mount.eye).normalize();
    let right = ahead.cross(Vec3::Y).normalize();
    let up = right.cross(ahead);
    let across = (coords::FOV_DEGREES.to_radians() * 0.5).tan();
    let down = HALF_VERTICAL.to_radians().tan();
    let mut hits = Vec::new();
    for eye in [mount.eye, mount.eye - Vec3::Y * DIP] {
        for i in -8..=8 {
            for j in -5..=5 {
                let (x, y) = (across * i as f32 / 8.0, down * j as f32 / 5.0);
                let dir = (ahead + right * x + up * y).normalize();
                let mut first: Option<Hit> = None;
                for (k, mesh) in interior.iter().enumerate() {
                    for draw in mesh.draws.iter().filter(|d| !d.state.blend) {
                        for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                            let at = |n: usize| Vec3::from(mesh.vertices[t[n] as usize].position);
                            let t = [at(0), at(1), at(2)];
                            if draw.state.cull_back && (t[1] - t[0]).cross(t[2] - t[0]).dot(dir) >= 0.0 {
                                continue;
                            }
                            let Some(d) = views::ray_hit(eye, dir, t).filter(|d| d * dir.dot(ahead) > mount.near)
                            else {
                                continue;
                            };
                            if first.as_ref().is_none_or(|f| d < f.d) {
                                let (across, up) = (x.atan().to_degrees(), y.atan().to_degrees());
                                first = Some(Hit { across, up, d, mesh: k });
                            }
                        }
                    }
                }
                hits.extend(first);
            }
        }
    }
    hits
}

/// Per drive, the in-car views' seat shake (`CAMERA_SHAKE`, body frame: x left, y rear, z up), its lowest `z` frames
/// and quantiles, and the quantiles over all of them.
fn shake(paths: &[String]) -> Result<(), Box<dyn Error>> {
    let mut pooled: Vec<f32> = Vec::new();
    for path in paths {
        let drive = Drive::parse(&std::fs::read(path)?)?;
        let samples = drive.samples();
        let (lo, hi) = samples
            .iter()
            .map(|s| s.shake[CAMERA_SHAKE])
            .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
        let mut z: Vec<(f32, u32)> = samples.iter().map(|s| (s.shake[CAMERA_SHAKE].z, s.step)).collect();
        z.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut low: Vec<(f32, u32)> = Vec::new();
        for &(v, step) in &z {
            if low.len() < 5 && low.iter().all(|&(_, s)| s.abs_diff(step) > 720) {
                low.push((v, step));
            }
        }
        let q = |p: f64| z[((z.len() - 1) as f64 * p) as usize].0;
        println!(
            "{path}: {} frames, x {:.4}..{:.4} y {:.4}..{:.4} z {:.4}..{:.4} m; z q1e-3 {:.4} median {:.4}; lowest (z, step) {low:.4?}",
            samples.len(),
            lo.x,
            hi.x,
            lo.y,
            hi.y,
            lo.z,
            hi.z,
            q(1e-3),
            q(0.5)
        );
        pooled.extend(z.iter().map(|&(v, _)| v));
    }
    pooled.sort_by(f32::total_cmp);
    let q = |p: f64| pooled[((pooled.len() - 1) as f64 * p) as usize];
    println!(
        "all {} frames: z min {:.4} q1e-4 {:.4} q1e-3 {:.4} q1e-2 {:.4} median {:.4}",
        pooled.len(),
        q(0.0),
        q(1e-4),
        q(1e-3),
        q(1e-2),
        q(0.5)
    );
    Ok(())
}

//! Fences (rbr-recomp `notes/interactive-scenery.md` 3.6): the stage's `.fnc` tape and net lines, drawn from the fence cloth
//! ([`cloth`]) so that a car going through one tears it, pushes it about and leaves it lying.
//!
//! Each line is one stage mesh: its spans as quads between the particle columns (post to post, every layer), its
//! posts as thin four-sided rods from the top particle to the foot (`CFenceRenderer::EmitPosts` 0x604610,
//! `EmitRails` 0x604960, `E90Seg::Emit` 0x606270, `E91EmitStates` 0x607c90). The textures are the `.fnc`'s names
//! in `Textures/fence.dat` (`CFenceRenderer::LoadTextures` 0x6058c0): the span's by the line's material, the post's
//! by its rail material; the uvs are the fence type's (`g_fenceTypes`). Ours: the vertex colour is the `.fnc`
//! post colour blended along the span for the moving cloth too (the game's moving spans are white), the end quads
//! from each post to the first inner column are drawn, the alpha test is the stage objects', and the car's contact
//! shadow (ours, `--enhance contact`) falls on them as on the ground. The game's `RenderShadows` (0x605670,
//! `crate::car_shadow`, the default) lays `car_dpshad2.tga` on the `.col` faces under the car, which hold no fences.
//!
//! A drive (`--drive`, `--live`) steps the cloth once per frame (one physics step, its own length) with the car's
//! collision body at its pose and the stage's wind; a line out of the frame's view and far from the car goes back to
//! rest (`CFenceHandler::Tic` 0x602240). A jump back starts over from the drive's first frame, so a screenshot shows
//! what the drive did to the fences.

pub mod cloth;

use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_formats::{dat, fnc};

use crate::camera::Frustum;
use crate::coords;
use crate::particles::weather::{TrackWeather, Wind};
use crate::playback::{Playback, Sample};
use crate::renderer::{mesh_bounds, Renderer};
use crate::stage::{
    Address, Anchor, Draw, DrawState, Image, Layer, Lod, Mesh, Stage, StageTexture, TextureRef, TextureSet, Vertex,
    OBJECT_ALPHA_REF,
};
use cloth::{CarShape, Fences, Line, TYPES};

/// The post rod's half width, m (`EmitRails`).
const ROD: f32 = 0.015;

/// A stage's fences, added to the stage and not yet tied to the renderer's meshes.
pub struct Loaded {
    fences: Fences,
    meshes: Vec<Mesh>,
}

/// Reads the stage's `.fnc` and its textures, and adds a mesh per line, at rest, to `stage` (before the car, whose
/// mesh indices the playback keeps). `None`, reported on stderr, when the stage has no fences.
pub fn load(data: &Data, stage: &mut Stage) -> Option<Loaded> {
    let path = rbr_formats::stage_files::file_in(&data.install().join("Maps"), &stage.name, "fnc");
    let file =
        match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| fnc::parse(&b).map_err(|e| e.to_string())) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("fences: {}: {e}", path.display());
                return None;
            }
        };
    if file.groups.is_empty() {
        return None;
    }
    let textures = textures(data, &file.textures);
    let fences = Fences::new(&file);
    let meshes: Vec<Mesh> = fences.lines.iter().map(mesh).collect();
    eprintln!(
        "fences: {} lines, {} posts, {}/{} textures",
        fences.lines.len(),
        fences.lines.iter().map(|l| l.posts.len()).sum::<usize>(),
        textures.iter().filter(|t| t.image.is_some()).count(),
        textures.len()
    );
    for (i, texture) in textures.into_iter().enumerate() {
        stage.textures.insert(TextureRef { set: TextureSet::Fence, index: i as u32 }, texture);
    }
    stage.meshes.extend(meshes.iter().cloned());
    stage.meshes.sort_by_key(|mesh| mesh.layer);
    Some(Loaded { fences, meshes })
}

/// The `.fnc`'s texture names (fixed width, NUL padded) from `Textures/fence.dat`.
fn textures(data: &Data, names: &[Vec<u8>]) -> Vec<StageTexture> {
    let path = data.install().join("Textures").join("fence.dat");
    let bytes = std::fs::read(&path).unwrap_or_default();
    let entries = dat::parse(&bytes).unwrap_or_default();
    names
        .iter()
        .map(|name| {
            let name = String::from_utf8_lossy(name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
            let file = format!("{name}.dds");
            let image = entries
                .iter()
                .find(|e| !e.is_dir && e.path.rsplit(['/', '\\']).next().is_some_and(|n| n.eq_ignore_ascii_case(&file)))
                .and_then(|e| dat::file_data(&bytes, e))
                .filter(|b| rbr_formats::dds::parse(b).is_ok())
                .map(|b| Image::Dds(b.to_vec()));
            if image.is_none() {
                eprintln!("fences: no texture {file} in {}", path.display());
            }
            StageTexture { request: format!("Textures\\{file}"), image }
        })
        .collect()
}

/// A stage's fences as the viewer draws them.
pub struct Drawn {
    fences: Fences,
    /// Each line's stage mesh.
    meshes: Vec<usize>,
    /// The car's model-space box, for a car without a collision mesh.
    car: (Vec3, Vec3),
    /// The car's collision body (`c_e_body`, grid 1) in its physics frame, and that mesh's box.
    collision: Option<(Vec<[Vec3; 3]>, Vec3, Vec3)>,
    /// The stage's wind (`track_w.ini`), stepped from the drive's start.
    weather: TrackWeather,
    wind: Wind,
    /// The last frame followed, and its step.
    last: Option<(usize, u32)>,
}

/// Ours: the fences' wind draws (`M9a20`'s random values are the game's own).
const WIND_SEED: u32 = 0x5eed_0090;

impl Drawn {
    /// Ties `loaded` to `stage`'s final mesh list, for a car whose model-space bounds are `car`.
    pub fn new(loaded: Loaded, stage: &Stage, car: (Vec3, Vec3)) -> Option<Drawn> {
        let meshes = loaded.meshes.iter().map(|m| stage.meshes.iter().position(|s| s == m)).collect::<Option<_>>()?;
        let weather = TrackWeather::default();
        Some(Drawn {
            fences: loaded.fences,
            meshes,
            car,
            collision: None,
            weather,
            wind: Wind::new(weather, WIND_SEED),
            last: None,
        })
    }

    /// Takes car `car`'s collision body in place of its box, and stage `stage`'s wind; reported on stderr. The body's
    /// vertices are in the model's axes about the model's origin; `playback`'s model offset puts them on the drawn car
    /// (ours: how the physics places the fence's car grid, `car->Vf30()`, is not traced).
    pub fn with_car_and_wind(mut self, data: &Data, car: &str, stage: &str, playback: &Playback) -> Drawn {
        let offset = playback.model_to_car().w_axis.truncate();
        match collision_faces(data, car) {
            Ok(faces) => {
                let faces: Vec<[Vec3; 3]> = faces.into_iter().map(|f| f.map(|p| p + offset)).collect();
                let points = faces.iter().flatten();
                let lo = points.clone().fold(Vec3::MAX, |lo, &p| lo.min(p));
                let hi = points.fold(Vec3::MIN, |hi, &p| hi.max(p));
                eprintln!("fences: the car is its collision body ({} faces)", faces.len());
                self.collision = Some((faces, lo, hi));
            }
            Err(e) => eprintln!("fences: {car}: {e}; the car is its model's box"),
        }
        if let Some(map) = crate::stage::map_number(stage) {
            self.weather = TrackWeather::load(data, map);
        }
        self.wind = Wind::new(self.weather, WIND_SEED);
        self
    }

    /// Follows the drive to its current frame and gives `renderer` the lines that moved. The camera seen through
    /// `view_projection` keeps the lines in its view as they are (ours: the frame's camera for every step it follows).
    pub fn draw(&mut self, renderer: &mut Renderer, playback: &Playback, view_projection: Mat4) {
        let samples = playback.samples();
        let index = playback.index();
        let (first, mut changed) = match self.last {
            Some((last, step)) if last <= index && samples[last].step == step => (last + 1, Vec::new()),
            _ => {
                self.wind = Wind::new(self.weather, WIND_SEED);
                (0, self.fences.reset())
            }
        };
        let frustum = Frustum::from_view_projection(view_projection);
        let seen = |lo: Vec3, hi: Vec3| frustum.intersects_box((lo + hi) * 0.5, (hi - lo) * 0.5);
        let ends: Vec<f64> = playback.frames().map(|(end, _)| end).take(index + 1).collect();
        for i in first..=index {
            let previous = i.checked_sub(1).map(|p| &samples[p]);
            let dt = (ends[i] - i.checked_sub(1).map_or(0.0, |p| ends[p])) as f32;
            // `M8fb0`'s wind, physics (z up) to stage axes as `Tic` swizzles it into `g_fenceWind`.
            let wind = self.wind.step(dt);
            let wind = Vec3::new(wind.x, wind.z, wind.y);
            let (to_stage, faces, lo, hi) = car_frame(&self.collision, self.car, playback, &samples[i]);
            let motion = previous.map_or(Vec3::ZERO, |p| {
                to_stage.w_axis.truncate() - car_frame(&self.collision, self.car, playback, p).0.w_axis.truncate()
            });
            let car = CarShape { to_stage, faces, lo, hi, motion };
            changed.extend(self.fences.step(dt, wind, &car, &seen));
        }
        self.last = Some((index, samples[index].step));
        changed.sort_unstable();
        changed.dedup();
        for line in changed {
            let vertices = vertices(&self.fences.lines[line]);
            let bounds = mesh_bounds(&vertices);
            renderer.set_vertices(self.meshes[line], &vertices, bounds);
        }
    }
}

/// The car's frame to stage at `sample`, and its faces and box in that frame: the collision body in the physics
/// body's frame, else the model's box `model` in the model's.
fn car_frame<'a>(
    collision: &'a Option<(Vec<[Vec3; 3]>, Vec3, Vec3)>,
    model: (Vec3, Vec3),
    playback: &Playback,
    sample: &Sample,
) -> (Mat4, &'a [[Vec3; 3]], Vec3, Vec3) {
    match collision {
        Some((faces, lo, hi)) => (crate::playback::car_to_stage(&sample.car), faces, *lo, *hi),
        None => (coords::physics_to_stage() * playback.model_to_physics(sample), &[], model.0, model.1),
    }
}

/// The faces of car `car`'s collision body (`c_e_body`) as the car loader emits it (`rbr_physics`' `car_meshes`), in
/// the physics body's frame.
fn collision_faces(data: &Data, car: &str) -> Result<Vec<[Vec3; 3]>, String> {
    let model = crate::car::model(data, car).map_err(|e| e.to_string())?;
    let bodies: Vec<rbr_formats::sgc::Collision> = model
        .into_iter()
        .filter_map(|e| match e {
            rbr_formats::sgc::Element::Collision(c) => Some(c),
            _ => None,
        })
        .collect();
    let meshes = rbr_physics::load::car_grids::car_meshes(&bodies).ok_or("no collision body")?;
    let body = meshes.iter().find(|m| m.id == 1).ok_or("no collision body")?;
    let faces: Vec<[Vec3; 3]> = body
        .faces
        .iter()
        .filter_map(|f| {
            let [a, b, c] = [f[0], f[1], f[2]].map(|i| body.vertices.get(i as usize).copied().map(Vec3::from));
            Some([a?, b?, c?])
        })
        .collect();
    if faces.is_empty() {
        return Err("the collision body has no faces".into());
    }
    Ok(faces)
}

/// A line's mesh at its current state: its spans' quads, then its posts' rods, in two draws.
fn mesh(line: &Line) -> Mesh {
    let vertices = vertices(line);
    let quads = vertices.len() / 4;
    let indices: Vec<u32> = (0..quads as u32).flat_map(|q| [0, 1, 2, 2, 1, 3].map(|k| q * 4 + k)).collect();
    let span_indices = span_quads(line) * 6;
    let draw = |range: std::ops::Range<usize>, texture: u32| Draw {
        indices: range,
        state: DrawState {
            textures: [Some(TextureRef { set: TextureSet::Fence, index: texture }), None],
            shadow: None,
            address: [Address::Wrap; 2],
            second_texture: false,
            alpha_ref: Some(OBJECT_ALPHA_REF),
            mip_bias: false,
            cull_back: false,
            blend: false,
            additive: false,
            depth_test: true,
            depth_write: true,
            shade: None,
            specular: None,
        },
        lod: Lod::Always,
    };
    let draws = vec![draw(0..span_indices, line.material), draw(span_indices..indices.len(), line.rail_material)]
        .into_iter()
        .filter(|d| !d.indices.is_empty())
        .collect();
    Mesh {
        layer: Layer::Solid,
        anchor: Anchor::World,
        bounds: mesh_bounds(&vertices),
        cell: None,
        vertices,
        indices,
        draws,
    }
}

fn span_quads(line: &Line) -> usize {
    let t = TYPES[line.span_type];
    let spans = line.span_segment.iter().flatten().count();
    if t.cols < 3 || t.rows < 2 {
        return 0;
    }
    spans * (t.layers * (t.cols - 1) * (t.rows - 1)) as usize
}

/// A line's vertices: four per quad, spans then posts.
pub fn vertices(line: &Line) -> Vec<Vertex> {
    let mut out = Vec::new();
    let t = TYPES[line.span_type];
    let vertex = |pos: Vec3, color: u32, uv: [f32; 2]| Vertex {
        position: pos.to_array(),
        color: coords::rgba_from_d3dcolor(color.to_le_bytes()),
        uv0: uv,
        uv1: [0.0; 2],
        uv2: [0.0; 2],
        normal: [0.0; 3],
        specular: [0.0; 3],
        occlusion: 0.0,
        sky_dir: 0,
        bounce: [0; 2],
        wet: 0.0,
    };
    if span_quads(line) > 0 {
        let (cols, rows, layers) = (t.cols as usize, t.rows as usize, t.layers as usize);
        let inner = cols - 2;
        for (i, span) in line.span_segment.iter().enumerate() {
            let Some(span) = *span else { continue };
            let (prev, next) = (&line.segments[line.post_segment[i]], &line.segments[line.post_segment[i + 1]]);
            let span = &line.segments[span];
            let colors = (line.posts[i].color, line.posts[i + 1].color);
            for layer in 0..layers {
                let at = |c: usize, r: usize| {
                    let index = if c == 0 {
                        prev.start + (layer * rows + r).min(prev.count - 1)
                    } else if c == cols - 1 {
                        next.start + (layer * rows + r).min(next.count - 1)
                    } else {
                        span.start + (layer * inner + c - 1) * rows + r
                    };
                    line.particles[index].pos
                };
                let uv = |c: usize, r: usize| {
                    let (u, v) = (c as f32 / (cols - 1) as f32, r as f32 / (rows - 1) as f32);
                    [t.u0 + (t.u1 - t.u0) * u, t.v0 + (t.v1 - t.v0) * v]
                };
                let color = |c: usize| blend(colors.0, colors.1, c as f32 / (cols - 1) as f32);
                for c in 0..cols - 1 {
                    for r in 0..rows - 1 {
                        for (dc, dr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            out.push(vertex(at(c + dc, r + dr), color(c + dc), uv(c + dc, r + dr)));
                        }
                    }
                }
            }
        }
    }
    let p = TYPES[line.post_type];
    let sides = [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z, Vec3::X].map(|v| v * ROD);
    for (i, &segment) in line.post_segment.iter().enumerate() {
        let post = &line.segments[segment];
        let top = line.particles[post.start + post.count - 3].pos;
        let foot = line.particles[post.start + post.count - 1].pos;
        let color = line.posts[i].color;
        for k in 0..4 {
            let (u0, u1) = if k % 2 == 0 { (p.u0, p.u1) } else { (p.u1, p.u0) };
            out.push(vertex(top + sides[k], color, [u0, p.v0]));
            out.push(vertex(foot + sides[k], color, [u0, p.v1]));
            out.push(vertex(top + sides[k + 1], color, [u1, p.v0]));
            out.push(vertex(foot + sides[k + 1], color, [u1, p.v1]));
        }
    }
    out
}

/// Two D3DCOLORs mixed, `t` of the way to `b`.
fn blend(a: u32, b: u32, t: f32) -> u32 {
    let (a, b) = (a.to_le_bytes(), b.to_le_bytes());
    u32::from_le_bytes(std::array::from_fn(|k| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t).round() as u8))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbr_formats::fnc::{FncGroup, FncPost};

    fn line(fence_type: u32, post_type: u32) -> Line {
        let posts = (0..3)
            .map(|i| FncPost {
                position: [i as f32 * 3.0, 0.0, 0.0],
                center: [0.0; 3],
                half_extent: [1.0; 3],
                color: 0xff80_8080,
            })
            .collect();
        Line::new(&FncGroup {
            fence_type,
            post_type,
            material: 0,
            rail_material: 1,
            center: [3.0, 0.0, 0.0],
            half_extent: [4.0, 1.0, 1.0],
            posts,
        })
    }

    #[test]
    fn mesh_covers_spans_and_posts() {
        let line = line(0, 4);
        let mesh = mesh(&line);
        // Two spans of 8 quads, three posts of 4 sides.
        assert_eq!(mesh.vertices.len(), (2 * 8 + 3 * 4) * 4);
        assert_eq!(mesh.indices.len(), (2 * 8 + 3 * 4) * 6);
        assert_eq!(mesh.draws.len(), 2);
        assert_eq!(mesh.draws[0].indices, 0..2 * 8 * 6);
        // The tape's first quad starts at the post's top row, 1 m up, u 0 and ends at the next column.
        let v = &mesh.vertices[..4];
        assert_eq!(v[0].position, [0.0, 1.0, 0.0]);
        assert_eq!(v[0].uv0, [0.0, 0.0]);
        assert!((v[1].uv0[0] - 14.0 / 8.0).abs() < 1e-6);
        assert!((v[2].position[1] - 0.926).abs() < 1e-6);
        // The posts stand 1.1 m.
        let rod = &mesh.vertices[2 * 8 * 4..2 * 8 * 4 + 2];
        assert!((rod[0].position[1] - 1.1).abs() < 1e-6 && rod[1].position[1] == 0.0);
        // The vertex count stays when the line is built (the renderer writes into a fixed buffer).
        let mut built = line.clone();
        built.built = true;
        assert_eq!(vertices(&built).len(), mesh.vertices.len());
    }

    #[test]
    fn nets_mesh() {
        let mesh = mesh(&line(2, 5));
        assert_eq!(mesh.vertices.len(), (2 * 8 * 4 + 3 * 4) * 4);
        let mesh = mesh_two_layers();
        assert_eq!(mesh.vertices.len(), (2 * 2 * 8 + 3 * 4) * 4);
    }

    fn mesh_two_layers() -> Mesh {
        mesh(&line(12, 14))
    }

    #[test]
    fn colours_blend() {
        assert_eq!(blend(0xff00_0000, 0xff00_00ff, 0.5), 0xff00_0080);
    }
}

//! How much sky each vertex of the interior set sees (renderer addition, ours: [`crate::renderer::Enhance::cabin`]).
//! The game lights the whole cabin with one ambient; here each interior vertex gets the share of its hemisphere that
//! reaches out of the car past its opaque faces, both sets (`i_*` and the exterior shell), the glass counting as open.
//! The cabin's ambient follows it (`cabin.wgsl`): footwells, the dash under its top and the seat folds go darker, faces
//! by the windows lighter. Measured once at load, at rest, so it costs nothing a frame.
//!
//! The car's opaque faces are put into a grid of [`VOXEL`] cells; [`RAYS`] cosine-weighted rays from each vertex step
//! through it for up to [`REACH`] metres.
//!
//! Each vertex also keeps which way it sees the sky ([`seen`]): the mean direction of its rays that get out. The cabin
//! takes the light of what lies that way outside (`cabin.wgsl`), so a face turned to a sunlit or a sunset window is lit
//! by it and one that sees out only toward the forest is darker; its sheen reflects only what lies within that cone.
//!
//! The vertices also say which they are, in the whole part of `specular[1]` (the car's draws have no specular map, so
//! the field is free): [`INTERIOR`] for the interior set's opaque faces (the cabin light touches nothing else of the
//! car, so not the glass the outside is seen through), [`GAUGE`] for its dial faces ([`mark_gauges`]) and [`MIRROR`]
//! for the glass of its mirrors.
//!
//! A mirror's glass ([`Mirror`]) is a `*mirrorglass*` part, or in a car without one the faces of its `*mirror*` parts
//! turned to the back of the car ([`FACING`]). The game has no mirror image: it draws that face with its texture, and
//! many RSF models leave it a flat texel of white or paint (the Impreza GC8 555's `i_mirrorglass_*` disc, the Lancer Evo
//! II's `i_mirror_*`). Here it mirrors the surroundings (`cabin_mirror` in `cabin.wgsl`), as the interior's opaque faces
//! of the glass class do (the BMW M3 E30's `i_body_tukor`).

use crate::damage::MeshDamage;
use crate::parallel::in_parallel;
use crate::stage::{Draw, Image, Mesh, Stage, StageTexture, TextureRef, TextureSet, Vertex};
use glam::{IVec3, Mat4, Vec2, Vec3};
use rbr_formats::dds;
use rbr_formats::rsf::gauges::{self, Atlas, Kind};
use std::collections::BTreeMap;
use std::path::Path;

/// `specular[1]` of a baked interior vertex.
pub const INTERIOR: f32 = 1.0;
/// `specular[1]` of an interior vertex on a dial face.
pub const GAUGE: f32 = 2.0;
/// `specular[1]` of an interior vertex on a mirror's glass.
pub const MIRROR: f32 = 3.0;
/// A mirror housing's face is its glass when its normal's share along the car's back (`+z`) is above this: the glass
/// turned toward the driver, not the housing's sides and front.
const FACING: f32 = 0.7;

/// A mirror's part, by its node name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirror {
    /// `*mirrorglass*`: all of it is glass.
    Glass,
    /// Any other `*mirror*` part: its glass is its faces turned to the back, unless the car has [`Mirror::Glass`] parts.
    Housing,
}

impl Mirror {
    pub fn of_part(name: &str) -> Option<Mirror> {
        let name = name.to_ascii_lowercase();
        match () {
            _ if name.contains("mirrorglass") => Some(Mirror::Glass),
            _ if name.contains("mirror") => Some(Mirror::Housing),
            _ => None,
        }
    }
}

/// The grid's cell, metres.
const VOXEL: f32 = 0.025;
/// Rays over each vertex's hemisphere.
const RAYS: usize = 64;
/// How far a ray looks, metres: past the cabin's width, so a ray that has not met the car by then is out of it.
const REACH: f32 = 2.5;
/// Where a ray starts, off its own face along the normal, metres (two cells: the face's own cells do not shade it).
const LIFT: f32 = 2.0 * VOXEL;

/// The car's opaque faces in cells.
struct Grid {
    low: Vec3,
    size: IVec3,
    solid: Vec<bool>,
}

impl Grid {
    /// The cells of `triangles`, or `None` without any.
    fn new(triangles: &[[Vec3; 3]]) -> Option<Grid> {
        let (low, high) = triangles.iter().flatten().fold(None, |bounds: Option<(Vec3, Vec3)>, &p| {
            Some(bounds.map_or((p, p), |(low, high)| (low.min(p), high.max(p))))
        })?;
        let low = low - Vec3::splat(2.0 * VOXEL);
        let size = ((high - low) / VOXEL).ceil().as_ivec3() + 2;
        let mut grid = Grid { low, size, solid: vec![false; (size.x * size.y * size.z) as usize] };
        for &[a, b, c] in triangles {
            grid.fill(a, b, c);
        }
        Some(grid)
    }

    fn index(&self, p: Vec3) -> Option<usize> {
        let cell = ((p - self.low) / VOXEL).floor().as_ivec3();
        (cell.cmpge(IVec3::ZERO).all() && cell.cmplt(self.size).all())
            .then(|| (cell.x + self.size.x * (cell.y + self.size.y * cell.z)) as usize)
    }

    /// Marks the cells of triangle `a b c`, sampled at half a cell.
    fn fill(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        let longest = (b - a).length().max((c - a).length()).max((c - b).length());
        let steps = (longest / (0.5 * VOXEL)).ceil().max(1.0) as u32;
        for i in 0..=steps {
            for j in 0..=steps - i {
                let p = a + (b - a) * (i as f32 / steps as f32) + (c - a) * (j as f32 / steps as f32);
                if let Some(k) = self.index(p) {
                    self.solid[k] = true;
                }
            }
        }
    }

    /// Whether a ray from `from` along `dir` leaves the car without meeting a face.
    fn open(&self, from: Vec3, dir: Vec3) -> bool {
        let step = 0.5 * VOXEL;
        let mut t = 0.0;
        while t < REACH {
            match self.index(from + dir * t) {
                Some(k) if self.solid[k] => return false,
                Some(_) => {}
                None => return true,
            }
            t += step;
        }
        true
    }

    /// The cosine-weighted share of the hemisphere over `normal` at `p` that is open, and where it lies: the mean of
    /// the [`RAYS`] directions, each that meets the car counted zero. The mean's length over the share says how
    /// gathered the open directions are: 2/3 for the whole hemisphere, toward 1 for a narrow window.
    fn sky(&self, p: Vec3, normal: Vec3) -> (f32, Vec3) {
        let n = normal.normalize_or_zero();
        if n == Vec3::ZERO {
            return (1.0, Vec3::ZERO);
        }
        let (t, b) = n.any_orthonormal_pair();
        let from = p + n * LIFT;
        let (mut open, mut seen) = (0, Vec3::ZERO);
        for k in 0..RAYS {
            // A spiral over the disc, lifted onto the hemisphere: cosine-weighted.
            let s = (k as f32 + 0.5) / RAYS as f32;
            let (r, angle) = (s.sqrt(), k as f32 * 2.399_963);
            let dir = t * (r * angle.cos()) + b * (r * angle.sin()) + n * (1.0 - s).sqrt();
            if self.open(from, dir) {
                open += 1;
                seen += dir;
            }
        }
        (open as f32 / RAYS as f32, seen / RAYS as f32)
    }
}

/// Whether `draw` writes depth and does not blend (the glass and the cut-out plastic let light in).
fn opaque(draw: &Draw) -> bool {
    draw.state.depth_write && !draw.state.blend
}

/// The normal and area of each triangle of `mesh`'s blended draws: the glass.
fn panes(mesh: &Mesh) -> impl Iterator<Item = (Vec3, f32)> + '_ {
    let at = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
    mesh.draws.iter().filter(|draw| draw.state.blend).flat_map(move |draw| {
        mesh.indices[draw.indices.clone()].chunks_exact(3).map(move |t| {
            let cross = (at(t[1]) - at(t[0])).cross(at(t[2]) - at(t[0]));
            (cross.normalize_or_zero(), 0.5 * cross.length())
        })
    })
}

/// The share of the light at cosine `cos` to a clear pane's normal through its two faces, `(1 - F)^2`, `F` Schlick's
/// with [`PANE_F0`] (`glass.wgsl`'s). Over the sky (the cosine-weighed hemisphere) it is 0.848, `cabin.wgsl`'s
/// `CABIN_SKY_THROUGH`.
fn pane_through(cos: f32) -> f32 {
    let f = PANE_F0 + (1.0 - PANE_F0) * (1.0 - cos.clamp(0.0, 1.0)).powi(5);
    (1.0 - f) * (1.0 - f)
}

const PANE_F0: f32 = 0.04;

/// The triangles of `mesh`'s [`opaque`] draws.
fn opaque_triangles(mesh: &Mesh) -> impl Iterator<Item = [Vec3; 3]> + '_ {
    let at = |i: u32| Vec3::from(mesh.vertices[i as usize].position);
    mesh.draws.iter().filter(|draw| opaque(draw)).flat_map(move |draw| {
        mesh.indices[draw.indices.clone()].chunks_exact(3).map(move |t| [at(t[0]), at(t[1]), at(t[2])])
    })
}

/// Stores in each vertex of `interior` how much it is shut off from the sky, `1 - sky`, past the opaque faces of both
/// `interior` and `exterior` (at the same placement), in `specular[2]`, and marks those of its [`opaque`] draws
/// [`INTERIOR`] (the glass the outside is seen through stays lit as in the game), and the glass of its mirror parts
/// (`damage`, per mesh) [`MIRROR`], with where each sees the sky ([`seen`]). Returns how light gets into the cabin
/// ([`SunIn`]), the interior's faces' colours read from `textures`.
pub fn bake(
    interior: &mut [Mesh],
    damage: &[MeshDamage],
    exterior: &[Mesh],
    textures: &BTreeMap<TextureRef, StageTexture>,
) -> Option<SunIn> {
    let triangles: Vec<[Vec3; 3]> = interior.iter().chain(exterior).flat_map(opaque_triangles).collect();
    let grid = Grid::new(&triangles)?;
    let points: Vec<(Vec3, Vec3)> = interior
        .iter()
        .flat_map(|m| m.vertices.iter().map(|v| (Vec3::from(v.position), Vec3::from(v.normal))))
        .collect();
    let skies = in_parallel(&points, |&(p, n)| grid.sky(p, n));
    let panes: Vec<(Vec3, f32)> = interior.iter().chain(exterior).flat_map(panes).collect();
    let sun_in = SunIn::measure(&grid, interior, &skies, &panes, textures);
    let mirrors: Vec<Option<Mirror>> = (0..interior.len()).map(|k| damage.get(k).and_then(|d| d.mirror)).collect();
    let glass_parts = mirrors.contains(&Some(Mirror::Glass));
    let mut skies = skies.into_iter();
    for (k, mesh) in interior.iter_mut().enumerate() {
        let Mesh { draws, indices, vertices, .. } = mesh;
        let seen: Vec<Vec3> = vertices
            .iter_mut()
            .zip(skies.by_ref())
            .map(|(vertex, (sky, seen))| {
                vertex.specular[2] = 1.0 - sky;
                seen
            })
            .collect();
        for draw in draws.iter().filter(|draw| opaque(draw)) {
            for &i in &indices[draw.indices.clone()] {
                let vertex = &mut vertices[i as usize];
                vertex.specular[1] = INTERIOR;
                *vertex = with_seen(*vertex, seen[i as usize]);
            }
        }
        let glass = |t: &&[u32]| match mirrors[k] {
            Some(Mirror::Glass) => true,
            Some(Mirror::Housing) if !glass_parts => {
                t.iter().map(|&i| Vec3::from(vertices[i as usize].normal)).sum::<Vec3>().normalize_or_zero().z > FACING
            }
            _ => false,
        };
        let glass: Vec<u32> = indices.chunks_exact(3).filter(glass).flatten().copied().collect();
        for i in glass {
            let vertex = &mut vertices[i as usize];
            vertex.specular[1] = MIRROR;
            *vertex = with_seen(*vertex, seen[i as usize]);
        }
    }
    Some(sun_in)
}

/// How light gets into the cabin (ours), means over the interior set's opaque faces by area, each weighed by how far it
/// is inside the cabin ([`inside`]): the sun on each by the sun's direction in the car's model space, `n.l` where the
/// sun reaches it past the car's opaque faces, measured at load for [`SUN_DIRECTIONS`] directions over the sphere from
/// [`SUN_FACES`] of the faces, with the share of the sun the glass lets through from each (its panes weighed by their
/// area toward the sun, [`pane_through`]; 1 with no glass that way, an open cockpit); the share of the sky each sees
/// and where it sees it ([`Grid::sky`]); and their linear albedo per channel, the texel at the face's middle times its
/// vertex colour as `shader.wgsl` shades it. The light that comes in lights the rest of the cabin back off its faces
/// (`cabin.wgsl`): the roof lining over a sunlit dash or seat, the footwells. The interior set holds the body seen
/// through the glass too, which takes in light the cabin does not: counted, it took the Clio Rally3's sky share from
/// 0.055 to 0.361, the bounce 1.5 to 2.7 stops too bright; the BMW M3 E30's green livery, half of its area, turned
/// the bounce teal.
#[derive(Debug, Clone, PartialEq)]
pub struct SunIn {
    directions: Vec<Vec3>,
    lit: Vec<f32>,
    through: Vec<f32>,
    /// The mean sky share, the mean of where the faces see it (model space) and the mean albedo.
    pub sky: f32,
    pub seen: Vec3,
    pub albedo: Vec3,
}

/// The directions [`SunIn`] measures.
const SUN_DIRECTIONS: usize = 128;
/// The most faces [`SunIn`] measures from (every so many of them, for the load's time).
const SUN_FACES: usize = 3000;
/// How sharply [`SunIn::at`] weighs the measured directions by their closeness.
const SUN_SHARPNESS: i32 = 32;

/// The sky share over which a face goes from inside the cabin to outside it (the body seen through the glass), the
/// range over which `cabin.wgsl` lights a face as the outside view does (`CABIN_OUT`).
const OUTSIDE: [f32; 2] = [0.6, 0.9];

/// How far a face that sees `sky` of the sky is inside the cabin, 1 to 0 over [`OUTSIDE`].
fn inside(sky: f32) -> f32 {
    let t = ((sky - OUTSIDE[0]) / (OUTSIDE[1] - OUTSIDE[0])).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The largest texel the albedo is read at: the texture's level at most this wide.
const ALBEDO_LEVEL: u32 = 64;
/// The most a face's albedo is taken as (`shader.wgsl`'s `ALBEDO_MOST`).
const ALBEDO_MOST: f32 = 0.9;

impl SunIn {
    /// The cabin's light bounced back off its faces against the light that comes in, per channel: the
    /// interreflection of a closed room of mean albedo `a`, `a / (1 - a)` (a tan cabin's light is warm).
    pub fn bounce(&self) -> Vec3 {
        self.albedo / (Vec3::ONE - self.albedo).max(Vec3::splat(0.1))
    }

    /// The faces' mean sky share, where they see it and their albedo ([`SunIn`]), `skies` per vertex of `interior` in
    /// order ([`Grid::sky`]).
    fn incoming(
        interior: &[Mesh],
        skies: &[(f32, Vec3)],
        textures: &BTreeMap<TextureRef, StageTexture>,
    ) -> (f32, Vec3, Vec3) {
        let mut decoded: BTreeMap<TextureRef, Option<(usize, usize, Vec<u8>)>> = BTreeMap::new();
        let mut texel = |key: &TextureRef, uv: Vec2| -> Option<Vec3> {
            let entry =
                decoded.entry(*key).or_insert_with(|| rgba_at_most(textures.get(key)?.image.as_ref()?, ALBEDO_LEVEL));
            let (w, h, rgba) = entry.as_ref()?;
            let x = ((uv.x.rem_euclid(1.0) * *w as f32) as usize).min(w - 1);
            let y = ((uv.y.rem_euclid(1.0) * *h as f32) as usize).min(h - 1);
            let at = (y * w + x) * 4;
            Some(Vec3::new(rgba[at] as f32, rgba[at + 1] as f32, rgba[at + 2] as f32) / 255.0)
        };
        let (mut area, mut sky, mut seen, mut albedo) = (0.0, 0.0, Vec3::ZERO, Vec3::ZERO);
        let mut first = 0;
        for mesh in interior {
            for draw in mesh.draws.iter().filter(|draw| opaque(draw)) {
                for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                    let t = [t[0], t[1], t[2]];
                    let v = t.map(|i| &mesh.vertices[i as usize]);
                    let [a, b, c] = v.map(|v| Vec3::from(v.position));
                    let size = 0.5 * (b - a).cross(c - a).length();
                    let middle = v.iter().map(|v| Vec2::from(v.uv0)).sum::<Vec2>() / 3.0;
                    let tint = v
                        .iter()
                        .map(|v| Vec3::new(v.color[0] as f32, v.color[1] as f32, v.color[2] as f32))
                        .sum::<Vec3>()
                        / (3.0 * 255.0);
                    let colour =
                        draw.state.textures[0].as_ref().and_then(|key| texel(key, middle)).unwrap_or(Vec3::ONE) * tint;
                    let linear = colour.powf(2.2).min(Vec3::splat(ALBEDO_MOST));
                    let [sa, sb, sc] = t.map(|i| skies[first + i as usize]);
                    let share = (sa.0 + sb.0 + sc.0) / 3.0;
                    let size = size * inside(share);
                    area += size;
                    sky += size * share;
                    seen += size * (sa.1 + sb.1 + sc.1) / 3.0;
                    albedo += size * linear;
                }
            }
            first += mesh.vertices.len();
        }
        if area <= 0.0 {
            return (0.0, Vec3::ZERO, Vec3::ZERO);
        }
        (sky / area, seen / area, albedo / area)
    }

    fn measure(
        grid: &Grid,
        interior: &[Mesh],
        skies: &[(f32, Vec3)],
        panes: &[(Vec3, f32)],
        textures: &BTreeMap<TextureRef, StageTexture>,
    ) -> SunIn {
        let (sky, seen, albedo) = SunIn::incoming(interior, skies, textures);
        // The faces inside the cabin by area ([`inside`]): the body seen through the glass takes in light the cabin does
        // not.
        let mut faces: Vec<(Vec3, Vec3, f32)> = Vec::new();
        let mut first = 0;
        for mesh in interior {
            let vertex = |i: u32| &mesh.vertices[i as usize];
            for draw in mesh.draws.iter().filter(|draw| opaque(draw)) {
                for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                    let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(vertex(i).position));
                    let normal = t.iter().map(|&i| Vec3::from(vertex(i).normal)).sum::<Vec3>().normalize_or_zero();
                    let share = t.iter().map(|&i| skies[first + i as usize].0).sum::<f32>() / 3.0;
                    faces.push(((a + b + c) / 3.0, normal, inside(share) * 0.5 * (b - a).cross(c - a).length()));
                }
            }
            first += mesh.vertices.len();
        }
        let faces: Vec<_> = faces.iter().step_by(faces.len().div_ceil(SUN_FACES).max(1)).copied().collect();
        let area: f32 = faces.iter().map(|f| f.2).sum();
        let directions: Vec<Vec3> = (0..SUN_DIRECTIONS)
            .map(|k| {
                // A Fibonacci spiral over the sphere.
                let y = 1.0 - 2.0 * (k as f32 + 0.5) / SUN_DIRECTIONS as f32;
                let (r, angle) = ((1.0 - y * y).sqrt(), k as f32 * 2.399_963);
                Vec3::new(r * angle.cos(), y, r * angle.sin())
            })
            .collect();
        let lit = in_parallel(&directions, |&l| {
            let sum: f32 = faces
                .iter()
                .filter(|&&(_, n, _)| n.dot(l) > 0.0)
                .filter(|&&(p, n, _)| grid.open(p + n * LIFT, l))
                .map(|&(_, n, a)| a * n.dot(l))
                .sum();
            if area > 0.0 {
                sum / area
            } else {
                0.0
            }
        });
        let through = directions
            .iter()
            .map(|&l| {
                let (sum, weight) = panes.iter().fold((0.0, 0.0), |(sum, weight), &(n, a)| {
                    let cos = n.dot(l).abs();
                    (sum + a * cos * pane_through(cos), weight + a * cos)
                });
                if weight > 0.0 {
                    sum / weight
                } else {
                    1.0
                }
            })
            .collect();
        SunIn { directions, lit, through, sky, seen, albedo }
    }

    /// The cabin's mean sun with the sun toward `to_sun` (model space): the measured directions near it, weighed.
    pub fn at(&self, to_sun: Vec3) -> f32 {
        self.weighed(&self.lit, to_sun)
    }

    /// The share of the sun toward `to_sun` (model space) the glass lets in.
    pub fn through(&self, to_sun: Vec3) -> f32 {
        self.weighed(&self.through, to_sun)
    }

    /// `values` per measured direction at `to_sun`: the directions near it, weighed.
    fn weighed(&self, values: &[f32], to_sun: Vec3) -> f32 {
        let to_sun = to_sun.normalize_or_zero();
        let (sum, weight) = self.directions.iter().zip(values).fold((0.0, 0.0), |(sum, weight), (d, &value)| {
            let w = d.dot(to_sun).max(0.0).powi(SUN_SHARPNESS);
            (sum + w * value, weight + w)
        });
        if weight > 0.0 {
            sum / weight
        } else {
            0.0
        }
    }
}

/// A texture's level at most `largest` wide as RGBA rows, its width and height.
pub(crate) fn rgba_at_most(image: &Image, largest: u32) -> Option<(usize, usize, Vec<u8>)> {
    match image {
        Image::Solid(texel) => Some((1, 1, texel.to_vec())),
        Image::Dds(bytes) => {
            let file = rbr_formats::dds::parse(bytes).ok()?;
            let levels = file.levels();
            let level = levels.iter().find(|l| l.width.max(l.height) <= largest).or(levels.last())?;
            let rgba = crate::particles::gpu::decode_blocks(level.width, level.height, file.format, level.data)?;
            Some((level.width as usize, level.height as usize, rgba))
        }
        _ => None,
    }
}

/// Where a baked interior vertex ([`INTERIOR`] or [`GAUGE`]) sees the sky ([`bake`]), in the car's model space: x and
/// y in `uv2` (the car's draws have no light map), z in the fraction of `specular[1]` ([`SEEN_Z`]).
pub fn seen(vertex: &Vertex) -> Vec3 {
    let z = (vertex.specular[1].fract() - SEEN_Z[0]) / SEEN_Z[1];
    Vec3::new(vertex.uv2[0], vertex.uv2[1], z)
}

/// `vertex`, [`INTERIOR`] or [`GAUGE`], with [`seen`] `dir`.
pub fn with_seen(vertex: Vertex, dir: Vec3) -> Vertex {
    let mut specular = vertex.specular;
    specular[1] = specular[1].floor() + SEEN_Z[0] + SEEN_Z[1] * dir.z.clamp(-1.0, 1.0);
    Vertex { uv2: [dir.x, dir.y], specular, ..vertex }
}

/// `vertex` placed by `matrix` as the playback places the car's parts: where it sees the sky turns with it (the
/// position and the normal are the caller's).
pub fn turned(vertex: &Vertex, matrix: Mat4) -> Vertex {
    if vertex.specular[1] < INTERIOR {
        return *vertex;
    }
    with_seen(*vertex, matrix.transform_vector3(seen(vertex)))
}

/// The fraction of `specular[1]` that holds [`seen`]'s z: `SEEN_Z[0] + SEEN_Z[1] * z`, inside (0, 1) so that the whole
/// part stays the vertex's mark (`cabin.wgsl` reads both).
const SEEN_Z: [f32; 2] = [0.5, 0.45];

/// A dial face: a circle in a texture's uv, its centre and its radius along u and v.
pub type Face = (Vec2, Vec2);

/// The dial faces of the RBRHUD gauges of the car whose folder is `car_folder` (the ones [`crate::dials`] draws), under
/// the RSF install `root`, by the texture of `stage` each is on. Ours: the faces are not in the data, the needles are,
/// so a face is the circle about a needle's pivot as far as the needle reaches, and a quarter more ([`FACE`]).
pub fn gauge_faces(root: &Path, car_folder: &str, stage: &Stage) -> BTreeMap<TextureRef, Vec<Face>> {
    let mut faces: BTreeMap<TextureRef, Vec<Face>> = BTreeMap::new();
    let Some((folder, configs)) = gauges::variant(root, car_folder) else { return faces };
    for config in configs {
        let Some(atlas) = rbr_formats::cars::resolve(&folder, &config.atlas)
            .and_then(|path| std::fs::read(path).ok())
            .map(|bytes| Atlas::parse(&String::from_utf8_lossy(&bytes)))
        else {
            continue;
        };
        let needles: Vec<(Vec2, f32)> = config
            .elements
            .iter()
            .filter_map(|element| {
                let Kind::Rotating(rotating) = &element.kind else { return None };
                let [_, _, w, h] = atlas.sprite(element.texture.as_deref()?)?;
                let pivot = Vec2::from(rotating.pivot);
                let reach = [(0.0, 0.0), (w as f32, 0.0), (0.0, h as f32), (w as f32, h as f32)]
                    .map(|corner| (Vec2::from(corner) - pivot).length())
                    .into_iter()
                    .fold(0.0, f32::max);
                Some((Vec2::new(element.x, element.y), reach * element.scale * FACE))
            })
            .collect();
        for (&key, texture) in &stage.textures {
            let file = texture.request.rsplit(['\\', '/']).next().unwrap_or_default();
            let Some(Image::Dial { base: bytes, .. }) = &texture.image else { continue };
            let Ok(dds) = dds::parse(bytes) else { continue };
            if key.set == TextureSet::Car && file.eq_ignore_ascii_case(&config.texture) {
                let size = Vec2::new(dds.width as f32, dds.height as f32);
                faces.entry(key).or_default().extend(needles.iter().map(|&(at, reach)| (at / size, reach / size)));
            }
        }
    }
    faces
}

/// A dial face's radius against its needle's reach.
const FACE: f32 = 1.25;

/// Marks [`GAUGE`] the baked vertices of `interior`'s triangles whose middle is on one of `faces` (in the uv of their
/// draw's first texture). The cabin keeps their light at least at the game's (`cabin.wgsl`): under the dash's hood they
/// would read darker than in the game, and until the exposure follows the cabin the eye cannot adapt to them. True when
/// any changed.
pub fn mark_gauges(mesh: &mut Mesh, faces: &BTreeMap<TextureRef, Vec<Face>>) -> bool {
    let Mesh { draws, indices, vertices, .. } = mesh;
    let mut marked = false;
    for draw in draws.iter() {
        let Some(faces) = draw.state.textures[0].and_then(|t| faces.get(&t)) else { continue };
        for triangle in indices[draw.indices.clone()].chunks_exact(3) {
            let middle = triangle.iter().map(|&i| Vec2::from(vertices[i as usize].uv0)).sum::<Vec2>() / 3.0;
            if faces.iter().any(|&(centre, radius)| ((middle - centre) / radius).length() <= 1.0) {
                for &i in triangle {
                    let vertex = &mut vertices[i as usize];
                    if vertex.specular[1].floor() == INTERIOR {
                        vertex.specular[1] += GAUGE - INTERIOR;
                        marked = true;
                    }
                }
            }
        }
    }
    marked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::{Address, Anchor, DrawState, Layer, Lod};
    use bytemuck::Zeroable;

    /// One opaque draw of `triangles`, each vertex with `normal`.
    fn opaque_mesh(triangles: &[[Vec3; 3]], normal: Vec3) -> Mesh {
        let vertices: Vec<Vertex> = triangles
            .iter()
            .flatten()
            .map(|&p| Vertex { position: p.into(), normal: normal.into(), ..Vertex::zeroed() })
            .collect();
        Mesh {
            layer: Layer::Solid,
            anchor: Anchor::World,
            bounds: None,
            cell: None,
            indices: (0..vertices.len() as u32).collect(),
            draws: vec![Draw {
                indices: 0..vertices.len(),
                state: DrawState {
                    textures: [None, None],
                    shadow: None,
                    address: [Address::Wrap; 2],
                    second_texture: false,
                    alpha_ref: None,
                    mip_bias: false,
                    cull_back: false,
                    blend: false,
                    depth_test: true,
                    depth_write: true,
                    shade: None,
                    specular: None,
                },
                lod: Lod::Always,
            }],
            vertices,
        }
    }

    /// The two triangles of the square `a b c d`.
    fn square(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> [[Vec3; 3]; 2] {
        [[a, b, c], [a, c, d]]
    }

    #[test]
    fn a_floor_sees_the_sky_and_a_roof_takes_it() {
        let floor = square(
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
        );
        let grid = Grid::new(&floor).unwrap();
        assert_eq!(grid.sky(Vec3::ZERO, Vec3::Y).0, 1.0);
        // A roof 0.5 m up over a 2 m square: only the rays low toward the sides get out.
        let roof = square(
            Vec3::new(-1.0, 0.5, -1.0),
            Vec3::new(1.0, 0.5, -1.0),
            Vec3::new(1.0, 0.5, 1.0),
            Vec3::new(-1.0, 0.5, 1.0),
        );
        let grid = Grid::new(&[floor, roof].concat()).unwrap();
        let under = grid.sky(Vec3::ZERO, Vec3::Y).0;
        assert!(under > 0.05 && under < 0.4, "{under}");
        // Facing a wall a hand away from the edge, it sees out the open side.
        assert!(grid.sky(Vec3::new(0.95, 0.25, 0.0), Vec3::X).0 > 0.8);
    }

    #[test]
    fn a_face_by_an_open_side_sees_the_sky_that_way() {
        // Over a floor, under a roof 0.5 m up that reaches past x = -1 but stops at x = 1: the sky is seen toward +x.
        let floor = square(
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
        );
        let roof = square(
            Vec3::new(-3.0, 0.5, -3.0),
            Vec3::new(1.0, 0.5, -3.0),
            Vec3::new(1.0, 0.5, 3.0),
            Vec3::new(-3.0, 0.5, 3.0),
        );
        let grid = Grid::new(&[floor, roof].concat()).unwrap();
        let (share, seen) = grid.sky(Vec3::new(0.5, 0.0, 0.0), Vec3::Y);
        assert!(share > 0.1 && share < 0.6, "{share}");
        assert!(seen.x > 0.5 * seen.length(), "{seen}");
        // Gathered: the mean's length over the share is past the whole hemisphere's 2/3.
        assert!(seen.length() / share > 0.7, "{seen} {share}");
        // Below the floor, facing down: all open, the mean at the whole hemisphere's.
        let (open, wide) = grid.sky(Vec3::new(0.0, -0.1, 0.0), Vec3::NEG_Y);
        assert_eq!(open, 1.0);
        assert!((wide.length() - 2.0 / 3.0).abs() < 0.02, "{wide}");
    }

    #[test]
    fn where_a_vertex_sees_the_sky_keeps_its_mark_and_turns_with_the_car() {
        let vertex = Vertex { specular: [0.0, GAUGE, 0.25], ..Vertex::zeroed() };
        let dir = Vec3::new(0.3, -0.2, 0.6);
        let seen_at = with_seen(vertex, dir);
        assert_eq!(seen_at.specular[1].floor(), GAUGE);
        assert_eq!(seen_at.specular[2], 0.25);
        assert!((seen(&seen_at) - dir).length() < 1e-5);
        let turn = Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2) * Mat4::from_translation(Vec3::X);
        assert!((seen(&turned(&seen_at, turn)) - turn.transform_vector3(dir)).length() < 1e-5);
        // Not the interior's: left as it is.
        let glass = Vertex { uv2: [0.5, 0.5], ..Vertex::zeroed() };
        assert_eq!(turned(&glass, turn), glass);
    }

    #[test]
    fn a_pane_passes_less_at_a_slant() {
        assert!((pane_through(1.0) - 0.96 * 0.96).abs() < 1e-6);
        assert!(pane_through(0.2) < 0.75 && pane_through(0.0) == 0.0);
        // Over the cosine-weighed hemisphere: `cabin.wgsl`'s `CABIN_SKY_THROUGH`.
        let steps = 10_000;
        let sky: f32 = (0..steps).map(|k| (k as f32 + 0.5) / steps as f32).map(|c| pane_through(c) * 2.0 * c).sum();
        assert!((sky / steps as f32 - 0.848).abs() < 1e-3, "{}", sky / steps as f32);
    }

    #[test]
    fn the_sun_gets_in_under_an_open_side() {
        // A floor under a roof open toward +x, as the interior's one opaque face.
        let floor = square(
            Vec3::new(-1.0, 0.0, -1.0),
            Vec3::new(-1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, -1.0),
        );
        let roof = square(
            Vec3::new(-3.0, 0.5, -3.0),
            Vec3::new(1.0, 0.5, -3.0),
            Vec3::new(1.0, 0.5, 3.0),
            Vec3::new(-3.0, 0.5, 3.0),
        );
        let grid = Grid::new(&[floor, roof].concat()).unwrap();
        let mut mesh = opaque_mesh(&floor, Vec3::Y);
        for v in &mut mesh.vertices {
            v.color = [128, 128, 128, 255];
        }
        let skies: Vec<(f32, Vec3)> =
            mesh.vertices.iter().map(|v| grid.sky(Vec3::from(v.position), Vec3::from(v.normal))).collect();
        let sun_in = SunIn::measure(&grid, &[mesh], &skies, &[], &BTreeMap::new());
        // Seen out the open side only, and grey 128 untextured: (128 / 255)^2.2 linear.
        assert!(sun_in.sky > 0.05 && sun_in.sky < 0.6, "{}", sun_in.sky);
        assert!(sun_in.seen.x > 0.0, "{}", sun_in.seen);
        assert!(
            (sun_in.albedo - Vec3::splat((128.0f32 / 255.0).powf(2.2))).abs().max_element() < 1e-3,
            "{}",
            sun_in.albedo
        );
        let low_side = sun_in.at(Vec3::new(1.0, 0.4, 0.0));
        assert!(low_side > 0.05, "{low_side}");
        for shut in [Vec3::Y, Vec3::new(-1.0, 0.4, 0.0), Vec3::NEG_Y] {
            assert!(sun_in.at(shut) < 0.1 * low_side, "{shut}: {}", sun_in.at(shut));
        }
    }

    #[test]
    fn mirror_parts_go_by_their_names() {
        assert_eq!(Mirror::of_part("i_mirrorglass_l"), Some(Mirror::Glass));
        assert_eq!(Mirror::of_part("i_mirror2_r"), Some(Mirror::Housing));
        assert_eq!(Mirror::of_part("i_RoomMirror"), Some(Mirror::Housing));
        assert_eq!(Mirror::of_part("i_body_tukor"), None, "its glass is of the glass class");
    }

    #[test]
    fn a_closed_box_sees_no_sky() {
        let (l, h) = (Vec3::splat(-0.5), Vec3::splat(0.5));
        let corner = |x: bool, y: bool, z: bool| {
            Vec3::new([l.x, h.x][x as usize], [l.y, h.y][y as usize], [l.z, h.z][z as usize])
        };
        let faces = [
            square(
                corner(false, false, false),
                corner(true, false, false),
                corner(true, false, true),
                corner(false, false, true),
            ),
            square(
                corner(false, true, false),
                corner(true, true, false),
                corner(true, true, true),
                corner(false, true, true),
            ),
            square(
                corner(false, false, false),
                corner(true, false, false),
                corner(true, true, false),
                corner(false, true, false),
            ),
            square(
                corner(false, false, true),
                corner(true, false, true),
                corner(true, true, true),
                corner(false, true, true),
            ),
            square(
                corner(false, false, false),
                corner(false, true, false),
                corner(false, true, true),
                corner(false, false, true),
            ),
            square(
                corner(true, false, false),
                corner(true, true, false),
                corner(true, true, true),
                corner(true, false, true),
            ),
        ];
        let grid = Grid::new(&faces.concat()).unwrap();
        assert_eq!(grid.sky(Vec3::new(0.0, -0.5, 0.0), Vec3::Y), (0.0, Vec3::ZERO));
    }
}

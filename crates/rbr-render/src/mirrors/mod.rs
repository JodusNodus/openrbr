//! The mirrors seen from the cockpit (renderer addition, ours: the game draws their glass with its texture). Each
//! mirror's glass shows the scene drawn from the eye reflected across the glass's plane (`renderer/mirrors.rs`,
//! `mirrors.wgsl`): the stage behind the car, the car's own flank in the side mirrors and the cabin in the centre one.
//!
//! The glass is found in the interior set at load ([`find`]): the faces of its mirror parts ([`Mirror`]: all of a
//! `*mirrorglass*` part, the back-facing faces of the other `*mirror*` parts when the car has none) and its opaque
//! faces of the glass class (the retail cars' mirrors, the BMW M3 E30's `i_body_tukor`). Connected faces make one
//! mirror when they lie on one plane ([`FLAT`]) facing the driver (toward the car's back); the rest keep the cube's
//! reflection.

use crate::car::cabin::Mirror;
use crate::damage::MeshDamage;
use crate::stage::{Draw, Mesh, Stage};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::collections::HashMap;

/// How the mirrors are updated (`viewer --mirrors`, the launcher's "Mirrors" setting).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rate {
    /// No mirror passes: the glass as without this addition.
    #[default]
    Off,
    /// Each mirror is drawn every this many frames, the mirrors taking turns (1 to 3).
    Every(u32),
}

impl Rate {
    /// The rates offered, in the launcher's order.
    pub const ALL: [Rate; 4] = [Rate::Off, Rate::Every(1), Rate::Every(2), Rate::Every(3)];

    /// `off`, `1`, `2` or `3`.
    pub fn parse(text: &str) -> Option<Rate> {
        match text {
            "off" => Some(Rate::Off),
            "1" | "2" | "3" => text.parse().ok().map(Rate::Every),
            _ => None,
        }
    }

    /// Its [`Rate::parse`] name.
    pub fn name(self) -> String {
        match self {
            Rate::Off => "off".to_owned(),
            Rate::Every(n) => n.to_string(),
        }
    }

    /// Whether mirror `index` is drawn on frame `frame`.
    pub fn due(self, index: usize, frame: u64) -> bool {
        match self {
            Rate::Off => false,
            Rate::Every(n) => (frame + index as u64).is_multiple_of(u64::from(n.max(1))),
        }
    }
}

/// Which mirror a glass is, by where it sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The rear-view mirror over the windscreen: it sees the cabin and the rear window.
    Centre,
    /// A door mirror: it sees the car's flank.
    Side,
}

/// A mirror's glass in the interior set, in model space.
#[derive(Debug, Clone, PartialEq)]
pub struct Glass {
    /// Its faces: the interior mesh and the triangle there (the index of its first index in `indices`).
    pub faces: Vec<(usize, usize)>,
    pub kind: Kind,
    /// The plane: a point on it (the glass's centre) and its normal, toward the driver.
    pub centre: Vec3,
    pub normal: Vec3,
    /// Two axes in the plane and the glass's half size along them: its outline's box.
    pub axes: [Vec3; 2],
    pub half: Vec2,
}

impl Glass {
    fn area(&self) -> f32 {
        4.0 * self.half.x * self.half.y
    }

    /// The outline's four corners, in model space.
    pub fn corners(&self) -> [Vec3; 4] {
        let [u, v] = [self.axes[0] * self.half.x, self.axes[1] * self.half.y];
        [self.centre - u - v, self.centre + u - v, self.centre + u + v, self.centre - u + v]
    }
}

/// The glass a face set made of no mirror, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Unfitted {
    pub mesh: usize,
    pub triangles: usize,
    /// The farthest a vertex lies off the faces' mean plane, metres.
    pub off_plane: f32,
}

/// What [`find`] found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    pub mirrors: Vec<Glass>,
    pub unfitted: Vec<Unfitted>,
}

/// The farthest a vertex of a mirror's glass may lie off its plane: its own size times this, at least
/// [`FLAT_LEAST`] (ours: a slightly domed door mirror still reads right reflected in its mean plane).
const FLAT: f32 = 0.04;
const FLAT_LEAST: f32 = 0.003;
/// Neighbouring faces are one pane when their normals are within this cosine and the plane holds them within
/// [`FLAT_LEAST`].
const SAME_PLANE: f32 = 0.985;
/// Glass narrower than this is not a mirror (a bevel, a rim), metres.
const SMALLEST: f32 = 0.03;
/// The eye's line reflected off a mirror's glass runs back along the car (`+z`) at least this much: the glass shows
/// what lies behind, not the roof or the dash.
const BACKWARD: f32 = 0.5;
/// Where a mirror's glass sits from the cockpit's eye (ours): ahead of it by these metres, at most [`BELOW`] under it,
/// and at most [`LARGEST`] across.
const AHEAD: [f32; 2] = [0.2, 1.5];
const BELOW: f32 = 0.45;
const LARGEST: f32 = 0.45;
/// A centre mirror's glass lies within this of the car's middle (`x` 0), metres.
const CENTRE_REACH: f32 = 0.25;
/// Where the centre mirror is looked for when the car names none (ours): within [`CENTRE_REACH`], ahead of the eye
/// by these metres, at most [`CENTRE_BELOW`] under it, and its pane this wide and at least [`CENTRE_ASPECT`] times as
/// wide as high.
const CENTRE_AHEAD: [f32; 2] = [0.2, 1.0];
const CENTRE_BELOW: f32 = 0.15;
const CENTRE_WIDTH: [f32; 2] = [0.1, 0.4];
const CENTRE_ASPECT: f32 = 1.8;
/// Panes whose centres are within this are one mirror's (a glass part and its housing, two copies), metres.
const ONE_MIRROR: f32 = 0.2;
/// Vertices this close are one, joining faces, metres.
const WELD: f32 = 0.001;
/// The points over a glass [`seen`] tries, across and up.
const SEEN_AT: [u32; 2] = [5, 3];

/// The mirrors of the interior set `interior` (in model space) with its parts' `damage` (their [`Mirror`] names), seen
/// from the cockpit's `eye`: the panes of the mirror parts and the opaque glass, else, for the centre mirror, the
/// largest pane of any opaque face where one hangs.
pub fn find(interior: &[Mesh], damage: &[MeshDamage], eye: Vec3) -> Found {
    let glass_parts = damage.iter().any(|d| d.mirror == Some(Mirror::Glass));
    let mut found = Found::default();
    let mut panes: Vec<Glass> = Vec::new();
    let mut hanging: Vec<Glass> = Vec::new();
    for (k, mesh) in interior.iter().enumerate() {
        let mirror = damage.get(k).and_then(|d| d.mirror);
        let named = match mirror {
            Some(Mirror::Glass) => true,
            Some(Mirror::Housing) => !glass_parts,
            None => false,
        };
        let triangles: Vec<(usize, bool)> = mesh
            .draws
            .iter()
            .filter(|draw| named || opaque(draw))
            .flat_map(|draw| draw.indices.clone().step_by(3).map(move |t| (t, named || opaque_glass(draw))))
            .collect();
        for pane in planes(mesh, &triangles) {
            let mirror = pane.iter().all(|&(_, mirror)| mirror);
            let pane: Vec<usize> = pane.into_iter().map(|(t, _)| t).collect();
            match fit(mesh, &pane, eye) {
                Ok(Some(glass)) if mirror => panes.push(in_mesh(glass, k)),
                Ok(Some(glass)) if hangs(&glass, eye) => hanging.push(in_mesh(glass, k)),
                Err(off_plane) if mirror => found.unfitted.push(Unfitted { mesh: k, triangles: pane.len(), off_plane }),
                _ => {}
            }
        }
    }
    // The largest pane of each mirror the eye sees past the opaque faces (not the glass class's: the windows), with
    // the copies of it on its plane; one mirror a side and one in the middle.
    let walls: Vec<([Vec3; 3], bool)> = interior
        .iter()
        .flat_map(|mesh| {
            let draws = mesh.draws.iter().filter(|draw| opaque(draw) && !opaque_glass(draw));
            draws.flat_map(|draw| draw.indices.clone().step_by(3).map(|t| (corners(mesh, t), draw.state.cull_back)))
        })
        .collect();
    let place = |m: &Glass| (m.kind, m.centre.x > 0.0 || m.kind == Kind::Centre);
    let seen = |m: &Glass| seen(&walls, eye, m);
    panes.sort_by(|a, b| b.area().total_cmp(&a.area()));
    for pane in panes.into_iter().filter(seen) {
        let taken = found.mirrors.iter().any(|m| place(m) == place(&pane));
        match found.mirrors.iter_mut().find(|m| m.centre.distance(pane.centre) < ONE_MIRROR) {
            Some(mirror)
                if mirror.normal.dot(pane.normal) >= SAME_PLANE
                    && (pane.centre - mirror.centre).dot(mirror.normal).abs() <= FLAT_LEAST =>
            {
                mirror.faces.extend(pane.faces)
            }
            Some(_) => {}
            None if taken => {}
            None => found.mirrors.push(pane),
        }
    }
    if !found.mirrors.iter().any(|m| m.kind == Kind::Centre) {
        let largest = hanging.into_iter().filter(seen).max_by(|a, b| a.area().total_cmp(&b.area()));
        found.mirrors.extend(largest);
    }
    found
}

/// Whether the eye sees `glass` past the opaque faces `walls` (each with whether it culls its back, as drawn) at any of
/// [`SEEN_AT`] points over it (a cage's tube may hide some).
fn seen(walls: &[([Vec3; 3], bool)], eye: Vec3, glass: &Glass) -> bool {
    let [u, v] = [glass.axes[0] * glass.half.x, glass.axes[1] * glass.half.y];
    let points = (0..SEEN_AT[0] * SEEN_AT[1]).map(|k| {
        let at = |i: u32, n: u32| 0.9 * (2.0 * (i as f32 + 0.5) / n as f32 - 1.0);
        glass.centre + u * at(k % SEEN_AT[0], SEEN_AT[0]) + v * at(k / SEEN_AT[0], SEEN_AT[1])
    });
    points.into_iter().any(|p| {
        let (dir, distance) = ((p - eye).normalize_or_zero(), p.distance(eye));
        // The front face's normal `e1 x e2` points at the viewer (`coords::FRONT_FACE`).
        !walls.iter().any(|&(t, cull_back)| {
            (!cull_back || (t[1] - t[0]).cross(t[2] - t[0]).dot(dir) < 0.0)
                && crate::views::ray_hit(eye, dir, t).is_some_and(|d| d < distance - SMALLEST)
        })
    })
}

/// `glass` with its faces in interior mesh `mesh`.
fn in_mesh(glass: Glass, mesh: usize) -> Glass {
    Glass { faces: glass.faces.into_iter().map(|(_, t)| (mesh, t)).collect(), ..glass }
}

/// Whether `glass` is where and what a centre mirror is ([`CENTRE_AHEAD`]).
fn hangs(glass: &Glass, eye: Vec3) -> bool {
    let ahead = eye.z - glass.centre.z;
    let (wide, high) = (2.0 * glass.half.x, 2.0 * glass.half.y);
    glass.kind == Kind::Centre
        && (CENTRE_AHEAD[0]..=CENTRE_AHEAD[1]).contains(&ahead)
        && glass.centre.y > eye.y - CENTRE_BELOW
        && (CENTRE_WIDTH[0]..=CENTRE_WIDTH[1]).contains(&wide)
        && wide >= CENTRE_ASPECT * high
}

/// A draw that writes depth and does not blend.
fn opaque(draw: &Draw) -> bool {
    draw.state.depth_write && !draw.state.blend
}

/// An opaque draw of the glass class: the game lights it as glass, but nothing is seen through it.
fn opaque_glass(draw: &Draw) -> bool {
    draw.state.shade.is_some_and(|shade| shade.glass) && opaque(draw)
}

fn corners(mesh: &Mesh, t: usize) -> [Vec3; 3] {
    [0, 1, 2].map(|i| Vec3::from(mesh.vertices[mesh.indices[t + i] as usize].position))
}

/// Triangle `t`'s mean vertex normal.
fn facing(mesh: &Mesh, t: usize) -> Vec3 {
    (0..3).map(|i| Vec3::from(mesh.vertices[mesh.indices[t + i] as usize].normal)).sum::<Vec3>().normalize_or_zero()
}

/// Triangle `t`'s face normal, turned to the side its vertex normals give.
fn face_normal(mesh: &Mesh, t: usize) -> Vec3 {
    let [a, b, c] = corners(mesh, t);
    let n = (b - a).cross(c - a).normalize_or_zero();
    if n.dot(facing(mesh, t)) < 0.0 {
        -n
    } else {
        n
    }
}

/// `triangles` of `mesh` (each with a flag) in panes: sets joined by shared corners (welded by position, [`WELD`])
/// between faces on one plane ([`SAME_PLANE`]).
fn planes<T: Copy + Ord>(mesh: &Mesh, triangles: &[(usize, T)]) -> Vec<Vec<(usize, T)>> {
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..triangles.len()).collect();
    let normals: Vec<Vec3> = triangles.iter().map(|&(t, _)| face_normal(mesh, t)).collect();
    let mut at: HashMap<[i32; 3], Vec<usize>> = HashMap::new();
    for (n, &(t, _)) in triangles.iter().enumerate() {
        for p in corners(mesh, t) {
            at.entry((p / WELD).round().as_ivec3().to_array()).or_default().push(n);
        }
    }
    for sharing in at.values() {
        for (i, &a) in sharing.iter().enumerate() {
            for &b in &sharing[i + 1..] {
                let off = (corners(mesh, triangles[b].0)[0] - corners(mesh, triangles[a].0)[0]).dot(normals[a]);
                if normals[a].dot(normals[b]) >= SAME_PLANE && off.abs() <= FLAT_LEAST {
                    let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                    parent[ra] = rb;
                }
            }
        }
    }
    let mut sets: HashMap<usize, Vec<(usize, T)>> = HashMap::new();
    for (n, &triangle) in triangles.iter().enumerate() {
        sets.entry(root(&mut parent, n)).or_default().push(triangle);
    }
    let mut sets: Vec<Vec<(usize, T)>> = sets.into_values().collect();
    sets.sort();
    sets
}

/// The mirror `triangles` of `mesh` make, seen from `eye`: `Ok(None)` when they are no mirror (too small, turned
/// away, not reflecting what lies behind), `Err` with how far they lie off one plane when they are not flat
/// ([`FLAT`]).
fn fit(mesh: &Mesh, triangles: &[usize], eye: Vec3) -> Result<Option<Glass>, f32> {
    let (mut area_normal, mut centroid, mut area) = (Vec3::ZERO, Vec3::ZERO, 0.0);
    for &t in triangles {
        let [a, b, c] = corners(mesh, t);
        let cross = (b - a).cross(c - a);
        area_normal += face_normal(mesh, t) * cross.length();
        centroid += (a + b + c) / 3.0 * cross.length();
        area += cross.length();
    }
    if area <= 0.0 {
        return Ok(None);
    }
    let centroid = centroid / area;
    let mut normal = area_normal.normalize_or_zero();
    // Toward the driver.
    if normal.dot(eye - centroid) < 0.0 {
        normal = -normal;
    }
    let seen = (centroid - eye).normalize_or_zero();
    if (seen - 2.0 * seen.dot(normal) * normal).z < BACKWARD {
        return Ok(None);
    }
    let u = Vec3::Y.cross(normal).normalize_or(Vec3::X);
    let v = normal.cross(u);
    let points: Vec<Vec3> = triangles.iter().flat_map(|&t| corners(mesh, t)).collect();
    let (low, high) = points.iter().fold((Vec2::MAX, Vec2::MIN), |(low, high), &p| {
        let q = Vec2::new((p - centroid).dot(u), (p - centroid).dot(v));
        (low.min(q), high.max(q))
    });
    let size = high - low;
    let ahead = eye.z - centroid.z;
    if size.min_element() < SMALLEST
        || size.max_element() > LARGEST
        || !(AHEAD[0]..=AHEAD[1]).contains(&ahead)
        || centroid.y < eye.y - BELOW
    {
        return Ok(None);
    }
    let off_plane = points.iter().map(|&p| (p - centroid).dot(normal).abs()).fold(0.0, f32::max);
    if off_plane > (FLAT * size.max_element()).max(FLAT_LEAST) {
        return Err(off_plane);
    }
    let middle = (low + high) * 0.5;
    let centre = centroid + u * middle.x + v * middle.y;
    let kind = if centre.x.abs() < CENTRE_REACH { Kind::Centre } else { Kind::Side };
    Ok(Some(Glass {
        faces: triangles.iter().map(|&t| (0, t)).collect(),
        kind,
        centre,
        normal,
        axes: [u, v],
        half: size * 0.5,
    }))
}

/// The most mirrors a car's glass is drawn for (`MIRRORS` in `mirrors.wgsl`).
pub const MOST_MIRRORS: usize = 4;

/// Marks the faces of the first [`MOST_MIRRORS`] of `found` in `interior` for the mirror passes: each face's corners
/// get `specular[0]` 1 plus the mirror's number (`mirrors.wgsl`; the car's draws have no specular map). A corner shared with a face of no mirror is copied, with its dent in
/// `damage`, so that the faces around keep their look.
pub fn mark(interior: &mut [Mesh], damage: &mut [MeshDamage], found: &Found) {
    for (number, glass) in found.mirrors.iter().take(MOST_MIRRORS).enumerate() {
        let mark = 1.0 + number as f32;
        let mut copies: HashMap<(usize, u32), u32> = HashMap::new();
        for &(k, t) in &glass.faces {
            let (mesh, dents) = (&mut interior[k], damage.get_mut(k).and_then(|d| d.dents.as_mut()));
            let mut dents = dents.filter(|d| d.vertices.len() == mesh.vertices.len());
            for c in 0..3 {
                let i = mesh.indices[t + c];
                let copy = *copies.entry((k, i)).or_insert_with(|| {
                    let vertex = crate::stage::Vertex {
                        specular: [mark, mesh.vertices[i as usize].specular[1], mesh.vertices[i as usize].specular[2]],
                        ..mesh.vertices[i as usize]
                    };
                    mesh.vertices.push(vertex);
                    if let Some(dents) = dents.as_mut() {
                        dents.vertices.push(dents.vertices[i as usize]);
                    }
                    (mesh.vertices.len() - 1) as u32
                });
                mesh.indices[t + c] = copy;
            }
        }
    }
}

/// Finds the mirrors of `interior` seen from the cockpit's `eye` ([`find`]) and marks them ([`mark`]) both there and
/// in `stage`, which holds `interior`'s meshes. Returns the glass the marks number, and the faces left unfitted.
pub fn add(stage: &mut Stage, interior: &mut [Mesh], damage: &mut [MeshDamage], eye: Vec3) -> Found {
    let mut found = find(interior, damage, eye);
    found.mirrors.truncate(MOST_MIRRORS);
    let staged: Vec<Option<usize>> = interior.iter().map(|mesh| stage.meshes.iter().position(|m| m == mesh)).collect();
    mark(interior, damage, &found);
    for (mesh, staged) in interior.iter().zip(staged) {
        if let Some(i) = staged {
            stage.meshes[i] = mesh.clone();
        }
    }
    found
}

/// A mirror's glass this frame, in stage space: its outline's corners, its normal toward the eye, and whether it is a
/// door mirror (it sees the car's exterior; the centre mirror the cabin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub corners: [Vec3; 4],
    pub normal: Vec3,
    pub side: bool,
}

impl Glass {
    /// The glass placed by the car body's model space to stage space `body`.
    pub fn placed(&self, body: Mat4) -> Placed {
        Placed {
            corners: self.corners().map(|c| body.transform_point3(c)),
            normal: body.transform_vector3(self.normal).normalize_or_zero(),
            side: self.kind == Kind::Side,
        }
    }
}

/// The plane `normal . p = normal . point` as a row: `w` the negated distance.
fn plane(point: Vec3, normal: Vec3) -> Vec4 {
    normal.extend(-normal.dot(point))
}

/// The reflection across the plane through `point` with unit `normal`.
pub fn reflection(point: Vec3, normal: Vec3) -> Mat4 {
    let n = normal;
    let d = -n.dot(point);
    Mat4::from_cols(
        Vec4::new(1.0 - 2.0 * n.x * n.x, -2.0 * n.x * n.y, -2.0 * n.x * n.z, 0.0),
        Vec4::new(-2.0 * n.y * n.x, 1.0 - 2.0 * n.y * n.y, -2.0 * n.y * n.z, 0.0),
        Vec4::new(-2.0 * n.z * n.x, -2.0 * n.z * n.y, 1.0 - 2.0 * n.z * n.z, 0.0),
        Vec4::new(-2.0 * d * n.x, -2.0 * d * n.y, -2.0 * d * n.z, 1.0),
    )
}

/// A mirror's view this frame: what to draw it with and where it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Stage space to the mirror texture's clip space: the main view reflected across the glass, cropped to the
    /// glass's box on screen, flipped left to right (so that the reflection keeps the triangles' winding) and with
    /// its near plane on the glass.
    pub view_projection: Mat4,
    /// The same for the camera-anchored draws (the main view's turned only).
    pub sky_view_projection: Mat4,
    /// The glass's box on the main view's screen, in pixels: its texture's size.
    pub size: (u32, u32),
}

/// The share past the glass's plane the reflected view starts at, metres: the glass itself and what lies on it are
/// left out (ours).
const NEAR_PAST: f32 = 0.01;
/// The largest mirror image, pixels.
pub const MOST: (u32, u32) = (1024, 512);
/// Mirror textures go in steps of this, pixels, so that a moving head does not make new ones every frame.
const STEP: u32 = 32;

/// The view of the glass `placed`, from the main view `view_projection` (and its camera-anchored
/// `sky_view_projection`) drawn at `screen` pixels: `None` when the glass is behind the eye, turned away, or off
/// screen.
pub fn view(view_projection: Mat4, sky_view_projection: Mat4, screen: (u32, u32), placed: &Placed) -> Option<View> {
    let Placed { corners, normal, .. } = *placed;
    let eye = crate::camera::eye(view_projection);
    let centre = corners.iter().sum::<Vec3>() / 4.0;
    if normal.dot(eye - centre) <= 0.0 {
        return None;
    }
    // The glass's box on screen, in normalised device coordinates.
    let (mut low, mut high) = (Vec2::MAX, Vec2::MIN);
    for corner in corners {
        let clip = view_projection * corner.extend(1.0);
        if clip.w <= 1e-4 {
            return None;
        }
        let ndc = Vec2::new(clip.x, clip.y) / clip.w;
        (low, high) = (low.min(ndc), high.max(ndc));
    }
    let (low, high) = (low.max(Vec2::splat(-1.0)), high.min(Vec2::splat(1.0)));
    if low.cmpge(high).any() {
        return None;
    }
    // Its size in pixels, up to a step and at most [`MOST`], and the box grown to it (the image keeps the screen's
    // texel aspect).
    let pixels = (high - low) * 0.5 * Vec2::new(screen.0 as f32, screen.1 as f32);
    let scale = (MOST.0 as f32 / pixels.x).min(MOST.1 as f32 / pixels.y).min(1.0);
    let side = |p: f32, most: u32| ((p * scale / STEP as f32).ceil() as u32 * STEP).clamp(STEP, most);
    let size = (side(pixels.x, MOST.0), side(pixels.y, MOST.1));
    let half = Vec2::new(size.0 as f32, size.1 as f32) / scale / Vec2::new(screen.0 as f32, screen.1 as f32);
    let middle = (low + high) * 0.5;
    // Clip x and y of the box to -1..1, x mirrored: x' = -(x - m w) / h.
    let crop = Mat4::from_cols(
        Vec4::new(-1.0 / half.x, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0 / half.y, 0.0, 0.0),
        Vec4::ZERO.with_z(1.0),
        Vec4::new(middle.x / half.x, -middle.y / half.y, 0.0, 1.0),
    );
    let reflect = reflection(centre, normal);
    let mut matrix = crop * view_projection * reflect;
    // The near plane on the glass (reversed depth: clip z <= w): z = w - k (plane . p) in the reflected stage, where
    // the plane is positive past the glass. k at most the least view depth per metre over the box, so that the far
    // side (z >= 0) lies beyond everything the view draws.
    let past = plane(centre - normal * NEAR_PAST, -normal);
    // The least cosine to the view's axis over the box's corners, unprojected.
    let (inverse, forward) = (view_projection.inverse(), view_projection.row(3).truncate().normalize());
    let k = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|(x, y)| {
            let p = inverse * Vec4::new(middle.x + x * half.x, middle.y + y * half.y, 0.5, 1.0);
            forward.dot((p.truncate() / p.w - eye).normalize_or_zero())
        })
        .into_iter()
        .fold(1.0f32, f32::min)
        * 0.95;
    let w = matrix.row(3);
    // The plane past the glass in the stage before the reflection is the reflected plane: its row times the
    // reflection.
    let reflected = row_times(past, reflect);
    let z = w - reflected * k;
    matrix = with_row(matrix, 2, z);
    let turned = Mat4::from_mat3(glam::Mat3::from_mat4(reflect));
    let sky_view_projection = crop * sky_view_projection * turned;
    (k > 0.0).then_some(View { view_projection: matrix, sky_view_projection, size })
}

/// The row vector `row` times `m`.
fn row_times(row: Vec4, m: Mat4) -> Vec4 {
    Vec4::new(row.dot(m.x_axis), row.dot(m.y_axis), row.dot(m.z_axis), row.dot(m.w_axis))
}

/// `m` with row `i` replaced.
fn with_row(m: Mat4, i: usize, row: Vec4) -> Mat4 {
    let mut cols = m.to_cols_array_2d();
    for (c, col) in cols.iter_mut().enumerate() {
        col[i] = row[c];
    }
    Mat4::from_cols_array_2d(&cols)
}

#[cfg(test)]
mod tests;

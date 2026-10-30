//! The stage's plants, for the sun's shadows and the light through their leaves: renderer additions behind
//! [`crate::renderer::Enhance::lighting`], not the game's look. A plant is a connected piece of an cut-out draw's
//! triangles whose part of the texture is cut out ([`cut_out`]; rocks, fences and trunks are cut-out draws
//! too, with opaque textures, and the object atlases hold the stage's spectators, rocks and vans next to cut-out
//! texels). Grass (plants lower than [`GRASS`]) casts no sun shadow: its flat cards' shadows show how flat they are.
//! Cut-out structures (power pylons, wire and wooden fences, gates, lamps, signs) and spectator cards are not plants:
//! their textures are named for them ([`STRUCTURES`]).

use crate::stage::{Draw, Image, Mesh, Stage, TextureRef, Vertex};
use glam::{Vec2, Vec3, Vec3Swizzles};
use rbr_formats::dds::{self, Format};
use std::collections::{HashMap, HashSet};

/// Plants lower than this, metres, are grass. Plant heights on three stages dip between 1.5 and 2 m, between the
/// tufts and low plants and the bushes and trees.
const GRASS: f32 = 1.5;
/// A texture, or a piece's part of it, is cut out when more than this share of its texels is at or below the alpha
/// reference.
const CUT: f32 = 0.05;
/// The texture level tested is the first no wider than this.
const TEST_WIDTH: u32 = 256;
/// Words in the file names of cut-out textures that are not plants (upper case), over the retail stages and the RSF
/// samples' (`FENCE_TILE`, `POWERPYLON01_UPDATE`, `fieldfnc`, `panneausta1` and the like), and of the RSF stages'
/// spectator cards (`divaci`, `lidi2D`, `S-Spectators-3`, `people1`, `S-01_Person_Albedo`). Neither their colour nor
/// their cut-out share tells them apart: snowy firs are as grey as a pylon, wooden fences as brown as autumn leaves, and
/// a card of people is as cut out as a tree's.
const STRUCTURES: [&str; 19] = [
    "FENCE",
    "FNC",
    "PYLON",
    "POWERLINE",
    "POLE",
    "GATE",
    "WINDMILL",
    "LAMP",
    "BARRIER",
    "GUARDRAIL",
    "BRIDGE",
    "BANNER",
    "PANNEAU",
    "CROIX",
    "SPECTA",
    "DIVACI",
    "LIDI",
    "PEOPLE",
    "PERSON",
];

/// Whether the texture of `file_name` (a path or a name) shows a cut-out structure rather than a plant.
fn structure(file_name: &str) -> bool {
    let name = file_name.rsplit(['\\', '/']).next().unwrap_or(file_name).to_ascii_uppercase();
    let stem = name.split('.').next().unwrap_or(&name);
    stem == "NET" || STRUCTURES.iter().any(|word| stem.contains(word))
}

/// A texture's texels at the level tested ([`TEST_WIDTH`]): the cut-out ones above and left of each texel corner,
/// `width + 1` by `height + 1`, which count a rectangle's in four reads.
pub(crate) struct Cut {
    width: usize,
    height: usize,
    sums: Vec<u32>,
}

impl Cut {
    /// The texture `width` texels wide whose texels, row by row, are cut out where `texels` says.
    fn new(width: usize, texels: &[bool]) -> Cut {
        let height = texels.len() / width;
        let mut sums = vec![0; (width + 1) * (height + 1)];
        for y in 0..height {
            let mut row = 0;
            for x in 0..width {
                row += u32::from(texels[y * width + x]);
                sums[(y + 1) * (width + 1) + x + 1] = sums[y * (width + 1) + x + 1] + row;
            }
        }
        Cut { width, height, sums }
    }

    /// Whether the texels under the uv rectangle `low` to `high` are cut out (the texture repeats).
    fn under(&self, low: [f32; 2], high: [f32; 2]) -> bool {
        // Each axis's texels, at most the texture's size, as their one or two runs within one copy of it.
        let runs = |size: usize, k: usize| {
            let (from, to) = ((low[k] * size as f32).floor() as i64, (high[k] * size as f32).ceil() as i64);
            let length = to.clamp(from + 1, from + size as i64) - from;
            let start = from.rem_euclid(size as i64);
            let end = start + length;
            let size = size as i64;
            ([(start, end.min(size)), (0, (end - size).max(0))].map(|(a, b)| (a as usize, b as usize)), length)
        };
        let ((columns, width), (rows, height)) = (runs(self.width, 0), runs(self.height, 1));
        let at = |x: usize, y: usize| self.sums[y * (self.width + 1) + x];
        let cut: u32 = rows
            .iter()
            .flat_map(|&(y0, y1)| columns.iter().map(move |&(x0, x1)| (x0, x1, y0, y1)))
            .filter(|&(x0, x1, y0, y1)| x0 < x1 && y0 < y1)
            .map(|(x0, x1, y0, y1)| at(x1, y1) + at(x0, y0) - at(x0, y1) - at(x1, y0))
            .sum();
        cut as f32 > CUT * (width * height) as f32
    }
}

/// The textures of the draws `foliage` picks that are cut out at the draw's alpha reference, but for structures, with
/// their cut-out texels.
pub(crate) fn cut_out(stage: &Stage, foliage: impl Fn(&Draw) -> bool) -> HashMap<TextureRef, Cut> {
    let mut tested = HashSet::new();
    let mut cut = HashMap::new();
    for draw in stage.meshes.iter().flat_map(|mesh| &mesh.draws).filter(|draw| foliage(draw)) {
        let (Some(texture), Some(alpha_ref)) = (draw.state.textures[0], draw.state.alpha_ref) else { continue };
        if !tested.insert(texture) || stage.texture_names.get(&texture).is_some_and(|name| structure(&name.file_name)) {
            continue;
        }
        let Some((width, rgba)) = test_texels(stage, texture) else { continue };
        let texels: Vec<bool> = rgba.chunks_exact(4).map(|texel| texel[3] <= alpha_ref).collect();
        if texels.iter().filter(|&&below| below).count() as f32 > CUT * texels.len() as f32 {
            cut.insert(texture, Cut::new(width, &texels));
        }
    }
    cut
}

/// The width and RGBA texels of `texture`'s first level no wider than [`TEST_WIDTH`], when it is a stage `.dds`.
pub(crate) fn test_texels(stage: &Stage, texture: TextureRef) -> Option<(usize, Vec<u8>)> {
    let Some(Some(Image::Dds(bytes))) = stage.textures.get(&texture).map(|t| &t.image) else { return None };
    let dds = dds::parse(bytes).ok()?;
    let levels = dds.levels();
    let level = levels.iter().find(|level| level.width <= TEST_WIDTH).or(levels.last())?;
    let bc = match dds.format {
        Format::Dxt1 => texpresso::Format::Bc1,
        Format::Dxt3 => texpresso::Format::Bc2,
        Format::Dxt5 => texpresso::Format::Bc3,
        Format::Rgba8 => return Some((level.width as usize, level.data.to_vec())),
    };
    let (width, height) = (level.width.max(4) as usize, level.height.max(4) as usize);
    let mut rgba = vec![0; width * height * 4];
    bc.decompress(level.data, width, height, &mut rgba);
    Some((width, rgba))
}

/// The share of a plant's height, from its foot, at which its crown's centre sits ([`with_plants`]).
const CROWN_CENTRE: f32 = 0.55;
/// A plant piece higher than this, metres, or wider than [`CROWN_WIDEST`] times its height, is not a crown: a backdrop
/// or tree-line panel, wires, or a row of low bushes (RSF `scene_1a`, `V-TreeLine-02`, `sloupy`; the tallest trees are
/// about 36 m). It keeps the normal it had.
const CROWN_HIGHEST: f32 = 40.0;
/// See [`CROWN_HIGHEST`]: real crowns' crossed cards span about their height or less.
const CROWN_WIDEST: f32 = 1.5;
/// How much of the normal the vertices had (the ground's under them, `relight::Ground::on_foliage`) a crown's normal
/// keeps, against the direction out from its centre ([`with_plants`]).
const CROWN_UP: f32 = 0.5;

/// `vertices` (of `mesh`) with 1 in `specular[1]` on each grass vertex and 1 in `specular[2]` on each plant vertex (the
/// light through its leaves, `sun_light` in `shader.wgsl`), which plant draws do not use (they have no specular map).
/// `plant` gives the cut-out texture of the draws that may hold plants: a piece of them is one where its part is cut.
/// A tree ([`trees`]) taller than grass is a crown: its vertices' normals point out from the ellipsoid it spans about
/// its axis (centred [`CROWN_CENTRE`] up it), blended with the normal they had by [`CROWN_UP`], so that a tree has a
/// sunlit and a shaded side and its underside looks down (the cards' own normals, or the ground's, light the whole tree
/// alike). One ellipsoid for the whole tree: each wing's own would light the halves of a tree as two shapes, with a
/// seam down the axis. Panels are not crowns ([`CROWN_HIGHEST`]).
pub(crate) fn with_plants<'a>(
    mut vertices: Vec<Vertex>,
    mesh: &Mesh,
    plant: impl Fn(&Draw) -> Option<&'a Cut> + Copy,
) -> Vec<Vertex> {
    // The vertices of draws that blend a second texture by `uv1`.
    let mut second = vec![false; vertices.len()];
    for draw in mesh.draws.iter().filter(|draw| draw.state.second_texture && plant(draw).is_some()) {
        for &i in &mesh.indices[draw.indices.clone()] {
            if let Some(s) = second.get_mut(i as usize) {
                *s = true;
            }
        }
    }
    let trees = trees(mesh, plant);
    for ((v, on), second) in vertices.iter_mut().zip(&trees.of).zip(second) {
        let Some(on) = on else { continue };
        if on.plant {
            let grass = on.high.y - on.low.y < GRASS;
            v.specular[1] = f32::from(u8::from(grass));
            v.specular[2] = 1.0;
            // Where in the tuft, for `own_light` in `shader.wgsl`: `wet` the share of its height, `uv1` the way out of
            // its middle across (-1 to 1 over half its width), which plant draws without a second texture do not use.
            if grass && !second {
                let (centre, half) = ((on.low + on.high) * 0.5, ((on.high - on.low) * 0.5).max(Vec3::splat(0.02)));
                let p = Vec3::from(v.position);
                v.wet = ((p.y - on.low.y) / (half.y * 2.0)).clamp(0.0, 1.0);
                v.uv1 = ((p.xz() - centre.xz()) / half.xz()).clamp_length_max(1.0).to_array();
            }
        }
        let tree = &trees.trees[on.tree];
        if tree.plant && tree.crown() {
            v.specular[0] = TREE_CARD;
            let (low, high) = (tree.low, tree.high);
            let y = low.y + (high.y - low.y) * CROWN_CENTRE;
            let axis = tree.axis(y);
            let centre = Vec3::new(axis.x, y, axis.y);
            let radii = ((high - low) * 0.5).max(Vec3::splat(0.1));
            let out = ((Vec3::from(v.position) - centre) / radii).normalize_or(Vec3::Y);
            let before = Vec3::from(v.normal).normalize_or(Vec3::Y);
            v.normal = (out * (1.0 - CROWN_UP) + before * CROWN_UP).normalize_or(Vec3::Y).to_array();
            // Where in the ellipsoid, about the leaning axis (affine in the position too), for `crown_sun_seen` in
            // `shader.wgsl`: `uv1` and `wet`, which plant draws without a second texture do not use.
            if !second {
                let p = Vec3::from(v.position);
                let at = (p.xz() - tree.axis_unclamped(p.y)) / radii.xz();
                v.uv1 = at.to_array();
                v.wet = (p.y - y) / radii.y;
            }
        }
    }
    vertices
}

/// Whether a plant piece spanning `low` to `high` is a crown ([`with_plants`]).
fn crown(low: Vec3, high: Vec3) -> bool {
    let (height, width) = (high.y - low.y, (high.x - low.x).max(high.z - low.z));
    (GRASS..=CROWN_HIGHEST).contains(&height) && width <= CROWN_WIDEST * height
}

/// Plant pieces whose centres are this close across, metres, and whose heights meet, are one tree's cards (crossed
/// cards); pieces that share a vertex's place across are too (wings that meet on the axis).
const SAME_AXIS: f32 = 0.3;
/// `specular[0]` of a crown's cards, its strips that are no plant's included, which plant draws do not use otherwise
/// (they have no specular map; under 1, the mirrors' mark). `plant_faded` in `shader.wgsl` fades them edge-on,
/// `crown_sun_seen` shades them.
const TREE_CARD: f32 = 0.3;

/// The trees of a mesh's plant draws ([`trees`]).
struct Trees {
    /// Each vertex's piece and tree; `None` off the plant draws.
    of: Vec<Option<On>>,
    trees: Vec<Tree>,
}

/// A vertex's piece: its box, whether it is a plant's, and its tree.
#[derive(Clone, Copy)]
struct On {
    low: Vec3,
    high: Vec3,
    plant: bool,
    tree: usize,
}

struct Tree {
    low: Vec3,
    high: Vec3,
    /// Whether any of its pieces is a plant's (a card's middle strips can be all leaves).
    plant: bool,
    /// The lowest and highest vertex of the edge its wings meet on, which can lean; `None` for crossed cards.
    hub: Option<(Vec3, Vec3)>,
}

impl Tree {
    fn crown(&self) -> bool {
        crown(self.low, self.high)
    }

    /// The tree's axis at height `y`: on its wings' edge, else its box's middle.
    fn axis(&self, y: f32) -> Vec2 {
        match self.hub {
            Some((low, high)) if high.y - low.y > 0.1 => self.axis_unclamped(y.clamp(low.y, high.y)),
            _ => self.axis_unclamped(y),
        }
    }

    /// [`Tree::axis`] carried on past its wings' edge: affine in `y`.
    fn axis_unclamped(&self, y: f32) -> Vec2 {
        match self.hub {
            Some((low, high)) if high.y - low.y > 0.1 => low.xz().lerp(high.xz(), (y - low.y) / (high.y - low.y)),
            Some((low, _)) => low.xz(),
            None => ((self.low + self.high) * 0.5).xz(),
        }
    }
}

/// The pieces of `mesh`'s plant draws ([`pieces`]) grouped into trees, the pieces that share an axis: crossed cards
/// whose centres meet ([`SAME_AXIS`]), or wings with a vertex in the same place across, the edge they meet on (the
/// retail trees are three wings, each a slice of the tree's picture, cut in strips).
fn trees<'a>(mesh: &Mesh, plant: impl Fn(&Draw) -> Option<&'a Cut>) -> Trees {
    let pieces = pieces(&mesh.vertices, mesh, plant);
    let mut boxes: Vec<(Vec3, Vec3, bool)> = Vec::new();
    let mut of_root = HashMap::new();
    for &(root, low, high, plant) in pieces.iter().flatten() {
        of_root.entry(root).or_insert_with(|| {
            boxes.push((low, high, plant));
            boxes.len() - 1
        });
    }
    let piece_of = |i: usize| pieces[i].map(|(root, ..)| of_root[&root]);
    let mut tree: Vec<usize> = (0..boxes.len()).collect();
    fn find(tree: &mut [usize], mut i: usize) -> usize {
        while tree[i] != i {
            tree[i] = tree[tree[i]];
            i = tree[i];
        }
        i
    }
    let heights_meet = |a: (Vec3, Vec3, bool), b: (Vec3, Vec3, bool)| a.0.y <= b.1.y && b.0.y <= a.1.y;
    // Pieces with a vertex in the same place across.
    let place = |p: [f32; 3]| ((p[0] * 1000.0).round() as i64, (p[2] * 1000.0).round() as i64);
    let mut at: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for i in 0..mesh.vertices.len() {
        let Some(k) = piece_of(i) else { continue };
        let list = at.entry(place(mesh.vertices[i].position)).or_default();
        if !list.contains(&k) {
            list.push(k);
        }
    }
    for list in at.values() {
        for &j in &list[1..] {
            if heights_meet(boxes[list[0]], boxes[j]) {
                let (a, o) = (find(&mut tree, list[0]), find(&mut tree, j));
                tree[o] = a;
            }
        }
    }
    // Pieces whose centres meet.
    let centre = |(low, high, _): (Vec3, Vec3, bool)| ((low + high) * 0.5).xz();
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (k, &b) in boxes.iter().enumerate() {
        let c = (centre(b) / SAME_AXIS).floor();
        for dx in -1..=1 {
            for dz in -1..=1 {
                for &j in grid.get(&(c.x as i32 + dx, c.y as i32 + dz)).into_iter().flatten() {
                    if heights_meet(b, boxes[j]) && centre(b).distance(centre(boxes[j])) < SAME_AXIS {
                        let (a, o) = (find(&mut tree, k), find(&mut tree, j));
                        tree[o] = a;
                    }
                }
            }
        }
        grid.entry((c.x as i32, c.y as i32)).or_default().push(k);
    }
    // Each tree's box, whether it holds a plant, and its hub.
    let mut index = HashMap::new();
    let mut trees: Vec<Tree> = Vec::new();
    let mut tree_of = vec![0; boxes.len()];
    for (k, &(low, high, plant)) in boxes.iter().enumerate() {
        let t = *index.entry(find(&mut tree, k)).or_insert_with(|| {
            trees.push(Tree { low, high, plant, hub: None });
            trees.len() - 1
        });
        let tree = &mut trees[t];
        (tree.low, tree.high, tree.plant) = (tree.low.min(low), tree.high.max(high), tree.plant || plant);
        tree_of[k] = t;
    }
    for (i, v) in mesh.vertices.iter().enumerate() {
        let p = Vec3::from(v.position);
        let Some(k) = piece_of(i).filter(|_| at[&place(v.position)].len() > 1) else { continue };
        let hub = trees[tree_of[k]].hub.get_or_insert((p, p));
        if p.y < hub.0.y {
            hub.0 = p;
        }
        if p.y > hub.1.y {
            hub.1 = p;
        }
    }
    let of = (0..mesh.vertices.len())
        .map(|i| {
            let k = piece_of(i)?;
            let (low, high, plant) = boxes[k];
            Some(On { low, high, plant, tree: tree_of[k] })
        })
        .collect();
    Trees { of, trees }
}

/// The piece of the plant draws each of `vertices` (of `mesh`) is on, its box, and whether it is a plant's: a
/// connected piece of the draws `plant` gives a cut-out texture for whose part of it is cut out. `None` off them.
fn pieces<'a>(
    vertices: &[Vertex],
    mesh: &Mesh,
    plant: impl Fn(&Draw) -> Option<&'a Cut>,
) -> Vec<Option<(usize, Vec3, Vec3, bool)>> {
    let mut parent: Vec<usize> = (0..vertices.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    // Each vertex's cut-out texture.
    let mut on = vec![None; vertices.len()];
    for (draw, cut) in mesh.draws.iter().filter_map(|draw| Some((draw, plant(draw)?))) {
        for triangle in mesh.indices[draw.indices.clone()].chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|k| triangle[k] as usize);
            if [a, b, c].iter().any(|&i| i >= vertices.len()) {
                continue;
            }
            for i in [a, b, c] {
                on[i] = Some(cut);
            }
            let (ra, rb, rc) = (root(&mut parent, a), root(&mut parent, b), root(&mut parent, c));
            parent[rb] = ra;
            parent[rc] = ra;
        }
    }
    // Each piece's box and its uv rectangle.
    let mut span = vec![(Vec3::MAX, Vec3::MIN, [f32::MAX; 2], [f32::MIN; 2]); vertices.len()];
    for i in (0..vertices.len()).filter(|&i| on[i].is_some()) {
        let r = root(&mut parent, i);
        let (p, uv) = (Vec3::from(vertices[i].position), vertices[i].uv0);
        let (low, high, uv_low, uv_high) = &mut span[r];
        (*low, *high) = (low.min(p), high.max(p));
        *uv_low = [uv_low[0].min(uv[0]), uv_low[1].min(uv[1])];
        *uv_high = [uv_high[0].max(uv[0]), uv_high[1].max(uv[1])];
    }
    // Whether each piece's part of a texture is cut out, tested once a piece and texture (a piece's vertices share
    // its rectangle, which can span thousands of texels).
    let mut tested = HashMap::new();
    (0..vertices.len())
        .map(|i| {
            let cut = on[i]?;
            let r = root(&mut parent, i);
            let (low, high, uv_low, uv_high) = span[r];
            let cut = *tested.entry((r, std::ptr::from_ref(cut))).or_insert_with(|| cut.under(uv_low, uv_high));
            Some((r, low, high, cut))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{structure, Cut};

    #[test]
    fn pieces_by_their_part_of_the_texture() {
        // A 4x4 texture cut out in its left half.
        let cut = Cut::new(4, &(0..16).map(|i| i % 4 < 2).collect::<Vec<_>>());
        assert!(cut.under([0.0, 0.0], [0.5, 1.0]));
        assert!(!cut.under([0.5, 0.0], [1.0, 1.0]), "the opaque half");
        assert!(!cut.under([0.6, 0.2], [0.6, 0.2]), "a point's texel");
        assert!(cut.under([1.6, 0.0], [2.4, 0.5]), "repeated, across the edge");
        assert!(cut.under([-3.0, 0.0], [5.0, 1.0]), "repeated, the whole texture");
    }

    #[test]
    fn pieces_count_their_texels_one_by_one() {
        // An 8x4 texture cut out in a scattered pattern, against counting each texel under rectangles across its edges.
        let texels: Vec<bool> = (0..32u32).map(|i| i.wrapping_mul(2_654_435_761) >> 29 < 3).collect();
        let cut = Cut::new(8, &texels);
        let counted = |low: [f32; 2], high: [f32; 2]| {
            let span = |size: i64, k: usize| {
                let from = (low[k] * size as f32).floor() as i64;
                from..((high[k] * size as f32).ceil() as i64).clamp(from + 1, from + size)
            };
            let (columns, rows) = (span(8, 0), span(4, 1));
            let under = rows
                .clone()
                .flat_map(|y| columns.clone().map(move |x| (x, y)))
                .filter(|&(x, y)| texels[y.rem_euclid(4) as usize * 8 + x.rem_euclid(8) as usize])
                .count();
            under as f32 > super::CUT * (columns.count() * rows.count()) as f32
        };
        for k in 0..400 {
            let at = |n: u32| (k * 7 + n * 13) as f32 % 29.0 / 7.0 - 2.0;
            let low = [at(1), at(2)];
            let high = [low[0] + at(3).abs() * 0.3, low[1] + at(4).abs() * 0.3];
            assert_eq!(cut.under(low, high), counted(low, high), "{low:?} {high:?}");
        }
    }

    #[test]
    fn structures_by_name() {
        for name in [
            "FENCE_TILE.tga",
            "POWERPYLON01_UPDATE.tga",
            "MAPS\\X\\fieldfnc.dds",
            "Net.dds",
            "panneausta1.dds",
            "divaci1.dds",
            "lidi2D.dds",
            "S-Spectators Itally-2_T.dds",
            "S-01_Person_Albedo_T.dds",
        ] {
            assert!(structure(name), "{name}");
        }
        for name in ["ObjectTexturePlateTransparent.dds", "HO_LD_DECIDUOUS_WOOD_1.tga", "genet2.dds", "Cabinet.dds"] {
            assert!(!structure(name), "{name}");
        }
    }
}

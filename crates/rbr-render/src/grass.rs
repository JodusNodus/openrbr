//! A grass field over the off-road ground near the road: a renderer addition behind
//! [`crate::renderer::Enhance::grass`] (ours, a proof of concept, not the game's look).
//!
//! The ground within [`BAND`] of the driveline (the off-road ground, [`crate::puddles::mark_ground`], and the road mesh,
//! whose shoulders can be grass) gets clumps of two crossed cards of grass blades drawn here ([`texture`]), up to
//! [`DENSITY`] a square metre where its colour (its texture, blurred, under its vertex tint) is green or straw
//! ([`green`]) at the clump's middle and its cards' ends (so none reaches over the road). Each card takes the ground's
//! hue, a little greener, and brightness under it ([`GRASS_MARK`]); noise patches make the field taller or shorter,
//! lusher or drier ([`patches`]), the cards lean a little and the shader sways their tops in the wind (`grass_sway`).
//!
//! [`add`] keeps the ground's triangles by [`TILE`] square tile (a [`Field`]); the stage holds [`SLOTS`] tile meshes,
//! and the renderer keeps those of the tiles about the eye filled with their cards ([`Ring`]): all of them near and a
//! thinning share further out ([`density`]), drawn within [`CLOSE_DISTANCE`] ([`Lod::Close`]). A tile holds a little
//! more than its density draws ([`take`]), its outermost cards shrunk in ([`Card::grow`]) so the field thickens
//! smoothly as the eye nears rather than popping at share steps. A worker thread makes the cards off the frame
//! ([`Ring::update`]), the frame only uploading finished tiles. They
//! are cut-out stage draws marked as grass tufts, so the plants' lighting takes them (`tuft_light` in `shader.wgsl`)
//! and they cast no sun shadow; they take the sky's occlusion and the bounce computed at load for the ground under
//! them ([`Light`]).

use crate::foliage::test_texels;
use crate::puddles::{self, GROUND_MARK};
use crate::stage::{
    Address, Anchor, Bounds, Draw, DrawState, Image, Layer, Lod, Mesh, Stage, StageTexture, TextureName, TextureRef,
    TextureSet, Vertex, OBJECT_ALPHA_REF,
};
use bytemuck::Zeroable;
use glam::{Vec2, Vec3, Vec3Swizzles, Vec4Swizzles};
use rbr_formats::dds;
use std::collections::HashMap;
use std::sync::Arc;

/// The grass texture's file name in [`Stage::texture_names`] (not a game file: [`texture`] draws it).
pub const TEXTURE: &str = "rerbr-grass.dds";
/// How far from the driveline, metres, grass grows.
const BAND: f32 = 20.0;
/// Clumps (two crossed cards) a square metre on fully grassy ground within [`FULL`] of the driveline, thinning to
/// [`FAR_SHARE`] of it at [`BAND`].
const DENSITY: f32 = 40.0;
const FULL: f32 = 10.0;
const FAR_SHARE: f32 = 0.4;
/// The steepest ground that takes grass: the cosine of its slope.
const LEVEL: f32 = 0.45;
/// The side of the square tiles, metres.
const TILE: f32 = 8.0;
/// How far from the eye grass draws, metres: the tiles with their middle within this of the eye stream in and draw
/// ([`CLOSE_DISTANCE`] covers a tile past it).
pub(crate) const REACH: f32 = 100.0;
/// How far past [`REACH`], about the eye a few frames ahead along its motion, tiles are built before the eye needs
/// them.
const PREFETCH: f32 = 16.0;
/// The drawn grass is full within this of the eye ([`density`]).
const FULL_DENSE: f32 = 14.0;
const HALVING: f32 = 10.0;
/// A tile holds this much more of its cards than its density draws, so the drawn share's outermost cards shrink in
/// rather than pop ([`Card::grow`]).
const GROW_MARGIN: f32 = 1.5;
/// The inclusion's steps a tile over ([`take`]): a tile rebuilds when its inclusion crosses one.
const LADDER: f32 = 24.0;
/// The tile meshes the stage holds, filled with the tiles about the eye: how many of them hold up to which share of
/// [`CAPACITY`] (the nearer tiles hold more of their cards, [`take`]). Sized to hold every drawn tile along a road
/// within [`REACH`], within 300 MB of video memory ([`tests::slots_fit_video_budget`]).
const SLOTS: [(usize, f32); 5] = [(48, 1.0), (40, 0.5), (52, 0.25), (100, 0.125), (320, 0.0625)];
/// The most cards a tile holds: its clumps at [`DENSITY`], with room for ground laid over ground.
const CAPACITY: usize = (TILE * TILE * DENSITY * 2.0 * 1.25) as usize;
/// The tiles built on the frame itself, within this of the eye, so the near field is there on the first frame and
/// steady run to run; the rest the worker thread builds ([`Ring::update`]).
const SYNC_DISTANCE: f32 = 24.0;
/// How many tiles the frame uploads a frame ([`UPLOAD_TILES`], [`UPLOAD_BYTES`]).
const UPLOAD_TILES: usize = 64;
const UPLOAD_BYTES: usize = 8_000_000;
/// A card's height range, metres, before its patch and kind scale it, and its width against its height.
const HEIGHT: [f32; 2] = [0.1, 0.2];
const WIDTH: [f32; 2] = [1.0, 1.6];
/// How deep a card's foot goes into the ground, metres: the ground curves away from the plane it stands on.
const SINK: f32 = 0.03;
/// How far a card's top leans, against its height.
const LEAN: f32 = 0.25;
/// The share of height a card's foot is lit as (`tuft_light`'s occlusion and sun at the foot): dense grass shades its
/// own foot.
const FOOT: f32 = 0.0;
/// `specular.x` of the cards is this plus their linear brightness against the texture's mean, by which the shader
/// scales their albedo (`GRASS_MARK` in `shader.wgsl`), within [`BRIGHT`]: the ground's under them, as the tint takes
/// only its hue.
const GRASS_MARK: f32 = 4.0;
const BRIGHT: [f32; 2] = [0.2, 2.5];
/// The cards' brightness against the ground's: the plants' lighting (wrapped sun, light through the blades, sunlit
/// tips) gives them more light than the ground.
const SHADE: f32 = 0.62;
/// The ground's grassiness ([`green`]) where grass starts and where it is full, off the road and on the road mesh
/// (whose shoulders take only plainly green ground: a pale gravel road is near straw).
const GRASSY: [[f32; 2]; 2] = [[0.02, 0.07], [0.05, 0.11]];
/// The texels each side the ground's texture is averaged over for the grass ([`blur`]).
const BLUR: usize = 3;
/// The share of the ground's baked vertex tint its colour keeps (`GROUND_HUE` in `shader.wgsl`).
const GROUND_HUE: f32 = 0.3;
/// How much more saturated the grass is than the ground under it: the ground's texture mixes grass with earth.
const SATURATION: f32 = 1.15;
/// The hue of lush grass (display, its largest channel 1), which the grass's hue leans towards by [`LUSH_SHARE`] away
/// from its dry patches: the ground's texture is greyed by its earth, the grass on it is green.
const LUSH: Vec3 = Vec3::new(0.72, 1.0, 0.42);
const LUSH_SHARE: f32 = 0.25;
/// The tint of dry grass, on the ground's hue, in the driest patches.
const STRAW: Vec3 = Vec3::new(1.12, 1.0, 0.72);
/// The texture's pictures: [`COLUMNS`] by [`ROWS`], each [`PICTURE`] texels (about a card's shape). The first eight
/// are lawn, then four meadow, two dry grass and two grass with seed heads ([`Kind`]).
const COLUMNS: usize = 4;
const ROWS: usize = 4;
const PICTURE: (usize, usize) = (256, 256);

/// The grass's ground by tile ([`add`]), from which a tile's cards are made when it is drawn ([`Field::cards`]).
pub struct Field {
    triangles: Vec<Ground>,
    tiles: HashMap<(i32, i32), Vec<u32>>,
    textures: Vec<Texels>,
    line: Line,
    /// The grass texture's mean colour (display).
    mean: Vec3,
}

impl Field {
    /// How many ground triangles it holds.
    pub fn triangles(&self) -> usize {
        self.triangles.len()
    }

    /// Each ground triangle's stage mesh index (once the tile meshes are in) and its corners' vertices there, whose
    /// [`Light`] the renderer reads back.
    pub(crate) fn corners(&self) -> impl Iterator<Item = (usize, [u32; 3])> + '_ {
        self.triangles.iter().map(|g| g.corners)
    }
}

/// A ground triangle: its corners' positions, texture coordinates, vertex alphas and tints ([`tint`]), whether it is
/// the road mesh's, its textures' indices in [`Field::textures`] (the second blended by `uv1` and the alpha), its
/// upward normal, its area, and its stage mesh's index and corners' vertices there.
struct Ground {
    p: [Vec3; 3],
    uv0: [Vec2; 3],
    uv1: [Vec2; 3],
    alpha: [f32; 3],
    hue: [Vec3; 3],
    road: bool,
    first: u32,
    second: Option<u32>,
    normal: Vec3,
    area: f32,
    corners: (usize, [u32; 3]),
}

/// One card of grass: its foot's middle, the ground's normal there, half its width across (x and z), its top's lean,
/// its height, its brightness against the texture's ([`GRASS_MARK`]), its tint, its picture, the ground's light, and
/// how grown it is (0 a point at its foot, 1 full): the inclusion's outermost cards shrink in rather than pop.
#[derive(Clone, Copy)]
struct Card {
    at: Vec3,
    normal: Vec3,
    across: Vec2,
    lean: Vec2,
    height: f32,
    bright: f32,
    hue: [u8; 3],
    picture: usize,
    light: Light,
    grow: f32,
}

/// The light the load computes for a ground vertex, unpacked: how little of the sky it sees
/// ([`Vertex::occlusion`]), the octahedral pair of the direction it sees it in ([`Vertex::sky_dir`], 0 for none) and
/// what it gets off the stage per unit of the sun's and of the sky's light ([`Vertex::bounce`]). A card takes the
/// ground's at its foot, blended across the triangle as the GPU blends it; all 0 (open sky) when the renderer has
/// none.
#[derive(Clone, Copy, Default)]
pub(crate) struct Light {
    occlusion: f32,
    sky: Vec2,
    bounce: [Vec3; 2],
}

/// The range of the bounce's 10-bit shares (`RANGE` in `bounce.wgsl`).
const BOUNCE_RANGE: f32 = 2.0;

impl Light {
    pub(crate) fn of(v: &Vertex) -> Light {
        let snorm = |w: u32| (f32::from(w as u16 as i16) / 32767.0).max(-1.0);
        let shares = |w: u32| {
            Vec3::new((w & 1023) as f32, (w >> 10 & 1023) as f32, (w >> 20 & 1023) as f32) * (BOUNCE_RANGE / 1023.0)
        };
        Light {
            occlusion: v.occlusion,
            sky: Vec2::new(snorm(v.sky_dir), snorm(v.sky_dir >> 16)),
            bounce: v.bounce.map(shares),
        }
    }

    /// The light at barycentric weights `w` of a triangle whose corners have `corners`.
    fn blend(corners: &[Light; 3], w: [f32; 3]) -> Light {
        let [a, b, c] = corners;
        Light {
            occlusion: a.occlusion * w[0] + b.occlusion * w[1] + c.occlusion * w[2],
            sky: a.sky * w[0] + b.sky * w[1] + c.sky * w[2],
            bounce: [0, 1].map(|k| a.bounce[k] * w[0] + b.bounce[k] * w[1] + c.bounce[k] * w[2]),
        }
    }

    /// Packed as a vertex holds it: its `occlusion`, `sky_dir` and `bounce`.
    fn packed(self) -> (f32, u32, [u32; 2]) {
        let snorm = |x: f32| u32::from((x.clamp(-1.0, 1.0) * 32767.0).round() as i16 as u16);
        let sky = if self.sky == Vec2::ZERO { 0 } else { (snorm(self.sky.x) | snorm(self.sky.y) << 16).max(1) };
        let shares = |c: Vec3| {
            let q = (c / BOUNCE_RANGE).clamp(Vec3::ZERO, Vec3::ONE) * 1023.0;
            q.x.round() as u32 | (q.y.round() as u32) << 10 | (q.z.round() as u32) << 20
        };
        (self.occlusion, sky, self.bounce.map(shares))
    }
}

/// Keeps the grass's ground of `stage` (after its textures are loaded and its ground and roads marked), and adds its
/// texture and its [`SLOTS`] empty tile meshes (before the blended ones) for the renderer to fill
/// ([`crate::renderer::Renderer::set_grass`]). `None` on a snow stage or without a driveline.
pub fn add(stage: &mut Stage) -> Option<Field> {
    if stage.snow_ground || stage.driveline.len() < 2 {
        return None;
    }
    let index = stage.texture_names.keys().chain(stage.textures.keys()).filter(|t| t.set == TextureSet::Stage);
    let texture = TextureRef { set: TextureSet::Stage, index: index.map(|t| t.index + 1).max().unwrap_or(0) };
    let (bytes, mean) = self::texture();
    let line = Line::new(&stage.driveline);
    let mut textures = Vec::new();
    let mut texture_index: HashMap<TextureRef, Option<u32>> = HashMap::new();
    let mut texels = |t: Option<TextureRef>| {
        *texture_index.entry(t?).or_insert_with(|| {
            let found = Texels::of(stage, t?)?;
            textures.push(found);
            Some(textures.len() as u32 - 1)
        })
    };
    let mut triangles = Vec::new();
    let mut tiles: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    // The tile meshes go before the first blended mesh, which moves the meshes from there on.
    let blended = stage.meshes.iter().position(|m| m.layer == Layer::Blended).unwrap_or(stage.meshes.len());
    let slots = SLOTS.iter().map(|&(count, _)| count).sum::<usize>();
    let meshes = stage.meshes.iter().enumerate().filter(|(_, m)| m.anchor == Anchor::World && m.layer == Layer::Solid);
    for (index, mesh) in meshes {
        let index = if index < blended { index } else { index + slots };
        for draw in mesh.draws.iter().filter(|d| d.lod != Lod::Far && puddles::ground(mesh, d)) {
            let Some(first) = texels(draw.state.textures[0]) else { continue };
            let second = if draw.state.second_texture { texels(draw.state.textures[1]) } else { None };
            for t in mesh.indices[draw.indices.clone()].chunks_exact(3) {
                let corners = [t[0], t[1], t[2]];
                let v = corners.map(|i| &mesh.vertices[i as usize]);
                // The off-road ground, or the road mesh, whose shoulders can be grass: its texture says where.
                if v.iter().any(|v| v.edge != GROUND_MARK && v.wet <= 0.0) {
                    continue;
                }
                let p = v.map(|v| Vec3::from(v.position));
                let cross = (p[1] - p[0]).cross(p[2] - p[0]);
                let normal = cross.normalize_or_zero() * cross.y.signum();
                let (low, high) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
                let near = line.distance(((low + high) * 0.5).xz()) - (high - low).xz().length() * 0.5;
                if normal.y < LEVEL || cross.y.abs() < 1e-6 || near > BAND {
                    continue;
                }
                let triangle = triangles.len() as u32;
                triangles.push(Ground {
                    p,
                    uv0: v.map(|v| Vec2::from(v.uv0)),
                    uv1: v.map(|v| Vec2::from(v.uv1)),
                    alpha: v.map(|v| f32::from(v.color[3]) / 255.0),
                    hue: v.map(|v| tint(v.color)),
                    road: v.iter().any(|v| v.wet > 0.0),
                    first,
                    second,
                    normal,
                    area: cross.length() * 0.5,
                    corners: (index, corners),
                });
                let (from, to) = (tile_of(low), tile_of(high));
                for x in from.0..=to.0 {
                    for z in from.1..=to.1 {
                        tiles.entry((x, z)).or_default().push(triangle);
                    }
                }
            }
        }
    }
    let slots =
        SLOTS.iter().flat_map(|&(count, share)| (0..count).map(move |_| slot_mesh(texture, slot_capacity(share))));
    stage.meshes.splice(blended..blended, slots);
    stage
        .texture_names
        .insert(texture, TextureName { track: stage.name.clone(), file_name: TEXTURE.to_owned(), ground: false });
    stage.textures.insert(texture, StageTexture { request: TEXTURE.to_owned(), image: Some(Image::Dds(bytes)) });
    Some(Field { triangles, tiles, textures, line, mean })
}

impl Field {
    /// The cards of `tile`: `take` of them (0 to 1, an even subset: the same cards at any inclusion that holds
    /// them) with `density` of them full size and the rest shrunk in ([`Card::grow`]), lit by `light`, each
    /// triangle's corners' (none: open sky). `take` of 1 draws the same cards as before, full size.
    fn cards(&self, tile: (i32, i32), take: f32, density: f32, capacity: usize, light: &[[Light; 3]]) -> Vec<Card> {
        let mut cards = Vec::new();
        let Some(list) = self.tiles.get(&tile) else { return cards };
        let luma = |c: Vec3| c.powf(2.2).dot(Vec3::new(0.2126, 0.7152, 0.0722));
        for &i in list {
            let g = &self.triangles[i as usize];
            let corners = light.get(i as usize).copied().unwrap_or_default();
            let p = g.p;
            let centre = (p[0] + p[1] + p[2]) / 3.0;
            let mut random = Random::at(centre);
            let wanted = g.area * DENSITY + random.next();
            let (e1, e2) = ((p[1] - p[0]).xz(), (p[2] - p[0]).xz());
            let det = e1.perp_dot(e2);
            // Barycentric along the triangle's plane, outside it too.
            let barycentric = |q: Vec3| {
                let d = (q - p[0]).xz();
                let (a, b) = (d.perp_dot(e2) / det, e1.perp_dot(d) / det);
                [1.0 - a - b, a, b]
            };
            let colour_at = |w: [f32; 3]| {
                let blend = |c: &[Vec2; 3]| c[0] * w[0] + c[1] * w[1] + c[2] * w[2];
                let mut colour = self.textures[g.first as usize].at(blend(&g.uv0));
                if let Some(second) = g.second {
                    let alpha = (g.alpha[0] * w[0] + g.alpha[1] * w[1] + g.alpha[2] * w[2]).clamp(0.0, 1.0);
                    let over = self.textures[second as usize].at(blend(&g.uv1));
                    colour = colour.lerp(over.xyz().extend(1.0), over.w * alpha);
                }
                colour.xyz() * (g.hue[0] * w[0] + g.hue[1] * w[1] + g.hue[2] * w[2])
            };
            for _ in 0..wanted as usize {
                // Every card draws the same numbers whatever it becomes, so the sequence holds across tiles and shares.
                let [a, b, keep, chance, turn, width, height, pick, lean, lean_turn] = [0; 10].map(|_| random.next());
                let (a, b) = if a + b > 1.0 { (1.0 - a, 1.0 - b) } else { (a, b) };
                let at = p[0] * (1.0 - a - b) + p[1] * a + p[2] * b;
                if tile_of(at) != tile || keep >= take {
                    continue;
                }
                let grow = grow_of(keep, take, density);
                let distance = self.line.distance(at.xz());
                let near = 1.0 - (1.0 - FAR_SHARE) * smoothstep(FULL, BAND, distance);
                let fade = 1.0 - smoothstep(BAND - 6.0, BAND, distance);
                let colour = colour_at([1.0 - a - b, a, b]);
                if chance >= green(colour, g.road) * near * fade {
                    continue;
                }
                let (tall, dry) = patches(at.xz());
                let kind = Kind::pick(pick, tall, dry);
                let height = lerp(HEIGHT, height) * tall * kind.height();
                let angle = turn * std::f32::consts::PI;
                let across = Vec2::new(angle.cos(), angle.sin()) * height * lerp(WIDTH, width) * 0.5;
                // Both cards' ends need half the middle's grassiness: off the road, through the ground's patches.
                let green_at = |q: Vec3| chance * 0.5 < green(colour_at(barycentric(q)), g.road) * near;
                let ends = [across, Vec2::new(-across.y, across.x)].map(|a| Vec3::new(a.x, 0.0, a.y));
                if !ends.iter().all(|&end| green_at(at + end) && green_at(at - end)) {
                    continue;
                }
                let hue = colour / self.mean;
                let grey = Vec3::splat(hue.element_sum() / 3.0);
                let hue = grey.lerp(hue, SATURATION).max(Vec3::splat(0.01));
                let hue = (hue / hue.max_element()).lerp(LUSH, LUSH_SHARE * (1.0 - dry));
                let hue = hue.lerp(hue * STRAW, dry * 0.35);
                let hue = (hue / hue.max_element() * 255.0).round().as_uvec3();
                let lean_angle = lean_turn * std::f32::consts::TAU;
                // Each clump's cards a little brighter or darker than the ground's.
                let bright = SHADE * luma(colour) / luma(self.mean) * (0.85 + 0.3 * (pick * 37.0).fract());
                let card = Card {
                    at,
                    normal: g.normal,
                    across,
                    lean: Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean * LEAN * height,
                    height,
                    bright: bright.clamp(BRIGHT[0], BRIGHT[1]),
                    hue: [hue.x as u8, hue.y as u8, hue.z as u8],
                    picture: kind.picture(pick),
                    light: Light::blend(&corners, [1.0 - a - b, a, b]),
                    grow,
                };
                // The other card across it, a little shorter, with another picture of its kind.
                let crossed = Card {
                    across: Vec2::new(-across.y, across.x),
                    height: height * 0.85,
                    picture: kind.picture((pick * 53.0).fract()),
                    ..card
                };
                cards.extend([card, crossed]);
                if cards.len() + 2 > capacity {
                    return cards;
                }
            }
        }
        cards
    }
}

/// The kinds of the texture's pictures.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Lawn,
    Meadow,
    Dry,
    Seeds,
}

impl Kind {
    /// A card's kind by its draw `pick` (0 to 1) and its patch's tallness and dryness ([`patches`]).
    fn pick(pick: f32, tall: f32, dry: f32) -> Kind {
        let seeds = 0.04 + 0.1 * smoothstep(1.0, 1.4, tall);
        let dry_share = 0.04 + 0.4 * dry;
        let meadow = 0.2 + 0.3 * smoothstep(0.8, 1.3, tall);
        if pick < seeds {
            Kind::Seeds
        } else if pick < seeds + dry_share {
            Kind::Dry
        } else if pick < seeds + dry_share + meadow {
            Kind::Meadow
        } else {
            Kind::Lawn
        }
    }

    /// Its height against a lawn card's.
    fn height(self) -> f32 {
        match self {
            Kind::Lawn => 1.0,
            Kind::Meadow => 1.3,
            Kind::Dry => 1.15,
            Kind::Seeds => 1.9,
        }
    }

    /// Its picture in the texture, one of its own by `pick`.
    fn picture(self, pick: f32) -> usize {
        match self {
            Kind::Lawn => ((pick * 97.0).fract() * 8.0) as usize % 8,
            Kind::Meadow => 8 + ((pick * 89.0).fract() * 4.0) as usize % 4,
            Kind::Dry => 12 + ((pick * 83.0).fract() * 2.0) as usize % 2,
            Kind::Seeds => 14 + ((pick * 79.0).fract() * 2.0) as usize % 2,
        }
    }
}

/// The field's patches at `p` (stage x and z): how tall the grass is against its mean (0.55 to 1.5) and how dry (0 to
/// 1), from value noise at a few metres and under a metre.
fn patches(p: Vec2) -> (f32, f32) {
    let tall = noise(p / 3.5, 1) * 0.7 + noise(p / 0.9, 2) * 0.3;
    let dry = smoothstep(0.6, 0.9, noise(p / 7.0, 3) * 0.8 + noise(p / 1.6, 4) * 0.2);
    (0.55 + 0.95 * tall, dry)
}

/// Smooth value noise, 0 to 1, of lattice `seed`.
fn noise(p: Vec2, seed: u64) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let at = |x: f32, y: f32| {
        let mut random = Random((cell.x + x) as i64 as u64 ^ ((cell.y + y) as i64 as u64).rotate_left(32) ^ seed << 56);
        random.next()
    };
    let s = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let top = at(0.0, 0.0) + (at(1.0, 0.0) - at(0.0, 0.0)) * s.x;
    let bottom = at(0.0, 1.0) + (at(1.0, 1.0) - at(0.0, 1.0)) * s.x;
    top + (bottom - top) * s.y
}

fn tile_of(p: Vec3) -> (i32, i32) {
    ((p.x / TILE).floor() as i32, (p.z / TILE).floor() as i32)
}

/// The density of the grass drawn with its tile middle `distance` metres from the eye: full within [`FULL_DENSE`],
/// halving every [`HALVING`] metres past it (as before to 34 m), then thinning on to nothing at [`REACH`].
/// Continuous in the distance, so the shrinking cards ([`Card::grow`]) glide rather than pop.
fn density(distance: f32) -> f32 {
    if distance <= FULL_DENSE {
        1.0
    } else {
        0.5f32.powf((distance - FULL_DENSE) / HALVING) * (1.0 - smoothstep(REACH - 20.0, REACH, distance))
    }
}

/// The share of a tile's cards its mesh holds for drawn density `density`: that and the [`GROW_MARGIN`] band past
/// it, up to a whole tile, on the [`LADDER`] steps. Zero where nothing draws.
fn take(density: f32) -> f32 {
    if density <= 0.0 {
        0.0
    } else {
        ((density * GROW_MARGIN * LADDER).ceil() / LADDER).min(1.0)
    }
}

/// [`take`] as its [`LADDER`] step, 1 to 24 (0 draws nothing and is never held).
fn step_of(density: f32) -> u32 {
    (take(density) * LADDER).round() as u32
}

/// How grown the card of rank `keep` is at drawn density `density` and inclusion `take`: full size within the drawn
/// density, shrinking smoothly to a point at the inclusion's edge (and full size everywhere at `take` 1).
fn grow_of(keep: f32, take: f32, density: f32) -> f32 {
    if take >= 1.0 || keep <= density {
        1.0
    } else {
        let t = ((take - keep) / (take - density)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// How many cards a tile mesh for `share` of a tile's cards holds.
fn slot_capacity(share: f32) -> usize {
    (CAPACITY as f32 * share) as usize
}

/// Reserve a mesh of `slots` for (`tile`, `take`): the smallest that fits, from meshes holding nothing `wanted`
/// (`spare_only`: only the wholly free ones, for the early tiles). `None` when already held or building, or when no
/// mesh fits.
fn reserve_slot(
    slots: &mut [Slot],
    in_flight: &std::collections::HashSet<Held>,
    tile: (i32, i32),
    take: u32,
    spare_only: bool,
    wanted: &dyn Fn(Option<Held>) -> bool,
) -> Option<(usize, usize)> {
    let held = Some((tile, take));
    if slots.iter().any(|s| s.held == held) || in_flight.contains(&(tile, take)) {
        return None;
    }
    let need = slot_capacity((take as f32 / LADDER).min(1.0));
    let slot = slots
        .iter_mut()
        .filter(|s| s.capacity >= need && (spare_only && s.held.is_none() || !spare_only && !wanted(s.held)))
        .min_by_key(|s| s.capacity)?;
    slot.held = held;
    Some((slot.mesh, slot.capacity))
}

/// An empty tile mesh of `capacity` cards' vertices, all zero (no area) until it is filled, and no bounds, so that the
/// load's bakes over the stage's extent leave it out.
fn slot_mesh(texture: TextureRef, capacity: usize) -> Mesh {
    let state = DrawState {
        textures: [Some(texture), None],
        shadow: None,
        address: [Address::Clamp; 2],
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
    };
    let indices: Vec<u32> = (0..capacity as u32).flat_map(|c| [0, 1, 2, 0, 2, 3].map(|k| c * 4 + k)).collect();
    Mesh {
        layer: Layer::Solid,
        anchor: Anchor::World,
        bounds: None,
        cell: None,
        draws: vec![Draw { indices: 0..indices.len(), state, lod: Lod::Close }],
        vertices: vec![Vertex::zeroed(); capacity * 4],
        indices,
    }
}

/// A tile and the inclusion of its cards a tile mesh holds: the tile and its [`take`] step ([`step_of`]).
type Held = ((i32, i32), u32);

/// A tile mesh's build job for the worker thread: the tile, its inclusion step and drawn density ([`density`]), and
/// how many cards its mesh holds.
struct Job {
    tile: (i32, i32),
    take: u32,
    density: f32,
    capacity: usize,
}

/// A finished job: the tile's first vertices and their bounds.
struct Done {
    tile: (i32, i32),
    take: u32,
    vertices: Vec<Vertex>,
    bounds: Bounds,
}

/// A tile mesh: its index in the renderer, how many cards it holds, and what it holds.
struct Slot {
    mesh: usize,
    capacity: usize,
    held: Option<Held>,
}

/// The tile meshes, and what each holds, filled about the eye ([`Ring::update`]). The cards are made on a worker
/// thread ([`Job`]); the frame only uploads finished tiles ([`Done`]).
pub(crate) struct Ring {
    field: Arc<Field>,
    slots: Vec<Slot>,
    /// Each ground triangle's corners' light ([`Field::corners`]).
    light: Arc<Vec<[Light; 3]>>,
    jobs: Option<std::sync::mpsc::SyncSender<Job>>,
    done: std::sync::mpsc::Receiver<Done>,
    in_flight: std::collections::HashSet<Held>,
    ready: Vec<Done>,
    last_eye: Option<Vec3>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Ring {
    fn drop(&mut self) {
        // Drop the sender first so the worker's receive fails and it ends, then join it.
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The worker thread making the tiles' cards ([`Job`] in, [`Done`] out): one end of each channel and its handle.
/// Respawned when it dies ([`Ring::update`]): the frame then only ever loses far tiles for a frame.
fn spawn_worker(
    field: &Arc<Field>,
    light: &Arc<Vec<[Light; 3]>>,
) -> (std::sync::mpsc::SyncSender<Job>, std::sync::mpsc::Receiver<Done>, Option<std::thread::JoinHandle<()>>) {
    let (jobs, receive) = std::sync::mpsc::sync_channel::<Job>(16);
    let (send, done) = std::sync::mpsc::channel::<Done>();
    let background = field.clone();
    let worker_light = light.clone();
    let worker = std::thread::Builder::new()
        .name("grass".into())
        .spawn(move || {
            while let Ok(job) = receive.recv() {
                let take = (job.take as f32 / LADDER).min(1.0);
                let cards = background.cards(job.tile, take, job.density, job.capacity, &worker_light);
                let (vertices, bounds) = tile_vertices(&cards);
                if send.send(Done { tile: job.tile, take: job.take, vertices, bounds }).is_err() {
                    break;
                }
            }
        })
        .ok();
    (jobs, done, worker)
}

impl Ring {
    /// The ring over `meshes`, the renderer's indices of the tile meshes and how many cards each holds, with the light
    /// of the ground's corners.
    pub(crate) fn new(field: Arc<Field>, meshes: Vec<(usize, usize)>, light: Vec<[Light; 3]>) -> Ring {
        let light = Arc::new(light);
        let (jobs, done, worker) = spawn_worker(&field, &light);
        Ring {
            field,
            slots: meshes.into_iter().map(|(mesh, capacity)| Slot { mesh, capacity, held: None }).collect(),
            light,
            jobs: Some(jobs),
            done,
            in_flight: std::collections::HashSet::new(),
            ready: Vec::new(),
            last_eye: None,
            worker,
        }
    }

    /// The finished tiles to upload for the eye at `eye`: each tile mesh's index with its first vertices (the ones to
    /// draw: the rest still hold an earlier tile's) and bounds. The tiles with ground within [`REACH`] of the eye are
    /// wanted nearest first, each with its [`density`] and inclusion ([`take`]); tiles about the eye a little ahead
    /// along its motion are built early ([`PREFETCH`]). Missing tiles within [`SYNC_DISTANCE`] are built on the frame
    /// itself, so the near field is steady run to run; the worker thread builds the rest, and uploads are capped a
    /// frame ([`UPLOAD_TILES`], [`UPLOAD_BYTES`]).
    pub(crate) fn update(&mut self, eye: Vec3) -> Vec<(usize, Vec<Vertex>, Bounds)> {
        // The worker dying loses its jobs: free their meshes and start another, so the frame only ever loses far
        // tiles for a frame.
        if self.worker.as_ref().is_some_and(|worker| worker.is_finished()) {
            for slot in self.slots.iter_mut() {
                if slot.held.is_some_and(|held| self.in_flight.contains(&held)) {
                    slot.held = None;
                }
            }
            self.in_flight.clear();
            let (jobs, done, worker) = spawn_worker(&self.field, &self.light);
            self.jobs = Some(jobs);
            self.done = done;
            self.worker = worker;
        }
        let velocity = self.last_eye.map(|last| eye - last).unwrap_or(Vec3::ZERO);
        self.last_eye = Some(eye);
        // Look a few frames ahead along the motion, up to a few tiles.
        let ahead = velocity * 2.0;
        let predicted = eye + if ahead.length() > TILE * 3.0 { ahead.normalize_or_zero() * TILE * 3.0 } else { ahead };
        let centre = |(x, z): (i32, i32)| Vec2::new((x as f32 + 0.5) * TILE, (z as f32 + 0.5) * TILE);
        let range = |at: Vec3, radius: f32| {
            let (cx, cz) = tile_of(at);
            let r = (radius / TILE).ceil() as i32;
            (cx - r..=cx + r).flat_map(move |x| (cz - r..=cz + r).map(move |z| (x, z)))
        };
        // The drawn tiles: ground within reach of the eye, nearest first.
        let reach = REACH + TILE;
        let mut wanted: Vec<((i32, i32), f32, f32, u32)> = range(eye, reach)
            .filter(|tile| self.field.tiles.contains_key(tile))
            .map(|tile| (tile, centre(tile).distance(eye.xz())))
            .filter(|&(_, d)| d < reach)
            .map(|(tile, d)| (tile, d, density(d)))
            .filter(|&(_, _, d)| d > 0.0)
            .map(|(tile, d, density)| (tile, d, density, step_of(density)))
            .collect();
        wanted.sort_by(|a, b| a.1.total_cmp(&b.1));
        // Tiles past the draw reach the eye is heading for: built early, into spare meshes only.
        let mut early: Vec<((i32, i32), f32, f32, u32)> = range(predicted, reach + PREFETCH)
            .filter(|tile| self.field.tiles.contains_key(tile))
            .map(|tile| (tile, centre(tile).distance(eye.xz())))
            .filter(|&(_, d)| d >= reach)
            .map(|(tile, _)| (tile, centre(tile).distance(predicted.xz())))
            .map(|(tile, d)| (tile, d, density(d)))
            .filter(|&(_, _, d)| d > 0.0)
            .map(|(tile, d, density)| (tile, d, density, step_of(density)))
            .collect();
        early.sort_by(|a, b| a.1.total_cmp(&b.1));
        early.truncate(8);
        let is_wanted = |held: Option<Held>| {
            wanted.iter().any(|&(t, _, _, k)| held == Some((t, k)))
                || early.iter().any(|&(t, _, _, k)| held == Some((t, k)))
        };
        // The drawn tiles first, nearest first; then the early ones into spare meshes.
        for &(tile, dist, density, take) in &wanted {
            if reserve_slot(&mut self.slots, &self.in_flight, tile, take, false, &is_wanted).is_none() {
                continue;
            }
            let capacity = self.slots.iter().find(|s| s.held == Some((tile, take))).map(|s| s.capacity).unwrap_or(0);
            if dist < SYNC_DISTANCE {
                let cards = self.field.cards(tile, (take as f32 / LADDER).min(1.0), density, capacity, &self.light);
                let (vertices, bounds) = tile_vertices(&cards);
                self.ready.push(Done { tile, take, vertices, bounds });
            } else if self
                .jobs
                .as_ref()
                .is_some_and(|jobs| jobs.try_send(Job { tile, take, density, capacity }).is_ok())
            {
                self.in_flight.insert((tile, take));
            } else {
                // The worker is busy: leave the tile for a nearer frame.
                if let Some(slot) = self.slots.iter_mut().find(|s| s.held == Some((tile, take))) {
                    slot.held = None;
                }
                break;
            }
        }
        for &(tile, _, density, take) in &early {
            if reserve_slot(&mut self.slots, &self.in_flight, tile, take, true, &is_wanted).is_none() {
                continue;
            }
            let capacity = self.slots.iter().find(|s| s.held == Some((tile, take))).map(|s| s.capacity).unwrap_or(0);
            if self.jobs.as_ref().is_some_and(|jobs| jobs.try_send(Job { tile, take, density, capacity }).is_ok()) {
                self.in_flight.insert((tile, take));
            } else {
                if let Some(slot) = self.slots.iter_mut().find(|s| s.held == Some((tile, take))) {
                    slot.held = None;
                }
                break;
            }
        }
        for done in self.done.try_iter() {
            self.in_flight.remove(&(done.tile, done.take));
            if self.slots.iter().any(|s| s.held == Some((done.tile, done.take))) {
                self.ready.push(done);
            }
        }
        // Upload the finished tiles nearest first: the drawn ones, then the early ones with what is left.
        let distance = |tile: (i32, i32)| centre(tile).distance(eye.xz());
        let mut ready = std::mem::take(&mut self.ready);
        ready.sort_by(|a, b| distance(a.tile).total_cmp(&distance(b.tile)));
        let drawn: std::collections::HashMap<(i32, i32), u32> = wanted.iter().map(|&(t, _, _, k)| (t, k)).collect();
        let ahead: std::collections::HashMap<(i32, i32), u32> = early.iter().map(|&(t, _, _, k)| (t, k)).collect();
        let mut filled = Vec::new();
        let mut bytes = 0;
        // The drawn tiles nearest first, then the early ones with what is left.
        let mut draw_waiting = false;
        for done in ready {
            let mesh = self.slots.iter().find(|s| s.held == Some((done.tile, done.take))).map(|s| s.mesh);
            let Some(mesh) = mesh else { continue };
            let fits = filled.len() < UPLOAD_TILES && bytes + done.vertices.len() * size_of::<Vertex>() <= UPLOAD_BYTES;
            if drawn.get(&done.tile) != Some(&done.take) {
                // No longer drawn at this inclusion: an early tile still, or dropped.
                if ahead.get(&done.tile) == Some(&done.take) && !draw_waiting && fits {
                    bytes += done.vertices.len() * size_of::<Vertex>();
                    filled.push((mesh, done.vertices, done.bounds));
                } else if ahead.get(&done.tile) == Some(&done.take) {
                    self.ready.push(done);
                }
                continue;
            }
            if fits {
                bytes += done.vertices.len() * size_of::<Vertex>();
                filled.push((mesh, done.vertices, done.bounds));
            } else {
                draw_waiting = true;
                self.ready.push(done);
            }
        }
        filled
    }
}

/// A tile mesh's first vertices, those of `cards`, and their bounds.
fn tile_vertices(cards: &[Card]) -> (Vec<Vertex>, Bounds) {
    let mut vertices = vec![Vertex::zeroed(); cards.len() * 4];
    let (mut low, mut high) = (Vec3::MAX, Vec3::MIN);
    let margin = Vec2::new(2.0 / (PICTURE.0 * COLUMNS) as f32, 2.0 / (PICTURE.1 * ROWS) as f32);
    for (card, out) in cards.iter().zip(vertices.chunks_exact_mut(4)) {
        let (column, row) = (card.picture % COLUMNS, card.picture / COLUMNS);
        let u = [column as f32 / COLUMNS as f32 + margin.x, (column + 1) as f32 / COLUMNS as f32 - margin.x];
        let v = [row as f32 / ROWS as f32 + margin.y, (row + 1) as f32 / ROWS as f32 - margin.y];
        let n = card.normal;
        let way = card.across.normalize_or_zero();
        let (occlusion, sky_dir, bounce) = card.light.packed();
        let corners = [(-1.0, false), (1.0, false), (1.0, true), (-1.0, true)];
        for (vertex, (side, top)) in out.iter_mut().zip(corners) {
            let x = card.across * side + if top { card.lean } else { Vec2::ZERO };
            // On the ground's plane through the foot.
            let ground = -(n.x * x.x + n.z * x.y) / n.y;
            let position = card.at + Vec3::new(x.x, ground + if top { card.height } else { -SINK }, x.y);
            // Shrunk in about the foot while growing ([`Card::grow`]): zero at the foot, no area.
            let position = card.at + (position - card.at) * card.grow;
            (low, high) = (low.min(position), high.max(position));
            *vertex = Vertex {
                position: position.to_array(),
                color: [card.hue[0], card.hue[1], card.hue[2], 255],
                uv0: [if side < 0.0 { u[0] } else { u[1] }, if top { v[0] } else { v[1] }],
                normal: n.to_array(),
                // A tuft for the plants' lighting, as `foliage::with_plants` marks grass: its share of height in `wet`
                // (the foot at [`FOOT`]), the way out across in `uv1`; and its brightness ([`GRASS_MARK`]).
                specular: [GRASS_MARK + card.bright, 1.0, 1.0],
                wet: if top { 1.0 } else { FOOT },
                uv1: (way * side).to_array(),
                // The top's height, for the wind (`grass_sway` in `shader.wgsl`).
                uv2: [if top { card.height } else { 0.0 }, 0.0],
                occlusion,
                sky_dir,
                bounce,
                ..Vertex::zeroed()
            };
        }
    }
    if cards.is_empty() {
        (low, high) = (Vec3::ZERO, Vec3::ZERO);
    }
    (vertices, Bounds { center: (low + high) * 0.5, half_extent: (high - low) * 0.5 })
}

/// How much grass ground of colour `rgb` (0 to 1) takes, off the road or on the road mesh, by how green or straw its
/// chromaticity is ([`GRASSY`]).
fn green(rgb: Vec3, road: bool) -> f32 {
    let c = rgb / rgb.element_sum().max(1e-3);
    // Green or straw (green over blue) and not earth (red over green) nor stone (grey).
    let [low, high] = GRASSY[usize::from(road)];
    smoothstep(low, high, c.y - c.z - 2.0 * (c.x - c.y - 0.02).max(0.0))
}

/// The tint of a ground vertex of colour `rgb`, as `own_light` in `shader.wgsl` takes it on the near ground: its hue,
/// [`GROUND_HUE`] of it.
fn tint([r, g, b, _]: [u8; 4]) -> Vec3 {
    let c = Vec3::new(r.into(), g.into(), b.into()) / 255.0;
    let most = c.max_element();
    let hue = Vec3::ONE.lerp(c / most.max(1e-3), smoothstep(0.02, 0.08, most));
    Vec3::ONE.lerp(hue, GROUND_HUE)
}

/// A ground texture's texels at its test level (`foliage::test_texels`), blurred ([`blur`]), sampled nearest and
/// repeated.
struct Texels {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

impl Texels {
    fn of(stage: &Stage, texture: TextureRef) -> Option<Texels> {
        let (width, rgba) = test_texels(stage, texture)?;
        let height = rgba.len() / 4 / width.max(1);
        (height > 0).then(|| Texels {
            width,
            height,
            rgba: blur(&blur(&rgba, width, height, (1, 0)), width, height, (0, 1)),
        })
    }

    /// RGBA, 0 to 1.
    fn at(&self, uv: Vec2) -> glam::Vec4 {
        let x = (uv.x.rem_euclid(1.0) * self.width as f32) as usize % self.width;
        let y = (uv.y.rem_euclid(1.0) * self.height as f32) as usize % self.height;
        let i = (y * self.width + x) * 4;
        glam::Vec4::from_array(std::array::from_fn(|c| f32::from(self.rgba[i + c]) / 255.0))
    }
}

/// `rgba` (`width` x `height`, repeated) averaged over [`BLUR`] texels each side along `step`: the grass follows the
/// ground's patches, not its texels.
fn blur(rgba: &[u8], width: usize, height: usize, step: (usize, usize)) -> Vec<u8> {
    let at = |x: usize, y: usize, c: usize| u32::from(rgba[((y % height) * width + x % width) * 4 + c]);
    let mut out = vec![0; rgba.len()];
    for y in 0..height {
        for x in 0..width {
            for c in 0..4 {
                let sum: u32 = (0..=2 * BLUR)
                    .map(|k| {
                        at(x + width * 2 + step.0 * k - step.0 * BLUR, y + height * 2 + step.1 * k - step.1 * BLUR, c)
                    })
                    .sum();
                out[(y * width + x) * 4 + c] = (sum / (2 * BLUR as u32 + 1)) as u8;
            }
        }
    }
    out
}

/// The driveline's segments in a grid, for the distance across to it.
struct Line {
    grid: HashMap<(i32, i32), Vec<(Vec2, Vec2)>>,
}

impl Line {
    fn new(points: &[Vec3]) -> Line {
        let mut grid: HashMap<(i32, i32), Vec<(Vec2, Vec2)>> = HashMap::new();
        for pair in points.windows(2) {
            let (a, b) = (pair[0].xz(), pair[1].xz());
            let (low, high) = (a.min(b) - BAND, a.max(b) + BAND);
            for x in Line::cell(low.x)..=Line::cell(high.x) {
                for z in Line::cell(low.y)..=Line::cell(high.y) {
                    grid.entry((x, z)).or_default().push((a, b));
                }
            }
        }
        Line { grid }
    }

    fn cell(c: f32) -> i32 {
        (c / BAND).floor() as i32
    }

    /// The distance across from `p` to the driveline, up to just past [`BAND`].
    fn distance(&self, p: Vec2) -> f32 {
        let segments = self.grid.get(&(Line::cell(p.x), Line::cell(p.y)));
        segments.into_iter().flatten().fold(BAND + 1.0, |near, &(a, b)| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            near.min(p.distance(a + ab * t))
        })
    }
}

/// A small hash-seeded generator (SplitMix64), so the same ground gets the same cards.
struct Random(u64);

impl Random {
    fn at(p: Vec3) -> Random {
        let q = p.to_array().map(|c| (c * 64.0).round() as i64 as u64);
        Random(q[0].wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ q[1].wrapping_mul(0xc2b2_ae3d_27d4_eb4f) ^ q[2])
    }

    /// 0 to 1.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn lerp([a, b]: [f32; 2], t: f32) -> f32 {
    a + (b - a) * t
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The texture being drawn: colour (display) and coverage per texel.
struct Canvas {
    width: usize,
    colour: Vec<Vec3>,
    alpha: Vec<f32>,
}

impl Canvas {
    /// One blade in the picture whose box is `left`, `top` (texels) and [`PICTURE`]: rooted at `root` across its bottom,
    /// rising `tall` with its tip `lean` across, bending over by `droop` (0 straight, 1 its tip back at half its height),
    /// `base` wide at the root and tapering to a point, shaded from `foot` to `tip`.
    #[allow(clippy::too_many_arguments)]
    fn blade(
        &mut self,
        (left, top): (usize, usize),
        root: f32,
        tall: f32,
        lean: f32,
        base: f32,
        droop: f32,
        foot: Vec3,
        tip: Vec3,
    ) {
        let (w, h) = (PICTURE.0 as f32, PICTURE.1 as f32);
        // A quadratic curve: up from the root, over towards the tip.
        let from = Vec2::new(root, h - 1.0);
        let bend = Vec2::new(root + lean * 0.35, h - 1.0 - tall);
        let to = Vec2::new(root + lean, h - 1.0 - tall * (1.0 - 0.5 * droop));
        let steps = ((tall + lean.abs()) * 2.0) as usize + 2;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let at = from * (1.0 - t) * (1.0 - t) + bend * 2.0 * t * (1.0 - t) + to * t * t;
            let half = base * 0.5 * (1.0 - t).powf(0.7) + 0.25;
            let colour = foot.lerp(tip, t.powf(0.7));
            let (x0, x1) = ((at.x - half - 1.0).floor() as i64, (at.x + half + 1.0).ceil() as i64);
            let (y0, y1) = ((at.y - half - 1.0).floor() as i64, (at.y + half + 1.0).ceil() as i64);
            for y in y0.max(0)..=y1.min(h as i64 - 1) {
                for x in x0.max(2)..=x1.min(w as i64 - 3) {
                    let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(at);
                    let cover = (half + 0.5 - d).clamp(0.0, 1.0);
                    if cover > 0.0 {
                        let i = (y as usize + top) * self.width + left + x as usize;
                        self.colour[i] = self.colour[i].lerp(colour, cover);
                        self.alpha[i] = self.alpha[i].max(cover);
                    }
                }
            }
        }
    }

    /// A seed head: an upright ellipse `long` by `thick` texels about `centre` in the picture at `left`, `top`.
    fn seeds(&mut self, (left, top): (usize, usize), centre: Vec2, long: f32, thick: f32, colour: Vec3) {
        let (w, h) = (PICTURE.0 as i64, PICTURE.1 as i64);
        for y in (centre.y - long) as i64..=(centre.y + long) as i64 {
            for x in (centre.x - thick) as i64..=(centre.x + thick) as i64 {
                let d = Vec2::new((x as f32 - centre.x) / thick, (y as f32 - centre.y) / long).length();
                let cover = ((1.0 - d) * thick).clamp(0.0, 1.0);
                if cover > 0.0 && (2..w - 2).contains(&x) && (0..h).contains(&y) {
                    let i = (y as usize + top) * self.width + left + x as usize;
                    self.colour[i] = self.colour[i].lerp(colour, cover);
                    self.alpha[i] = self.alpha[i].max(cover);
                }
            }
        }
    }
}

/// The grass texture: [`COLUMNS`] by [`ROWS`] pictures of blades ([`Kind`]), as an RGBA `.dds` with its mip chain, each
/// level's alpha scaled to keep the top level's cut-out coverage (so far grass does not thin away), and the blades'
/// mean colour (0 to 1, display).
fn texture() -> (Vec<u8>, Vec3) {
    let (width, height) = (PICTURE.0 * COLUMNS, PICTURE.1 * ROWS);
    let mut canvas = Canvas { width, colour: vec![Vec3::ZERO; width * height], alpha: vec![0.0; width * height] };
    let mut random = Random(0x5eed);
    let (w, h) = (PICTURE.0 as f32, PICTURE.1 as f32);
    // Lush and dry blades' foot and tip colours: near grey, as the ground's hue colours them, the foot in the shade of
    // the blades about it.
    let lush = (Vec3::new(0.2, 0.22, 0.17), Vec3::new(0.62, 0.66, 0.55));
    let straw = (Vec3::new(0.27, 0.25, 0.2), Vec3::new(0.76, 0.71, 0.56));
    for picture in 0..COLUMNS * ROWS {
        let corner = ((picture % COLUMNS) * PICTURE.0, (picture / COLUMNS) * PICTURE.1);
        let (count, dry, broad, droops) = match picture {
            0..=7 => (170, 0.06, 0.0, 0.3),
            8..=11 => (130, 0.1, 0.25, 0.55),
            12 | 13 => (130, 0.8, 0.0, 0.45),
            _ => (100, 0.2, 0.0, 0.3),
        };
        for _ in 0..count {
            // Rooted across the card, the outer blades shorter and leaning out; many short blades under a few tall.
            let out = random.next() * 2.0 - 1.0;
            let root = w * 0.5 + w * 0.42 * out;
            let tall = h * (0.18 + 0.8 * random.next().powf(1.7)) * (1.0 - 0.4 * out * out);
            let base = if random.next() < broad { 5.0 + 2.5 * random.next() } else { 2.0 + 2.0 * random.next() };
            let droop = if random.next() < droops { 0.2 + 0.8 * random.next() } else { 0.0 };
            let side = if random.next() < 0.5 { -1.0 } else { 1.0 };
            let lean = w * (0.08 * out + 0.05 * (random.next() - 0.5)) + side * droop * tall * 0.5;
            let dryness = if random.next() < dry { 0.6 + 0.4 * random.next() } else { 0.15 * random.next() };
            // Each blade a little lighter or darker, yellower or bluer.
            let shade = 0.75 + 0.35 * random.next();
            let k = random.next() - 0.5;
            let tint = Vec3::new(1.0 + 0.12 * k, 1.0, 1.0 - 0.16 * k) * shade;
            let foot = lush.0.lerp(straw.0, dryness) * tint;
            let tip = lush.1.lerp(straw.1, dryness) * tint;
            canvas.blade(corner, root, tall, lean, base, droop, foot, tip);
        }
        if picture >= 14 {
            // Seed heads on thin stalks over the grass.
            for _ in 0..5 {
                let root = w * (0.15 + 0.7 * random.next());
                let tall = h * (0.78 + 0.18 * random.next());
                let lean = w * 0.06 * (random.next() - 0.5);
                canvas.blade(corner, root, tall, lean, 1.4, 0.0, straw.0, straw.1 * 0.9);
                let tip = Vec2::new(root + lean, h - 1.0 - tall + 8.0);
                canvas.seeds(corner, tip, 8.0, 2.2, Vec3::new(0.62, 0.55, 0.36));
            }
        }
    }
    let Canvas { colour, alpha, .. } = canvas;
    // The cut texels take the blades' mean colour, so filtering darkens no edge.
    let (sum, weight) =
        colour.iter().zip(&alpha).fold((Vec3::ZERO, 0.0), |(sum, weight), (c, a)| (sum + *c * *a, weight + a));
    let mean = sum / weight.max(1.0);
    let pixels: Vec<u8> = colour
        .iter()
        .zip(&alpha)
        .flat_map(|(c, &a)| {
            let c = c.lerp(mean, (1.0 - a).clamp(0.0, 1.0));
            [c.x, c.y, c.z, a].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        })
        .collect();
    let bytes = dds::with_mips(&dds::rgba8_file(width as u32, height as u32, &pixels)).expect("an RGBA file");
    (keep_coverage(bytes, width, height), mean)
}

/// `bytes` (an RGBA `.dds` of `width` x `height` with its mip chain) with each lower level's alpha scaled so that as
/// many of its texels pass the cut-out test as of the top level's.
fn keep_coverage(mut bytes: Vec<u8>, width: usize, height: usize) -> Vec<u8> {
    let sizes: Vec<usize> = (0..)
        .map(|l| (width >> l).max(1) * (height >> l).max(1) * 4)
        .take_while({
            let mut done = false;
            move |&s| !std::mem::replace(&mut done, s == 4)
        })
        .collect();
    let mut at = bytes.len() - sizes.iter().sum::<usize>();
    let passing = |level: &[u8], scale: f32| {
        let cut = f32::from(OBJECT_ALPHA_REF);
        level.chunks_exact(4).filter(|t| f32::from(t[3]) * scale > cut).count() as f32 / (level.len() / 4) as f32
    };
    let top = passing(&bytes[at..at + sizes[0]], 1.0);
    for &size in &sizes {
        let level = &mut bytes[at..at + size];
        let (mut low, mut high) = (0.1f32, 8.0f32);
        for _ in 0..20 {
            let mid = (low + high) * 0.5;
            if passing(level, mid) < top {
                low = mid;
            } else {
                high = mid;
            }
        }
        for texel in level.chunks_exact_mut(4) {
            texel[3] = (f32::from(texel[3]) * high).min(255.0) as u8;
        }
        at += size;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_texture_keeps_its_coverage_down_the_mips() {
        let (bytes, mean) = texture();
        let dds = dds::parse(&bytes).unwrap();
        assert_eq!((dds.width as usize, dds.height as usize), (PICTURE.0 * COLUMNS, PICTURE.1 * ROWS));
        let cover = |data: &[u8]| {
            data.chunks_exact(4).filter(|t| t[3] > OBJECT_ALPHA_REF).count() as f32 / (data.len() / 4) as f32
        };
        let levels = dds.levels();
        let top = cover(levels[0].data);
        assert!((0.05..0.7).contains(&top), "{top}");
        for level in &levels[1..6] {
            assert!((cover(level.data) - top).abs() < 0.03, "{} {}", level.width, cover(level.data));
        }
        assert!(mean.max_element() - mean.min_element() < 0.12, "near grey: {mean}");
    }

    #[test]
    fn grass_grows_on_green_and_straw_ground_only() {
        // Green, then a straw meadow (Gabiria-Legazpi's), earth, grey stone and pale gravel.
        assert_eq!(green(Vec3::new(0.4, 0.5, 0.2), true), 1.0);
        assert!(green(Vec3::new(0.57, 0.53, 0.37), false) > 0.9);
        assert_eq!(green(Vec3::new(0.5, 0.4, 0.3), false), 0.0);
        assert_eq!(green(Vec3::splat(0.5), false), 0.0);
        assert_eq!(green(Vec3::new(0.6, 0.57, 0.5), true), 0.0);
    }

    #[test]
    fn density_halves_every_ten_metres_and_fades_at_the_reach() {
        assert_eq!(density(0.0), 1.0);
        assert_eq!(density(FULL_DENSE), 1.0);
        assert!((density(24.0) - 0.5).abs() < 1e-6, "{}", density(24.0));
        assert!((density(34.0) - 0.25).abs() < 1e-6, "{}", density(34.0));
        assert!((density(44.0) - 0.125).abs() < 1e-6, "{}", density(44.0));
        assert!(density(70.0) < 0.025, "{}", density(70.0));
        assert_eq!(density(REACH), 0.0);
        assert!(density(REACH - 1.0) < 0.005, "{}", density(REACH - 1.0));
        let mut last = 2.0;
        for d in (0..=10000).map(|k| k as f32 * 0.01) {
            assert!(density(d) <= last, "{d}");
            last = density(d);
        }
    }

    #[test]
    fn grown_cards_glide_in_rather_than_pop() {
        assert_eq!(grow_of(0.99, 1.0, 1.0), 1.0);
        assert_eq!(grow_of(0.0, 0.5, 0.25), 1.0);
        assert_eq!(grow_of(0.25, 0.5, 0.25), 1.0);
        assert_eq!(grow_of(0.5, 0.5, 0.25), 0.0);
        // Continuous across the band: no step bigger than a twenty-fifth of the height.
        let (mut last, mut worst) = (1.0f32, 0.0f32);
        for k in 0..=100 {
            let g = grow_of(0.25 + k as f32 * 0.0025, 0.5, 0.25);
            worst = worst.max((last - g).abs());
            last = g;
        }
        assert!(worst < 0.04, "{worst}");
    }

    #[test]
    fn near_tiles_hold_everything_full_size() {
        assert_eq!(take(1.0), 1.0);
        assert_eq!(step_of(1.0), LADDER as u32);
        assert_eq!(take(0.0), 0.0);
        assert!((take(0.5) - 0.75).abs() < 1e-6, "{}", take(0.5));
        for k in 0..500 {
            let p = Vec2::new(k as f32 * 0.37 - 90.0, k as f32 * 1.13 - 300.0);
            let (tall, dry) = patches(p);
            assert!((0.55..=1.5).contains(&tall) && (0.0..=1.0).contains(&dry), "{tall} {dry}");
        }
    }

    #[test]
    fn slots_fit_video_budget() {
        // Vertices and indices of every tile mesh, and the grass texture with its mips: under 300 MB.
        let cards: usize = SLOTS.iter().map(|&(count, share)| count * slot_capacity(share)).sum();
        let bytes = cards * 4 * size_of::<Vertex>()
            + cards * 6 * size_of::<u32>()
            + (PICTURE.0 * COLUMNS * PICTURE.1 * ROWS * 4) * 4 / 3;
        assert!(bytes < 300_000_000, "{bytes}");
    }
}

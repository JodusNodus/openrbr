//! Working dials in the cockpit, from the RallySimFans install's RBRHUD gauges (`rbr_formats::rsf::gauges`,
//! `notes/cameras-hud-launcher.md` "RBRHUD gauges"). Ours: the plugin is not traced; this reads its data and draws it the way the data
//! says. A gauge config replaces one car texture with its `remapped` copy (the car's texture with the painted needles
//! removed) and draws its elements on it: needles turned to the live values, lights and digits shown by their
//! conditions, numbers and bars. The drawing is on the GPU ([`gpu`]): the copy stays a texture of its own, and when a
//! value moves, the part of the config's dirty regions it touches is restored from the copy and the sprites drawn over
//! it, at every mip level. The first draw covers the whole texture.
//!
//! Ours, where the plugin's behaviour is not in its data: positions are texels of the remapped texture, a needle's at its
//! pivot and a sprite's at its top left corner; angles turn clockwise in texel space; a needle follows its value as a
//! spring of `springconstant * rotationspeed` with `dampingratio`; numbers start at their position's top and are
//! justified about it; mode 0 elements show always and the others on their page of the car's gauge mode, which
//! [`Dials::next_mode`] cycles from 1 (the plugin's End key). `UpShift` is the engine speed past the car's own upshift
//! point for the gear (its VCU's `Gear<N>Upshift`, off where that is 0), `RPMPercent` the engine speed's share of its
//! `RPMLimit`. A `Damage` element shows the HUD's body damage for every part (the live drive reports no part damage),
//! a `Pie` the wedge from its first angle, clockwise. The values the live drive does not report are fixed: warm oil, a
//! steady oil pressure while the engine runs, whole tyres; the warning lights follow the HUD's lamps. The emissive
//! masks light the faces by night (`shader.wgsl`, `DIAL_GLOW`). `Pulse` effects are not drawn.

use crate::renderer::Renderer;
use crate::stage::{Image, Stage, TextureRef, TextureSet};
use crate::start_board::Rgba;
use glam::Vec2;
use rbr_formats::dds;
use rbr_formats::rsf::gauges::{self, Atlas, Config, Element, Font, Kind};
use rbr_recording::shift::ShiftPoints;
use std::path::Path;
use std::sync::Arc;

pub mod gpu;

/// The needles' spring integration step, s.
const STEP: f32 = 1.0 / 240.0;
/// Angles are compared at this resolution, degrees, so a settled needle does not redraw its region.
const ANGLE_STEP: f32 = 0.01;
/// Ours: the values the live drive does not report (the water temperature without a digidash).
const COOLANT_C: f32 = 85.0;
const OIL_C: f32 = 95.0;
const OIL_BAR: f32 = 4.0;
const BATTERY_V: f32 = 13.8;
/// `UpShift` is the engine speed past the shift point (its lights' thresholds are about -1500 to 0); this keeps them off.
const NO_SHIFT: f32 = -1.0e4;
/// Ours: a `Damage` element's blink, ms, and the tyres' health (the live drive reports no wear).
const DAMAGE_BLINK: f32 = 500.0;
const TYRE_HEALTH: f32 = 1.0;
/// The degrees between a pie's wedge points, at most.
const PIE_STEP: f32 = 10.0;
const PSI_PER_BAR: f32 = 14.503_774;
const PA_PER_BAR: f32 = 1.0e5;

/// The live values the dials show.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Values {
    pub rpm: f32,
    pub kmh: f32,
    /// The drive's gear index: 0 reverse, 1 neutral, 2 first gear, and so on (as RBRHUD's `Gear`).
    pub gear: i32,
    /// The race clock, s.
    pub race_time: f32,
    pub throttle: f32,
    pub brake: f32,
    pub handbrake: f32,
    pub steering: f32,
    /// The digidash's water temperature, degrees C, and turbo pressure, Pa.
    pub water: Option<f32>,
    pub boost: Option<f32>,
    /// The odometer and the distance to go, m.
    pub odometer: Option<f32>,
    pub to_go: Option<f32>,
    /// The warning lamps lit (`hud::model::LAMPS`): hot, engine, battery, and the engine stalled.
    pub hot: bool,
    pub engine: bool,
    pub battery: bool,
    pub stalled: bool,
    /// The car's shift points and rev limit (the live drive's SHF1), the battery's voltage (DSH1) and the HUD's body
    /// damage, 0 to 1.
    pub shift: Option<ShiftPoints>,
    pub volts: Option<f32>,
    pub body: Option<f32>,
}

/// The engine speed `rpm` past the upshift point of gear `gear` (as [`Values::gear`]), and under its downshift point;
/// `None` for a gear without one (0).
fn past(shift: &ShiftPoints, gear: i32, rpm: f32) -> (Option<f32>, Option<f32>) {
    let point = |points: &[f32]| usize::try_from(gear).ok().and_then(|g| points.get(g).copied()).filter(|&p| p > 0.0);
    (point(&shift.up).map(|up| rpm - up), point(&shift.down).map(|down| down - rpm))
}

/// The value of telemetry `name` (RBRHUD's names); 0 for the ones not known. Without an odometer the distance is
/// `distance_km`.
fn telemetry(name: &str, values: &Values, distance_km: f32) -> f32 {
    let flag = |on: bool| f32::from(u8::from(on));
    let distance_km = values.odometer.map_or(distance_km, |m| m / 1000.0);
    let coolant = values.water.unwrap_or(COOLANT_C);
    let (up, down) = values.shift.map_or((None, None), |s| past(&s, values.gear, values.rpm));
    let limit = values.shift.map_or(0.0, |s| s.limit);
    let oil_bar = if values.stalled { 0.0 } else { OIL_BAR };
    let name = name.to_ascii_lowercase();
    if name.starts_with("damage") {
        return values.body.unwrap_or(0.0);
    }
    if name.starts_with("tirehealth") {
        return TYRE_HEALTH;
    }
    match name.as_str() {
        "rpm" => values.rpm,
        "speedkph" | "groundspeedkph" => values.kmh,
        "gear" => values.gear as f32,
        "racetime" => values.race_time,
        "racetimeminutes" => (values.race_time / 60.0).floor(),
        "racetimeseconds" => values.race_time % 60.0,
        "distancedriven" => distance_km,
        "distancedrivenmeters" => distance_km * 1000.0,
        "distancetofinish" => values.to_go.unwrap_or(0.0) / 1000.0,
        "distancetofinishmeters" => values.to_go.unwrap_or(0.0),
        "engineon" => flag(!values.stalled),
        "engineoff" => flag(values.stalled),
        "coolanttemperaturewarning" => flag(values.hot),
        "oilpressurewarning" => flag(values.engine),
        "batterylevelwarning" => flag(values.battery),
        "turbopressurebar" => values.boost.unwrap_or(0.0) / PA_PER_BAR,
        "turbopressurepsi" => values.boost.unwrap_or(0.0) / PA_PER_BAR * PSI_PER_BAR,
        "countdownended" => flag(values.race_time > 0.0),
        "coolanttemperaturec" | "radiatorcoolanttemperaturec" => coolant,
        "oiltemperaturec" => OIL_C,
        "oilpressurebar" => oil_bar,
        "oilpressurepsi" => oil_bar * PSI_PER_BAR,
        "batteryvoltage" => values.volts.unwrap_or(BATTERY_V),
        "handbrakeon" => flag(values.handbrake > 0.5),
        "inputthrottle" | "inputthrottleprocessed" => values.throttle,
        "inputbrake" => values.brake,
        "inputhandbrake" | "inputhandbrakeprocessed" => values.handbrake,
        "inputsteering" => values.steering,
        "upshift" => up.unwrap_or(NO_SHIFT),
        "downshift" => down.unwrap_or(NO_SHIFT),
        "rpmlimit" => limit,
        "rpmpercent" if limit > 0.0 => values.rpm / limit * 100.0,
        _ => 0.0,
    }
}

/// An atlas sprite drawn on the texture: texel `p` shows the sprite at `anchor + R(-angle) (p - origin) / scale`,
/// inside the `clip` share of the sprite (u0, v0, u1, v1).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Quad {
    sprite: [u32; 4],
    origin: Vec2,
    anchor: Vec2,
    /// Degrees, clockwise in texel space.
    angle: f32,
    scale: f32,
    /// `AARRGGBB`.
    colour: u32,
    clip: [f32; 4],
    /// A pie's wedge about the sprite's middle, from and to these degrees (clockwise from pointing left).
    wedge: Option<[f32; 2]>,
}

const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

impl Quad {
    fn at(sprite: [u32; 4], element: &Element) -> Quad {
        Quad {
            sprite,
            origin: Vec2::new(element.x, element.y),
            anchor: Vec2::ZERO,
            angle: element.rotation,
            scale: element.scale,
            colour: element.colour,
            clip: WHOLE,
            wedge: None,
        }
    }

    fn rotation(&self) -> Vec2 {
        Vec2::from_angle(self.angle.to_radians())
    }

    /// The texel rect `x0, y0, x1, y1` the quad covers.
    fn bounds(&self) -> [i64; 4] {
        let [_, _, w, h] = self.sprite.map(|v| v as f32);
        let corners = [Vec2::ZERO, Vec2::new(w, 0.0), Vec2::new(0.0, h), Vec2::new(w, h)]
            .map(|s| self.origin + self.rotation().rotate((s - self.anchor) * self.scale));
        let lo = corners.iter().fold(Vec2::MAX, |a, &c| a.min(c));
        let hi = corners.iter().fold(Vec2::MIN, |a, &c| a.max(c));
        [lo.x.floor() as i64, lo.y.floor() as i64, hi.x.ceil() as i64, hi.y.ceil() as i64]
    }

    /// The quad's corners in texels of the texture and in the `atlas` (normalised), clipped to [`Quad::clip`], and its
    /// colour as RGBA; a wedge's triangles, each a sprite with its last two corners the same.
    fn sprites(&self, atlas: &Rgba, out: &mut Vec<gpu::Sprite>) {
        let [x, y, w, h] = self.sprite.map(|v| v as f32);
        let size = Vec2::new(atlas.width as f32, atlas.height as f32);
        let [a, r, g, b] = self.colour.to_be_bytes().map(|c| f32::from(c) / 255.0);
        let sprite = |corners: [Vec2; 4]| gpu::Sprite {
            corners: corners.map(|s| (self.origin + self.rotation().rotate((s - self.anchor) * self.scale)).to_array()),
            uvs: corners.map(|s| ((Vec2::new(x, y) + s) / size).to_array()),
            colour: [r, g, b, a],
        };
        match self.wedge {
            None => {
                let [u0, v0, u1, v1] = self.clip;
                out.push(sprite([(u0, v0), (u1, v0), (u0, v1), (u1, v1)].map(|(u, v)| Vec2::new(u * w, v * h))));
            }
            Some(wedge) => {
                let points = wedge_points(wedge, Vec2::new(w, h));
                let middle = Vec2::new(w, h) / 2.0;
                out.extend(points.windows(2).map(|p| sprite([middle, p[0], p[1], p[1]])));
            }
        }
    }
}

/// The points on the edge of a `size` sprite, seen from its middle, from angle `from` to `to` (degrees clockwise from
/// pointing left, texel y down): at most [`PIE_STEP`] apart, and at each corner passed, so the triangles from the middle
/// cover the wedge exactly.
fn wedge_points([from, to]: [f32; 2], size: Vec2) -> Vec<Vec2> {
    let half = size / 2.0;
    let at = |degrees: f32| {
        let d = -Vec2::from_angle(degrees.to_radians());
        half + d * (half.x / d.x.abs().max(1e-6)).min(half.y / d.y.abs().max(1e-6))
    };
    let (lo, hi) = (from.min(to), from.max(to));
    let steps = ((hi - lo) / PIE_STEP).ceil().max(1.0) as u32;
    let mut angles: Vec<f32> = (0..=steps).map(|i| lo + (hi - lo) * i as f32 / steps as f32).collect();
    // The corners are at 180 m +- the diagonal's angle.
    let diagonal = half.y.atan2(half.x).to_degrees();
    let turns = ((lo / 180.0).floor() as i32 - 1)..=((hi / 180.0).ceil() as i32 + 1);
    let corners = turns.flat_map(|m| [180.0 * m as f32 - diagonal, 180.0 * m as f32 + diagonal]);
    angles.extend(corners.filter(|&c| c > lo && c < hi));
    angles.sort_by(f32::total_cmp);
    angles.into_iter().map(at).collect()
}

/// `value` through the `(value, output)` pairs, linear between them and clamped past the ends.
fn through(map: &[(f32, f32)], value: f32) -> f32 {
    let Some(&(first_value, first)) = map.first() else { return 0.0 };
    if value <= first_value {
        return first;
    }
    for pair in map.windows(2) {
        let ((v0, o0), (v1, o1)) = (pair[0], pair[1]);
        if value <= v1 {
            return if v1 == v0 { o1 } else { o0 + (o1 - o0) * (value - v0) / (v1 - v0) };
        }
    }
    map.last().map_or(0.0, |&(_, o)| o)
}

/// A number as the format `format` writes it.
fn formatted(format: &str, value: f32) -> String {
    let whole = value.trunc() as i64;
    let clock = |seconds: f32| {
        let s = seconds.max(0.0);
        let (h, m, sec) = ((s / 3600.0) as u32, (s / 60.0) as u32 % 60, s as u32 % 60);
        (h, m, sec, s.fract())
    };
    match format {
        "Decimal1" => format!("{value:.1}"),
        "Decimal2" => format!("{value:.2}"),
        "Percentage" => format!("{:.0}", value * 100.0),
        "TimeMinSec" => {
            let (h, m, s, _) = clock(value);
            format!("{}:{s:02}", h * 60 + m)
        }
        "TimeMinSecMs" => {
            let (h, m, s, f) = clock(value);
            format!("{}:{s:02}.{:03}", h * 60 + m, (f * 1000.0) as u32)
        }
        "TimeMinSecMsc" => {
            let (h, m, s, f) = clock(value);
            format!("{}:{s:02}.{:02}", h * 60 + m, (f * 100.0) as u32)
        }
        "TimeHoursMin" => {
            let (h, m, _, _) = clock(value);
            format!("{h}:{m:02}")
        }
        "TimeHoursMinSec" => {
            let (h, m, s, _) = clock(value);
            format!("{h}:{m:02}:{s:02}")
        }
        _ => match format.strip_prefix("PaddedSpace").or(format.strip_prefix("Padded")).map(str::parse::<usize>) {
            Some(Ok(width)) if format.starts_with("PaddedSpace") => format!("{whole:>width$}"),
            Some(Ok(width)) => format!("{whole:0width$}"),
            _ => whole.to_string(),
        },
    }
}

/// The glyph quads of `text` in `font`, justified about the element's position. A character the font lacks (a space)
/// advances by the width of its `0`.
fn text_quads(text: &str, font: &Font, justification: &str, element: &Element) -> Vec<Quad> {
    let advance = font.glyph('0').map_or(0.0, |g| g[2] as f32);
    let glyphs: Vec<(Option<[u32; 4]>, f32)> =
        text.chars().map(|c| font.glyph(c).map_or((None, advance), |g| (Some(g), g[2] as f32))).collect();
    let width: f32 = glyphs.iter().map(|(_, w)| w).sum();
    let mut left = match justification.to_ascii_lowercase().as_str() {
        "right" => -width,
        "center" => -width / 2.0,
        _ => 0.0,
    };
    let mut quads = Vec::new();
    for (glyph, w) in glyphs {
        if let Some(glyph) = glyph {
            quads.push(Quad { anchor: Vec2::new(-left, 0.0), ..Quad::at(glyph, element) });
        }
        left += w;
    }
    quads
}

/// A needle's angle and turn rate, degrees and degrees per second.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Needle {
    angle: f32,
    rate: f32,
}

impl Needle {
    /// Moves towards `target` for `dt` seconds as a spring of stiffness `k` and damping ratio `zeta`.
    fn follow(self, target: f32, dt: f32, k: f32, zeta: f32) -> Needle {
        let steps = (dt / STEP).ceil().max(1.0);
        let h = dt / steps;
        let mut needle = self;
        for _ in 0..steps as u32 {
            let accel = k * (target - needle.angle) - 2.0 * zeta * k.sqrt() * needle.rate;
            needle.rate += accel * h;
            needle.angle += needle.rate * h;
        }
        needle
    }
}

/// One config's drawing: the textures it replaces, its atlas, its regions, its needles' state and the quads drawn
/// (`None` before the first draw).
struct Face {
    keys: Vec<TextureRef>,
    config: Config,
    atlas: Atlas,
    sprites: Arc<Rgba>,
    size: (u32, u32),
    regions: Vec<[u32; 4]>,
    needles: Vec<Option<Needle>>,
    drawn: Option<Vec<Quad>>,
}

/// The car's working dials.
pub struct Dials {
    faces: Vec<Face>,
    time: f32,
    distance_km: f32,
    /// The gauge mode shown (1 on), and the car's last (its configs' highest element mode, at least 1).
    mode: u32,
    modes: u32,
}

/// Why a gauge config is not drawn.
#[derive(Debug)]
pub enum Skipped {
    /// No car texture of its `texture` name in the stage.
    NoTexture(String),
    /// A file it names does not read.
    Unreadable(String),
}

impl Dials {
    /// The dials of the car whose folder is `car_folder`, from the RSF install `root`: each config's car texture in
    /// `stage` becomes a texture the dials draw on. `None` when the car has no gauge or none of its configs can be
    /// drawn; the configs that cannot be are listed.
    pub fn load(root: &Path, car_folder: &str, stage: &mut Stage) -> (Option<Dials>, Vec<Skipped>) {
        let Some((folder, configs)) = gauges::variant(root, car_folder) else { return (None, Vec::new()) };
        let migrations = std::fs::read(root.join(gauges::MIGRATIONS)).unwrap_or_default();
        let migrations = String::from_utf8_lossy(&migrations);
        let mut skipped = Vec::new();
        let faces: Vec<Face> = configs
            .into_iter()
            .filter_map(|config| Face::load(&folder, config, &migrations, stage).map_err(|e| skipped.push(e)).ok())
            .collect();
        let modes = (faces.iter().flat_map(|f| &f.config.elements)).map(|e| e.mode).max().unwrap_or(0).max(1);
        ((!faces.is_empty()).then_some(Dials { faces, time: 0.0, distance_km: 0.0, mode: 1, modes }), skipped)
    }

    /// Shows the next gauge mode (after the last, the first): the next page of a digital dash.
    pub fn next_mode(&mut self) {
        self.mode = self.mode % self.modes + 1;
    }

    /// Draws `values` after `dt` seconds (`None`: the needles at their values at once) into the renderer's textures,
    /// where they changed.
    pub fn update(&mut self, values: &Values, dt: Option<f32>, renderer: &mut Renderer) {
        let step = dt.unwrap_or(0.0);
        self.time += step;
        self.distance_km += values.kmh / 3600.0 * step;
        let at = At { distance_km: self.distance_km, time: self.time, mode: self.mode };
        let draws: Vec<gpu::Draw> = (self.faces.iter_mut()).filter_map(|f| f.draw(values, at, dt)).collect();
        if !draws.is_empty() {
            renderer.draw_dials(draws);
        }
    }
}

impl Face {
    /// The face of `config` (in the gauge folder `folder`) on its textures in `stage`, with the rects `migrations` (the
    /// text of `texture_migrations.ini`) names for its texture copied from the car's own texture.
    fn load(folder: &Path, config: Config, migrations: &str, stage: &mut Stage) -> Result<Face, Skipped> {
        let keys: Vec<TextureRef> = stage
            .textures
            .iter()
            .filter(|(key, texture)| {
                let file = texture.request.rsplit(['\\', '/']).next().unwrap_or_default();
                key.set == TextureSet::Car && file.eq_ignore_ascii_case(&config.texture)
            })
            .map(|(&key, _)| key)
            .collect();
        let first = keys.first().ok_or_else(|| Skipped::NoTexture(config.texture.clone()))?;
        let file = |name: &str| {
            rbr_formats::cars::resolve(folder, name)
                .and_then(|path| std::fs::read(path).ok())
                .ok_or_else(|| Skipped::Unreadable(name.to_owned()))
        };
        let own = match stage.textures[first].image.as_ref() {
            Some(Image::Dds(bytes)) => Some(bytes),
            _ => None,
        };
        let mut remapped = match (&config.remapped, own) {
            (Some(name), _) => file(name)?,
            (None, Some(bytes)) => bytes.clone(),
            (None, None) => return Err(Skipped::Unreadable(config.texture.clone())),
        };
        let unreadable = || Skipped::Unreadable(config.remapped.clone().unwrap_or_default());
        let rects = gauges::migrations(migrations, &config.texture);
        let skin = own.filter(|_| !rects.is_empty() && config.remapped.is_some()).and_then(|b| dds::parse(b).ok());
        if let Some(own) = skin {
            let mut base = Rgba::decode(&dds::parse(&remapped).map_err(|_| unreadable())?);
            migrate(&mut base, &Rgba::decode(&own), &rects);
            remapped = dds::rgba8_file(base.width, base.height, &base.pixels);
        }
        // With the mip chain most packs' bases lack (139 of RSF's 150), as the car's own textures get it
        // (`crate::car::files`): one level aliases over the whole cockpit where the upscaler does not replace it.
        let remapped = dds::with_mips(&remapped).unwrap_or(remapped);
        let remapped = crate::upscale::with_block_mips(&remapped).unwrap_or(remapped);
        // The config places its elements in texels of the base as the pack has it.
        let base = dds::parse(&remapped).map_err(|_| unreadable())?;
        let texels = (base.width, base.height);
        // A pack's base larger than the car's own texture is a smooth resize of it (RBRHUD's 13 such: their finest
        // octave holds half to two thirds of the next one's detail), so its art is the car's size.
        let art = own
            .and_then(|b| dds::parse(b).ok())
            .map_or(texels, |own| (own.width.min(texels.0), own.height.min(texels.1)));
        // A mask that does not read leaves the face unlit, not undrawn.
        let glow = (config.emissive_mask.as_deref())
            .and_then(|name| file(name).ok())
            .filter(|bytes| dds::parse(bytes).is_ok());
        let atlas = Atlas::parse(&String::from_utf8_lossy(&file(&config.atlas)?));
        let sprites =
            read_image(&file(&atlas.texture)?, &atlas.texture).ok_or(Skipped::Unreadable(atlas.texture.clone()))?;
        for key in &keys {
            if let Some(texture) = stage.textures.get_mut(key) {
                texture.image = Some(Image::Dial { base: remapped.clone(), glow: glow.clone(), texels, art });
            }
        }
        let mut face = Face {
            keys,
            atlas,
            sprites: Arc::new(sprites),
            size: texels,
            regions: Vec::new(),
            needles: vec![None; config.elements.len()],
            drawn: None,
            config,
        };
        face.regions = face.dirty_rects();
        Ok(face)
    }

    /// The config's dirty regions clipped to the texture; the whole texture when it has none (ours: `i20_rally2`'s
    /// digital dash lists none).
    fn dirty_rects(&self) -> Vec<[u32; 4]> {
        let (w, h) = self.size;
        if self.config.regions.is_empty() {
            return vec![[0, 0, w, h]];
        }
        (self.config.regions.iter())
            .map(|r| [r.left.min(w), r.top.min(h), r.right.min(w), r.bottom.min(h)])
            .filter(|[x0, y0, x1, y1]| x0 < x1 && y0 < y1)
            .collect()
    }

    /// The quads the elements draw for `values` at `at`, with the needles moved on by `dt`.
    fn quads(&mut self, values: &Values, at: At, dt: Option<f32>) -> Vec<Quad> {
        let value = |data: &Option<String>| data.as_deref().map_or(0.0, |d| telemetry(d, values, at.distance_km));
        let blinks = |ms: f32| ((at.time / (ms.max(1.0) / 1000.0)) as u64).is_multiple_of(2);
        let mut quads = Vec::new();
        for (element, needle) in self.config.elements.iter().zip(&mut self.needles) {
            if element.mode != 0 && element.mode != at.mode {
                continue;
            }
            let v = value(&element.data);
            let mut colour = match &element.kind {
                Kind::Colour(map) => blend(map, v),
                _ => element.colour,
            };
            let mut shown = true;
            if let Kind::Damage(damage) = &element.kind {
                let worst = (damage.parts.iter()).map(|p| telemetry(p, values, at.distance_km)).fold(0.0, f32::max);
                let top = damage.levels.len().saturating_sub(1);
                if let Some(&(level, blink)) = damage.levels.get(((worst * top as f32).ceil() as usize).min(top)) {
                    colour = level;
                    shown &= !blink || blinks(DAMAGE_BLINK);
                }
            }
            for effect in self.config.effects.iter().filter(|e| element.effects.iter().any(|n| n == &e.name)) {
                let on = effect.condition.holds(value(&effect.data));
                for kind in &effect.kinds {
                    match kind.as_str() {
                        "Hide" => shown &= !on,
                        "Show" => shown &= on,
                        "Blink" if on => shown &= blinks(effect.blink.unwrap_or(500.0)),
                        "ColourChange" if on => colour = effect.new_colour.unwrap_or(colour),
                        _ => {}
                    }
                }
            }
            if !shown {
                continue;
            }
            let sprite = element.texture.as_deref().and_then(|t| self.atlas.sprite(t));
            let at = |sprite| Quad { colour, ..Quad::at(sprite, element) };
            match &element.kind {
                Kind::Static | Kind::Colour(_) | Kind::Damage(_) => quads.extend(sprite.map(at)),
                Kind::Simple if element.condition.holds(v) => quads.extend(sprite.map(at)),
                Kind::Pie(map) => {
                    let (from, to) = (through(map, f32::MIN), through(map, v));
                    let wedge = Some([from, (to / ANGLE_STEP).round() * ANGLE_STEP]);
                    quads.extend(sprite.filter(|_| to != from).map(|s| Quad { wedge, ..at(s) }));
                }
                Kind::Rotating(rotating) => {
                    let target = through(&rotating.map, v);
                    let moved = match (*needle, dt) {
                        (Some(state), Some(dt)) => {
                            let k = rotating.spring.unwrap_or(30.0) * rotating.speed.unwrap_or(10.0);
                            state.follow(target, dt, k.max(1.0), rotating.damping.unwrap_or(1.0))
                        }
                        _ => Needle { angle: target, rate: 0.0 },
                    };
                    *needle = Some(moved);
                    let angle = (moved.angle / ANGLE_STEP).round() * ANGLE_STEP;
                    quads.extend(sprite.map(|s| Quad { anchor: Vec2::from(rotating.pivot), angle, ..at(s) }));
                }
                Kind::Expanding(bar) => {
                    let f = through(&bar.map, v).clamp(0.0, 1.0);
                    let clip = match bar.direction.to_ascii_lowercase().as_str() {
                        "left" => [1.0 - f, 0.0, 1.0, 1.0],
                        "up" => [0.0, 1.0 - f, 1.0, 1.0],
                        "down" => [0.0, 0.0, 1.0, f],
                        _ => [0.0, 0.0, f, 1.0],
                    };
                    quads.extend(sprite.map(|s| Quad { clip, ..at(s) }));
                }
                Kind::Number(number) => {
                    let Some(font) = self.atlas.font(&number.font) else { continue };
                    let mut text = number.prefix.clone().unwrap_or_default();
                    if element.data.is_some() {
                        text.push_str(&formatted(&number.format, v));
                    }
                    text.push_str(number.suffix.as_deref().unwrap_or_default());
                    let element = Element { colour, ..element.clone() };
                    quads.extend(text_quads(&text, font, &number.justification, &element));
                }
                Kind::Simple | Kind::Other(_) => {}
            }
        }
        quads
    }

    /// The draw for `values`: the parts of the regions the quads that moved touch (the whole texture the first time),
    /// or `None` when nothing moved.
    fn draw(&mut self, values: &Values, at: At, dt: Option<f32>) -> Option<gpu::Draw> {
        let quads = self.quads(values, at, dt);
        let rects = match &self.drawn {
            None => vec![[0, 0, self.size.0, self.size.1]],
            Some(drawn) if *drawn == quads => return None,
            Some(drawn) => {
                let changed: Vec<[i64; 4]> = (quads.iter().filter(|q| !drawn.contains(q)))
                    .chain(drawn.iter().filter(|q| !quads.contains(q)))
                    .map(Quad::bounds)
                    .collect();
                touched(&self.regions, &changed)
            }
        };
        let mut sprites = Vec::with_capacity(quads.len());
        quads.iter().for_each(|q| q.sprites(&self.sprites, &mut sprites));
        self.drawn = Some(quads);
        Some(gpu::Draw { keys: self.keys.clone(), atlas: Arc::clone(&self.sprites), size: self.size, rects, sprites })
    }
}

/// Where the dials draw: the distance driven, km, the time, s, and the gauge mode shown.
#[derive(Debug, Clone, Copy)]
struct At {
    distance_km: f32,
    time: f32,
    mode: u32,
}

/// The colour (`AARRGGBB`) of `value` through the `(value, colour)` pairs, each channel linear between them and clamped
/// past the ends.
fn blend(map: &[(f32, u32)], value: f32) -> u32 {
    let channel = |shift: u32| {
        let pairs: Vec<(f32, f32)> = map.iter().map(|&(v, c)| (v, ((c >> shift) & 0xff) as f32)).collect();
        (through(&pairs, value).round().clamp(0.0, 255.0) as u32) << shift
    };
    [24, 16, 8, 0].into_iter().map(channel).fold(0, |a, c| a | c)
}

/// Per region, the part the `changed` texel rects touch.
fn touched(regions: &[[u32; 4]], changed: &[[i64; 4]]) -> Vec<[u32; 4]> {
    (regions.iter())
        .filter_map(|region| {
            let [rx0, ry0, rx1, ry1] = region.map(i64::from);
            (changed.iter())
                .map(|&[x0, y0, x1, y1]| [x0.max(rx0), y0.max(ry0), x1.min(rx1), y1.min(ry1)])
                .filter(|[x0, y0, x1, y1]| x0 < x1 && y0 < y1)
                .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
                .map(|rect| rect.map(|v| v as u32))
        })
        .collect()
}

/// Copies the `rects` (fractions of the texture, `x0, y0, x1, y1`) of `own` into `base`, each texel from the nearest of
/// `own`'s (the two may differ in size).
fn migrate(base: &mut Rgba, own: &Rgba, rects: &[[f32; 4]]) {
    let (w, h) = (base.width as f32, base.height as f32);
    for &[x0, y0, x1, y1] in rects {
        for y in (y0 * h) as u32..((y1 * h).ceil() as u32).min(base.height) {
            for x in (x0 * w) as u32..((x1 * w).ceil() as u32).min(base.width) {
                let texel = own.sample((x as f32 + 0.5) / w, (y as f32 + 0.5) / h);
                let i = ((y * base.width + x) * 4) as usize;
                base.pixels[i..i + 4].copy_from_slice(&texel);
            }
        }
    }
}

/// An atlas texture (`.png` or `.dds`) as RGBA.
fn read_image(bytes: &[u8], name: &str) -> Option<Rgba> {
    if name.to_ascii_lowercase().ends_with(".dds") {
        return dds::parse(bytes).ok().map(|d| Rgba::decode(&d));
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    pixels.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some(Rgba { width: info.width, height: info.height, pixels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_map_through_the_pairs() {
        let map = [(500.0, -35.0), (8000.0, 218.0)];
        assert_eq!(through(&map, 0.0), -35.0);
        assert_eq!(through(&map, 9000.0), 218.0);
        assert!((through(&map, 4250.0) - 91.5).abs() < 1e-4);
        assert_eq!(through(&[(40.0, 45.0), (80.0, 70.0), (140.0, 150.0)], 110.0), 110.0);
    }

    #[test]
    fn numbers_format_as_the_plugin_names() {
        assert_eq!(formatted("Decimal1", 12.34), "12.3");
        assert_eq!(formatted("Whole", 87.9), "87");
        assert_eq!(formatted("Padded3", 7.0), "007");
        assert_eq!(formatted("PaddedSpace3", 7.0), "  7");
        assert_eq!(formatted("TimeMinSec", 125.5), "2:05");
        assert_eq!(formatted("TimeMinSecMs", 65.25), "1:05.250");
    }

    /// A needle sprite pointing left from its pivot turns clockwise about it.
    #[test]
    fn a_needle_turns_about_its_pivot() {
        let atlas = Rgba { width: 8, height: 2, pixels: vec![255; 64] };
        // A 4x1 sprite with its pivot at its right end.
        let quad = |angle| Quad {
            sprite: [4, 1, 4, 1],
            origin: Vec2::new(8.0, 8.0),
            anchor: Vec2::new(4.0, 0.5),
            angle,
            scale: 1.0,
            colour: 0x80ff_0000,
            clip: WHOLE,
            wedge: None,
        };
        let sprite = |quad: Quad| {
            let mut out = Vec::new();
            quad.sprites(&atlas, &mut out);
            out[0]
        };
        let left = sprite(quad(0.0));
        assert_eq!(left.corners, [[4.0, 7.5], [8.0, 7.5], [4.0, 8.5], [8.0, 8.5]]);
        assert_eq!(left.uvs, [[0.5, 0.5], [1.0, 0.5], [0.5, 1.0], [1.0, 1.0]]);
        assert_eq!(left.colour, [1.0, 0.0, 0.0, 128.0 / 255.0]);
        // 90 degrees clockwise from pointing left is pointing up (texel y down).
        let tip = sprite(quad(90.0)).corners[0];
        assert!((tip[0] - 8.5).abs() < 1e-5 && (tip[1] - 4.0).abs() < 1e-5, "{tip:?}");
    }

    #[test]
    fn a_colour_blends_by_channel() {
        let map = [(-2000.0, 0xffff_ff00), (0.0, 0xffff_0000)];
        assert_eq!(blend(&map, -3000.0), 0xffff_ff00);
        assert_eq!(blend(&map, -1000.0), 0xffff_8000);
        assert_eq!(blend(&map, 10.0), 0xffff_0000);
    }

    /// A wedge's points run along the sprite's edge through the corners it passes.
    #[test]
    fn a_wedge_follows_the_edge() {
        let size = Vec2::new(4.0, 2.0);
        let points = wedge_points([0.0, 180.0], size);
        assert!((points[0] - Vec2::new(0.0, 1.0)).length() < 1e-5, "{points:?}");
        assert!((points.last().unwrap() - Vec2::new(4.0, 1.0)).length() < 1e-4, "{points:?}");
        // Through the top corners (0, 0) and (4, 0): pointing left then up then right is the top half.
        for corner in [Vec2::ZERO, Vec2::new(4.0, 0.0)] {
            assert!(points.iter().any(|p| (*p - corner).length() < 1e-4), "{corner} {points:?}");
        }
        assert!(points.iter().all(|p| p.y <= 1.0 + 1e-5));
    }

    #[test]
    fn upshift_is_past_the_gears_point() {
        let mut shift = ShiftPoints { limit: 7500.0, ..ShiftPoints::default() };
        shift.up[2] = 6000.0;
        shift.down[3] = 3640.0;
        let values = |gear, rpm| Values { gear, rpm, shift: Some(shift), ..Values::default() };
        assert_eq!(telemetry("UpShift", &values(2, 5500.0), 0.0), -500.0);
        assert_eq!(telemetry("UpShift", &values(7, 7000.0), 0.0), NO_SHIFT);
        assert_eq!(telemetry("DownShift", &values(3, 3000.0), 0.0), 640.0);
        assert_eq!(telemetry("RPMPercent", &values(2, 3750.0), 0.0), 50.0);
        assert_eq!(telemetry("UpShift", &Values { rpm: 9000.0, ..Values::default() }, 0.0), NO_SHIFT);
    }

    #[test]
    fn only_the_regions_a_change_touches_redraw() {
        let regions = [[0, 0, 100, 100], [200, 0, 300, 100]];
        assert_eq!(touched(&regions, &[[90, 10, 120, 20], [-5, 50, 10, 60]]), [[0, 10, 100, 60]]);
        assert!(touched(&regions, &[[120, 0, 180, 50]]).is_empty());
    }

    #[test]
    fn migrations_copy_the_car_texture_into_the_copy() {
        let mut base = Rgba { width: 4, height: 2, pixels: vec![0; 32] };
        let own = Rgba { width: 2, height: 1, pixels: vec![10, 10, 10, 255, 20, 20, 20, 255] };
        migrate(&mut base, &own, &[[0.5, 0.0, 1.0, 0.5]]);
        let texel = |x: usize, y: usize| base.pixels[(y * 4 + x) * 4];
        assert_eq!([texel(1, 0), texel(2, 0), texel(3, 0), texel(2, 1)], [0, 20, 20, 0]);
    }

    #[test]
    fn a_spring_settles_on_its_target() {
        let needle = Needle { angle: 0.0, rate: 0.0 }.follow(100.0, 2.0, 300.0, 1.5);
        assert!((needle.angle - 100.0).abs() < 0.1, "{needle:?}");
        let early = Needle { angle: 0.0, rate: 0.0 }.follow(100.0, 0.05, 300.0, 1.5);
        assert!(early.angle > 0.0 && early.angle < 100.0);
    }
}

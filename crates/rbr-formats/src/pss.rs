//! Reader for the particle definitions `Maps/PS_<Rally>.ini` and `Maps/PS_<Rally>_LOW.ini`, and the sprite list
//! `Misc/particletextures.ini` (in `Root/misc.rbz`). Format: rbr-recomp `notes/particles.md`; meanings:
//! `notes/renderer.md` "Particles".
//!
//! The files are "version3" token streams, read as `E83System::LoadFile` (0x5dffd0, `src/rbr/RBR_E83_D.cpp`) reads
//! them: `;` starts a comment that runs to the end of the line, `$` ends the text, and tokens are separated by white
//! space. The sections come in a fixed order, each a count followed by its records. Names are kept as written; the
//! game compares them without case (`_stricmp`), so the lookups here do too.

use std::fmt::{self, Display, Formatter};

/// Why a file did not parse.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The first token is not `version3`.
    Version(String),
    /// The text ended inside `section`.
    Truncated { section: &'static str },
    /// A token that should be a number is not one.
    Number { section: &'static str, token: String },
    /// An emitter class type or emitter force kind the loader does not know (it breaks into the debugger).
    Kind { section: &'static str, kind: i64 },
    /// Tokens after the last section.
    Trailing(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Version(t) => write!(f, "expected version3, found {t:?}"),
            Error::Truncated { section } => write!(f, "the text ends in the {section} section"),
            Error::Number { section, token } => write!(f, "{section}: {token:?} is not a number"),
            Error::Kind { section, kind } => write!(f, "{section}: unknown kind {kind}"),
            Error::Trailing(t) => write!(f, "tokens after the last section, from {t:?}"),
        }
    }
}

impl std::error::Error for Error {}

/// How a particle class is blended (`E83_ReadBlend`, 0x5dfa70).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// `BLEND_BLEND`: source alpha over the frame.
    Blend,
    /// `BLEND_ADD`: added to the frame.
    Add,
}

/// A particle class: the look of one particle over its life.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleClass {
    pub name: String,
    /// Index into `Misc/particletextures.ini`.
    pub sprite: u32,
    /// RGBA at birth, at `mid_color_at` (a fraction of the life) and at death.
    pub start_color: [f32; 4],
    pub mid_color_at: f32,
    pub mid_color: [f32; 4],
    pub end_color: [f32; 4],
    /// Width and height, metres, at birth, at `mid_size_at` and at death.
    pub start_size: [f32; 2],
    pub mid_size: [f32; 2],
    pub end_size: [f32; 2],
    pub mid_size_at: f32,
    pub size_variance: f32,
    /// Seconds.
    pub life: f32,
    pub weight: f32,
    pub trail_length: f32,
    pub trail_variance: f32,
    pub camera_attenuation: [f32; 2],
    pub blend: Blend,
    pub ground_push: i32,
    pub offset_width: f32,
    pub offset_height: f32,
    pub in_car_space: bool,
}

/// The shape and launch of an emitter class (`CEmitParam`, the type token).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Type 0: from a point (sparks).
    Point,
    /// Type 1: `speed, diversion` (smoke).
    Cone { speed: f32, diversion: f32 },
    /// Type 2: from a `base` rectangle, launched with `force` and `deviation`, sheared (tyre spray, weather).
    Box { base: [f32; 2], force: f32, deviation: f32, shear: [f32; 2] },
    /// Type 3: along a strip of the car (`CAR_STRIP`, `CAR_ENGINE_STRIP`) or a listed set of points.
    Strip { strip: String, points: Vec<[f32; 3]>, speed: f32, deviation: f32 },
    /// Type 4: two floats (not in the retail files).
    Kind4 { a: f32, b: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmitterClass {
    pub name: String,
    pub shape: Shape,
    pub frustum_only: bool,
    pub back_plane_kill: f32,
    pub force_variance: f32,
    /// The particle class names listed. The loader keeps the last one only.
    pub particle_classes: Vec<String>,
}

/// A force on an emitter's particles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Force {
    /// Kind 1: an acceleration, m/s², z up.
    Acceleration([f32; 3]),
    /// Kind 4: the weather's wind, by this factor.
    Wind(f32),
    /// Kind 5: friction.
    Friction(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    pub name: String,
    pub class: String,
    pub max_particles: u32,
    pub emit_rate: f32,
    pub camera_speed_modulation: f32,
    pub forces: Vec<Force>,
}

/// A 3D particle (the stones).
#[derive(Debug, Clone, PartialEq)]
pub struct Stone {
    pub life: f32,
    pub weight: f32,
    pub restitution: f32,
    pub friction: f32,
    pub radius: f32,
    pub inertia: f32,
    pub gravity_scale: f32,
    pub mesh: u32,
    pub color: [f32; 4],
    pub force_flags: u32,
    pub cull_flags: u32,
}

/// A tyre signal handler: how a wheel on `material` feeds `emitter`.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalHandler {
    pub material: u32,
    pub emitter: String,
    /// Each flow term is `(input + reduction) * modifier`, plus a random `±perturbation`.
    pub mass_flow: [f32; 3],
    pub smoke_flow: [f32; 3],
    /// The emission rate's smoothing: each step moves this fraction of the way to the new rate.
    pub equalization: f32,
    pub velocity_modifier: f32,
    pub velocity_perturbation: f32,
    /// Bit 0: add the car's velocity times `car_speed_factor`.
    pub flags: u32,
    pub car_speed_factor: f32,
    /// The speed term `(speed + reduction) * modifier`.
    pub speed_reduction: f32,
    pub speed_modifier: f32,
    /// The rate's ceiling.
    pub limit: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skidmark {
    pub material: u32,
    pub texture: u32,
    pub color: [f32; 3],
    pub size_modifier: f32,
    pub flush_threshold: f32,
    pub general_add: f32,
    pub rubber: [f32; 2],
    pub depth: [f32; 2],
    pub dirt: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct FakeSmoke {
    pub material: u32,
    pub dirt: [f32; 2],
    pub smoke: [f32; 2],
    pub speed: [f32; 2],
    pub speed_factor: f32,
    pub emitter: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Damping {
    pub material: u32,
    pub emitter: String,
    pub penetration: [f32; 2],
    pub velocity_modifier: f32,
}

/// One `PS_*.ini` file, its sections in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleFile {
    pub leaf_colors: [[f32; 4]; 4],
    pub air_friction: f32,
    pub classes: Vec<ParticleClass>,
    pub emitter_classes: Vec<EmitterClass>,
    pub emitters: Vec<Emitter>,
    pub max_live_stones: u32,
    pub stones: Vec<Stone>,
    pub handlers: Vec<SignalHandler>,
    pub skidmarks: Vec<Skidmark>,
    /// `(from, to)`: a surface `from` is handled as `to`.
    pub material_aliases: Vec<(u32, u32)>,
    pub skidmark_aliases: Vec<(u32, u32)>,
    pub fake_smoke: Vec<FakeSmoke>,
    pub damping: Vec<Damping>,
}

/// The emitters the game looks up by name after the file (`LoadFile`'s tail). Not every file defines each: only the
/// debris b and c, debris smoke, engine steam, broken exhaust, fire, fire smoke and pop emitters are in all 12; the
/// arctic files name their exhaust `g_exhaust_arctic_emitter`.
pub const NAMED_EMITTERS: [&str; 14] = [
    "g_exhaust_emitter",
    "g_exhaust_broken_emitter",
    "g_pop_emitter",
    "g_fire_emitter",
    "g_firesmoke_emitter",
    "g_engine_steam_emitter",
    "g_debris_a_emitter",
    "g_debris_b_emitter",
    "g_debris_c_emitter",
    "g_debris_smoke_emitter",
    "g_car_sparks_emitter",
    "g_carwater_emitter",
    "g_splash_emitter",
    "g_splash_smoke_emitter",
];

impl ParticleFile {
    pub fn class(&self, name: &str) -> Option<&ParticleClass> {
        self.classes.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn emitter_class(&self, name: &str) -> Option<&EmitterClass> {
        self.emitter_classes.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn emitter(&self, name: &str) -> Option<&Emitter> {
        self.emitters.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// The particle class an emitter's particles take: its class's last listed one, as the loader keeps.
    pub fn emitter_particles(&self, emitter: &Emitter) -> Option<&ParticleClass> {
        self.class(self.emitter_class(&emitter.class)?.particle_classes.last()?)
    }

    /// The surface id a wheel on `material` is handled as (the material aliases; unaliased ids stand for
    /// themselves).
    pub fn handled_material(&self, material: u32) -> u32 {
        self.material_aliases.iter().find(|&&(from, _)| from == material).map_or(material, |&(_, to)| to)
    }

    /// Every name a section refers to that the file does not define, as `"what: name"`.
    pub fn unresolved(&self) -> Vec<String> {
        let mut out = Vec::new();
        for class in &self.emitter_classes {
            for name in &class.particle_classes {
                if self.class(name).is_none() {
                    out.push(format!("particle class: {name}"));
                }
            }
        }
        for emitter in &self.emitters {
            if self.emitter_class(&emitter.class).is_none() {
                out.push(format!("emitter class: {}", emitter.class));
            }
        }
        let emitters = self.handlers.iter().map(|h| &h.emitter);
        let emitters = emitters.chain(self.fake_smoke.iter().map(|h| &h.emitter));
        for name in emitters.chain(self.damping.iter().map(|h| &h.emitter)) {
            if self.emitter(name).is_none() {
                out.push(format!("emitter: {name}"));
            }
        }
        out
    }
}

/// The tokens of `text` up to `$`, without comments.
fn tokens(text: &str) -> Vec<&str> {
    let text = text.split('$').next().unwrap_or("");
    text.lines().flat_map(|line| line.split(';').next().unwrap_or("").split_whitespace()).collect()
}

struct Reader<'a> {
    tokens: Vec<&'a str>,
    at: usize,
    section: &'static str,
}

impl<'a> Reader<'a> {
    fn word(&mut self) -> Result<&'a str, Error> {
        let token = self.tokens.get(self.at).ok_or(Error::Truncated { section: self.section })?;
        self.at += 1;
        Ok(token)
    }

    fn float(&mut self) -> Result<f32, Error> {
        let token = self.word()?;
        token.parse().map_err(|_| Error::Number { section: self.section, token: token.to_owned() })
    }

    fn floats<const N: usize>(&mut self) -> Result<[f32; N], Error> {
        let mut out = [0.0; N];
        for value in &mut out {
            *value = self.float()?;
        }
        Ok(out)
    }

    /// An integer as the loader's `atoi`-style reads take it: the leading sign and digits, so `1.0` (in community
    /// particle files) is 1. A token without leading digits is an error here (`atoi` would give 0).
    fn int(&mut self) -> Result<i64, Error> {
        let token = self.word()?;
        let digits = token.strip_prefix(['-', '+']).unwrap_or(token);
        let len = digits.bytes().take_while(u8::is_ascii_digit).count();
        let number = &token[..token.len() - digits.len() + len];
        number.parse().map_err(|_| Error::Number { section: self.section, token: token.to_owned() })
    }

    fn count(&mut self) -> Result<usize, Error> {
        let n = self.int()?;
        usize::try_from(n).map_err(|_| Error::Number { section: self.section, token: n.to_string() })
    }

    fn uint(&mut self) -> Result<u32, Error> {
        let n = self.int()?;
        u32::try_from(n).map_err(|_| Error::Number { section: self.section, token: n.to_string() })
    }

    fn name(&mut self) -> Result<String, Error> {
        self.word().map(str::to_owned)
    }

    fn list<T>(
        &mut self,
        section: &'static str,
        mut one: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        self.section = section;
        let n = self.count()?;
        (0..n).map(|_| one(self)).collect()
    }
}

/// Parses a whole `PS_*.ini` text.
pub fn parse(text: &str) -> Result<ParticleFile, Error> {
    let mut r = Reader { tokens: tokens(text), at: 0, section: "version" };
    let version = r.word()?;
    if version != "version3" {
        return Err(Error::Version(version.to_owned()));
    }
    r.section = "general settings";
    let leaf_colors = [r.floats()?, r.floats()?, r.floats()?, r.floats()?];
    let air_friction = r.float()?;
    let classes = r.list("particle classes", |r| {
        Ok(ParticleClass {
            name: r.name()?,
            sprite: r.uint()?,
            start_color: r.floats()?,
            mid_color_at: r.float()?,
            mid_color: r.floats()?,
            end_color: r.floats()?,
            start_size: r.floats()?,
            mid_size: r.floats()?,
            end_size: r.floats()?,
            mid_size_at: r.float()?,
            size_variance: r.float()?,
            life: r.float()?,
            weight: r.float()?,
            trail_length: r.float()?,
            trail_variance: r.float()?,
            camera_attenuation: r.floats()?,
            blend: match r.word()? {
                b if b.eq_ignore_ascii_case("BLEND_ADD") => Blend::Add,
                b if b.eq_ignore_ascii_case("BLEND_BLEND") => Blend::Blend,
                b => return Err(Error::Number { section: "blend mode", token: b.to_owned() }),
            },
            ground_push: r.int()? as i32,
            offset_width: r.float()?,
            offset_height: r.float()?,
            in_car_space: r.int()? != 0,
        })
    })?;
    let emitter_classes = r.list("emitter classes", |r| {
        let name = r.name()?;
        let shape = match r.int()? {
            0 => Shape::Point,
            1 => Shape::Cone { speed: r.float()?, diversion: r.float()? },
            2 => Shape::Box { base: r.floats()?, force: r.float()?, deviation: r.float()?, shear: r.floats()? },
            3 => {
                let strip = r.name()?;
                let [speed, deviation] = r.floats()?;
                let builtin = ["CAR_STRIP", "CAR_ENGINE_STRIP"].iter().any(|s| strip.eq_ignore_ascii_case(s));
                let points = if builtin {
                    Vec::new()
                } else {
                    let n = r.count()?;
                    (0..n).map(|_| r.floats()).collect::<Result<_, _>>()?
                };
                Shape::Strip { strip, points, speed, deviation }
            }
            4 => Shape::Kind4 { a: r.float()?, b: r.float()? },
            kind => return Err(Error::Kind { section: "emitter classes", kind }),
        };
        let frustum_only = r.int()? != 0;
        let back_plane_kill = r.float()?;
        let force_variance = r.float()?;
        let n = r.count()?;
        let particle_classes = (0..n).map(|_| r.name()).collect::<Result<_, _>>()?;
        Ok(EmitterClass { name, shape, frustum_only, back_plane_kill, force_variance, particle_classes })
    })?;
    let emitters = r.list("emitters", |r| {
        let name = r.name()?;
        let class = r.name()?;
        let max_particles = r.uint()?;
        let emit_rate = r.float()?;
        let camera_speed_modulation = r.float()?;
        let mut forces = Vec::new();
        for _ in 0..4 {
            match r.int()? {
                0 => {}
                1 => forces.push(Force::Acceleration(r.floats()?)),
                4 => forces.push(Force::Wind(r.float()?)),
                5 => forces.push(Force::Friction(r.float()?)),
                kind => return Err(Error::Kind { section: "emitters", kind }),
            }
        }
        Ok(Emitter { name, class, max_particles, emit_rate, camera_speed_modulation, forces })
    })?;
    r.section = "more general settings";
    let max_live_stones = r.uint()?;
    let stones = r.list("3d particles", |r| {
        Ok(Stone {
            life: r.float()?,
            weight: r.float()?,
            restitution: r.float()?,
            friction: r.float()?,
            radius: r.float()?,
            inertia: r.float()?,
            gravity_scale: r.float()?,
            mesh: r.uint()?,
            color: r.floats()?,
            force_flags: r.uint()?,
            cull_flags: r.uint()?,
        })
    })?;
    let handlers = r.list("signal handlers", |r| {
        Ok(SignalHandler {
            material: r.uint()?,
            emitter: r.name()?,
            mass_flow: r.floats()?,
            smoke_flow: r.floats()?,
            equalization: r.float()?,
            velocity_modifier: r.float()?,
            velocity_perturbation: r.float()?,
            flags: r.uint()?,
            car_speed_factor: r.float()?,
            speed_reduction: r.float()?,
            speed_modifier: r.float()?,
            limit: r.float()?,
        })
    })?;
    let skidmarks = r.list("skidmarks", |r| {
        Ok(Skidmark {
            material: r.uint()?,
            texture: r.uint()?,
            color: r.floats()?,
            size_modifier: r.float()?,
            flush_threshold: r.float()?,
            general_add: r.float()?,
            rubber: r.floats()?,
            depth: r.floats()?,
            dirt: r.floats()?,
        })
    })?;
    let material_aliases = r.list("material aliases", |r| Ok((r.uint()?, r.uint()?)))?;
    let skidmark_aliases = r.list("skidmark aliases", |r| Ok((r.uint()?, r.uint()?)))?;
    let fake_smoke = r.list("fake smoke signal handlers", |r| {
        Ok(FakeSmoke {
            material: r.uint()?,
            dirt: r.floats()?,
            smoke: r.floats()?,
            speed: r.floats()?,
            speed_factor: r.float()?,
            emitter: r.name()?,
        })
    })?;
    let damping = r.list("damping signal handlers", |r| {
        Ok(Damping { material: r.uint()?, emitter: r.name()?, penetration: r.floats()?, velocity_modifier: r.float()? })
    })?;
    if let Some(t) = r.tokens.get(r.at) {
        return Err(Error::Trailing((*t).to_owned()));
    }
    Ok(ParticleFile {
        leaf_colors,
        air_friction,
        classes,
        emitter_classes,
        emitters,
        max_live_stones,
        stones,
        handlers,
        skidmarks,
        material_aliases,
        skidmark_aliases,
        fake_smoke,
        damping,
    })
}

/// Parses `Misc/particletextures.ini`: a count, then that many texture paths (`Textures\br1_pebble.tga`). The
/// particle classes' sprite ids index this list.
pub fn parse_textures(text: &str) -> Result<Vec<String>, Error> {
    let mut r = Reader { tokens: tokens(text), at: 0, section: "particle textures" };
    let n = r.count()?;
    let names = (0..n).map(|_| r.name()).collect::<Result<Vec<_>, _>>()?;
    if let Some(t) = r.tokens.get(r.at) {
        return Err(Error::Trailing((*t).to_owned()));
    }
    Ok(names)
}

/// The sprite's entry name in `Textures/particles.dat`: `Textures\br1_pebble.tga` is `br1_pebble.dds`.
pub fn sprite_file(texture: &str) -> String {
    let base = texture.rsplit(['\\', '/']).next().unwrap_or(texture);
    let stem = base.rsplit_once('.').map_or(base, |(stem, _)| stem);
    format!("{stem}.dds")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; header\r\nversion3\r\n1 0 0 1 ;leaf 1\n0 1 0 1\n0 0 1 1\n1 1 1 1\n10 ;air friction\n\
        1\nspray_pc\n13\n0.4 0.4 0.4 1\n0.6\n0.65 0.65 0.65 1\n0.65 0.65 0.65 1\n0.025 0.025\n0.025 0.025\n0.025 0.025\n\
        0\n1\n2\n0.41\n0\n0\n0\n0\nBLEND_BLEND\n0\n0\n0.99\n0\n\
        1\nspray_ec\n2\n0.2 0.2\n-3 0.3\n0 0\n0\n0\n0\n1\nspray_pc\n\
        1\nspray_em\nspray_ec\n50\n0\n0\n1\n0 0 -9.81\n5\n0.01\n0\n0\n\
        10\n0\n\
        1\n17\nspray_em\n0 5 0\n0 0 0\n0.5\n0 0\n1\n0\n-10 1\n8\n\
        0\n2\n2 18\n6 18\n0\n0\n0\n$ ignored";

    #[test]
    fn sample_parses() {
        let file = parse(SAMPLE).unwrap();
        assert_eq!(file.air_friction, 10.0);
        assert_eq!(file.classes[0].sprite, 13);
        assert_eq!(file.classes[0].offset_height, 0.99);
        assert_eq!(
            file.emitter_classes[0].shape,
            Shape::Box { base: [0.2, 0.2], force: -3.0, deviation: 0.3, shear: [0.0, 0.0] }
        );
        assert_eq!(file.emitters[0].forces, vec![Force::Acceleration([0.0, 0.0, -9.81]), Force::Friction(0.01)]);
        assert_eq!(file.handlers[0].mass_flow, [0.0, 5.0, 0.0]);
        assert_eq!(file.handlers[0].limit, 8.0);
        assert_eq!(file.handled_material(6), 18);
        assert_eq!(file.handled_material(17), 17);
        assert_eq!(file.emitter_particles(&file.emitters[0]).unwrap().name, "spray_pc");
        assert!(file.unresolved().is_empty());
    }

    #[test]
    fn truncated_and_bad_version_fail() {
        assert_eq!(parse("version2"), Err(Error::Version("version2".into())));
        assert!(matches!(parse("version3 1 0 0 1"), Err(Error::Truncated { .. })));
    }

    #[test]
    fn numbers_kinds_and_trailing_fail_with_their_section() {
        let bad = SAMPLE.replacen("10 ;air friction", "x ;air friction", 1);
        assert_eq!(parse(&bad), Err(Error::Number { section: "general settings", token: "x".into() }));
        // An integer takes the leading digits, like the loader's `atoi`-style reads (`1.0` in community files).
        let lenient = SAMPLE.replacen("\n13\n", "\n13.5\n", 1);
        assert_eq!(parse(&lenient).unwrap().classes[0].sprite, 13);
        // One emitter class of an unknown shape (16 leaf floats, the air friction, no classes, then it).
        let one = format!("version3\n{}0\n0\n1\ne\n9\n", "0 ".repeat(16));
        assert_eq!(parse(&one), Err(Error::Kind { section: "emitter classes", kind: 9 }));
        let trailing = SAMPLE.replacen("$ ignored", "extra\n$ ignored", 1);
        assert_eq!(parse(&trailing), Err(Error::Trailing("extra".into())));
    }

    #[test]
    fn texture_list_and_sprite_names() {
        let names = parse_textures("; n\n2\nTextures\\a.tga ;0\nTextures\\b_c.tga\n;\n$").unwrap();
        assert_eq!(names, ["Textures\\a.tga", "Textures\\b_c.tga"]);
        assert_eq!(sprite_file(&names[1]), "b_c.dds");
    }
}

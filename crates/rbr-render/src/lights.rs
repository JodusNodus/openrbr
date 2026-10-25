//! The car's lights: which lamps are lit, the car parts that show each state, and the flares.
//!
//! The game's rules are in `notes/renderer.md` "Lights". `NPhysics::CCarLightSystem` switches the lamps by kind: the front and
//! tail lights are on, the brake lights follow the brake pedal (above [`BRAKE_ON`]) and the reverse lights the gear.
//! `CHH_Car::RenderLights` draws an additive sprite at each lamp with a `Light` section in `<car>_settings.ini`.
//! In live play the lamps are the game's own ([`Lamps`], the standalone's PEV1 lights event): each kind's switch (the
//! VCU sets the brake lights from the pedal that brakes, swapped pedals included, and the reverse lights from the
//! gear), the master word the engine's electrical damage clears, and each lamp's works and glass flags, which the
//! damage handler clears when a lamp is hit. `lightglass_*` parts show while their lamp's glass is whole. A recorded
//! drive has none of that: its brake lights follow the recorded brake word (the pedal swap unknown) and its reverse
//! lights stay off (the recordings hold no gear). Ours: the flare's shape is its texture's, and its tint is the
//! record's colour; only the headlights draw one, and every lit lamp's part glows.

use crate::camera::Lens;
use crate::stage::{Address, Anchor, Draw, DrawState, Layer, Lod, Mesh, TextureRef, Vertex};
use bytemuck::Zeroable;
use glam::Vec3;
use rbr_formats::car_parts;
use rbr_formats::ini::Section;

/// The brake pedal value above which the brake lights are on (`CCarLightSystem`: the value sent to the drive's brake).
pub const BRAKE_ON: f32 = 0.1;
/// Brightness of a flare facing the eye head on, from the game's alpha of 114.75 out of 255 (0x101b4398, 0.45).
const PEAK: f32 = 0.45;
/// How far a flare sits from its lamp toward the eye, metres (`CHH_Car::RenderLights`, 0x101b41e8).
const TOWARD_EYE: f32 = 0.2;
/// Triangles of the flare's quad: its corners in the order top left, top right, bottom left, bottom right.
const INDICES: [u32; 6] = [0, 2, 1, 1, 2, 3];

/// A lamp kind, numbered as `CCarLightSystem` numbers them. `<car>_settings.ini`'s `Id` is `number * 4 + lamp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Front,
    Tail,
    Brake,
    Reverse,
}

impl Kind {
    /// `CCarLightSystem`'s number of the kind.
    pub fn number(self) -> u32 {
        match self {
            Kind::Front => 1,
            Kind::Tail => 3,
            Kind::Brake => 4,
            Kind::Reverse => 5,
        }
    }

    fn from_number(number: u32) -> Option<Kind> {
        match number {
            1 => Some(Kind::Front),
            3 => Some(Kind::Tail),
            4 => Some(Kind::Brake),
            5 => Some(Kind::Reverse),
            _ => None,
        }
    }

    /// The kind and lamp of a switch box (`SwitchBox` in `<car>.ini`), and whether it is the lamp's glass, as
    /// `CHH_World::OnCarLight` names them: a prefix (`light`, `brake`, `reverse`, or `lightglass` for the glass) and a
    /// side (`_lf`, `_rf` for the front lights' lamps 0 and 1, `_lb`, `_rb`, `_m` for lamps 0 to 2 of the others).
    /// `light_lf` and `light_rf` are front lights, `light_lb` and `light_rb` tail lights. Other boxes are not lamps.
    fn of_box(name: &str) -> Option<(Kind, u32, bool)> {
        let (prefix, side) = name.split_once('_')?;
        let rear = |kind| match side {
            "lb" => Some((kind, 0)),
            "rb" => Some((kind, 1)),
            "m" => Some((kind, 2)),
            _ => None,
        };
        let (kind, lamp) = match (prefix, side) {
            ("light" | "lightglass", "lf") => (Kind::Front, 0),
            ("light" | "lightglass", "rf") => (Kind::Front, 1),
            ("light" | "lightglass", _) => rear(Kind::Tail)?,
            ("brake", _) => rear(Kind::Brake)?,
            ("reverse", _) => rear(Kind::Reverse)?,
            _ => return None,
        };
        Some((kind, lamp, prefix == "lightglass"))
    }

    /// The switch box `CHH_World::OnCarLight` switches for lamp `lamp`: the kind's prefix and the side of the lamp
    /// (`_lf`, `_rf`, `_lb` for the front lights' lamps 0 to 2, `_lb`, `_rb`, `_m` for the others'). `None` for lamp 3,
    /// which it ignores.
    fn box_name(self, lamp: u32) -> Option<String> {
        const SIDES: [&str; 5] = ["_lf", "_rf", "_lb", "_rb", "_m"];
        let (prefix, side) = match self {
            Kind::Front => ("light", lamp),
            Kind::Tail => ("light", lamp + 2),
            Kind::Brake => ("brake", lamp + 2),
            Kind::Reverse => ("reverse", lamp + 2),
        };
        (lamp < 3).then(|| format!("{prefix}{}", SIDES[side as usize]))
    }
}

/// The game's lamps after a frame (`rbr_recording::particles::Event::Lights`): what `CCarLightSystem::Update`
/// (0x4f2150) sends per lamp. A lamp is lit while its kind's switch times the master word is not zero and the lamp
/// works (`EmitLights` 0x4f20c0); its glass shows while whole.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lamps {
    pub switches: [u32; 6],
    pub master: u32,
    /// Per kind, the lamps' works flags (bits 0 to 3) and glass flags (bits 8 to 11).
    pub lamps: [u32; 6],
}

impl Lamps {
    /// Whether lamp `lamp` of `kind` is lit.
    pub fn lit(&self, kind: Kind, lamp: u32) -> bool {
        let k = kind.number() as usize - 1;
        self.switches[k].wrapping_mul(self.master) != 0 && self.lamps[k] >> lamp & 1 != 0
    }

    /// Whether lamp `lamp` of `kind` has its glass.
    pub fn glass(&self, kind: Kind, lamp: u32) -> bool {
        self.lamps[kind.number() as usize - 1] >> (8 + lamp) & 1 != 0
    }
}

/// What the lamps follow in one frame: the game's lamps when the live process sends them, else the brake pedal value
/// and whether the gear is reverse.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Inputs {
    pub brake: f32,
    pub reverse: bool,
    pub game: Option<Lamps>,
    /// The headlight key's setting (ours).
    pub headlights: Headlights,
}

/// The headlight key's settings, in the order it steps them (ours: 1.02 has the headlights always on): main beam,
/// dipped beam, and the headlights and tail lights off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Headlights {
    #[default]
    Main,
    Dipped,
    Off,
}

impl Headlights {
    pub fn next(self) -> Headlights {
        match self {
            Headlights::Main => Headlights::Dipped,
            Headlights::Dipped => Headlights::Off,
            Headlights::Off => Headlights::Main,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Headlights::Main => "main beam",
            Headlights::Dipped => "dipped",
            Headlights::Off => "off",
        }
    }
}

/// Whether lamp `lamp` of `kind` is lit with `inputs`.
pub fn lit(kind: Kind, lamp: u32, inputs: Inputs) -> bool {
    if inputs.headlights == Headlights::Off && matches!(kind, Kind::Front | Kind::Tail) {
        return false;
    }
    if let Some(game) = inputs.game {
        return game.lit(kind, lamp);
    }
    match kind {
        Kind::Front | Kind::Tail => true,
        Kind::Brake => inputs.brake > BRAKE_ON,
        Kind::Reverse => inputs.reverse,
    }
}

/// A car part that is drawn only while its lamp is in the state of its `Predicate` (`Switch = true`): the lit or the
/// unlit lamp mesh, or the lamp's glass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    pub kind: Kind,
    pub lamp: u32,
    /// `Predicate = on`: the lit lamp (the whole glass). `off`: the unlit one (the broken glass).
    pub lit: bool,
    /// A `lightglass_*` box: the part follows the lamp's glass, not its light.
    pub glass: bool,
}

impl Switch {
    /// The switch of a part that is a lamp's, or `None` for any other part.
    pub fn of_part(part: &Section) -> Option<Switch> {
        if !car_parts::is_switched(part) {
            return None;
        }
        let (kind, lamp, glass) = Kind::of_box(part.get("SwitchBox")?)?;
        let lit = match part.get("Predicate")? {
            "on" => true,
            "off" => false,
            _ => return None,
        };
        Some(Switch { kind, lamp, lit, glass })
    }

    /// Whether the part is drawn with `inputs`. The glass is whole unless the game says otherwise.
    pub fn shown(&self, inputs: Inputs) -> bool {
        let on = if self.glass {
            inputs.game.is_none_or(|game| game.glass(self.kind, self.lamp))
        } else {
            lit(self.kind, self.lamp, inputs)
        };
        on == self.lit
    }
}

/// A `Light` section of `<car>_settings.ini`: one lamp's flare. The defaults are the game's (`InitLights`).
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub kind: Kind,
    /// The lamp's index within its kind (`Id % 4`).
    pub lamp: u32,
    /// The lamp's `Point_light_*` section in `<car>.ini`, which gives its position and normal.
    pub part: String,
    /// Seconds to fade in when the lamp turns on, and to fade out when it turns off.
    pub fade_in: f32,
    pub fade_out: f32,
    /// Red, green and blue, 0 to 1.
    pub colour: [f32; 3],
    /// Texture file name, in the car's `Textures` folder.
    pub texture: String,
    /// Half the sprite's width at full facing, metres.
    pub size: f32,
    /// `None` when the lamp switches the flare. `Some(on)` when it never does, and the flare keeps its `Active` value:
    /// `OnCarLight` switches only the first record with the lamp's `Id`, and only when the car has the lamp's switch
    /// box (the extra lamps of the RSF cars' light pods share the headlights' `Id`).
    pub fixed: Option<bool>,
    /// Whether the record draws a flare: only the headlights' do (ours: the rear lamps' sprites read as blobs by day; their
    /// lit parts glow instead), and not a headlight that only lights the stage at night ([`flares`]).
    pub flare: bool,
}

/// A lamp's flare: its record, where it sits in the car's model, and the mesh it is drawn as.
#[derive(Debug, Clone, PartialEq)]
pub struct Flare {
    pub light: Light,
    /// The lamp's position and outward normal in the model (`Pos` and `Normal` of its `Point_light_*` section).
    pub position: Vec3,
    pub normal: Vec3,
    /// The quad the flare is drawn as. The playback rewrites its vertices each frame.
    pub mesh: Mesh,
}

/// The `Light` sections of `settings` whose lamp exists, each placed by its part in `parts`. `texture` gives the stage
/// texture of a file name in the car's `Textures` folder. As `InitLights` reads them: `Light0`, `Light1`, ... up to the
/// first one without an `Id` key.
///
/// A car with no headlight record gets one per `Point_light_lf` and `Point_light_rf` part, in colour `beam`, which
/// lights the stage at night and draws no flare (ours). RSF cars ship that way when their top-level files are the
/// modder's "lights off" option (the Focus WRC 2006, Fiesta WRC 2019, Delta HF 4WD, Escort Cosworth, BMW M3 E30 and
/// Lancer Evo IX samples): RSF's car-parts picker swaps in the "on" option's files, which we do not apply
/// (`notes/formats.md` "RSF cars").
pub fn flares(
    settings: &[Section],
    parts: &[Section],
    beam: [f32; 3],
    mut texture: impl FnMut(&str) -> TextureRef,
) -> Vec<Flare> {
    let boxes: Vec<&str> =
        parts.iter().filter(|p| car_parts::is_switched(p)).filter_map(|p| p.get("SwitchBox")).collect();
    let records: Vec<&Section> = (0..)
        .map_while(|n| car_parts::section(settings, &format!("Light{n}")).filter(|s| s.get("Id").is_some()))
        .collect();
    // The game's key of a record: an `Id` it can't read is 0, which no lamp has.
    let id = |section: &Section| section.get("Id").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let mut lights: Vec<Light> = records
        .iter()
        .enumerate()
        .filter_map(|(n, section)| {
            let mut light = light(section)?;
            let first = records[..n].iter().all(|other| id(other) != id(section));
            let switched = first && light.kind.box_name(light.lamp).is_some_and(|name| boxes.contains(&name.as_str()));
            light.fixed = (!switched).then(|| section.get("Active").is_some_and(|v| v.eq_ignore_ascii_case("true")));
            Some(light)
        })
        .collect();
    if lights.iter().all(|light| light.kind != Kind::Front) {
        lights.extend(["Point_light_lf", "Point_light_rf"].into_iter().zip(0..).map(|(part, lamp)| Light {
            kind: Kind::Front,
            lamp,
            part: part.to_owned(),
            fade_in: 1.0,
            fade_out: 1.0,
            colour: beam,
            texture: "Light.dds".to_owned(),
            size: 1.0,
            fixed: None,
            flare: false,
        }));
    }
    lights
        .into_iter()
        .filter_map(|light| {
            let point = car_parts::section(parts, &light.part)?;
            let position = vector(point, "Pos")?;
            let normal = vector(point, "Normal")?;
            let mesh = mesh(texture(&light.texture), &quad(position, Vec3::X, Vec3::Y, light.size, [0.0; 3]));
            Some(Flare { light, position, normal, mesh })
        })
        .collect()
}

/// The headlights' colour of every retail car's records (`Red`, `Green`, `Blue` 255, 255, 233).
pub const RETAIL_HEADLIGHT: [f32; 3] = [1.0, 1.0, 233.0 / 255.0];

/// The colour of the first headlight record of `settings`, else [`RETAIL_HEADLIGHT`]. `settings` is a car-parts
/// option's `_settings.ini`, whose `Light` sections are the ones it replaces (`Light2`, `Light12`), in file order.
pub fn headlight_colour(settings: &[Section]) -> [f32; 3] {
    settings
        .iter()
        .filter(|s| s.name.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("Light")))
        .filter_map(light)
        .find(|light| light.kind == Kind::Front)
        .map_or(RETAIL_HEADLIGHT, |light| light.colour)
}

/// A `Light` section as a record, or `None` when it has no `Id` of a lamp kind or no `Car Part`.
fn light(section: &Section) -> Option<Light> {
    let id: u32 = section.get("Id")?.parse().ok()?;
    let number = |key: &str, default: f32| section.get(key).and_then(|v| v.parse::<f32>().ok()).unwrap_or(default);
    let kind = Kind::from_number(id / 4)?;
    Some(Light {
        kind,
        lamp: id % 4,
        part: section.get("Car Part")?.to_owned(),
        fade_in: number("Fade In Time", 1.0),
        fade_out: number("Fade Out Time", 1.0),
        colour: ["Red", "Green", "Blue"].map(|key| number(key, 255.0) / 255.0),
        texture: section.get("Texture").unwrap_or("Light.dds").to_owned(),
        size: number("Size", 1.0),
        fixed: None,
        flare: kind == Kind::Front,
    })
}

/// Three numbers of a key, as `x y z`.
fn vector(section: &Section, key: &str) -> Option<Vec3> {
    let values: Vec<f32> = section.get(key)?.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
    match values[..] {
        [x, y, z] => Some(Vec3::new(x, y, z)),
        _ => None,
    }
}

/// The fade (0 to 1) of a flare `since` seconds after its lamp switched to `on`: it fades in while the lamp is on and out
/// while it is off, and `None` once an off lamp has faded out.
pub fn fade(light: &Light, on: bool, since: f32) -> Option<f32> {
    if on {
        Some(if light.fade_in > 0.0 { (since / light.fade_in).min(1.0) } else { 1.0 })
    } else if since < light.fade_out {
        Some(1.0 - since / light.fade_out)
    } else {
        None
    }
}

/// Seconds the lamp has been in its state at frame `index`. The look back stops after `horizon` seconds, which is
/// as long as any fade; a lamp in its state since the drive's first frame has been in it from before the start (the
/// countdown), `horizon`. `start(i)` is when frame `i` starts, `lit_at(i)` the lamp's state in frame `i`, and `now` the
/// time at the end of frame `index`.
pub fn since_switch(
    now: f64,
    index: usize,
    start: impl Fn(usize) -> f64,
    lit_at: impl Fn(usize) -> bool,
    horizon: f64,
) -> f32 {
    let state = lit_at(index);
    let mut first = index;
    while first > 0 && lit_at(first - 1) == state && now - start(first - 1) < horizon {
        first -= 1;
    }
    if first == 0 {
        return horizon as f32;
    }
    (now - start(first)) as f32
}

/// A lit lamp as a light on the stage at night (renderer addition, `Atmosphere::night`; the game has no night): where
/// it is, the way it points and the car's up (stage space), its colour times its intensity, and whether it is a
/// headlight's beam (`beam` in `lamps.wgsl`) rather than a lamp lighting what it faces, and then whether it is the main
/// beam rather than the dipped one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub colour: Vec3,
    pub beam: bool,
    pub main: bool,
}

/// Each kind's luminous intensity on its axis, candela (ours): the headlights' dipped beam's hot spot (an ECE dipped
/// beam's, which reaches about 60 m; the main beam's axis gets five times it) and the rear lamps' typical ECE values
/// (R7: tail 4 to 17, stop 60 to 185; R23: reversing 80 to 300).
const CANDELA: [f32; 4] = [20000.0, 8.0, 100.0, 150.0];
/// The lamps' light per candela (`lamps.wgsl`'s unit: a stage surface facing the lamp 1 m away along its axis gets
/// `HEADLIGHT_UNIT` for `HEADLIGHT_CD`, `night.wgsl`).
const UNITS_PER_CANDELA: f32 = 3000.0 / 20000.0;

/// `light`'s lamp at `position` with outward normal `normal`, on a car whose front and top point along `forward` and
/// `up` (stage space), at `fade`, a headlight on main beam when `main`. The headlights point along the car, the others
/// along their normal. Its colour is the record's at the kind's luminous intensity ([`CANDELA`]).
pub fn lamp(light: &Light, position: Vec3, normal: Vec3, forward: Vec3, up: Vec3, fade: f32, main: bool) -> Lamp {
    let (candela, beam) = match light.kind {
        Kind::Front => (CANDELA[0], true),
        Kind::Tail => (CANDELA[1], false),
        Kind::Brake => (CANDELA[2], false),
        Kind::Reverse => (CANDELA[3], false),
    };
    let forward = if beam { forward } else { normal };
    let colour = Vec3::from(light.colour);
    let luminance = colour.dot(Vec3::new(0.2126, 0.7152, 0.0722)).max(1e-3);
    let colour = colour / luminance * candela * UNITS_PER_CANDELA * fade;
    Lamp { position, forward, up, colour, beam, main: beam && main }
}

/// The flare's four vertices for a lamp at `position` with normal `normal` (world space), seen through `lens`, at
/// `fade`. `None` when the record draws no flare or the lamp faces away from the eye: its normal points away from the
/// camera's forward direction, or the camera is behind its face.
pub fn sprite(light: &Light, position: Vec3, normal: Vec3, lens: &Lens, fade: f32) -> Option<[Vertex; 4]> {
    let facing = -normal.dot(lens.forward);
    if !light.flare || facing <= 0.0 || normal.dot(position - lens.eye) >= 0.0 {
        return None;
    }
    let right = lens.up.cross(lens.forward).normalize();
    let up = lens.forward.cross(right).normalize();
    let intensity = PEAK * facing * fade;
    let centre = position - lens.forward * TOWARD_EYE;
    Some(quad(centre, right, up, light.size * facing, light.colour.map(|c| c * intensity)))
}

/// The four corners of a flare quad at `centre`, half-sized by `half` along `right` and `up`, tinted by `colour` (0 to 1).
fn quad(centre: Vec3, right: Vec3, up: Vec3, half: f32, colour: [f32; 3]) -> [Vertex; 4] {
    let tint = colour.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    let corner = |s: f32, t: f32, uv: [f32; 2]| Vertex {
        position: (centre + right * (s * half) + up * (t * half)).to_array(),
        color: [tint[0], tint[1], tint[2], 0],
        uv0: uv,
        ..Vertex::zeroed()
    };
    [
        corner(-1.0, 1.0, [0.0, 0.0]),
        corner(1.0, 1.0, [1.0, 0.0]),
        corner(-1.0, -1.0, [0.0, 1.0]),
        corner(1.0, -1.0, [1.0, 1.0]),
    ]
}

/// The mesh of a flare: one quad, drawn additively with its texture and no bounds (always in view).
fn mesh(texture: TextureRef, vertices: &[Vertex; 4]) -> Mesh {
    let state = DrawState {
        textures: [Some(texture), None],
        shadow: None,
        address: [Address::Clamp; 2],
        second_texture: false,
        alpha_ref: None,
        mip_bias: false,
        cull_back: false,
        blend: true,
        additive: true,
        depth_test: true,
        depth_write: false,
        shade: None,
        specular: None,
    };
    Mesh {
        layer: Layer::Blended,
        anchor: Anchor::World,
        bounds: None,
        cell: None,
        vertices: vertices.to_vec(),
        indices: INDICES.to_vec(),
        draws: vec![Draw { indices: 0..INDICES.len(), state, lod: Lod::Always }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::TextureSet;
    use rbr_formats::ini;

    fn sections(text: &str) -> Vec<Section> {
        ini::parse(text).expect("ini")
    }

    #[test]
    fn headlights_point_along_the_car_and_the_others_along_their_normal() {
        let settings = sections("[Light0]\r\nId = 4\r\nCar Part = Point_light_lf\r\n[Light1]\r\nId = 12\r\nCar Part = Point_light_lb\r\nRed = 255\r\nGreen = 0\r\nBlue = 0\r\n");
        let [head, tail] = [0, 1].map(|i| light(&settings[i]).expect("light"));
        let (forward, up, outward) = (Vec3::NEG_Z, Vec3::Y, Vec3::new(0.0, 0.0, 1.0));
        let lit = lamp(&head, Vec3::ZERO, Vec3::new(0.3, 0.0, -0.9), forward, up, 0.5, true);
        assert!(lit.beam && lit.main);
        assert_eq!((lit.forward, lit.colour), (forward, Vec3::splat(1500.0)));
        let back = lamp(&tail, Vec3::ZERO, outward, forward, up, 1.0, true);
        assert!(!back.beam && !back.main);
        assert_eq!(back.forward, outward);
        assert!((back.colour - Vec3::new(8.0 * 0.15 / 0.2126, 0.0, 0.0)).length() < 1e-3, "{:?}", back.colour);
    }

    #[test]
    fn brake_lights_follow_the_pedal_and_reverse_the_gear() {
        let pedal = |brake| Inputs { brake, ..Inputs::default() };
        assert!(!lit(Kind::Brake, 0, pedal(BRAKE_ON)));
        assert!(lit(Kind::Brake, 1, pedal(0.11)));
        assert!(lit(Kind::Reverse, 0, Inputs { reverse: true, ..Inputs::default() }));
        assert!(!lit(Kind::Reverse, 0, Inputs::default()));
        assert!(lit(Kind::Front, 0, Inputs::default()) && lit(Kind::Tail, 1, Inputs::default()));
    }

    #[test]
    fn the_games_lamps_switch_by_kind_master_and_lamp() {
        // As the standalone sent them on tanner-damage after the tree: front lamp 1 and its glass broken, brakes on.
        let lamps =
            Lamps { switches: [1, 0, 1, 1, 0, 0], master: 1, lamps: [0xd0d, 0xf0f, 0xf0f, 0xf0f, 0xf0f, 0xf0f] };
        let game = Inputs { brake: 0.0, reverse: true, game: Some(lamps), headlights: Headlights::Main };
        assert!(lit(Kind::Front, 0, game) && !lit(Kind::Front, 1, game));
        assert!(lit(Kind::Brake, 0, game), "the game's switch, not the brake word");
        assert!(!lit(Kind::Reverse, 0, game), "the game's switch, not the HUD gear");
        let glass = |lamp| Switch { kind: Kind::Front, lamp, lit: true, glass: true };
        assert!(glass(0).shown(game) && !glass(1).shown(game));
        assert!(glass(1).shown(Inputs::default()), "whole without the game's word");
        let dark = Inputs { game: Some(Lamps { master: 0, ..lamps }), ..game };
        assert!(!lit(Kind::Tail, 0, dark), "the electrical damage puts every lamp out");
        let dipped = Inputs { headlights: Headlights::Dipped, ..game };
        assert!(lit(Kind::Front, 0, dipped) && lit(Kind::Tail, 0, dipped));
        let off = Inputs { headlights: Headlights::Off, ..game };
        assert!(!lit(Kind::Front, 0, off) && !lit(Kind::Tail, 0, off), "the headlight key");
        assert!(lit(Kind::Brake, 0, off), "the brake lights stay on the pedal");
    }

    #[test]
    fn switch_parts_follow_their_box_and_predicate() {
        let parts = sections(
            "[e_light_lf_lit]\r\nSwitch = true\r\nSwitchBox = light_lf\r\nPredicate = on\r\n\
             [e_brake_lb]\r\nSwitch = true\r\nSwitchBox = brake_lb\r\nPredicate = off\r\n\
             [e_lightglass_lf]\r\nSwitch = true\r\nSwitchBox = lightglass_lf\r\nPredicate = on\r\n\
             [e_box_off]\r\nSwitch = true\r\nSwitchBox = horn\r\nPredicate = off\r\n",
        );
        let [lit_part, brake, glass, horn] = &parts[..] else {
            panic!("four sections");
        };
        assert_eq!(Switch::of_part(lit_part), Some(Switch { kind: Kind::Front, lamp: 0, lit: true, glass: false }));
        assert_eq!(Switch::of_part(brake), Some(Switch { kind: Kind::Brake, lamp: 0, lit: false, glass: false }));
        assert_eq!(Switch::of_part(glass), Some(Switch { kind: Kind::Front, lamp: 0, lit: true, glass: true }));
        assert_eq!(Switch::of_part(horn), None);
        let unlit_brake = Switch { kind: Kind::Brake, lamp: 0, lit: false, glass: false };
        assert!(unlit_brake.shown(Inputs::default()));
        assert!(!unlit_brake.shown(Inputs { brake: 0.5, ..Inputs::default() }));
        assert!(!Switch { kind: Kind::Front, lamp: 1, lit: false, glass: false }.shown(Inputs::default()));
    }

    #[test]
    fn settings_give_each_lamp_its_record_and_defaults() {
        let settings = sections(
            "[Light0]\r\nId = 16\r\nCar Part = Point_light_brake_lb\r\nFade In Time = 0.025\r\nFade Out Time = 0.05\r\n\
             Red = 255\r\nGreen = 0\r\nBlue = 0\r\nTexture = Light.dds\r\nSize = 0.25\r\n\
             [Light1]\r\nId = 4\r\nCar Part = Point_light_lf\r\n\
             [Light2]\r\nId = 8\r\nCar Part = Point_light_nothing\r\n",
        );
        let brake = light(&settings[0]).expect("brake");
        assert_eq!((brake.kind, brake.lamp), (Kind::Brake, 0));
        assert_eq!((brake.fade_in, brake.fade_out, brake.size), (0.025, 0.05, 0.25));
        assert_eq!(brake.colour, [1.0, 0.0, 0.0]);
        let front = light(&settings[1]).expect("front");
        assert_eq!(
            (front.kind, front.lamp, front.fade_in, front.fade_out, front.size),
            (Kind::Front, 0, 1.0, 1.0, 1.0)
        );
        assert_eq!((front.colour, front.texture.as_str()), ([1.0; 3], "Light.dds"));
        assert!(light(&settings[2]).is_none(), "kind 2 has no lamps");
    }

    #[test]
    fn flares_sit_at_their_part_and_draw_additively() {
        let parts = sections("[Point_light_lf]\r\nNormal = 0 0 -1\r\nPos = 1 2 3\r\n");
        let settings = sections(
            "[Light0]\r\nId = 4\r\nCar Part = Point_light_lf\r\n[Light1]\r\nId = 5\r\nCar Part = Point_light_gone\r\n",
        );
        let flares = flares(&settings, &parts, RETAIL_HEADLIGHT, |file| TextureRef {
            set: TextureSet::Car,
            index: file.len() as u32,
        });
        let [flare] = &flares[..] else {
            panic!("one flare");
        };
        assert_eq!((flare.position, flare.normal), (Vec3::new(1.0, 2.0, 3.0), Vec3::NEG_Z));
        assert_eq!(flare.mesh.bounds, None);
        assert!(flare.mesh.draws[0].state.additive && flare.mesh.draws[0].state.blend);
    }

    #[test]
    fn only_the_first_record_of_a_lamp_with_its_box_switches() {
        // As the RSF Evo IX's light pods: three records share the right headlight's `Id`; the reverse lamp has no box.
        let parts = sections(
            "[e_light_rf_lit]\r\nSwitch = true\r\nSwitchBox = light_rf\r\nPredicate = on\r\n\
             [e_reverse_lb]\r\nSwitch = false\r\nSwitchBox = reverse_lb\r\nPredicate = on\r\n\
             [Point_light_rf]\r\nPos = 0 0 0\r\nNormal = 0 0 -1\r\n[Point_light_bon1]\r\nPos = 0 1 0\r\nNormal = 0 0 -1\r\n\
             [Point_light_bon2]\r\nPos = 0 2 0\r\nNormal = 0 0 -1\r\n[Point_light_reverse_lb]\r\nPos = 0 0 1\r\nNormal = 0 0 1\r\n",
        );
        let settings = sections(
            "[Light0]\r\nId = 5\r\nCar Part = Point_light_rf\r\nActive = false\r\n\
             [Light1]\r\nId = 5\r\nCar Part = Point_light_bon1\r\nActive = true\r\n\
             [Light3]\r\nId = 5\r\nCar Part = Point_light_bon2\r\nActive = true\r\n\
             [Light2]\r\nId =\r\nCar Part =\r\n\
             [Light4]\r\nId = 20\r\nCar Part = Point_light_reverse_lb\r\n\
             [Light6]\r\nId = 16\r\nCar Part = Point_light_rf\r\n",
        );
        let flares = flares(&settings, &parts, RETAIL_HEADLIGHT, |_| TextureRef { set: TextureSet::Car, index: 0 });
        let fixed: Vec<(&str, Option<bool>)> = flares.iter().map(|f| (f.light.part.as_str(), f.light.fixed)).collect();
        assert_eq!(
            fixed,
            [
                ("Point_light_rf", None),
                ("Point_light_bon1", Some(true)),
                ("Point_light_bon2", Some(true)),
                ("Point_light_reverse_lb", Some(false)),
            ],
            "the empty Id is read and skipped, and the reading stops at the missing Light5"
        );
    }

    #[test]
    fn a_car_without_headlight_records_gets_beams_without_flares() {
        // As the RSF Focus WRC 2006 as installed (its "lights off" option): brake records only, both headlight points.
        let parts = sections(
            "[Point_light_lf]\r\nPos = -1 0 0\r\nNormal = 0 0 -1\r\n[Point_light_rf]\r\nPos = 1 0 0\r\nNormal = 0 0 -1\r\n\
             [Point_light_brake_lb]\r\nPos = 0 0 2\r\nNormal = 0 0 1\r\n",
        );
        let settings = sections("[Light0]\r\nId = 16\r\nCar Part = Point_light_brake_lb\r\nActive = false\r\n");
        let warm = [1.0, 0.8, 0.6];
        let flares = flares(&settings, &parts, warm, |_| TextureRef { set: TextureSet::Car, index: 0 });
        assert_eq!(flares[1].light.colour, warm);
        let beams: Vec<(Kind, u32, Option<bool>, bool)> =
            flares.iter().map(|f| (f.light.kind, f.light.lamp, f.light.fixed, f.light.flare)).collect();
        assert_eq!(
            beams,
            [(Kind::Brake, 0, Some(false), false), (Kind::Front, 0, None, false), (Kind::Front, 1, None, false)]
        );
        let lens = Lens { eye: Vec3::new(0.0, 0.0, -10.0), forward: Vec3::Z, up: Vec3::Y, fov: 1.0, near: 0.1 };
        assert!(sprite(&flares[1].light, Vec3::ZERO, Vec3::NEG_Z, &lens, 1.0).is_none(), "no flare, facing the eye");
    }

    #[test]
    fn the_headlight_colour_is_the_first_front_record_of_an_option() {
        // As the RSF Escort Cosworth's "lights on" option: tail records only, on the front points.
        let tails =
            sections("[Light6]\r\nId = 14\r\nCar Part = Point_light_lf\r\nRed = 255\r\nGreen = 255\r\nBlue = 100\r\n");
        assert_eq!(headlight_colour(&tails), RETAIL_HEADLIGHT);
        let on = sections(
            "[Light2]\r\nId = 16\r\nCar Part = Point_light_brake_lb\r\nRed = 255\r\nGreen = 0\r\nBlue = 0\r\n\
             [Light4]\r\nId = 4\r\nCar Part = Point_light_lf\r\nRed = 255\r\nGreen = 255\r\nBlue = 200\r\n",
        );
        assert_eq!(headlight_colour(&on), [1.0, 1.0, 200.0 / 255.0]);
    }

    #[test]
    fn fades_run_in_while_on_and_out_while_off() {
        let settings = sections("[Light0]\r\nId = 4\r\nCar Part = p\r\nFade In Time = 1\r\nFade Out Time = 0.5\r\n");
        let light = light(&settings[0]).expect("light");
        assert_eq!(fade(&light, true, 0.0), Some(0.0));
        assert_eq!(fade(&light, true, 0.25), Some(0.25));
        assert_eq!(fade(&light, true, 2.0), Some(1.0));
        assert_eq!(fade(&light, false, 0.25), Some(0.5));
        assert_eq!(fade(&light, false, 0.5), None);
    }

    #[test]
    fn since_switch_counts_back_to_the_last_change_within_the_horizon() {
        // Frames of 0.1 s; the lamp is lit from frame 2 on, and the end of frame 4 is 0.5 s.
        let starts = |i: usize| i as f64 * 0.1;
        let lit_at = |i: usize| i >= 2;
        assert!((since_switch(0.5, 4, starts, lit_at, 1.0) - 0.3).abs() < 1e-6);
        assert!((since_switch(0.5, 4, starts, lit_at, 0.1) - 0.1).abs() < 1e-6);
        // Unlit from the first frame: settled, not fading out from a light it never had.
        assert_eq!(since_switch(0.15, 1, starts, lit_at, 1.0), 1.0);
    }

    #[test]
    fn sprites_face_the_eye_and_vanish_facing_away() {
        let settings = sections("[Light0]\r\nId = 4\r\nCar Part = p\r\nSize = 0.5\r\n");
        let light = light(&settings[0]).expect("light");
        let lens = Lens { eye: Vec3::new(0.0, 0.0, -10.0), forward: Vec3::Z, up: Vec3::Y, fov: 1.0, near: 0.1 };
        let front = sprite(&light, Vec3::ZERO, Vec3::NEG_Z, &lens, 1.0).expect("facing the eye");
        // Facing 1: the colour is the game's 0.45 of white, 115 of 255, and the quad's top left is 0.5 m away along -x,
        // +y and 0.2 m toward the eye.
        assert_eq!(front[0].color[0], 115);
        assert!((Vec3::from(front[0].position) - Vec3::new(-0.5, 0.5, -0.2)).length() < 1e-5);
        assert!(sprite(&light, Vec3::ZERO, Vec3::Z, &lens, 1.0).is_none(), "facing away");
    }
}

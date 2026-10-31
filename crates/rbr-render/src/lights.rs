//! The car's lights: which lamps are lit, the car parts that show each state, and where each lamp sits.
//!
//! The game's rules are in `notes/renderer.md` "Lights". `NPhysics::CCarLightSystem` switches the lamps by kind: the front and
//! tail lights are on, the brake lights follow the brake pedal (above [`BRAKE_ON`]) and the reverse lights the gear.
//! `CHH_Car::RenderLights` draws an additive sprite at each lamp with a `Light` section in `<car>_settings.ini`.
//! In live play the lamps are the game's own ([`Lamps`], the standalone's PEV1 lights event): each kind's switch (the
//! VCU sets the brake lights from the pedal that brakes, swapped pedals included, and the reverse lights from the
//! gear), the master word the engine's electrical damage clears, and each lamp's works and glass flags, which the
//! damage handler clears when a lamp is hit. `lightglass_*` parts show while their lamp's glass is whole. A recorded
//! drive has none of that: its brake lights follow the recorded brake word (the pedal swap unknown) and its reverse
//! lights stay off (the recordings hold no gear). Ours: no sprite is drawn (user, 2026-10-31); every lit lamp's part
//! glows, and the records place the lamps' light on the stage ([`lamp`]).

use glam::Vec3;
use rbr_formats::car_parts;
use rbr_formats::ini::Section;

/// The brake pedal value above which the brake lights are on (`CCarLightSystem`: the value sent to the drive's brake).
pub const BRAKE_ON: f32 = 0.1;
/// The tail and brake lamps' colour: their records' (255, 0, 0 in 351 of the installed cars' 409, 2026-10-31).
const LAMP_RED: [f32; 3] = [1.0, 0.0, 0.0];
/// How bright a tail lamp's lit part glows by day against the other lamps' (ours): about 8 cd against a brake lamp's
/// 100 ([`CANDELA`]), it barely reads in sunlight, so braking shows. By night, and at dusk as the lamps light the stage,
/// it glows as the others do (all the lamps' parts outshine the dark there).
const TAIL_DAY: f32 = 0.3;

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
    /// The part takes its lamp by its name ([`Switch::of_part`]): the game never draws it lit, so its texture's colour
    /// need not be the lamp's (the GC8's brake lamps' is a white reflector).
    pub named: bool,
}

impl Switch {
    /// The switch of `part`, one of the car's `parts`, when it is a lamp's: by its switch box, or else by its name.
    /// Ours: many RSF cars switch their lamps' lit parts on the `light` or `brake` box, or with `Predicate = NotUsed`,
    /// so the game never draws them (the tail lamps of 43 installed cars, the headlamps of 42, 2026-10-31). A lamp's
    /// lit part, `e_light_lf_lit`, `e_light_lb_lit` and so on, that the game never draws follows its lamp, and so does
    /// its unlit pair `e_light_lb` when that one's box does not switch it. A lit part the game always draws stays (the
    /// GC8's `e_light_lb_lit` is its reverse lamp's reflector, which glowed white).
    pub fn of_part(part: &Section, parts: &[Section]) -> Option<Switch> {
        Switch::of_box(part).or_else(|| {
            let named = Switch::of_name(&part.name)?;
            let lit = if named.lit { Some(part) } else { car_parts::section(parts, &format!("{}_lit", part.name)) };
            lit.filter(|lit| Switch::of_box(lit).is_none() && !shown_at_start(lit)).map(|_| named)
        })
    }

    /// The switch a part's box and predicate give, or `None`.
    fn of_box(part: &Section) -> Option<Switch> {
        if !car_parts::is_switched(part) {
            return None;
        }
        let (kind, lamp, glass) = Kind::of_box(part.get("SwitchBox")?)?;
        let lit = match part.get("Predicate")? {
            "on" => true,
            "off" => false,
            _ => return None,
        };
        Some(Switch { kind, lamp, lit, glass, named: false })
    }

    /// The lamp a part is by its name: `e_light_lf` and `e_light_rf` the headlamps', `e_light_lb`, `e_light_rb` and
    /// `e_light_m` the tail lamps', with `brake_` or `reverse_` after `e_light_` the brake or reverse lamps', and lit
    /// with `_lit` after.
    fn of_name(name: &str) -> Option<Switch> {
        let name = name.to_ascii_lowercase();
        let (name, lit) = name.strip_suffix("_lit").map_or((name.as_str(), false), |name| (name, true));
        let lamp = name.strip_prefix("e_light_")?;
        let lamp_box = if lamp.starts_with("brake_") || lamp.starts_with("reverse_") {
            lamp.to_owned()
        } else {
            format!("light_{lamp}")
        };
        let (kind, lamp, _) = Kind::of_box(&lamp_box)?;
        Some(Switch { kind, lamp, lit, glass: false, named: true })
    }

    /// What a lit part of the lamp glows in, its texture times this colour (ours; `LAMP_GLOW` in `night.wgsl` over
    /// it), and the share of it kept by day: a tail or brake lamp's part taken by its name glows in the lamp's red
    /// ([`LAMP_RED`]), not its texture's colour, and a tail lamp keeps [`TAIL_DAY`].
    pub fn glow(&self) -> [f32; 4] {
        let [r, g, b] = match self.kind {
            Kind::Tail | Kind::Brake if self.named => LAMP_RED,
            _ => [1.0; 3],
        };
        [r, g, b, if self.kind == Kind::Tail { TAIL_DAY } else { 1.0 }]
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

/// Whether a part is shown at the stage start. The loader makes a part with `Switch = true` and a non-empty
/// `SwitchBox` and `Predicate` a `CGeomObjectSwitch` (`FUN_1001fc70`, `FUN_1001fde0`), hidden until its box is set to
/// its predicate (`FUN_10043090`). `CHH_World`'s vf14 (0x100690a0) sets every box whose name contains `light`, `brake`
/// or `reverse` to `off` (its table also lists `lightglass` and `brakedisc`, which those already match); other boxes
/// stay unset. So the unlit lights and brake discs (`Predicate = off`) are drawn and the lit ones are not.
pub fn shown_at_start(part: &Section) -> bool {
    let value = |key| part.get(key).filter(|v| !v.is_empty());
    match (car_parts::is_switched(part), value("SwitchBox"), value("Predicate")) {
        (true, Some(switch_box), Some(predicate)) => {
            ["light", "brake", "reverse"].iter().any(|n| switch_box.contains(n)) && predicate == "off"
        }
        _ => true,
    }
}

/// A `Light` section of `<car>_settings.ini`: one lamp. The defaults are the game's (`InitLights`).
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
    /// `None` when the lamp switches the record. `Some(on)` when it never does, and the record keeps its `Active`
    /// value: `OnCarLight` switches only the first record with the lamp's `Id`, and only when the car has the lamp's
    /// switch box (the extra lamps of the RSF cars' light pods share the headlights' `Id`).
    pub fixed: Option<bool>,
}

/// A lamp's record and where it sits in the car's model.
#[derive(Debug, Clone, PartialEq)]
pub struct LampPoint {
    pub light: Light,
    /// The lamp's position and outward normal in the model (`Pos` and `Normal` of its `Point_light_*` section).
    pub position: Vec3,
    pub normal: Vec3,
}

/// The `Light` sections of `settings` whose lamp exists, each placed by its part in `parts`. As `InitLights` reads
/// them: `Light0`, `Light1`, ... up to the first one without an `Id` key.
///
/// A car with no headlight record that ever lights gets one per `Point_light_lf` and `Point_light_rf` part, in colour
/// `beam`, which lights the stage at night (ours). RSF cars ship that way when their top-level files are the modder's
/// "lights off" option, with no headlight records (the Focus WRC 2006, Fiesta WRC 2019, Delta HF 4WD, Escort Cosworth,
/// BMW M3 E30 and Lancer Evo IX samples) or with ones that are `Active = false` and have no switch box (the Escort
/// Mk II): RSF's car-parts picker swaps in the "on" option's files, which we do not apply (`notes/formats.md` "RSF
/// cars").
pub fn lamp_points(settings: &[Section], parts: &[Section], beam: [f32; 3]) -> Vec<LampPoint> {
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
    if lights.iter().all(|light| light.kind != Kind::Front || light.fixed == Some(false)) {
        lights.extend(HEADLIGHT_POINTS.into_iter().zip(0..).map(|(part, lamp)| Light {
            kind: Kind::Front,
            lamp,
            part: part.to_owned(),
            fade_in: 1.0,
            fade_out: 1.0,
            colour: beam,
            fixed: None,
        }));
    }
    lights
        .into_iter()
        .filter_map(|light| {
            let point = car_parts::section(parts, &light.part)?;
            Some(LampPoint { position: vector(point, "Pos")?, normal: vector(point, "Normal")?, light })
        })
        .collect()
}

/// The headlights' parts in `<car>.ini`, lamps 0 and 1.
const HEADLIGHT_POINTS: [&str; 2] = ["Point_light_lf", "Point_light_rf"];

/// `parts` (a `<car>.ini`) with the headlights' points it lacks taken from `on`, its "lights on" option's (ours): a car
/// installed with its lights off can lack them as well as its headlight records, and then lit nothing at night (the
/// GR Yaris Rally2).
pub fn with_headlight_points(parts: &[Section], on: &[Section]) -> Vec<Section> {
    let lacking = HEADLIGHT_POINTS.into_iter().filter(|point| car_parts::section(parts, point).is_none());
    parts.iter().chain(lacking.filter_map(|point| car_parts::section(on, point))).cloned().collect()
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
        fixed: None,
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

/// The fade (0 to 1) of a lamp's light `since` seconds after it switched to `on`: it fades in while the lamp is on and out
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

#[cfg(test)]
mod tests {
    use super::*;
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
        let glass = |lamp| Switch { kind: Kind::Front, lamp, lit: true, glass: true, named: false };
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
        assert_eq!(
            Switch::of_part(lit_part, &parts),
            Some(Switch { kind: Kind::Front, lamp: 0, lit: true, glass: false, named: false })
        );
        assert_eq!(
            Switch::of_part(brake, &parts),
            Some(Switch { kind: Kind::Brake, lamp: 0, lit: false, glass: false, named: false })
        );
        assert_eq!(
            Switch::of_part(glass, &parts),
            Some(Switch { kind: Kind::Front, lamp: 0, lit: true, glass: true, named: false })
        );
        assert_eq!(Switch::of_part(horn, &parts), None);
        let unlit_brake = Switch { kind: Kind::Brake, lamp: 0, lit: false, glass: false, named: false };
        assert!(unlit_brake.shown(Inputs::default()));
        assert!(!unlit_brake.shown(Inputs { brake: 0.5, ..Inputs::default() }));
        assert!(!Switch { kind: Kind::Front, lamp: 1, lit: false, glass: false, named: false }.shown(Inputs::default()));
    }

    #[test]
    fn lamp_parts_the_game_never_lights_follow_their_lamp_by_name() {
        let parts = sections(
            "[e_light_lb]\r\nSwitch = false\r\nSwitchBox = light\r\nPredicate = off\r\n\
             [e_light_lb_lit]\r\nSwitch = true\r\nSwitchBox = light_lb\r\nPredicate = NotUsed\r\n\
             [e_light_brake_rb_lit]\r\nSwitch = true\r\nSwitchBox = brake\r\nPredicate = on\r\n\
             [e_light_reverse_lb]\r\nSwitch = false\r\nSwitchBox = reverse_lb\r\nPredicate = off\r\n\
             [e_light_reverse_lb_lit]\r\nSwitch = true\r\nSwitchBox = reverse_lb\r\nPredicate = on\r\n\
             [e_light_rb]\r\nSwitch = true\r\nSwitchBox = light\r\nPredicate = NotUsed\r\n\
             [e_light_rb_lit]\r\nSwitch = false\r\nSwitchBox = light\r\nPredicate = NotUsed\r\n\
             [e_light_m]\r\nSwitch = false\r\nSwitchBox = light\r\nPredicate = NotUsed\r\n\
             [e_light_lf_lit]\r\nSwitch = true\r\nSwitchBox = light\r\nPredicate = on\r\n",
        );
        let switch = |name| Switch::of_part(car_parts::section(&parts, name).unwrap(), &parts);
        let tail = Switch { kind: Kind::Tail, lamp: 0, lit: true, glass: false, named: true };
        assert_eq!(switch("e_light_lb_lit"), Some(tail));
        assert_eq!(switch("e_light_lb"), Some(Switch { lit: false, ..tail }));
        assert_eq!(switch("e_light_brake_rb_lit"), Some(Switch { kind: Kind::Brake, lamp: 1, ..tail }));
        assert_eq!(switch("e_light_reverse_lb"), None, "its lit pair has its box");
        assert_eq!(switch("e_light_rb_lit"), None, "the game always draws it");
        assert_eq!(switch("e_light_rb"), None, "its lit pair is always drawn");
        assert_eq!(switch("e_light_m"), None, "no lit pair");
        assert_eq!(switch("e_light_lf_lit"), Some(Switch { kind: Kind::Front, ..tail }));
        assert_eq!(tail.glow(), [1.0, 0.0, 0.0, TAIL_DAY], "red, dimmer by day");
        assert_eq!(Switch { kind: Kind::Brake, named: false, ..tail }.glow(), [1.0; 4], "in its texture's colour");
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
        assert_eq!((brake.fade_in, brake.fade_out), (0.025, 0.05));
        assert_eq!(brake.colour, [1.0, 0.0, 0.0]);
        let front = light(&settings[1]).expect("front");
        assert_eq!((front.kind, front.lamp, front.fade_in, front.fade_out), (Kind::Front, 0, 1.0, 1.0));
        assert_eq!(front.colour, [1.0; 3]);
        assert!(light(&settings[2]).is_none(), "kind 2 has no lamps");
    }

    #[test]
    fn lamps_sit_at_their_part() {
        let parts = sections("[Point_light_lf]\r\nNormal = 0 0 -1\r\nPos = 1 2 3\r\n");
        let settings = sections(
            "[Light0]\r\nId = 4\r\nCar Part = Point_light_lf\r\nActive = true\r\n\
             [Light1]\r\nId = 5\r\nCar Part = Point_light_gone\r\n",
        );
        let [point] = &lamp_points(&settings, &parts, RETAIL_HEADLIGHT)[..] else {
            panic!("one lamp");
        };
        assert_eq!((point.position, point.normal), (Vec3::new(1.0, 2.0, 3.0), Vec3::NEG_Z));
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
        let points = lamp_points(&settings, &parts, RETAIL_HEADLIGHT);
        let fixed: Vec<(&str, Option<bool>)> = points.iter().map(|p| (p.light.part.as_str(), p.light.fixed)).collect();
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
    fn a_car_without_headlight_records_gets_beams() {
        // As the RSF Focus WRC 2006 as installed (its "lights off" option): brake records only, both headlight points.
        let parts = sections(
            "[Point_light_lf]\r\nPos = -1 0 0\r\nNormal = 0 0 -1\r\n[Point_light_rf]\r\nPos = 1 0 0\r\nNormal = 0 0 -1\r\n\
             [Point_light_brake_lb]\r\nPos = 0 0 2\r\nNormal = 0 0 1\r\n",
        );
        let settings = sections("[Light0]\r\nId = 16\r\nCar Part = Point_light_brake_lb\r\nActive = false\r\n");
        let warm = [1.0, 0.8, 0.6];
        let points = lamp_points(&settings, &parts, warm);
        assert_eq!(points[1].light.colour, warm);
        let beams: Vec<(Kind, u32, Option<bool>)> =
            points.iter().map(|p| (p.light.kind, p.light.lamp, p.light.fixed)).collect();
        assert_eq!(beams, [(Kind::Brake, 0, Some(false)), (Kind::Front, 0, None), (Kind::Front, 1, None)]);
    }

    #[test]
    fn a_car_whose_headlight_records_never_light_gets_beams() {
        // As the RSF Escort Mk II as installed: headlight records `Active = false`, its lamps' parts on the `light` box.
        let parts = sections(
            "[e_light_lf_lit]\r\nSwitch = true\r\nSwitchBox = light\r\nPredicate = off\r\n\
             [Point_light_lf]\r\nPos = -1 0 0\r\nNormal = 0 0 -1\r\n[Point_light_rf]\r\nPos = 1 0 0\r\nNormal = 0 0 -1\r\n",
        );
        let off = "[Light0]\r\nId = 4\r\nCar Part = Point_light_lf\r\nActive = false\r\n\
                   [Light1]\r\nId = 5\r\nCar Part = Point_light_rf\r\nActive = false\r\n";
        let beams = |settings: &str| -> Vec<(u32, Option<bool>)> {
            lamp_points(&sections(settings), &parts, RETAIL_HEADLIGHT)
                .iter()
                .map(|p| (p.light.lamp, p.light.fixed))
                .collect()
        };
        assert_eq!(beams(off), [(0, Some(false)), (1, Some(false)), (0, None), (1, None)]);
        let on = off.replace("false", "true");
        assert_eq!(beams(&on), [(0, Some(true)), (1, Some(true))], "the \"on\" option's lamps always light");
    }

    #[test]
    fn a_car_without_headlight_points_takes_them_from_its_lights_on_option() {
        // As the RSF GR Yaris Rally2 as installed: no headlight records and no headlight points, which its "lights on"
        // option's `<car>.ini` has.
        let parts = sections("[Point_light_rf]\r\nPos = 1 0 0\r\nNormal = 0 0 -1\r\n");
        let on = sections(
            "[Point_light_rf]\r\nPos = 5 0 0\r\nNormal = 0 0 -1\r\n[Point_light_lf]\r\nPos = -1 0 0\r\nNormal = 0 0 -1\r\n",
        );
        let parts = with_headlight_points(&parts, &on);
        let points = lamp_points(&[], &parts, RETAIL_HEADLIGHT);
        let beams: Vec<(Kind, u32, Vec3)> = points.iter().map(|p| (p.light.kind, p.light.lamp, p.position)).collect();
        assert_eq!(beams, [(Kind::Front, 0, Vec3::new(-1.0, 0.0, 0.0)), (Kind::Front, 1, Vec3::X)]);
        assert_eq!(with_headlight_points(&parts, &[]), parts, "nothing to take");
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
}

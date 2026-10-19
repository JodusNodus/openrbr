//! RBRHUD's in-car gauges (`Plugins/RBRHUD/gauges/` of an RSF install; `notes/cameras-hud-launcher.md` "RBRHUD gauges"). The plugin
//! draws working dials by redrawing parts of a car's interior texture each frame: a gauge folder per car
//! (`gauges/<car folder>/`) holds variants (`default/`, `race/`, ...), each with one `config<N>.ini` per texture it
//! draws on, and the textures and sprite atlases under `textures/`. `gauge_selections.ini` names each car's variant.
//!
//! A config's `[main]` section names the car texture it replaces (`texture`, a file name), the copy it draws on
//! (`remapped`, the car's texture with its painted needles removed) and its sprite atlas (`atlas`); every other section
//! is an element by its `type`: `DirtyRegion` (a texel rect redrawn each frame), `Rotating` (a needle), `Static`,
//! `Simple` (a sprite shown by a condition), `Number`, `Expanding` (a bar), `Effect` (blink, show, hide or recolour an
//! element by a condition), `Colour` (a sprite tinted by its value), `Pie` (a sprite shown as a wedge up to its value's
//! angle) and `Damage` (a sprite coloured by the worst of some parts' damage). Positions are texels of the remapped
//! texture.

use crate::ini::{self, Section};
use std::path::{Path, PathBuf};

/// The gauges folder under an RSF install.
pub const GAUGES: &str = "Plugins/RBRHUD/gauges";
/// The per-car variant choices under an RSF install.
pub const SELECTIONS: &str = "Plugins/RBRHUD/gauge_selections.ini";
/// The variant a car without a choice gets.
pub const DEFAULT_VARIANT: &str = "default";
/// The rects copied from a car's own texture into its remapped copy, under an RSF install.
pub const MIGRATIONS: &str = "Plugins/RBRHUD/texture_migrations.ini";

/// A texel rect, edges inclusive of `left` and `top`, exclusive of `right` and `bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

/// One `config<N>.ini`: the drawing on one car texture.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// The car texture replaced, a file name (`Ascona_400_internal.dds`).
    pub texture: String,
    /// The texture drawn on, relative to the gauge folder (`textures/Ascona_400_internal.dds`).
    pub remapped: Option<String>,
    /// The sprite atlas's `.ini`, relative to the gauge folder.
    pub atlas: String,
    pub emissive_mask: Option<String>,
    pub regions: Vec<Rect>,
    pub elements: Vec<Element>,
    pub effects: Vec<Effect>,
}

/// A drawn element.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub name: String,
    pub kind: Kind,
    /// The telemetry value it shows (`RPM`, `SpeedKPH`, `Gear`, ...).
    pub data: Option<String>,
    /// The gauge mode it shows in (0: every mode).
    pub mode: u32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    /// Degrees, clockwise in texel space.
    pub rotation: f32,
    /// `AARRGGBB`.
    pub colour: u32,
    /// The atlas sprite.
    pub texture: Option<String>,
    pub condition: Condition,
    /// The effects applied, by section name.
    pub effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A sprite with its top left corner at the position.
    Static,
    /// A [`Kind::Static`] shown while its condition holds.
    Simple,
    /// A sprite turned about its pivot, which is at the position.
    Rotating(Rotating),
    Number(Number),
    Expanding(Expanding),
    /// A [`Kind::Static`] tinted by its value: `(value, AARRGGBB)` from `value<N>` and `colour<N>`, blended between.
    Colour(Vec<(f32, u32)>),
    /// A sprite shown as the wedge about its middle from the first angle to its value's: `(value, degrees)` from
    /// `rotation<N>value` and `rotation<N>deg`, as a [`Rotating`] turns.
    Pie(Vec<(f32, f32)>),
    Damage(Damage),
    /// Unknown types.
    Other(String),
}

/// A `Damage` element: its sprite coloured by the level of the worst of the parts' damage (`damage_telemetry`).
#[derive(Debug, Clone, PartialEq)]
pub struct Damage {
    pub parts: Vec<String>,
    /// `damageleveldisp<N>`: the level's colour (`AARRGGBB`) and whether it blinks, from no damage up.
    pub levels: Vec<(u32, bool)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rotating {
    /// The pivot in sprite texels.
    pub pivot: [f32; 2],
    /// `(value, degrees)` from `rotation<N>value` and `rotation<N>deg`, in file order.
    pub map: Vec<(f32, f32)>,
    pub spring: Option<f32>,
    pub damping: Option<f32>,
    pub speed: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    /// The atlas font ([`Font`]).
    pub font: String,
    pub format: String,
    /// `Left`, `Center` or `Right`.
    pub justification: String,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

/// The characters of a `simplefont` strip, in order.
pub const FONT_CHARS: &str = "0123456789:.-";

#[derive(Debug, Clone, PartialEq)]
pub struct Expanding {
    /// `right`, `left`, `up` or `down`.
    pub direction: String,
    /// `(value, shown fraction)`: `expandstartvalue` and `expandendvalue` (0 to 1), or the `expandmap<N>` pairs.
    pub map: Vec<(f32, f32)>,
}

/// When an element or effect is on: its value equals `equal`; else it is under `limit`, when there is one, and over
/// `threshold` (0 when neither is given).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Condition {
    pub equal: Option<f32>,
    pub threshold: Option<f32>,
    pub limit: Option<f32>,
}

impl Condition {
    pub fn holds(&self, value: f32) -> bool {
        match (self.equal, self.threshold, self.limit) {
            (Some(equal), ..) => value == equal,
            (None, threshold, Some(limit)) => value < limit && threshold.is_none_or(|t| value > t),
            (None, threshold, None) => value > threshold.unwrap_or(0.0),
        }
    }
}

/// An `Effect` section: while its data's condition holds, the effects in `kinds` (`Blink`, `Hide`, `Show`,
/// `ColourChange`, `Pulse`) apply to the elements naming it.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub name: String,
    pub kinds: Vec<String>,
    pub data: Option<String>,
    pub condition: Condition,
    /// Milliseconds.
    pub blink: Option<f32>,
    pub new_colour: Option<u32>,
}

/// A sprite atlas: its texture (relative to the gauge folder), its sprites and its fonts, as texel rects `x, y, w, h`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Atlas {
    pub texture: String,
    pub sprites: Vec<(String, [u32; 4])>,
    pub fonts: Vec<(String, Font)>,
}

/// A font's characters and their atlas rects: a `simplefont` strip cut into [`FONT_CHARS`]' equal cells, or one rect
/// per character code (`46=330,0,15,62`), relative to the section's `charset` rect when it has one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Font {
    pub glyphs: Vec<(char, [u32; 4])>,
}

impl Font {
    fn strip([x, y, w, h]: [u32; 4]) -> Font {
        let n = FONT_CHARS.len() as u32;
        let glyphs = FONT_CHARS.chars().zip(0..).map(|(c, i)| (c, [x + w * i / n, y, w / n, h])).collect();
        Font { glyphs }
    }

    pub fn glyph(&self, c: char) -> Option<[u32; 4]> {
        self.glyphs.iter().find(|(g, _)| *g == c).map(|(_, r)| *r)
    }
}

impl Atlas {
    pub fn parse(text: &str) -> Atlas {
        let sections = ini::parse(text).unwrap_or_default();
        let mut atlas = Atlas::default();
        for section in &sections {
            if section.name.eq_ignore_ascii_case("MAIN") {
                atlas.texture = section.get("texture").unwrap_or_default().to_owned();
            } else if section.name.eq_ignore_ascii_case("Objects") {
                atlas.sprites.extend(section.entries.iter().filter_map(|(k, v)| Some((k.clone(), rect(v)?))));
            } else if let Some(r) = section.get("simplefont").or(section.get("fontsimple")).and_then(rect) {
                atlas.fonts.push((section.name.clone(), Font::strip(r)));
            } else {
                // A `charset` strip (`yaris_rally2`'s fonts) holds the glyphs, their rects relative to it (one starts
                // at x -1).
                let [cx, cy, ..] = section.get("charset").and_then(rect).unwrap_or_default();
                let glyphs = section
                    .entries
                    .iter()
                    .filter_map(|(k, v)| {
                        let [x, y, w, h] = numbers::<i64>(v)?;
                        let at = |c: u32, v: i64| u32::try_from((i64::from(c) + v).max(0)).ok();
                        let rect = [at(cx, x)?, at(cy, y)?, u32::try_from(w).ok()?, u32::try_from(h).ok()?];
                        Some((char::from_u32(k.trim().parse().ok()?)?, rect))
                    })
                    .collect();
                atlas.fonts.push((section.name.clone(), Font { glyphs }));
            }
        }
        atlas
    }

    /// The sprite named `name`, with or without its `.png`, ignoring case.
    pub fn sprite(&self, name: &str) -> Option<[u32; 4]> {
        find(&self.sprites, name).copied()
    }

    pub fn font(&self, name: &str) -> Option<&Font> {
        find(&self.fonts, name)
    }
}

fn find<'a, T>(list: &'a [(String, T)], name: &str) -> Option<&'a T> {
    let stem = |s: &str| {
        let s = s.trim();
        s.strip_suffix(".png").or(s.strip_suffix(".PNG")).unwrap_or(s).to_ascii_lowercase()
    };
    let name = stem(name);
    list.iter().find(|(k, _)| stem(k) == name).map(|(_, r)| r)
}

fn rect(value: &str) -> Option<[u32; 4]> {
    numbers(value)
}

/// The four comma-separated numbers of `value`.
fn numbers<T: std::str::FromStr>(value: &str) -> Option<[T; 4]> {
    let parts: Vec<T> = value.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    parts.try_into().ok()
}

impl Config {
    pub fn parse(text: &str) -> Option<Config> {
        let sections = ini::parse(text).ok()?;
        let main = sections.iter().find(|s| s.name.eq_ignore_ascii_case("main"))?;
        let mut config = Config {
            texture: main.get("texture")?.to_owned(),
            remapped: main.get("remapped").filter(|s| !s.is_empty()).map(str::to_owned),
            atlas: main.get("atlas")?.to_owned(),
            emissive_mask: main.get("emissive_mask").filter(|s| !s.is_empty()).map(str::to_owned),
            regions: Vec::new(),
            elements: Vec::new(),
            effects: Vec::new(),
        };
        for section in sections.iter().filter(|s| !std::ptr::eq(*s, main)) {
            let Some(kind) = section.get("type") else { continue };
            match kind {
                "DirtyRegion" => {
                    let edge = |key| number(section, key).map(|v| v.max(0.0) as u32);
                    if let (Some(left), Some(top), Some(right), Some(bottom)) =
                        (edge("left"), edge("top"), edge("right"), edge("bottom"))
                    {
                        config.regions.push(Rect { left, top, right, bottom });
                    }
                }
                "Effect" => config.effects.push(Effect {
                    name: section.name.clone(),
                    kinds: list(section.get("effecttype").unwrap_or_default()),
                    data: text_of(section, "data"),
                    condition: condition(section),
                    blink: number(section, "blinkspeed"),
                    new_colour: section.get("newcolour").and_then(colour),
                }),
                _ => config.elements.push(element(section, kind)),
            }
        }
        Some(config)
    }
}

fn element(section: &Section, kind: &str) -> Element {
    let kind = match kind {
        "Static" => Kind::Static,
        "Simple" => Kind::Simple,
        "Rotating" => Kind::Rotating(Rotating {
            pivot: [number(section, "xpivot").unwrap_or(0.0), number(section, "ypivot").unwrap_or(0.0)],
            map: pairs(section, "rotation", "value", "deg"),
            spring: number(section, "springconstant"),
            damping: number(section, "dampingratio"),
            speed: number(section, "rotationspeed"),
        }),
        "Number" => {
            let enabled = |key: &str| section.get(key).is_some_and(|v| v.eq_ignore_ascii_case("true"));
            Kind::Number(Number {
                font: section.get("font").unwrap_or_default().to_owned(),
                format: section.get("format").unwrap_or("Whole").to_owned(),
                justification: section.get("justification").unwrap_or("Left").to_owned(),
                prefix: section.get("prefix").filter(|_| enabled("prefixenabled")).map(str::to_owned),
                suffix: section.get("suffix").filter(|_| enabled("suffixenabled")).map(str::to_owned),
            })
        }
        "Expanding" => {
            let mut map = pairs(section, "expandmap", "value", "expand");
            if let (Some(start), Some(end)) = (number(section, "expandstartvalue"), number(section, "expandendvalue")) {
                map = vec![(start, 0.0), (end, 1.0)];
            }
            Kind::Expanding(Expanding { direction: section.get("direction").unwrap_or("right").to_owned(), map })
        }
        "Colour" => Kind::Colour(
            (0..)
                .map_while(|n| {
                    Some((number(section, &format!("value{n}"))?, colour(section.get(&format!("colour{n}"))?)?))
                })
                .collect(),
        ),
        "Pie" => Kind::Pie(pairs(section, "rotation", "value", "deg")),
        "Damage" => Kind::Damage(Damage {
            parts: list(section.get("damage_telemetry").unwrap_or_default()),
            levels: (0..)
                .map_while(|n| {
                    let (colour_text, blink) = section.get(&format!("damageleveldisp{n}"))?.split_once(',')?;
                    Some((colour(colour_text)?, blink.trim() == "1"))
                })
                .collect(),
        }),
        other => Kind::Other(other.to_owned()),
    };
    Element {
        name: section.name.clone(),
        kind,
        data: text_of(section, "data"),
        mode: number(section, "mode").unwrap_or(0.0) as u32,
        x: number(section, "xpos").unwrap_or(0.0),
        y: number(section, "ypos").unwrap_or(0.0),
        scale: number(section, "scale").unwrap_or(1.0),
        rotation: number(section, "rotation").unwrap_or(0.0),
        colour: section.get("colour").and_then(colour).unwrap_or(0xffff_ffff),
        texture: text_of(section, "texture"),
        condition: condition(section),
        effects: list(section.get("effect").unwrap_or_default()),
    }
}

fn number(section: &Section, key: &str) -> Option<f32> {
    section.get(key)?.parse().ok()
}

fn text_of(section: &Section, key: &str) -> Option<String> {
    section.get(key).filter(|s| !s.is_empty()).map(str::to_owned)
}

fn condition(section: &Section) -> Condition {
    Condition {
        equal: number(section, "equal"),
        threshold: number(section, "threshold"),
        limit: number(section, "limit"),
    }
}

fn colour(value: &str) -> Option<u32> {
    u32::from_str_radix(value.trim(), 16).ok()
}

/// The non-empty items of a `|` or `,` separated list.
fn list(value: &str) -> Vec<String> {
    value.split(['|', ',']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

/// `<prefix><N><a>` and `<prefix><N><b>` pairs for N from 0 while both are present.
fn pairs(section: &Section, prefix: &str, a: &str, b: &str) -> Vec<(f32, f32)> {
    (0..)
        .map_while(|n| {
            Some((number(section, &format!("{prefix}{n}{a}"))?, number(section, &format!("{prefix}{n}{b}"))?))
        })
        .collect()
}

/// The rects of `texture_migrations.ini` (`text`) for the car texture `texture` (a file name, any case): `x0, y0, x1,
/// y1` as fractions of the texture, ordered. The plugin copies them from the car folder's texture into the remapped
/// one, so that a custom skin's colours show on the old cars whose dash shares a texture with the body.
pub fn migrations(text: &str, texture: &str) -> Vec<[f32; 4]> {
    let sections = ini::parse(text).unwrap_or_default();
    let Some(section) = sections.iter().find(|s| s.name.trim().eq_ignore_ascii_case(texture)) else {
        return Vec::new();
    };
    let rects = section.get("migrations").unwrap_or_default().split(';');
    rects
        .filter_map(|rect| {
            let v: Vec<f32> = rect.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
            let [x0, y0, x1, y1] = v.try_into().ok()?;
            Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)].map(|f: f32| f.clamp(0.0, 1.0)))
        })
        .filter(|[x0, y0, x1, y1]| x0 < x1 && y0 < y1)
        .collect()
}

/// A car's chosen gauge variant under the RSF install `root`: its folder and its configs (in `config<N>` order), or
/// `None` when the car has no gauge or its choice is not installed.
pub fn variant(root: &Path, car_folder: &str) -> Option<(PathBuf, Vec<Config>)> {
    let chosen = std::fs::read_to_string(root.join(SELECTIONS))
        .ok()
        .and_then(|text| ini::parse(&text).ok())
        .and_then(|sections| {
            let section = sections.into_iter().find(|s| s.name.eq_ignore_ascii_case(car_folder))?;
            section.get("gauge").map(str::to_owned)
        })
        .unwrap_or_else(|| DEFAULT_VARIANT.to_owned());
    let car = crate::cars::resolve(&root.join(GAUGES), car_folder)?;
    let folder = crate::cars::resolve(&car, &chosen)?;
    let mut names: Vec<PathBuf> = std::fs::read_dir(&folder)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")))
        .filter(|p| {
            p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.to_ascii_lowercase().starts_with("config"))
        })
        .collect();
    names.sort();
    let configs: Vec<Config> =
        names.iter().filter_map(|p| Config::parse(&String::from_utf8_lossy(&std::fs::read(p).ok()?))).collect();
    (!configs.is_empty()).then_some((car, configs))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASCONA: &str = "[main]\r\nauthor = A\r\natlas = textures/defaultatlas.ini\r\n\
        remapped = textures/Ascona_400_internal.dds\r\ntexture = Ascona_400_internal.dds\r\ndescription = \r\n\
        [dirtyregion1]\r\ntype = DirtyRegion\r\nleft = 3592\r\ntop = 82\r\nright = 4029\r\nbottom = 484\r\n\
        [engineoffblink]\r\ntype = Effect\r\neffecttype = Blink|Hide|\r\ndata = EngineOn\r\nblinkspeed = 900.000000\r\n\
        [rpmgauge]\r\ntype = Rotating\r\ndata = RPM\r\nmode = 0\r\nxpos = 3853.000000\r\nypos = 994.000000\r\n\
        scale = 1.000000\r\nrotation = 0.000000\r\ncolour = 3c000000\r\ntexture = RPMneedle4.png\r\nxpivot = 157.0\r\n\
        ypivot = 29.0\r\nspringconstant = 30.0\r\ndampingratio = 1.5\r\nrotation0deg = -35.0\r\n\
        rotation0value = 500.0\r\nrotation1deg = 218.0\r\nrotation1value = 8000.0\r\n\
        [gear1]\r\ntype = Simple\r\ndata = Gear\r\nequal = 2\r\ntexture = Gear1.png\r\neffect = a,b\r\n\
        [ring]\r\ntype = Colour\r\ndata = UpShift\r\nvalue0 = -2000.0\r\ncolour0 = ffffff00\r\nvalue1 = 0.0\r\n\
        colour1 = ffff0000\r\n[dmg]\r\ntype = Damage\r\ndamage_telemetry = DamageHubLF,DamageWheelLF,\r\n\
        damageleveldisp0 = 00ffffff,0\r\ndamageleveldisp1 = ffff0000,1\r\n";

    #[test]
    fn a_config_reads() {
        let config = Config::parse(ASCONA).unwrap();
        assert_eq!(config.texture, "Ascona_400_internal.dds");
        assert_eq!(config.regions, [Rect { left: 3592, top: 82, right: 4029, bottom: 484 }]);
        assert_eq!(config.effects[0].kinds, ["Blink", "Hide"]);
        assert_eq!(config.effects[0].blink, Some(900.0));
        let rpm = &config.elements[0];
        assert_eq!((rpm.x, rpm.y, rpm.colour), (3853.0, 994.0, 0x3c00_0000));
        let Kind::Rotating(needle) = &rpm.kind else { panic!("{:?}", rpm.kind) };
        assert_eq!(needle.pivot, [157.0, 29.0]);
        assert_eq!(needle.map, [(500.0, -35.0), (8000.0, 218.0)]);
        let gear = &config.elements[1];
        assert_eq!((gear.kind.clone(), gear.effects.clone()), (Kind::Simple, vec!["a".to_owned(), "b".to_owned()]));
        assert!(gear.condition.holds(2.0) && !gear.condition.holds(3.0));
        assert_eq!(config.elements[2].kind, Kind::Colour(vec![(-2000.0, 0xffff_ff00), (0.0, 0xffff_0000)]));
        let Kind::Damage(damage) = &config.elements[3].kind else { panic!("{:?}", config.elements[3].kind) };
        assert_eq!(damage.parts, ["DamageHubLF", "DamageWheelLF"]);
        assert_eq!(damage.levels, [(0x00ff_ffff, false), (0xffff_0000, true)]);
    }

    #[test]
    fn migrations_read() {
        let text = "; comment\n[Trabant P800RS_internal.dds]\nmigrations=0.0,0.001,0.6,0.87\n\
            [205-t16_e1_internal.dds]\nmigrations=0.0,0.0,0.6,0.8;0.59,0.57,0.7,0.34\n";
        assert_eq!(migrations(text, "trabant p800rs_internal.dds"), [[0.0, 0.001, 0.6, 0.87]]);
        assert_eq!(migrations(text, "205-T16_e1_internal.dds")[1], [0.59, 0.34, 0.7, 0.57]);
        assert!(migrations(text, "other.dds").is_empty());
    }

    #[test]
    fn an_atlas_reads() {
        let atlas = Atlas::parse(
            "[MAIN]\ntexture=textures/defaultatlas.png\n[Objects]\nspeedneedle4.png=2,2,194,70\n\
             [trabantfont.png]\nsimplefont=200,35,208,30",
        );
        assert_eq!(atlas.texture, "textures/defaultatlas.png");
        assert_eq!(atlas.sprite("SpeedNeedle4"), Some([2, 2, 194, 70]));
        assert_eq!(atlas.font("trabantfont").and_then(|f| f.glyph('1')), Some([216, 35, 16, 30]));
        let atlas = Atlas::parse("[Arial_smalldata]\n45=310,0,18,62\n46=330,0,15,62\n");
        assert_eq!(atlas.font("arial_smalldata").and_then(|f| f.glyph('.')), Some([330, 0, 15, 62]));
        let atlas = Atlas::parse("[Roboto_stage]\ncharset=0,3447,4096,190\n37 =207 ,0,46,89\n");
        assert_eq!(atlas.font("roboto_stage").and_then(|f| f.glyph('%')), Some([207, 3447, 46, 89]));
        let atlas = Atlas::parse("[Arial_bigdata]\ncharset=385,0,604,101\n48=-1  ,0,48,98\n");
        assert_eq!(atlas.font("arial_bigdata").and_then(|f| f.glyph('0')), Some([384, 0, 48, 98]));
    }

    /// The pages of digital dashes in the install `RBR_RSF`: the Fiesta Rally2's (RSF 81, `fiesta_rally2_23`) two, and
    /// the i20 Rally2's `four_mode_test`, which spells the key `Mode` (its layout, without one, shows on every page).
    #[test]
    fn digital_dash_pages_read() {
        let Some(root) = std::env::var_os("RBR_RSF").map(PathBuf::from) else {
            eprintln!("skipping: RBR_RSF not set");
            return;
        };
        let modes = |configs: &[Config]| {
            let mut modes: Vec<u32> = configs.iter().flat_map(|c| &c.elements).map(|e| e.mode).collect();
            modes.sort();
            modes.dedup();
            modes
        };
        let (_, fiesta) = variant(&root, "fiesta_rally2_23").unwrap();
        assert_eq!(modes(&fiesta), [0, 1, 2]);
        let path = root.join(GAUGES).join("i20_rally2/four_mode_test/config1.ini");
        let four = Config::parse(&String::from_utf8_lossy(&std::fs::read(path).unwrap())).unwrap();
        assert_eq!(modes(&[four]), [0, 1, 2, 3, 4]);
    }
}

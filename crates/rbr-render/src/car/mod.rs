//! Loads one car for drawing: its `.sgc` meshes, placed by their world matrices, each face drawn
//! with the diffuse texture its material names in `<car>_shaders.ini`.
//!
//! Everything is read through `rbr-formats`: `Cars/Cars.ini` (the car's files), the model (`sgc`),
//! its part sections (`<car>.ini`, `car_parts`), the material sections (`<car>_shaders.ini`), the
//! shader settings (for the class names) and the DDS textures. Placement goes through [`coords`].
//!
//! Milestone 3 draws the base diffuse texture times the vertex colour (white, since the model has
//! none), unlit. Glass is alpha blended (see [`Pass`]). Milestone 5 keeps the base texture only: the
//! dirt layer is zero at the stage start, and the rest is open (`notes/formats.md` "Shaders and vertex layouts").
//! The shader classes drawn with the base texture only are listed in [`CarReport::base_only_shaders`].
//! The damageable panels keep their deformation data ([`Car::damage`], [`crate::damage`]): [`Car::dent`] dents them and
//! sets the scrape amount, which `CarPaint` and `CarPaintInt` faces blend their scrape texture in by. Each window part
//! also gets its crack overlays (`Textures\WS_Broken.dds` projected as `ProjectUVs` does), which [`Car::crack`] keeps
//! for the cracks a car has taken.
//!
//! A node's world matrix is relative to its parent's, so interior parts are placed through their
//! parents (`sgc::chained_worlds`), and each mesh's transform block is composed under it as the loader
//! does ([`coords::part_to_car`]): the wheels turn by pi so their rims face out. The side doors and their
//! interior panels are hinged bodies at rest, placed by [`coords::hinged_panel_to_car`].
//!
//! Not drawn (see [`Skip`]): ghost parts, the overlay layers `texture_*`, the shadow planes `Plane_*`,
//! the interior set `i_*` (the game draws it only in the in-car views), parts the car shows only in some
//! states and hidden at the start (`Switch = true`, [`lights::shown_at_start`]), gravel and snow wheels and spare wheels (the tarmac ones stay at the same
//! nodes), and parts the 1.02 loader does not build (`k_`, `d_`). Wheels have no physics: they sit at their
//! nodes.

use crate::coords;
use crate::damage::{self, Cracks, Dents, MeshDamage, Zones};
use crate::lights::{self, Flare};
use crate::parallel::in_parallel;
use crate::stage::{Draw, Mesh, Pose, Selection, Shade, Stage, StageTexture, TextureRef, TextureSet, Vertex};
use glam::{Mat4, Vec3};
use rbr_formats::data::Data;
use rbr_formats::ini::Section;
use rbr_formats::sgc;
use rbr_formats::{car_parts, cars};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

pub mod cabin;
mod files;
mod meshes;
mod parts;
mod shading;

use files::{folder, read_bytes, read_ini, read_text, resolve, shader_file, Textures};
use meshes::{build_meshes, crack_overlays, in_draw_order};
pub(crate) use parts::wheel_of;
use parts::{detach_part, ghost_part_for, hinge_side, interior_part_for, part_for};
use shading::{dirt_texture, list_for, pass_for, scrape_texture, shade_for, FaceStyle, List, Order, Pass};

/// Vertex colour of every car vertex: white, as the model has none, so the texture is drawn as it is. The alpha is the
/// scrape amount ([`crate::damage`]), 0 until the car is damaged.
const UNDAMAGED: [u8; 4] = [255, 255, 255, 0];
/// Name prefix of the ghost parts, which only the ghost car uses ([`load_ghost`]).
const GHOST_PREFIX: &str = "ghost_";
/// Name prefix of the overlay layers (dirt and scrape maps, milestone 5).
const OVERLAY_PREFIX: &str = "texture_";
/// Name prefix of the shadow planes: decals under the car, drawn with the shadow texture (milestone 5).
const PLANE_PREFIX: &str = "Plane_";
/// Interior parts that the 1.02 loader does not build (rbr-recomp `notes/formats/sgc.md`, section 9).
const LOADER_DROPS: [&str; 4] = ["i_wing", "i_bumper", "i_window_lf", "i_window_rf"];
/// The placeholder box (a 2.3 m cube round the cabin) the loader files under a kind that no draw list takes
/// (HedgeHog3D 0x1001e090 names it kind 10; `FUN_1001e430` adds kinds 0 to 9 to the lists).
const PREMOVE_PART: &str = "pc_e_premove";
/// Name prefixes of parts the loader never builds as geometry: `k_` (the broken-window parts; `k_windscreen` is
/// read into a damage object and freed) and `d_` (rbr-recomp `notes/formats/sgc.md`, section 9).
const UNBUILT_PREFIXES: [&str; 2] = ["k_", "d_"];
/// Name prefix of the interior set. The loader keeps its shaders in their own lists, and `CHH_Car::Render` draws
/// either the interior lists (mode 0, the in-car views) or the exterior ones (rbr-recomp `notes/formats/sgc.md`, section 9).
/// [`load`] reads the exterior set and [`load_interior`] the interior one, for the viewer's cockpit view.
const INTERIOR_PREFIX: &str = "i_";
/// Wheels and spare wheels (and their straps) come in surface variants. The loader builds those whose name has
/// its surface; only tarmac is drawn, as the surface choice is not traced.
const SURFACE_PARTS: [&str; 2] = ["_wheel_", "sparewheel"];
const SURFACE: &str = "tarmac";
/// The 1.02 shader pair of each car class, as the game's id tables load it (`ShaderManager.cpp`:
/// `g_vsFogDescs`, `g_psFogDescs`, `g_psGeneralDescs`; `notes/formats.md` "Shaders and vertex layouts"): the class, the file stem
/// and the folder under `VertexShaders` and `PixelShaders`. `CarColor` and `CarColorNoDirt` load the
/// `CarTextureNoDirt` pair, and `CarLight` the `CarRim` pair. `CarIntGlass` is in `General`, which the 1.02
/// patch does not replace.
const SHADER_FILES: [(&str, &str, &str); 12] = [
    ("CarColor", "CarTextureNoDirt", "Fog"),
    ("CarColorNoDirt", "CarTextureNoDirt", "Fog"),
    ("CarTextureNoDirt", "CarTextureNoDirt", "Fog"),
    ("CarTexture", "CarTexture", "Fog"),
    ("CarTire", "CarTexture", "Fog"),
    ("CarPaint", "CarPaint1", "Fog"),
    ("CarPaintInt", "CarPaintInternal", "Fog"),
    ("CarPlastic", "CarPlastic", "Fog"),
    ("CarRim", "CarRim", "Fog"),
    ("CarLight", "CarRim", "Fog"),
    ("CarGlass", "CarGlass", "Fog"),
    ("CarIntGlass", "CarGlassInternal", "General"),
];
/// The crack texture's name in reports, and the class whose shaders draw it (`CHH_Car::InitDamage`, HH 0x10017f50:
/// VS 0x2e and PS 0x11, `CarTextureNoDirt`). The texture is ours ([`crate::crack_decal`]), in place of the game's
/// `Textures\WS_Broken.dds`.
const CRACK_TEXTURE: &str = "crack decal (ours, in place of Textures\\WS_Broken.dds)";
const CRACK_SHADER: &str = "CarTextureNoDirt";
/// The version token at the start of a shader: `vs_1_1` and `ps_1_1`, little-endian.
const VERTEX_SHADER_1_1: u32 = 0xfffe_0101;
const PIXEL_SHADER_1_1: u32 = 0xffff_0101;

/// Why a model mesh is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// No `<car>.ini` section of that name.
    NoSection,
    /// A ghost part.
    Ghost,
    /// Not in the ghost set ([`load_ghost`]).
    NotGhost,
    /// An overlay layer (dirt or scrape map).
    Overlay,
    /// A shadow plane, a decal under the car.
    Plane,
    /// An interior part the 1.02 loader does not build.
    LoaderDrops,
    /// The interior set, drawn only in the in-car views.
    Interior,
    /// Not in the interior set ([`load_interior`]): the outside views' parts.
    Exterior,
    /// A switch part (`Switch = true`) whose box is not in its `Predicate` state at the start ([`lights::shown_at_start`]):
    /// the lit lights and the like.
    Switched,
    /// A gravel or snow wheel or spare wheel.
    SurfaceWheel,
    /// No `FaceBlock` keys, so no shader slot: planes, premove parts and the like.
    NoShaderSlots,
}

impl Display for Skip {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Skip::NoSection => "no section in <car>.ini",
            Skip::Ghost => "ghost part",
            Skip::NotGhost => "not a ghost part",
            Skip::Overlay => "overlay layer (milestone 5)",
            Skip::Plane => "shadow plane (decal, milestone 5)",
            Skip::LoaderDrops => "not built by the 1.02 loader",
            Skip::Switched => "switch part hidden at the start (Switch = true)",
            Skip::Interior => "interior set (in-car views only)",
            Skip::Exterior => "exterior set (outside views only)",
            Skip::SurfaceWheel => "surface wheel (gravel or snow)",
            Skip::NoShaderSlots => "no FaceBlock keys",
        })
    }
}

/// Failure to load a car. Missing textures and skipped parts are reports, not errors.
#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Cars(cars::Error),
    /// No `Cars.ini` entry whose folder has this name.
    UnknownCar(String),
    /// A file the car's entries name is not in the install or the patch.
    Missing(String),
    /// A shader file that is not `vs_1_1` or `ps_1_1` bytecode.
    NotShader(PathBuf),
    Ini {
        path: PathBuf,
        line: usize,
        message: &'static str,
    },
    Sgc(sgc::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Cars(e) => write!(f, "{e}"),
            Error::UnknownCar(name) => write!(f, "no car named {name} in Cars.ini"),
            Error::Missing(path) => write!(f, "{path}: not in the install or the patch"),
            Error::NotShader(path) => {
                write!(f, "{}: not vs_1_1 or ps_1_1 bytecode", path.display())
            }
            Error::Ini { path, line, message } => write!(f, "{}:{line}: {message}", path.display()),
            Error::Sgc(e) => write!(f, "model: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// What loading a car drew and what it left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CarReport {
    /// Model meshes drawn.
    pub meshes_drawn: usize,
    /// Model meshes not drawn, with the reason.
    pub skipped: Vec<(String, Skip)>,
    /// Requests for diffuse textures with no readable DDS (drawn magenta).
    pub missing_textures: Vec<String>,
    /// Triangles with an index outside their mesh, or a face range outside the list. Dropped.
    pub bad_triangles: usize,
    /// Shader classes of drawn faces. Each is drawn with its base texture only.
    pub base_only_shaders: BTreeSet<String>,
    /// Shader classes of drawn faces that the car's shader settings do not define.
    pub unknown_shaders: BTreeSet<String>,
    /// The 1.02 shader files of each drawn class in [`SHADER_FILES`], read through the data overlay.
    pub shader_files: BTreeMap<String, ShaderFiles>,
}

/// The vertex and pixel shader files of a car class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderFiles {
    pub vertex: PathBuf,
    pub pixel: PathBuf,
}

/// A car's meshes in stage space, and its textures. The textures are in [`TextureSet::Car`], so they
/// never share a key with the stage's own.
#[derive(Debug, Clone, PartialEq)]
pub struct Car {
    pub name: String,
    pub meshes: Vec<Mesh>,
    /// Per mesh, what damage does to it.
    pub damage: Vec<MeshDamage>,
    /// The lamps' flares (the exterior set only). Their meshes are added to the stage with the car's; the playback
    /// draws them ([`lights`]).
    pub flares: Vec<Flare>,
    pub textures: BTreeMap<TextureRef, StageTexture>,
    pub report: CarReport,
}

/// The placement of a car at a stage pose (see [`coords::car_to_stage`]). `None` puts the car at the
/// origin with its front along `-z`.
pub fn placement_at(pose: Option<Pose>) -> Mat4 {
    match pose {
        Some(pose) => coords::car_to_stage(pose.position, pose.forward),
        None => Mat4::IDENTITY,
    }
}

/// Loads car `name` (its `Cars.ini` folder, for example `XSARA`, matched ignoring case) from `data`
/// (the install, with the 1.02 patch over it when set), with its model-space positions mapped by `placement`.
pub fn load(data: &Data, name: &str, placement: Mat4) -> Result<Car, Error> {
    load_set(data, name, placement, part_for, true)
}

/// Loads car `name`'s interior set (`i_*`) as [`load`] loads the exterior. `CHH_Car::Render` draws these instead of
/// the exterior in the in-car views (the cockpit camera; rbr-recomp `notes/formats/sgc.md`, section 9).
pub fn load_interior(data: &Data, name: &str, placement: Mat4) -> Result<Car, Error> {
    load_set(data, name, placement, interior_part_for, false)
}

/// Loads car `name`'s ghost set (`ghost_*`, one `GhostCar` mesh per car), which the ghost car is drawn with
/// (`E09_LoadGhostCar` 0x466910). [`crate::ghost`] gives it the ghost's look.
pub fn load_ghost(data: &Data, name: &str, placement: Mat4) -> Result<Car, Error> {
    load_set(data, name, placement, ghost_part_for, false)
}

/// The file of `dos_path`'s name in the "lights on" car-parts option `option` ([`rbr_formats::rsf::headlights_on`]),
/// when it ships one.
fn option_ini(option: &Path, dos_path: &str) -> Option<Vec<Section>> {
    let path = cars::resolve(option, dos_path.rsplit(['\\', '/']).next()?)?;
    rbr_formats::ini::parse(&String::from_utf8_lossy(&std::fs::read(path).ok()?)).ok()
}

/// The meshes `part` admits, each with its part section.
fn load_set(
    data: &Data,
    name: &str,
    placement: Mat4,
    part: for<'a> fn(&str, &'a [Section]) -> Result<&'a Section, Skip>,
    exterior: bool,
) -> Result<Car, Error> {
    let entry = entry(data, name)?;

    let model_path = resolve(data, &entry.file_name)?;
    let model = sgc::parse(&read_bytes(&model_path)?).map_err(Error::Sgc)?;
    let parts = read_ini(data, &entry.ini_file)?;
    let shaders = read_ini(data, &entry.shader_file)?;
    let settings = read_ini(data, &format!("{}_clear.ini", entry.shader_settings))?;
    let classes: BTreeSet<String> = settings.iter().map(|s| s.name.clone()).collect();

    let chained = sgc::chained_worlds(&model);
    // The side doors' hinge point: the exterior door's node translation.
    let hinge = |side: &str| {
        let door = format!("e_door_{side}");
        model.iter().find_map(|e| match e {
            sgc::Element::Mesh(m) if m.node.name == door => {
                let w = chained[&m.node.id];
                Some([w[0][3], w[1][3], w[2][3]])
            }
            _ => None,
        })
    };
    let mut textures = Textures::new(data);
    let mut report = CarReport::default();
    // The car's shaders in the order the loader creates them: one per class and material, at its first face
    // (`FUN_1001e430`). Each list draws its shaders in that order.
    let mut ranks: Vec<(String, String)> = Vec::new();
    let mut groups: Vec<(Order, Mesh, MeshDamage)> = Vec::new();
    for element in &model {
        let sgc::Element::Mesh(mesh) = element else {
            continue;
        };
        let part = match part(&mesh.node.name, &parts) {
            Ok(part) => part,
            Err(reason) => {
                report.skipped.push((mesh.node.name.clone(), reason));
                continue;
            }
        };
        let faces = car_parts::face_blocks(part);
        for face in &faces {
            if !classes.contains(&face.shader) {
                report.unknown_shaders.insert(face.shader.clone());
            }
            report.base_only_shaders.insert(face.shader.clone());
        }
        let face_styles: Vec<FaceStyle> = (0..mesh.faces.len())
            .map(|i| {
                let (shader, material) = faces.get(i).map_or(("", ""), |b| (b.shader.as_str(), b.material.as_str()));
                let texture = car_parts::diffuse_path(&shaders, material).map(|dos_path| textures.key(dos_path));
                let opacity = car_parts::opacity_channel(&shaders, material);
                let scrape = scrape_texture(shader)
                    .and_then(|name| car_parts::friendly_path(&shaders, name))
                    .map(|dos_path| textures.key(dos_path));
                let rank = match ranks.iter().position(|(s, m)| s == shader && m == material) {
                    Some(rank) => rank,
                    None => {
                        ranks.push((shader.to_owned(), material.to_owned()));
                        ranks.len() - 1
                    }
                };
                // `CarPaint` blends its dirt map in by the dirt level (`CarPaint1.pso`, `crate::dirt`); `CarTexture`
                // and `CarTire` bind theirs as their second texture (`Dirt`, `TireDirt`: `<car>_external_dynamic_mud`,
                // `lerp(t0, c3.xyz, t1.a)`). Ours: those two also by the dirt level and in the paint's dirt colour
                // (the game's mask is not scaled and its `c3.xyz` is never written, `notes/formats.md` "Shaders and vertex layouts").
                let dirt = dirt_texture(shader)
                    .and_then(|name| car_parts::friendly_path(&shaders, name))
                    .map(|dos_path| textures.key(dos_path));
                FaceStyle {
                    texture,
                    scrape,
                    shade: shade_for(&settings, shader).map(|shade| Shade {
                        dirt,
                        grime: crate::dirt::grime(&mesh.node.name, shader),
                        spatter: crate::dirt::spatter(&mesh.node.name),
                        ..shade
                    }),
                    pass: pass_for(shader, opacity),
                    order: (list_for(shader, opacity), rank),
                }
            })
            .collect();
        let world = chained.get(&mesh.node.id).copied().unwrap_or(mesh.node.world);
        // The node's world matrix after its transform block (`coords::part_to_car`). The side doors are hinged
        // bodies at rest (`coords::hinged_panel_to_car`).
        let block = mesh.node.transform.words();
        let transform = match hinge_side(&mesh.node.name).and_then(|(s, side)| Some((hinge(s)?, side))) {
            Some((hinge, side)) => coords::hinged_panel_to_car(world, block, hinge, side),
            None => coords::part_to_car(world, block),
        };
        let built = build_meshes(mesh, placement * transform, &face_styles);
        report.bad_triangles += built.bad_triangles;
        if !built.meshes.is_empty() {
            report.meshes_drawn += 1;
        }
        let lamp = lights::Switch::of_part(part, &parts);
        let detach = detach_part(&mesh.node.name);
        let wheel = wheel_of(&mesh.node.name).map(|(_, k)| k);
        let steering = mesh.node.name.eq_ignore_ascii_case("i_steeringwheel");
        let mirror = cabin::Mirror::of_part(&mesh.node.name);
        let window = damage::window_slot(&mesh.node.name);
        groups.extend(built.meshes.into_iter().map(|(order, mut placed, effect)| {
            if let Some(switch) = lamp.filter(|switch| switch.lit && !switch.glass) {
                for shade in placed.draws.iter_mut().filter_map(|draw| draw.state.shade.as_mut()) {
                    shade.glow = Some(switch.glow());
                }
            }
            if window.is_some() {
                for shade in placed.draws.iter_mut().filter_map(|draw| draw.state.shade.as_mut()) {
                    shade.window = true;
                }
            }
            (order, placed, MeshDamage { lamp, detach, wheel, mirror, window, steering, ..effect })
        }));
        if let Some(window) = damage::window_slot(&mesh.node.name) {
            let style = FaceStyle {
                texture: Some(textures.generated(CRACK_TEXTURE, crate::crack_decal::dds)),
                scrape: None,
                shade: shade_for(&settings, CRACK_SHADER).map(|shade| Shade { crack: true, ..shade }),
                pass: Pass::Crack,
                order: (List::Transparent, usize::MAX),
            };
            groups.extend(crack_overlays(mesh, window, transform, placement, style));
        }
    }
    let (meshes, damage) = in_draw_order(groups).into_iter().unzip();
    let flares = if exterior {
        let settings_file = entry.ini_file.replace(".ini", "_settings.ini");
        let settings = read_ini(data, &settings_file)?;
        let on = model_path.parent().and_then(rbr_formats::rsf::headlights_on);
        let on_ini = |file: &str| on.as_deref().and_then(|on| option_ini(on, file)).unwrap_or_default();
        let beam = lights::headlight_colour(&on_ini(&settings_file));
        let parts = lights::with_headlight_points(&parts, &on_ini(&entry.ini_file));
        lights::flares(&settings, &parts, beam, |file| textures.key(&format!("{}{file}", entry.texture_path)))
    } else {
        Vec::new()
    };

    for class in &report.base_only_shaders {
        let Some((_, stem, set)) = SHADER_FILES.iter().find(|(c, _, _)| *c == class) else {
            continue;
        };
        let files = ShaderFiles {
            vertex: shader_file(data, &format!("VertexShaders\\{set}\\{stem}.vso"), VERTEX_SHADER_1_1)?,
            pixel: shader_file(data, &format!("PixelShaders\\{set}\\{stem}.pso"), PIXEL_SHADER_1_1)?,
        };
        report.shader_files.insert(class.clone(), files);
    }

    Ok(Car { name: name.to_owned(), meshes, damage, flares, textures: textures.finish(&mut report), report })
}

impl Car {
    /// Keeps the crack overlays of the cracks `cracks` has taken, and drops the others.
    pub fn crack(&mut self, cracks: &Cracks) {
        let (meshes, damage) = std::mem::take(&mut self.meshes)
            .into_iter()
            .zip(std::mem::take(&mut self.damage))
            .filter(|(_, d)| d.crack.is_none_or(|c| cracks.shows(c)))
            .unzip();
        self.meshes = meshes;
        self.damage = damage;
    }

    /// Adds the inner side of its glass ([`Shade::glass`], not the crack overlays) as meshes of their own, wound the
    /// other way and drawn before any of the glass (ours, [`crate::renderer::Enhance::cabin`]): the far window seen
    /// through the near one tints and reflects too, where the game culls its back faces, and the near pane, blended
    /// after it, stays over it.
    pub fn add_inner_glass(&mut self) {
        let glass = |draw: &Draw| draw.state.shade.is_some_and(|shade| shade.glass);
        let Some(first) = self.meshes.iter().position(|mesh| mesh.draws.iter().any(glass)) else {
            return;
        };
        let (meshes, damage): (Vec<Mesh>, Vec<MeshDamage>) = (self.meshes.iter().zip(&self.damage))
            .filter(|(mesh, effect)| effect.crack.is_none() && mesh.draws.iter().any(glass))
            .map(|(mesh, effect)| {
                let mut indices = Vec::new();
                let draws = (mesh.draws.iter().filter(|draw| glass(draw)))
                    .map(|draw| {
                        let start = indices.len();
                        let faces = mesh.indices[draw.indices.clone()].chunks_exact(3);
                        indices.extend(faces.flat_map(|t| [t[0], t[2], t[1]]));
                        Draw { indices: start..indices.len(), ..draw.clone() }
                    })
                    .collect();
                let vertices = mesh.vertices.iter().map(|v| Vertex { normal: v.normal.map(|n| -n), ..*v }).collect();
                (Mesh { vertices, indices, draws, ..mesh.clone() }, effect.clone())
            })
            .unzip();
        self.meshes.splice(first..first, meshes);
        self.damage.splice(first..first, damage);
    }

    /// The windows that have crack overlays.
    pub fn windows(&self) -> [bool; damage::WINDOWS.len()] {
        let mut present = [false; damage::WINDOWS.len()];
        for crack in self.damage.iter().filter_map(|d| d.crack) {
            present[crack.window] = true;
        }
        present
    }

    /// Dents the damageable panels and sets their scrape amount for the damage handler's `zones`.
    pub fn dent(&mut self, zones: &Zones) {
        for (mesh, damage) in self.meshes.iter_mut().zip(&self.damage) {
            let Some(dents) = &damage.dents else {
                continue;
            };
            for (vertex, (offset, scrape)) in mesh.vertices.iter_mut().zip(dents.under(zones)) {
                vertex.position = (Vec3::from(vertex.position) + offset).to_array();
                vertex.color[3] = damage::scrape_byte(scrape);
            }
        }
    }

    /// The box around the car's meshes, or `None` when it has none.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        self.meshes
            .iter()
            .filter_map(|mesh| mesh.bounds)
            .map(|b| (b.center - b.half_extent, b.center + b.half_extent))
            .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
    }

    /// The car as a stage of its own, with `selection`, no start location and no track names, and its
    /// report.
    pub fn into_stage(self, selection: Selection) -> (Stage, CarReport) {
        let stage = Stage {
            name: self.name,
            selection,
            meshes: self.meshes,
            interactive: Vec::new(),
            texture_names: BTreeMap::new(),
            textures: self.textures,
            start_digits: None,
            snow_ground: false,
            collision: None,
            start: None,
            atmosphere: Default::default(),
            driveline: Vec::new(),
            report: Default::default(),
        };
        (stage, self.report)
    }

    /// Every load numbers its car textures from 0: moves this car's past the car textures already in `stage`, so a
    /// second set (the interior, [`load_interior`]) added to it doesn't replace the first one's.
    pub fn after_textures_of(mut self, stage: &Stage) -> Car {
        let base =
            stage.textures.keys().filter(|key| key.set == TextureSet::Car).map(|key| key.index + 1).max().unwrap_or(0);
        let moved = |key: TextureRef| match key.set {
            TextureSet::Car => TextureRef { index: key.index + base, ..key },
            _ => key,
        };
        for draw in self.meshes.iter_mut().flat_map(|mesh| mesh.draws.iter_mut()) {
            let state = &mut draw.state;
            for texture in state.textures.iter_mut().chain([&mut state.shadow, &mut state.specular]) {
                *texture = texture.map(moved);
            }
        }
        self.textures = self.textures.into_iter().map(|(key, texture)| (moved(key), texture)).collect();
        self
    }

    /// The car at the stage start: its lamp parts in their start state (front and tail lights on, brake and reverse lights
    /// off), and no flares, which only the playback draws.
    pub fn at_rest(mut self) -> Car {
        let rest = lights::Inputs::default();
        let (meshes, damage) = std::mem::take(&mut self.meshes)
            .into_iter()
            .zip(std::mem::take(&mut self.damage))
            .filter(|(_, d)| d.lamp.is_none_or(|lamp| lamp.shown(rest)))
            .unzip();
        self.meshes = meshes;
        self.damage = damage;
        self.flares.clear();
        self
    }

    /// Adds the car's meshes, and its flares' meshes, and textures to `stage`, keeping the meshes sorted by layer.
    pub fn add_to_stage(self, stage: &mut Stage) -> CarReport {
        stage.meshes.extend(self.meshes);
        stage.meshes.extend(self.flares.into_iter().map(|flare| flare.mesh));
        stage.meshes.sort_by_key(|mesh| mesh.layer);
        stage.textures.extend(self.textures);
        self.report
    }
}

/// The sections of car `name`'s `<car>.ini`: its parts, and the `Cam_*` camera sections.
pub fn part_sections(data: &Data, name: &str) -> Result<Vec<Section>, Error> {
    read_ini(data, &entry(data, name)?.ini_file)
}

/// Car `name`'s `.ini` sections with the `[Cam_*]` sections of the `<ini>_personal.ini` beside it in their place:
/// RallySimFans keeps the player's camera places for a car there (`notes/formats.md` "RSF cars"). Whether RSF's plugin
/// reads it at load is not traced; ours takes it as the car's camera.
pub fn camera_sections(data: &Data, name: &str) -> Result<Vec<Section>, Error> {
    let ini_file = entry(data, name)?.ini_file;
    let mut sections = read_ini(data, &ini_file)?;
    let personal = ini_file.rsplit_once('.').map(|(stem, _)| format!("{stem}_personal.ini"));
    let over = personal.filter(|p| data.resolve(p).is_some()).map(|p| read_ini(data, &p)).transpose()?;
    for section in over.unwrap_or_default().into_iter().filter(|s| s.name.starts_with("Cam_")) {
        match sections.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&section.name)) {
            Some(old) => *old = section,
            None => sections.push(section),
        }
    }
    Ok(sections)
}

/// Car `name`'s `.sgc` model (the in-car glass reads its windscreen and wipers, `crate::glass`).
pub(crate) fn model(data: &Data, name: &str) -> Result<Vec<sgc::Element>, Error> {
    sgc::parse(&read_bytes(&model_file(data, name)?)?).map_err(Error::Sgc)
}

/// Where car `name`'s node or mesh `node` (ignoring case) sits in the model frame (`sgc::chained_worlds`), or `None`
/// without one.
pub fn node_place(data: &Data, name: &str, node: &str) -> Result<Option<Vec3>, Error> {
    let model = model(data, name)?;
    let chained = sgc::chained_worlds(&model);
    Ok(model
        .iter()
        .filter_map(|e| match e {
            sgc::Element::Mesh(m) => Some(&m.node),
            sgc::Element::Node(n) => Some(n),
            _ => None,
        })
        .find(|n| n.name.eq_ignore_ascii_case(node))
        .map(|n| coords::affine(chained.get(&n.id).copied().unwrap_or(n.world)).w_axis.truncate()))
}

/// Car `name`'s `.sgc` file.
pub(crate) fn model_file(data: &Data, name: &str) -> Result<PathBuf, Error> {
    resolve(data, &entry(data, name)?.file_name)
}

/// Car `name`'s `Cars/Cars.ini` entry (its folder, matched ignoring case), with `Data::with_car`'s car in its slot.
fn entry(data: &Data, name: &str) -> Result<cars::CarEntry, Error> {
    data.car_slots(cars::parse(&read_text(data, "Cars\\Cars.ini")?).map_err(Error::Cars)?)
        .into_iter()
        .find(|e| folder(&e.file_name).is_some_and(|f| f.eq_ignore_ascii_case(name)))
        .ok_or_else(|| Error::UnknownCar(name.to_owned()))
}

/// Why the mesh `name` is not drawn, or the part section it is drawn with.
/// Ours: an interior panel of a detachable part that the game leaves undented (no morph target: COROLLA's `i_door_l`
/// and `i_door_r`, rbr-recomp `notes/car-damage-visuals.md` section 6) dents with its exterior twin, each vertex as the nearest
/// vertex of the twin's largest damageable mesh. Both sets must be at the same placement.
pub fn follow_dents(interior: &[Mesh], interior_damage: &mut [MeshDamage], exterior: &[Mesh], damage: &[MeshDamage]) {
    for (mesh, effect) in interior.iter().zip(interior_damage) {
        let Some(part) = effect.detach.filter(|_| effect.dents.is_none() && effect.crack.is_none()) else {
            continue;
        };
        let twin = exterior
            .iter()
            .zip(damage)
            .filter(|(_, d)| d.detach == Some(part) && d.crack.is_none())
            .filter_map(|(m, d)| Some((m, d.dents.as_ref()?)))
            .max_by_key(|(m, _)| m.vertices.len());
        let Some((twin, dents)) = twin else { continue };
        // Each vertex's nearest over the machine's threads (the RSF Clio Rally3's cabin took 12 s on one).
        let vertices = in_parallel(&mesh.vertices, |v| {
            let p = Vec3::from(v.position);
            let nearest = twin.vertices.iter().enumerate().min_by(|(_, a), (_, b)| {
                p.distance_squared(Vec3::from(a.position)).total_cmp(&p.distance_squared(Vec3::from(b.position)))
            })?;
            dents.vertices.get(nearest.0).copied()
        });
        let vertices = vertices.into_iter().flatten().collect::<Vec<_>>();
        if vertices.len() == mesh.vertices.len() {
            effect.dents = Some(Dents { panel: dents.panel.clone(), vertices });
        }
    }
}

#[cfg(test)]
mod tests;

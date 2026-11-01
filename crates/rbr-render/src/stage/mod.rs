//! Loads one stage from `Maps/` into plain CPU data: meshes to draw, and the textures they use.
//!
//! Everything is read through `rbr-formats`: the `.lbs` file (geometry, object lanes, sky dome, clouds),
//! the stage `.ini` texture list, `TextureFilenameMap.dat`, the `*_textures.rbz` archives and their
//! `.dds` files. [`load`] reads the geometry and the texture names. [`Stage::load_textures`] reads the
//! texture files for one [`Selection`] of weather and wear, so the same geometry can be checked against
//! every selection.
//!
//! Each draw carries its render state ([`DrawState`]): which textures, the address modes, the second
//! texture blend, the alpha test, culling and depth. Sources are in `notes/formats.md` "Shaders and vertex layouts".
//! The texture chain: `notes/formats.md` "Stage geometry" and `notes/formats.md` "Textures and sprites".

use crate::atmosphere::Atmosphere;
use glam::Vec3;
use rbr_formats::{ini, lbs, stage_files, stage_textures, trk};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::particles::ground::Ground;

mod mesh;
mod textures;

use mesh::{
    block_mesh, camera_mesh, cell_bounds, face_mesh, interactive_mesh, object_mesh, sky_fields, start_pose, water_mesh,
};
use textures::{texture_name, texture_names};

pub use mesh::{
    Address, Bounds, Draw, DrawState, Image, Layer, Lod, Mesh, Shade, StageTexture, Vertex, CLOSE_DISTANCE, LOD_BAND,
    LOD_DISTANCE, MIP_MAP_BIAS,
};

/// Flat RGBA fill for water while its retail texture keys remain unresolved. This is a viewer stand-in,
/// not the game's water texture.
pub const WATER_STAND_IN: [u8; 4] = [64, 150, 184, 112];

/// The stage `viewer` opens when none is named.
pub const DEFAULT_STAGE: &str = "track-10_O";

/// Object items' alpha test: texels with `alpha <= OBJECT_ALPHA_REF / 255` are dropped (`alpha > 60/255`
/// passes, `notes/formats.md` "Shaders and vertex layouts", object lanes).
pub const OBJECT_ALPHA_REF: u8 = 60;
/// First runtime handle assigned to the `(1,6)` visibility records.
pub const INTERACTIVE_HANDLE_BASE: u32 = 0x9c40;

/// Weather folder of the stage textures (`<track>_textures\<weather>\`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    #[default]
    Dry,
    Damp,
    Wet,
}

impl Weather {
    pub const ALL: [Weather; 3] = [Weather::Dry, Weather::Damp, Weather::Wet];

    pub fn as_str(self) -> &'static str {
        match self {
            Weather::Dry => "dry",
            Weather::Damp => "damp",
            Weather::Wet => "wet",
        }
    }

    /// How wet the stage is: 0 dry, 0.5 damp, 1 wet (ours).
    pub fn wetness(self) -> f32 {
        match self {
            Weather::Dry => 0.0,
            Weather::Damp => 0.5,
            Weather::Wet => 1.0,
        }
    }

    /// Parses `dry`, `damp` or `wet` (any case).
    pub fn parse(text: &str) -> Option<Weather> {
        Weather::ALL.into_iter().find(|w| w.as_str().eq_ignore_ascii_case(text))
    }

    /// The weather folders a texture is read from, in order: this one, then the drier ones. Ours: the install lacks
    /// some `damp` and `wet` folders and files (the Arctic and Australia stages, track-4 and the stage and yard maps
    /// 71 to 89), and the reconstructed loader has no fallback. The game does not ask for them there: the season table
    /// gives wetness 0 to every row of those maps (`notes/gameplay.md` "Weather"; `tests/stage_data.rs` checks it). So a snow
    /// stage with only its `dry` set uses that set in every weather, and a `wet` request on a stage with only a `damp`
    /// set uses the `damp` one.
    pub fn fallbacks(self) -> &'static [Weather] {
        match self {
            Weather::Dry => &[Weather::Dry],
            Weather::Damp => &[Weather::Damp, Weather::Dry],
            Weather::Wet => &[Weather::Wet, Weather::Damp, Weather::Dry],
        }
    }
}

/// Wear folder of the ground textures (`<track>_textures\<weather>\<wear>\`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wear {
    #[default]
    New,
    Normal,
    Worn,
}

impl Wear {
    pub const ALL: [Wear; 3] = [Wear::New, Wear::Normal, Wear::Worn];

    pub fn as_str(self) -> &'static str {
        match self {
            Wear::New => "new",
            Wear::Normal => "normal",
            Wear::Worn => "worn",
        }
    }

    /// Parses `new`, `normal` or `worn` (any case).
    pub fn parse(text: &str) -> Option<Wear> {
        Wear::ALL.into_iter().find(|w| w.as_str().eq_ignore_ascii_case(text))
    }
}

/// The weather and wear whose texture folders are read, and the weather row's sky (`None`: picked by the weather).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    pub weather: Weather,
    pub wear: Wear,
    pub sky: Option<Sky>,
}

/// A season row's sky words (`viewer --sky`, the launcher's weather row): with the stage's map and letter they name its
/// `TrackSettings.ini` section (`rsf::WeatherRow`), its sun, light, clouds and fog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sky {
    pub cloud_letter: u32,
    pub preset: u32,
    pub cloud: u32,
}

impl Sky {
    /// The words of a section name such as `21O_E_heavyfog_heavycloud`.
    pub fn parse(section: &str) -> Option<Sky> {
        let id = section.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?;
        let row = rbr_formats::rsf::WeatherRow::parse(id, section)?;
        Some(Sky { cloud_letter: row.cloud_letter, preset: row.preset, cloud: row.cloud })
    }

    /// The section it names for stage `name` (`track-21_O`).
    pub fn section(self, name: &str) -> Option<String> {
        let (map, letter) = stage_files::parts(name)?;
        let letter = rbr_formats::rsf::LETTERS.iter().position(|&l| l == letter)? as u32;
        rbr_formats::rsf::WeatherRow::section_name(map, letter, self.cloud_letter, self.preset, self.cloud)
    }
}

/// Which texture source a reference uses: the stage, global clouds, a car's own textures
/// (`Cars\<car>\textures`, loaded by [`crate::car`]), a stage shadow stream, or a renderer-only fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextureSet {
    Stage,
    Clouds,
    Car,
    /// A stage shadow stream (`ShadowTexture<N>`), a single-channel light map.
    Shadow,
    /// The renderer's flat water fill; not a retail texture.
    WaterStandIn,
    /// A stage specular map (`SpecularTexture<N>`), for slots 6 and 8.
    Specular,
    /// A stage character's texture (`SGS\textures`, loaded by [`crate::characters`]).
    Character,
    /// A fence texture (`Textures/fence.dat`, named by the stage's `.fnc`; [`crate::fences`]).
    Fence,
}

/// A texture index in one of the texture sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureRef {
    pub set: TextureSet,
    pub index: u32,
}

impl TextureRef {
    fn stage(index: u32) -> TextureRef {
        TextureRef { set: TextureSet::Stage, index }
    }

    fn clouds(index: u32) -> TextureRef {
        TextureRef { set: TextureSet::Clouds, index }
    }

    fn shadow(index: u32) -> TextureRef {
        TextureRef { set: TextureSet::Shadow, index }
    }

    fn specular(index: u32) -> TextureRef {
        TextureRef { set: TextureSet::Specular, index }
    }

    fn water_stand_in() -> TextureRef {
        TextureRef { set: TextureSet::WaterStandIn, index: 0 }
    }
}

/// Where a mesh is placed: at its stored position, or moved with the camera. The global clouds are drawn
/// at infinity with the view's rotation only, so they never move with the camera's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    World,
    Camera,
}

/// A loaded stage: its meshes, the texture names it uses, and (after [`Stage::load_textures`]) the
/// texture files for one selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    pub name: String,
    /// Weather and wear: picks the global sky and the texture folders.
    pub selection: Selection,
    /// Sorted by layer: sky, solid, then blended (drawn in this order).
    pub meshes: Vec<Mesh>,
    /// `(1,6)` interactive objects expanded once per visibility record. Their vertices stay in object space so the
    /// viewer can replace the load placement with a live solid pose.
    pub interactive: Vec<InteractiveInstance>,
    /// Track name, file name and ground flag of each texture a draw uses.
    pub texture_names: BTreeMap<TextureRef, TextureName>,
    /// Texture files for `selection`, from [`Stage::load_textures`].
    pub textures: BTreeMap<TextureRef, StageTexture>,
    /// `Textures\start_clock_digits.dds` when a draw shows the start board's clock face ([`crate::start_board`]),
    /// from [`Stage::load_textures`].
    pub start_digits: Option<Vec<u8>>,
    /// The off-road ground is snow's ([`crate::puddles::mark_ground`]); the far ground's calming leaves it alone.
    pub snow_ground: bool,
    /// The collision ground for `selection` (`.col`, and the `.mat` entry for its weather), when marking the surfaces
    /// built it ([`load`] in the wet, [`Stage::mark_surfaces`]), for the particles to take rather than build again
    /// ([`crate::particles::Drawn::load`]).
    pub collision: Option<Arc<Ground>>,
    /// The car start location, if the stage has one.
    pub start: Option<Pose>,
    /// The light and haze (`Maps/TrackSettings.ini`), a renderer choice of section (`atmosphere`).
    pub atmosphere: Atmosphere,
    /// The driveline's points (`.trk`), in stage space; empty without one. Ours: the renderer bakes its reflection
    /// probes along it (`renderer::probes`).
    pub driveline: Vec<Vec3>,
    pub report: LoadReport,
}

/// A texture's name in its set: the track whose archive holds it, the `[TextureInfo]` file name, and
/// whether it is a ground texture (wear folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureName {
    pub track: String,
    pub file_name: String,
    pub ground: bool,
}

/// A position and a unit heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub position: Vec3,
    pub forward: Vec3,
}

/// One `(1,6)` visibility record and its draw data. `handle` is `0x9c40 +` the record's file-order index.
#[derive(Debug, Clone, PartialEq)]
pub struct InteractiveInstance {
    pub handle: u32,
    /// The load-time object-to-world matrix, as four columns in the `.lbs` record.
    pub placement: [f32; 16],
    /// Local vertices and draws for this instance.
    pub mesh: Arc<Mesh>,
    /// The visibility cell its position lies in ([`Mesh::cell`]).
    pub cell: Option<Bounds>,
}

/// One pose from the awake physics list. The live channel serializes these as three position and four
/// quaternion floats after the `handle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InteractivePose {
    pub handle: u32,
    pub position: [f32; 3],
    pub orientation: [f32; 4],
}

/// The original assigns each `.lbs` visibility record a runtime handle in file order.
pub fn interactive_handle(index: u32) -> u32 {
    INTERACTIVE_HANDLE_BASE + index
}

/// The file-order visibility index for an interactive solid handle.
pub fn interactive_record_index(handle: u32) -> Option<u32> {
    handle.checked_sub(INTERACTIVE_HANDLE_BASE)
}

/// The stage's map number from its name (`track-33_N` is map 33, also in a stage folder's path).
pub fn map_number(stage: &str) -> Option<u32> {
    stage_files::parts(stage).map(|(map, _)| map)
}

/// What loading skipped or could not find. Counts only: loading never stops on bad data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Requests for textures with no readable `.dds`, for the selection last loaded.
    pub missing_textures: Vec<String>,
    /// Requests for textures read from a drier weather's folder, because the stage's own folder lacks them
    /// ([`Weather::fallbacks`]).
    pub fallback_textures: Vec<String>,
    /// Triangles with an index outside its vertex buffer, or outside the draw's range. Dropped.
    pub bad_triangles: usize,
    /// Render chunks on a slot with no known vertex layout, or on an empty view. Dropped.
    pub skipped_draws: usize,
    /// Water items read from `(1,8)`.
    pub water_items_parsed: usize,
    /// Water items emitted as stand-in draws.
    pub water_items_drawn: usize,
    /// Water items that had no usable triangles and were withheld.
    pub water_items_not_drawn: usize,
    /// Reflection items read from `(1,7)`.
    pub reflection_items_parsed: usize,
    /// Reflection items drawn (zero while their runtime texture and state remain unresolved).
    pub reflection_items_drawn: usize,
    /// Reflection items withheld because their runtime texture and render state remain unresolved.
    pub reflection_items_not_drawn: usize,
    /// Named objects parsed from `(1,6)`.
    pub interactive_objects_parsed: usize,
    /// Interactive geometry records parsed from `(1,6)`.
    pub interactive_draw_items_parsed: usize,
    /// Interactive records emitted as meshes.
    pub interactive_draw_items_drawn: usize,
    /// Parsed interactive records withheld because their flags, vertex layout or texture keys are unresolved.
    pub interactive_draw_items_not_drawn: usize,
    /// Visibility records parsed from `(1,6)`.
    pub interactive_instances_parsed: usize,
    /// Per-instance meshes emitted from supported `(1,6)` objects.
    pub interactive_instances_drawn: usize,
    /// Instances withheld with an unsupported object draw record.
    pub interactive_instances_withheld: usize,
    /// Number of withheld instances by `.lbs` object name.
    pub interactive_withheld_by_class: BTreeMap<String, usize>,
}

/// Load failure. Bad texture data is a report entry, not an error.
#[derive(Debug)]
pub enum Error {
    Io { path: PathBuf, source: std::io::Error },
    Lbs(lbs::Error),
    Ini { path: PathBuf, line: usize, message: &'static str },
    FilenameMap(stage_textures::Error),
    NoGeometry,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Lbs(e) => write!(f, "lbs: {e}"),
            Error::Ini { path, line, message } => write!(f, "{}:{line}: {message}", path.display()),
            Error::FilenameMap(e) => write!(f, "TextureFilenameMap.dat: {e}"),
            Error::NoGeometry => write!(f, "no geometry chunk"),
        }
    }
}

impl std::error::Error for Error {}

impl Stage {
    /// Draws over all meshes.
    pub fn draw_count(&self) -> usize {
        self.meshes.iter().map(|m| m.draws.len()).sum()
    }

    /// Triangles over all draws.
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().flat_map(|m| &m.draws).map(|d| d.indices.len() / 3).sum()
    }
}

#[cfg(test)]
mod interactive_tests {
    use super::*;

    #[test]
    fn handles_map_to_visibility_records_in_file_order() {
        assert_eq!(interactive_record_index(INTERACTIVE_HANDLE_BASE - 1), None);
        for index in [0, 1, 35, 13_702] {
            assert_eq!(interactive_record_index(interactive_handle(index)), Some(index));
        }
    }
}

/// The global clouds set (`Tracks.ini` `[General]` `CloudsFilename`), in `Maps`.
const CLOUDS_TRACK: &str = "Clouds";

/// Loads stage `name` (for example `track-10_O`, or a stage folder's path, `rbr_formats::stage_files`) from `maps`
/// for `selection`: its geometry, object lanes, sky faces and the global clouds, and the names of the textures they
/// use. Weather picks the clouds and wear picks the ground textures. No texture file is read yet
/// ([`Stage::load_textures`]).
pub fn load(maps: &Path, name: &str, selection: Selection) -> Result<Stage, Error> {
    let lbs_bytes = read(&stage_files::file_in(maps, name, "lbs"))?;
    // Some RSF stages (299, 300) keep their geometry, object lanes and visibility entries in a `.lb2` beside the
    // `.lbs`, framed as `.lbs` chunks; RSF's plugin loads them from there (its `lbsloading.log`).
    let lb2_path = stage_files::file_in(maps, name, "lb2");
    let lb2_bytes = if lb2_path.is_file() { read(&lb2_path)? } else { Vec::new() };
    let mut lbs = lbs::parse(&lbs_bytes).map_err(Error::Lbs)?;
    lbs.chunks.extend(lbs::parse(&lb2_bytes).map_err(Error::Lbs)?.chunks);
    let sections = ini_sections(&stage_files::file_in(maps, name, "ini"))?;
    let track = stage_files::name(name);

    let geometry = lbs
        .chunks
        .iter()
        .find_map(|chunk| match chunk {
            lbs::Chunk::Geometry(geometry) => Some(geometry),
            _ => None,
        })
        .ok_or(Error::NoGeometry)?;
    let start = lbs.chunks.iter().find_map(|chunk| match chunk {
        lbs::Chunk::StartLocation(location) => start_pose(location),
        _ => None,
    });

    // The visibility cells: cell `i` holds block `i` and object lane `i` (`crate::visibility`).
    let cells: Vec<Bounds> = lbs
        .chunks
        .iter()
        .find_map(|chunk| match chunk {
            lbs::Chunk::Visibility(visibility) => Some(visibility.entries.iter().map(cell_bounds).collect()),
            _ => None,
        })
        .unwrap_or_default();

    let mut report = LoadReport::default();
    let mut meshes: Vec<Mesh> = geometry.blocks.iter().map(|block| block_mesh(block, &mut report)).collect();
    for (mesh, cell) in meshes.iter_mut().zip(&cells) {
        mesh.cell = Some(*cell);
    }
    let mut interactive_instances = Vec::new();
    let mut interactive_index = 0u32;
    for chunk in &lbs.chunks {
        match chunk {
            lbs::Chunk::Objects(objects) => {
                for (index, lane) in objects.lanes.iter().enumerate() {
                    for item in lane.runs.iter().flatten() {
                        meshes.push(Mesh { cell: cells.get(index).copied(), ..object_mesh(item, &mut report) });
                    }
                }
            }
            lbs::Chunk::Interactive(interactive) => {
                report.interactive_objects_parsed += interactive.objects.len();
                // Each record shows while the cell its position lies in does (`visibility::cell_at`).
                for object in &interactive.objects {
                    report.interactive_instances_parsed += object.visibility.len();
                    let count = object.draws.len();
                    report.interactive_draw_items_parsed += count;
                    let supported = !object.draws.is_empty()
                        && object.draws.iter().all(|draw| {
                            draw.flags == 0x15
                                && draw.vertex_size == 24
                                && draw
                                    .texture0
                                    .is_some_and(|key| texture_name(&sections, track, "Texture", key).is_some())
                                && draw.texture1.is_none()
                        });
                    if !supported {
                        report.interactive_draw_items_not_drawn += count;
                        let class = String::from_utf8_lossy(object.name).into_owned();
                        report.interactive_instances_withheld += object.visibility.len();
                        *report.interactive_withheld_by_class.entry(class).or_default() += object.visibility.len();
                        interactive_index += object.visibility.len() as u32;
                        continue;
                    }
                    let mesh = Arc::new(interactive_mesh(object, &mut report));
                    if mesh.draws.is_empty() {
                        report.interactive_draw_items_not_drawn += count;
                        let class = String::from_utf8_lossy(object.name).into_owned();
                        report.interactive_instances_withheld += object.visibility.len();
                        *report.interactive_withheld_by_class.entry(class).or_default() += object.visibility.len();
                        interactive_index += object.visibility.len() as u32;
                        continue;
                    }
                    report.interactive_draw_items_drawn += count;
                    for visibility in &object.visibility {
                        let position = Vec3::from_slice(&visibility.matrix[12..15]);
                        interactive_instances.push(InteractiveInstance {
                            handle: interactive_handle(interactive_index),
                            placement: visibility.matrix,
                            cell: crate::visibility::cell_at(&cells, position).copied(),
                            mesh: Arc::clone(&mesh),
                        });
                        report.interactive_instances_drawn += 1;
                        interactive_index += 1;
                    }
                }
            }
            lbs::Chunk::Water(water) => {
                for item in water.faces.iter().flat_map(|face| &face.items) {
                    report.water_items_parsed += 1;
                    let mesh = water_mesh(item, &mut report);
                    if mesh.draws.is_empty() {
                        report.water_items_not_drawn += 1;
                    } else {
                        report.water_items_drawn += 1;
                    }
                    meshes.push(mesh);
                }
            }
            lbs::Chunk::Reflection(reflection) => {
                let count: usize = reflection.faces.iter().map(|face| face.items.len()).sum();
                report.reflection_items_parsed += count;
                // Block17 builds and submits these as reflection draw items, but its runtime texture
                // lookup and render-state values are not resolved here. Keep every item counted and
                // withhold it until those inputs are proven.
                report.reflection_items_not_drawn += count;
            }
            lbs::Chunk::Sky(sky) => {
                meshes.extend(sky.items.iter().map(|item| face_mesh(item, &mut report)));
            }
            _ => {}
        }
    }
    // The clouds go first: the capture draws them right after the sky dome, before the sky faces and the
    // stage, with no depth test, so everything drawn later covers them (`global_clouds`).
    let sky = selection.sky.and_then(|sky| sky.section(name));
    // Ours: in the wet and the damp the roads take the film by their surface ([`crate::puddles::mark_roads`]); dry, only
    // the road scans need them ([`Stage::mark_surfaces`]).
    let wet = selection.weather != Weather::Dry;
    let collision = wet.then(|| collision(maps, name, selection)).flatten();
    if wet {
        if let Some(marking) = marking(collision.as_ref(), maps, name, selection) {
            let triangles = crate::puddles::ground_triangles(&meshes, &marking);
            crate::puddles::mark_roads(&mut meshes, &triangles);
        }
    }
    let atmosphere = Atmosphere::load(maps, name, selection.weather.as_str(), sky.as_deref());
    meshes.splice(0..0, global_clouds(maps, selection.weather, atmosphere.cloud_name.as_deref(), &mut report)?);
    meshes.sort_by_key(|mesh| mesh.layer);

    let texture_names = texture_names(maps, track, &sections, &meshes, &interactive_instances)?;
    Ok(Stage {
        name: name.to_owned(),
        selection,
        meshes,
        interactive: interactive_instances,
        texture_names,
        textures: BTreeMap::new(),
        start_digits: None,
        snow_ground: false,
        collision,
        start,
        atmosphere,
        driveline: driveline(maps, name),
        report,
    })
}

/// Stage `name`'s collision ground for `selection` ([`Stage::collision`]).
fn collision(maps: &Path, name: &str, selection: Selection) -> Option<Arc<Ground>> {
    crate::particles::load_ground(maps, name, selection).map(Arc::new)
}

/// The collision ground to mark the surfaces by: the `weather`'s, or the dry entry's where the `.mat` has none for the
/// weather (460M): only the kind of surface counts.
pub(crate) fn marking(
    weather: Option<&Arc<Ground>>,
    maps: &Path,
    name: &str,
    selection: Selection,
) -> Option<Arc<Ground>> {
    let dry = || collision(maps, name, Selection { weather: Weather::Dry, ..selection });
    weather.filter(|g| g.has_surfaces()).cloned().or_else(dry)
}

impl Stage {
    /// Marks the roads in dry weather too when `roads`, for the road scans (`Enhance::gravel`, `asphalt`, `snow`), which
    /// take the loose and the paved ones and the snow by [`Vertex::wet`] (in the wet and the damp [`load`] has; kept out
    /// of the dry load without them: about a second more on the larger stages, 621); then the off-road ground when
    /// `ground`, for the ground scan (`Enhance::ground`, [`crate::puddles::mark_ground`]). One load of the collision
    /// ground and one lookup of the surface under each ground triangle for both: each its own took 0.6 s more on 580.
    pub fn mark_surfaces(&mut self, maps: &Path, roads: bool, ground: bool) {
        let roads = roads && self.selection.weather == Weather::Dry;
        if !(roads || ground) {
            return;
        }
        if self.collision.is_none() {
            self.collision = collision(maps, &self.name, self.selection);
        }
        let Some(marking) = marking(self.collision.as_ref(), maps, &self.name, self.selection) else { return };
        let triangles = crate::puddles::ground_triangles(&self.meshes, &marking);
        if roads {
            crate::puddles::mark_roads(&mut self.meshes, &triangles);
        }
        if ground {
            self.snow_ground = crate::puddles::mark_ground(&mut self.meshes, &triangles);
        }
    }
}

/// The points of `name`'s driveline (`.trk`, physics axes) in stage space; none when it does not load.
fn driveline(maps: &Path, name: &str) -> Vec<Vec3> {
    let Some(trk) = std::fs::read(stage_files::file_in(maps, name, "trk")).ok().and_then(|b| trk::parse(&b).ok())
    else {
        return Vec::new();
    };
    let records = trk.chunks.into_iter().find_map(|c| match c.payload {
        trk::ChunkPayload::DriveLine(d) => Some(d.records),
        _ => None,
    });
    records.unwrap_or_default().iter().map(|r| Vec3::new(r.vectors[0].x, r.vectors[0].z, r.vectors[0].y)).collect()
}

/// The global cloud faces for `weather` and the section's `CloudName`, camera-anchored (drawn at infinity). Each
/// face is one item of a `Clouds.lbs` chunk ([`cloud_shows`]). They are in the sky layer: the capture draws the
/// clouds after the sky dome and before the stage, without depth test or write, so the stage covers them.
///
/// The global sky dome in `skyboxes.lbs` is not drawn: its vertex colour is unused, because `SkyDome`
/// computes the colour from the scattering constants (`c41`-`c66`), which are not reproduced yet.
fn global_clouds(
    maps: &Path,
    weather: Weather,
    cloud_name: Option<&str>,
    report: &mut LoadReport,
) -> Result<Vec<Mesh>, Error> {
    let mut meshes = Vec::new();
    let clouds_bytes = read(&maps.join(format!("{CLOUDS_TRACK}.lbs")))?;
    let clouds = lbs::parse(&clouds_bytes).map_err(Error::Lbs)?;
    for chunk in &clouds.chunks {
        let lbs::Chunk::Clouds(face) = chunk else {
            continue;
        };
        if !cloud_shows(&face_stem(face.name), weather, cloud_name) {
            continue;
        }
        for item in &face.items {
            let state = DrawState {
                textures: [item.texture0.map(TextureRef::clouds), item.texture1.map(TextureRef::clouds)],
                shadow: None,
                address: [Address::Clamp; 2],
                second_texture: item.texture1.is_some(),
                alpha_ref: Some(0),
                mip_bias: false,
                cull_back: false,
                blend: true,
                depth_test: false,
                depth_write: false,
                shade: None,
                specular: None,
            };
            meshes.push(camera_mesh(&item.buffers, sky_fields(item.flags), Layer::Sky, state, report));
        }
    }
    Ok(meshes)
}

/// The upper-case file stem of a face name: `MAPS/GLOBAL/SKYBOXES NEW TECH/US_CLEAR.KSH` gives `US_CLEAR`.
fn face_stem(name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    stage_textures::stem(&name).to_ascii_uppercase()
}

/// Whether the cloud face `stem` is drawn: the one the stage's `TrackSettings.ini` section names (`CloudName`,
/// for example `Us_precipitation02_pc` for the face `US_PRECIPITATION02_PC`; the names match, the game's lookup is
/// not traced), else, with no section, a guess from the face names for `weather`.
fn cloud_shows(stem: &str, weather: Weather, cloud_name: Option<&str>) -> bool {
    if let Some(name) = cloud_name {
        return stem.eq_ignore_ascii_case(name);
    }
    let faces: &[&str] = match weather {
        Weather::Dry => &["US_CLEAR"],
        Weather::Damp => &["US_CLOUDED", "US_PARTLY_CLOUDED"],
        Weather::Wet => &["US_OVERCAST", "US_PRECIPITATION"],
    };
    faces.contains(&stem)
}

fn ini_sections(path: &Path) -> Result<Vec<ini::Section>, Error> {
    let bytes = read(path)?;
    ini::parse(&String::from_utf8_lossy(&bytes)).map_err(|e| Error::Ini {
        path: path.to_owned(),
        line: e.line,
        message: e.message,
    })
}

fn read(path: &Path) -> Result<Vec<u8>, Error> {
    std::fs::read(path).map_err(|source| Error::Io { path: path.to_owned(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_numbers_come_from_stage_names() {
        assert_eq!(map_number("track-33_N"), Some(33));
        assert_eq!(map_number("track-4_E"), Some(4));
        assert_eq!(map_number("tanner"), None);
        assert_eq!(map_number("/rsf/Maps/101-Autiovaara II/track-101_O"), Some(101));
    }

    #[test]
    fn a_sky_names_its_section_for_the_stage() {
        let sky = Sky::parse("21O_E_heavyfog_heavycloud").unwrap();
        assert_eq!(sky.section("track-21_O").as_deref(), Some("21O_E_heavyfog_heavycloud"));
        // The same words on another variant or map name that one's section; the cloud letter only on O and S.
        assert_eq!(sky.section("/rsf/Maps/129-Peyregrosse/track-129_O").as_deref(), Some("129O_E_heavyfog_heavycloud"));
        assert_eq!(sky.section("track-21_M").as_deref(), Some("21M_heavyfog_heavycloud"));
        assert_eq!(Sky::parse("54E_crisp_clear").unwrap().section("track-54_E").as_deref(), Some("54E_crisp_clear"));
        assert_eq!(Sky::parse("21O_E_sunny_heavycloud"), None);
        assert_eq!(sky.section("tanner"), None);
    }

    #[test]
    fn selection_names_parse_in_any_case() {
        assert_eq!(Weather::parse("Wet"), Some(Weather::Wet));
        assert_eq!(Wear::parse("NORMAL"), Some(Wear::Normal));
        assert_eq!(Weather::parse("snow"), None);
        assert_eq!(Wear::parse("used"), None);
    }

    #[test]
    fn levels_of_detail_follow_the_capture() {
        // Frames 175 and 250 of the capture (track-54_E, `RenderQuality = low`, its switch at 200 m): blocks up to
        // 128 m from the eye drew only their near chunks, 187 and 198 m both, 203-428 m only the far ones; list 2 at
        // every distance. The same rule at high's 600 m.
        let shown = |d: f32| (Lod::Near.shows(d), Lod::Far.shows(d), Lod::Always.shows(d));
        let high = LOD_DISTANCE - 200.0;
        assert_eq!(shown(high + 128.0), (true, false, true));
        assert_eq!(shown(high + 187.0), (true, true, true));
        assert_eq!(shown(high + 198.0), (true, true, true));
        assert_eq!(shown(high + 203.0), (false, true, true));
        assert_eq!(shown(high + 428.0), (false, true, true));
        assert_eq!([1, 2, 3].map(Lod::of_list), [Lod::Near, Lod::Always, Lod::Far]);
    }

    #[test]
    fn the_section_names_the_cloud_face() {
        assert!(cloud_shows("US_PRECIPITATION02_PC", Weather::Wet, Some("Us_precipitation02_pc")));
        assert!(!cloud_shows("US_PRECIPITATION", Weather::Wet, Some("Us_precipitation02_pc")));
        assert!(!cloud_shows("US_OVERCAST", Weather::Wet, Some("Us_precipitation02_pc")));
        assert!(cloud_shows("US_CLEAR02", Weather::Wet, Some("Us_clear02")));
        // No section: the weather's faces.
        assert!(cloud_shows("US_OVERCAST", Weather::Wet, None));
        assert!(cloud_shows("US_PRECIPITATION", Weather::Wet, None));
        assert!(!cloud_shows("US_CLEAR", Weather::Wet, None));
        assert!(cloud_shows("US_CLEAR", Weather::Dry, None));
    }
}

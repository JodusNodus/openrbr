//! Loading the stage characters: the `.lbs` `(2, 14)` items and `(2, 15)` groups, their load scripts, the models and
//! clips the scripts name, and the meshes added to the stage (`notes/stage-characters.md`, sections 1 and 2).

use super::anim::{Clip, Skeleton, Skin};
use super::random;
use super::script::{Host, Lua, Value};
use crate::stage::{
    Address, Anchor, Bounds, Draw, DrawState, Image, Layer, Lod, Mesh, Shade, Stage, StageTexture, TextureRef,
    TextureSet, Vertex,
};
use glam::Vec3;
use rbr_formats::data::Data;
use rbr_formats::lua::{self, Chunk};
use rbr_formats::{dds, ini, lbs, pe, rbz, sga, sgc, sgs};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

/// `E01FrontendName` (0x43cd60): the `.data` table of script folders, one per ten maps (`"arctic"` for 10-19).
const RALLY_FOLDERS: u32 = 0x007C_03DC;
/// The table's last rally entry (`rallyschool`, maps 70-79); the entries after it are not folders.
const RALLY_FOLDER_LAST: u32 = 7;

/// Lighting of the character faces (ours: the `Texture` shader the models name is not traced). Lit like the car by
/// the section's ambient and sun, without a highlight.
const SHADE: Shade = Shade {
    ka: 1.0,
    kd: 0.8,
    ks: 0.0,
    m: 0.3,
    fresnel_min: 0.0,
    fresnel_max: 0.0,
    kf: 0.0,
    glass: false,
    glow: false,
    crack: false,
    dirt: None,
    grime: 0,
};
/// Alpha test of faces with an opacity map (ours, as the object lanes').
const ALPHA_REF: u8 = 60;

/// What a load script asked for (`app:LoadCharacter`, `app:LoadAnimation`, `app:AddAnimationToCharacter`).
#[derive(Debug, Default)]
struct Requests {
    model: Option<[String; 3]>,
    animations: HashMap<String, String>,
    added: Vec<String>,
    unknown: BTreeSet<String>,
}

/// `LuaRandom` and the `app` calls during a load script. `LuaRandom(a, b)` is the exe's ([`random::Lua`]).
struct LoadHost<'a> {
    random: &'a mut random::Lua,
    /// [`Options::birdstrike`].
    birdstrike: bool,
    requests: Requests,
}

impl Host for LoadHost<'_> {
    fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, String> {
        let text = |i: usize| args.get(i).and_then(Value::text).unwrap_or_default();
        match function {
            "LuaRandom" => {
                // `tolua_getnumber` to int (`_ftol`, truncation).
                let lo = args.first().and_then(Value::number).unwrap_or(1.0) as i32;
                let hi = args.get(1).and_then(Value::number).unwrap_or(1.0) as i32;
                let value = self.random.range(lo, hi);
                if self.birdstrike && (lo, hi) == (1, 200) {
                    return Ok(vec![Value::Number(1.0)]);
                }
                return Ok(vec![Value::Number(f64::from(value))]);
            }
            "app:LoadCharacter" | "app:LoadCharacterNoCull" => self.requests.model = Some([text(0), text(1), text(2)]),
            "app:LoadAnimation" => {
                self.requests.animations.insert(text(1), text(0));
            }
            "app:AddAnimationToCharacter" => self.requests.added.push(text(1)),
            other => {
                self.requests.unknown.insert(other.to_owned());
            }
        }
        Ok(Vec::new())
    }
}

/// A loaded model: its node tree and its skinned meshes.
#[derive(Debug)]
pub struct Model {
    pub skeleton: Skeleton,
    pub meshes: Vec<ModelMesh>,
}

/// One skinned mesh of a model: the rest vertices and skin, and the stage mesh it is drawn with (its positions
/// replaced every frame).
#[derive(Debug)]
pub struct ModelMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub weights: Vec<sgs::SkinWeight>,
    pub skin: Skin,
    /// The skeleton node named as the mesh's own node (`Driver` for the driver), which unskinned vertices follow.
    pub node: Option<usize>,
    pub template: Mesh,
}

/// One placed character.
#[derive(Debug, Clone)]
pub struct Spawn {
    pub id: u32,
    pub key: String,
    pub load: String,
    pub run: Option<String>,
    pub position: Vec3,
    pub degrees: Vec3,
    pub scale: f32,
    pub model: usize,
    /// Clip indices in `AddAnimationToCharacter` order.
    pub clips: Vec<usize>,
    pub role: Role,
}

/// How the stage's items fared.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub items: usize,
    pub spawned: usize,
    /// Items left out by their random group (`PickRand`, kind 1).
    pub other_variant: usize,
    /// Items whose `Load` returned other than 0 (`load_birdstrike` 199 times in 200).
    pub declined: usize,
    /// Characters of the scripted events: bird strikes, car-flip spectators and the rescue helicopter.
    pub scripted: usize,
    /// Items that failed to load, with the reason.
    pub failed: BTreeMap<String, usize>,
    /// Host functions the scripts called that are not modelled.
    pub unknown_calls: BTreeSet<String>,
}

/// Everything the runtime needs.
pub struct Loaded {
    pub models: Vec<Model>,
    pub clips: Vec<Clip>,
    pub spawns: Vec<Spawn>,
    pub runs: HashMap<String, Chunk>,
    pub report: Report,
    /// `rand()` after the load's draws, for the scripted events.
    pub crt: random::CrtRand,
    /// The stage's visibility cells.
    pub cells: Vec<super::cells::Cell>,
}

/// The scripts archive (the 1.02 patch's when present), by lowercase path.
pub(super) struct Scripts {
    files: HashMap<String, Vec<u8>>,
    parsed: HashMap<String, Option<Arc<Chunk>>>,
}

impl Scripts {
    pub(super) fn open(data: &Data) -> Result<Scripts, String> {
        let path =
            data.resolve("Scripts.rbz").or_else(|| data.resolve("Root/Scripts.rbz")).ok_or("Scripts.rbz: not found")?;
        let mut zip = rbz::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut files = HashMap::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().map_err(|e| e.to_string())?.to_ascii_lowercase();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            files.insert(name.replace('\\', "/"), bytes);
        }
        Ok(Scripts { files, parsed: HashMap::new() })
    }

    /// Adds the `.lua` files of `dir` as `scripts/<file>`, over the archive's: a community stage's own scripts
    /// (RallySimFans's `AnimationPath`, `<stage folder>/CustomData/Scripts`; ours, RSF's lookup is not traced).
    fn overlay(&mut self, dir: &Path) {
        for path in std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()) {
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_ascii_lowercase) else { continue };
            if let (true, Ok(bytes)) = (name.ends_with(".lua"), std::fs::read(&path)) {
                self.files.insert(format!("scripts/{name}"), bytes);
            }
        }
    }

    /// `E01ResolveA` (0x43cdd0): `scripts/<folder>/<file>` when it exists, else `scripts/<file>`. Only the item path's
    /// file name counts (`CArchivePath::m_file`), so `.../SCRIPTS/NEW/X.LUA` reads `scripts/x.lua` too.
    fn resolve(&self, folder: &str, item_path: &str) -> Option<String> {
        let file = item_path.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
        let candidates = [format!("scripts/{folder}/{file}"), format!("scripts/{file}")];
        candidates.into_iter().find(|p| self.files.contains_key(p))
    }

    pub(super) fn chunk(&mut self, path: &str) -> Option<Arc<Chunk>> {
        if let Some(parsed) = self.parsed.get(path) {
            return parsed.clone();
        }
        let parsed = self.files.get(path).and_then(|bytes| lua::parse(bytes).ok()).map(Arc::new);
        self.parsed.insert(path.to_owned(), parsed.clone());
        parsed
    }
}

/// The rally's script folder for map `map`, read from the exe (`RBR_PATCH`'s 1.02 exe, else the install's). Empty
/// when the exe or the entry is missing.
fn rally_folder(data: &Data, map: u32) -> String {
    let index = map / 10;
    if index > RALLY_FOLDER_LAST {
        return String::new();
    }
    let image = data
        .resolve("RichardBurnsRally_SSE.exe")
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| pe::parse(&bytes).ok());
    let Some(image) = image else { return String::new() };
    let Some(at) = image.u32(RALLY_FOLDERS + 4 * index) else { return String::new() };
    let mut name = Vec::new();
    for offset in 0..64 {
        match image.read(at + offset, 1) {
            Some([0]) | None => break,
            Some(&[b]) => name.push(b),
            Some(_) => break,
        }
    }
    String::from_utf8_lossy(&name).into_owned()
}

/// Load options (ours, for checks and captures).
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// `load_birdstrike.lua`'s `LuaRandom(1, 200)` returns 1, so every bird-strike item loads (the draw is still
    /// made, so the other draws stay the game's).
    pub birdstrike: bool,
    /// The race start's `time(NULL)` that seeds `rand()` and `LuaRandom` ([`random`]; ours: a fixed default).
    pub seed: u32,
}

/// Loads the characters of stage `name` and adds their meshes and textures to `stage`: the stage's items, then the
/// characters the exe loads itself (the car-flip spectators and the rescue helicopter, [`Role`]).
pub fn load(data: &Data, name: &str, stage: &mut Stage, options: Options) -> Result<Loaded, String> {
    let lbs_path = rbr_formats::stage_files::file_in(&data.install().join("Maps"), name, "lbs");
    let bytes = std::fs::read(&lbs_path).map_err(|e| format!("{}: {e}", lbs_path.display()))?;
    let parsed = lbs::parse(&bytes).map_err(|e| format!("{}: {e}", lbs_path.display()))?;
    let (mut items, mut groups) = (Vec::new(), Vec::new());
    for chunk in &parsed.chunks {
        match chunk {
            lbs::Chunk::AnimationObjects(objects) => items.extend(objects.items.iter().cloned()),
            lbs::Chunk::ContainerObjects(containers) => groups.extend(containers.items.iter().map(|c| c.dwords)),
            _ => {}
        }
    }
    let map = crate::stage::map_number(name).unwrap_or(0);
    let mut scripts = Scripts::open(data)?;
    if let Some(folder) = rbr_formats::stage_files::folder(name) {
        if let Some(path) = rbr_formats::rsf::open(folder).ok().and_then(|s| s.animation_path) {
            scripts
                .overlay(&rbr_formats::stage_files::in_dir(folder, &path).unwrap_or(folder.join(path)).join("Scripts"));
        }
    }
    let mut loader = Loader {
        folder: rally_folder(data, map),
        scripts,
        crt: random::CrtRand::srand(options.seed),
        random: random::Lua::new(options.seed),
        lua: Lua::new(),
        assets: Assets::new(data),
        options,
        loaded: Loaded {
            models: Vec::new(),
            clips: Vec::new(),
            spawns: Vec::new(),
            runs: HashMap::new(),
            report: Report::default(),
            crt: random::CrtRand::srand(options.seed),
            cells: super::cells::cells(&parsed),
        },
    };
    // `PickRand` (0x445c70): each group's `{id, kind, rand() % count}`.
    let picks: HashMap<u32, (u32, u32)> =
        groups.iter().map(|&[id, kind, count]| (id, (kind, loader.crt.draw() as u32 % count.max(1)))).collect();
    // Ours: `rootpath` is set by the exe (its value is not traced); the scripts' paths are install-relative with it
    // empty. `startup.lua` defines `true` and `false` (Lua 4.0 has no booleans).
    loader.lua.set_global("rootpath", Value::Str(String::new()));
    loader.lua.set_global("true", Value::Number(1.0));
    loader.lua.set_global("false", Value::Number(0.0));

    for item in &items {
        loader.loaded.report.items += 1;
        let [id, variant, group] = item.dwords;
        // The stage loader (0x445eb0), before each item's load: an item of a kind-2 group draws `LuaRandom` without
        // repeats in its group.
        match picks.get(&group) {
            Some(&(2, _)) => loader.random.select(group),
            _ => loader.random.clear(),
        }
        // An item loads when its group is missing, not kind 1, or picked this variant.
        if let Some(&(kind, pick)) = picks.get(&group) {
            if kind == 1 && variant != pick {
                loader.loaded.report.other_variant += 1;
                continue;
            }
        }
        let load_name = String::from_utf8_lossy(item.material_a).into_owned();
        let run_name = String::from_utf8_lossy(item.material_b).into_owned();
        let [position, degrees, ..] = item.vectors;
        let placed = (Vec3::from(position), Vec3::from(degrees));
        if loader.spawn(id, &load_name, &run_name, placed, Role::Stage) {
            let run = loader.loaded.spawns.last().and_then(|s| s.run.as_deref());
            if run.is_some_and(|r| r.ends_with(BIRDSTRIKE)) {
                loader.loaded.report.scripted += 1;
            }
        }
    }
    // `E03CarFlipInit` (0x445380): five spectators unless the map is in the exe's list.
    if !no_flip_map(data, map) {
        for i in 1..=5 {
            let load_name = format!("load_car_flip{i}.lua");
            if loader.spawn(i, &load_name, "run_car_flip.lua", (Vec3::ZERO, Vec3::ZERO), Role::Flip(i as usize - 1)) {
                loader.loaded.report.scripted += 1;
            }
        }
    }
    // `CPC_RescueHelicopter::Vf18` (0x452290) loads it when called; here with the stage.
    if loader.spawn(0, "load_rescuechopper.lua", "run_rescuechopper.lua", (Vec3::ZERO, Vec3::ZERO), Role::Rescue) {
        loader.loaded.report.scripted += 1;
    }
    stage.textures.extend(loader.assets.textures);
    loader.loaded.crt = loader.crt;
    Ok(loader.loaded)
}

/// The run script of the stage's bird-strike items.
pub const BIRDSTRIKE: &str = "run_birdstrike.lua";

/// The maps `E03CarFlipInit` loads no flip spectators on: the `i32`s from 0x72ace0 to 0x72ad34 of the exe.
const NO_FLIP_MAPS: (u32, u32) = (0x0072_ACE0, 0x0072_AD34);

fn no_flip_map(data: &Data, map: u32) -> bool {
    let image = data
        .resolve("RichardBurnsRally_SSE.exe")
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| pe::parse(&bytes).ok());
    let Some(image) = image else { return false };
    (NO_FLIP_MAPS.0..NO_FLIP_MAPS.1).step_by(4).any(|at| image.u32(at) == Some(map))
}

/// What a character is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A stage `(2, 14)` item.
    Stage,
    /// Car-flip spectator `n` of five (`load_car_flip<n + 1>.lua`, impl +0xcc + 4n).
    Flip(usize),
    /// The rescue helicopter.
    Rescue,
}

/// The state of one load.
struct Loader<'a> {
    folder: String,
    scripts: Scripts,
    crt: random::CrtRand,
    random: random::Lua,
    lua: Lua,
    assets: Assets<'a>,
    options: Options,
    loaded: Loaded,
}

impl Loader<'_> {
    /// Runs one load script and spawns its character (`E03AnimImpl::LoadAnim`, 0x445090); false when it is
    /// declined or fails (counted in the report).
    fn spawn(&mut self, id: u32, load_name: &str, run_name: &str, placed: (Vec3, Vec3), role: Role) -> bool {
        match self.try_spawn(id, load_name, run_name, placed, role) {
            Ok(Some(spawn)) => {
                self.loaded.spawns.push(spawn);
                self.loaded.report.spawned += 1;
                true
            }
            Ok(None) => {
                self.loaded.report.declined += 1;
                false
            }
            Err(reason) => {
                *self.loaded.report.failed.entry(reason).or_default() += 1;
                false
            }
        }
    }

    fn try_spawn(
        &mut self,
        id: u32,
        load_name: &str,
        run_name: &str,
        (position, degrees): (Vec3, Vec3),
        role: Role,
    ) -> Result<Option<Spawn>, String> {
        let load_path =
            self.scripts.resolve(&self.folder, load_name).ok_or_else(|| format!("no load script {load_name}"))?;
        let run_path = self.scripts.resolve(&self.folder, run_name);
        let chunk = self.scripts.chunk(&load_path).ok_or_else(|| format!("{load_path} does not parse"))?;
        let birdstrike = self.options.birdstrike;
        let mut host = LoadHost { random: &mut self.random, birdstrike, requests: Requests::default() };
        self.lua.run(&chunk, &mut host).map_err(|e| format!("{load_path}: {e}"))?;
        let key = format!("{}_{id}", self.lua.global("name").and_then(Value::text).unwrap_or_default());
        let result = match self.lua.call("Load", &[Value::Str(key.clone())], &mut host) {
            Some(Ok(values)) => values.first().and_then(Value::number),
            Some(Err(e)) => return Err(format!("{load_path}: {e}")),
            None => None,
        };
        let requests = host.requests;
        self.loaded.report.unknown_calls.extend(requests.unknown.iter().cloned());
        if result != Some(0.0) {
            return Ok(None);
        }
        let scale = self.lua.global("scale").and_then(Value::number).unwrap_or(1.0) as f32;
        let model_paths = requests.model.ok_or_else(|| format!("{load_path}: no model"))?;
        let model = self.assets.model(&model_paths, &mut self.loaded.models)?;
        let mut clips = Vec::new();
        for clip_name in &requests.added {
            let Some(sga_path) = requests.animations.get(clip_name) else { continue };
            match self.assets.clip(model, sga_path, &self.loaded.models, &mut self.loaded.clips) {
                Ok(clip) => clips.push(clip),
                Err(e) => *self.loaded.report.failed.entry(e).or_default() += 1,
            }
        }
        if clips.is_empty() {
            return Err(format!("{load_path}: no clip"));
        }
        if let Some(run_path) = &run_path {
            if let Some(chunk) = self.scripts.chunk(run_path) {
                self.loaded.runs.entry(run_path.clone()).or_insert_with(|| (*chunk).clone());
            }
        }
        Ok(Some(Spawn {
            id,
            key,
            load: load_path,
            run: run_path.filter(|p| self.loaded.runs.contains_key(p)),
            position,
            degrees,
            scale,
            model,
            clips,
            role,
        }))
    }
}

/// The parts of a skinned or static model mesh that its drawn mesh takes.
#[derive(Clone, Copy)]
struct Geometry<'m> {
    /// The mesh's node name: its section of the model's `.ini`.
    name: &'m str,
    vertices: &'m [sgc::Vertex],
    uvs: &'m [[f32; 2]],
    triangles: &'m [[u16; 3]],
    faces: &'m [sgc::Face],
}

impl<'m> Geometry<'m> {
    fn of(
        name: &'m str,
        vertices: &'m [sgc::Vertex],
        uvs: &'m [[f32; 2]],
        triangles: &'m [[u16; 3]],
        faces: &'m [sgc::Face],
    ) -> Geometry<'m> {
        Geometry { name, vertices, uvs, triangles, faces }
    }
}

/// A static mesh of a character model (the rescue helicopter's parts) as a mesh skinned whole to its node: the rest
/// vertices are placed as the car loader places a part ([`crate::coords::part_to_car`]) and follow the node's pose
/// from its bind matrix in the node table. Ours: how LocoMotion moves a static mesh with its node is not traced.
fn rigid(skeleton: &Skeleton, table: &sgs::Hierarchy, mesh: &sgc::Mesh, template: Mesh) -> ModelMesh {
    let place = crate::coords::part_to_car(mesh.node.world, mesh.node.transform.words());
    let node = skeleton.find(&mesh.node.name);
    let bind = node.map_or(place, |n| crate::coords::affine(table.nodes[n].world));
    let weight =
        sgs::SkinWeight { weights: [1.0, 0.0, 0.0, 0.0], offsets: [0, sgs::NO_BONE, sgs::NO_BONE, sgs::NO_BONE] };
    ModelMesh {
        positions: mesh.vertices.iter().map(|v| place.transform_point3(Vec3::from(v.position))).collect(),
        normals: mesh
            .vertices
            .iter()
            .map(|v| place.transform_vector3(Vec3::from(v.normal)).normalize_or_zero())
            .collect(),
        weights: vec![weight; mesh.vertices.len()],
        skin: Skin { nodes: vec![node], inverse_bind: vec![bind.inverse()], missing: usize::from(node.is_none()) },
        node,
        template,
    }
}

/// Model, clip and texture caches.
pub(super) struct Assets<'a> {
    data: &'a Data,
    models: HashMap<String, usize>,
    clips: HashMap<(usize, String), usize>,
    texture_keys: HashMap<String, TextureRef>,
    pub(super) textures: BTreeMap<TextureRef, StageTexture>,
}

impl<'a> Assets<'a> {
    pub(super) fn new(data: &'a Data) -> Assets<'a> {
        Assets {
            data,
            models: HashMap::new(),
            clips: HashMap::new(),
            texture_keys: HashMap::new(),
            textures: BTreeMap::new(),
        }
    }

    fn read(&self, dos_path: &str) -> Result<Vec<u8>, String> {
        let path = self.data.resolve(dos_path).ok_or_else(|| format!("{dos_path}: not found"))?;
        std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn ini(&self, dos_path: &str) -> Result<Vec<ini::Section>, String> {
        let bytes = self.read(dos_path)?;
        ini::parse(&String::from_utf8_lossy(&bytes)).map_err(|e| format!("{dos_path}: line {}: {}", e.line, e.message))
    }

    /// A model `[sgs, ini, shaders ini]`, loaded once.
    pub(super) fn model(&mut self, paths: &[String; 3], models: &mut Vec<Model>) -> Result<usize, String> {
        let key = paths[0].to_ascii_lowercase();
        if let Some(&index) = self.models.get(&key) {
            return Ok(index);
        }
        let bytes = self.read(&paths[0])?;
        let parsed = sgs::parse(&bytes).map_err(|e| format!("{}: {e}", paths[0]))?;
        let table = parsed
            .elements
            .iter()
            .find_map(|e| if let sgs::Element::Hierarchy(h) = e { Some(h) } else { None })
            .ok_or_else(|| format!("{}: no node table", paths[0]))?;
        let skeleton = Skeleton::new(table);
        let faces = self.ini(&paths[1]).unwrap_or_default();
        let materials = self.ini(&paths[2]).unwrap_or_default();
        let mut meshes = Vec::new();
        for element in &parsed.elements {
            match element {
                sgs::Element::SkinnedMesh(mesh) => {
                    let geometry =
                        Geometry::of(&mesh.node.name, &mesh.vertices, &mesh.uvs, &mesh.triangles, &mesh.faces);
                    let template = self.template(geometry, &faces, &materials);
                    meshes.push(ModelMesh {
                        positions: mesh.vertices.iter().map(|v| Vec3::from(v.position)).collect(),
                        normals: mesh.vertices.iter().map(|v| Vec3::from(v.normal)).collect(),
                        weights: mesh.skin.weights.clone(),
                        skin: Skin::bind(&skeleton, &mesh.skin),
                        node: skeleton.find(&mesh.node.name),
                        template,
                    });
                }
                sgs::Element::StaticMesh(mesh) => {
                    let geometry =
                        Geometry::of(&mesh.node.name, &mesh.vertices, &mesh.uvs, &mesh.triangles, &mesh.faces);
                    let template = self.template(geometry, &faces, &materials);
                    meshes.push(rigid(&skeleton, table, mesh, template));
                }
                _ => {}
            }
        }
        if meshes.is_empty() {
            return Err(format!("{}: no mesh", paths[0]));
        }
        models.push(Model { skeleton, meshes });
        self.models.insert(key, models.len() - 1);
        Ok(models.len() - 1)
    }

    /// An `.sga` bound to a model's tree, loaded once per model.
    pub(super) fn clip(
        &mut self,
        model: usize,
        dos_path: &str,
        models: &[Model],
        clips: &mut Vec<Clip>,
    ) -> Result<usize, String> {
        let key = (model, dos_path.to_ascii_lowercase());
        if let Some(&index) = self.clips.get(&key) {
            return Ok(index);
        }
        let bytes = self.read(dos_path)?;
        let elements = sga::parse(&bytes).map_err(|e| format!("{dos_path}: {e}"))?;
        clips.push(Clip::bind(&models[model].skeleton, &elements));
        self.clips.insert(key, clips.len() - 1);
        Ok(clips.len() - 1)
    }

    /// The mesh drawn for a model mesh: its vertices with texture coordinates (positions set each frame), one draw
    /// per face with the texture its material names (`FaceBlock<N>_Material` in the mesh's section of the model's
    /// `.ini`, else its first section; the `<material>_diffuse_bitmap` file in `SGS/textures`).
    fn template(&mut self, mesh: Geometry<'_>, faces: &[ini::Section], materials: &[ini::Section]) -> Mesh {
        let block = faces.iter().find(|s| s.name == mesh.name).or(faces.first());
        let mut indices = Vec::new();
        let mut draws = Vec::new();
        for (n, face) in mesh.faces.iter().enumerate() {
            let material = block
                .and_then(|s| s.get(&format!("FaceBlock{n}_Material")))
                .map(str::to_owned)
                .unwrap_or_else(|| face.name.clone());
            let section = materials.iter().find(|s| s.name == material);
            let opacity = section.and_then(|s| s.get("OpacityChannel")).is_some_and(|v| v.eq_ignore_ascii_case("true"));
            let file = materials
                .iter()
                .find(|s| s.name == format!("{material}_diffuse_bitmap"))
                .and_then(|s| s.get("Filename"))
                .map(|f| f.trim().to_owned());
            let texture = file.map(|f| self.texture(&f));
            let start = face.start_index as usize / 3;
            let Some(triangles) = mesh.triangles.get(start..start + face.prim_count as usize) else { continue };
            let first = indices.len();
            for tri in triangles {
                if tri.iter().all(|&i| usize::from(i) < mesh.vertices.len()) {
                    indices.extend(tri.map(u32::from));
                }
            }
            if indices.len() > first {
                draws.push(Draw {
                    indices: first..indices.len(),
                    state: draw_state(texture, opacity),
                    lod: Lod::Always,
                });
            }
        }
        let vertices = mesh
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let uv = mesh.uvs.get(i).copied().unwrap_or([0.0; 2]);
                Vertex {
                    position: v.position,
                    color: [255, 255, 255, 255],
                    uv0: uv,
                    uv1: uv,
                    uv2: [0.0; 2],
                    normal: v.normal,
                    specular: [0.0; 3],
                }
            })
            .collect();
        Mesh {
            layer: Layer::Solid,
            anchor: Anchor::World,
            bounds: Some(Bounds { center: Vec3::ZERO, half_extent: Vec3::ONE }),
            cell: None,
            vertices,
            indices,
            draws,
        }
    }

    /// A texture of `SGS/textures`, loaded once; a missing one keeps its key with no image (drawn magenta).
    fn texture(&mut self, file: &str) -> TextureRef {
        let request = format!("SGS\\textures\\{file}");
        if let Some(&key) = self.texture_keys.get(&request.to_ascii_lowercase()) {
            return key;
        }
        let key = TextureRef { set: TextureSet::Character, index: self.texture_keys.len() as u32 };
        let image = self.read(&request).ok().filter(|bytes| dds::parse(bytes).is_ok()).map(Image::Dds);
        self.texture_keys.insert(request.to_ascii_lowercase(), key);
        self.textures.insert(key, StageTexture { request, image });
        key
    }
}

fn draw_state(texture: Option<TextureRef>, opacity: bool) -> DrawState {
    DrawState {
        textures: [texture, None],
        shadow: None,
        address: [Address::Wrap; 2],
        second_texture: false,
        alpha_ref: opacity.then_some(ALPHA_REF),
        mip_bias: false,
        cull_back: !opacity,
        blend: false,
        additive: false,
        depth_test: true,
        depth_write: true,
        shade: Some(SHADE),
        specular: None,
    }
}

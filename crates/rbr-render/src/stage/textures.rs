//! Reading the textures a [`Stage`] uses: the stage `.ini` names, `TextureFilenameMap.dat`, the `*_textures.rbz`
//! archives and loose folders, RSF fallbacks and texture packs.

use super::mesh::{Image, Mesh, StageTexture};
use super::{
    ini_sections, read, Error, InteractiveInstance, Stage, TextureName, TextureRef, TextureSet, Weather, CLOUDS_TRACK,
    WATER_STAND_IN,
};
use rbr_formats::rbz::{self, ZipArchive};
use rbr_formats::{cars, dds, ini, rsf, stage_files, stage_textures};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

impl Stage {
    /// Reads the texture files for this stage's selection from `maps` into [`Stage::textures`], and lists
    /// the ones that are missing in [`LoadReport::missing_textures`].
    ///
    /// A stage in a folder of its own (RallySimFans) overlays its `TextureFilenameMap<id>.ini` on the retail map, and
    /// its requests are looked up in its folder first, then in the other stage folders beside it, then in `maps`, as
    /// archives (`<track>_textures.rbz`) or loose folders (`<track>_textures\..`). A request that none of those
    /// has is tried last as the stage's base retail track's (`rsf::base_track`).
    pub fn load_textures(&mut self, maps: &Path) -> Result<(), Error> {
        let map_bytes = read(&maps.join("TextureFilenameMap.dat"))?;
        let mut filename_map = stage_textures::FilenameMap::parse(&map_bytes).map_err(Error::FilenameMap)?;
        let mut dirs = vec![];
        // An RSF stage's own track prefix and its base track's (`rsf::base_track`), whose textures stand in last.
        let mut base = None;
        let mut own = None;
        if let Some(folder) = stage_files::folder(&self.name) {
            if let Some((id, _)) = stage_files::parts(&self.name) {
                match rsf::texture_map(folder, id) {
                    Ok(remap) => filename_map = filename_map.overlaid(remap),
                    Err(e) => eprintln!("stage: {e}"),
                }
                base = rsf::base_track(folder, id).map(|b| (format!("track-{id}_"), format!("track-{b}_")));
                own = Some(format!("track-{id}_"));
            }
            dirs.push(folder.to_owned());
            if let Some(rsf_maps) = folder.parent() {
                let mut others: Vec<PathBuf> = std::fs::read_dir(rsf_maps)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && p != folder)
                    .collect();
                others.sort();
                dirs.extend(others);
                dirs.push(rsf_maps.to_owned());
            }
        }
        dirs.push(maps.to_owned());
        let mut archives = Archives::new(&dirs);
        let mut textures = BTreeMap::new();
        let mut missing = Vec::new();
        let mut fallbacks = Vec::new();
        for (&texture, name) in &self.texture_names {
            // The candidate requests with their weather, in the order they are tried.
            let requests: Vec<(Weather, String)> = match texture.set {
                TextureSet::Shadow => {
                    vec![(self.selection.weather, stage_textures::shadow_request(&name.track, &name.file_name))]
                }
                // The loader tries the wear folder, then the plain form, in each weather folder.
                TextureSet::Specular => self
                    .selection
                    .weather
                    .fallbacks()
                    .iter()
                    .flat_map(|&weather| {
                        stage_textures::specular_requests(
                            &name.track,
                            &name.file_name,
                            weather.as_str(),
                            self.selection.wear.as_str(),
                        )
                        .map(|request| (weather, request))
                    })
                    .collect(),
                _ => self
                    .selection
                    .weather
                    .fallbacks()
                    .iter()
                    .map(|&weather| {
                        let request = stage_textures::texture_request(
                            &name.track,
                            &name.file_name,
                            name.ground,
                            weather.as_str(),
                            self.selection.wear.as_str(),
                        );
                        (weather, request)
                    })
                    .collect(),
            };
            let on_base: Vec<(Weather, String)> = base
                .iter()
                .flat_map(|(own, base)| {
                    requests.iter().filter_map(move |(weather, request)| {
                        let rest =
                            request.get(own.len()..).filter(|_| request[..own.len()].eq_ignore_ascii_case(own))?;
                        Some((*weather, format!("{base}{rest}")))
                    })
                })
                .collect();
            let elsewhere = match (&own, requests.first()) {
                (Some(own), Some((_, request))) => other_folders(request, own),
                _ => Vec::new(),
            };
            // An RSF ground texture that its archive has only without the wear folder (322: `damp\nth_shen.dds`, the
            // remap's target for `dry\nth_shen.dds`) is tried last in the plain form. Ours: the game's loader asks
            // for the wear folder only, and what RSF's plugin does is not traced.
            let plain: Vec<(Weather, String)> = match texture.set {
                TextureSet::Shadow | TextureSet::Specular => Vec::new(),
                _ if !name.ground || own.is_none() => Vec::new(),
                _ => self
                    .selection
                    .weather
                    .fallbacks()
                    .iter()
                    .map(|&weather| {
                        let request = stage_textures::texture_request(
                            &name.track,
                            &name.file_name,
                            false,
                            weather.as_str(),
                            self.selection.wear.as_str(),
                        );
                        (weather, request)
                    })
                    .collect(),
            };
            let resolved: Vec<(Weather, String)> = requests
                .into_iter()
                .chain(on_base)
                .chain(elsewhere)
                .chain(plain)
                .map(|(weather, request)| (weather, filename_map.resolve(&request)))
                .collect();
            let found = resolved.iter().find_map(|(weather, request)| {
                let image = archives.entry_bytes(request).and_then(|bytes| decode(texture.set, bytes))?;
                Some((*weather, request, image))
            });
            let (request, image) = match found {
                Some((weather, request, image)) => {
                    if weather != self.selection.weather {
                        fallbacks.push(request.clone());
                    }
                    (request.clone(), Some(image))
                }
                None => {
                    let first = resolved.into_iter().next().map(|(_, request)| request).unwrap_or_default();
                    missing.push(first.clone());
                    (first, None)
                }
            };
            textures.insert(texture, StageTexture { request, image });
        }
        if self
            .meshes
            .iter()
            .flat_map(|mesh| &mesh.draws)
            .any(|draw| draw.state.textures.contains(&Some(TextureRef::water_stand_in())))
        {
            textures.insert(
                TextureRef::water_stand_in(),
                StageTexture {
                    request: "WATER_STAND_IN (stand-in; not the game's texture)".to_owned(),
                    image: Some(Image::Solid(WATER_STAND_IN)),
                },
            );
        }
        self.textures = textures;
        if self.start_board().is_some() {
            let textures = maps.parent().map(|install| install.join("Textures"));
            self.start_digits = textures
                .and_then(|dir| cars::resolve(&dir, crate::start_board::DIGITS))
                .and_then(|path| std::fs::read(path).ok());
        }
        self.report.missing_textures = missing;
        self.report.fallback_textures = fallbacks;
        Ok(())
    }

    /// The start board's clock face texture ([`crate::start_board::PLACEHOLDER`]), when a draw shows it.
    pub fn start_board(&self) -> Option<TextureRef> {
        let placeholder = crate::start_board::PLACEHOLDER.to_ascii_lowercase();
        self.texture_names
            .iter()
            .find(|(_, name)| name.file_name.to_ascii_lowercase().ends_with(&placeholder))
            .map(|(&texture, _)| texture)
    }

    /// Replaces each loaded texture that the texture pack in `dir` holds: a file at the texture's request
    /// path (`track-10_o_textures/dry/x.dds`, `Cars/ACCENT/textures/accent_external.dds`, as `texture-pack` writes
    /// it), of the sets `sets` takes. Dial faces keep theirs: their needles are placed in texels. Pack files that do
    /// not read are ignored. Returns how many textures were replaced. Not the game's behaviour: packs are local
    /// add-ons.
    pub fn apply_texture_pack(&mut self, dir: &Path, sets: impl Fn(TextureSet) -> bool) -> usize {
        let mut replaced = 0;
        for (texture, loaded) in &mut self.textures {
            if !sets(texture.set) || matches!(loaded.image, Some(Image::Dial { .. })) {
                continue;
            }
            let path = dir.join(loaded.request.replace('\\', "/"));
            if let Some(image) = std::fs::read(path).ok().and_then(|bytes| decode(texture.set, bytes)) {
                loaded.image = Some(image);
                replaced += 1;
            }
        }
        replaced
    }
}

/// An RSF stage's request (`track-157_o_textures\dry\new\ah1.dds`, the stage's prefix `own` being `track-157_`) in
/// the stage's other variants' and weathers' folders: its own variant's other weathers first, then each other
/// variant's. Ours, inferred from the files: RSF stages leave some textures in another variant's archive (157's `O`
/// draws `M`'s) or only in another weather's folder, with no remap line, and what RSF's plugin does is not traced.
pub(super) fn other_folders(request: &str, own: &str) -> Vec<(Weather, String)> {
    let Some(rest) = request.get(own.len()..).filter(|_| request[..own.len()].eq_ignore_ascii_case(own)) else {
        return Vec::new();
    };
    let Some((letter, rest)) = rest.split_once("_textures\\") else { return Vec::new() };
    let Some((weather, file)) = rest.split_once('\\') else { return Vec::new() };
    let letter = letter.to_ascii_lowercase();
    let letters = std::iter::once(letter.clone())
        .chain(rsf::LETTERS.iter().map(|l| l.to_ascii_lowercase().to_string()).filter(|l| *l != letter));
    letters
        .flat_map(|l| Weather::ALL.into_iter().map(move |w| (l.clone(), w)))
        .filter(|(l, w)| *l != letter || !w.as_str().eq_ignore_ascii_case(weather))
        .map(|(l, w)| (w, format!("{own}{l}_textures\\{}\\{file}", w.as_str())))
        .collect()
}

/// The image of a texture file's `bytes` if it reads: a `.dds` for stage and cloud textures, the shadow
/// stream for [`TextureSet::Shadow`].
fn decode(set: TextureSet, bytes: Vec<u8>) -> Option<Image> {
    match set {
        TextureSet::Shadow => shadow_image(bytes),
        TextureSet::WaterStandIn => None,
        _ => {
            let bytes = dds::to_rgba8(&bytes).unwrap_or(bytes);
            let bytes = dds::with_mips(&bytes).unwrap_or(bytes);
            let readable = dds::parse(&bytes).is_ok();
            readable.then_some(Image::Dds(bytes))
        }
    }
}

/// A shadow stream: little-endian u32 width and height, then `width * height` `L8` texels.
fn shadow_image(bytes: Vec<u8>) -> Option<Image> {
    let word = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (width, height) = (word(0)?, word(4)?);
    let len = (width as usize).checked_mul(height as usize)?;
    let pixels = bytes.get(8..)?.get(..len)?.to_vec();
    Some(Image::Shadow { width, height, pixels })
}

/// The names of the textures the meshes use: stage textures from the stage's `.ini`, cloud textures from
/// `Clouds.ini`. A name missing from its list is left out, and shows up as a missing texture later.
pub(super) fn texture_names(
    maps: &Path,
    stage: &str,
    stage_sections: &[ini::Section],
    meshes: &[Mesh],
    interactive: &[InteractiveInstance],
) -> Result<BTreeMap<TextureRef, TextureName>, Error> {
    let used: BTreeSet<TextureRef> = meshes
        .iter()
        .chain(interactive.iter().map(|instance| instance.mesh.as_ref()))
        .flat_map(|m| &m.draws)
        .flat_map(|d| d.state.textures.into_iter().chain([d.state.shadow, d.state.specular]).flatten())
        .collect();
    let clouds_sections = ini_sections(&maps.join(format!("{CLOUDS_TRACK}.ini")))?;
    Ok(used
        .into_iter()
        .filter_map(|texture| {
            let (sections, track, key) = match texture.set {
                TextureSet::Stage => (stage_sections, stage, "Texture"),
                TextureSet::Clouds => (clouds_sections.as_slice(), CLOUDS_TRACK, "Texture"),
                TextureSet::Shadow => (stage_sections, stage, "ShadowTexture"),
                TextureSet::Specular => (stage_sections, stage, "SpecularTexture"),
                // Car, character and fence textures come from their own folders, not a track's archive.
                TextureSet::Car | TextureSet::Character | TextureSet::Fence => return None,
                TextureSet::WaterStandIn => return None,
            };
            Some((texture, texture_name(sections, track, key, texture.index)?))
        })
        .collect())
}

/// One `[TextureInfo]` entry (`key` is `Texture` or `ShadowTexture`): its file name and whether its section
/// marks it as a ground texture.
pub(super) fn texture_name(sections: &[ini::Section], track: &str, key: &str, index: u32) -> Option<TextureName> {
    let section = |name: &str| sections.iter().find(|s| s.name.eq_ignore_ascii_case(name));
    let file_name = section("TextureInfo")?.get(&format!("{key}{index}"))?.to_owned();
    let ground =
        section(&file_name).and_then(|s| s.get("IsGroundTexture")).is_some_and(|v| v.eq_ignore_ascii_case("true"));
    Some(TextureName { track: track.to_owned(), file_name, ground })
}

/// The `*_textures.rbz` archives in `Maps` (and in stage folders), opened on first use, and the loose
/// `*_textures` folders beside them.
struct Archives {
    /// Lower-case file name to the file's actual path: the first folder that has it wins.
    files: HashMap<String, PathBuf>,
    open: HashMap<String, Option<OpenArchive>>,
}

/// An archive and its entries by lower-case path.
struct OpenArchive {
    zip: ZipArchive<BufReader<File>>,
    entries: HashMap<String, usize>,
}

impl Archives {
    fn new(dirs: &[PathBuf]) -> Archives {
        let mut files = HashMap::new();
        for dir in dirs {
            for path in std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_ascii_lowercase) {
                    files.entry(name).or_insert(path);
                }
            }
        }
        Archives { files, open: HashMap::new() }
    }

    /// The bytes of the archive entry at `path` (`track-10_o_textures\dry\x.dds`), if it exists.
    fn entry_bytes(&mut self, path: &str) -> Option<Vec<u8>> {
        let (archive, _) = stage_textures::split_archive_path(path)?;
        let archive = archive.to_ascii_lowercase();
        let entry = path.replace('\\', "/").to_ascii_lowercase();
        // A loose folder (`track-129_M_textures\dry\x.dds`) when there is no archive of the name.
        if !self.files.contains_key(&archive) {
            let (folder, rest) = path.split_once('\\')?;
            let dir = self.files.get(&folder.to_ascii_lowercase()).filter(|d| d.is_dir())?;
            return std::fs::read(cars::resolve(dir, rest)?).ok();
        }
        let open = self.open.entry(archive.clone()).or_insert_with(|| {
            let file = self.files.get(&archive)?;
            let zip = rbz::open(file).ok()?;
            let entries = (0..zip.len())
                .filter_map(|i| {
                    let name = zip.name_for_index(i)?.ok()?.to_ascii_lowercase();
                    Some((name, i))
                })
                .collect();
            Some(OpenArchive { zip, entries })
        });
        let archive = open.as_mut()?;
        let &index = archive.entries.get(&entry)?;
        let mut bytes = Vec::new();
        archive.zip.by_index(index).ok()?.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_rsf_request_is_looked_for_in_the_other_folders_of_its_stage() {
        let found = other_folders("track-157_o_textures\\dry\\new\\ah1.dds", "track-157_");
        assert_eq!(found.len(), 14, "two other weathers of O, three of each other variant");
        assert_eq!(found[0], (Weather::Damp, "track-157_o_textures\\damp\\new\\ah1.dds".to_owned()));
        assert_eq!(found[2], (Weather::Dry, "track-157_m_textures\\dry\\new\\ah1.dds".to_owned()));
        assert!(other_folders("track-54_e_textures\\dry\\a.dds", "track-157_").is_empty());
    }
}

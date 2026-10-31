//! The car's files: diffuse textures by DOS path, shader files and the ini sections, resolved in the install.

use super::{CarReport, Error};
use crate::stage::{Image, StageTexture, TextureRef, TextureSet};
use rbr_formats::data::Data;
use rbr_formats::dds;
use rbr_formats::ini::{self, Section};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// The car's diffuse textures by DOS path. Each path is read once; a missing or unreadable one keeps
/// its key with no DDS, so the renderer draws it magenta.
pub(super) struct Textures<'a> {
    data: &'a Data,
    keys: HashMap<String, TextureRef>,
    loaded: BTreeMap<TextureRef, StageTexture>,
}

impl<'a> Textures<'a> {
    pub(super) fn new(data: &'a Data) -> Self {
        Textures { data, keys: HashMap::new(), loaded: BTreeMap::new() }
    }

    pub(super) fn key(&mut self, dos_path: &str) -> TextureRef {
        if let Some(&key) = self.keys.get(dos_path) {
            return key;
        }
        let key = TextureRef { set: TextureSet::Car, index: self.keys.len() as u32 };
        self.keys.insert(dos_path.to_owned(), key);
        let image = resolve(self.data, dos_path)
            .ok()
            .and_then(|path| fs::read(path).ok())
            .and_then(|bytes| {
                // RSF cars' plain `A8R8G8B8` textures, with the mip chain they lack.
                let bytes = dds::to_rgba8(&bytes).unwrap_or(bytes);
                let bytes = dds::with_mips(&bytes).unwrap_or(bytes);
                // And the block-compressed ones (the 131 Grp4's 4096x4096 livery).
                let bytes = crate::upscale::with_block_mips(&bytes).unwrap_or(bytes);
                if dds::parse(&bytes).is_ok() {
                    Some(bytes)
                } else {
                    alpha_only(&bytes)
                }
            })
            .map(Image::Dds);
        self.loaded.insert(key, StageTexture { request: dos_path.to_owned(), image });
        key
    }

    /// A texture made by `make` (once per car) rather than read.
    pub(super) fn generated(&mut self, name: &str, make: fn() -> Vec<u8>) -> TextureRef {
        if let Some(&key) = self.keys.get(name) {
            return key;
        }
        let key = TextureRef { set: TextureSet::Car, index: self.keys.len() as u32 };
        self.keys.insert(name.to_owned(), key);
        self.loaded.insert(key, StageTexture { request: name.to_owned(), image: Some(Image::Dds(make())) });
        key
    }

    pub(super) fn finish(self, report: &mut CarReport) -> BTreeMap<TextureRef, StageTexture> {
        report.missing_textures =
            self.loaded.values().filter(|t| t.image.is_none()).map(|t| t.request.clone()).collect();
        self.loaded
    }
}

/// An uncompressed alpha-only `.dds` (`DDPF_ALPHA`, 8 bits: ACCENT's and IMPREZA03's `Dirt` and `TireDirt` maps) as
/// a white `DXT3` with that alpha in 4 bits (the renderer reads the block formats only). A map that is not square
/// (the `TireDirt` maps are 512x256) is stretched to its larger side.
fn alpha_only(bytes: &[u8]) -> Option<Vec<u8>> {
    let word = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (height, width, flags, bits) = (word(12)? as usize, word(16)? as usize, word(80)?, word(88)?);
    let size = width.max(height);
    if bytes.get(..4)? != b"DDS "
        || flags != 2
        || bits != 8
        || !width.is_power_of_two()
        || !height.is_power_of_two()
        || width.min(height) < 4
    {
        return None;
    }
    let texels = bytes.get(128..128 + width * height)?;
    let alpha = (0..size * size)
        .map(|i| f32::from(texels[i / size * height / size * width + i % size * width / size]) / 255.0)
        .collect();
    Some(crate::crack_decal::white_dxt3(size, alpha, false))
}

/// The folder of a model path such as `Cars\xsara\xsara.sgc` (`xsara`).
pub(super) fn folder(dos_path: &str) -> Option<&str> {
    dos_path.split(['\\', '/']).filter(|c| !c.is_empty()).nth(1)
}

pub(super) fn resolve(data: &Data, dos_path: &str) -> Result<PathBuf, Error> {
    data.resolve(dos_path).ok_or_else(|| Error::Missing(dos_path.to_owned()))
}

/// Resolves a shader file and checks its version token.
pub(super) fn shader_file(data: &Data, dos_path: &str, version: u32) -> Result<PathBuf, Error> {
    let path = resolve(data, dos_path)?;
    let token = read_bytes(&path)?.first_chunk::<4>().map(|bytes| u32::from_le_bytes(*bytes));
    if token != Some(version) {
        return Err(Error::NotShader(path));
    }
    Ok(path)
}

pub(super) fn read_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|source| Error::Io { path: path.to_owned(), source })
}

pub(super) fn read_text(data: &Data, dos_path: &str) -> Result<String, Error> {
    let path = resolve(data, dos_path)?;
    Ok(String::from_utf8_lossy(&read_bytes(&path)?).into_owned())
}

pub(super) fn read_ini(data: &Data, dos_path: &str) -> Result<Vec<Section>, Error> {
    let path = resolve(data, dos_path)?;
    let text = String::from_utf8_lossy(&read_bytes(&path)?).into_owned();
    ini::parse(&text).map_err(|e| Error::Ini { path, line: e.line, message: e.message })
}

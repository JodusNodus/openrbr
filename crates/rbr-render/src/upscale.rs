//! Upscaled textures at load (ours, not the game's): each car texture and [`stage_textures`] texture is upscaled 4x by
//! a Real-ESRGAN ncnn binary with [`MODEL`], halved where 4x would pass [`MAX`], and re-encoded in its block format
//! with a full mip chain. The results are kept in a cache folder by the source file's SHA-256, so a texture is
//! upscaled once, whichever car or stage shows it, and again only when its file changes.
//!
//! The upscaler's folder (the launcher's `<player folder>/Upscaler`, which its "Download upscaler…" fills) holds the
//! binary, its `models` and the `cache`. `RBR_UPSCALER`, else `upscaler/` beside the running program, names another
//! binary, with the model in `models` beside it. The binary's `-s 2` scrambles the tiles of a 4x-only model, so 2048
//! sources run at 4x and are halved. Larger tiles (`-t 1024`) break the output past one tile.

use crate::stage::{Image, Layer, Stage, StageTexture, TextureRef, TextureSet};
use rbr_formats::dds::{self, Format};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The upscaling model, 4x: Real-ESRGAN's general WDN v3 (BSD-3), a compact network. On liveries, signs and the
/// cockpit's grain as good as UltraSharp, crisper on lettering, about 3x faster (2026-10-26).
pub const MODEL: &str = "RealESRGAN_General_WDN_x4_v3";
/// The largest side an upscaled texture gets: as sharp as 8192 on the Fiesta R5's liveries, at a third of the memory.
pub const MAX: u32 = 4096;
/// The smallest largest side upscaled: below it a texture is a light, a flat colour or a fence tile, with nothing to
/// sharpen.
pub const MIN: u32 = 32;
/// The player's folder's subfolder the launcher keeps the upscaler in.
pub const DIR: &str = "Upscaler";
/// The binary's file name.
pub const BINARY: &str = if cfg!(windows) { "realesrgan-ncnn-vulkan.exe" } else { "realesrgan-ncnn-vulkan" };

/// The upscaler binary and its models folder.
pub struct Upscaler {
    binary: PathBuf,
    models: PathBuf,
}

impl Upscaler {
    /// `RBR_UPSCALER`, else the binary in the folder `dir`, else in `upscaler/` beside the running program: the first
    /// with [`MODEL`] in `models` beside it.
    pub fn find(dir: &Path) -> Option<Upscaler> {
        let beside = || Some(std::env::current_exe().ok()?.parent()?.join("upscaler").join(BINARY));
        let candidates = match std::env::var_os("RBR_UPSCALER") {
            Some(binary) => vec![PathBuf::from(binary)],
            None => [Some(dir.join(BINARY)), beside()].into_iter().flatten().collect(),
        };
        candidates.into_iter().find_map(|binary| {
            let models = binary.parent()?.join("models");
            (binary.is_file() && models.join(format!("{MODEL}.param")).is_file()).then_some(Upscaler { binary, models })
        })
    }

    /// Upscales the PNG `input` 4x into the PNG `output`.
    fn run(&self, input: &Path, output: &Path) -> Result<(), String> {
        let status = Command::new(&self.binary)
            .arg("-i")
            .arg(input)
            .arg("-o")
            .arg(output)
            .arg("-m")
            .arg(&self.models)
            .args(["-n", MODEL, "-s", "4", "-f", "png"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("{}: {e}", self.binary.display()))?;
        status.success().then_some(()).ok_or_else(|| format!("{} failed: {status}", self.binary.display()))
    }
}

/// Replaces each texture of the sets `sets` takes in `textures` (a `.dds` file's, its larger side at most half
/// [`MAX`]) by its upscaled copy from `cache`, upscaling with `upscaler` the ones not there yet; `progress` is told
/// (done, total) before each. A texture that fails keeps its own. Returns how many were replaced.
pub fn apply(
    textures: &mut BTreeMap<TextureRef, StageTexture>,
    pick: impl Fn(TextureRef) -> bool,
    upscaler: &Upscaler,
    cache: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> usize {
    let folder = cache.join(format!("{MODEL}-{MAX}"));
    let mut planned: Vec<(TextureRef, PathBuf)> = Vec::new();
    let mut missing: BTreeMap<PathBuf, &[u8]> = BTreeMap::new();
    for (&key, texture) in textures.iter() {
        let Some(Image::Dds(bytes)) = &texture.image else { continue };
        if !pick(key) || !texture.request.to_ascii_lowercase().ends_with(".dds") || scale(bytes).is_none() {
            continue;
        }
        let path = folder.join(format!("{:x}.dds", Sha256::digest(bytes)));
        if !path.is_file() {
            missing.insert(path.clone(), bytes);
        }
        planned.push((key, path));
    }
    let total = missing.len();
    for (done, (path, bytes)) in missing.into_iter().enumerate() {
        progress(done, total);
        if let Err(e) = make(upscaler, bytes, &path) {
            eprintln!("upscale: {}: {e}", path.display());
        }
    }
    let mut replaced = 0;
    for (key, path) in planned {
        let Some(bytes) = std::fs::read(&path).ok().filter(|bytes| dds::parse(bytes).is_ok()) else { continue };
        if let Some(texture) = textures.get_mut(&key) {
            texture.image = Some(Image::Dds(bytes));
            replaced += 1;
        }
    }
    replaced
}

/// The stage's textures: the ground's atlases, signs, banners, barriers, vehicles, buildings and plants. Not the sky's,
/// nor the start board's clock face, which [`crate::start_board`] draws digits into.
pub fn stage_textures(stage: &Stage) -> BTreeSet<TextureRef> {
    let sky: BTreeSet<TextureRef> = stage
        .meshes
        .iter()
        .filter(|mesh| mesh.layer == Layer::Sky)
        .flat_map(|mesh| &mesh.draws)
        .flat_map(|draw| draw.state.textures.into_iter().flatten())
        .collect();
    let start_board = stage.start_board();
    stage
        .textures
        .keys()
        .copied()
        .filter(|&key| key.set == TextureSet::Stage && !sky.contains(&key) && Some(key) != start_board)
        .collect()
}

/// The scale a `.dds` file is upscaled by: 4, else 2 where 4 would pass [`MAX`]; none where 2 would, under [`MIN`],
/// or where it does not read.
fn scale(bytes: &[u8]) -> Option<u32> {
    let dds = dds::parse(bytes).ok()?;
    let side = dds.width.max(dds.height);
    [4, 2].into_iter().find(|s| side >= MIN && side * s <= MAX)
}

/// Upscales the `.dds` file `bytes` into `out`, through PNGs in a work folder beside it. Written whole, then renamed,
/// so a stopped load leaves no part file in its place.
fn make(upscaler: &Upscaler, bytes: &[u8], out: &Path) -> Result<(), String> {
    let dds = dds::parse(bytes).map_err(|e| e.to_string())?;
    let scale = scale(bytes).ok_or("too large")?;
    let level = &dds.levels()[0];
    let work = out.with_extension("work");
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let (input, output) = (work.join("in.png"), work.join("out.png"));
    let rgba = decompress(dds.format, level.data, level.width, level.height);
    let result = write_png(&input, level.width, level.height, &rgba)
        .map_err(|e| e.to_string())
        .and_then(|()| upscaler.run(&input, &output))
        .and_then(|()| read_png(&output).map_err(|e| e.to_string()))
        .and_then(|(mut width, mut height, mut rgba)| {
            if scale == 2 {
                (width, height, rgba) = half(width, height, &rgba);
            }
            let part = out.with_extension("part");
            std::fs::write(&part, encode(dds.format, width, height, rgba)).map_err(|e| e.to_string())?;
            std::fs::rename(&part, out).map_err(|e| e.to_string())
        });
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// The block format a `.dds` file's `format` is encoded in.
fn bc(format: Format) -> texpresso::Format {
    match format {
        Format::Dxt1 => texpresso::Format::Bc1,
        Format::Dxt3 => texpresso::Format::Bc2,
        // Plain RGBA (a community stage's, an RSF car's) is packed as DXT5, which keeps its alpha.
        Format::Dxt5 | Format::Rgba8 => texpresso::Format::Bc3,
    }
}

/// A `.dds` level's texels as RGBA8.
pub fn decompress(format: Format, data: &[u8], width: u32, height: u32) -> Vec<u8> {
    if format == Format::Rgba8 {
        return data.to_vec();
    }
    let mut rgba = vec![0; (width * height * 4) as usize];
    bc(format).decompress(data, width as usize, height as usize, &mut rgba);
    rgba
}

/// A DDS file in `format` (DXT1, DXT3, else DXT5) with the full mip chain of `rgba`.
pub fn encode(format: Format, mut width: u32, mut height: u32, mut rgba: Vec<u8>) -> Vec<u8> {
    let mut levels = Vec::new();
    loop {
        let mut block = vec![0; bc(format).compressed_size(width as usize, height as usize)];
        let params = texpresso::Params::default();
        bc(format).compress(&rgba, width as usize, height as usize, params, &mut block);
        levels.push((width, height, block));
        if width == 1 && height == 1 {
            break;
        }
        (width, height, rgba) = half(width, height, &rgba);
    }
    let (width, height, _) = levels[0];
    let fourcc = match format {
        Format::Dxt1 => b"DXT1",
        Format::Dxt3 => b"DXT3",
        Format::Dxt5 | Format::Rgba8 => b"DXT5",
    };
    let mut header = [0u32; 32];
    header[0] = u32::from_le_bytes(*b"DDS ");
    header[1] = 124;
    // Caps, height, width, pixel format, mip count and linear size are set.
    header[2] = 0x1 | 0x2 | 0x4 | 0x1000 | 0x2_0000 | 0x8_0000;
    header[3] = height;
    header[4] = width;
    header[5] = levels[0].2.len() as u32;
    header[7] = levels.len() as u32;
    header[19] = 32;
    header[20] = 0x4;
    header[21] = u32::from_le_bytes(*fourcc);
    // Texture, complex, mipmap.
    header[27] = 0x1000 | 0x8 | 0x40_0000;
    let mut file: Vec<u8> = header.iter().flat_map(|word| word.to_le_bytes()).collect();
    for (_, _, block) in &levels {
        file.extend_from_slice(block);
    }
    file
}

/// The next mip level: each texel the mean of a 2x2 box (edges repeat on a side of 1).
pub fn half(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let (w, h) = ((width / 2).max(1), (height / 2).max(1));
    let texel =
        |x: u32, y: u32, c: usize| rgba[((y.min(height - 1) * width + x.min(width - 1)) * 4) as usize + c] as u32;
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            for c in 0..4 {
                let sum = texel(2 * x, 2 * y, c)
                    + texel(2 * x + 1, 2 * y, c)
                    + texel(2 * x, 2 * y + 1, c)
                    + texel(2 * x + 1, 2 * y + 1, c);
                out.push(((sum + 2) / 4) as u8);
            }
        }
    }
    (w, h, out)
}

/// Writes `rgba` as an RGBA8 PNG.
pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// A PNG as RGBA8. An image without alpha reads as opaque.
pub fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels,
        png::ColorType::Rgb => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG after expansion".into()),
    };
    Ok((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grey DXT1 file `side` texels square, as `encode` writes it.
    fn file(side: u32) -> Vec<u8> {
        encode(Format::Dxt1, side, side, vec![128; (side * side * 4) as usize])
    }

    #[test]
    fn scale_fits_max() {
        assert_eq!(scale(&file(1024)), Some(4));
        assert_eq!(scale(&file(2048)), Some(2));
        assert_eq!(scale(&file(4096)), None);
        assert_eq!(scale(&file(16)), None);
    }

    #[test]
    fn encode_reads_back_with_mips() {
        let bytes = file(16);
        let dds = dds::parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.format), (16, 16, Format::Dxt1));
        assert_eq!(dds.levels().len(), 5);
    }
}

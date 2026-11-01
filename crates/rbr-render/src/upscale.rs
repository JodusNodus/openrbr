//! Upscaled textures at load (ours, not the game's): each car texture and [`stage_textures`] texture is upscaled 4x by
//! a Real-ESRGAN ncnn binary with [`MODEL`], halved where 4x would pass [`MAX`] ([`INTERIOR_MAX`] for the in-car
//! view's textures in a 4K frame, [`interior_max`]), and re-encoded in its block format with a full mip chain. A
//! texture the dials draw on (`Image::Dial`, the cockpit of a car with a gauge pack) has its base upscaled from its
//! art's size: RBRHUD's bases larger than the car's own texture are smooth resizes of it. The results are kept in a
//! cache folder per largest side by the source file's SHA-256, so a texture is upscaled once, whichever car or stage
//! shows it, and again only when its file changes.
//!
//! The upscaler's folder (the launcher's `<player folder>/Upscaler`, which its "Download upscaler…" fills) holds the
//! binary, its `models` and the `cache`. `RBR_UPSCALER`, else `upscaler/` beside the running program, names another
//! binary, with the model in `models` beside it. The binary's `-s 2` scrambles the tiles of a 4x-only model, so 2048
//! sources run at 4x and are halved. Larger tiles (`-t 1024`) break the output past one tile. The missing textures go
//! through one run of the binary over a folder (each run costs ~0.2 s to start), and each is encoded while the binary
//! makes the next.

use crate::parallel::in_parallel;
use crate::stage::{Image, Layer, Stage, StageTexture, TextureRef, TextureSet};
use rbr_formats::dds::{self, Format};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

/// The upscaling model, 4x: Real-ESRGAN's general WDN v3 (BSD-3), a compact network. On liveries, signs and the
/// cockpit's grain as good as UltraSharp, crisper on lettering, about 3x faster (2026-10-26).
pub const MODEL: &str = "RealESRGAN_General_WDN_x4_v3";
/// The largest side an upscaled texture gets: as sharp as 8192 on the Fiesta R5's liveries, at a third of the memory.
pub const MAX: u32 = 4096;
/// The largest side the in-car view's textures get when the frame is at least [`INTERIOR_HEIGHT`] high: at 4K a
/// 4096 cockpit is magnified on the dash (the Fiat 131 Grp4's, about 0.8 texels a pixel), 8192 is sharper there
/// (user, 2026-11-01). Up to 350 MB of GPU memory for a cockpit the dials draw on (an `RGBA8` target).
pub const INTERIOR_MAX: u32 = 8192;
/// The frame height, after the render scale, from which the in-car view's textures get [`INTERIOR_MAX`]: 4K's 2160
/// and a little under it.
pub const INTERIOR_HEIGHT: u32 = 1800;
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

    /// Starts upscaling each PNG in the folder `input` 4x into a PNG of the same name in `output`, naming each saved
    /// file on its standard error (`-v`: `IN -> OUT done`).
    fn spawn(&self, input: &Path, output: &Path) -> Result<Child, String> {
        Command::new(&self.binary)
            .arg("-i")
            .arg(input)
            .arg("-o")
            .arg(output)
            .arg("-m")
            .arg(&self.models)
            .args(["-n", MODEL, "-s", "4", "-f", "png", "-v"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{}: {e}", self.binary.display()))
    }
}

/// The largest side the in-car view's textures get in a frame `height` high (after the render scale).
pub fn interior_max(height: u32) -> u32 {
    if height >= INTERIOR_HEIGHT {
        INTERIOR_MAX
    } else {
        MAX
    }
}

/// Replaces each texture `pick` gives a largest side (at most [`MAX`] or [`INTERIOR_MAX`]) in `textures` (a `.dds`
/// file's or a dial's base, its larger side, or its art's, at most half that side) by its upscaled copy from `cache`
/// (a folder per largest side), upscaling with `upscaler` the ones not there yet; `progress` is told (done, total)
/// before each. A texture that fails keeps its own. Returns how many were replaced.
pub fn apply(
    textures: &mut BTreeMap<TextureRef, StageTexture>,
    pick: impl Fn(TextureRef) -> Option<u32>,
    upscaler: &Upscaler,
    cache: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> usize {
    let mut planned: Vec<(TextureRef, PathBuf)> = Vec::new();
    let mut missing: BTreeMap<PathBuf, Source> = BTreeMap::new();
    // Each with the larger side it is upscaled from (a dial's base its art's) and the largest side it gets.
    let wanted: Vec<_> = textures
        .iter()
        .filter_map(|(&key, texture)| {
            let max = pick(key)?;
            let (bytes, side) = match texture.image.as_ref()? {
                Image::Dds(bytes) => (bytes, larger_side(bytes)?),
                Image::Dial { base, art, .. } => (base, art.0.max(art.1)),
                _ => return None,
            };
            (texture.request.to_ascii_lowercase().ends_with(".dds") && scale(side, max).is_some()).then_some((
                key,
                bytes.as_slice(),
                side,
                max,
            ))
        })
        .collect();
    let digests = in_parallel(&wanted, |&(_, bytes, _, _)| Sha256::digest(bytes));
    for ((key, bytes, side, max), digest) in wanted.into_iter().zip(digests) {
        let path = cache.join(format!("{MODEL}-{max}")).join(format!("{digest:x}.dds"));
        if !path.is_file() {
            missing.insert(path.clone(), (bytes, side, max));
        }
        planned.push((key, path));
    }
    if !missing.is_empty() {
        let work = cache.join(format!("work-{}", std::process::id()));
        if let Err(e) = make(upscaler, missing.into_iter().collect(), &work, progress) {
            eprintln!("upscale: {e}");
        }
        let _ = std::fs::remove_dir_all(&work);
    }
    let mut replaced = 0;
    let read = in_parallel(&planned, |(_, path)| std::fs::read(path).ok().filter(|bytes| dds::parse(bytes).is_ok()));
    for ((key, _), bytes) in planned.into_iter().zip(read) {
        let Some(bytes) = bytes else { continue };
        if let Some(texture) = textures.get_mut(&key) {
            match &mut texture.image {
                Some(Image::Dial { base, .. }) => *base = bytes,
                image => *image = Some(Image::Dds(bytes)),
            }
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
        .filter(|key| stage.texture_names.get(key).is_none_or(|name| name.file_name != crate::grass::TEXTURE))
        .collect()
}

/// A `.dds` file's larger side, `None` where it does not read.
fn larger_side(bytes: &[u8]) -> Option<u32> {
    dds::parse(bytes).ok().map(|dds| dds.width.max(dds.height))
}

/// A texture to upscale: its `.dds` file, the larger side it is upscaled from and the largest side it gets.
type Source<'a> = (&'a [u8], u32, u32);

/// The scale a texture of larger side `side` is upscaled by to at most `max`: 4, else 2 where 4 would pass it; none
/// where 2 would or under [`MIN`].
fn scale(side: u32, max: u32) -> Option<u32> {
    [4, 2].into_iter().find(|s| side >= MIN && side * s <= max)
}

/// Upscales each `.dds` file of `missing` into its cache file: their top levels, halved down to the side each is
/// upscaled from, as PNGs into `work`, one run of the binary over them, and each file it saves halved where 4x passes
/// its largest side and encoded while it makes the next. The largest go first, so the last encodes are short.
/// `progress` is told (done, total) before each.
fn make(
    upscaler: &Upscaler,
    mut missing: Vec<(PathBuf, Source)>,
    work: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    missing.sort_by_key(|&(_, (_, side, max))| std::cmp::Reverse(side * scale(side, max).unwrap_or(1)));
    let (input, output) = (work.join("in"), work.join("out"));
    for dir in [&input, &output] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let name = |index: usize| format!("{index:04}.png");
    for (index, (path, (bytes, side, _))) in missing.iter().enumerate() {
        let written = dds::parse(bytes).map_err(|e| e.to_string()).and_then(|dds| {
            let level = &dds.levels()[0];
            let (mut width, mut height) = (level.width, level.height);
            let mut rgba = decompress(dds.format, level.data, width, height);
            while width.max(height) > *side {
                (width, height, rgba) = half(width, height, &rgba);
            }
            write_png(&input.join(name(index)), width, height, &rgba).map_err(|e| e.to_string())
        });
        if let Err(e) = written {
            eprintln!("upscale: {}: {e}", path.display());
        }
    }
    let total = missing.len();
    progress(0, total);
    let mut child = upscaler.spawn(&input, &output)?;
    let stderr = child.stderr.take().ok_or("no standard error")?;
    let (send, saved) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let file = line.strip_suffix(" done").and_then(|line| line.rsplit(['/', '\\']).next());
            if let Some(index) = file.and_then(|file| file.strip_suffix(".png")?.parse::<usize>().ok()) {
                let _ = send.send(index);
            }
        }
    });
    let mut done = vec![false; total];
    let mut take = |index: usize, done: &mut [bool]| {
        let (path, (bytes, side, max)) = &missing[index];
        if let Err(e) = finish(bytes, *side, *max, &output.join(name(index)), path) {
            eprintln!("upscale: {}: {e}", path.display());
        }
        done[index] = true;
        let count = done.iter().filter(|&&done| done).count();
        if count < total {
            progress(count, total);
        }
    };
    for index in saved.iter().filter(|&index| index < total) {
        take(index, &mut done);
    }
    let _ = reader.join();
    let status = child.wait().map_err(|e| e.to_string())?;
    // A file the binary saved without naming it (another build's messages).
    for index in 0..total {
        if !done[index] && output.join(name(index)).is_file() {
            take(index, &mut done);
        }
    }
    status.success().then_some(()).ok_or_else(|| format!("{} failed: {status}", upscaler.binary.display()))
}

/// Encodes the upscaled PNG `upscaled` of the `.dds` file `bytes`, upscaled from larger side `side`, into `out`, halved
/// where 4x passes `max`. Written whole, then renamed, so a stopped load leaves no part file in its place.
fn finish(bytes: &[u8], side: u32, max: u32, upscaled: &Path, out: &Path) -> Result<(), String> {
    let dds = dds::parse(bytes).map_err(|e| e.to_string())?;
    let (mut width, mut height, mut rgba) = read_png(upscaled).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(upscaled);
    if scale(side, max) == Some(2) {
        (width, height, rgba) = half(width, height, &rgba);
    }
    if let Some(folder) = out.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    let part = out.with_extension("part");
    std::fs::write(&part, encode(dds.format, width, height, rgba)).map_err(|e| e.to_string())?;
    std::fs::rename(&part, out).map_err(|e| e.to_string())
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

/// A one-level block-compressed file with its mip chain down to 1x1 appended, each level the box-filtered one above
/// re-encoded in the file's format (the top level stays as it is): `None` when it is plain RGBA, already has levels or
/// is under 8 texels. Some RSF cars' 4096x4096 liveries have no levels and alias in the distance.
pub fn with_block_mips(bytes: &[u8]) -> Option<Vec<u8>> {
    let file = dds::parse(bytes).ok()?;
    if file.format == Format::Rgba8 || file.mip_count != 1 || file.width.max(file.height) < 8 {
        return None;
    }
    let (mut width, mut height) = (file.width, file.height);
    let mut rgba = decompress(file.format, file.data, width, height);
    let mut out = bytes[..128 + file.data.len()].to_vec();
    let mut count = 1u32;
    while width > 1 || height > 1 {
        (width, height, rgba) = half(width, height, &rgba);
        let format = bc(file.format);
        let mut block = vec![0; format.compressed_size(width as usize, height as usize)];
        let params = texpresso::Params { algorithm: texpresso::Algorithm::RangeFit, ..Default::default() };
        format.compress(&rgba, width as usize, height as usize, params, &mut block);
        out.extend_from_slice(&block);
        count += 1;
    }
    out[28..32].copy_from_slice(&count.to_le_bytes());
    Some(out)
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
        assert_eq!(larger_side(&file(1024)), Some(1024));
        assert_eq!(scale(1024, MAX), Some(4));
        assert_eq!(scale(2048, MAX), Some(2));
        assert_eq!(scale(4096, MAX), None);
        assert_eq!(scale(16, MAX), None);
        assert_eq!(scale(2048, INTERIOR_MAX), Some(4));
        assert_eq!(scale(4096, INTERIOR_MAX), Some(2));
        assert_eq!((interior_max(1080), interior_max(2160)), (MAX, INTERIOR_MAX));
    }

    #[test]
    fn a_one_level_block_file_gains_its_chain() {
        let full = file(16);
        let top = dds::parse(&full).unwrap().levels()[0].data.to_vec();
        let mut one = full[..128].to_vec();
        one[28..32].copy_from_slice(&1u32.to_le_bytes());
        one.extend_from_slice(&top);
        let bytes = with_block_mips(&one).unwrap();
        let dds = dds::parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.mip_count, dds.levels()[0].data), (16, 5, top.as_slice()));
        // Already mipped: left as it is.
        assert_eq!(with_block_mips(&bytes), None);
    }

    #[test]
    fn encode_reads_back_with_mips() {
        let bytes = file(16);
        let dds = dds::parse(&bytes).unwrap();
        assert_eq!((dds.width, dds.height, dds.format), (16, 16, Format::Dxt1));
        assert_eq!(dds.levels().len(), 5);
    }
}

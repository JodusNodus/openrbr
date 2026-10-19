//! Builds an upscaled texture pack for one stage, for `viewer --textures`.
//!
//! ```text
//! RBR_DATA=<install> texture-pack [STAGE] <dir> [--weather W] [--wear W] [--model NAME] [--max PX]
//! ```
//!
//! Each block-compressed texture the stage draws with for that weather and wear is decoded to
//! `<dir>/source/`, upscaled 4x by Upscayl's Real-ESRGAN binary into `<dir>/upscaled/`, and encoded into
//! `<dir>/pack/<request path>` in its original format (DXT1 or DXT3) with a full mip chain, the largest level
//! at most `--max` pixels (default 4096). Each step skips what already exists, so a run resumes. `--model` is an
//! Upscayl model name (default `upscayl-standard-4x`). `RBR_UPSCALER` replaces the binary (default the Upscayl
//! app's); the models are in `../models` from it.
//!
//! Not the game's behaviour: the pack is a local add-on derived from the retail data, never committed.

use rbr_formats::data::Data;
use rbr_formats::dds::{self, Format};
use rbr_render::stage::{self, Image, Selection, Wear, Weather};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const UPSCALER: &str = "/Applications/Upscayl.app/Contents/Resources/bin/upscayl-bin";
const MODEL: &str = "upscayl-standard-4x";
const SCALE: u32 = 4;
const MAX: u32 = 4096;

struct Args {
    stage: String,
    dir: PathBuf,
    selection: Selection,
    model: String,
    max: u32,
}

/// A texture to upscale: its request path and the source file's format.
struct Job {
    request: String,
    format: Format,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("texture-pack: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args().skip(1))?;
    let data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let maps = data.install().join("Maps");
    let mut stage = stage::load(&maps, &args.stage, args.selection).map_err(|e| e.to_string())?;
    stage.load_textures(&maps).map_err(|e| e.to_string())?;

    let (source, upscaled, pack) = (args.dir.join("source"), args.dir.join("upscaled"), args.dir.join("pack"));
    std::fs::create_dir_all(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut jobs = Vec::new();
    for texture in stage.textures.values() {
        let Some(Image::Dds(bytes)) = &texture.image else { continue };
        let Ok(dds) = dds::parse(bytes) else { continue };
        let png = source.join(flat_name(&texture.request));
        if !png.exists() {
            let level = &dds.levels()[0];
            let rgba = decompress(dds.format, level.data, level.width, level.height);
            write_png(&png, level.width, level.height, &rgba).map_err(|e| format!("{}: {e}", png.display()))?;
        }
        jobs.push(Job { request: texture.request.clone(), format: dds.format });
    }
    // Several draws can share one file.
    jobs.sort_by(|a, b| a.request.cmp(&b.request));
    jobs.dedup_by(|a, b| a.request == b.request);
    eprintln!("{} textures", jobs.len());

    if jobs.iter().any(|job| !upscaled.join(flat_name(&job.request)).exists()) {
        upscale(&source, &upscaled, &args.model)?;
    }

    for job in &jobs {
        let out = pack.join(job.request.replace('\\', "/"));
        if out.exists() {
            continue;
        }
        let png = upscaled.join(flat_name(&job.request));
        let (width, height, rgba) = read_png(&png).map_err(|e| format!("{}: {e}", png.display()))?;
        let bytes = encode(job.format, width, height, rgba, args.max);
        std::fs::create_dir_all(out.parent().expect("request has a folder")).map_err(|e| e.to_string())?;
        std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))?;
        eprintln!("{} {width}x{height}", job.request);
    }
    eprintln!("pack: {}", pack.display());
    Ok(())
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut positional = Vec::new();
    let mut selection = Selection::default();
    let mut model = MODEL.to_owned();
    let mut max = MAX;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--weather" => {
                let value = args.next().ok_or("--weather needs dry, damp or wet")?;
                selection.weather =
                    Weather::parse(&value).ok_or_else(|| format!("unknown weather {value}; use dry, damp or wet"))?;
            }
            "--wear" => {
                let value = args.next().ok_or("--wear needs new, normal or worn")?;
                selection.wear =
                    Wear::parse(&value).ok_or_else(|| format!("unknown wear {value}; use new, normal or worn"))?;
            }
            "--model" => model = args.next().ok_or("--model needs a model name")?,
            "--max" => {
                let value = args.next().ok_or("--max needs pixels")?;
                max = value.parse().map_err(|e| format!("--max {value}: {e}"))?;
            }
            option if option.starts_with("--") => return Err(format!("unknown option {option}")),
            _ => positional.push(arg),
        }
    }
    let (stage, dir) = match <[String; 2]>::try_from(positional) {
        Ok([stage, dir]) => (stage, dir),
        Err(positional) => match <[String; 1]>::try_from(positional) {
            Ok([dir]) => (stage::DEFAULT_STAGE.to_owned(), dir),
            Err(_) => return Err("usage: texture-pack [STAGE] <dir> [options]".to_owned()),
        },
    };
    Ok(Args { stage, dir: PathBuf::from(dir), selection, model, max })
}

/// The request path as one file name: `track-10_o_textures\dry\x.dds` is `track-10_o_textures__dry__x.png`.
fn flat_name(request: &str) -> String {
    let stem = request.strip_suffix(".dds").unwrap_or(request);
    format!("{}.png", stem.replace('\\', "__"))
}

fn upscale(source: &Path, upscaled: &Path, model: &str) -> Result<(), String> {
    let binary = std::env::var_os("RBR_UPSCALER").map_or_else(|| PathBuf::from(UPSCALER), PathBuf::from);
    let models = binary.parent().and_then(Path::parent).map(|root| root.join("models")).ok_or("no models folder")?;
    std::fs::create_dir_all(upscaled).map_err(|e| format!("{}: {e}", upscaled.display()))?;
    let status = Command::new(&binary)
        .arg("-i")
        .arg(source)
        .arg("-o")
        .arg(upscaled)
        .arg("-m")
        .arg(&models)
        .args(["-n", model, "-s", &SCALE.to_string(), "-f", "png"])
        .status()
        .map_err(|e| format!("{}: {e}", binary.display()))?;
    status.success().then_some(()).ok_or_else(|| format!("{} failed: {status}", binary.display()))
}

fn bc(format: Format) -> texpresso::Format {
    match format {
        Format::Dxt1 => texpresso::Format::Bc1,
        Format::Dxt3 => texpresso::Format::Bc2,
        // Plain RGBA (a community stage's) is packed as DXT5, which keeps its alpha.
        Format::Dxt5 | Format::Rgba8 => texpresso::Format::Bc3,
    }
}

fn decompress(format: Format, data: &[u8], width: u32, height: u32) -> Vec<u8> {
    if format == Format::Rgba8 {
        return data.to_vec();
    }
    let mut rgba = vec![0; (width * height * 4) as usize];
    bc(format).decompress(data, width as usize, height as usize, &mut rgba);
    rgba
}

/// A DDS file in `format` with the full mip chain of `rgba`, starting at the first level no wider or taller
/// than `max`.
fn encode(format: Format, mut width: u32, mut height: u32, mut rgba: Vec<u8>, max: u32) -> Vec<u8> {
    let mut levels = Vec::new();
    loop {
        if width.max(height) <= max {
            let mut block = vec![0; bc(format).compressed_size(width as usize, height as usize)];
            let params = texpresso::Params::default();
            bc(format).compress(&rgba, width as usize, height as usize, params, &mut block);
            levels.push((width, height, block));
        }
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
fn half(width: u32, height: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
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

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// A PNG as RGBA8. An image without alpha reads as opaque.
fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
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

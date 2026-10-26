//! Builds an upscaled texture pack for one stage or car, for `viewer --textures`.
//!
//! ```text
//! RBR_DATA=<install> texture-pack [STAGE] <dir> [--car NAME | --rsf-car PACK|ID] [--weather W] [--wear W]
//!     [--model NAME] [--max PX]
//! ```
//!
//! Each block-compressed texture the stage draws with for that weather and wear (with `--car`, the car's exterior
//! and interior sets' instead; `--rsf-car` as the viewer's) is decoded to `<dir>/source/x<s>/`, upscaled by a
//! Real-ESRGAN ncnn binary into `<dir>/upscaled/x<s>/`, and encoded into `<dir>/pack/<request path>` in its original
//! format (DXT1 or DXT3) with a full mip chain. The scale `s` is 4, else 2 where 4 would pass `--max` pixels
//! (default 4096: the Fiesta R5's liveries look as sharp as at 8192, at a third of the memory); a texture that 2
//! would pass is left out. Each step skips what already exists, so a run resumes. `--model` is the model name, `{s}`
//! replaced by the scale (default `realesr-animevideov3-x{s}`, the compact model: on retail liveries and signs
//! cleaner than `realesrgan-x4plus` and about 5x faster). `RBR_UPSCALER` is the binary (default
//! `realesrgan-ncnn-vulkan` on the path); its models are in `models` beside it or `../models` (Upscayl's app).
//!
//! Not the game's behaviour: the pack is a local add-on derived from the retail data, never committed.

use rbr_formats::data::Data;
use rbr_formats::dds::{self, Format};
use rbr_render::car;
use rbr_render::stage::{self, Image, Selection, StageTexture, Wear, Weather};
use rbr_render::upscale::{decompress, encode, read_png, write_png};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const UPSCALER: &str = "realesrgan-ncnn-vulkan";
const MODEL: &str = "realesr-animevideov3-x{s}";
const SCALES: [u32; 2] = [4, 2];
const MAX: u32 = 4096;

struct Args {
    stage: String,
    car: Option<String>,
    rsf_car: Option<String>,
    dir: PathBuf,
    selection: Selection,
    model: String,
    max: u32,
}

/// A texture to upscale: its request path, the source file's format and the scale.
struct Job {
    request: String,
    format: Format,
    scale: u32,
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
    let mut data = Data::from_env().ok_or("RBR_DATA is not set")?;
    let mut car_name = args.car.clone();
    if let Some(key) = &args.rsf_car {
        let root = std::env::var_os("RBR_RSF").map(PathBuf::from);
        let selected = rbr_formats::rsf::select(data, key, rbr_formats::rsf::SLOT, root.as_deref())?;
        car_name.get_or_insert_with(|| selected.folder().to_owned());
        data = selected.data;
    }
    let textures: Vec<StageTexture> = match &car_name {
        Some(name) => {
            let exterior = car::load(&data, name, car::placement_at(None)).map_err(|e| e.to_string())?;
            let interior = car::load_interior(&data, name, car::placement_at(None)).map_err(|e| e.to_string())?;
            exterior.textures.into_values().chain(interior.textures.into_values()).collect()
        }
        None => {
            let maps = data.install().join("Maps");
            let mut stage = stage::load(&maps, &args.stage, args.selection).map_err(|e| e.to_string())?;
            stage.load_textures(&maps).map_err(|e| e.to_string())?;
            stage.textures.into_values().collect()
        }
    };

    let (source, upscaled, pack) = (args.dir.join("source"), args.dir.join("upscaled"), args.dir.join("pack"));
    let scaled = |dir: &Path, scale: u32| dir.join(format!("x{scale}"));
    let mut jobs = Vec::new();
    for texture in &textures {
        let Some(Image::Dds(bytes)) = &texture.image else { continue };
        let Ok(dds) = dds::parse(bytes) else { continue };
        // Ours (the crack decal) are made, not read.
        if !texture.request.to_ascii_lowercase().ends_with(".dds") {
            continue;
        }
        let level = &dds.levels()[0];
        let Some(scale) = SCALES.into_iter().find(|s| level.width.max(level.height) * s <= args.max) else {
            eprintln!("{} {}x{}: over --max at 2x, left out", texture.request, level.width, level.height);
            continue;
        };
        let dir = scaled(&source, scale);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let png = dir.join(flat_name(&texture.request));
        if !png.exists() {
            let rgba = decompress(dds.format, level.data, level.width, level.height);
            write_png(&png, level.width, level.height, &rgba).map_err(|e| format!("{}: {e}", png.display()))?;
        }
        jobs.push(Job { request: texture.request.clone(), format: dds.format, scale });
    }
    // Several draws can share one file.
    jobs.sort_by(|a, b| a.request.cmp(&b.request));
    jobs.dedup_by(|a, b| a.request == b.request);
    eprintln!("{} textures", jobs.len());

    for scale in SCALES {
        if jobs.iter().any(|job| job.scale == scale && !scaled(&upscaled, scale).join(flat_name(&job.request)).exists())
        {
            upscale(
                &scaled(&source, scale),
                &scaled(&upscaled, scale),
                &args.model.replace("{s}", &scale.to_string()),
                scale,
            )?;
        }
    }

    for job in &jobs {
        let out = pack.join(job.request.replace('\\', "/"));
        if out.exists() {
            continue;
        }
        let png = scaled(&upscaled, job.scale).join(flat_name(&job.request));
        let (width, height, rgba) = read_png(&png).map_err(|e| format!("{}: {e}", png.display()))?;
        let bytes = encode(job.format, width, height, rgba);
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
    let mut car = None;
    let mut rsf_car = None;
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
            "--car" => car = Some(args.next().ok_or("--car needs a car name")?),
            "--rsf-car" => rsf_car = Some(args.next().ok_or("--rsf-car needs a pack or an id")?),
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
    Ok(Args { stage, car, rsf_car, dir: PathBuf::from(dir), selection, model, max })
}

/// The request path as one file name: `track-10_o_textures\dry\x.dds` is `track-10_o_textures__dry__x.png`.
fn flat_name(request: &str) -> String {
    let stem = request.strip_suffix(".dds").unwrap_or(request);
    format!("{}.png", stem.replace(['\\', '/'], "__"))
}

fn upscale(source: &Path, upscaled: &Path, model: &str, scale: u32) -> Result<(), String> {
    let binary = std::env::var_os("RBR_UPSCALER").map_or_else(|| PathBuf::from(UPSCALER), PathBuf::from);
    let folder = binary.parent().unwrap_or(Path::new(""));
    let models = [folder.join("models"), folder.join("../models")].into_iter().find(|dir| dir.is_dir());
    std::fs::create_dir_all(upscaled).map_err(|e| format!("{}: {e}", upscaled.display()))?;
    let mut command = Command::new(&binary);
    command.arg("-i").arg(source).arg("-o").arg(upscaled);
    if let Some(models) = models {
        command.arg("-m").arg(models);
    }
    let status = command
        .args(["-n", model, "-s", &scale.to_string(), "-f", "png"])
        .status()
        .map_err(|e| format!("{}: {e}", binary.display()))?;
    status.success().then_some(()).ok_or_else(|| format!("{} failed: {status}", binary.display()))
}

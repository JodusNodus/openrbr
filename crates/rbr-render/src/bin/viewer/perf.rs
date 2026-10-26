//! `--perf-log`: the session's header and the renderer's feed (`rbr_render::perf_log`).

use crate::args::Args;
use rbr_render::perf_log;
use rbr_render::renderer::Renderer;

/// Starts feeding the log from `renderer` and writes the header: the machine, the GPU and what the session draws.
pub fn begin(
    args: &Args,
    adapter: &wgpu::Adapter,
    (width, height): (u32, u32),
    present: String,
    renderer: &mut Renderer,
) {
    let gpu_timed = renderer.enable_perf_log();
    let info = adapter.get_info();
    let scale = if args.enhance.hdr { args.render_scale.min(1.0) } else { 1.0 };
    let side = |side: u32| ((side as f32 * scale).round() as u32).max(1);
    let enhance = format!("{:?}", args.enhance);
    let enhance = enhance.trim_start_matches("Enhance { ").trim_end_matches(" }");
    let lines = [
        ("rerbr", env!("CARGO_PKG_VERSION").to_owned()),
        ("os", os()),
        ("backend", format!("{:?}", info.backend)),
        ("adapter", format!("{} ({:?})", info.name, info.device_type)),
        ("driver", format!("{} {}", info.driver, info.driver_info)),
        ("window", format!("{width}x{height}{}", if args.fullscreen { " fullscreen" } else { "" })),
        ("render size", format!("{}x{}", side(width), side(height))),
        ("render scale", args.render_scale.to_string()),
        ("present mode", present),
        ("anti-aliasing", args.enhance.anti_aliasing.name().to_owned()),
        ("reflections", args.enhance.reflections.name().to_owned()),
        ("shadows", args.enhance.shadows.name().to_owned()),
        ("enhancements", enhance.to_owned()),
        ("stage", args.stage.clone().unwrap_or_else(|| "none".into())),
        ("car", args.car.clone().unwrap_or_else(|| "none".into())),
        ("weather", format!("{:?}, wear {:?}", args.selection.weather, args.selection.wear)),
        (
            "gpu timing",
            if gpu_timed { "passes timed" } else { "none on this device (CPU spans and frame times only)" }.to_owned(),
        ),
    ];
    perf_log::header(&lines);
}

/// The system's name: its distribution on Linux.
fn os() -> String {
    let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let name = release.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|n| n.trim_matches('"').to_owned());
    format!("{} {}{}", std::env::consts::OS, std::env::consts::ARCH, name.map_or(String::new(), |n| format!(", {n}")))
}

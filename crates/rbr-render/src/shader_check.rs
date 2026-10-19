//! Every WGSL module the renderer builds, translated to SPIR-V the way wgpu's Vulkan backend does it (the user plays
//! on Linux/Vulkan; the renderer is mostly run on Metal): parsed, validated with the capabilities wgpu allows on a
//! device with no optional shader features, its overrides set per entry point, and written with the backend's SPIR-V
//! options (`wgpu-hal` `vulkan::adapter` and `vulkan::device::compile_stage`).

use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};

/// The capabilities `wgpu-core`'s `create_validator` gives a Vulkan device with every downlevel flag and none of the
/// optional shader features ([`crate::renderer::request_gpu`] asks for BC compression and timestamps only).
fn capabilities() -> Capabilities {
    Capabilities::MULTISAMPLED_SHADING | Capabilities::CUBE_ARRAY_TEXTURES | Capabilities::SHADER_FLOAT16_IN_FLOAT32
}

/// The backend's writer options on a Vulkan 1.3 device without robust buffer or image access (the bounds checks
/// written in), its SPIR-V capabilities the ones every device has plus the downlevel ones.
fn options() -> spv::Options<'static> {
    spv::Options {
        lang_version: (1, 6),
        flags: spv::WriterFlags::LABEL_VARYINGS | spv::WriterFlags::FORCE_POINT_SIZE,
        capabilities: Some(
            [
                spv::Capability::Shader,
                spv::Capability::Matrix,
                spv::Capability::Sampled1D,
                spv::Capability::Image1D,
                spv::Capability::ImageQuery,
                spv::Capability::DerivativeControl,
                spv::Capability::StorageImageExtendedFormats,
                spv::Capability::SampledCubeArray,
                spv::Capability::SampleRateShading,
            ]
            .into_iter()
            .collect(),
        ),
        bounds_check_policies: naga::proc::BoundsCheckPolicies {
            index: naga::proc::BoundsCheckPolicy::Restrict,
            buffer: naga::proc::BoundsCheckPolicy::Restrict,
            image_load: naga::proc::BoundsCheckPolicy::Restrict,
            binding_array: naga::proc::BoundsCheckPolicy::Unchecked,
        },
        zero_initialize_workgroup_memory: spv::ZeroInitializeWorkgroupMemoryMode::Polyfill,
        force_loop_bounding: true,
        ..Default::default()
    }
}

/// Builds `source` for every entry point with each of `constants` (override name and value pairs), as pipelines do.
/// Returns the failures.
fn check(name: &str, source: &str, constants: &[Vec<(&str, f64)>]) -> Vec<String> {
    let module = match naga::front::wgsl::parse_str(source) {
        Ok(module) => module,
        Err(e) => return vec![format!("{name}: parse: {}", e.emit_to_string(source))],
    };
    let info = match Validator::new(ValidationFlags::all(), capabilities()).validate(&module) {
        Ok(info) => info,
        Err(e) => return vec![format!("{name}: validate: {}", e.emit_to_string(source))],
    };
    let options = options();
    let mut failures = Vec::new();
    for set in constants {
        let set: naga::back::PipelineConstants = set.iter().map(|&(k, v)| (k.to_owned(), v)).collect();
        for entry in &module.entry_points {
            let at = format!("{name} {} {set:?}", entry.name);
            let (module, info) = match naga::back::pipeline_constants::process_overrides(
                &module,
                &info,
                Some((entry.stage, &entry.name)),
                &set,
            ) {
                Ok(done) => done,
                Err(e) => {
                    failures.push(format!("{at}: overrides: {e}"));
                    continue;
                }
            };
            let pipeline = spv::PipelineOptions { shader_stage: entry.stage, entry_point: entry.name.clone() };
            if let Err(e) = spv::write_vec(&module, &info, &options, Some(&pipeline)) {
                failures.push(format!("{at}: spv: {e}"));
            }
        }
    }
    failures
}

/// Every combination of `names` set to 0 or 1.
fn flags<'a>(names: &[&'a str]) -> Vec<Vec<(&'a str, f64)>> {
    (0..1u32 << names.len())
        .map(|bits| names.iter().enumerate().map(|(i, &n)| (n, f64::from(bits >> i & 1))).collect())
        .collect()
}

#[test]
fn every_shader_builds_for_vulkan() {
    let none = || vec![Vec::new()];
    let mut failures = Vec::new();
    failures.extend(check(
        "stage",
        crate::renderer::SHADER,
        &flags(&["ALPHA_TEST", "WATER_PLUS", "CABIN", "SKY_LUT", "MIRRORS"]),
    ));
    failures.extend(check("rain", &crate::effects::rain_source(), &flags(&["HDR"])));
    failures.extend(check("particles", &crate::particles::gpu::source(), &flags(&["HDR"])));
    failures.extend(check("post", crate::post::SHADER, &flags(&["HDR", "SHAFTS"])));
    failures.extend(check("light shafts", crate::light_shafts::SHADER, &none()));
    failures.extend(check("depth copy", crate::depth_copy::SHADER, &none()));
    failures.extend(check("skid marks", crate::skidmarks_pass::SHADER, &flags(&["HDR"])));
    failures.extend(check("glass", crate::glass::gpu::SHADER, &flags(&["HDR"])));
    failures.extend(check("sun", crate::sun::SHADER, &none()));
    failures.extend(check("skylight", crate::skylight::SHADER, &none()));
    failures.extend(check("dials", crate::dials::gpu::SHADER, &none()));
    failures.extend(check("mip blit", crate::renderer::BLIT, &none()));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

//! The GPU: the wgpu instance, its adapter, device and queue.

/// A GPU adapter with its device and queue (handles: a clone shares them).
#[derive(Clone)]
pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Why no usable GPU was found.
#[derive(Debug)]
pub enum GpuError {
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    /// The adapter lacks BC texture compression, which the DDS textures need.
    NoBcCompression,
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::Adapter(e) => write!(f, "no GPU adapter: {e}"),
            GpuError::Device(e) => write!(f, "no GPU device: {e}"),
            GpuError::NoBcCompression => write!(f, "GPU lacks BC texture compression"),
        }
    }
}

impl std::error::Error for GpuError {}

/// The wgpu instance: Vulkan, Metal or DX12 only unless `WGPU_BACKEND` names others. Left in, the GL backend's EGL
/// instance crashed NVIDIA's EGL on Wayland when dropped at exit, which lost the exit's output.
pub fn instance() -> wgpu::Instance {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        descriptor.backends = wgpu::Backends::PRIMARY;
    }
    wgpu::Instance::new(descriptor)
}

/// Asks the system for an adapter that can draw to `compatible_surface` (or to any target when
/// `None`), with BC texture compression, and timestamps inside encoders where it has them (`viewer --profile`).
pub fn request_gpu(instance: &wgpu::Instance, compatible_surface: Option<&wgpu::Surface<'_>>) -> Result<Gpu, GpuError> {
    let adapter = futures_executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface,
        apply_limit_buckets: false,
    }))
    .map_err(GpuError::Adapter)?;
    if !adapter.features().contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
        return Err(GpuError::NoBcCompression);
    }
    let (device, queue) = futures_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("rbr-render"),
        // FSR (`fsr.rs`) keeps its history in storage formats past WebGPU's, where the adapter has them.
        required_features: wgpu::Features::TEXTURE_COMPRESSION_BC
            | adapter.features()
                & (wgpu::Features::TIMESTAMP_QUERY
                    | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                    | wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES),
        // The reflection probes take six layers each (`probes::MOST`).
        // The reflections' fragment stage reads 17 sampled textures, past the default 16, which wgpu 30 checks at
        // the pipeline layout; Metal and desktop Vulkan allow far more.
        // Some RSF stages carry textures past the default 8192 texels (484's 16384 x 8192 backdrop).
        required_limits: wgpu::Limits {
            max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
            max_texture_array_layers: adapter.limits().max_texture_array_layers.min(6 * super::probes::MOST as u32),
            max_sampled_textures_per_shader_stage: adapter.limits().max_sampled_textures_per_shader_stage,
            ..Default::default()
        },
        ..Default::default()
    }))
    .map_err(GpuError::Device)?;
    Ok(Gpu { adapter, device, queue })
}

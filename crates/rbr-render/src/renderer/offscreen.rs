//! Drawing a frame into a texture and reading its pixels back.

use super::frame::FrameStats;
use super::Renderer;
use glam::Mat4;

/// Why an offscreen frame could not be read back.
#[derive(Debug)]
pub enum ReadbackError {
    Poll(wgpu::PollError),
    Map(wgpu::BufferAsyncError),
}

impl std::fmt::Display for ReadbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadbackError::Poll(e) => write!(f, "GPU poll failed: {e}"),
            ReadbackError::Map(e) => write!(f, "readback buffer did not map: {e}"),
        }
    }
}

impl std::error::Error for ReadbackError {}

impl Renderer {
    /// Draws one frame into a texture of this renderer's format and returns its counts and its pixels
    /// (`width * height * 4` bytes, rows top to bottom). Only for targets in `Rgba8Unorm`.
    pub fn render_offscreen(
        &mut self,
        size: (u32, u32),
        view_projection: Mat4,
        sky_view_projection: Mat4,
    ) -> Result<(FrameStats, Vec<u8>), ReadbackError> {
        self.render_offscreen_with(size, view_projection, sky_view_projection, |_, _| {})
    }

    /// [`Renderer::render_offscreen`] with `overlay` drawn into the frame after the stage (the viewer's HUD).
    pub fn render_offscreen_with(
        &mut self,
        size: (u32, u32),
        view_projection: Mat4,
        sky_view_projection: Mat4,
        overlay: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<(FrameStats, Vec<u8>), ReadbackError> {
        let (width, height) = size;
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let started = std::time::Instant::now();
        let stats = self.render(&mut encoder, &view, size, view_projection, sky_view_projection);
        if let Some(profiler) = &mut self.profiler {
            profiler.add("frame (profiled)", started.elapsed());
        }
        overlay(&mut encoder, &view);

        let row_bytes = width * 4;
        let padded_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        let (sender, receiver) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            // The receiver waits below, so the send cannot fail.
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(ReadbackError::Poll)?;
        receiver.recv().expect("map callback runs before poll returns").map_err(ReadbackError::Map)?;

        let mapped = readback.slice(..).get_mapped_range();
        let pixels =
            mapped.chunks_exact(padded_row as usize).flat_map(|row| &row[..row_bytes as usize]).copied().collect();
        drop(mapped);
        readback.unmap();
        if let Some(profiler) = &mut self.profiler {
            profiler.end_frame();
        }
        Ok((stats, pixels))
    }
}

//! The gravel scan of [`Enhance::gravel`](super::Enhance::gravel) (`gravel_scan` in `shader.wgsl`; ours): Poly Haven's
//! "Gravel Ground 01" by Rob Tuytel (CC0, `notes/references/README.md`), 3 m across, packed into one RGBA texture by
//! `scripts/pack-gravel.py`: red its brightness, green and blue its OpenGL normal's x and y, alpha its height.

use std::io::Cursor;

/// The packed scan, 1024 texels a side.
const SCAN: &[u8] = include_bytes!("../../data/gravel.png");

/// The scan with its mips and its repeating sampler; with `on` false a 1x1 placeholder the shader never reads.
pub(super) fn load(device: &wgpu::Device, queue: &wgpu::Queue, on: bool) -> (wgpu::TextureView, wgpu::Sampler) {
    let (mut width, mut height, mut rgba) = match on {
        true => decode(SCAN).expect("the packed gravel scan reads"),
        false => (1, 1, vec![128; 4]),
    };
    let levels = 32 - width.max(height).leading_zeros();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gravel"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for level in 0..levels {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * width), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        if level + 1 < levels {
            (width, height, rgba) = crate::upscale::half(width, height, &rgba);
        }
    }
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("gravel"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    });
    (texture.create_view(&Default::default()), sampler)
}

/// An RGBA PNG's width, height and texels.
fn decode(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    let mut rgba = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut rgba)?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(format!("{:?} {:?}, not 8-bit RGBA", info.color_type, info.bit_depth).into());
    }
    rgba.truncate(info.buffer_size());
    Ok((info.width, info.height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scan_is_square_rgba() {
        let (width, height, rgba) = decode(SCAN).unwrap();
        assert_eq!((width, height), (1024, 1024));
        assert_eq!(rgba.len(), 1024 * 1024 * 4);
    }
}

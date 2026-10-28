//! The road scans (`road_scan` in `shader.wgsl`; ours), packed by `scripts/pack-scan.py` into one RGBA texture each:
//! red its brightness, green and blue its OpenGL normal's x and y, alpha its height. Layer 0 the gravel of
//! [`Enhance::gravel`](super::Enhance::gravel), Poly Haven's "Gravel Ground 01" by Rob Tuytel, 3 m across; layer 1 the
//! asphalt of [`Enhance::asphalt`](super::Enhance::asphalt), Poly Haven's "Asphalt Track" by Dimitrios Savva, 2 m across;
//! layer 2 the snow of [`Enhance::snow`](super::Enhance::snow), Poly Haven's "Snow 02" by Rob Tuytel, 2 m across (all CC0,
//! `notes/references/README.md`).

use std::io::Cursor;

/// The packed scans by layer, 1024 texels a side.
const SCANS: [&[u8]; 3] = [
    include_bytes!("../../data/gravel.png"),
    include_bytes!("../../data/asphalt.png"),
    include_bytes!("../../data/snow.png"),
];
/// Their side.
const SIDE: u32 = 1024;

/// The scans with their mips and their repeating sampler; with `on` false a 1x1 placeholder the shader never reads.
pub(super) fn load(device: &wgpu::Device, queue: &wgpu::Queue, on: bool) -> (wgpu::TextureView, wgpu::Sampler) {
    let side = if on { SIDE } else { 1 };
    let levels = 32 - side.leading_zeros();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("road scans"),
        size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: SCANS.len() as u32 },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, scan) in SCANS.iter().enumerate() {
        let (mut width, mut height, mut rgba) = match on {
            true => decode(scan).expect("a packed road scan reads"),
            false => (1, 1, vec![128; 4]),
        };
        for level in 0..levels {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 },
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
    }
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("road scans"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 16,
        ..Default::default()
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    (view, sampler)
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
    fn the_scans_are_square_rgba() {
        for scan in SCANS {
            let (width, height, rgba) = decode(scan).unwrap();
            assert_eq!((width, height), (SIDE, SIDE));
            assert_eq!(rgba.len(), (SIDE * SIDE * 4) as usize);
        }
    }
}

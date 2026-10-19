//! The start board's clock digits. HedgeHog3D 0x10070b20 draws them on the stage object `START_CLOCK`: eight quads
//! `h/10 h%10 : m/10 m%10 : s/10 s%10` textured from `Textures\start_clock_digits.dds` (128x16, 10-texel cells, 0-9
//! then the colon at 10, u step 0.078125), from the object's local corner (-0.4718, 0.4009) on, 0.122525 across
//! and 0.228 down a digit. That rect is the board mesh's face textured `START_CLOCK_DIGITS_PLACEHOLDER.tga` (`Dynamic`
//! in the track ini), whose uvs span `FACE`. Here the digits are written into that face's texture instead of drawn
//! as quads over it (ours): the same texels in the same place, alpha-blended over the placeholder's.

use rbr_formats::dds::{self, Format};

/// The board face's texture, by its `[TextureInfo]` file name's end.
pub const PLACEHOLDER: &str = "START_CLOCK_DIGITS_PLACEHOLDER.tga";
/// The digits' texture under `Textures`.
pub const DIGITS: &str = "start_clock_digits.dds";
/// The colon's cell.
const COLON: u8 = 10;
/// The u width of a cell (10 / 128).
const CELL_U: f32 = 0.078125;
/// The face's uv rect in the placeholder, u0 v0 u1 v1 (the board mesh's).
const FACE: [f32; 4] = [0.03125, 0.125, 0.96875, 0.875];
/// The face texture's size: the digits' 80x16 texels three times over inside `FACE`.
pub const SIZE: (u32, u32) = (256, 64);

/// The cells for a time of day in seconds: hours, minutes and seconds, each truncated, as `hud::clock_label`.
pub fn cells(seconds: f32) -> [u8; 8] {
    let s = seconds.max(0.0) as u32;
    let (h, m, s) = ((s / 3600 % 24) as u8, (s / 60 % 60) as u8, (s % 60) as u8);
    [h / 10, h % 10, COLON, m / 10, m % 10, COLON, s / 10, s % 10]
}

/// A decoded texture's top level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn decode(dds: &dds::Dds) -> Rgba {
        let (width, height) = (dds.width as usize, dds.height as usize);
        let data = dds.levels()[0].data;
        let mut pixels = vec![0u8; width * height * 4];
        let bc = match dds.format {
            Format::Dxt1 => texpresso::Format::Bc1,
            Format::Dxt3 => texpresso::Format::Bc2,
            Format::Dxt5 => texpresso::Format::Bc3,
            Format::Rgba8 => {
                let len = pixels.len();
                pixels.copy_from_slice(&data[..len]);
                return Rgba { width: dds.width, height: dds.height, pixels };
            }
        };
        bc.decompress(data, width, height, &mut pixels);
        Rgba { width: dds.width, height: dds.height, pixels }
    }

    /// The nearest texel to `(u, v)`, clamped.
    pub(crate) fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        let x = ((u * self.width as f32) as u32).min(self.width - 1);
        let y = ((v * self.height as f32) as u32).min(self.height - 1);
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].try_into().unwrap()
    }
}

/// The face texture ([`SIZE`]): the placeholder, with `cells` over its face when there is a time.
pub fn compose(placeholder: &Rgba, digits: Option<&Rgba>, cells: Option<[u8; 8]>) -> Vec<u8> {
    let (width, height) = SIZE;
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (u, v) = ((x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32);
            let mut texel = placeholder.sample(u, v);
            let fu = (u - FACE[0]) / (FACE[2] - FACE[0]);
            let fv = (v - FACE[1]) / (FACE[3] - FACE[1]);
            let inside = (0.0..1.0).contains(&fu) && (0.0..1.0).contains(&fv);
            if let (Some(digits), Some(cells), true) = (digits, cells, inside) {
                let cell = (fu * 8.0) as usize;
                let digit = digits.sample((f32::from(cells[cell]) + fu * 8.0 - cell as f32) * CELL_U, fv);
                let a = u16::from(digit[3]);
                for c in 0..3 {
                    texel[c] = ((u16::from(digit[c]) * a + u16::from(texel[c]) * (255 - a)) / 255) as u8;
                }
            }
            out.extend_from_slice(&texel);
        }
    }
    out
}

/// The board face's texture on the GPU and the time it shows.
pub struct StartBoard {
    texture: wgpu::Texture,
    placeholder: Rgba,
    digits: Option<Rgba>,
    shown: Option<[u8; 8]>,
}

impl StartBoard {
    /// The face texture, showing the placeholder until [`StartBoard::set`] gives it a time.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, placeholder: Rgba, digits: Option<Rgba>) -> StartBoard {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("start board"),
            size: wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let board = StartBoard { texture, placeholder, digits, shown: None };
        board.write(queue);
        board
    }

    pub fn view(&self) -> wgpu::TextureView {
        self.texture.create_view(&Default::default())
    }

    /// Shows the time of day `seconds` (`None`: the placeholder alone).
    pub fn set(&mut self, queue: &wgpu::Queue, seconds: Option<f32>) {
        let shown = seconds.map(cells);
        if shown != self.shown {
            self.shown = shown;
            self.write(queue);
        }
    }

    fn write(&self, queue: &wgpu::Queue) {
        let pixels = compose(&self.placeholder, self.digits.as_ref(), self.shown);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * SIZE.0), rows_per_image: Some(SIZE.1) },
            wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cells_are_the_boards() {
        assert_eq!(cells(8.0 * 3600.0 + 15.0 * 60.0 + 7.9), [0, 8, COLON, 1, 5, COLON, 0, 7]);
        assert_eq!(cells(25.0 * 3600.0), [0, 1, COLON, 0, 0, COLON, 0, 0]);
    }

    /// Each cell of the face shows its digit's texels: an opaque digit texture's colour by cell index.
    #[test]
    fn the_digits_fill_the_face() {
        let placeholder = Rgba { width: 1, height: 1, pixels: vec![0, 0, 0, 255] };
        let pixels = (0..128u32).flat_map(|x| [(x / 10) as u8, 0, 0, 255]).collect::<Vec<_>>().repeat(16);
        let digits = Rgba { width: 128, height: 16, pixels };
        let face = compose(&placeholder, Some(&digits), Some([1, 2, COLON, 3, 4, COLON, 5, 6]));
        let at = |x: u32, y: u32| face[((y * SIZE.0 + x) * 4) as usize];
        // Inside the face the 240 texels are 30 per cell; outside it the placeholder.
        let cell = |k: u32| at(8 + 30 * k + 15, 32);
        assert_eq!((0..8).map(cell).collect::<Vec<_>>(), [1, 2, 10, 3, 4, 10, 5, 6]);
        assert_eq!(at(2, 32), 0);
        assert_eq!(compose(&placeholder, Some(&digits), None)[(32 * SIZE.0 + 23) as usize * 4], 0);
    }
}

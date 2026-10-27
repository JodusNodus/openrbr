//! The start board's clock digits. HedgeHog3D 0x10070b20 draws them on the stage object `START_CLOCK`: eight quads
//! `h/10 h%10 : m/10 m%10 : s/10 s%10` textured from `Textures\start_clock_digits.dds` (128x16, 10-texel cells, 0-9
//! then the colon at 10, u step 0.078125), from the object's local corner (-0.4718, 0.4009) on, 0.122525 across
//! and 0.228 down a digit. That rect is the board mesh's face textured `START_CLOCK_DIGITS_PLACEHOLDER.tga` (`Dynamic`
//! in the track ini), whose uvs span `FACE`. Here the digits are written into that face's texture instead of drawn
//! as quads over it (ours): the same texels in the same place, alpha-blended over the placeholder's.
//!
//! The board's red and green lamps (`START_CLOCK_*LIGHT_ON.tga`, also `Dynamic`, their retail texels filler) are
//! switched by the race clock (ours, the game's switching not traced, after the electronic start clocks of rallies):
//! red while the countdown runs, green from the go for [`GREEN_SECONDS`], none after a jump start's go; an unlit
//! lamp is its texture darkened to [`UNLIT`]. By night and at dusk the digits glow (ours; the game has no night), and
//! the lit lamps at any hour, through emissive masks written with them (`Views::glows` in `renderer`).

use crate::live::RaceHud;
use rbr_formats::dds::{self, Format};

/// The board face's texture, by its `[TextureInfo]` file name's end.
pub const PLACEHOLDER: &str = "START_CLOCK_DIGITS_PLACEHOLDER.tga";
/// The digits' texture under `Textures`.
pub const DIGITS: &str = "start_clock_digits.dds";
/// The lamps' face textures, green then red, by their `[TextureInfo]` file names' ends.
pub const LAMPS: [&str; 2] = ["START_CLOCK_GREENLIGHT_ON.tga", "START_CLOCK_REDLIGHT_ON.tga"];
/// How long the green lamp stays lit after the go, s.
pub const GREEN_SECONDS: f32 = 5.0;
/// An unlit lamp's share of its texture's colour: the dark lens.
const UNLIT: u16 = 64;
/// The colon's cell.
const COLON: u8 = 10;
/// The u width of a cell (10 / 128).
const CELL_U: f32 = 0.078125;
/// The face's uv rect in the placeholder, u0 v0 u1 v1 (the board mesh's).
const FACE: [f32; 4] = [0.03125, 0.125, 0.96875, 0.875];
/// The face texture's size: the digits' 80x16 texels three times over inside `FACE`.
pub const SIZE: (u32, u32) = (256, 64);

/// The cells for a time of day in seconds: hours, minutes and seconds, each truncated.
pub fn cells(seconds: f32) -> [u8; 8] {
    let s = seconds.max(0.0) as u32;
    let (h, m, s) = ((s / 3600 % 24) as u8, (s / 60 % 60) as u8, (s % 60) as u8);
    [h / 10, h % 10, COLON, m / 10, m % 10, COLON, s / 10, s % 10]
}

/// Which lamps the race clock lights, green then red (`None`: no race, both unlit).
pub fn lamps(race: Option<&RaceHud>) -> [bool; 2] {
    let Some(race) = race else { return [false; 2] };
    let counting = !race.running && race.countdown > 0.0;
    let go = race.running && !race.jump_start && race.countdown > -GREEN_SECONDS;
    [go, counting]
}

/// A lamp's face ([`LAMPS`]) and its emissive mask: the texture, darkened when unlit; the mask white when lit.
pub fn lamp(face: &Rgba, lit: bool) -> (Vec<u8>, Vec<u8>) {
    let shade = |c: u8| if lit { c } else { (u16::from(c) * UNLIT / 255) as u8 };
    let texels = face.pixels.chunks_exact(4).flat_map(|t| [shade(t[0]), shade(t[1]), shade(t[2]), t[3]]).collect();
    let glow = if lit { 255 } else { 0 };
    (texels, [glow, glow, glow, 255].repeat(face.pixels.len() / 4))
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

/// The face texture ([`SIZE`]): the placeholder, with `cells` over its face when there is a time; and its emissive
/// mask, each digit texel's alpha in grey.
pub fn compose(placeholder: &Rgba, digits: Option<&Rgba>, cells: Option<[u8; 8]>) -> (Vec<u8>, Vec<u8>) {
    let (width, height) = SIZE;
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    let mut glow = Vec::with_capacity(out.capacity());
    for y in 0..height {
        for x in 0..width {
            let (u, v) = ((x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32);
            let mut texel = placeholder.sample(u, v);
            let mut lit = 0;
            let fu = (u - FACE[0]) / (FACE[2] - FACE[0]);
            let fv = (v - FACE[1]) / (FACE[3] - FACE[1]);
            let inside = (0.0..1.0).contains(&fu) && (0.0..1.0).contains(&fv);
            if let (Some(digits), Some(cells), true) = (digits, cells, inside) {
                let cell = (fu * 8.0) as usize;
                let digit = digits.sample((f32::from(cells[cell]) + fu * 8.0 - cell as f32) * CELL_U, fv);
                let a = u16::from(digit[3]);
                lit = digit[3];
                for c in 0..3 {
                    texel[c] = ((u16::from(digit[c]) * a + u16::from(texel[c]) * (255 - a)) / 255) as u8;
                }
            }
            out.extend_from_slice(&texel);
            glow.extend_from_slice(&[lit, lit, lit, 255]);
        }
    }
    (out, glow)
}

/// A texture the board writes and its emissive mask, on the GPU.
pub struct Dynamic {
    texture: wgpu::Texture,
    glow: wgpu::Texture,
}

impl Dynamic {
    fn new(device: &wgpu::Device, (width, height): (u32, u32)) -> Dynamic {
        let texture = |label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        Dynamic { texture: texture("start board"), glow: texture("start board glow") }
    }

    pub fn view(&self) -> wgpu::TextureView {
        self.texture.create_view(&Default::default())
    }

    pub fn glow_view(&self) -> wgpu::TextureView {
        self.glow.create_view(&Default::default())
    }

    fn write(&self, queue: &wgpu::Queue, (texels, glow): (Vec<u8>, Vec<u8>)) {
        for (texture, pixels) in [(&self.texture, texels), (&self.glow, glow)] {
            let size = texture.size();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * size.width),
                    rows_per_image: Some(size.height),
                },
                size,
            );
        }
    }
}

/// The board face's texture and the time it shows.
pub struct StartBoard {
    pub face: Dynamic,
    placeholder: Rgba,
    digits: Option<Rgba>,
    shown: Option<[u8; 8]>,
}

impl StartBoard {
    /// The face texture, showing the placeholder until [`StartBoard::set`] gives it a time.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, placeholder: Rgba, digits: Option<Rgba>) -> StartBoard {
        let board = StartBoard { face: Dynamic::new(device, SIZE), placeholder, digits, shown: None };
        board.face.write(queue, compose(&board.placeholder, board.digits.as_ref(), None));
        board
    }

    /// Shows the time of day `seconds` (`None`: the placeholder alone).
    pub fn set(&mut self, queue: &wgpu::Queue, seconds: Option<f32>) {
        let shown = seconds.map(cells);
        if shown != self.shown {
            self.shown = shown;
            self.face.write(queue, compose(&self.placeholder, self.digits.as_ref(), shown));
        }
    }
}

/// One of the board's lamps ([`LAMPS`]) and whether it is lit.
pub struct StartLamp {
    pub face: Dynamic,
    image: Rgba,
    lit: bool,
}

impl StartLamp {
    /// The lamp, unlit.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, image: Rgba) -> StartLamp {
        let face = Dynamic::new(device, (image.width, image.height));
        face.write(queue, lamp(&image, false));
        StartLamp { face, image, lit: false }
    }

    pub fn set(&mut self, queue: &wgpu::Queue, lit: bool) {
        if lit != self.lit {
            self.lit = lit;
            self.face.write(queue, lamp(&self.image, lit));
        }
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
        let (face, glow) = compose(&placeholder, Some(&digits), Some([1, 2, COLON, 3, 4, COLON, 5, 6]));
        let at = |x: u32, y: u32| face[((y * SIZE.0 + x) * 4) as usize];
        let lit = |x: u32, y: u32| glow[((y * SIZE.0 + x) * 4) as usize];
        // Inside the face the 240 texels are 30 per cell; outside it the placeholder.
        let cell = |k: u32| at(8 + 30 * k + 15, 32);
        assert_eq!((0..8).map(cell).collect::<Vec<_>>(), [1, 2, 10, 3, 4, 10, 5, 6]);
        assert_eq!(at(2, 32), 0);
        // The digits glow, the placeholder around them does not.
        assert_eq!((lit(23, 32), lit(2, 32)), (255, 0));
        let (face, glow) = compose(&placeholder, Some(&digits), None);
        assert_eq!((face[(32 * SIZE.0 + 23) as usize * 4], glow[(32 * SIZE.0 + 23) as usize * 4]), (0, 0));
    }

    /// Red through the countdown, green for its first seconds after the go, none after a jump start's go.
    #[test]
    fn the_lamps_follow_the_race_clock() {
        let race = |countdown, running, jump_start| RaceHud { countdown, running, jump_start, ..Default::default() };
        assert_eq!(lamps(None), [false, false]);
        assert_eq!(lamps(Some(&race(7.0, false, false))), [false, true]);
        assert_eq!(lamps(Some(&race(0.01, false, false))), [false, true]);
        assert_eq!(lamps(Some(&race(-0.01, true, false))), [true, false]);
        assert_eq!(lamps(Some(&race(-GREEN_SECONDS - 0.01, true, false))), [false, false]);
        assert_eq!(lamps(Some(&race(-1.0, true, true))), [false, false]);
    }

    /// An unlit lamp is its texture darkened, with no glow; a lit one its texture, glowing.
    #[test]
    fn an_unlit_lamp_is_dark() {
        let face = Rgba { width: 1, height: 1, pixels: vec![255, 100, 0, 255] };
        assert_eq!(lamp(&face, true), (vec![255, 100, 0, 255], vec![255, 255, 255, 255]));
        assert_eq!(lamp(&face, false), (vec![64, 25, 0, 255], vec![0, 0, 0, 255]));
    }
}

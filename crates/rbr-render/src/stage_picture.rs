//! A stage's picture, as the game's quick-race menus show it: the launcher's stage preview and the viewer's loading
//! screen.
//!
//! The game draws the picture with `CScreenshotUI::Vf0C` (0x53f7d0): sprite 0x520 + n of the menu bank
//! (`rbr_formats::bnk`, `CPC_GraphicsDaemon::LoadMenuBank` 0x4c1100) for the map's base map (`E52ModeHost::Mode`
//! 0x4a1560, the II stages share their base's picture). We read the 1280 bank, the largest pictures. A RallySimFans
//! stage shows the first `<id>-*.dds` of its folder (its loading picture) when it is readable. Its files are never
//! decrypted.

use std::collections::HashMap;
use std::path::Path;

use rbr_formats::data::Data;
use rbr_formats::dds::{self, Format};
use rbr_formats::{bnk, stage_files};

/// The menu bank read for the stage pictures.
const BANK: &str = "Generic\\PC_Generic_1280.bnk";
/// The largest picture width decoded from an RSF stage's `.dds` (a smaller mip level is taken).
const RSF_PICTURE_WIDTH: u32 = 512;

/// A picture's width, height and RGBA texels.
pub type Pixels = (usize, usize, Vec<u8>);

/// The base map whose picture and 3D map a map shows: `E52ModeHost::Mode` (0x4a1560).
pub fn base_map(map: u32) -> u32 {
    match map {
        0x0e => 10,
        0x0f => 0x0b,
        0x18 => 0x14,
        0x19 => 0x16,
        0x23 => 0x1f,
        0x24 => 0x22,
        0x2d => 0x2a,
        0x2e => 0x29,
        0x37 => 0x33,
        0x38 => 0x36,
        0x41 => 0x3e,
        0x42 => 0x3f,
        _ => map,
    }
}

/// The menu bank sprite of the map's picture (`CScreenshotUI::Vf0C`, 0x53f7d0), or `None` where the game shows its
/// warning sprite (0x4df).
pub fn picture_sprite(map: u32) -> Option<usize> {
    let base = base_map(map);
    let first =
        |rally: u32, sprite: usize| (rally..rally + 4).contains(&base).then(|| sprite + (base - rally) as usize);
    first(10, 0x520)
        .or_else(|| first(20, 0x524))
        .or_else(|| first(31, 0x528))
        .or_else(|| first(41, 0x52c))
        .or_else(|| first(51, 0x530))
        .or_else(|| first(61, 0x534))
        .or((base == 0x47).then_some(0x538))
}

/// The picture of `stage` (a retail stage name or a community stage's path, as the viewer takes it), if it has one.
pub fn of_stage(data: &Data, stage: &str) -> Option<Pixels> {
    let (map, _) = stage_files::parts(stage)?;
    match stage_files::folder(stage) {
        Some(folder) => rsf_picture(folder, map),
        None => Pictures::new(data.clone()).sprite(picture_sprite(map)?),
    }
}

/// The menu bank's stage pictures, the bank and its decoded pages read once.
pub struct Pictures {
    data: Data,
    bank: Option<Result<bnk::Bank, String>>,
    /// The bank's decoded pages.
    pages: HashMap<u32, Option<Pixels>>,
}

impl Pictures {
    pub fn new(data: Data) -> Pictures {
        Pictures { data, bank: None, pages: HashMap::new() }
    }

    /// The menu bank sprite's pixels.
    pub fn sprite(&mut self, sprite: usize) -> Option<Pixels> {
        let data = &self.data;
        let bank = self.bank.get_or_insert_with(|| {
            let path = data.resolve(BANK).ok_or(format!("{BANK}: not found"))?;
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            bnk::parse(&bytes).map_err(|e| e.to_string())
        });
        let bank = match bank {
            Ok(bank) => bank,
            Err(e) => {
                eprintln!("stage pictures: {e}");
                return None;
            }
        };
        let sprite = *bank.sprites.get(sprite)?;
        let page = self
            .pages
            .entry(sprite.page)
            .or_insert_with(|| {
                let name = bank.pages.get(sprite.page as usize)?;
                let path = data.resolve(&format!("Generic\\{name}"))?;
                let bytes = std::fs::read(&path).ok()?;
                decode(&dds::parse(&bytes).ok()?, u32::MAX)
            })
            .as_ref()?;
        Some(crop(page, sprite.rect()))
    }
}

/// The first `<id>-*.dds` of an RSF stage folder, decoded, unless it is encrypted or does not read.
pub fn rsf_picture(folder: &Path, id: u32) -> Option<Pixels> {
    let prefix = format!("{id}-");
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .ok()?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(&prefix) && n.to_ascii_lowercase().ends_with(".dds"))
        .collect();
    names.sort();
    names.iter().find_map(|name| {
        let bytes = std::fs::read(folder.join(name)).ok()?;
        if stage_files::encrypted(&bytes) {
            return None;
        }
        decode(&dds::parse(&bytes).ok()?, RSF_PICTURE_WIDTH)
    })
}

/// The first mip level at most `widest` texels wide, decoded to RGBA.
pub fn decode(dds: &dds::Dds, widest: u32) -> Option<Pixels> {
    let levels = dds.levels();
    let level = levels.iter().find(|l| l.width <= widest).or(levels.last())?;
    let (width, height) = (level.width as usize, level.height as usize);
    let bc = match dds.format {
        Format::Dxt1 => texpresso::Format::Bc1,
        Format::Dxt3 => texpresso::Format::Bc2,
        Format::Dxt5 => texpresso::Format::Bc3,
        Format::Rgba8 => return Some((width, height, level.data.get(..width * height * 4)?.to_vec())),
    };
    let mut rgba = vec![0u8; width * height * 4];
    bc.decompress(level.data, width, height, &mut rgba);
    Some((width, height, rgba))
}

/// The part of the page inside `rect` (left, top, right, bottom in 0..1).
fn crop((width, height, rgba): &Pixels, rect: [f32; 4]) -> Pixels {
    let px = |f: f32, size: usize| ((f * size as f32).round() as usize).min(size);
    let (left, top, right, bottom) =
        (px(rect[0], *width), px(rect[1], *height), px(rect[2], *width), px(rect[3], *height));
    let (w, h) = (right.saturating_sub(left), bottom.saturating_sub(top));
    let pixels =
        (top..top + h).flat_map(|y| &rgba[4 * (y * width + left)..4 * (y * width + left + w)]).copied().collect();
    (w, h, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ii_stages_show_their_base_maps_picture() {
        assert_eq!(picture_sprite(10), Some(0x520));
        assert_eq!(picture_sprite(14), Some(0x520));
        assert_eq!(picture_sprite(54), Some(0x533));
        assert_eq!(picture_sprite(56), Some(0x533));
        assert_eq!(picture_sprite(64), Some(0x537));
        assert_eq!(picture_sprite(71), Some(0x538));
        assert_eq!(picture_sprite(5), None);
    }

    /// The retail stages' pictures read from the menu bank (`RBR_DATA`).
    #[test]
    fn a_retail_stage_has_its_picture() {
        let Some(data) = Data::from_env() else {
            eprintln!("skipping: RBR_DATA not set");
            return;
        };
        let (width, height, rgba) = of_stage(&data, "track-54_E").expect("Tanner's picture");
        assert!(width > 0 && height > 0 && rgba.len() == width * height * 4);
        assert!(of_stage(&data, "track-5_O").is_none());
    }
}

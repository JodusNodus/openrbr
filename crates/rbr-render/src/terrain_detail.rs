//! Terrain detail (ours, renderer choice; `terrain_detail.wgsl`): each stage texture's most detailed window, which the
//! near ground tiles as its own fine structure. An RSF stage's ground atlases (Luceram's 4096 `z*`) hold leaf litter
//! and stones next to flat padding and road strips; tiled whole, the flat parts left the detail patchy.

use rbr_formats::dds::{self, Format};

/// The window's side in the texture's own texels: an eighth of the texture, at least 256 (or all of it).
fn window_texels(width: u32) -> u32 {
    (width / 8).max(256).min(width)
}

/// The uv rectangle (low u and v, width and height) of `dds`'s window whose least detailed quarter is the most
/// detailed: the fine structure (luma against its 3x3 mean) summed over a quarter, read at a level no wider than 512.
pub(crate) fn window(dds: &dds::Dds) -> Option<[f32; 4]> {
    let levels = dds.levels();
    let level = levels.iter().find(|l| l.width <= 512 && l.height <= 512)?;
    let (w, h) = (level.width as usize, level.height as usize);
    if w < 16 || h < 16 {
        return None;
    }
    let mut rgba = vec![0u8; w * h * 4];
    match dds.format {
        Format::Dxt1 => texpresso::Format::Bc1.decompress(level.data, w, h, &mut rgba),
        Format::Dxt3 => texpresso::Format::Bc2.decompress(level.data, w, h, &mut rgba),
        Format::Dxt5 => texpresso::Format::Bc3.decompress(level.data, w, h, &mut rgba),
        Format::Rgba8 => rgba.copy_from_slice(level.data.get(..w * h * 4)?),
    }
    let luma: Vec<f32> = rgba
        .chunks_exact(4)
        .map(|t| 0.2126 * f32::from(t[0]) + 0.7152 * f32::from(t[1]) + 0.0722 * f32::from(t[2]))
        .collect();
    let at = |x: usize, y: usize| luma[y.min(h - 1) * w + x.min(w - 1)];
    // The window and its quarters at this level.
    let side = ((window_texels(dds.width) * level.width / dds.width) as usize).clamp(8, w.min(h));
    let cell = side / 2;
    let (cw, ch) = (w / cell, h / cell);
    // Each cell's fine structure, then the windows of 2x2 cells.
    let mut energy = vec![0f32; cw * ch];
    for y in 0..ch * cell {
        for x in 0..cw * cell {
            let mut mean = 0.0;
            for (dx, dy) in [(0, 0), (1, 0), (2, 0), (0, 1), (2, 1), (0, 2), (1, 2), (2, 2), (1, 1)] {
                mean += at((x + dx).saturating_sub(1), (y + dy).saturating_sub(1));
            }
            energy[(y / cell) * cw + x / cell] += (at(x, y) - mean / 9.0).abs();
        }
    }
    let best = (0..ch.saturating_sub(1))
        .flat_map(|y| (0..cw.saturating_sub(1)).map(move |x| (x, y)))
        .map(|(x, y)| {
            let least = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .map(|(dx, dy)| energy[(y + dy) * cw + x + dx])
                .into_iter()
                .fold(f32::INFINITY, f32::min);
            (least, x, y)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    let (_, x, y) = best;
    Some([(x * cell) as f32 / w as f32, (y * cell) as f32 / h as f32, side as f32 / w as f32, side as f32 / h as f32])
}

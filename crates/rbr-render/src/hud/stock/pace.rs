//! The pace-note symbols as the game draws them: its `GEN_BOX_PACE_VERSION1_*` sprites from the menu bank
//! (`hud::pace::sprite`, the 1280 bank's), mirrored for the left corners and tinted by the symbol's colour. A custom
//! co-driver's text calls, and every symbol when the bank does not load, are drawn as their words.

use std::collections::HashMap;

use egui::{Color32, FontId, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use rbr_formats::data::Data;

use crate::hud::pace::{self, Kind, Symbol};
use crate::stage_picture::{Pictures, Pixels};

/// The symbol's box size, points.
pub const SIZE: Vec2 = Vec2::splat(72.0);
/// The tallest pace sprite's height in the 1280 bank (the flat's), texels: it fills the box's height, and the others
/// keep their size against it, as in the game.
const SPRITE_HEIGHT: f32 = 92.0;

/// The game's pace sprites: decoded from the menu bank, then egui's textures from the first [`Sprites::upload`].
#[derive(Default)]
pub struct Sprites {
    pixels: Vec<(usize, Pixels)>,
    textures: HashMap<usize, (TextureHandle, Vec2)>,
}

impl Sprites {
    /// The sprites of `data`'s menu bank (none, reported, when it does not load).
    pub fn load(data: &Data) -> Sprites {
        let mut pictures = Pictures::new(data.clone());
        let pixels: Vec<_> = pace::SPRITES.filter_map(|s| Some((s, pictures.sprite(s)?))).collect();
        if pixels.is_empty() {
            eprintln!("hud: no pace sprites in the menu bank: the notes are drawn as words");
        }
        Sprites { pixels, textures: HashMap::new() }
    }

    /// Makes the decoded sprites egui's textures, once.
    pub fn upload(&mut self, ctx: &egui::Context) {
        for (sprite, (width, height, rgba)) in self.pixels.drain(..) {
            let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba);
            let texture = ctx.load_texture(format!("pace sprite {sprite:#x}"), image, TextureOptions::LINEAR);
            self.textures.insert(sprite, (texture, Vec2::new(width as f32, height as f32)));
        }
    }
}

fn colour(rgb: [f32; 3], alpha: f32) -> Color32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgba_unmultiplied(c(rgb[0]), c(rgb[1]), c(rgb[2]), c(alpha))
}

/// Draws `symbol` in `rect` (square) at `alpha`: its sprite when `sprites` has it, else its words.
pub fn paint(painter: &Painter, rect: Rect, symbol: &Symbol, alpha: f32, sprites: &Sprites) {
    let tint = colour(symbol.colour, alpha);
    if let Some((texture, size)) = pace::sprite(symbol.kind).and_then(|s| sprites.textures.get(&s)) {
        let at = Rect::from_center_size(rect.center(), *size * (rect.height() / SPRITE_HEIGHT));
        let (u0, u1) = if symbol.left { (1.0, 0.0) } else { (0.0, 1.0) };
        let uv = Rect::from_min_max(Pos2::new(u0, 0.0), Pos2::new(u1, 1.0));
        painter.image(texture.id(), at, uv, tint);
        return;
    }
    let words = match symbol.kind {
        Kind::Text(word) => word.to_owned(),
        _ => pace::label(symbol).to_uppercase(),
    };
    // The words wrapped to the box, centred.
    let mut job = egui::text::LayoutJob::simple(words, FontId::proportional(rect.width() * 0.2), tint, rect.width());
    job.halign = egui::Align::Center;
    let galley = painter.layout_job(job);
    painter.galley(rect.center() - Vec2::new(0.0, galley.size().y / 2.0), galley, tint);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every symbol paints without panicking on a headless context, as a sprite (a stand-in texture) or as words.
    #[test]
    fn every_symbol_paints() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
            ..Default::default()
        };
        let mut sprites = Sprites { pixels: vec![(0x503, (2, 3, vec![255; 24]))], ..Default::default() };
        let mut fullscreen = ctx.run_pass(raw, |ctx| {
            sprites.upload(ctx);
            let painter = ctx.layer_painter(egui::LayerId::background());
            let symbols = [
                Kind::Corner(1),
                Kind::Corner(6),
                Kind::Twisty,
                Kind::Water,
                Kind::Text("CUT"),
                Kind::Text("A VERY LONG CUSTOM CALL NAME"),
            ];
            for (k, kind) in symbols.into_iter().enumerate() {
                let symbol = Symbol { kind, left: k % 2 == 0, colour: [1.0, 1.0, 1.0], tag: k as u32 };
                paint(&painter, Rect::from_center_size(Pos2::new(100.0, 100.0), SIZE), &symbol, 1.0, &sprites);
            }
        });
        assert!(sprites.textures.contains_key(&0x503));
        let _ = ctx.tessellate(fullscreen.shapes, fullscreen.pixels_per_point);
        fullscreen.textures_delta.clear();
    }
}

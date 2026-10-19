//! Reader for the menu graphics bank `Generic/PC_Generic_<width>.bnk`, one per menu resolution (640, 800, 1024,
//! 1280), as `CPC_GraphicsDaemon::LoadMenuBank` (0x4c1100) reads it:
//!
//! ```text
//! u32 page_count, u32 sprite_count
//! page_count x char[64]   texture page file name in `Generic\` (NUL-terminated, 0xff padded)
//! sprite_count x { u32 page; f32 u[4]; f32 v[4] }   corners top left, top right, bottom left, bottom right
//! ```
//!
//! Every retail bank has 0x54d sprites (the loader stops past 0x54c: "MenuGfx bank is incosistent with the information
//! in gendef.h"); a sprite's number is the same in each bank. The pages are DXT `.dds` textures.

use std::fmt;

/// One sprite: its page and its texture rectangle, in 0..1 of the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub page: u32,
    pub u: [f32; 4],
    pub v: [f32; 4],
}

impl Sprite {
    /// The rectangle's left, top, right and bottom, in 0..1 of the page.
    pub fn rect(&self) -> [f32; 4] {
        [self.u[0], self.v[0], self.u[3], self.v[3]]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bank {
    /// The page textures' file names, relative to `Generic\`.
    pub pages: Vec<String>,
    pub sprites: Vec<Sprite>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "menu bank: {}", self.0)
    }
}

impl std::error::Error for Error {}

const NAME_LEN: usize = 64;
const SPRITE_LEN: usize = 36;

pub fn parse(bytes: &[u8]) -> Result<Bank, Error> {
    let u32_at = |at: usize| -> Result<u32, Error> {
        let word = bytes.get(at..at + 4).ok_or_else(|| Error(format!("ends at {at}")))?;
        Ok(u32::from_le_bytes(word.try_into().expect("four bytes")))
    };
    let (page_count, sprite_count) = (u32_at(0)? as usize, u32_at(4)? as usize);
    let sprites_at = 8 + page_count * NAME_LEN;
    if bytes.len() != sprites_at + sprite_count * SPRITE_LEN {
        return Err(Error(format!("{} bytes for {page_count} pages and {sprite_count} sprites", bytes.len())));
    }
    let pages = bytes[8..sprites_at]
        .chunks_exact(NAME_LEN)
        .map(|name| {
            let end = name.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
            String::from_utf8_lossy(&name[..end]).into_owned()
        })
        .collect();
    let sprites = (0..sprite_count)
        .map(|k| {
            let at = sprites_at + k * SPRITE_LEN;
            let float = |n: usize| f32::from_bits(u32_at(at + 4 + 4 * n).expect("in bounds"));
            Ok(Sprite {
                page: u32_at(at)?,
                u: [float(0), float(1), float(2), float(3)],
                v: [float(4), float(5), float(6), float(7)],
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(Bank { pages, sprites })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Data;

    #[test]
    fn a_bank_reads_its_pages_and_sprites() {
        let mut bytes = vec![1, 0, 0, 0, 1, 0, 0, 0];
        let mut name = b"PC_1280_01.dds\0".to_vec();
        name.resize(NAME_LEN, 0xff);
        bytes.extend(name);
        bytes.extend(0u32.to_le_bytes());
        for value in [0.25f32, 0.75, 0.25, 0.75, 0.5, 0.5, 1.0, 1.0] {
            bytes.extend(value.to_le_bytes());
        }
        let bank = parse(&bytes).unwrap();
        assert_eq!(bank.pages, ["PC_1280_01.dds"]);
        assert_eq!(bank.sprites[0].rect(), [0.25, 0.5, 0.75, 1.0]);
        assert!(parse(&bytes[..bytes.len() - 1]).is_err());
    }

    /// Every retail bank has the 0x54d sprites, each on one of its pages, and the stage pictures (0x520-0x538).
    #[test]
    fn retail_banks_parse() {
        let Some(data) = Data::from_env() else { return };
        for width in [640, 800, 1024, 1280] {
            let path = data.resolve(&format!("Generic\\PC_Generic_{width}.bnk")).expect("the bank");
            let bank = parse(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(bank.sprites.len(), 0x54d, "{width}");
            assert!(bank.sprites.iter().all(|s| (s.page as usize) < bank.pages.len()), "{width}");
            for page in &bank.pages {
                assert!(data.resolve(&format!("Generic\\{page}")).is_some(), "{width}: {page}");
            }
            let [l, t, r, b] = bank.sprites[0x520].rect();
            assert!(r > l && b > t, "{width}");
        }
    }
}

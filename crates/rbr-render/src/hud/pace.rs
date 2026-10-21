//! The pace-note symbols. The meaning is the game's: `E62GameMenu::M5721c0` (0x5721c0) maps a key's tag to one of the
//! `GEN_BOX_PACE_VERSION1_*` sprites (`Gendef.h`), mirrored for the left corners, and to a colour (halved when drawn,
//! 0x574090). The stock HUD draws them (`stock::pace`).

use rbr_gameplay::markers::flag_word_id;

/// What a symbol shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `R1` (easy) to `R6` (hairpin).
    Corner(u8),
    Twisty,
    Narrow,
    /// Over crest, bump and jump share the crest sprite.
    Crest,
    /// The ford's sprite is the waves.
    Water,
    /// Caution (`VERSION2_WARNING`).
    Caution,
    Flat,
    Bridge,
    Straight,
}

/// A symbol: what, which way, and the game's colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Symbol {
    pub kind: Kind,
    pub left: bool,
    /// The sprite's colour as drawn (the table's value times 0.5).
    pub colour: [f32; 3],
    pub tag: u32,
}

/// The tag's symbol, `None` for the tags the HUD skips (`E12_AllowTag` 0x474b70: distance, wide out, start, finish,
/// split, end and the keep calls have none).
pub fn symbol(tag: u32) -> Option<Symbol> {
    let (kind, left) = match tag {
        0..=5 => (Kind::Corner(6 - tag as u8), true),
        6..=11 => (Kind::Corner(tag as u8 - 5), false),
        12 => (Kind::Twisty, false),
        14 => (Kind::Narrow, false),
        16 | 19 | 20 => (Kind::Crest, false),
        17 => (Kind::Water, false),
        18 | 32 => (Kind::Caution, false),
        25 => (Kind::Flat, false),
        26 => (Kind::Flat, true),
        27 => (Kind::Bridge, false),
        28 => (Kind::Straight, false),
        _ => return None,
    };
    let rgb = match tag {
        1 | 10 => [1.9, 0.4, 0.4],
        2 | 9 | 18 => [2.0, 0.9, 0.5],
        3 | 8 => [2.0, 1.4, 0.7],
        4 | 7 => [1.9, 1.7, 1.0],
        5 | 6 => [1.7, 1.8, 1.4],
        12 => [1.3, 1.6, 1.6],
        14 | 16 => [1.4, 1.4, 1.4],
        17 | 27 => [1.3, 1.7, 2.0],
        19 | 20 => [1.7, 1.2, 1.6],
        25 | 26 => [1.9, 1.9, 1.7],
        28 => [1.4, 1.4, 1.4],
        _ => [2.0, 0.0, 0.0],
    };
    Some(Symbol { kind, left, colour: rgb.map(|c| (c * 0.5f32).min(1.0)), tag })
}

/// The symbol's label, as the co-driver calls it (rbr-recomp `notes/sound.md` 10.1).
pub fn label(symbol: &Symbol) -> &'static str {
    match symbol.tag {
        0 | 11 => "hairpin",
        1 | 10 => "90",
        2 | 9 => "K",
        3 | 8 => "medium",
        4 | 7 => "fast",
        5 | 6 => "easy",
        12 => "twisty",
        14 => "narrows",
        16 => "over crest",
        17 => "ford",
        18 | 32 => "caution",
        19 => "bump",
        20 => "jump",
        25 | 26 => "flat",
        27 => "bridge",
        _ => "straight",
    }
}

/// The word the co-driver adds for the key's flags (`E25_BitId`, 0x4b5540), if any.
pub fn modifier(flags: u32) -> Option<&'static str> {
    Some(match flag_word_id(flags) {
        0x15 => "cut",
        0x16 => "don't cut",
        0x23 => "long",
        0x24 => "maybe",
        0x27 => "narrows",
        0x29 => "tightens bad",
        0x2a => "tightens",
        0x2d => "wide out",
        0x2e => "plus",
        0x2f => "minus",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_run_from_easy_to_hairpin_and_left_ones_are_mirrored() {
        assert_eq!(symbol(0).map(|s| (s.kind, s.left)), Some((Kind::Corner(6), true)));
        assert_eq!(symbol(5).map(|s| (s.kind, s.left)), Some((Kind::Corner(1), true)));
        assert_eq!(symbol(6).map(|s| (s.kind, s.left)), Some((Kind::Corner(1), false)));
        assert_eq!(symbol(11).map(|s| (s.kind, s.left)), Some((Kind::Corner(6), false)));
        assert_eq!(symbol(26).map(|s| (s.kind, s.left)), Some((Kind::Flat, true)));
    }

    #[test]
    fn the_skipped_tags_have_no_symbol() {
        for tag in [13, 15, 21, 22, 23, 24, 29, 30, 31] {
            assert_eq!(symbol(tag), None, "tag {tag}");
        }
        assert_eq!(symbol(17).unwrap().kind, Kind::Water);
        assert_eq!(symbol(32).unwrap().kind, Kind::Caution);
    }

    #[test]
    fn colours_are_the_tables_halved() {
        assert_eq!(symbol(0).unwrap().colour, [1.0, 0.0, 0.0]);
        assert_eq!(symbol(5).unwrap().colour, [0.85, 0.9, 0.7]);
        assert_eq!(modifier(4), Some("tightens"));
        assert_eq!(modifier(0), None);
    }
}

//! The pace-note symbols. The meaning is the game's: `E62GameMenu::M5721c0` (0x5721c0) maps a key's tag to one of the
//! `GEN_BOX_PACE_VERSION1_*` sprites (`Gendef.h`), mirrored for the left corners, and to a colour (halved when drawn,
//! 0x574090). The stock HUD draws them (`stock::pace`).

use rbr_formats::rsf::pacenotes::{id, Note};
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
    /// A plugin-only call of a custom co-driver (`custom_symbol`), drawn as its short label.
    Text(&'static str),
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

/// The menu bank's pace sprites (`rbr_formats::bnk`, `GEN_BOX_PACE_VERSION1_*`).
pub const SPRITES: std::ops::RangeInclusive<usize> = 0x4fe..=0x50c;

/// The kind's sprite in the menu bank: `M5721c0`'s second table (0x5723e9), mirrored for the left corners by the
/// caller; `None` for a custom co-driver's text calls.
pub fn sprite(kind: Kind) -> Option<usize> {
    Some(match kind {
        Kind::Corner(grade) => 0x502 + usize::from(grade),
        Kind::Bridge => 0x4fe,
        Kind::Crest => 0x500,
        Kind::Flat => 0x501,
        Kind::Narrow => 0x502,
        Kind::Straight => 0x509,
        Kind::Twisty => 0x50a,
        Kind::Caution => 0x50b,
        Kind::Water => 0x50c,
        Kind::Text(_) => return None,
    })
}

/// The symbol of a custom co-driver's note: a short label for the plugin-only calls (`CUT`, `DON'T CUT`, ...), the
/// water for `CAUTION_WATER`, else the game's symbol of its 1.02 stand-in (`type` bits 0-15: the id itself for a game
/// id); `None` for the calls with nothing to show.
pub fn custom_symbol(note: &Note) -> Option<Symbol> {
    let text = |word| Some(Symbol { kind: Kind::Text(word), left: false, colour: [1.0, 1.0, 1.0], tag: note.id() });
    match note.id() {
        id::CAUTION_WATER => symbol(17),
        id::CUT => text("CUT"),
        id::DONTCUT => text("DON'T CUT"),
        id::LONG => text("LONG"),
        id::TIGHTENS => text("TIGHTENS"),
        id::TIGHTENSBAD => text("TIGHTENS BAD"),
        id::MAYBE => text("MAYBE"),
        id::AND => text("AND"),
        id::INTO => text("INTO"),
        id::ONTO => text("ONTO"),
        _ => symbol(note.kind & 0xffff),
    }
}

/// The symbol's label, as the co-driver calls it (rbr-recomp `notes/sound.md` 10.1).
pub fn label(symbol: &Symbol) -> &'static str {
    if let Kind::Text(word) = symbol.kind {
        return word;
    }
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

    /// Each tag's sprite and mirroring are `M5721c0`'s (0x5721c0, its second jump table: tags 0-5 the corners
    /// 0x508 down to 0x503 mirrored, 6-11 the same unmirrored, 26 the flat mirrored).
    #[test]
    fn the_tags_take_the_games_sprites() {
        let game = [
            (0, 0x508, true),
            (1, 0x507, true),
            (2, 0x506, true),
            (3, 0x505, true),
            (4, 0x504, true),
            (5, 0x503, true),
            (6, 0x503, false),
            (7, 0x504, false),
            (8, 0x505, false),
            (9, 0x506, false),
            (10, 0x507, false),
            (11, 0x508, false),
            (12, 0x50a, false),
            (14, 0x502, false),
            (16, 0x500, false),
            (17, 0x50c, false),
            (18, 0x50b, false),
            (19, 0x500, false),
            (20, 0x500, false),
            (25, 0x501, false),
            (26, 0x501, true),
            (27, 0x4fe, false),
            (28, 0x509, false),
            (32, 0x50b, false),
        ];
        for (tag, sprite_id, left) in game {
            let symbol = symbol(tag).unwrap();
            assert_eq!((sprite(symbol.kind), symbol.left), (Some(sprite_id), left), "tag {tag}");
            assert!(SPRITES.contains(&sprite_id));
        }
    }

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

    #[test]
    fn custom_calls_map_through_their_stand_in_or_a_label() {
        let note = |kind, flags| Note { kind, flags, distance: 0.0 };
        // A game id maps as the game maps it, an extended id (here OVERCREST_NOLINK, 2269) through its stand-in.
        assert_eq!(custom_symbol(&note(3, 0)).map(|s| (s.kind, s.left)), Some((Kind::Corner(3), true)));
        assert_eq!(custom_symbol(&note(16, 2269 << 19 | 0x4000)), symbol(16));
        // The plugin-only calls get their label; the water its symbol; the rest nothing.
        assert_eq!(custom_symbol(&note(id::CAUTION_WATER, 0)), symbol(17));
        for (call, word) in [(id::CUT, "CUT"), (id::DONTCUT, "DON'T CUT"), (id::TIGHTENSBAD, "TIGHTENS BAD")] {
            assert_eq!(label(&custom_symbol(&note(call, 0)).unwrap()), word);
        }
        for call in [id::EMPTY_CALL, id::SOUND_FILE, id::PLACEHOLDER] {
            assert_eq!(custom_symbol(&note(call, 0)), None);
        }
        // The plugin's modifier bits read through the game's words.
        assert_eq!(modifier(0x40), Some("cut"));
        assert_eq!(modifier(0x20), Some("don't cut"));
        assert_eq!(modifier(0x400), Some("long"));
        assert_eq!(modifier(0x2000), Some("maybe"));
    }
}

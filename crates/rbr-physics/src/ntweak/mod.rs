//! The NTweak apply walk: one `.lsp` group applied to a tweak object, as `NTweak::CBase::Parse` (0x52f0c0) does it
//! (`src/rbr/RBR_E48_NTweak.cpp`). The group comes as the reader's items (`rbr_formats::lsp`); the reader sees each
//! retail file as the game's tokenizer does (`rbr-formats/tests/lsp_tokens.rs`).
//!
//! For each item, in order:
//! - a leading quoted string names the object: a copy goes to the token heap and `+0x04` points at it;
//! - a number (a signed digit string, or what `IsFloat` takes) is skipped;
//! - an atom that is a key of the object's table (`lsp_keys`, the class's own keys, then its parents') hands the items
//!   after it to that member's own parse (its vtable slot 2): a `CReal` takes one float (at `+0x0c`), a `CUnsigned`
//!   or `CInteger` one integer (`+0x0c`, both `atoi`), a `CVector` up to three floats (`+0x10`), a `CVector4` up to
//!   four, a bit-field the `+name`/`-name` items after it (its mask at `+0x0c`, [`set_bit`]), and a member object
//!   (slot 2 is `CBase::Parse`, such as a wheel's `SpringDamper`) its group, walked the same way. A member that refuses
//!   the item leaves it for the walk. A key with nothing after it ends the walk. The material manager's keys name its
//!   materials by tag;
//! - a group (not a member's) goes to the object's slot 5 (`Unk14`) once per item after its leading quoted name, each
//!   item's quoted name ([`get_quoted`]) first copied to a token-heap block: an entity definition's appends the
//!   registry index of the group's name to its links (`+0x18` count, `+0x1c` indices), such as `prime.lsp`'s
//!   `("CAR_WHEEL_LF" ("CAR_WHEEL_LF"))` in the car's group; the definition manager's (`CDefManager::Vf14`, 0x501ef0)
//!   takes the group's name as a factory and each item as a definition, which the host makes or finds and walks the
//!   item on; `CBase`'s does nothing;
//! - anything else is skipped.
//!
//! Then the object's slot 17 (`Unk44`) runs. Values are stored as the original's `atof`/`atoi` give them, the float
//! rounded once from the double. The definition manager's dictionary is the registry itself (`Unk20`, 0x501ed0): an
//! atom of its group that names a definition would hand the items after it to that definition. No retail file has one
//! (their atoms there are `NULL_PPTOKEN`), and the walk skips such atoms.

use rbr_formats::lsp;

pub mod layout;
pub mod plan;
pub mod store;
pub mod tokens;

pub use layout::{
    apply, construct_base, construct_integer, construct_real, construct_unsigned, construct_vector, construct_vector4,
    name_or_null, parse, plan, set_dirty, stack_alloc, tokenize, ApplyHost, Frame, ParseHost, Plan, EMPTY_TOKENS,
    TOKEN_HEAP,
};

/// A group the walk does not cover; nothing is written when it says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

/// `CTokenCursor::IsFloat` (0x52eed0): an optional sign, then digits with at most one dot.
pub fn is_float(t: &str) -> bool {
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    digits.bytes().all(|c| c.is_ascii_digit() || c == b'.') && digits.bytes().filter(|&c| c == b'.').count() <= 1
}

/// `CTokenCursor::IsInteger` (0x52ef40): an optional sign, then digits.
pub fn is_integer(t: &str) -> bool {
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    digits.bytes().all(|c| c.is_ascii_digit())
}

/// `atof` on what `IsFloat` takes, rounded to the float the member stores.
pub fn atof(t: &str) -> f32 {
    t.parse::<f64>().unwrap_or(0.0) as f32
}

/// `CTokenCursor::GetQuoted(buf, len)` (0x52f000) on an item: the quoted string it starts with, inside its opening
/// parentheses, cut to `len - 1` bytes; empty for anything else.
pub fn get_quoted(item: &lsp::Node, len: u32) -> String {
    match item {
        lsp::Node::Quoted(s) => s.chars().take(len as usize - 1).collect(),
        lsp::Node::List(items) => items.first().map_or(String::new(), |i| get_quoted(i, len)),
        lsp::Node::Atom(_) => String::new(),
    }
}

/// `CBitField::Parse` (0x52f930) on one item, `+name` or `-name`: the bit of the name `names` has (compared ignoring
/// case) set or cleared in `mask`. An unknown name leaves the mask; the item is taken all the same.
pub fn set_bit(mask: u32, names: &[String], item: &str) -> u32 {
    let set = item.starts_with('+');
    match names.iter().position(|n| n.eq_ignore_ascii_case(&item[1..])) {
        Some(bit) if set => mask | 1 << bit,
        Some(bit) => mask & !(1 << bit),
        None => mask,
    }
}

/// `atoi` on what `IsInteger` takes.
pub fn atoi(t: &str) -> u32 {
    t.parse::<i64>().unwrap_or(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifiers_follow_the_originals() {
        assert!(is_float("+0.33") && is_float("-2.47") && is_float("5.") && is_float("+"));
        assert!(!is_float("1.2.3") && !is_float("1e5") && !is_float("\"x"));
        assert!(is_integer("+2") && !is_integer("2.0"));
    }

    #[test]
    fn bits_are_named_ignoring_case() {
        let names: Vec<String> = ["HARD", "VALID"].map(String::from).to_vec();
        assert_eq!(set_bit(0, &names, "+valid"), 2);
        assert_eq!(set_bit(3, &names, "-HARD"), 2);
        assert_eq!(set_bit(1, &names, "+SOFT"), 1);
    }

    #[test]
    fn get_quoted_takes_the_leading_name() {
        let file = lsp::parse("((\"GROUND\" Width 1) x \"0123456789abcdef0123456789abcdefXX\")").unwrap();
        let lsp::Node::List(items) = &file[0] else { unreachable!() };
        assert_eq!(get_quoted(&items[0], 0x20), "GROUND");
        assert_eq!(get_quoted(&items[1], 0x20), "");
        assert_eq!(get_quoted(&items[2], 0x20).len(), 31);
    }
}

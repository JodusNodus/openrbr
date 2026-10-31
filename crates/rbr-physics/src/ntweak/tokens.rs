//! The tokenizer on a text's bytes, and the token heap's stack allocator on its words. `layout` writes them
//! into game memory.

use super::Unsupported;

/// The token table's size: its words, the terminator's included.
pub const TABLE_WORDS: usize = 0x1000;

/// ASCII whitespace in the C locale: space and 0x09-0x0D.
fn is_space(c: u8) -> bool {
    c == b' ' || (0x09..=0x0d).contains(&c)
}

/// The cut the tokenizer makes of a text: the offsets the tokens start at, in order, and the offsets of the bytes it sets
/// to NUL.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tokens {
    pub starts: Vec<u32>,
    pub cuts: Vec<u32>,
}

/// Where the tokenizer is between two bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Between,
    Atom,
    Quoted,
    /// A group at this depth; `quoted` while inside a string in it.
    Group {
        depth: u32,
        quoted: bool,
    },
}

/// The tokenizer on `text` (up to its NUL, not empty): leading whitespace, one `(` and whitespace again are skipped, then
/// each token starts at its first byte. An atom runs up to whitespace, a quoted string from its `"` up to the next `"`,
/// a group from its `(` up to the `)` that closes it (the parentheses inside strings not counted); the byte that ends
/// a token becomes NUL. A `)` between tokens ends the cut; the end of the text ends it in any state. A byte past ASCII
/// and a table past its 0x1000 words are [`Unsupported`].
pub fn tokenize(text: &[u8]) -> Result<Tokens, Unsupported> {
    if text.iter().any(|&c| c >= 0x80) {
        return Err(Unsupported("a byte past ASCII in the text"));
    }
    let skip = |i: usize| i + text[i..].iter().take_while(|&&c| is_space(c)).count();
    let mut i = skip(0);
    if text.get(i) == Some(&b'(') {
        i = skip(i + 1);
    }
    let mut out = Tokens::default();
    let mut state = State::Between;
    for (at, &c) in text.iter().enumerate().skip(i) {
        let at = at as u32;
        state = match state {
            State::Between => {
                let next = match c {
                    b')' => break,
                    b'"' => State::Quoted,
                    b'(' => State::Group { depth: 1, quoted: false },
                    _ if is_space(c) => continue,
                    _ => State::Atom,
                };
                out.starts.push(at);
                next
            }
            State::Group { depth, quoted } => {
                let quoted = quoted != (c == b'"');
                let depth = match c {
                    b'(' if !quoted => depth + 1,
                    b')' if !quoted => depth - 1,
                    _ => depth,
                };
                if depth == 0 {
                    out.cuts.push(at);
                    State::Between
                } else {
                    State::Group { depth, quoted }
                }
            }
            State::Quoted if c == b'"' => {
                out.cuts.push(at);
                State::Between
            }
            State::Atom if is_space(c) => {
                out.cuts.push(at);
                State::Between
            }
            other => other,
        };
    }
    if out.starts.len() >= TABLE_WORDS {
        return Err(Unsupported("a token table past its 0x1000 words (the original traps at int3)"));
    }
    Ok(out)
}

/// Whether a token is a group: it holds whitespace and is not a quoted string.
pub fn is_group(t: &[u8]) -> bool {
    t.first().is_some_and(|&c| c != b'"') && t.iter().any(|&c| is_space(c))
}

/// The quoted name of a token: after whitespace and `(`, the quoted string's bytes up to its closing `"`, cut to
/// `len - 1`; empty when no `"` comes first. The buffer gets them and a NUL.
pub fn quoted(t: &[u8], len: u32) -> &[u8] {
    let start = t.iter().position(|&c| !is_space(c) && c != b'(').unwrap_or(t.len());
    let Some(rest) = t[start..].strip_prefix(b"\"") else {
        return &[];
    };
    let end = rest.iter().position(|&c| c == b'"').unwrap_or(rest.len());
    &rest[..end.min(len as usize - 1)]
}

/// The stack allocator on the heap's `used` and `cap` words: the block's offset from the heap's base
/// and the new `used`, the offset aligned up to `align` (a power of two). A block that does not end below `cap` is
/// [`Unsupported`].
pub fn stack_alloc(used: u32, cap: u32, size: u32, align: u32) -> Result<(u32, u32), Unsupported> {
    let off = used.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1);
    let end = off.wrapping_add(size);
    if end >= cap {
        return Err(Unsupported("the stack heap is full (the original logs, then traps at int3)"));
    }
    Ok((off, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tokens as the table reads them after the cut.
    fn cut(text: &str) -> Vec<String> {
        let tokens = tokenize(text.as_bytes()).unwrap();
        let mut bytes = text.as_bytes().to_vec();
        for &at in &tokens.cuts {
            bytes[at as usize] = 0;
        }
        let end = |at: usize| at + bytes[at..].iter().position(|&c| c == 0).unwrap_or(bytes.len() - at);
        tokens.starts.iter().map(|&at| String::from_utf8_lossy(&bytes[at as usize..end(at as usize)]).into()).collect()
    }

    #[test]
    fn tokens_follow_the_originals_cut() {
        assert_eq!(
            cut("  ( \"Name\" Key 1.5\t(\"G\" (a \")\" b)) x"),
            ["\"Name", "Key", "1.5", "(\"G\" (a \")\" b)", "x"]
        );
        // A `)` between tokens ends the cut; one inside an atom is part of it.
        assert_eq!(cut("(a b ) c"), ["a", "b"]);
        assert_eq!(cut("(a b) c"), ["a", "b)", "c"]);
        // Only one leading `(` is skipped; the next opens a group the end of the text leaves open.
        assert_eq!(cut("((a) (b"), ["(a", "(b"]);
        assert_eq!(cut("\"unterminated"), ["\"unterminated"]);
    }

    #[test]
    fn past_ascii_and_a_full_table_are_unsupported() {
        assert!(tokenize(b"(a \x80)").is_err());
        assert!(tokenize("a ".repeat(TABLE_WORDS - 1).as_bytes()).is_ok());
        assert!(tokenize("a ".repeat(TABLE_WORDS).as_bytes()).is_err());
    }

    #[test]
    fn groups_and_quoted_names() {
        assert!(is_group(b"(\"G\" a") && !is_group(b"(\"G\"") && !is_group(b"\"a b") && !is_group(b""));
        assert_eq!(quoted(b"(\"CAR_WHEEL_LF\"", 0x20), b"CAR_WHEEL_LF");
        assert_eq!(quoted(b"( \t\"x\" y", 0x20), b"x");
        assert_eq!(quoted(b"(a \"x\"", 0x20), b"");
        assert_eq!(quoted(b"\"0123456789", 4), b"012");
    }

    #[test]
    fn stack_alloc_aligns_then_fits_below_the_cap() {
        assert_eq!(stack_alloc(0x13, 0x100, 0x20, 0x10), Ok((0x20, 0x40)));
        assert_eq!(stack_alloc(0x20, 0x100, 0, 1), Ok((0x20, 0x20)));
        assert!(stack_alloc(0xe1, 0x100, 0x10, 0x10).is_err());
        assert!(stack_alloc(0xd0, 0x100, 0x30, 0x10).is_err());
    }
}

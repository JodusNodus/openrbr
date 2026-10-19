//! Reader for the per-car `repair.ini` damage hierarchy (`Physics/<car>/repair.ini`).
//!
//! Layout, from the parser `E75_Svc::M5b16f0` (`0x5b16f0`) and its recursive part reader
//! `E73_0dc0::M0dc0` (`0x5b0dc0`), both in `src/rbr/RBR_E73_List.cpp` and `RBR_E73_GS.cpp`:
//!
//! - The first token is the total number of parts in the tree, `74` in every retail file.
//! - Each top-level part is a `{` block. Inside a block, in order: physics ID (int), importance
//!   (int), debug name (one word), unmount time, unmount min variance, unmount max variance,
//!   mount time, mount min variance, mount max variance (floats). Child parts follow as nested
//!   `{` blocks, and the block ends with `}`.
//! - `;` starts a comment that runs to the end of the line. The file ends with `$`.
//!
//! The loader reads top-level blocks until the parts it has read, children included, reach the
//! header total (`n += i` in the loop of `M5b16f0`). A block is read whole, so the last one can
//! take the total past the header. Retail: seven cars have 76 nodes against the header 74, all
//! 76 are read, and the loader logs a warning. `mg_zr` has 69 nodes; the loader would expect
//! another `{` and hit a debug break, so a short file is an error here. See
//! `notes/formats.md` "Cars".
//!
//! Only the parsed fields are kept. The game draws the actual mount and unmount times from
//! these values; that is not modelled here.

use std::fmt::{self, Display, Formatter};

/// Deepest nesting accepted. The retail trees go up to a few levels.
const MAX_DEPTH: usize = 64;

/// Unmount or mount timing, as written in the file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub time: f32,
    pub min_variance: f32,
    pub max_variance: f32,
}

/// One part of the damage tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub physics_id: i32,
    pub importance: i32,
    pub name: String,
    pub unmount: Timing,
    pub mount: Timing,
    pub children: Vec<Part>,
}

/// Failure to read `repair.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The first token is not a part count.
    BadCount,
    /// A token where a value, `{` or `}` was expected.
    Unexpected { token: String },
    /// Input ended inside a block.
    UnexpectedEnd,
    /// Nesting deeper than [`MAX_DEPTH`].
    TooDeep,
    /// The file ends before the parts read reach the count in the header.
    PartsShort { declared: usize, found: usize },
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadCount => write!(f, "repair.ini: first token is not a part count"),
            Error::Unexpected { token } => write!(f, "repair.ini: unexpected token {token:?}"),
            Error::UnexpectedEnd => write!(f, "repair.ini: input ends inside a part"),
            Error::TooDeep => write!(f, "repair.ini: parts nested deeper than {MAX_DEPTH}"),
            Error::PartsShort { declared, found } => {
                write!(f, "repair.ini: header counts {declared} parts, file has {found}")
            }
        }
    }
}

impl std::error::Error for Error {}

/// Splits the text into whitespace-separated tokens, dropping `;` comments.
fn tokens(text: &str) -> Vec<&str> {
    text.lines()
        .flat_map(|line| line.split(';').next().unwrap_or("").split_whitespace())
        .take_while(|t| *t != "$")
        .collect()
}

struct Reader<'a> {
    tokens: &'a [&'a str],
    pos: usize,
    found: usize,
}

impl<'a> Reader<'a> {
    fn next(&mut self) -> Result<&'a str, Error> {
        let token = self.tokens.get(self.pos).copied().ok_or(Error::UnexpectedEnd)?;
        self.pos += 1;
        Ok(token)
    }

    fn peek(&self) -> Option<&'a str> {
        self.tokens.get(self.pos).copied()
    }

    fn expect(&mut self, want: &str) -> Result<(), Error> {
        let token = self.next()?;
        if token == want {
            Ok(())
        } else {
            Err(Error::Unexpected { token: token.to_owned() })
        }
    }

    fn int(&mut self) -> Result<i32, Error> {
        let token = self.next()?;
        token.parse().map_err(|_| Error::Unexpected { token: token.to_owned() })
    }

    fn float(&mut self) -> Result<f32, Error> {
        let token = self.next()?;
        token.parse().map_err(|_| Error::Unexpected { token: token.to_owned() })
    }

    fn timing(&mut self) -> Result<Timing, Error> {
        Ok(Timing { time: self.float()?, min_variance: self.float()?, max_variance: self.float()? })
    }

    /// One part block, after its `{` has been read. Consumes the closing `}`.
    fn part(&mut self, depth: usize) -> Result<Part, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::TooDeep);
        }
        self.found += 1;
        let physics_id = self.int()?;
        let importance = self.int()?;
        let name = self.next()?.to_owned();
        let unmount = self.timing()?;
        let mount = self.timing()?;
        let mut children = Vec::new();
        while self.peek() == Some("{") {
            self.pos += 1;
            children.push(self.part(depth + 1)?);
        }
        self.expect("}")?;
        Ok(Part { physics_id, importance, name, unmount, mount, children })
    }
}

/// Parses a `repair.ini` file into its top-level parts, as the game reads them: top-level blocks
/// are read until the parts read (children included) reach the header count, and a block is read
/// whole. Use [`node_count`] to count parts with their children.
pub fn parse(text: &str) -> Result<Vec<Part>, Error> {
    let tokens = tokens(text);
    let header: usize = tokens.first().and_then(|t| t.parse().ok()).ok_or(Error::BadCount)?;
    let mut reader = Reader { tokens: &tokens[1..], pos: 0, found: 0 };
    let mut parts = Vec::new();
    while reader.found < header {
        if reader.peek().is_none() {
            return Err(Error::PartsShort { declared: header, found: reader.found });
        }
        reader.expect("{")?;
        parts.push(reader.part(0)?);
    }
    Ok(parts)
}

/// Number of parts in `parts`, children included.
pub fn node_count(parts: &[Part]) -> usize {
    parts.iter().map(|p| 1 + node_count(&p.children)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_PARTS: &str = "2\t; Num parts\n; hierarchy\n\n{\t\t\t; Windshield\n\t28\t\t; Physics ID\n\
        \t5\t\t; Importance\n\tWindshield\t; Debug string\n\t50.0\t\t; Unmount time\n\t20.0\t\t; min\n\
        \t20.0\t\t; max\n\t120.0\t\t; Mount time\n\t20.0\t\t; min\n\t60.0\t\t; max\n\
        \t{\n\t\t1\n\t\t2\n\t\tChild\n\t\t0.0\n\t\t0.0\n\t\t0.0\n\t\t1.0\n\t\t1.0\n\t\t1.0\n\t}\n}\n\n\
        {\n\t3\n\t1\n\tBodywork\n\t0.0\n\t0.0\n\t0.0\n\t260.0\n\t20.0\n\t20.0\n}\n$\t; Dont remove me\n";

    #[test]
    fn parses_fields_in_file_order_with_children() {
        let text = TWO_PARTS.replacen("2\t; Num parts", "3\t; Num parts", 1);
        let parts = parse(&text).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(node_count(&parts), 3);
        let w = &parts[0];
        assert_eq!((w.physics_id, w.importance, w.name.as_str()), (28, 5, "Windshield"));
        assert_eq!(w.unmount, Timing { time: 50.0, min_variance: 20.0, max_variance: 20.0 });
        assert_eq!(w.mount.time, 120.0);
        assert_eq!(w.children.len(), 1);
        assert_eq!(w.children[0].name, "Child");
        assert_eq!(parts[1].name, "Bodywork");
        assert_eq!(parts[1].mount.time, 260.0);
    }

    #[test]
    fn blocks_past_the_header_count_are_not_read() {
        // Header 2 is reached by Windshield plus its child, so Bodywork is ignored, as the game does.
        let parts = parse(TWO_PARTS).unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(node_count(&parts), 2);
    }

    #[test]
    fn rejects_bad_count_and_syntax() {
        assert_eq!(parse("").unwrap_err(), Error::BadCount);
        assert_eq!(parse("x\n").unwrap_err(), Error::BadCount);
        assert!(matches!(parse("1\n{ 1 2 Name 1 2 3 4 5 }\n").unwrap_err(), Error::Unexpected { .. }));
        assert_eq!(parse("1\n{\n1\n").unwrap_err(), Error::UnexpectedEnd);
    }

    #[test]
    fn short_file_is_an_error() {
        let one = "1\n{\n1\n2\nA\n0\n0\n0\n0\n0\n0\n}\n";
        assert_eq!(parse(one).unwrap().len(), 1);
        assert_eq!(parse(&one.replacen('1', "2", 1)).unwrap_err(), Error::PartsShort { declared: 2, found: 1 });
        assert_eq!(
            parse(&format!("2\n{}", "{\n1\n2\nA\n0\n0\n0\n0\n0\n0\n}\n")).unwrap_err(),
            Error::PartsShort { declared: 2, found: 1 }
        );
    }

    #[test]
    fn nesting_limit_is_enforced() {
        let depth = MAX_DEPTH + 2;
        let mut text = format!("{depth}\n");
        for _ in 0..depth {
            text.push_str("{\n1\n2\nN\n0\n0\n0\n0\n0\n0\n");
        }
        for _ in 0..depth {
            text.push_str("}\n");
        }
        assert_eq!(parse(&text).unwrap_err(), Error::TooDeep);
    }
}

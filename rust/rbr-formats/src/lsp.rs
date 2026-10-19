//! Reader for the game's `.lsp` setup scripts.
//!
//! An `.lsp` file is Lisp-like text: nested parenthesised lists with
//! `"quoted"` names, bare `key value` atoms (words, numbers, `+FLAG`/`-FLAG`)
//! and `;` / `//` line comments. The game tokenises files with the function at
//! `0x52ec80` (whitespace/quotes/parens only — comments survive tokenising and
//! are skipped later as unknown keys) and applies each file to the live
//! `NPhysics::NTweak` tree. See `notes/formats/physics-lsp.md`.
//!
//! [`parse`] builds a plain tree ([`Node`]) with no semantics. The typed key tables
//! (`crate::lsp_keys`: every key to its class and field offset) live separately.
//! Two observed
//! quirks are accepted for compatibility with the retail files and the game's
//! tolerant loader: a `NUL` byte reads as trivia (`Misc/state.lsp` ships one
//! trailing `NUL`, invisible to the game's C-string reads), and a single
//! stray top-level `)` is accepted (both retail `tyres.lsp` ship one extra
//! closer; the game's tokenizer simply stops there).

use std::fmt::{self, Display, Formatter};

/// One parsed item: a bare atom, a `"quoted"` string (content without the
/// quotes), or a parenthesised list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Atom(String),
    Quoted(String),
    List(Vec<Node>),
}

/// Parse failure with a 1-based line and byte column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, message: &str) -> ParseError {
        ParseError { line: self.line, column: self.col, message: message.to_owned() }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }

    fn bump(&mut self) {
        if self.bytes.get(self.pos) == Some(&b'\n') {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        self.pos += 1;
    }

    fn skip_line(&mut self) {
        while let Some(b) = self.peek() {
            if b == b'\n' {
                break;
            }
            self.bump();
        }
    }

    /// Whitespace, `NUL` bytes and `;` / `//` line comments.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n' | b'\0') => self.bump(),
                Some(b';') => self.skip_line(),
                Some(b'/') if self.peek_next() == Some(b'/') => self.skip_line(),
                _ => break,
            }
        }
    }

    fn parse_quoted(&mut self) -> Result<Node, ParseError> {
        debug_assert_eq!(self.peek(), Some(b'"'));
        self.bump();
        let start = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated quoted string")),
                Some(b'"') => {
                    let content = &self.bytes[start..self.pos];
                    self.bump();
                    // Retail files are ASCII; delimiters are ASCII so this
                    // slice always ends on a char boundary for UTF-8 input.
                    return Ok(Node::Quoted(String::from_utf8_lossy(content).into_owned()));
                }
                Some(_) => self.bump(),
            }
        }
    }

    fn parse_atom(&mut self) -> Node {
        let start = self.pos;
        while let Some(b) = self.peek() {
            match b {
                b' ' | b'\t' | b'\r' | b'\n' | b'\0' | b'(' | b')' | b'"' | b';' => break,
                b'/' if self.peek_next() == Some(b'/') => break,
                _ => self.bump(),
            }
        }
        Node::Atom(String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned())
    }

    fn parse_item(&mut self) -> Result<Node, ParseError> {
        match self.peek() {
            Some(b'(') => {
                self.bump();
                let mut items = Vec::new();
                loop {
                    self.skip_trivia();
                    match self.peek() {
                        None => return Err(self.error("unexpected end of file inside list")),
                        Some(b')') => {
                            self.bump();
                            return Ok(Node::List(items));
                        }
                        Some(_) => items.push(self.parse_item()?),
                    }
                }
            }
            Some(b'"') => self.parse_quoted(),
            Some(b')') => Err(self.error("unexpected ')'")),
            Some(_) => Ok(self.parse_atom()),
            None => Err(self.error("unexpected end of file")),
        }
    }

    fn parse_document(&mut self) -> Result<Vec<Node>, ParseError> {
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                None => return Ok(items),
                // The game's tokenizer stops at a stray top-level ')'; both
                // retail tyres.lsp ship one extra closer. Accept it, but
                // require end of file right after so real corruption still
                // fails loudly instead of being silently ignored.
                Some(b')') => {
                    self.bump();
                    self.skip_trivia();
                    if self.peek().is_some() {
                        return Err(self.error("unexpected text after top-level list"));
                    }
                    return Ok(items);
                }
                Some(_) => items.push(self.parse_item()?),
            }
        }
    }
}

/// Parses a whole `.lsp` document into its top-level items.
///
/// Every retail file yields exactly one [`Node::List`].
pub fn parse(text: &str) -> Result<Vec<Node>, ParseError> {
    Parser { bytes: text.as_bytes(), pos: 0, line: 1, col: 1 }.parse_document()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: Vec<Node>) -> Node {
        Node::List(items)
    }

    fn atom(s: &str) -> Node {
        Node::Atom(s.to_owned())
    }

    fn quoted(s: &str) -> Node {
        Node::Quoted(s.to_owned())
    }

    #[test]
    fn minimal_document() {
        let doc = parse("( ( \"PhysicsEngine\"\n MaxTimeStep 0.03 ) )\n").unwrap();
        assert_eq!(doc, vec![list(vec![list(vec![quoted("PhysicsEngine"), atom("MaxTimeStep"), atom("0.03"),]),])]);
    }

    #[test]
    fn comments_flags_and_quoted_parens() {
        let doc = parse("( ( \"X\" ;id 17, rev 1203\n+DAMP -WORN (\"(tarmac_dry)\") // note\n) )\n").unwrap();
        assert_eq!(
            doc,
            vec![list(vec![list(
                vec![quoted("X"), atom("+DAMP"), atom("-WORN"), list(vec![quoted("(tarmac_dry)")]),]
            )])]
        );
    }

    #[test]
    fn stray_trailing_closer_accepted() {
        assert_eq!(parse("( ( \"X\" )) )").unwrap(), vec![list(vec![list(vec![quoted("X")])])]);
    }

    #[test]
    fn trailing_junk_rejected() {
        assert!(parse("( ( \"X\" )) ) junk").is_err());
        assert!(parse("( ( \"X\" )").is_err());
        assert!(parse("( \"X\" ").is_err());
        assert!(parse("( \"unterminated )").is_err());
        assert!(parse(")").unwrap().is_empty());
        assert!(parse("").unwrap().is_empty());
    }
}

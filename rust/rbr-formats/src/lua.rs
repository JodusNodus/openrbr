//! Reader for the character scripts in `Root/Scripts.rbz` (`Scripts/**/*.lua`), the files the exe runs for every
//! stage character (`notes/stage-characters.md`, sections 2 and 5). The exe links Lua 4.0 with tolua (the strings
//! `Lua 4.0`, `settagmethod`, `tolua_tbl_itype` in `RichardBurnsRally_SSE.exe`), so there are no booleans: `true` and
//! `false` are plain globals (`Scripts/startup.lua` sets them to 1 and 0), and only `nil` is false.
//!
//! The retail scripts use a small part of Lua: global and `local` assignments, `function` definitions, `if` /
//! `elseif` / `else`, `return` with several values (last in its block), calls (`f(...)`, `app:Method(...)`),
//! numbers, strings, `nil`, and the operators `or and not == ~= < <= > >= .. + - * /` (Lua's precedence). This module
//! parses exactly that into a syntax tree; anything else (loops, tables, `local function`) is an error, so a script
//! the subset does not cover is reported, not misread. Running the tree is the caller's (`rbr-render`'s
//! `characters::script`).

use std::fmt::{self, Display, Formatter};

/// A parsed script: its top-level statements in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub body: Block,
}

pub type Block = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `[local] a, b = x, y`.
    Assign {
        local: bool,
        names: Vec<String>,
        values: Vec<Expr>,
    },
    /// A call used as a statement.
    Call(Expr),
    /// `if c then ... elseif c then ... else ... end`: the condition-block pairs in order and the `else` block.
    If {
        branches: Vec<(Expr, Block)>,
        otherwise: Block,
    },
    Return(Vec<Expr>),
    /// `function name(params) ... end` (global).
    Function {
        name: String,
        params: Vec<String>,
        body: Block,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Nil,
    Number(f64),
    Str(String),
    Name(String),
    /// `f(args)`.
    Call {
        function: String,
        args: Vec<Expr>,
    },
    /// `object:method(args)`.
    Method {
        object: String,
        method: String,
        args: Vec<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Not(Box<Expr>),
    Negate(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
}

/// A parse failure: the 1-based line and what went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

/// Parses a script's text. Bytes that are not UTF-8 (a few retail comments) are read as Latin-1.
pub fn parse(bytes: &[u8]) -> Result<Chunk, Error> {
    let text: String = match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    let tokens = tokenize(&text)?;
    let mut parser = Parser { tokens, at: 0 };
    let body = parser.block()?;
    match parser.peek() {
        Tok::Eof => Ok(Chunk { body }),
        other => Err(parser.error(format!("unexpected {other:?}"))),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Number(f64),
    Str(String),
    /// A keyword or an operator / punctuation.
    Sym(&'static str),
    Eof,
}

/// Lua 4.0's reserved words (`llex.c`): no `true`/`false`.
const KEYWORDS: [&str; 19] = [
    "and", "break", "do", "else", "elseif", "end", "for", "function", "if", "in", "local", "nil", "not", "or",
    "repeat", "return", "then", "until", "while",
];

/// Longest first, so `..` wins over `.` and `==` over `=`.
const SYMBOLS: [&str; 22] = [
    "...", "..", "==", "~=", "<=", ">=", "<", ">", "=", "(", ")", ",", ":", ";", "+", "-", "*", "/", ".", "{", "}", "#",
];

fn tokenize(text: &str) -> Result<Vec<(Tok, usize)>, Error> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let (mut i, mut line) = (0, 1);
    let error = |line, message: String| Error { line, message };
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            if chars.get(i + 2) == Some(&'[') && chars.get(i + 3) == Some(&'[') {
                return Err(error(line, "block comments are not in the subset".to_owned()));
            }
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let value = word.parse::<f64>().map_err(|_| error(line, format!("bad number {word}")))?;
            tokens.push((Tok::Number(value), line));
        } else if c == '"' || c == '\'' {
            let mut value = String::new();
            i += 1;
            loop {
                match chars.get(i) {
                    None | Some('\n') => return Err(error(line, "unterminated string".to_owned())),
                    Some(&q) if q == c => break,
                    Some('\\') => {
                        let escaped = chars.get(i + 1).copied().ok_or_else(|| error(line, "bad escape".to_owned()))?;
                        value.push(match escaped {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        i += 2;
                    }
                    Some(&other) => {
                        value.push(other);
                        i += 1;
                    }
                }
            }
            i += 1;
            tokens.push((Tok::Str(value), line));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let token = match KEYWORDS.iter().find(|k| **k == word) {
                Some(k) => Tok::Sym(k),
                None => Tok::Name(word),
            };
            tokens.push((token, line));
        } else {
            let rest: String = chars[i..chars.len().min(i + 3)].iter().collect();
            let symbol = SYMBOLS
                .iter()
                .find(|s| rest.starts_with(**s))
                .ok_or_else(|| error(line, format!("unexpected character {c:?}")))?;
            tokens.push((Tok::Sym(symbol), line));
            i += symbol.len();
        }
    }
    tokens.push((Tok::Eof, line));
    Ok(tokens)
}

struct Parser {
    tokens: Vec<(Tok, usize)>,
    at: usize,
}

/// Binary operators by Lua 5.0 precedence, loosest first; `..` is right-associative.
const LEVELS: [&[(&str, BinOp)]; 6] = [
    &[("or", BinOp::Or)],
    &[("and", BinOp::And)],
    &[("==", BinOp::Eq), ("~=", BinOp::Ne), ("<", BinOp::Lt), ("<=", BinOp::Le), (">", BinOp::Gt), (">=", BinOp::Ge)],
    &[("..", BinOp::Concat)],
    &[("+", BinOp::Add), ("-", BinOp::Sub)],
    &[("*", BinOp::Mul), ("/", BinOp::Div)],
];
const CONCAT_LEVEL: usize = 3;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.at].0
    }

    fn next(&mut self) -> Tok {
        let token = self.tokens[self.at].0.clone();
        if self.at + 1 < self.tokens.len() {
            self.at += 1;
        }
        token
    }

    fn error(&self, message: String) -> Error {
        Error { line: self.tokens[self.at].1, message }
    }

    fn is(&self, symbol: &str) -> bool {
        matches!(self.peek(), Tok::Sym(s) if *s == symbol)
    }

    fn accept(&mut self, symbol: &str) -> bool {
        let found = self.is(symbol);
        if found {
            self.next();
        }
        found
    }

    fn expect(&mut self, symbol: &str) -> Result<(), Error> {
        if self.accept(symbol) {
            Ok(())
        } else {
            Err(self.error(format!("expected `{symbol}`, found {:?}", self.peek())))
        }
    }

    fn name(&mut self) -> Result<String, Error> {
        match self.next() {
            Tok::Name(name) => Ok(name),
            other => Err(self.error(format!("expected a name, found {other:?}"))),
        }
    }

    /// Statements up to a block end (`end`, `else`, `elseif` or the end of the file), not consumed.
    fn block(&mut self) -> Result<Block, Error> {
        let mut body = Vec::new();
        loop {
            while self.accept(";") {}
            if matches!(self.peek(), Tok::Eof) || ["end", "else", "elseif"].iter().any(|s| self.is(s)) {
                return Ok(body);
            }
            let stmt = self.statement()?;
            let last = matches!(stmt, Stmt::Return(_));
            body.push(stmt);
            if last {
                while self.accept(";") {}
                return Ok(body);
            }
        }
    }

    fn statement(&mut self) -> Result<Stmt, Error> {
        if self.accept("function") {
            let name = self.name()?;
            self.expect("(")?;
            let mut params = Vec::new();
            if !self.is(")") {
                params.push(self.name()?);
                while self.accept(",") {
                    params.push(self.name()?);
                }
            }
            self.expect(")")?;
            let body = self.block()?;
            self.expect("end")?;
            return Ok(Stmt::Function { name, params, body });
        }
        if self.accept("if") {
            let mut branches = Vec::new();
            let mut otherwise = Vec::new();
            loop {
                let condition = self.expr(0)?;
                self.expect("then")?;
                branches.push((condition, self.block()?));
                if self.accept("elseif") {
                    continue;
                }
                if self.accept("else") {
                    otherwise = self.block()?;
                }
                self.expect("end")?;
                return Ok(Stmt::If { branches, otherwise });
            }
        }
        if self.accept("return") {
            let mut values = Vec::new();
            if !(matches!(self.peek(), Tok::Eof)
                || self.is("end")
                || self.is("else")
                || self.is("elseif")
                || self.is(";"))
            {
                values = self.expr_list()?;
            }
            return Ok(Stmt::Return(values));
        }
        let local = self.accept("local");
        if let Tok::Sym(s) = self.peek() {
            return Err(self.error(format!("`{s}` is not in the subset")));
        }
        let first = self.name()?;
        if !local && (self.is("(") || self.is(":")) {
            return Ok(Stmt::Call(self.call(first)?));
        }
        let mut names = vec![first];
        while self.accept(",") {
            names.push(self.name()?);
        }
        let values = if self.accept("=") {
            self.expr_list()?
        } else if local {
            Vec::new()
        } else {
            return Err(self.error("expected `=` or a call".to_owned()));
        };
        Ok(Stmt::Assign { local, names, values })
    }

    fn expr_list(&mut self) -> Result<Vec<Expr>, Error> {
        let mut values = vec![self.expr(0)?];
        while self.accept(",") {
            values.push(self.expr(0)?);
        }
        Ok(values)
    }

    fn expr(&mut self, level: usize) -> Result<Expr, Error> {
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.expr(level + 1)?;
        loop {
            let Some(&(_, op)) = LEVELS[level].iter().find(|(s, _)| self.is(s)) else {
                return Ok(left);
            };
            self.next();
            // `..` is right-associative: its right side may hold more `..`.
            let right = self.expr(if level == CONCAT_LEVEL { level } else { level + 1 })?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
            if level == CONCAT_LEVEL {
                return Ok(left);
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.accept("not") {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.accept("-") {
            return Ok(Expr::Negate(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        match self.next() {
            Tok::Number(n) => Ok(Expr::Number(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Sym("nil") => Ok(Expr::Nil),
            Tok::Sym("(") => {
                let inner = self.expr(0)?;
                self.expect(")")?;
                Ok(inner)
            }
            Tok::Name(name) if self.is("(") || self.is(":") => self.call(name),
            Tok::Name(name) => Ok(Expr::Name(name)),
            other => Err(self.error(format!("unexpected {other:?} in an expression"))),
        }
    }

    /// `name(args)` or `name:method(args)`, after `name`.
    fn call(&mut self, name: String) -> Result<Expr, Error> {
        let method = if self.accept(":") { Some(self.name()?) } else { None };
        self.expect("(")?;
        let args = if self.is(")") { Vec::new() } else { self.expr_list()? };
        self.expect(")")?;
        Ok(match method {
            Some(method) => Expr::Method { object: name, method, args },
            None => Expr::Call { function: name, args },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_run_script() {
        let text = b"-- comment\nfunction Update( state )\n if state == 0\n then return 0, state end\n state = 2\n \
                     return 1, state\nend -- Tic( state )\n";
        let chunk = parse(text).unwrap();
        let [Stmt::Function { name, params, body }] = chunk.body.as_slice() else { panic!("{chunk:?}") };
        assert_eq!((name.as_str(), params.as_slice()), ("Update", ["state".to_owned()].as_slice()));
        assert_eq!(body.len(), 3);
        assert!(matches!(&body[2], Stmt::Return(values) if values.len() == 2));
    }

    #[test]
    fn precedence_and_concat() {
        let chunk = parse(b"x = a .. \"b\" .. c\ny = 1 + 2 * 3 == 7 and not z\nscale = .01\nfalse = 0").unwrap();
        let Stmt::Assign { values, .. } = &chunk.body[0] else { panic!() };
        let Expr::Binary { op: BinOp::Concat, right, .. } = &values[0] else { panic!() };
        assert!(matches!(**right, Expr::Binary { op: BinOp::Concat, .. }));
        let Stmt::Assign { values, .. } = &chunk.body[1] else { panic!() };
        assert!(matches!(&values[0], Expr::Binary { op: BinOp::And, .. }));
        assert_eq!(
            chunk.body[2],
            Stmt::Assign { local: false, names: vec!["scale".into()], values: vec![Expr::Number(0.01)] }
        );
        assert_eq!(
            chunk.body[3],
            Stmt::Assign { local: false, names: vec!["false".into()], values: vec![Expr::Number(0.0)] }
        );
    }

    #[test]
    fn method_calls_and_unsupported_syntax() {
        let chunk = parse(b"app:LoadAnimation( rootpath..\"Sga\\\\a.sga\", \"idle\" .. x )").unwrap();
        assert!(
            matches!(&chunk.body[0], Stmt::Call(Expr::Method { method, args, .. }) if method == "LoadAnimation" && args.len() == 2)
        );
        assert!(parse(b"for i = 1, 2 do end").is_err());
        assert!(parse(b"t = {}").is_err());
        assert!(parse(b"x = \"open").is_err());
        assert!(parse(b"function f() return 0\nx = 1 end").is_err(), "return must end its block");
    }
}

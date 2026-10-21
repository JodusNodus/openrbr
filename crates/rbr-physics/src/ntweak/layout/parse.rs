//! `NTweak::CBase::Parse` (0x52f0c0) on the token tables in game memory, as the original makes them: the group's own
//! tokens and each inner group's (`CTokenCursor::Tokenize`, 0x52ec80, cutting the text in place), the name and the
//! items on the token heap (`E11_StackAlloc`, 0x473a70). The walk over the tokens is `super::plan`'s over the reader's
//! items; a member object's own group goes to its slot 2 with the cursor, as the original hands it.
//!
//! The group's walk is worked out before anything is written, so an `Unsupported` group writes nothing. A full token
//! heap stops it midway; the original traps there.

use rbr_formats::lsp_keys::Class;

use super::{
    c_string, lookup, member_at, BASE_PARSE, BIT_FIELDS, BIT_FIELD_PARSE, INTEGER_PARSE, ITEM_BLOCK, NGP_TABLE,
    NGP_TABLE_LEN, NGP_TABLE_VTABLE, REAL_PARSE, UNK14_SKIP, UNSIGNED_PARSE, VALUE, VECTOR4_PARSE, VECTOR_PARSE,
    VECTOR_VALUE,
};
use crate::mem::Mem;
use crate::ntweak::tokens::{self, is_group, quoted, Tokens};
use crate::ntweak::{atof, atoi, is_float, is_integer, set_bit, Unsupported};
use crate::physics_core::flags;

/// `g_tokenHeap`, the word holding the token heap's record (base, used, capacity), and `g_emptyTokens`, the table an
/// empty text gets.
pub const TOKEN_HEAP: u32 = 0x165f1ac;
pub const EMPTY_TOKENS: u32 = 0x165f1a8;
/// A table's bytes (`operator new(0x4000)`).
const TABLE_BYTES: u32 = 4 * tokens::TABLE_WORDS as u32;
/// The token heap's alignment for names and items.
const HEAP_ALIGN: u32 = 0x10;

/// The calls `Parse` makes outside the walk.
pub trait ParseHost {
    /// `operator new(size)`: a table, in `Tokenize`.
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32;
    /// `operator delete(at)` on a table.
    fn free(&mut self, mem: &mut Mem, at: u32);
    /// The object's slot 5 (`Unk14`), when it is not `CBase`'s, on an inner group's item: `(name, block, cursor)`,
    /// the group's name (or 0), the item's token-heap block and the inner group's cursor.
    fn unk14(&mut self, mem: &mut Mem, this: u32, name: u32, block: u32, cursor: u32);
    /// A member object's slot 2 (`CBase::Parse`) with the group's cursor at its token.
    fn member_parse(&mut self, mem: &mut Mem, member: u32, cursor: u32);
    /// The object's slot 17 (`Unk44`), after its tokens.
    fn after_parse(&mut self, mem: &mut Mem, this: u32);
    /// `CDictBase::Find` on the definition registry (0x7c8040), the definition manager's dictionary: the index of the
    /// definition named `name`.
    fn def_index(&mut self, mem: &mut Mem, name: &str) -> Option<u32>;
}

/// The two cursors on `Parse`'s frame (table, then current token): the group's and an inner group's.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub tokens: u32,
    pub inner: u32,
}

/// The bytes at `at` up to their NUL.
fn c_bytes(mem: &Mem, at: u32) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(c) = Some(mem.u8(at + out.len() as u32)).filter(|&c| c != 0) {
        out.push(c);
    }
    out
}

/// `text` cut as `Tokenize` cuts it: the tokens' bytes.
fn token_bytes(text: &[u8], tokens: &Tokens) -> Vec<Vec<u8>> {
    let mut cut = text.to_vec();
    for &at in &tokens.cuts {
        cut[at as usize] = 0;
    }
    let token = |at: usize| cut[at..].iter().take_while(|&&c| c != 0).copied().collect();
    tokens.starts.iter().map(|&at| token(at as usize)).collect()
}

/// The table of `tokens` of the text at `text` on the cursor at `cursor`, and the text cut.
fn write_tokens(mem: &mut Mem, table: u32, cursor: u32, text: u32, tokens: &Tokens) {
    mem.set_u32(cursor, table);
    mem.set_u32(cursor + 4, table);
    for (k, &at) in tokens.starts.iter().enumerate() {
        mem.set_u32(table + 4 * k as u32, text + at);
    }
    mem.set_u32(table + 4 * tokens.starts.len() as u32, 0);
    for &at in &tokens.cuts {
        mem.set_u8(text + at, 0);
    }
}

/// The tokens of the text at `text` (null for none), or `Unsupported` for a text `Tokenize` does not cover.
fn cut_at(mem: &Mem, text: u32) -> Result<(Vec<u8>, Tokens), Unsupported> {
    let bytes = if text == 0 { Vec::new() } else { c_bytes(mem, text) };
    let tokens = if bytes.is_empty() { Tokens::default() } else { tokens::tokenize(&bytes)? };
    Ok((bytes, tokens))
}

/// The table [`cut_at`] worked out, written: an empty text's is the shared empty table.
fn write_cut(mem: &mut Mem, alloc: impl FnOnce(&mut Mem, u32) -> u32, cursor: u32, text: u32, cut: &(Vec<u8>, Tokens)) {
    if cut.0.is_empty() {
        mem.set_u32(cursor, EMPTY_TOKENS);
        mem.set_u32(EMPTY_TOKENS, 0);
        mem.set_u32(cursor + 4, EMPTY_TOKENS);
    } else {
        let table = alloc(mem, TABLE_BYTES);
        write_tokens(mem, table, cursor, text, &cut.1);
    }
}

/// `CTokenCursor::Tokenize(text, 0)` on the cursor at `cursor`: the table (from `alloc`, `operator new(0x4000)`) of
/// the tokens of the text at `text`, cut in place; a null or empty text gets the shared empty table.
pub fn tokenize(
    mem: &mut Mem,
    alloc: impl FnOnce(&mut Mem, u32) -> u32,
    cursor: u32,
    text: u32,
) -> Result<(), Unsupported> {
    let cut = cut_at(mem, text)?;
    write_cut(mem, alloc, cursor, text, &cut);
    Ok(())
}

/// `E11_StackAlloc(size, heap, align)` on the heap record at `heap` (base, used, capacity): the block's address.
pub fn stack_alloc(mem: &mut Mem, size: u32, heap: u32, align: u32) -> Result<u32, Unsupported> {
    let (off, used) = tokens::stack_alloc(mem.u32(heap + 4), mem.u32(heap + 8), size, align)?;
    mem.set_u32(heap + 4, used);
    Ok(mem.u32(heap).wrapping_add(off))
}

/// One step of the group's walk, each taking its tokens from the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    /// The leading quoted string: a copy on the token heap, which `+0x04` points at.
    Name(Vec<u8>),
    /// A number, or a token no key names.
    Skip,
    /// An inner group, handed to the object's slot 5 item by item.
    Group,
    /// A key and the value tokens after it its member takes: the words they store.
    Key(Vec<(u32, u32)>, usize),
    /// A key with nothing after it, which ends the walk.
    Last,
    /// A key of a member object: its slot 2 takes the token after it.
    Member(u32),
}

/// The floats from token `i` on, up to `n`, as words from `at`: what a `CReal`, `CVector` or `CVector4` reader takes.
fn floats(toks: &[&str], i: usize, n: usize, at: u32) -> Vec<(u32, u32)> {
    toks[i..]
        .iter()
        .take(n)
        .take_while(|t| is_float(t))
        .enumerate()
        .map(|(k, t)| (at + 4 * k as u32, atof(t).to_bits()))
        .collect()
}

/// `CUnsigned::Parse` and `CInteger::Parse`: the token at `i` when it is an integer.
fn integer(toks: &[&str], i: usize, at: u32) -> Vec<(u32, u32)> {
    toks.get(i).filter(|t| is_integer(t)).map(|t| (at, atoi(t))).into_iter().collect()
}

/// The word at `at` as the walk has left it so far: its last store, else memory's.
fn current(mem: &Mem, steps: &[Step], at: u32) -> u32 {
    let stores = steps.iter().rev().flat_map(|s| match s {
        Step::Key(stores, _) => stores.iter().rev().copied().collect(),
        _ => Vec::new(),
    });
    stores.into_iter().find(|&(to, _)| to == at).map_or_else(|| mem.u32(at), |(_, value)| value)
}

/// `CBitField::Parse` (0x52f930) on the bit-field at `member`: the `+name` and `-name` tokens from `i` on.
fn bit_field(mem: &Mem, steps: &[Step], member: u32, toks: &[&str], i: usize) -> Result<Step, Unsupported> {
    let vtable = mem.u32(member);
    let Some(&(_, table, count)) = BIT_FIELDS.iter().find(|(v, _, _)| *v == vtable) else {
        return Err(Unsupported("a bit-field of a class not ported"));
    };
    let names: Vec<String> = (0..count).map(|k| c_string(mem, flags::bit_name(mem, table, k))).collect();
    let taken = toks[i..].iter().take_while(|t| t.starts_with(['+', '-'])).count();
    let at = member + VALUE;
    let mask = toks[i..i + taken].iter().fold(current(mem, steps, at), |mask, t| set_bit(mask, &names, t));
    Ok(Step::Key(if taken > 0 { vec![(at, mask)] } else { Vec::new() }, taken))
}

/// The member at `member`'s reader on the tokens from `i` on.
fn read_member(mem: &Mem, steps: &[Step], member: u32, toks: &[&str], i: usize) -> Result<Step, Unsupported> {
    let stores = |stores: Vec<(u32, u32)>| {
        let taken = stores.len();
        Step::Key(stores, taken)
    };
    if mem.u32(member) == NGP_TABLE_VTABLE {
        // NGP's reader: `CUnsigned::Parse`, then up to 30 floats.
        let mut out = integer(toks, i, member + VALUE);
        out.extend(floats(toks, i + out.len(), NGP_TABLE_LEN, member + NGP_TABLE));
        return Ok(stores(out));
    }
    Ok(match mem.u32(mem.u32(member) + 8) {
        REAL_PARSE => stores(floats(toks, i, 1, member + VALUE)),
        VECTOR_PARSE => stores(floats(toks, i, 3, member + VECTOR_VALUE)),
        VECTOR4_PARSE => stores(floats(toks, i, 4, member + VECTOR_VALUE)),
        UNSIGNED_PARSE | INTEGER_PARSE => stores(integer(toks, i, member + VALUE)),
        BIT_FIELD_PARSE => bit_field(mem, steps, member, toks, i)?,
        BASE_PARSE => Step::Member(member),
        _ => return Err(Unsupported("a member whose reader is not ported")),
    })
}

/// The walk over the group's tokens `toks` (cut out of `text`, whose inner groups are checked to tokenize).
fn plan(
    mem: &mut Mem,
    host: &mut impl ParseHost,
    this: u32,
    class: Class,
    toks: &[&str],
) -> Result<Vec<Step>, Unsupported> {
    let mut steps = Vec::new();
    let mut i = 0;
    if let Some(name) = toks.first().and_then(|t| t.strip_prefix('"')) {
        steps.push(Step::Name(name.as_bytes().to_vec()));
        i = 1;
    }
    while i < toks.len() {
        let t = toks[i];
        if is_integer(t) || is_float(t) {
            steps.push(Step::Skip);
            i += 1;
            continue;
        }
        if is_group(t.as_bytes()) {
            tokens::tokenize(t.as_bytes())?;
            steps.push(Step::Group);
            i += 1;
            continue;
        }
        let member = match class {
            Class::DefManager if host.def_index(mem, t).is_some() => {
                return Err(Unsupported("an atom of the definition manager's group names a definition"))
            }
            Class::DefManager => None,
            _ => lookup(class, t).map(|info| member_at(this, &info)),
        };
        let Some(member) = member else {
            steps.push(Step::Skip);
            i += 1;
            continue;
        };
        i += 1;
        if i == toks.len() {
            steps.push(Step::Last);
            break;
        }
        let step = read_member(mem, &steps, member, toks, i)?;
        i += match &step {
            Step::Key(_, taken) => *taken,
            _ => 1,
        };
        steps.push(step);
    }
    Ok(steps)
}

/// An inner group at the token `group` on the inner cursor at `cursor`: its tokens, then each item after its leading
/// quoted name copied to a token-heap block (`GetQuoted(block, 0x20)`) and handed to the object's slot 5, until the
/// cursor reaches the end; then its table freed.
fn inner_group(
    mem: &mut Mem,
    host: &mut impl ParseHost,
    this: u32,
    cursor: u32,
    group: u32,
) -> Result<(), Unsupported> {
    let cut = cut_at(mem, group)?;
    write_cut(mem, |mem, size| host.alloc(mem, size), cursor, group, &cut);
    let first = mem.u32(mem.u32(cursor + 4));
    let name = if first != 0 && mem.u8(first) == b'"' {
        mem.set_u32(cursor + 4, mem.u32(cursor + 4) + 4);
        first + 1
    } else {
        0
    };
    let heap = mem.u32(TOKEN_HEAP);
    loop {
        let at = mem.u32(cursor + 4);
        let item = mem.u32(at);
        if item == 0 {
            break;
        }
        let block = stack_alloc(mem, ITEM_BLOCK, heap, HEAP_ALIGN)?;
        let mut text = quoted(&c_bytes(mem, item), ITEM_BLOCK).to_vec();
        text.push(0);
        mem.map(block, &text);
        match mem.u32(mem.u32(this) + 0x14) {
            UNK14_SKIP => mem.set_u32(cursor + 4, at + 4),
            _ => {
                host.unk14(mem, this, name, block, cursor);
                assert_ne!(mem.u32(cursor + 4), at, "an Unk14 that leaves the item: the original loops forever");
            }
        }
    }
    let table = mem.u32(cursor);
    if table != EMPTY_TOKENS {
        host.free(mem, table);
    }
    mem.set_u32(cursor, 0);
    mem.set_u32(cursor + 4, 0);
    Ok(())
}

/// `CBase::Parse(cursor)` on the object at `this` of class `class`, with the cursor at `cursor` and the frame's two
/// cursors at `frame`: the group at the cursor's token cut into a table, the walk over its tokens, slot 17, the table
/// freed; the cursor moves past the group. A null token leaves everything.
pub fn parse(
    mem: &mut Mem,
    host: &mut impl ParseHost,
    frame: Frame,
    this: u32,
    class: Class,
    cursor: u32,
) -> Result<(), Unsupported> {
    let at = mem.u32(cursor + 4);
    let text = mem.u32(at);
    if text == 0 {
        return Ok(());
    }
    let cut = cut_at(mem, text)?;
    let bytes = token_bytes(&cut.0, &cut.1);
    let toks: Vec<&str> = bytes.iter().map(|t| std::str::from_utf8(t).expect("the tokenizer takes ASCII")).collect();
    let steps = plan(mem, host, this, class, &toks)?;

    write_cut(mem, |mem, size| host.alloc(mem, size), frame.tokens, text, &cut);
    mem.set_u32(cursor + 4, at + 4);
    let table = mem.u32(frame.tokens);
    let heap = mem.u32(TOKEN_HEAP);
    let mut i = 0;
    for step in steps {
        let token = table + 4 * i;
        match step {
            Step::Name(name) => {
                let block = stack_alloc(mem, name.len() as u32 + 1, heap, HEAP_ALIGN)?;
                mem.map(block, &[name.as_slice(), &[0]].concat());
                mem.set_u32(this + 4, block);
                i += 1;
            }
            Step::Skip => i += 1,
            Step::Group => {
                inner_group(mem, host, this, frame.inner, mem.u32(token))?;
                i += 1;
            }
            Step::Key(stores, taken) => {
                for (at, value) in stores {
                    mem.set_u32(at, value);
                }
                i += 1 + taken as u32;
            }
            Step::Last => i += 1,
            Step::Member(member) => {
                mem.set_u32(frame.tokens + 4, token + 4);
                host.member_parse(mem, member, frame.tokens);
                i += 2;
            }
        }
    }
    mem.set_u32(frame.tokens + 4, table + 4 * i);
    host.after_parse(mem, this);
    if table != EMPTY_TOKENS {
        host.free(mem, table);
    }
    Ok(())
}

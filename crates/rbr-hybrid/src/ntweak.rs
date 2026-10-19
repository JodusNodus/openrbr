//! Load-time bindings: the NTweak apply walk (`rbr_physics::ntweak`) at `NTweak::CBase::Parse` (0x52f0c0). They run
//! from load-time entry dumps (`RBR_SNAPAT`), not from step snapshots. The group comes from the cursor's current
//! token, read with the Rust reader; the original's own tokenizer still runs through the host, so its token table and
//! the text it cuts in place are the original's.

use rbr_formats::lsp::{self, Node};
use rbr_formats::lsp_keys::Class;
use rbr_physics::mem::Mem;
use rbr_physics::ntweak::{self, ApplyHost};

use crate::pure::CRT_DELETE;
use crate::{Binding, Ctx, Decline, Host, Regs, Return};

pub static BINDINGS: [Binding; 4] = [
    Binding { addr: 0x52f450, name: "NTweak::CBase::SetDirty", subsystem: "ntweak", run: set_dirty },
    Binding { addr: 0x52f090, name: "NTweak::CBase::CBase", subsystem: "ntweak", run: construct_base },
    Binding { addr: 0x52f480, name: "NTweak::CBase::Unk44", subsystem: "ntweak", run: crate::entities::empty_hook_run },
    Binding { addr: 0x52f0c0, name: "NTweak::CBase::Parse", subsystem: "ntweak", run: parse },
];

/// The objects the walk is ported for, by vtable.
const CLASSES: [(u32, Class); 1] = [(0x740124, Class::SpringDef)];

/// `CTokenCursor::Tokenize (text, 0)` (thiscall on the cursor), `E11_StackAlloc (size, stack, align)` and
/// `operator delete` (cdecl).
const TOKENIZE: u32 = 0x52ec80;
const STACK_ALLOC: u32 = 0x473a70;
/// `g_tokenHeap`, the arena names go to, and `g_emptyTokens`, the table of an empty text.
const TOKEN_HEAP: u32 = 0x165f1ac;
const EMPTY_TOKENS: u32 = 0x165f1a8;
/// The object's slot 17 (`Unk44`).
const SLOT_AFTER_PARSE: u32 = 0x44;

struct Original<'a>(&'a dyn Host);

impl ApplyHost for Original<'_> {
    fn alloc_name(&mut self, mem: &mut Mem, len: u32) -> u32 {
        let heap = mem.u32(TOKEN_HEAP);
        self.0.call(STACK_ALLOC, 0, &[len, heap, 0x10]).eax
    }

    fn after_parse(&mut self, mem: &mut Mem, this: u32) {
        self.0.call(mem.u32(mem.u32(this) + SLOT_AFTER_PARSE), this, &[]);
    }

    /// The ported classes' slot 5 is `CBase`'s, which links nothing.
    fn def_index(&mut self, _: &Mem, _: &str) -> Option<u32> {
        unreachable!("the spring-damper definition's inner groups link nothing")
    }

    /// Only the definition manager's slot 5 makes definitions.
    fn define(&mut self, _: &mut Mem, _: &str, _: u32, _: &str, _: &[Node]) -> Result<(), ntweak::Unsupported> {
        unreachable!("the spring-damper definition's inner groups define nothing")
    }
}

/// The NUL-terminated string at `at`.
fn c_string(mem: &Mem, at: u32) -> String {
    let mut out = Vec::new();
    loop {
        let chunk = mem.read(at + out.len() as u32, 64);
        match chunk.iter().position(|&c| c == 0) {
            Some(n) => {
                out.extend_from_slice(&chunk[..n]);
                return String::from_utf8_lossy(&out).into_owned();
            }
            None => out.extend_from_slice(&chunk),
        }
    }
}

/// thiscall `(cursor*)`, eax 0: the cursor's current token is the object's group, `(` up to the closing parenthesis
/// the tokenizer cut off.
fn parse(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let (this, cursor) = (r.ecx, r.stack[0]);
    let vtable = ctx.mem.u32(this);
    let Some(&(_, class)) = CLASSES.iter().find(|(v, _)| *v == vtable) else {
        return Err(Decline::Unsupported("the walk is ported for the spring-damper definition only"));
    };
    let current = ctx.mem.u32(cursor + 4);
    let text_at = ctx.mem.u32(current);
    if text_at == 0 {
        return Ok(Return::void(0, 4));
    }
    let text = c_string(&ctx.mem, text_at);
    let items = match lsp::parse(&format!("{text})")).ok().as_deref() {
        Some([Node::List(items)]) => items.clone(),
        _ => return Err(Decline::Unsupported("the group is not one list")),
    };
    let plan =
        ntweak::plan(&ctx.mem, crate::ngp::variant(), this, class, &items).map_err(|e| Decline::Unsupported(e.0))?;
    // The original's token table, on its frame's cursor, here a temporary below the entry.
    let tok = (r.esp - 0x100) & !0xf;
    ctx.host.call(TOKENIZE, tok, &[text_at, 0]);
    ctx.mem.set_u32(cursor + 4, current + 4);
    plan.apply(&mut ctx.mem, &mut Original(ctx.host)).map_err(|e| Decline::Unsupported(e.0))?;
    let table = ctx.mem.u32(tok);
    if table != EMPTY_TOKENS {
        ctx.host.call(CRT_DELETE, 0, &[table]);
    }
    Ok(Return::void(0, 4))
}

/// thiscall `()`: `NTweak::CBase::SetDirty`; eax is the caller's.
fn set_dirty(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ntweak::set_dirty(&mut ctx.mem, r.ecx);
    Ok(Return::void(r.eax, 0))
}

/// thiscall `(name)`: `NTweak::CBase::CBase`, eax is `this`.
fn construct_base(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    ntweak::construct_base(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(r.ecx, 4))
}

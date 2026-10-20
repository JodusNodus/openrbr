//! `E01Log` (0x43c8e0) and `GameLog` (0x490f50) on game memory (`super` describes them).

use super::{format, Args, Unsupported};
use crate::mem::Mem;

/// `GameLog`'s console flag: set, the text goes to `vprintf` instead of a sink.
const CONSOLE: u32 = 0x7c18f8;
/// The sink index of each channel and level (`channel + 8 * level`), and the sinks (8 bytes each, the object at +4).
const SINK_INDEX: u32 = 0x7c29d8;
const SINKS: u32 = 0x7c615c;
/// The sinks' class (`vtable 0x737af8`), whose slot 1 (0x4dee10) calls the function at the sink's +4 with the text
/// and its length.
const SINK_VTABLE: u32 = 0x737af8;
const SINK_WRITE: u32 = 0x4dee10;
/// The sink functions: the log file's (`fwrite` and `fflush` on the file at 0x8eb76c, when one is open) and the
/// debugger's (a copy for `OutputDebugStringA`).
const FILE_SINK: u32 = 0x4ded80;
const FILE: u32 = 0x8eb76c;
const DEBUG_SINK: u32 = 0x4dedb0;
/// The buffer `vsprintf` writes the text into.
pub const BUFFER: u32 = 0x7ece58;

/// The calls the debugger's sink makes: `operator new` and `operator delete` (cdecl). Its `OutputDebugStringA` has no
/// effect on the game's memory, so it is not a call here.
pub trait LogHost {
    fn alloc(&mut self, mem: &mut Mem, size: u32) -> u32;
    /// eax after the call.
    fn free(&mut self, mem: &mut Mem, at: u32) -> u32;
}

/// The variadic words from `at`, and the strings `%s` takes.
struct Stack<'a> {
    mem: &'a Mem,
    at: u32,
}

impl Args for Stack<'_> {
    fn word(&mut self) -> u32 {
        let w = self.mem.u32(self.at);
        self.at += 4;
        w
    }

    fn string(&self, at: u32) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(c) = Some(self.mem.u8(at + out.len() as u32)).filter(|&c| c != 0) {
            out.push(c);
        }
        out
    }
}

/// `E01Log(channel, level, fmt, ...)` with the variadic words at `args`: eax after it. With no sink, eax is the
/// channel; the log file's sink with no file open leaves 0; the debugger's sink writes the text into [`BUFFER`] and the
/// copy it frees, and leaves `operator delete`'s eax. The console and an open log file are [`Unsupported`].
pub fn log(
    mem: &mut Mem,
    host: &mut impl LogHost,
    channel: u32,
    level: u32,
    fmt: u32,
    args: u32,
) -> Result<u32, Unsupported> {
    if mem.u32(CONSOLE) != 0 {
        return Err(Unsupported("the log goes to the console"));
    }
    let index = mem.u32(SINK_INDEX.wrapping_add(channel.wrapping_add(level.wrapping_mul(8)).wrapping_mul(4)));
    let sink = mem.u32(SINKS.wrapping_add(index.wrapping_mul(8)));
    if sink == 0 {
        return Ok(channel);
    }
    let vtable = mem.u32(sink);
    if vtable != SINK_VTABLE || mem.u32(vtable + 4) != SINK_WRITE {
        return Err(Unsupported("a log sink of another class"));
    }
    let write = mem.u32(sink + 4);
    if write == FILE_SINK && mem.u32(FILE) != 0 {
        return Err(Unsupported("the log file is open"));
    }
    if write != FILE_SINK && write != DEBUG_SINK {
        return Err(Unsupported("a log sink the port does not cover"));
    }
    let stack = Stack { mem, at: args };
    let fmt = stack.string(fmt);
    let mut text = format(&fmt, &mut Stack { mem, at: args })?;
    text.push(0);
    mem.map(BUFFER, &text);
    if write == FILE_SINK {
        return Ok(0);
    }
    let copy = host.alloc(mem, text.len() as u32);
    mem.map(copy, &text);
    Ok(host.free(mem, copy))
}

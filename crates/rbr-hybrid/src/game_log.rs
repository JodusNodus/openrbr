//! `E01Log` (0x43c8e0), the game's logger (`rbr_physics::game_log`), and the logging callees of the signal emits.

use rbr_physics::game_log::{self, LogHost};
use rbr_physics::mem::Mem;

use crate::engine_driveline::E01LOG;
use crate::pure::CRT_DELETE;
use crate::rigid_body::OPERATOR_NEW;
use crate::{Binding, Ctx, Decline, Host, Regs, Return};

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    crate::bind("game_log", addr, name, run)
}

pub static BINDINGS: [Binding; 1] = [bind(E01LOG, "E01Log", log)];

/// The debugger sink's `operator new` and `operator delete`, entered where the original calls them: `E01Log` and
/// `GameLog` push 0x18 bytes below the entry, the sink's slot 1 and its function 0x18 more, and the function four
/// registers and the argument (the return slot 0x48 below `E01Log`'s).
struct Sink<'a> {
    host: &'a dyn Host,
    sp: u32,
}

impl LogHost for Sink<'_> {
    fn alloc(&mut self, _: &mut Mem, size: u32) -> u32 {
        self.host.call_at(self.sp, OPERATOR_NEW, 0, &[size]).eax
    }

    fn free(&mut self, _: &mut Mem, at: u32) -> u32 {
        self.host.call_at(self.sp, CRT_DELETE, 0, &[at]).eax
    }
}

/// cdecl `(channel, level, fmt, ...)`, eax as `GameLog` leaves it.
fn log(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [channel, level, fmt, ..] = r.stack;
    let mut sink = Sink { host: ctx.host, sp: r.esp - 0x48 };
    let eax = game_log::log(&mut ctx.mem, &mut sink, channel, level, fmt, r.esp + 0x10)
        .map_err(|e| Decline::Unsupported(e.0))?;
    Ok(Return::void(eax, 0))
}

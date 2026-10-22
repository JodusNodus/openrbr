//! Bindings of the stage's time keeper (`NRegulation::CTimeKeeper`, `src/rbr/RBR_E21_TimeKeeper.cpp`): `Record` (0x4a0540,
//! called by `CCar::Vf6cx` once a step) and `Update` (0x4a0320, its count steers `CCar::Update9AF0`). The typed port is
//! `rbr_gameplay::timing`; the memory is read into it and the words it changed are written back (`layout`).
//!
//! `Record`'s checkpoint is emitted by `CMultiSignal1::Emit` (0x4a0390, bound here) as a host call, so the connections run.
//! The same template's `CSigCarControl` instance (0x48cd90), which hands each step's control payload to the car before
//! `StepForward`, is bound here too.
//! The original emits from two frames: `Check` (the start, when `Check` started timing) before `Record` sets `last` and
//! `cur`, and `Record` itself otherwise. Both are replayed at the original's entry esp.

pub mod layout;

use rbr_gameplay::timing::{Checkpoint, CheckpointKind};
use rbr_physics::physics_core::layout::signal_emit;

use crate::engine_driveline::SIGNALS;
use crate::{Binding, Ctx, Decline, Regs, Return, SignalCalleesAt};
use rbr_physics::car_body::store::CarStore;

/// `CMultiSignal1<const CSigCheckPointTime&>::Emit` (thiscall, the payload's address on the stack).
const EMIT: u32 = 0x4a0390;
/// `CSignalManager::CMultiSignal1<const CSigCarControl&>::Emit`, the same template.
const CONTROL_EMIT: u32 = 0x48cd90;
/// The signal manager (`[0x893600]`) and the checkpoint signal's offset in it.
const CHECKPOINT_SIGNAL: u32 = 0xc900;

pub static BINDINGS: [Binding; 5] = [
    Binding { addr: 0x4a0540, name: "NRegulation::CTimeKeeper::Record", subsystem: "timing", run: record },
    Binding { addr: 0x4a0320, name: "NRegulation::CTimeKeeper::Update", subsystem: "timing", run: update },
    Binding { addr: 0x4a0290, name: "CTimeKeeper::Diff", subsystem: "timekeeper", run: diff },
    Binding { addr: EMIT, name: "CMultiSignal1<CSigCheckPointTime>::Emit", subsystem: "timing", run: emit_at::<EMIT> },
    Binding {
        addr: CONTROL_EMIT,
        name: "CMultiSignal1<CSigCarControl>::Emit",
        subsystem: "timing",
        run: emit_at::<CONTROL_EMIT>,
    },
];

/// thiscall `(payload*)`, `ret 4`: the `CMultiSignal1::Emit` instance `EMIT` (0x4a0390 or 0x48cd90), the physics core's
/// emit with the connections in the emulator, each entered where the original's `Invoke` call is (the emit's frame: four
/// pushes and the argument below the entry). A count out of range declines (the original traps).
fn emit_at<const EMIT: u32>(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    crate::decline_emit_trap(&ctx.mem, r.ecx)?;
    let mut callees = SignalCalleesAt { host: ctx.host, sp: r.esp - 0x18, name: crate::emit_name(EMIT) };
    signal_emit::emit(&mut ctx.mem, &mut callees, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// thiscall `(current*)`: return `finish - start` from the active time card, in the original x87 double format.
fn diff(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let values = ctx.mem.time_keeper_diff(r.ecx, r.stack[0]);
    Ok(Return::float(values.end - values.origin, 4))
}

/// thiscall `(f, p)`, `ret 8`: `CTimeKeeper::Record`. `p` points at the clock (a copy the caller made), the card and the
/// keeper are read and written back, and a checkpoint is emitted at the frame the original emits from.
fn record(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let card = layout::card(&ctx.mem, r.ecx);
    let x = f32::from_bits(r.stack[0]);
    let clock = layout::f64_at(&ctx.mem, r.stack[1]);
    let before = layout::load_card(&ctx.mem, card);
    let (after, checkpoint) = before.record(x, clock, layout::max_time_step(&ctx.mem));
    let Some(cp) = checkpoint else {
        layout::store_card(&mut ctx.mem, card, &after);
        return Ok(Return::void(0, 8));
    };
    if checked_by_check(&before, &cp) {
        // `Check` writes the start, `flag20` and the emit, then `Record` writes `last` and `cur` (the refresh).
        let mut started = after;
        started.last = before.last;
        started.cur = before.cur;
        layout::store_card(&mut ctx.mem, card, &started);
        layout::set_flag20(&mut ctx.mem, card);
        emit(ctx, r.esp.wrapping_sub(0x4c), r.esp.wrapping_sub(0x3c), &cp);
        layout::store_card(&mut ctx.mem, card, &after);
    } else {
        layout::store_card(&mut ctx.mem, card, &after);
        emit(ctx, r.esp.wrapping_sub(0x24), r.esp.wrapping_sub(0x14), &cp);
    }
    Ok(Return::void(0, 8))
}

/// Whether `Check` (not `Record`'s own start) made the checkpoint: it runs only unheld and unstarted, and then starts
/// with `fired` (the speed or distance start) or at a nonnegative elapsed time (the first branch, `fired` 0).
fn checked_by_check(before: &rbr_gameplay::timing::TimeCard, cp: &Checkpoint) -> bool {
    !before.started && before.hold == 0 && cp.kind == CheckpointKind::Start && (cp.fired || cp.time >= 0.0)
}

/// `CMultiSignal1<const CSigCheckPointTime&>::Emit` (0x4a0390) on the checkpoint signal, as a host call with the call
/// placed at `sp` and the payload at `at` (the original's `leal` frame slot).
fn emit(ctx: &mut Ctx, sp: u32, at: u32, cp: &Checkpoint) {
    for (k, word) in layout::payload(cp).iter().enumerate() {
        ctx.mem.set_u32(at + 4 * k as u32, *word);
    }
    let signal = ctx.mem.u32(SIGNALS) + CHECKPOINT_SIGNAL;
    ctx.host.call_at(sp, EMIT, signal, &[at]);
}

/// thiscall, `ret 0`: `CTimeKeeper::Update`. eax is the count the original returns.
fn update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let before = layout::load_keeper(&ctx.mem, r.ecx);
    let (after, count) = before.update(layout::step(&ctx.mem));
    layout::store_sampling(&mut ctx.mem, r.ecx, &after);
    Ok(Return::void(count, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Host, STACK_ARGS};
    use rbr_physics::mem::Mem;
    use std::cell::RefCell;

    /// The card at 0x1000 (no splits, hold 0, not started, origin `origin`), the keeper at 0x2000 with `last` as the
    /// card's last distance and `sampled` and `still` as its counters, the clock at 0x3000 and the application root
    /// (step 200, `MaxTimeStep` 0.007), the signal manager at 0x7000.
    fn world(origin: f64, clock: f64, last: f32) -> Mem {
        let mut mem = Mem::new();
        mem.map(0x1000, &[0; 0xa0]);
        mem.set_u32(0x2004, 0x1000);
        mem.set_f32(0x1030, 100.0);
        mem.set_f32(0x1048, 400.0);
        put_f64(&mut mem, 0x1038, origin);
        mem.set_f32(0x101c, last);
        mem.set_f32(0x2008, last);
        mem.set_u32(0x200c, 0);
        put_f64(&mut mem, 0x3000, clock);
        mem.set_u32(layout::APP, 0x4000);
        mem.set_u32(0x4020, 0x5000);
        mem.set_u32(0x5018, 200);
        mem.set_u32(0x4024, 0x6000);
        mem.set_f32(0x6000 + 0x1608c, 0.007);
        mem.set_u32(0x893600, 0x7000);
        mem
    }

    fn put_f64(mem: &mut Mem, at: u32, value: f64) {
        mem.set_u32(at, value.to_bits() as u32);
        mem.set_u32(at + 4, (value.to_bits() >> 32) as u32);
    }

    /// A host call: the stack pointer, the callee, `ecx` and the arguments.
    type Logged = (u32, u32, u32, Vec<u32>);

    /// Records the host calls.
    #[derive(Default)]
    struct Calls(RefCell<Vec<Logged>>);

    impl Host for Calls {
        fn call_at(&self, sp: u32, addr: u32, ecx: u32, args: &[u32]) -> crate::Called {
            self.0.borrow_mut().push((sp, addr, ecx, args.to_vec()));
            crate::Called { eax: 0, st0: 0.0 }
        }
    }

    /// The entry registers of a call on `x` with the clock at 0x3000, the keeper at 0x2000 and the return slot at 0x9000.
    fn record_regs(x: f32) -> Regs {
        let mut stack = [0; STACK_ARGS];
        stack[0] = x.to_bits();
        stack[1] = 0x3000;
        Regs { ecx: 0x2000, esp: 0x9000, stack, ..Regs::default() }
    }

    #[test]
    fn record_that_check_starts_emits_from_checks_frame() {
        // Before the start (origin 43200, clock 43185): the distance is past the start line, so Check's second branch.
        let host = Calls::default();
        let mut ctx = Ctx { mem: world(43200.0, 43185.0, 0.0), host: &host };
        record(&mut ctx, &record_regs(150.0)).expect("runs");
        assert_eq!(ctx.mem.u32(0x1040), 1, "started");
        assert_eq!(ctx.mem.u32(0x1020), 1, "flag20 set by Check");
        assert_eq!(ctx.mem.f32(0x101c), 150.0, "last");
        let calls = host.0.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (0x9000 - 0x4c, EMIT, 0x7000 + 0xc900, vec![0x9000 - 0x3c]));
        let payload = (0..5).map(|k| ctx.mem.u32(0x9000 - 0x3c + 4 * k)).collect::<Vec<_>>();
        assert_eq!(payload, [0, 1, (-15.0f32).to_bits(), 0, 1]);
    }

    #[test]
    fn update_returns_the_count_on_a_sampled_step() {
        let host = Calls::default();
        let mut ctx = Ctx { mem: world(43200.0, 43185.0, 150.0), host: &host };
        ctx.mem.set_u32(0x1040, 1);
        let r = update(&mut ctx, &record_regs(0.0)).expect("runs");
        assert_eq!(r.eax, 1);
        assert_eq!(ctx.mem.u32(0x200c), 1);
        assert!(host.0.borrow().is_empty());
    }
}

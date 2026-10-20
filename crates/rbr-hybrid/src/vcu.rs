//! Bindings of the VCU ports (`rbr_physics::vcu`, exe 0x536880-0x5380a0). The ABIs are the original's
//! (`harness/vcu_vectors.py` VCU_SPECS: thiscall `this` in ecx, the bytes `ret` pops). Each binding decodes the
//! registers and runs its entry's sequence (`rbr_physics::vcu::layout`). Accessors and setters on the drive, the
//! engine, the car and the lights stay original: the entries call them through the host (`VcuHost`).

use rbr_physics::vcu::layout::{self, VcuCalls};
use rbr_physics::vcu::store::VcuStore;

use crate::ngp::variant;
use crate::{Binding, Ctx, Decline, Host, Regs, Return};

pub const TABLE_BLEND: u32 = 0x536950;
pub const M6840: u32 = 0x506840;
pub(crate) const SET_DIRTY: u32 = 0x52f450; // NTweak::CBase::SetDirty (the gear block's +0x564)
pub const LOOKUP_PAIR_TABLE: u32 = 0x536880;
pub const LOCK_MAX: u32 = 0x5376e0;
pub const DIFF_LOCKS: u32 = 0x537b70;
pub const REV_LIMIT_CUT: u32 = 0x536ee0;
pub const CHECK_SLIP: u32 = 0x537110;
pub const GEARBOX_UPDATE: u32 = 0x537160;
pub const LAUNCH_CLUTCH: u32 = 0x5375b0;
pub const STEP: u32 = 0x537f50;
pub const STEP_BASIC: u32 = 0x5380a0;
pub const FLAG_DISPATCH: u32 = 0x537a80;
pub const READ_PACKET: u32 = 0x537f20;

/// The original callees the bindings call.
pub const GET_4520: u32 = 0x504520; // CDrive: engine speed (rad/s)
pub const GET_4540: u32 = 0x504540; // CDrive: the diff-lock speed key
pub const GET_15F8: u32 = 0x4e6770; // CCar: car speed
pub const SET_4570: u32 = 0x504570; // CDrive: diff lock (channel, value)
pub const SET_4820: u32 = 0x504820; // CDrive: clutch slip
pub const SET_STATE4: u32 = 0x50d340; // CEngine: the four cut flags
pub const SET_SCRATCH08: u32 = 0x50eb90; // CEngine: pedal
pub const SET_1E40: u32 = 0x4f1e40; // Lights: brake light
pub const SET_1ED0: u32 = 0x4f1ed0; // Lights: neutral light
pub const QUERY_SLIP: u32 = 0x536fe0; // E50_QuerySlip (fastcall on the scratch)

const fn bind(addr: u32, name: &'static str, run: crate::BindingFn) -> Binding {
    Binding { addr, name, subsystem: "vcu", run }
}

pub static BINDINGS: [Binding; 16] = [
    bind(TABLE_BLEND, "E49Proxy::VcuTableBlend", table_blend),
    bind(M6840, "CDrive::M6840", set_commanded_gear),
    bind(LOOKUP_PAIR_TABLE, "NPhysics::LookupPairTable", lookup_pair_table),
    bind(LOCK_MAX, "CScratchSim::LockMax", lock_max),
    bind(DIFF_LOCKS, "CScratchSim::DiffLocks", diff_locks),
    bind(REV_LIMIT_CUT, "CScratchSim::RevLimitCut", rev_limit_cut),
    bind(CHECK_SLIP, "CScratchSim::CheckSlip", check_slip),
    bind(GEARBOX_UPDATE, "CScratchSim::GearboxUpdate", gearbox_update),
    bind(LAUNCH_CLUTCH, "CScratchSim::LaunchClutch", launch_clutch),
    bind(STEP, "CScratchSim::Step", step_full),
    bind(STEP_BASIC, "CScratchSim::StepBasic", step_basic),
    bind(0x536f70, "CScratchSim::NopA", nop_a),
    bind(FLAG_DISPATCH, "CScratchSim::FlagDispatch", flag_dispatch),
    bind(READ_PACKET, "CScratchSim::ReadPacket", read_packet),
    bind(QUERY_SLIP, "CVehicleControlUnit::E50_QuerySlip", query_slip),
    bind(0x537c90, "CScratchSim::CopyDef", copy_def),
];

/// `CScratchSim::NopA(dt)` (0x536f70, thiscall, `ret 4`): does nothing; eax is the caller's.
fn nop_a(_ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(r.eax, 4))
}

/// The original callees of the VCU's entries, through the host. The binding's entry `esp` places the two calls the
/// original makes at a known depth (the slip query and `SetDirty`).
pub(crate) struct VcuHost<'a> {
    pub host: &'a dyn Host,
    pub esp: u32,
}

impl VcuHost<'_> {
    fn get_f32(&self, addr: u32, this: u32) -> f32 {
        self.host.call(addr, this, &[]).st0 as f32
    }
}

impl VcuCalls for VcuHost<'_> {
    fn engine_speed(&mut self, drive: u32) -> f32 {
        self.get_f32(GET_4520, drive)
    }

    fn lock_speed(&mut self, drive: u32) -> f32 {
        self.get_f32(GET_4540, drive)
    }

    fn car_speed(&mut self, car: u32) -> f32 {
        self.get_f32(GET_15F8, car)
    }

    fn set_diff_lock(&mut self, drive: u32, channel: u32, value: f32) -> u32 {
        self.host.call(SET_4570, drive, &[channel, value.to_bits()]).eax
    }

    fn set_clutch_slip(&mut self, drive: u32, value: f32) -> u32 {
        self.host.call(SET_4820, drive, &[value.to_bits()]).eax
    }

    fn set_cut_flags(&mut self, engine: u32, flags: [u32; 4]) -> u32 {
        self.host.call(SET_STATE4, engine, &flags).eax
    }

    fn set_engine_pedal(&mut self, engine: u32, value: f32) {
        self.host.call(SET_SCRATCH08, engine, &[value.to_bits()]);
    }

    fn set_brake_light(&mut self, lights: u32, on: bool) {
        self.host.call(SET_1E40, lights, &[on as u32]);
    }

    fn set_neutral_light(&mut self, lights: u32, on: bool) {
        self.host.call(SET_1ED0, lights, &[on as u32]);
    }

    /// In the original at FlagDispatch's esp: three pushes and the return slot below the entry.
    fn query_slip(&mut self, scratch: u32) -> bool {
        self.host.call_at(self.esp - 0x10, QUERY_SLIP, scratch, &[]).eax != 0
    }

    /// Entered two words below `M6840`'s entry.
    fn set_dirty(&mut self, object: u32) {
        self.host.call_at(self.esp - 12, SET_DIRTY, object, &[]);
    }
}

fn calls<'a>(host: &'a dyn Host, r: &Regs) -> VcuHost<'a> {
    VcuHost { host, esp: r.esp }
}

fn f(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// stdcall `(table, x, lo, hi, n)`, st0. The VCU always passes 11 knots; the port's index math is checked for
/// counts from 2.
fn table_blend(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [table, x, lo, hi, n, ..] = r.stack;
    if !(2..=64).contains(&n) {
        return Err(Decline::Unsupported("knot count outside 2..=64"));
    }
    Ok(Return::float(layout::table_blend(&ctx.mem, table, f(x), f(lo), f(hi), n, variant()), 0x14))
}

/// stdcall `(table, x, n)`, st0.
fn lookup_pair_table(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [table, x, n, ..] = r.stack;
    if !(1..=64).contains(&n) {
        return Err(Decline::Unsupported("record count outside 1..=64"));
    }
    Ok(Return::float(layout::lookup_pair_table(&ctx.mem, table, f(x), n, variant()), 0xc))
}

/// thiscall `(t1, t2, t3, f)`, st0: the largest of the throttle, brake and speed maps.
fn lock_max(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [t1, t2, t3, x, ..] = r.stack;
    Ok(Return::float(layout::lock_max(&ctx.mem, r.ecx, [t1, t2, t3], f(x), variant()) as f64, 0x10))
}

/// thiscall `()`: the three diff locks, sent to the drive's `Set4570`.
fn diff_locks(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let eax = layout::diff_locks(&ctx.mem, &mut calls(ctx.host, r), r.ecx, variant());
    Ok(Return::void(eax, 0))
}

/// fastcall `()`, `ret 0`: `E50_QuerySlip` (0x536fe0), the slip query before a downshift.
fn query_slip(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let on = layout::query_slip(&ctx.mem, &mut calls(ctx.host, r), r.ecx);
    Ok(Return::void(on as u32, 0))
}

/// thiscall `()`: the rev-limit cut flags, sent to the engine's `SetState4`.
fn rev_limit_cut(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let margin = crate::ngp::vcu_tuning(ctx).cut_margin;
    let eax = layout::rev_limit_cut(&ctx.mem, &mut calls(ctx.host, r), r.ecx, margin);
    Ok(Return::void(eax, 0))
}

/// thiscall `(x)`, eax: the slip gate.
fn check_slip(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(layout::check_slip(&ctx.mem, r.ecx, f(r.stack[0])) as u32, 4))
}

/// thiscall `(dt)`: the gear logic. eax is 0 only on the flag-off return; Step, the one caller, reloads it at once, so
/// the other exits' residue is not modelled.
fn gearbox_update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let tuning = crate::ngp::vcu_tuning(ctx);
    layout::gearbox_update(&mut ctx.mem, &mut calls(ctx.host, r), r.ecx, f(r.stack[0]), tuning);
    Ok(Return::void(0, 4))
}

/// thiscall `(dt)`: the launch clutch, sent to the drive's `Set4820` (nothing with the launch mode off).
fn launch_clutch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let tuning = crate::ngp::vcu_tuning(ctx);
    let eax = layout::launch_clutch(&mut ctx.mem, &mut calls(ctx.host, r), r.ecx, f(r.stack[0]), tuning);
    Ok(Return::void(eax, 4))
}

/// thiscall `(packet)`, `ret 4`: the packet's flags as gear requests. eax is whether anything was requested.
fn flag_dispatch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let packet = ctx.mem.packet(r.stack[0]);
    let tuning = crate::ngp::vcu_tuning(ctx);
    let on = layout::apply_packet(&mut ctx.mem, &mut calls(ctx.host, r), r.ecx, packet, &tuning);
    Ok(Return::void(on as u32, 4))
}

/// thiscall `(packet)`, `ret 4`: copies the packet into the scratch, then tail-jumps to FlagDispatch with the stack
/// as it entered (the asm stores the packet pointer back into its own argument slot, the same word).
fn read_packet(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let packet = layout::read_packet(&mut ctx.mem, r.ecx, r.stack[0]);
    let tuning = crate::ngp::vcu_tuning(ctx);
    let on = layout::apply_packet(&mut ctx.mem, &mut calls(ctx.host, r), r.ecx, packet, &tuning);
    Ok(Return::void(on as u32, 4))
}

/// thiscall `(value)`, `ret 4`: `CDrive::M6840` (0x506840).
fn set_commanded_gear(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    layout::set_commanded_gear(&mut ctx.mem, &mut calls(ctx.host, r), r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

fn step_full(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    run_step(ctx, r, true)
}

fn step_basic(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    run_step(ctx, r, false)
}

/// thiscall `(dt)`: Step (`gear`, with the gear logic) or StepBasic; eax is the state pointer.
fn run_step(ctx: &mut Ctx, r: &Regs, gear: bool) -> Result<Return, Decline> {
    let tuning = crate::ngp::vcu_tuning(ctx);
    let eax = layout::step(&mut ctx.mem, &mut calls(ctx.host, r), r.ecx, f(r.stack[0]), gear, tuning, variant());
    Ok(Return::void(eax, 4))
}

/// thiscall `(setup)`, `ret 4`: `CScratchSim::CopyDef` (0x537c90), the VCU setup into the scratch's definition
/// (`rbr_physics::vcu::layout::copy_def`).
fn copy_def(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    rbr_physics::vcu::layout::copy_def(&mut ctx.mem, r.ecx, r.stack[0]);
    Ok(Return::void(0, 4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Script;
    use rbr_physics::mem::Mem;

    fn regs(ecx: u32, stack: &[u32]) -> Regs {
        let mut r = Regs { ecx, ..Regs::default() };
        r.stack[..stack.len()].copy_from_slice(stack);
        r
    }

    /// Eleven knots at `at`, 0x10 apart, value at +0x0c: knot k is k * 0.25.
    fn knots(mem: &mut Mem, at: u32) {
        for k in 0..11 {
            mem.map(at + 0x10 * k, &[0; 0x10]);
            mem.set_f32(at + 0x10 * k + 0x0c, k as f32 * 0.25);
        }
    }

    #[test]
    fn table_blend_runs_on_game_memory() {
        let host = Script::default();
        let mut ctx = Ctx { mem: Mem::new(), host: &host };
        knots(&mut ctx.mem, 0x1000);
        let r = regs(0, &[0x1000, 0.25f32.to_bits(), 0, 1f32.to_bits(), 11]);
        assert_eq!(table_blend(&mut ctx, &r), Ok(Return::float(0.625, 0x14)));
        let r = regs(0, &[0x1000, 0, 0, 1f32.to_bits(), 0]);
        assert!(table_blend(&mut ctx, &r).is_err());
    }

    #[test]
    fn gearbox_update_requests_an_upshift_above_the_gears_rpm() {
        // Launch flag on, VCU off: only the rpm rules run. Gear 2 at 500 rad/s is above its 3000 rpm threshold.
        let (this, def, state, car, drive, drive_def, gears) = (0x1000, 0x2000, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000);
        let host = Script { floats: vec![(GET_4520, drive, 500.0), (GET_15F8, car, 10.0)], ..Script::default() };
        let mut ctx = Ctx { mem: Mem::new(), host: &host };
        ctx.mem.map(this, &[0; 0x60]);
        ctx.mem.set_u32(this, def);
        ctx.mem.set_u32(this + 4, state);
        ctx.mem.set_u32(this + 0x40, 1);
        ctx.mem.set_u32(this + 0x58, drive);
        ctx.mem.map(def, &[0; 0xcac]);
        ctx.mem.set_f32(def + 0x18 + 0x20 * 2, 3000.0);
        ctx.mem.set_f32(def + 0x158, 0.25);
        ctx.mem.map(state, &[0; 0x6c]);
        ctx.mem.set_u32(state + 0x68, car);
        ctx.mem.map(drive, &[0; 0x10]);
        ctx.mem.set_u32(drive + 4, drive_def);
        ctx.mem.set_u32(drive + 8, gears);
        ctx.mem.map(drive_def, &[0; 0x658]);
        ctx.mem.set_u32(drive_def + 0x654, 6);
        ctx.mem.map(gears, &[0; 0x574]);
        ctx.mem.set_u32(gears + 0x54c, 2);
        ctx.mem.set_u32(gears + 0x570, 2);
        assert_eq!(gearbox_update(&mut ctx, &regs(this, &[0.01f32.to_bits()])), Ok(Return::void(0, 4)));
        let got = ctx.mem.gearbox(gears);
        assert_eq!((got.gear, got.commanded, got.changed), (2, 3, 1));
        let got = ctx.mem.state(state);
        assert_eq!((got.shift_lockout, got.shift_cooldown), (0.25, 0.05));
        assert_eq!(*host.log.borrow(), vec![(GET_4520, drive, vec![]), (GET_15F8, car, vec![])]);
    }

    #[test]
    fn rev_limit_cut_sends_the_flags_to_the_engine() {
        // Engine speed 0: below the limit, no cooldown -> no cut.
        let host = Script::default();
        let mut ctx = Ctx { mem: Mem::new(), host: &host };
        let (this, def, state, engine, drive) = (0x1000, 0x2000, 0x4000, 0x5000, 0x6000);
        ctx.mem.map(this, &[0; 0x60]);
        ctx.mem.set_u32(this, def);
        ctx.mem.set_u32(this + 4, state);
        ctx.mem.set_u32(this + 0x54, engine);
        ctx.mem.set_u32(this + 0x58, drive);
        ctx.mem.map(def, &[0; 0xcac]);
        ctx.mem.set_f32(def + 0x188, 7000.0);
        ctx.mem.map(state, &[0; 0x68]);
        let ret = rev_limit_cut(&mut ctx, &regs(this, &[])).unwrap();
        assert_eq!(ret, Return::void(0, 0));
        let log = host.log.borrow();
        assert_eq!(log[0], (GET_4520, drive, vec![]));
        assert_eq!(log[1].0, SET_STATE4);
        assert_eq!(log[1].1, engine);
        let want = rbr_physics::vcu::cut::rev_limit_cut(0.0, &ctx.mem.def(def), &ctx.mem.state(state), 300.0);
        assert_eq!(log[1].2, want);
    }
}

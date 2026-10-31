//! NGP's wrappers of the physics step's functions.

use rbr_physics::engine_driveline::layout::{with_worker, DriveWorker, HookBlock};
use rbr_physics::mem::Mem;
use rbr_physics::ngp::atmosphere::atmosphere;
use rbr_physics::ngp::axle;
use rbr_physics::ngp::brake::{brake_step, Brake, BrakeIn};
use rbr_physics::ngp::diff;
use rbr_physics::ngp::downforce::{downforce, DownforceIn, Path};
use rbr_physics::ngp::drive;
use rbr_physics::ngp::layout::{drive_line, material, tag_blend};
use rbr_physics::ngp::outputs;
use rbr_physics::ngp::step_wrapper::{self, SurfaceHeatPercent};
use rbr_physics::tyre::store::TyreStore;

use crate::{Ctx, Decline, Regs, Return};

use super::address::{at, FILE_BASE};
use super::{data_f32, f, SETTINGS, SPLIT};

/// The words `0x1004b5a0` stores, in [`rbr_physics::ngp::atmosphere::Atmosphere`]'s order.
const ATMOSPHERE_OUT: [u32; 6] = [0x1017_1adc, 0x1017_1ad8, 0x1017_1ae0, 0x1017_67c8, 0x1017_67cc, 0x1017_67d0];

/// `0x1004b5a0` (cdecl, the car): the atmosphere at the car's height (`+0x148`), into NGP's `.data`. Leaves the car
/// in eax (its first load).
pub(crate) fn atmosphere_update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let car = r.stack[0];
    apply_atmosphere(&mut ctx.mem, car);
    Ok(Return::void(car, 0))
}

/// `0x1004b5a0`'s work for `car`.
pub(crate) fn apply_atmosphere(m: &mut Mem, car: u32) {
    let constants = rbr_physics::ngp::layout::atmosphere::constants(m, at(FILE_BASE).expect("NGP atmosphere"));
    let a = atmosphere(m.f32(car + 0x148), &constants);
    let values = [a.temperature, a.pressure, a.density, a.inv_pressure, a.pressure_115, a.pressure_0987];
    for (addr, v) in ATMOSPHERE_OUT.into_iter().zip(values) {
        m.set_f32(at(addr).expect("hooked only with NGP"), v);
    }
}

/// `0x10047040` (cdecl, `p`; the car is `*(*(p+8)+0x580)`): the downforce into the car's sums, and the drag factor.
/// `eax` is what the original's path leaves: 8 (no downforce), the speed's direction (0, 1 or -1, no speed) or the
/// wheel at `car+0x474` (applied).
pub(crate) fn downforce_update(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    Ok(Return::void(apply_downforce(&mut ctx.mem, r.stack[0]), 0))
}

/// `0x10047040`'s work for `p`; returns its eax.
fn apply_downforce(m: &mut Mem, p: u32) -> u32 {
    let car = m.u32(m.u32(p + 8) + 0x580);
    let def = m.u32(car + 0x34);
    let dmg = m.u32(car + 0x9f4);
    let i = DownforceIn {
        damage: (dmg != 0).then(|| [m.u32(dmg + 0x3db4), m.u32(dmg + 0x3dbc)]),
        front: m.f32(def + 0x1fc),
        rear: m.f32(def + 0x20c),
        density: m.f32(at(0x1017_1ae0).expect("hooked only with NGP")),
        speed: m.f32(car + 0x15f8),
        wheel_arms: [0x470, 0x474, 0x478, 0x47c].map(|o| m.f32(m.u32(car + o) + 0x8b4)),
        force_sum: m.f32(car + 0x378),
        moment_sum: m.f32(car + 0x310),
    };
    let o = downforce(&i);
    m.set_f32(car + 0x4a4, o.speed_ratio_sq);
    m.set_f32(car + 0x4a8, o.front_force);
    m.set_f32(car + 0x4ac, o.rear_force);
    m.set_f32(car + 0x378, o.force_sum);
    m.set_f32(car + 0x310, o.moment_sum);
    m.set_f32(at(0x1017_147c).expect("hooked only with NGP"), o.drag_factor);
    match o.path {
        Path::NoDownforce => 8,
        Path::NoSpeed(direction) => direction as u32,
        Path::Applied => m.u32(car + 0x474),
    }
}

/// `0x10048d50` (thiscall on the drive, `dt`, `ret 4`): NGP's wrapper of `M9BC0` (`rbr_physics::ngp::diff`). The
/// downforce and the gain's loss, the diffs' lock torques for NGP's drivetrain types, then the wheel worker: NGP's axle
/// worker `0x100487f0` for types 4 and 5 (ported), else `M9BC0` in the original through NGP's trampoline `0x10045df0`.
pub(crate) fn drive_wrapper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let drive = r.ecx;
    let m = &ctx.mem;
    let (def, state, obj) = (m.u32(drive + 4), m.u32(drive + 8), m.u32(drive + 0x170));
    let word = m.u32(def + 0x640);
    let kind = diff::drive_type(word);
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    if settings == 0 {
        return Err(Decline::Unsupported("NGP's settings object is not made yet"));
    }
    if (4..8).contains(&kind) && (0..4).any(|k| m.u32(def + 0x158 + 0xf0 * k) > 3) {
        return Err(Decline::Unsupported("a wheel's pack index is past the axles"));
    }
    if matches!(kind, 4 | 5) && !axle_fits(m, def, kind - 3) {
        return Err(Decline::Unsupported("the axle's blocks or packs are out of range"));
    }
    apply_downforce(&mut ctx.mem, drive);
    let loss = ctx.mem.u32(settings + 0xe88);
    let gain = diff::after_loss(ctx.mem.f32(state + 0x37c), loss);
    ctx.mem.set_f32(state + 0x37c, gain);
    if (4..8).contains(&kind) {
        let m = &ctx.mem;
        let car = m.u32(state + 0x580);
        let locks = m.u32(car + 0x850);
        let tuning = diff::DiffTuning {
            locks: [[0x1b8, 0x268], [0x738, 0x7e8], [0x9f8, 0xaa8]].map(|p| p.map(|o| m.f32(locks + o))),
            handbrake: m.f32(car + 0x864),
            handbrake_release: m.f32(locks + 0x198),
            max_torque: [0, 1, 2].map(|g| m.f32(def + 0x58 + 0x60 * g)),
            enabled: [4, 0xc, 0x14].map(|o| m.u32(obj + o)),
            split: data_f32(ctx, 0x1017_1a90),
            viscous_gain: data_f32(ctx, 0x1017_1aa0),
        };
        let w = DriveWorker::load(m, drive);
        let scales = diff::lock_torques(&w, diff::diff_types(word), &tuning);
        for (g, v) in scales.into_iter().enumerate() {
            ctx.mem.set_f32(state + 0x18 + 0x14 * g as u32, v);
        }
    }
    let dt = r.stack[0];
    if matches!(kind, 4 | 5) {
        let g = (kind - 3) as usize;
        with_worker(&mut ctx.mem, drive, |w| axle::axle_worker(w, f(dt), g));
        return Ok(Return::void(0, 4));
    }
    let called = ctx.host.call(at(WHEEL_WORKER).unwrap(), drive, &[dt]);
    Ok(Return::void(called.eax, 4))
}

/// Whether `0x100487f0` on axle `g` stays in the drive's records: its two blocks are wheels and their packs index
/// the active flags (at most 2).
fn axle_fits(m: &Mem, def: u32, g: u32) -> bool {
    [0x28, 0x38].into_iter().all(|o| {
        let k = m.u32(def + 0x60 * g + o);
        k < 4 && m.u32(def + 0x158 + 0xf0 * k) <= 2
    })
}

/// `0x100487f0` (thiscall on the drive, `(dt, g)`, `ret 8`, void): NGP's axle worker (`rbr_physics::ngp::axle`).
pub(crate) fn axle_worker(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [dt, g, ..] = r.stack;
    let def = ctx.mem.u32(r.ecx + 4);
    if !matches!(g, 1 | 2) || !axle_fits(&ctx.mem, def, g) {
        return Err(Decline::Unsupported("the axle's blocks or packs are out of range"));
    }
    with_worker(&mut ctx.mem, r.ecx, |w| axle::axle_worker(w, f(dt), g as usize));
    Ok(Return::void(0, 8))
}

/// `0x10015fc0` (thiscall on the drive line, `(pos, x)`, `ret 8`, st0): NGP's `SampleBig` in doubles
/// (`rbr_physics::ngp::drive_line`).
pub(crate) fn drive_line_sample(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let v = drive_line::sample(&ctx.mem, r.ecx, r.stack[0], f(r.stack[1]));
    Ok(Return::float(f64::from(v), 8))
}

/// NGP's rewrite of `E39_Hook4160`, which `Step1E0` calls at `0x50a2a8`.
pub(crate) const CURVE_TERM: u32 = 0x1004_6880;

/// `0x10046880` (thiscall on the drive, `(a1, c, n, x, dt)`, `ret 0x14`, st0): NGP's curve term
/// (`rbr_physics::ngp::brake`), with 1.02's `Worker4060` ported.
pub(crate) fn curve_term_binding(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a1, c, n, x, dt, ..] = r.stack;
    let v = curve_term(&mut ctx.mem, r.ecx, [a1, c, n, x, dt])?;
    Ok(Return::float(f64::from(v), 0x14))
}

/// `0x10046880`'s work; declines (before any writes) without NGP's settings or for a state that isn't a wheel's.
pub(crate) fn curve_term(m: &mut Mem, drive: u32, args: [u32; 5]) -> Result<f32, Decline> {
    let [a1, c, n, x, dt] = args;
    let state = m.u32(drive + 8);
    let car = m.u32(state + 0x580);
    let u = a1.wrapping_sub(state).wrapping_sub(0x48) / 200;
    if u > 3 {
        return Err(Decline::Unsupported("the curve state is not a wheel's"));
    }
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    if settings == 0 {
        return Err(Decline::Unsupported("NGP's settings object is not made yet"));
    }
    let front = u < 2;
    let rear_split =
        diff::diff_types(m.u32(n + 0x640))[0] == 3 && !front && m.f32(m.u32(car + 0x850) + 0x198) >= m.f32(car + 0x864);
    // The hook block's words are NGP's per-wheel brake keys (`Brake*_NGP`): ratio +0xec, piston area +0xcc, effective
    // radius +0x6c, plate area +0x9c, internal convection +0xbc, heat absorption +0x8c, specific heat +0x7c, mass
    // +0x3c, convection factor +0x4c, layer mass share +0x5c, vent area +0xac.
    let block = HookBlock::load(m, c);
    let i = BrakeIn {
        brake: Brake {
            ratio: block.e_scale,
            piston_area: block.dt_scale,
            effective_radius: block.gain,
            plate_area: block.p_scale,
            internal_convection: block.x1_scale,
            heat_absorption: block.w_scale,
            specific_heat: block.den_a,
            mass: block.den_b,
            convection_factor: m.f32(c + 0x4c),
            layer_mass_share: m.f32(c + 0x5c),
            vent_area: m.f32(c + 0xac),
        },
        wheel_speed: m.f32(a1 + 0x1c),
        layer_temperature: m.u32(a1 + 0x7c),
        core_temperature: m.u32(a1 + 0x8c),
        pedal_pressure: m.f32(x + 0x60),
        handbrake_pressure: m.f32(x + 0x70),
        handbrake_gain: m.f32(a1 + 0xac),
        handbrake_factor: m.f32(at(0x1017_1a94).unwrap()),
        released: front && m.u32(m.u32(drive + 0x170) + 0x80) == 0,
        speed_fade: m.f32(settings + if front { 0xfe8 } else { 0x1168 }),
        car_speed: m.f32(car + 0x15f8),
        air_temperature: m.f32(at(0x1017_1adc).unwrap()),
        dt: f(dt),
        split_handbrake: rear_split,
    };
    let o = brake_step(&i, |force| crate::engine_driveline::row_value_at(m, force, a1, c, n, f(dt)) as f32);
    m.set_u32(a1 + 0x7c, o.layer_temperature);
    m.set_u32(a1 + 0x8c, o.core_temperature);
    let row = at(0x1017_662c).unwrap() + 0x3c * u;
    for (k, v) in o.telemetry.into_iter().enumerate() {
        m.set_f32(row + 4 * k as u32, v);
    }
    if let Some(t) = o.transfer {
        let slot = state + 200 * u - 0x98;
        m.set_f32(slot, m.f32(slot) + t);
    }
    Ok(o.torque)
}

const WHEEL_WORKER: u32 = 0x1004_5df0;

/// `0x1005b660` (thiscall on the scratch, 16 words, `ret 0x40`): NGP's wrapper of `CScratch::Step`
/// (`rbr_physics::ngp::step_wrapper`) around the ported `Step`.
pub(crate) fn step_wrapper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let m = &ctx.mem;
    let settings = m.u32(m.u32(at(SETTINGS).expect("hooked only with NGP")));
    if settings == 0 {
        return Err(Decline::Unsupported("NGP's settings object is not made yet"));
    }
    let def = m.u32(r.ecx + 0x10);
    let saved: Vec<u32> = (0x204..0x284).step_by(4).map(|o| m.u32(def + o)).collect();
    let p = r.stack[4];
    let k = step_wrapper::speed_factor(m.f32(r.ecx + 0x360));
    let percent = SurfaceHeatPercent {
        class_2: m.u32(settings + 0xe98),
        class_1: m.u32(settings + 0xea8),
        class_3: m.u32(settings + 0xeb8),
    };
    let scale = step_wrapper::surface_scale(m.u32(p + 0x148), m.u32(def + 0xc), percent);
    let words = [0x48, 0x58, 0x78].map(|o| m.u32(p + o));
    for o in [0x48, 0x58, 0x78] {
        ctx.mem.set_f32(p + o, k * ctx.mem.f32(p + o));
    }
    if let Some(s) = scale {
        for o in (0x204..0x244).step_by(4) {
            ctx.mem.set_f32(def + o, ctx.mem.f32(def + o) * s);
        }
    }
    let ret = crate::tyre::step(ctx, r);
    for (o, w) in (0x204..0x284).step_by(4).zip(saved) {
        ctx.mem.set_u32(def + o, w);
    }
    if ret.is_err() {
        // `Step` declined before writing: undo the speed factor too, so the original runs on the entry state.
        for (o, w) in [0x48, 0x58, 0x78].into_iter().zip(words) {
            ctx.mem.set_u32(p + o, w);
        }
    }
    ret
}

/// `0x1005b4a0` (thiscall, `CScratch::ComputeOutputs`'s 17 stack words): the 1.02 function through NGP's call of the
/// original, then its edits of the lateral and longitudinal end slips.
pub(crate) fn compute_outputs(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let s = &r.stack;
    let input = ctx.mem.load_friction_limits(r.ecx, s[6], [f(s[1]), f(s[2])], [f(s[4]), f(s[5])]);
    let out = outputs::friction_limits(&input);
    ctx.mem.store_friction_limits(std::array::from_fn(|i| s[7 + i]), &out);
    Ok(Return::void(0, 0x44))
}

/// `0x1005e8a0` (stdcall `(blk)`): NGP's rewrite of `M3D230`. Its eax (the CRT's leftovers) isn't modelled.
pub(crate) fn tag_blend(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    tag_blend::update(&mut ctx.mem, r.stack[0]);
    Ok(Return::void(0, 4))
}

/// `0x100458c0` (thiscall `(x)`): NGP's `M5FE0`.
pub(crate) fn wheel_slip_value(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let split = data_f32(ctx, SPLIT);
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| drive::wheel_slip_value(w, x, split));
    Ok(Return::void(0, 4))
}

/// `0x10045b80` (thiscall `(x)`): NGP's `M6260`.
pub(crate) fn wheel_latch(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let split = data_f32(ctx, SPLIT);
    let x = r.stack[0] as usize;
    with_worker(&mut ctx.mem, r.ecx, |w| drive::wheel_latch(w, x, split));
    Ok(Return::void(0, 4))
}

/// `0x1004df40` (thiscall `(a, b)` on the slot): NGP's 16 by 16 sheet lookup.
pub(crate) fn sample_slot(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let [a, b, ..] = r.stack;
    Ok(Return::void(material::sample_slot(&ctx.mem, r.ecx, f(a), f(b)), 8))
}

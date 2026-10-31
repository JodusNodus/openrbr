//! NGP's starter and engine wrapper.

use rbr_physics::engine_driveline::layout::with_worker;
use rbr_physics::mem::Mem;
use rbr_physics::ngp::engine::{self as ngp_engine, EngineDamage, TorqueIn};
use rbr_physics::ngp::starter;

use crate::{Ctx, Decline, Regs, Return};

use super::address::{at, WALL_CLOCK};
use super::f;
use super::steps::apply_atmosphere;

/// NGP's starter, the rewrite of `CDrive::M6C90` that `Step1E0` calls at `0x50a25c`.
pub(crate) const STARTER: u32 = 0x1004_56c0;

/// NGP's wall-clock stamp of the starter (us, eight bytes); timed by simulation time, the port keeps its hold there.
const STARTER_HELD: u32 = 0x1017_6718;

/// `0x100456c0` (thiscall on the drive, `(dt)`, `ret 4`, st0): NGP's starter (`rbr_physics::ngp::starter`).
pub(crate) fn starter_binding(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let v = starter_step(&mut ctx.mem, r.ecx, f(r.stack[0]));
    Ok(Return::float(f64::from(v), 4))
}

/// `0x100456c0`'s work, timed by simulation time (the hold kept at NGP's stamp, `notes/ngp.md`) or by the wall clock
/// set for the step (the stamp taken while the gate is clear, as NGP does).
pub(crate) fn starter_step(m: &mut Mem, drive: u32, dt: f32) -> f32 {
    let held_at = at(STARTER_HELD).expect("hooked only with NGP");
    let kept = u64::from(m.u32(held_at)) | u64::from(m.u32(held_at + 4)) << 32;
    let band = m.f32(at(0x1017_1a98).unwrap());
    let (v, kept) = with_worker(m, drive, |w| {
        let gated = w.this.lane_gate != 0;
        let (held, kept) = match WALL_CLOCK.get() {
            Some(now) if gated => (now.wrapping_sub(kept), kept),
            Some(now) => (0, now),
            None => {
                let held = starter::sim_held(gated, dt, kept);
                (held, held)
            }
        };
        (starter::starter(w, dt, band, held), kept)
    });
    m.set_u32(held_at, kept as u32);
    m.set_u32(held_at + 4, (kept >> 32) as u32);
    v
}

/// NGP's engine wrapper, the target of `CCar::Fwd18E20`'s `jmp`.
pub(crate) const ENGINE_WRAPPER: u32 = 0x1004_b8e0;

/// NGP's speed ring (an MSVC `std::deque<float>`: proxy, map, map size, offset, size) and idle table.
const SPEED_RING: u32 = 0x1017_67b0;

const IDLE_TABLE: u32 = 0x1017_67e8;

/// The idle table's lookup (slot 5 of its vtable).
const TABLE_LOOKUP: u32 = 0x1004_a700;

/// NGP's own CRT `operator new` (cdecl: size).
pub(crate) const OPERATOR_NEW: u32 = 0x1011_fe32;

/// `0x1004b8e0` (thiscall on the engine, `(dt, speed)`, `ret 8`, st0): NGP's engine wrapper
/// (`rbr_physics::ngp::engine`) around the ported `Sim8E20`. When the engine dies it emits the engine fire through
/// NGP's emit wrapper ([`engine_dies`]). Declines (before any writes) when the speed ring would grow its map, or for
/// an idle table of another class: neither occurs (`notes/ngp.md`, "Unreachable guards"). A block the ring's new entry
/// needs is allocated first, through NGP's `operator new`, as `push_back` does.
pub(crate) fn engine_wrapper(ctx: &mut Ctx, r: &Regs) -> Result<Return, Decline> {
    let engine = r.ecx;
    let [dt, speed, ..] = r.stack.map(f);
    let m = &ctx.mem;
    let car = m.u32(engine + 0x44);
    let health_at = m.u32(engine + 0x48);
    let rev_limit = ngp_engine::rev_limit(m.f32(m.u32(car + 0x850) + 0x188));
    let was_alive = m.u32(health_at + 0x74) != 0;
    let health = ngp_engine::worn_health(m.f32(health_at + 0x58), speed, rev_limit, dt);
    let dies = was_alive && ngp_engine::is_fatal(health);
    let alive = was_alive && !dies;
    let modes = m.u32(at(0x1017_67d4).unwrap());
    let table = m.u32(at(IDLE_TABLE).unwrap());
    let ring = at(SPEED_RING).unwrap();
    if alive && modes & ngp_engine::IDLE_MODEL_MODE != 0 && table != 0 {
        if m.u32(m.u32(table) + 0x14) != at(TABLE_LOOKUP).unwrap() {
            return Err(Decline::Unsupported("NGP's idle table is of another class"));
        }
        let Some((.., block_at)) = ring_slot(m, ring).filter(|_| m.u32(m.u32(ring)) == ring) else {
            return Err(Decline::Unsupported("NGP's speed ring would grow its map"));
        };
        if m.u32(block_at) == 0 {
            let block = ctx.host.call(at(OPERATOR_NEW).unwrap(), 0, &[16]).eax;
            ctx.mem.set_u32(block_at, block);
        }
    }
    if was_alive {
        ctx.mem.set_f32(health_at + 0x58, health);
    }
    if dies {
        engine_dies(ctx, r.esp, health_at);
    }
    apply_atmosphere(&mut ctx.mem, car);
    let sim = crate::engine_driveline::engine_step(ctx, r)?;
    let r_value = sim.st0.expect("Sim8E20 returns st0") as f32;
    if !alive || !crate::engine_driveline::engine_running(ctx, engine) {
        return Ok(Return::float(ngp_engine::output(r_value), 8));
    }
    let m = &ctx.mem;
    let (def, e8) = (m.u32(engine + 4), m.u32(engine + 8));
    let i = TorqueIn {
        speed,
        rev_limit,
        throttle: m.f32(engine + 0x20),
        shift_cooldown: m.f32(m.u32(car + 0x854) + 0x64),
        shift_idle_scale: m.f32(def + 0x283c),
        stage_enabled: m.u32(def + 0x26f8) != 0,
        modes,
        has_idle_table: table != 0,
        braking: m.f32(at(0x1017_234c).unwrap()),
        max_torque: m.f32(at(0x1017_1aa0).unwrap()),
        stall_speed: m.f32(at(0x1017_1a9c).unwrap()),
        idle_band: m.f32(at(0x1017_1a98).unwrap()),
        density: m.f32(at(0x1017_1ae0).unwrap()),
        friction_torque: m.f32(e8 + 0x270),
        damage: EngineDamage {
            parts: [0x8, 0x18, 0x28, 0x38].map(|o| m.f32(health_at + o)),
            damage_a: m.f32(health_at + 0x54),
            health: m.f32(health_at + 0x58),
            condition: m.f32(health_at + 0x5c),
            damage_b: m.f32(health_at + 0x60),
        },
    };
    // The table's two vectors (begin, end), read at knot slots as the lookup asks (`ngp_engine::idle_table_lookup`).
    let [xs, end, ys] = if table != 0 { [8, 0xc, 0x14].map(|o| m.u32(table + o)) } else { [0; 3] };
    let n = (end.wrapping_sub(xs) / 8) as usize;
    let table_mem = m.clone();
    let double = move |at: u32| f64::from_bits(u64::from(table_mem.u32(at)) | u64::from(table_mem.u32(at + 4)) << 32);
    let slot = |base: u32, k: isize| base.wrapping_add((8 * k) as u32);
    let mem = &mut ctx.mem;
    let v = ngp_engine::torque(
        &i,
        r_value,
        || ring_shift(mem, ring, speed, i.stall_speed),
        |x| ngp_engine::idle_table_lookup(n, |k| double(slot(xs, k)), |k| double(slot(ys, k)), x) as f32,
    );
    Ok(Return::float(ngp_engine::output(v), 8))
}

/// The game's signal hub as NGP's getter `0x1002cc00` gives it (`*(*101760d4)`, `*0x893600`), its engine-fire signal
/// (`CMultiSignal1<CSigEngineFire>`, the one `E36DamageOps::EngineGate` emits), and NGP's emit wrapper of that signal.
const SIGNALS: u32 = 0x1017_60d4;

const ENGINE_FIRE_SIGNAL: u32 = 0xb840;

const ENGINE_FIRE_EMIT: u32 = 0x100e_df40;

/// `0x1004b9b3..0x1004ba5c`, the engine's death in the wrapper whose entry `esp` is `esp`: the health object's alive flag
/// (`+0x74`) clears, and NGP emits the engine fire with payload 1.0 (the particles' and the car sound's fire, through
/// their `CSigEngineFire` connections). NGP's code is the emit wrapper's inline (the post on its strand when its
/// singleton's `+0xd` is set, else the fan-out), so the host runs that wrapper: the original's own code in the
/// emulator, and in the pure host the 1.02 emit it stands for (`original_for`), which the host shows as an engine fire.
/// The payload sits where the original keeps it (its `ebp - 0x10`).
fn engine_dies(ctx: &mut Ctx, esp: u32, health_at: u32) {
    ctx.mem.set_u32(health_at + 0x74, 0);
    let payload = esp - 0x14;
    ctx.mem.set_f32(payload, 1.0);
    let signal = ctx.mem.u32(ctx.mem.u32(at(SIGNALS).unwrap())) + ENGINE_FIRE_SIGNAL;
    ctx.host.call(at(ENGINE_FIRE_EMIT).unwrap(), signal, &[payload]);
}

/// The ring's offset and size after dropping its oldest entry, the new entry's index in its block and the map slot of
/// that block (`None` when `push_back` would grow the map).
fn ring_slot(m: &Mem, ring: u32) -> Option<(u32, u32, u32, u32)> {
    let (map, map_size) = (m.u32(ring + 4), m.u32(ring + 8));
    let size = m.u32(ring + 0x10).wrapping_sub(1);
    let off = if size == 0 { 0 } else { m.u32(ring + 0xc).wrapping_add(1) };
    if off.wrapping_add(size) & 3 == 0 && map_size <= size.wrapping_add(4) >> 2 {
        return None;
    }
    let off = off & map_size.wrapping_mul(4).wrapping_sub(1);
    let pos = off.wrapping_add(size);
    Some((off, size, pos & 3, map + 4 * ((pos >> 2) & map_size.wrapping_sub(1))))
}

/// NGP's ring step: `pop_front`, `push_back(speed)`, then whether no entry is above `stall_speed`.
fn ring_shift(m: &mut Mem, ring: u32, speed: f32, stall_speed: f32) -> bool {
    let (off, size, index, block_at) = ring_slot(m, ring).expect("checked before any writes");
    m.set_f32(m.u32(block_at) + 4 * index, speed);
    m.set_u32(ring + 0xc, off);
    m.set_u32(ring + 0x10, size + 1);
    let (map, map_size) = (m.u32(ring + 4), m.u32(ring + 8));
    !(off..off.wrapping_add(size + 1)).any(|k| {
        let block = m.u32(map + 4 * ((k >> 2) & map_size.wrapping_sub(1)));
        m.f32(block + 4 * (k & 3)) > stall_speed
    })
}

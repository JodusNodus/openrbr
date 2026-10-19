"""What a pure Rust step (crates/rbr-hybrid/src/pure.rs) still lacks, and whether the car and the wheels depend on the
listeners it skips: hybrid chains (every rbr-hybrid binding, harness/rust_step.py's machines) with the recording's
controls.

Usage: .venv/bin/python -I -B harness/pure_check.py SNAPSHOT... --recording DRIVE.bin [--steps N] [--skip ADDR:POP]...

  The worklist: the hybrid's host calls that rbr_hybrid::pure::uncovered says a pure step would stop at (no binding,
  no stand-in, after following the Invoke trampolines), with the binding that makes them, summed over the chain. The
  connections of the control emit (0x48cd90) are not host calls in the hybrid (the harness runs the original emit):
  harness-side code, so they are not in it (the pure driver, pure_step, shows them).
  The listener check: a third chain skips the listeners (LISTENERS, or the --skip ones): each returns at its entry as
  `ret POP` would, eax unchanged, as pure.rs's stand-in does. After every step the frame words (car and wheels, as the
  recording holds them) where it differs from the hybrid (all words) and from the original (outside the lane-3 words
  and wheel+0x9ac, which the hybrid leaves other than the original, and the words where the hybrid differs from the
  original too: rust_step.py judges those, e.g. as the original's stale-stack noise) are counted, and the writable
  bytes that differ from the hybrid at the end (the listeners' own state) are printed.
"""
import argparse
import contextlib
import io
import os
import struct
import sys

from unicorn import UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_EIP, UC_X86_REG_ESP

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402
import rust_step  # noqa: E402

# {entry: bytes its ret pops}: the targets the connections' Invoke trampolines jump to (thiscall on the signal).
LISTENERS = {
    0x476F30: 4,  # VCAnimationSpline, CSigCarControl
    0x476F10: 4,  # VCAnimationSpline (the school's checkpoint), CSigCarBrakeWallPenetration
    0x4B06A0: 4,  # CCarSound::M6a0, CSigEngineNoise
    0x5DD400: 4,  # particles (E82), CSigEngineNoise
    0x5DD460: 4,  # particles (E82, E82E_Sys::ScanEmit), CSolid::Emit's damping event
    0x5FE250: 4,  # CAnimatedReplayManager (E89), CSigEngineNoise
    0x61FC10: 4,  # CMrReplayManager (E94), CSigMrRBodyState
    0x61FCF0: 4,  # CMrReplayManager (E94), CSigMrRBodyEvent
    0x4996B0: 4,  # CReplayManager::PosFwd, CSigCarPosition
    0x4D6560: 4,  # E31Tire::M6560 (sound), CSigTireParticles
    0x5E3050: 4,  # E83, CSigTireParticles
    0x5E4A90: 4,  # E84, CSigTireParticles
    0x5F7040: 4,  # E87, CSigTireParticles
    0x5FE2B0: 4,  # CAnimatedReplayManager (E89), CSigTireParticles
    0x48F1420: 4,  # HedgeHog3D.dll (+0x1420), CSigTireParticles
    0x4956E90: 4,  # HedgeHog3D.dll (+0x66e90, its address in the drives), CHH_World::OnCarDamage, CSigCarDamage
    0x4906D50: 4,  # HedgeHog3D.dll (+0x16d50, the same), CHH_Car::OnCarDamage, CSigCarDamage
    0x726CDB0: 4,  # LocoMotion.dll (+0x1cdb0, the damage drives), CLM_CharacterCar::OnDetach, CSigDetachFromParent
    0x4906ED0: 4,  # HedgeHog3D.dll (+0x16ed0, the damage drives), CHH_Car::OnDetachFromParent, CSigDetachFromParent
    0x6E70AC0: 4,  # LocoMotion.dll (+0x20ac0), CSigCarControl (its load address in tanner-corolla and tarmac-lancer;
                   # pure.rs finds it by its code bytes on every drive)
    0x5735B0: 4,  # E62GameMenu::M5735b0 (the menu's mode map), CSigCarStandStill
    0x61F4D0: 4,  # E94Manager::E94DispatchB (the replay), CSigMrRCarGearChange
    0x491B10: 4,  # E17_ConnFFBDamage::Vf04 (force feedback), CSigCarDamage
    0x61F7A0: 4,  # E94Conn::E94ConnFwdE (the replay's damage snapshot), CSigCarDamage
    0x5E3E50: 4,  # E83 particles (EmitDebris), CSigCarDamage
    0x4B3120: 4,  # E24Conn_CarDamage::Invoke (CCollisionSnd::OnDamage, sound), CSigCarDamage
    0x493C80: 4,  # E18_ConnGhostRespawn::Vf04 (CGhostCarManager::SetPlayback, the ghost), CSigRespawn
    0x573900: 4,  # E62GameMenu::M573900 (the HUD's handler, the menu's timer), CSigOutsideAssistance
    0x6157E0: 4,  # CMrReplayCar::E93OnAssist (the replay's assist record), CSigOutsideAssistance
    0x5662C0: 4,  # E60Flow::M5662c0 (the race flow's RetireCar handler), CSigRetireCar
}
# The time keeper (CTimeKeeper::Update 0x4a0320, Record 0x4a0540) is not a listener: Update's count steers the car
# (pure.rs, LISTENERS).


def skip_listeners(m, listeners):
    """Each entry in listeners returns at once: eip from the return slot, esp past it and the popped bytes."""
    m.skipped_calls = {}

    def ret(mu, addr, size, _):
        esp = mu.reg_read(UC_X86_REG_ESP)
        mu.reg_write(UC_X86_REG_EIP, struct.unpack("<I", mu.mem_read(esp, 4))[0])
        mu.reg_write(UC_X86_REG_ESP, esp + 4 + listeners[addr])
        m.skipped_calls[addr] = m.skipped_calls.get(addr, 0) + 1
    for addr in listeners:
        m.mu.hook_add(UC_HOOK_CODE, ret, begin=addr, end=addr)


def check(path, a, hybrid, o, controls, listeners, static):
    """One snapshot's chains and report; (ok, the x87 results the cache lacked)."""
    ok = True
    snap = replay.Snapshot(path)
    snap.schedule = rust_step.schedule(a.recording, snap)
    orig, hyb, skip = rust_step.machine(snap, o), rust_step.machine(snap, o, hybrid), rust_step.machine(snap, o, hybrid)
    skip_listeners(skip, listeners)
    stack = replay.stack_below(snap)
    first, steps, last_diff = None, 0, 0
    for k in range(a.steps):
        step = snap.step + k
        control = controls.get(step) if k else b""
        if control is None:
            print("%s: no recorded control for step %d" % (path, step))
            break
        stops = [rust_step.advance(m, snap, k, control) for m in (orig, hyb, skip)]
        if any(stops):
            print("%s: step %d stopped: %s" % (path, step, "; ".join(s for s in stops if s)))
            ok = False
            break
        steps += 1
        fo, fh, fs = (rust_step.words(m.frame()) for m in (orig, hyb, skip))
        vs_hybrid = [i for i in range(len(fs)) if fs[i] != fh[i]]
        vs_orig = [i for i in range(len(fs)) if fs[i] != fo[i] and fh[i] == fo[i] and i not in static]
        if (vs_hybrid or vs_orig) and first is None:
            first = (step, vs_hybrid, vs_orig)
        last_diff = len([x for x in replay.differing(hyb, skip) if x not in stack])
    calls = ", ".join("%#x x%d" % kv for kv in sorted(skip.skipped_calls.items()))
    print("%s: %d steps; skipped %s" % (path, steps, calls or "none"))
    for (addr, caller), n in sorted(hyb.uncovered.items()):
        print("  pure stop: %#x x%d (from %#x)" % (addr, n, caller))
    if first:
        ok = False
        step, vh, vo = first
        print("  frame differs after step %d: %d words from the hybrid, %d from the original (first %s)" % (
            step, len(vh), len(vo), ", ".join("%#x" % (4 * i) for i in (vh or vo)[:6])))
    else:
        print("  car and wheels equal to the hybrid (every word) and the original (outside the lane-3 words and "
              "wheel+0x9ac) after every step; %d writable bytes differ from the hybrid at the end" % last_diff)
    return ok, set().union(orig.fpu_missing, hyb.fpu_missing, skip.fpu_missing)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshots", nargs="+")
    ap.add_argument("--recording", required=True)
    ap.add_argument("--steps", type=int, default=30)
    ap.add_argument("--skip", action="append", metavar="ADDR:POP",
                    type=lambda s: tuple(int(x, 16) for x in s.split(":")))
    a = ap.parse_args()
    listeners = dict(a.skip) if a.skip else LISTENERS
    hybrid, o = rust_step.Hybrid(), rust_step.oracle()
    controls = rust_step.recorded_controls(a.recording)
    # The frame words the hybrid itself leaves other than the original: lane 3 (rust_step.py's static words) and
    # wheel+0x9ac (RECORDING_GARBAGE, from other stack residue).
    static = set(rust_step.STATIC_FRAME_WORDS) | set(rust_step.RECORDING_GARBAGE)
    ok = True
    for path in a.snapshots:
        # With RBR_REMOTE the x87 results the cache lacks are fetched from the test machine and the snapshot rerun (as
        # rust_step.py); only the last round's report is printed.
        for _ in range(8):
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                good, missing = check(path, a, hybrid, o, controls, listeners, static)
            if not missing or not replay.REMOTE:
                break
            o.fetch(missing)
        print(out.getvalue(), end="")
        ok &= good
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

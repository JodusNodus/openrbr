"""Drives the hybrid step live: the original's StepForward in the emulator from a snapshot, with every rbr-hybrid
binding (harness/rust_step.py), one step per control payload read from stdin, one recording frame written to stdout
per step. It takes recorded car-control payloads only: the viewer's live mode (rust/rbr-render, `viewer --live`) sends
its device input (rbr_recording::input), which this refuses.

Usage: .venv/bin/python -I -B harness/live.py SNAPSHOT [--original]

  stdin: 32-byte control payloads, the recordings' layout (notes/gameplay-loop.md): throttle, brake, handbrake,
  clutch, steering (f32), flags (bit 0 gear up, bit 1 gear down), gear, aux. stdout: one FRM1 frame per payload
  (rust/rbr-recording), its pre and post counting from the snapshot's step, each followed by a 16-byte HUD record
  (`hud`: gear, engine speed, car speed) and a DMG1 record (`damage`: the damage zones and the last damage event,
  rbr_recording::damage) for the viewer. The first payload drives the snapshot's own call, whose control the game
  had already emitted: its bytes are not used.

  x87 results missing from replay.py's cache are not fetched: fsin/fcos are modelled (rust_step.py) and the other
  operations keep the emulator's result. Their count goes to stderr: those steps are not verified against the CPU.
  --original runs without bindings.
"""
import argparse
import os
import struct
import sys

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402
import rust_step  # noqa: E402

MAGIC = 0x314D5246  # 'FRM1'


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshot")
    ap.add_argument("--original", action="store_true")
    a = ap.parse_args()
    snap = replay.Snapshot(a.snapshot)
    o = rust_step.oracle()
    m = rust_step.machine(snap, o, None if a.original else rust_step.Hybrid())
    m.unverified_ok = True
    out = sys.stdout.buffer
    k = unverified = 0
    while True:
        control = sys.stdin.buffer.read(replay.CONTROL_BYTES)
        if len(control) < replay.CONTROL_BYTES:
            break
        if struct.unpack_from("<I", control, 28)[0] > 1:
            # rbr_recording::input: the viewer's device input, which only rbr-standalone puts through the game's filter.
            sys.exit("live.py: step %d: not a recorded car-control payload (word 7 %#x); "
                     "use rbr-standalone --live for the viewer's input" % (snap.step + k, struct.unpack_from("<I", control, 28)[0]))
        m.fpu_missing.clear()
        modelled = o.known.used
        stop = rust_step.advance(m, snap, k, control)
        if stop:
            sys.exit("live.py: step %d: %s" % (snap.step + k, stop))
        del m.writes[:]
        unverified += bool(m.fpu_missing) or o.known.used > modelled
        out.write(struct.pack("<5I", MAGIC, snap.step + k, snap.step + k + 1, 1, replay.CAR_BYTES) + control
                  + m.frame() + hud(m) + damage(m))
        out.flush()
        k += 1
        if k % 1000 == 0:
            report(k, unverified)
    report(k, unverified)


HUD_MAGIC = 0x31445548  # 'HUD1'
CAR_DRIVE = 0x4B0  # the car's embedded CDrive (car-body.md)
DRIVE_STATE = 0x8  # the drive's state block (CDrive's workers read it at this+8)
STATE_ENGINE_SPEED, STATE_GEAR = 0x45C, 0x54C  # rad/s (Get4520), current gear index (Get4530)
CAR_SPEED = 0x15FC  # |velocity|, m/s


def hud(m):
    """The 16-byte HUD record after each frame: HUD1, the gear index, the engine speed (rad/s) and the car speed (m/s)."""
    u32 = lambda at: struct.unpack("<I", m.read(at, 4))[0]  # noqa: E731
    car = u32(replay.CAR_PTR)
    state = u32(car + CAR_DRIVE + DRIVE_STATE)
    return struct.pack("<I", HUD_MAGIC) + m.read(state + STATE_GEAR, 4) + m.read(state + STATE_ENGINE_SPEED, 4) + m.read(
        car + CAR_SPEED, 4)


DMG_MAGIC = 0x31474D44  # 'DMG1'
CAR_DAMAGE_HANDLER = 0x9F4  # CCarDamageHandler pointer (car-damage-visuals.md)
HANDLER_ZONES, HANDLER_EVENT_CODE, HANDLER_EVENT_INDEX = 0x3808, 0x3E60, 0x3E68  # 64 zone floats; CSigCarDamage record


def damage(m):
    """The 268-byte damage record after the HUD record: DMG1, the 64 zone values, the last damage event's code and
    functor index."""
    handler = struct.unpack("<I", m.read(struct.unpack("<I", m.read(replay.CAR_PTR, 4))[0] + CAR_DAMAGE_HANDLER, 4))[0]
    return (struct.pack("<I", DMG_MAGIC) + m.read(handler + HANDLER_ZONES, 256) + m.read(handler + HANDLER_EVENT_CODE, 4)
            + m.read(handler + HANDLER_EVENT_INDEX, 4))


def report(steps, unverified):
    print("live.py: %d steps, %d with x87 results not from the CPU (sin/cos modelled, others the emulator's)" % (
        steps, unverified), file=sys.stderr)


if __name__ == "__main__":
    main()

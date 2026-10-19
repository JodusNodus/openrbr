"""Tape of the four struts (CSpringDamper, wheel+0xec4) over N steps of the original, for the pure-Rust run
(crates/rbr-physics/tests/pure_struts.rs).

Usage: .venv/bin/python -I -B harness/subtree_tape.py SNAPSHOT OUT --recording DRIVE.bin --steps N

  Runs the original only (run A of rust_step.py's chain: the recording's control, then StepForward(1)) for N steps
  from the snapshot and records, in order, every call of the struts' per-step methods with its inputs and outputs,
  and each strut's bytes after every step. Writes to a strut's bytes from outside these calls are listed as
  `foreign` lines: the pure run doesn't model them, so a tape with any is not a closed subtree.

Output (hex words and bytes, little-endian):

    strut K CONTROLLER DEF STATE FADE OWNER_ID       wheel K's strut and the objects it points at
    start K CONTROLLER STATE FADE DEF                 their bytes before the first step (0x30, 0x3c, 8, 0x11c)
    step N                                            the snapshot's step + N begins
    call K TRAVEL COORDINATE VELOCITY > SPRING BREAKPOINT DAMPING     Step (0x52e090): inputs > outputs
    reset K > ID PEAK BUMP DROOP VALID                ResetOutputs (0x52e010): the payload it emits
    update K                                          UpdateAndReset (0x52e060)
    end K CONTROLLER STATE FADE                       the bytes after the step
    def K DEF                                         after the last step
    foreign PC ADDR SIZE                              a write to a strut's bytes outside the calls above
"""

import argparse
import os
import struct
import sys

from unicorn import UC_HOOK_BLOCK, UC_HOOK_CODE, UC_HOOK_MEM_WRITE
from unicorn.x86_const import UC_X86_REG_ECX, UC_X86_REG_EIP, UC_X86_REG_ESP

sys.path.append(os.path.dirname(os.path.abspath(__file__)))
import replay  # noqa: E402
import rust_step  # noqa: E402

STEP, RESET, UPDATE = 0x52E090, 0x52E010, 0x52E060
POPS = {STEP: 0x18, RESET: 0, UPDATE: 0}
STRUT, CONTROLLER, STATE, FADE, DEF = 0xEC4, 0x30, 0x3C, 8, 0x11C


def u32(m, at):
    return struct.unpack("<I", m.read(at, 4))[0]


def hexwords(b):
    return " ".join("%08x" % w for w in struct.unpack("<%dI" % (len(b) // 4), b))


class Tape:
    def __init__(self, m):
        self.m, self.lines, self.open, self.returns = m, [], None, set()
        car = u32(m, replay.CAR_PTR)
        self.struts = [u32(m, car + 0x470 + 4 * k) + STRUT for k in range(4)]
        self.objects = []
        for k, c in enumerate(self.struts):
            d, s, f = u32(m, c + 4), u32(m, c + 8), u32(m, c + 0x10)
            if not f:
                sys.exit("strut %d (%#x) has no fade block" % (k, c))
            self.objects.append((c, s, f, d))
            self.lines.append("strut %d %08x %08x %08x %08x %08x" % (k, c, d, s, f, u32(m, u32(m, s + 0x18) + 0xEB4)))
        for k, (c, s, f, d) in enumerate(self.objects):
            self.lines.append("start %d %s %s %s %s" % (k, m.read(c, CONTROLLER).hex(), m.read(s, STATE).hex(),
                                                       m.read(f, FADE).hex(), m.read(d, DEF).hex()))
        for addr in POPS:  # block hooks: UpdateAndReset is reached by a tail jump
            m.mu.hook_add(UC_HOOK_BLOCK, self.on_entry, begin=addr, end=addr)
        for c, s, f, d in self.objects:
            for at, n in ((c, CONTROLLER), (s, STATE), (f, FADE), (d, DEF)):
                m.mu.hook_add(UC_HOOK_MEM_WRITE, self.on_write, begin=at, end=at + n - 1)

    def on_entry(self, mu, addr, size, _):
        esp, ecx = mu.reg_read(UC_X86_REG_ESP), mu.reg_read(UC_X86_REG_ECX)
        k = self.struts.index(ecx)
        args = struct.unpack("<7I", self.m.read(esp, 28))
        if addr == RESET:
            self.lines.append("reset %d > %s" % (k, hexwords(self.m.read(ecx + 0x1C, 20))))
        elif addr == UPDATE:
            self.lines.append("update %d" % k)
        ret = args[0]
        if ret not in self.returns:
            self.returns.add(ret)
            mu.hook_add(UC_HOOK_CODE, self.on_return, begin=ret, end=ret)
        self.open = (addr, k, esp + 4 + POPS[addr], args[1:])

    def on_return(self, mu, addr, size, _):
        if not self.open or mu.reg_read(UC_X86_REG_ESP) != self.open[2]:
            return
        entry, k, _, args = self.open
        self.open = None
        if entry == STEP:
            out = [u32(self.m, p) for p in args[3:6]]
            self.lines.append("call %d %08x %08x %08x > %08x %08x %08x" % ((k,) + args[:3] + tuple(out)))

    def on_write(self, mu, access, addr, size, value, _):
        if not self.open:
            self.lines.append("foreign %08x %08x %d" % (mu.reg_read(UC_X86_REG_EIP), addr, size))

    def end_step(self):
        for k, (c, s, f, d) in enumerate(self.objects):
            self.lines.append("end %d %s %s %s" % (k, self.m.read(c, CONTROLLER).hex(), self.m.read(s, STATE).hex(),
                                                   self.m.read(f, FADE).hex()))


def record(path, recording, steps, o):
    """The tape's lines, or exits with the run's stop."""
    snap = replay.Snapshot(path)
    controls = rust_step.recorded_controls(recording)
    m = rust_step.machine(snap, o)
    tape = Tape(m)
    for k in range(steps):
        control = controls.get(snap.step + k) if k else b""
        if control is None:
            sys.exit("%s: no recorded control for step %d" % (recording, snap.step + k))
        tape.lines.append("step %d" % k)
        stop = rust_step.advance(m, snap, k, control)
        if stop:
            return None, m, "step %d: %s" % (snap.step + k, stop)
        tape.end_step()
    for k, (c, s, f, d) in enumerate(tape.objects):
        tape.lines.append("def %d %s" % (k, m.read(d, DEF).hex()))
    return tape.lines, m, None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("snapshot")
    ap.add_argument("out")
    ap.add_argument("--recording", required=True)
    ap.add_argument("--steps", type=int, required=True)
    a = ap.parse_args()
    o = rust_step.oracle()
    for _ in range(8):  # as rust_step.py: x87 results the cache lacks are fetched from the test machine, then the run repeated
        o.known.used, o.known.modelled = 0, set()
        lines, m, stop = record(a.snapshot, a.recording, a.steps, o)
        missing = set(m.fpu_missing) | o.known.modelled
        if not missing or not replay.REMOTE:
            break
        o.fetch(missing)
    if stop:
        sys.exit("%s: %s" % (a.snapshot, stop))
    if o.known.used:
        sys.exit("%s: %d modelled x87 results (set RBR_REMOTE to fetch them)" % (a.snapshot, o.known.used))
    head = "# harness/subtree_tape.py %s --recording %s --steps %d" % (
        os.path.basename(a.snapshot), os.path.basename(a.recording), a.steps)
    with open(a.out, "w") as f:
        f.write("\n".join([head] + lines) + "\n")
    kinds = {}
    for line in lines:
        kinds[line.split()[0]] = kinds.get(line.split()[0], 0) + 1
    print("%s: %s" % (a.out, ", ".join("%d %s" % (n, k) for k, n in sorted(kinds.items()))))


if __name__ == "__main__":
    main()

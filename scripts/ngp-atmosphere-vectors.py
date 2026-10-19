"""Direct calls of NGP's atmosphere update (0x1004b5a0, notes/ngp.md "Atmosphere") in the emulator, for
rust/rbr-physics/tests/ngp_atmosphere.rs. Each call gets a car whose height (+0x148) is the input; the six words
it stores are the output. NGP.dll is relocated in a snapshot: its base is found from the CCar::CDef dict, which points
at NGP's key table (notes/ngp-load-path.md).

Usage: python3 -I scripts/ngp-atmosphere-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines: h, then T (10171adc), p (10171ad8), rho (10171ae0), 1/p (101767c8), 1.15 p (101767cc), 0.987 p (101767d0),
all f32 bits in hex.
"""
import importlib.util, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import UC_X86_REG_ESP  # noqa: E402

ATMOSPHERE = 0x1004B5A0
OUTPUTS = (0x10171ADC, 0x10171AD8, 0x10171AE0, 0x101767C8, 0x101767CC, 0x101767D0)


def main(snap_path, out, count="2000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    rebase = lambda a: a - 0x10000000 + base
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    car = m.heap_base
    rng = random.Random(1)
    heights = [0.0, -0.0, 684.1458, 1.0, -1.0, 4000.0, -500.0] + [rng.uniform(-200.0, 3000.0) for _ in range(int(count))]
    with open(out, "w") as f:
        for h in heights:
            hb = struct.pack("<f", h)
            mu.mem_write(car + 0x148, hb)
            mu.mem_write(esp, struct.pack("<II", replay.SENTINEL, car))
            mu.reg_write(UC_X86_REG_ESP, esp)
            mu.emu_start(rebase(ATMOSPHERE), replay.SENTINEL, timeout=10 * 1000000)
            words = [mu.mem_read(rebase(a), 4) for a in OUTPUTS]
            f.write(" ".join("%08x" % struct.unpack("<I", w)[0] for w in [hb] + words) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:])

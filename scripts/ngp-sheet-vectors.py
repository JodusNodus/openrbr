"""Direct calls of NGP's material sheet lookup (0x1004df40, rbr_physics::ngp::material::sample_slot) in the emulator,
for crates/rbr-physics/tests/ngp_material.rs. The slot's data is the 256 bytes 0..255 (byte i = i), so the result is
the cell's index; 1 call in 20 has no data (0).

Usage: python3 -I scripts/ngp-sheet-vectors.py SNAPSHOT OUT [COUNT]   (the worktree's .venv: unicorn, numpy)
Lines (hex words): has data, a, b (f32 bits), eax.
"""
import importlib.util, os, random, struct, sys

spec = importlib.util.spec_from_file_location(
    "replay", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "harness", "replay.py"))
replay = importlib.util.module_from_spec(spec)
spec.loader.exec_module(replay)
from unicorn.x86_const import UC_X86_REG_EAX, UC_X86_REG_ECX, UC_X86_REG_ESP  # noqa: E402

PICK = 0x1004DF40


def value(rng):
    k = rng.random()
    if k < 0.05:
        return rng.choice([0.0, -0.0, float("nan"), float("inf"), -float("inf"), 1e10, -1e10])
    if k < 0.15:
        return rng.choice([i / 16 for i in range(-2, 19)])  # cell edges
    return rng.uniform(-0.2, 1.2)


def main(snap_path, out, count="3000"):
    snap = replay.Snapshot(snap_path)
    base = struct.unpack("<I", snap.read(0x8EF73C + 0xC, 4))[0] - 0x171318
    m = replay.Machine(snap, log_writes=False)
    mu = m.mu
    esp = mu.reg_read(UC_X86_REG_ESP) - 0x100
    slot, data = m.heap_base, m.heap_base + 0x100
    mu.mem_write(data, bytes(range(256)))
    rng = random.Random(1)
    with open(out, "w") as f:
        for _ in range(int(count)):
            has = rng.random() >= 0.05
            a, b = value(rng), value(rng)
            mu.mem_write(slot + 4, struct.pack("<I", data if has else 0))
            mu.mem_write(esp, struct.pack("<Iff", replay.SENTINEL, a, b))
            mu.reg_write(UC_X86_REG_ESP, esp)
            mu.reg_write(UC_X86_REG_ECX, slot)
            mu.emu_start(PICK - 0x10000000 + base, replay.SENTINEL, timeout=10 * 1000000)
            words = [int(has)] + [struct.unpack("<I", struct.pack("<f", x))[0] for x in (a, b)]
            words.append(mu.reg_read(UC_X86_REG_EAX))
            f.write(" ".join("%08x" % w for w in words) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:])

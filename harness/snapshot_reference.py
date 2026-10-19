#!/usr/bin/env python3
"""Print `replay.py`'s Snapshot view in a line format consumed by the Rust parity test.

The Snapshot class is loaded from replay.py's AST so this reference command needs only Python's
standard library; importing the full replay tool would also require Unicorn and NumPy.
Usage: python3 -I -B harness/snapshot_reference.py SNAPSHOT [SNAPSHOT ...]
"""

import ast
import bisect
import mmap
import os
import struct
import sys


def replay_snapshot_class():
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "replay.py")
    tree = ast.parse(open(path, "r", encoding="utf-8").read(), filename=path)
    selected = []
    for node in tree.body:
        if isinstance(node, ast.ClassDef) and node.name == "Snapshot":
            selected.append(node)
        elif isinstance(node, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "MAGIC" for target in node.targets
        ):
            selected.append(node)
    namespace = {"bisect": bisect, "mmap": mmap, "os": os, "struct": struct}
    exec(compile(ast.Module(body=selected, type_ignores=[]), path, "exec"), namespace)
    return namespace["Snapshot"]


def row(*values):
    print("\t".join(str(value) for value in values))


def emit(path, Snapshot):
    snap = Snapshot(path)
    row("snapshot", os.path.basename(path))
    row(
        "header",
        snap.version,
        snap.step,
        snap.substeps,
        snap.thread,
        snap.teb,
        snap.stack_lo,
        len(snap.stack),
        int(snap.returns),
    )
    row("registers", *(snap.regs[name] for name in ("eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi")))
    row("fxsave", snap.fx.hex())
    for region in snap.regions:
        row("region", *region)

    samples = []
    if snap.stack:
        samples.append(("stack_head", snap.stack_lo, min(64, len(snap.stack))))
    if snap.has(0x8EF660):
        samples.append(("car_pointer", 0x8EF660, 4))
        car = snap.u32(0x8EF660)
        if car and snap.has(car):
            samples.append(("car", car, 0x1700))
            wheel_slots = car + 0x470
            samples.append(("wheel_ptrs", wheel_slots, 16))
            for i in range(4):
                wheel = snap.u32(wheel_slots + i * 4)
                if wheel:
                    samples.append(("wheel%d" % i, wheel, 0xA00))
    for label, address, size in samples:
        row("sample", label, "%#x" % address, snap.read(address, size).hex())
    row("end")


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    Snapshot = replay_snapshot_class()
    for path in sys.argv[1:]:
        emit(path, Snapshot)


if __name__ == "__main__":
    main()

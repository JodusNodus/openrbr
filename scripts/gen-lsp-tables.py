#!/usr/bin/env python3
"""Regenerate rust/rbr-formats/src/lsp_tables.rs from the 1.02 executable.

Reads the four `CBitField` name tables (0x7cbc10, 0x7cbcb0, 0x7cbd90,
0x7caf18; counts from the `GetBitCount` overrides) and the DefManager
registry (0x7c7fa0) from the exe's `.data`, and emits them as Rust
constants. Nothing is transcribed: every string and address comes from
the binary.

Run from the repo root:

    python3 -I -B scripts/gen-lsp-tables.py [--exe PATH]

`--exe` defaults to the in-repo 1.02 executable. The script checks that
its output matches the checked-in file byte for byte and exits nonzero
with a diff hint when it does not.
"""

import argparse
import os
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(  # the 1.02 exe in $RBR_PATCH (as the Rust tests), else the in-repo one
    os.environ.get("RBR_PATCH", os.path.join(ROOT, "original", "patch-1.02", "files", "Main_group")),
    "RichardBurnsRally_SSE.exe",
)
OUT = os.path.join(ROOT, "rust", "rbr-formats", "src", "lsp_tables.rs")

HEADER = """\
//! `CBitField` name tables and the DefManager registry, read from the
//! original 1.02 executable's `.data`.
//!
//! The bit counts come from the `GetBitCount` overrides (see
//! `notes/formats.md` "Physics scripts"); every name and address below is read
//! from the binary. Generated (run `python3 -I -B scripts/gen-lsp-tables.py`
//! from the repo root to regenerate).
"""

# (rust const, key, table VA, bit count, GetBitCount source).
BIT_TABLES = [
    ("BITFIELD_FLAGS", "Flags", 0x7CBC10, 20, "CPhysics::CDef::CFlags 0x523bb0"),
    ("BITFIELD_DBGFLAGS", "DbgFlags", 0x7CBCB0, 27, "CPhysics::CDef::CDbgFlags 0x523be0"),
    ("BITFIELD_LOGFLAGS", "LogFlags", 0x7CBD90, 20, "CPhysics::CDef::CLogFlags 0x523c10"),
    ("BITFIELD_MATERIAL", "Flags", 0x7CAF18, 4, "CMaterial::CFlags 0x520860"),
]

REGISTRY = 0x7C7FA0
# Expected (name, factory) pairs from notes/formats.md "Physics scripts"; checked
# against the binary to catch address drift, not used as data.
EXPECTED_REGISTRY = [
    ("Ground", 0x51BAA0),
    ("Wheel", 0x53B610),
    ("Solid", 0x52B8B0),
    ("Car", 0x4E6F20),
    ("CarBody", 0x4EBA80),
    ("HingedBody", 0x51F880),
    ("MountedBody", 0x521B50),
]


def sections(data):
    e_lfanew = struct.unpack_from("<I", data, 0x3C)[0]
    nsec = struct.unpack_from("<H", data, e_lfanew + 6)[0]
    opthdr = struct.unpack_from("<H", data, e_lfanew + 20)[0]
    imagebase = struct.unpack_from("<I", data, e_lfanew + 24 + 28)[0]
    secoff = e_lfanew + 24 + opthdr
    secs = []
    for i in range(nsec):
        _name, vsize, vaddr, rawsize, rawptr = struct.unpack_from(
            "<8sIIII", data, secoff + i * 40
        )
        secs.append((vaddr, rawsize, rawptr))
    return imagebase, secs


def va(data, imagebase, secs, addr):
    rva = addr - imagebase
    for vaddr, rawsize, rawptr in secs:
        if vaddr <= rva < vaddr + rawsize:
            return rawptr + (rva - vaddr)
    raise ValueError("address 0x%x outside image sections" % addr)


def read_str(data, imagebase, secs, va_addr):
    off = va(data, imagebase, secs, va_addr)
    end = data.index(b"\0", off)
    raw = data[off:end]
    if not raw or len(raw) > 63 or any(c < 32 or c > 126 for c in raw):
        raise ValueError("bad string at 0x%x" % va_addr)
    return raw.decode("ascii")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=EXE)
    args = ap.parse_args()
    with open(args.exe, "rb") as f:
        data = f.read()
    imagebase, secs = sections(data)

    out = [HEADER]
    for const, key, addr, count, src in BIT_TABLES:
        names = []
        for i in range(count):
            ptr = struct.unpack_from(
                "<I", data, va(data, imagebase, secs, addr + 4 * i)
            )[0]
            names.append(read_str(data, imagebase, secs, ptr))
        out.append("/// `%s` bit names (`%s`, table `0x%x`, %d bits)."
                   % (key, src, addr, count))
        single = "pub const %s: &[&str] = &[%s];" % (
            const, ", ".join('"%s"' % name for name in names)
        )
        if len(single) <= 100:
            out.append(single)
        else:
            out.append("pub const %s: &[&str] = &[" % const)
            for name in names:
                out.append('    "%s",' % name)
            out.append("];")
        out.append("")

    entries = []
    for i in range(16):
        rec = va(data, imagebase, secs, REGISTRY + 20 * i)
        name_ptr, dict, _z1, _z2, factory = struct.unpack_from("<IIIII", data, rec)
        try:
            name = read_str(data, imagebase, secs, name_ptr)
        except ValueError:
            break  # sentinel after the class records
        if factory == 0:
            break
        entries.append((name, dict, factory))
    if [(n, f) for n, _d, f in entries] != EXPECTED_REGISTRY:
        raise ValueError(
            "registry drift: %s"
            % [(n, "0x%x" % f) for n, _d, f in entries]
        )
    out.append("/// One DefManager registry record (table `0x%x`, stride 20): the")
    out.append("/// block name, the class dict address, and the factory function.")
    out.append("#[derive(Clone, Copy, PartialEq, Eq, Debug)]")
    out.append("pub struct RegistryEntry {")
    out.append("    pub name: &'static str,")
    out.append("    pub dict: u32,")
    out.append("    pub factory: u32,")
    out.append("}")
    out.append("")
    out.append("/// Every class record in the registry (order matches the exe).")
    out.append("pub const REGISTRY: &[RegistryEntry] = &[")
    for name, dict, factory in entries:
        out.append("    RegistryEntry {")
        out.append('        name: "%s",' % name)
        out.append("        dict: 0x%x," % dict)
        out.append("        factory: 0x%x," % factory)
        out.append("    },")
    out.append("];")
    out.append("")
    out.append("/// Bit names for a bitfield key (`None` when the key is not a")
    out.append("/// bitfield, or its name table is not proven). `Flags` is the")
    out.append("/// `CBitField` child of [`crate::lsp_keys::Class::PhysDef`] and")
    out.append("/// [`crate::lsp_keys::Class::Material`]; other `CBitField` members (such as")
    out.append("/// the solid `+0x14c` flags) have no proven name table.")
    out.append("pub fn bit_names(class: crate::lsp_keys::Class, key: &str) -> Option<&'static [&'static str]> {")
    out.append("    match (class, key) {")
    out.append('        (crate::lsp_keys::Class::PhysDef, \"Flags\") => Some(BITFIELD_FLAGS),')
    out.append('        (crate::lsp_keys::Class::PhysDef, \"DbgFlags\") => Some(BITFIELD_DBGFLAGS),')
    out.append('        (crate::lsp_keys::Class::PhysDef, \"LogFlags\") => Some(BITFIELD_LOGFLAGS),')
    out.append('        (crate::lsp_keys::Class::Material, \"Flags\") => Some(BITFIELD_MATERIAL),')
    out.append("        _ => None,")
    out.append("    }")
    out.append("}")
    text = "\n".join(out) + "\n"
    # As cargo fmt leaves it (rust/rustfmt.toml).
    text = subprocess.run(["rustfmt", "--edition", "2021", "--config-path", os.path.join(ROOT, "rust")], input=text,
                          capture_output=True, text=True, check=True).stdout

    with open(OUT, encoding="utf-8") as f:
        current = f.read()
    if current != text:
        with open(OUT + ".regen", "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        sys.stderr.write("lsp_tables.rs differs from generator output; see %s.regen\n" % OUT)
        return 1
    print("lsp_tables.rs matches generator output (%d bytes)" % len(text))
    return 0


if __name__ == "__main__":
    sys.exit(main())

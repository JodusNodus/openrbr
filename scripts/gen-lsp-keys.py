#!/usr/bin/env python3
"""Regenerate crates/rbr-formats/src/lsp_keys.rs from the key tables in the exe.

The table addresses and record counts come from
data/physics-lsp-fields.md (``## <class>`` sections with a
``Table `0x...` (N record(s))`` line); the records themselves are read from
original/patch-1.02/files/Main_group/RichardBurnsRally_SSE.exe. The
class-to-enum mapping, variant order, and parent links are fixed data below
(they mirror the evidence in that file).

Run from the repo root:

    python3 -I -B scripts/gen-lsp-keys.py [--exe PATH]

`--exe` defaults to the in-repo 1.02 executable. The reader kind, child
class and composite member of each key come from the `kind` column of the
data file (asm-proven member constructions; `open` means unconfirmed).
Retail use comes from scripts/lsp-unused.txt (keys no retail `.lsp`
attributes to that class).

The script checks that its output matches the checked-in file byte for byte
and exits nonzero with a diff hint when it does not. The `NGP_KEYS` block at
the end of the file is kept by hand (it lists the keys NGP's own files use,
which the exe does not hold) and carried over unchanged.
"""

import argparse
import os
import re
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXE = os.path.join(  # the 1.02 exe in $RBR_PATCH (as the Rust tests), else the in-repo one
    os.environ.get("RBR_PATCH", os.path.join(ROOT, "original", "patch-1.02", "files", "Main_group")),
    "RichardBurnsRally_SSE.exe",
)
FIELDS = os.path.join(ROOT, "data", "physics-lsp-fields.md")
UNUSED = os.path.join(ROOT, "scripts", "lsp-unused.txt")
OUT = os.path.join(ROOT, "crates", "rbr-formats", "src", "lsp_keys.rs")
NGP_KEYS_DOC = "/// Keys added by the NGP physics mod"

# md section title -> Rust Class variant, in enum order.
CLASSES = [
    ("CCar::CDef", "CarDef"),
    ("CCar::CSetup", "CarSetup"),
    ("CCar::CSetupOptions", "CarSetupOptions"),
    ("CCar::CState", "CarState"),
    ("CCarBody::CDef", "CarBodyDef"),
    ("CCarBody::CState", "CarBodyState"),
    ("CCarLightSystem::CDef", "CarLightDef"),
    ("CCarLightSystem::CState", "CarLightState"),
    ("CCollisionSolver::CDef", "CollisionSolverDef"),
    ("CCompleteCarSetup", "CompleteCarSetup"),
    ("CCompleteCarSetupOptions", "CompleteCarSetupOptions"),
    ("CDrive::CDef", "DriveDef"),
    ("CDrive::CSetup", "DriveSetup"),
    ("CDrive::CSetupOptions", "DriveSetupOptions"),
    ("CDrive::CState", "DriveState"),
    ("CEngine::CDef", "EngineDef"),
    ("CEngine::CState", "EngineState"),
    ("CEngine::CSetup", "EngineSetup"),
    ("CEngine::CSetupOptions", "EngineSetupOptions"),
    ("CMaterial", "Material"),
    ("CMaterialManager", "MaterialManager"),
    ("CPassenger::CDef", "PassengerDef"),
    ("CPassenger::CState", "PassengerState"),
    ("CPhysics::CDef", "PhysDef"),
    ("CPhysics::CState", "PhysState"),
    ("CSpringDamper::CDef", "SpringDef"),
    ("CSpringDamper::CState", "SpringState"),
    ("CSpringDamper::CSetup", "SpringSetup"),
    ("CSpringDamper::CSetupOptions", "SpringSetupOptions"),
    ("CTyre::CDef", "TyreDef"),
    ("CTyre::CState", "TyreState"),
    ("CTyre::CTyreTypes", "TyreTypes"),
    ("CTyre::CSetupOptions", "TyreSetupOptions"),
    ("CTyre::CSetup", "TyreSetup"),
    ("CVehicleControlUnit::CDef", "VcuDef"),
    ("CVehicleControlUnit::CState", "VcuState"),
    ("CVehicleControlUnit::CSetupOptions", "VcuSetupOptions"),
    ("CVehicleControlUnit::CSetup", "VcuSetup"),
    ("CWheel::CDef", "WheelDef"),
    ("CWheel::CState", "WheelState"),
    ("CWheel::CSetup", "WheelSetup"),
    ("CWheel::CSetupOptions", "WheelSetupOptions"),
    ("CBody::CState", "BodyState"),
    ("CGround::CDef", "GroundDef"),
    ("CHingedRigidBody::CDef", "HingedDef"),
    ("CMountedRigidBody::CDef", "MountedDef"),
    ("CSolid::CDef", "SolidDef"),
    ("HingedState", "HingedState"),
    ("MountedState", "MountedState"),
    ("shared entity-state table", "EntityState"),
    (None, "DefManager"),
]

PARENTS = [
    ("CarDef", "SolidDef"),
    ("CarDef", "CarState"),
    ("CarState", "EntityState"),
    ("HingedDef", "SolidDef"),
    ("HingedDef", "HingedState"),
    ("MountedDef", "SolidDef"),
    ("MountedDef", "MountedState"),
    ("WheelDef", "SolidDef"),
    ("WheelDef", "WheelState"),
    ("HingedState", "EntityState"),
    ("MountedState", "EntityState"),
    ("WheelState", "EntityState"),
]

HEADER = """\
//! Static `.lsp` key tables extracted from the original 1.02 executable.
//!
//! Each entry is one `{name, w1, w2, w3}` record from a `.data` key table:
//! `w2 * 4` is the member byte offset in the class object (`w1` links a
//! sub-object table). Generated from `data/physics-lsp-fields.md`
//! (run `python3 -I -B scripts/gen-lsp-keys.py` from the repo root to
//! regenerate); see that file for table addresses, dicts, and member
//! types.
"""


def parse_fields(path):
    """md section title -> (table address, record count, [(key, kind)])."""
    tables = {}
    title = None
    for line in open(path, encoding="utf-8"):
        m = re.match(r"## (.+?)\s*$", line)
        if m:
            title = m.group(1)
            continue
        m = re.match(r"Table `(0x[0-9a-f]+)` \((\d+) record", line)
        if m and title is not None:
            tables[title] = [int(m.group(1), 16), int(m.group(2)), []]
            continue
        m = re.match(r"\| (\S+) \| `\+0x[0-9a-f]+` \| ([^|]+) \|", line)
        if m and title in tables:
            tables[title][2].append((m.group(1), m.group(2).strip()))
            continue
        # MaterialManager surface-id rows (`id N`, not member offsets).
        m = re.match(r"\| (\S+) \| id \d+ .* \| surface id \|", line)
        if m and title in tables:
            tables[title][2].append((m.group(1), "surface id"))
    return tables


# Composite member name -> (variant, default sub-word reader or None,
# {sub-offset (hex, no leading zeroes): reader}). A None default fails
# loudly on unlisted offsets.
COMPOSITES = {
    "E13RealTriple": ("RealTriple", "Real", {}),
    "E13Real10": ("Real10", "Real", {}),
    "E13Real9": ("Real9", "Real", {}),
    "CReal2A": ("Real2A", "Real", {}),
    "CReal2B": ("Real2B", "Real", {}),
    "CRealPair": ("RealPair", "Real", {}),
    "E13UnsignedPair": ("UnsignedPair", "Unsigned", {}),
    "E13UnsignedTriple": ("UnsignedTriple", "Unsigned", {}),
    "Block60": ("Block60", "Real", {"0": "Unsigned"}),
    "E13Mixed50": ("Mixed50", None, {
        "0": "Real", "10": "Integer", "20": "Integer",
        "30": "Real", "40": "Real",
    }),
    "E39CDefCtorA": ("E39CDefCtorA", None, {
        "0": "Integer", "10": "Unsigned", "20": "Unsigned",
        "30": "Real", "40": "Real", "50": "Unsigned",
    }),
    "E39Hook5140": ("E39Hook5140", None, {
        "0": "Unsigned", "10": "Real", "20": "Unsigned",
        "c0": "Real", "d0": "Unsigned", "e0": "Real",
    }),
}

# Reader for the first sub-word of each composite (the `[0]` rows).
COMPOSITE_BASE = {
    "E13RealTriple": "Real",
    "E13Real10": "Real",
    "E13Real9": "Real",
    "CReal2A": "Real",
    "CReal2B": "Real",
    "CRealPair": "Real",
    "E13UnsignedPair": "Unsigned",
    "E13UnsignedTriple": "Unsigned",
    "Block60": "Unsigned",
    "E13Mixed50": "Real",
    "E39CDefCtorA": "Integer",
    "E39Hook5140": "Unsigned",
}

DIRECT = {
    "CReal": "Real",
    "CUnsigned": "Unsigned",
    "CInteger": "Integer",
    "CBitField": "Bitfield",
    "CVector": "Vector",
    "CObjIdQueue": "ObjIdQueue",
}


def parse_kind(kind, variants):
    """kind cell -> (reader, composite or None, child variant or None)."""
    if kind == "open":
        return ("Unknown", None, None)
    if kind == "surface id":
        return ("Child", None, "Material")
    m = re.match(r"`(\w+)( \(\d+ bits\))?`$", kind)
    if m and m.group(1) in DIRECT:
        return (DIRECT[m.group(1)], None, None)
    m = re.match(r"`(CReal|CUnsigned|CVector)\[\d+\]`$", kind)
    if m:
        return (DIRECT[m.group(1)], None, None)
    m = re.match(r"^child -> (\S+?)\*?$", kind)
    if m:
        name = m.group(1).strip("`")
        if name not in variants:
            raise ValueError("unknown child class %r" % name)
        return ("Child", None, variants[name])
    m = re.match(r"^(\w+)\[(\d+)\]$", kind.strip("`"))
    if m and m.group(1) in COMPOSITES:
        comp = COMPOSITES[m.group(1)][0]
        return (COMPOSITE_BASE[m.group(1)], comp, None)
    m = re.match(r"^(\w+)(?: \S+)? sub-word \+0x([0-9a-f]+)$", kind.strip("`"))
    if m and m.group(1) in COMPOSITES:
        comp, default, subs = COMPOSITES[m.group(1)]
        raw = int(m.group(2), 16)
        if m.group(1) == "E13Mixed50":
            # Array-relative in the notes (m_unk0C is the array base).
            raw %= 0x50
        sub = "%x" % raw
        reader = subs.get(sub, default)
        if reader is None:
            raise ValueError("unknown sub-offset %r in %s" % (m.group(2), kind))
        return (reader, comp, None)
    raise ValueError("unparsable kind %r" % kind)


def read_str(data, va):
    off = va - 0x400000
    end = data.index(b"\0", off)
    raw = data[off:end]
    if not raw or len(raw) > 63 or any(c < 32 or c > 126 for c in raw):
        raise ValueError("bad key string at 0x%x" % va)
    return raw.decode("ascii")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=EXE)
    args = ap.parse_args()
    with open(args.exe, "rb") as f:
        data = f.read()
    tables = parse_fields(FIELDS)
    with open(UNUSED, encoding="utf-8") as f:
        unused = set(line.strip() for line in f if line.strip())
    variants = {title: variant for title, variant in CLASSES if title}
    variants["CDefManager"] = "DefManager"

    out = [HEADER]
    out.append("#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]")
    out.append("pub enum Class {")
    for _, variant in CLASSES:
        out.append("    %s," % variant)
    out.append("}")
    out.append("")
    out.append("/// Value reader for one key: the member `Parse` the loader")
    out.append("/// dispatches to (`CBase::Parse` looks the member up by key and calls")
    out.append("/// its virtual `Parse`). `Unknown` means no retail `.lsp` attributes")
    out.append("/// the key to its class, so no member construction confirms it.")
    out.append("#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]")
    out.append("pub enum ReaderKind {")
    for reader in ["Real", "Unsigned", "Integer", "Bitfield", "Vector",
                   "Child", "ObjIdQueue", "Unknown"]:
        out.append("    %s," % reader)
    out.append("}")
    out.append("")
    out.append("/// Composite member holding a key's value: the key addresses one")
    out.append("/// sub-word (see the `kind` column of data/physics-lsp-fields.md). `None` for")
    out.append("/// direct members.")
    out.append("#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]")
    out.append("pub enum CompositeKind {")
    for comp in ["RealTriple", "Real10", "Mixed50", "UnsignedPair",
                 "UnsignedTriple", "RealPair", "Real2A", "Real2B",
                 "Block60", "E39CDefCtorA", "E39Hook5140"]:
        out.append("    %s," % comp)
    out.append("}")
    out.append("")
    out.append("/// One key-table record: the owning class, the key, and `w2 * 4`.")
    out.append("/// For [`Class::MaterialManager`] the offset is the surface id")
    out.append("/// (`w2 = 88 * id + 3`, see `E44_TableBuild`).")
    out.append("#[derive(Clone, Copy, PartialEq, Eq, Debug)]")
    out.append("pub struct KeyInfo {")
    out.append("    pub class: Class,")
    out.append("    pub key: &'static str,")
    out.append("    pub offset: u32,")
    out.append("    pub reader: ReaderKind,")
    out.append("    pub child: Option<Class>,")
    out.append("    pub composite: Option<CompositeKind>,")
    out.append("    pub used_in_retail: bool,")
    out.append("}")
    out.append("")
    out.append("/// Every key-table record in the original tables (order matches the exe).")
    out.append("#[rustfmt::skip]")
    out.append("pub const KEYS: &[KeyInfo] = {")
    out.append("    use {Class as C, CompositeKind as K, ReaderKind as R};")
    out.append("    const fn k(")
    out.append("        class: Class, key: &'static str, offset: u32, reader: ReaderKind,")
    out.append("        child: Option<Class>, composite: Option<CompositeKind>, used_in_retail: bool,")
    out.append("    ) -> KeyInfo {")
    out.append("        KeyInfo { class, key, offset, reader, child, composite, used_in_retail }")
    out.append("    }")
    out.append("    &[")
    for title, variant in CLASSES:
        if title is None:
            continue
        if title not in tables:
            raise ValueError("no table for section %r in %s" % (title, FIELDS))
        addr, count, kinds = tables[title]
        if len(kinds) != count:
            raise ValueError("%s: %d kind rows for %d records" % (title, len(kinds), count))
        for i in range(count):
            w0, _w1, w2, _w3 = struct.unpack(
                "<4I", data[addr - 0x400000 + i * 16:addr - 0x400000 + i * 16 + 16]
            )
            key = read_str(data, w0)
            notes_key, notes_kind = kinds[i]
            if notes_key != key:
                raise ValueError("%s record %d: notes %r vs exe %r"
                                 % (title, i, notes_key, key))
            reader, composite, child = parse_kind(notes_kind, variants)
            if variant == "MaterialManager":
                offset = (w2 - 3) // 88
            else:
                offset = w2 * 4
            child = "None" if child is None else "Some(C::%s)" % child
            composite = "None" if composite is None else "Some(K::%s)" % composite
            used = "%s::%s" % (variant, key) not in unused
            out.append('        k(C::%s, "%s", 0x%x, R::%s, %s, %s, %s),'
                       % (variant, key, offset, reader, child, composite, str(used).lower()))
    out.append("    ]")
    out.append("};")
    out.append("")
    out.append("/// Parent-table links (`CEntityContainerDict` dictA/dictB): a key lookup")
    out.append("/// searches these after the class own table (`E33Dict` parent chain).")
    out.append("pub const PARENTS: &[(Class, Class)] = &[")
    for child, parent in PARENTS:
        out.append("    (Class::%s, Class::%s)," % (child, parent))
    out.append("];")
    out.append("")
    out.append("/// The record of `key` in `class`, else in its parents (`None` when the key")
    out.append("/// is unknown to the class and its parents). Mirrors `CBase::Parse` key")
    out.append("/// dispatch.")
    out.append("pub fn key_info(class: Class, key: &str) -> Option<&'static KeyInfo> {")
    out.append("    KEYS.iter()")
    out.append("        .find(|info| info.class == class && info.key == key)")
    out.append("        .or_else(|| {")
    out.append("            PARENTS")
    out.append("                .iter()")
    out.append("                .filter(|(child, _)| *child == class)")
    out.append("                .find_map(|(_, parent)| key_info(*parent, key))")
    out.append("        })")
    out.append("}")
    out.append("")
    out.append("/// Byte offset of `key` in `class` (`None` when the key is unknown to the")
    out.append("/// class and its parents).")
    out.append("pub fn key_offset(class: Class, key: &str) -> Option<u32> {")
    out.append("    key_info(class, key).map(|info| info.offset)")
    out.append("}")
    text = "\n".join(out) + "\n"

    with open(OUT, encoding="utf-8") as f:
        current = f.read()
    # NGP_KEYS, the inventory of the keys NGP content uses beyond 1.02, is not in the exe: it is kept by hand at the
    # end of the file and carried over.
    hand = current.find(NGP_KEYS_DOC)
    if hand >= 0:
        text += "\n" + current[hand:]
    # As cargo fmt leaves it (rustfmt.toml).
    text = subprocess.run(["rustfmt", "--edition", "2021", "--config-path", ROOT], input=text,
                          capture_output=True, text=True, check=True).stdout
    if current != text:
        with open(OUT + ".regen", "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        sys.stderr.write("lsp_keys.rs differs from generator output; see %s.regen\n" % OUT)
        return 1
    print("lsp_keys.rs matches generator output (%d bytes)" % len(text))
    return 0


if __name__ == "__main__":
    sys.exit(main())

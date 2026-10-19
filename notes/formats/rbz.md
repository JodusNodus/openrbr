# `.rbz` archives

**Verdict: `.rbz` is a plain ZIP file.** No custom container, no encryption,
no obfuscation. Local file headers, then a central directory, then
end-of-central-directory — readable by any ZIP tool (verified byte-identical
extraction with Info-ZIP `unzip`).

## Layout

Standard ZIP (PKWARE APPNOTE), little-endian, no alignment or padding:

| Offset | Content |
|---|---|
| `0x00` | Local file header `PK\x03\x04` (`0x04034b50`): version-needed u16, flags u16, method u16, DOS time/date u16×2, CRC-32 u32, compressed size u32, uncompressed size u32, name len u16, extra len u16, name, data |
| … | Repeated per entry (directory entries are stored, size 0, name ends with `/`) |
| | Central directory: `PK\x01\x02` (`0x02014b50`) records with the same fields plus version-made-by, disk fields, internal/external attrs and the local-header offset |
| end | End-of-central-directory `PK\x05\x06` (`0x06054b50`): disk numbers, entry counts, central-dir size/offset, comment length |

Observed across all 93 retail 1.0 archives (5,289 entries):

- **Version needed: 2.0** everywhere; **version made by: 0 (FAT)** everywhere.
- **Methods: 0 (stored) and 8 (deflate) only.** 4,474 deflated, 815 stored
  (814 of those are directory entries; the single stored file is the 75-byte
  `Misc/lng.dll` text stub in `misc.rbz`).
- **Flags: 0 or 2 only.** Bit 1 is the deflate "maximum compression" flag —
  normal compressor output, not game-specific. No encryption (bit 0), no data
  descriptors (bit 3), no UTF-8 flag (bit 11) anywhere.
- **No extra fields, no archive/file comments** in any entry.
- **Names are ASCII with `/` separators**, e.g. `Physics/c_xsara/common.lsp`,
  `Scripts/arctic/load_321go_01.lua`. Case varies (`Physics/…`, `Misc/…`,
  `Scripts/…`, `Strings/…`, `Ghosts/…`); the game looks names up
  case-insensitively.
- DOS timestamps: data files 2004-07-06, directory entries 2004-07-29.

## Compression / encryption

- Deflate = raw RFC 1951 stream (no zlib header), i.e. what minizip/the
  `zip` tool writes. Decompresses with any inflater; CRC-32 (ISO HDLC) and
  sizes verify for every entry of every archive.
- Nothing is encrypted. The only "suspicious" bytes in the survey were the
  deflate maximum-compression flag and zlib version *strings* in the exe
  (below) — both benign.

## Exe evidence (official 1.02 `RichardBurnsRally_SSE.exe`)

The exe statically links **minizip `unzip.c` 1.00 + `ioapi.c`** (Gilles
Vollant) over **zlib 1.2.1**:

- RDATA strings (file offsets / VAs): `1.2.1` (= `ZLIB_VERSION`) at
  `0x390088` / `0x790088`, immediately followed by
  ` unzip 1.00 Copyright 1998-2003 Gilles Vollant - http://www.winimage.com/zLibDll`
  at `0x3900cc` / `0x7900cc`. (The `1.1.4` / `zlib error` strings nearby belong
  to libpng, not the archive code.)
- ZIP magic checks in the decompiled code
  (`out/decomp/RichardBurnsRally_SSE.exe.c`):
  - `FUN_00420860` — backwards search for the EOCD (`unzlocal_SearchCentralDir`).
  - `FUN_004209a0` — `unzOpen`: fills the `ioapi` callback table
    (`FUN_00434bb0`, `LAB_00434c00…`), parses the EOCD, rejects multi-disk
    archives (`-1`, and `-0x67` = `UNZ_BADZIPFILE` (-103) on count mismatch).
  - `FUN_00420c80` — central-entry parse (`0x02014b50`), including the DOS
    date→`tm_unz` unpack (`>>0x19` + `0x7bc` = year 1980+…, etc.).
  - `FUN_00421040` / `FUN_00421090` / `FUN_00421100` — `unzGoToFirstFile` /
    `unzGoToNextFile` / `unzLocateFile` (the latter uses the case-insensitive
    compare `FUN_004207d0` and a `0x100` = `UNZ_MAXFILENAMEINZIP` buffer).
  - `FUN_00421320` — local-header check (`0x04034b50`); **rejects any method
    other than 0 or 8** and entries with flag bit 3 (data descriptor).
  - `FUN_00421500` — `unzOpenCurrentFile` (`malloc(0x90)` context +
    `malloc(0x4000)` = `UNZ_BUFSIZE` stream buffer; `-0x68` =
    `UNZ_INTERNALERROR` (-104) on alloc failure).
  - `FUN_004217e0` / `FUN_00421ae0` / `FUN_00421bd0` — read / close-current /
    close. `FUN_004206c0` / `FUN_00420630` are the `getLong`/`getShort`
    little-endian readers.
- Game-side wrapper (`NLBLib2::NFileCore`, unprefixed `FUN_00401xxx`):
  - `FUN_00401070` — open archive via `unzOpen`, handle at object+`0x1c`.
  - `FUN_004011b0` — locate entry (`unzLocateFile`; `-100` =
    `UNZ_END_OF_LIST_OF_FILE` means missing).
  - `FUN_00401260` — read entry into a heap allocation.
  - `FUN_00401460` — read entry into a caller buffer (with size check).
  - `FUN_004015d0` — query uncompressed size.
  - `FUN_004018f0` — get-or-open archive from a global list (`0x1703e34`,
    compared with `_stricmp`).
  - `FUN_00401ba0` — file open: tries `CreateFileA` on the literal path
    first; on failure falls back to archives, normalising `\`→`/` (helper
    `FUN_00401160`), rewriting the extension after the last `.` to
    `g_archiveExtension` (`".rbz"`, `0x401d28`), and walking up directory
    prefixes as candidate archive names (`sprintf "%s/%s"`, referenced at
    code VA `0x401e11` from the literal at VA `0x78e738`).
  - Callers request logical paths such as `Physics/physics.lsp`,
    `misc/ai/ai.ini`, `MISC/PHYSICS.LSP` — the `.rbz` resolution is
    transparent to them.

## Verification

- Python survey over all 93 files: uniform structure above, `testzip()` CRC
  clean on every archive.
- Rust reader (`rust/rbr-formats`, module `rbz`): a thin wrapper over the
  `zip` crate, since the archives are standard ZIP. `RBR_DATA=… cargo test`
  reads all 5,289 entries (~1.29 GB) with size and CRC-32 checks, zero failures.
- `rbz extract Root/physics.rbz` output is byte-identical to Info-ZIP `unzip`
  (`diff -rq` clean).

## What's inside (retail 1.0)

- `Root/physics.rbz` — 115 entries (19 dirs): per-car `Physics/<car>/`
  directories (`common.lsp`, `gravel/snow/tarmac.lsp`, `r_*.lsp` race variants,
  `setups/d_*.lsp` defaults, `repair.ini`) plus `Physics/localphysics.lsp`.
  All Lisp-like setup text.
- `Root/misc.rbz` — 50 entries: AI/weather `.ini` (`Misc/ai/*`), dash/damage
  `.ini`, `prime.lsp`, `state.lsp`, `maps.m2b`/`maps.rmb` (menu maps?),
  `lng.dll` (75-byte language list, misnamed `.dll`).
- `Root/Scripts.rbz` — 749 `.lua` event/camera scripts, one per stage variant.
- `Root/strings.rbz` — 8 files (`bstrings.{cze,eng,fre,ger,ita,pol,rus,spa}`).
- `Root/ghosts.rbz` — 32 entries: `demo1.rpl` + 20 `.gho` ghost runs + 11 `.arp`.
- `Maps/*_textures.rbz` (88) — per-stage `.dds` textures (3,485 total).
- Patch 1.02 re-ships `Scripts.rbz`/`strings.rbz` with identical entry lists;
  1 script (`load_sp_redbull_can1a.lua`, same size, new CRC) and 7 string
  tables differ.

## Resolved unknowns

- The literal `.rbz` (file offset `0x38e740` / VA `0x78e740`) **is**
  referenced: one `.data` pointer at VA `0x7e9f28`, used by `CFile::OpenExe`
  (RBR `0x401ba0`) at `0x401d28`/`0x401e35` (the earlier scan missed it
  because it is a `.data` pointer, not a `.text` immediate). It is
  `g_archiveExtension` (`src/nlblib2/NFileCoreCFile.cpp:20`, used at
  `NFileCoreCFile.cpp:117` and `src/rbr/RBR_N0_FileCore.cpp:85`): on the
  archive-fallback path the loader replaces the request's extension after
  the last `.` with it (`Physics/physics.lsp` becomes the archive
  `Physics/physics.rbz`), then walks directory prefixes as candidate
  archives. Not a leftover.
- CRC mismatches on load are **not** detected at runtime (proven
  statically; our reader checks them anyway). The zip read path
  (`0x4217e0`) and close path (`ZipCloseCurrent`, `0x421ae0`, which only
  frees state and returns 0) compute no CRC, and the only `Crc32::Run`
  caller in the binary is recording/replay code (`FUN_0048dca0`), not the
  zip loader.
- Filenames are all ASCII in practice. The game compares raw bytes
  case-insensitively.

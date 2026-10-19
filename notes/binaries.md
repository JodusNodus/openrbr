# Binaries and sources

## Sources

| Source | Location | Notes |
|---|---|---|
| Retail 1.0 ISO | `~/Downloads/Richard Burns rally.iso` (sha256 `5b7af78c…6cd25329`) | Game data extracted to `original/retail-1.0/install/` (1,899 files, 2.6 GB, per-file hashes in `install.SHA256SUMS`). The disc exe has a copy-protection wrapper (`.hjio`/`.nsjcmg` sections), so it's not used for decomp. |
| Official 1.02 patch | `~/Downloads/RBR_v102.zip` | InstallShield 2002 SFX: `scripts/is_sfx_extract.py` unpacks the overlay, then `unshield` the inner cab into `original/patch-1.02/files/`. Ships full DRM-free binaries. **Decomp target.** |
| RSF install | `cachy:~/Games/Richard Burns Rally` | Read-only, never modify. Copies in `original/rsf-1.02/`. |
| Vanilla 1.02 test install | `cachy:~/Games/rbr-vanilla-1.02` | Ours to run and modify: retail 1.0 `install/Root` + content folders, with the 1.02 patch `Main_group` laid over it (`PixelShaders/Fog` and `VertexShaders/Fog` merged into the retail `fog` dirs). Launch with `rbr-vanilla [exe]` on cachy: same Proton (`proton-cachyos-slr` via umu) and native VC7 runtime overrides as the RSF launcher, separate prefix `~/Games/prefixes/rbr-vanilla`. Needs `trackinfo.dat` in the game root (the installer writes it; not on the disc or in the patch): 29 bytes = CRC, size 21, flag 0, `"rbr pc version 0.42"`. Without it `CPC_Application::Vf04` shows "Invalid fingerprint" and quits (`E03TrackInfoCheck` 0x00447100). Copy in `original/install-generated/`. Working ini: windowed, `Depth = 32`, `RunIntro = false` (fullscreen 16-bit with the intro hung). Controllers: the T500 shifter is disabled in this prefix (`HKCU\Software\Wine\DirectInput\Joysticks`, backup `user.reg.bak-t500`): its centred axes read as a held input, so the controls screen bound them instead of the wheel. Force feedback needs the harness plugin (`harness/README.md`). |

All hashes are in `original/SHA256SUMS`.

## Target binaries (`original/patch-1.02/files/Main_group/`)

| Binary | Role | Real functions | Vtables |
|---|---|---|---|
| `RichardBurnsRally_SSE.exe` | Game | 22,731 | 931 |
| `HedgeHog3D.dll` | Warthog 3D engine, the only d3d9 user | 10,507 | 140 |
| `LocoMotion.dll` | Driver/co-driver animation, exports `LM_Create`/`LM_Delete` | 3,086 | 102 |

- "Real" excludes thunks and compiler EH funclets (`Unwind@`/`Catch@`). About 36.5k in total, of which ~17.5k are unreferenced functions found by `CreatePaddedFunctions.java`: mostly tiny inline/template instances kept because the original link didn't use `/OPT:REF`. Those largely match for free once class definitions exist. Roughly 12% of the exe's code is statically linked third-party code (see `exe-subsystems.md`).
- The exe loads both DLLs with `LoadLibraryA`. `_NoSSE` variants and a 68 KB launcher (`RichardBurnsRally.exe`) pick SSE/NoSSE.
- Internal codename "Rally_7". Source paths like `\Rally_7\SourceMaterial\CodeBase\Game\Application\Car.cpp` and PDB names (`Rally7App.pdb`, `HedgeHog3D.pdb`) are referenced; no PDBs shipped.
- RSF's exe differs from official 1.02 only by: LARGE_ADDRESS_AWARE flag, header checksum, one code byte at file offset `0xb225` (`0x1f` → `0x20`), and 6 trailing zero bytes. The DLLs are identical.

## Structure highlights

- HedgeHog3D: `NHedgehog::` engine, `NLBLib2::` Warthog base lib. Renderer tiers `CRendererLow/Medium/High/Custom/Develop`, one shader class per car material.
- Exe physics `NPhysics::`: each component (`CCar`, `CWheel`, `CTyre`, `CEngine`, `CDrive`, `CSpringDamper`, ...) is split into `CDef` (static data), `CSetup`/`CSetupOptions`, `CState` and `CScratch`, plus an `NTweak` reflection system. Physics is data-driven (per-car files in `physics.rbz`).

## Official plugin SDK (shipped in the 1.02 patch, `Plugins/RBRTestPlugin/`)

- `IRBRGame.h`: implemented in the exe by `CRBRGameStub`. Slots 0–13 follow header order (slot 14 is the scalar deleting destructor). Names and signatures applied via `symbols/`, enums in `types/rbr_sdk.h` (rbr-recomp).
- `IPlugin.h`: the plugin callback interface (implemented by plugins).
- `Gendef.h`: 1,359 menu graphics box IDs (the exe's `DrawCarIcon` uses `0x519..0x51f`).

## Ghidra pipeline

The Ghidra project, its scripts (`scripts/analyze.sh`, `scripts/ghidra/`), the verified names (`symbols/`) and types
(`types/`) are in the rbr-recomp repository, with the C++ reconstruction.

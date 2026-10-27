# External references

Third-party material copied into the repository, with its licence next to it. Use it as evidence
(offsets, value codes, struct layouts of RBR 1.02); confirm each fact against the original's code
before relying on it, and cite the reference where it names or confirms something.

| Reference | Files | Licence | Useful for |
|---|---|---|---|
| NGPCarMenu by MIKA-N, https://github.com/mika-n/NGPCarMenu (src/RBRAPI.h) | `ngpcarmenu/RBRAPI.h`, `ngpcarmenu/LicenseText.txt` | MIT-style plus: derived work must be published as open source (user accepted, 2026-10-14). Keep the copyright notice with anything copied or derived from it. | Next-stage settings 0x01660800 (track, car, tyre 0-6, weather 0-2, damage 0-3 at +0x50), car object 0x008EF660 (+0x100 quat, +0x110 matrix, +0x190 spin, +0x1C0 velocity, +0x85C controls), game-mode state machine 0x007EAC48+0x728, race/HUD state 0x0165FC68, pace notes, menu system. |

Its licence also credits: Suxin's replay entry point notes (https://suxin.space/notes/rbr-play-replay/) and
TheIronWolf's RBR pace-note interface (https://gitlab.com/TheIronWolfModding/rbrcrewchief).

## Cited, not copied

Facts below were read from these projects and recorded in our own words. No code is copied; most are
GPL-3.0, so keep it that way.

| Reference | Licence | Useful for |
|---|---|---|
| RBR track exporter for Blender, https://github.com/RichardBurnsRally/blender-track-exporter. The source is only in the release zips (`releases/rbr-track-exporter-v0.3.2.zip`, package `rbr_track_formats/`); the repo pages are docs (`features.md`, `errors.md`). | GPL-3.0 | Writer-side models for `.trk`, `.col`, `.mat`, `.fnc`, `.dls` and `.lbs`, plus the material id list; names for all vertex and pixel shader IDs, the vertex attributes of each geometry type, object-lane sway data and LOD/alpha modes, render-state flags. Findings and retail checks: `../formats.md` (Stage files, Collision, Stage geometry, Markers, Shaders and vertex layouts). It covers only what the exporter writes, so it does not show every retail variant. |
| RBRHshift 3.5 (Racer_S, mod by Menkaura81), https://github.com/Menkaura81/RBRHshift3.5 (`src/shifter.h`) | GPL-3.0 | Car object `0x008EF660`: f32 clutch at `+0x86C` (bite point compared against 0..1) and writable gear at `+0x1100` (0 reverse, 1 neutral, 2-7 gears 1-6). Writing it changes gear, which answers the open question in `ngpcarmenu/RBRAPI.h`. It writes only while the long at `0x0165F10F` is nonzero (meaning still unknown). Not checked in our code. |
| RBRControls 5.3.7.0 HShifter (Steven Lee, mika-n, Towerbrah, Detegr, RSWats), from the RSF install; no public source, read from the DLL | MIT (stated in the DLL folder) | Initial reference for the advanced H-shifter (`../ngp.md`, H-shifter). The launcher reads its `Shifter_Error.wav` from the RSF install. |
| rbr-setup-extractor, https://github.com/wbsth/rbr-setup-extractor | none stated | Replays (`.rpl`) hold the car setup as plain `.lsp` text, from `(("` to `)\n ))`. |
| openRBRVR, https://github.com/Detegr/openRBRVR (`src/RBR.cpp`, `src/Dx.cpp`) | MPL-2.0 | VR renderer for 1.02 on DXVK. Exe render function `0x47E1E0`, camera functions `0x487680` (change camera), `0x4825B0` (camera position), `0x4BF690` (field of view), `0x485760` (bumper cam), particle renderers `0x5EFF60`/`0x5EFED0`/`0x5EFFD0`/`0x5ECA60`; the game creates 40 vertex shaders; vertex shader constants c0 (world-view-projection) and c20 (the matrix its sky and fog patch rewrites). The two registers are confirmed by the disassembly (`../formats.md`, Shaders and vertex layouts); the other addresses are not checked in our code. |

Checked and not useful: pmfrlyn/RBRTools (setup diff plus RSF/NGP UDP telemetry, not 1.02) and
tobier/rbrpad (rumble via SimHub and NGP telemetry).

# Exe subsystem map — RichardBurnsRally_SSE.exe (official 1.02)

Image base 0x400000, MSVC 7.1 RTM. `.text` 0x401000–0x72874b (~3.2 MB),
`.rdata` 0x7329000+, entry point 0x70c6a4 (CRT `__tmainCRTStartup` →
`WinMain` @ 0x473490). ~12,975 real functions, 931 vtables, 918 RTTI
classes. All addresses are VAs (image base 0x400000).

How subsystems were identified: RTTI class names from vtables, source-path
strings (`\Rally_7\SourceMaterial\...`, only 9 survive, all in
`Game\Application\...`), log/assert strings that name their own function
(`CApplication::Tic`, `CGameflow::ReadDamageHandler`, ...), full
`__FUNCSIG__` strings passed as `%s` to the log function, tolua
`#ferror in function 'X'` binding stubs, `RBR_CreatePlugin` / `HH_Create` /
`LM_Create` factory imports, and call relationships. Ranges below are
approximate (the linker scatters methods; ranges are min–max over
RTTI-attributed vtable methods unless noted).

The game log goes through `FUN_0043c8e0` (varargs trampoline, args
`level, ?, fmt, ...`) into worker `FUN_00490f50(level, ?, fmt, va_list)`.
Observed levels: 4 = info, 5 = notice, 6 = warning, 7 = error. Neither is
named in the TSV (no evidence for the name itself).

## Application / main loop

- `WinMain` @ 0x473490 (named in TSV): CRT calls it with
  `(hInstance, 0, lpCmdLine, nCmdShow)` after `__set_app_type(2)`.
  Installs unhandled-exception filter 0x4732f0 (writes `CRASHDUMP.TXT`
  via MiniDumpWriteDump from DBGHELP.DLL), reads `RichardBurnsRally.ini`
  (`FUN_004110f0`/`FUN_00411670` ini API), honors
  `Settings/RunStartup|RunIntro|RunDiagnose`, shows a "class RBR
  BlankScreen" splash window (`FUN_00472ff0`), checks CPUID SSE bit
  (0x2000000) — this is the SSE binary — then `CoInitialize`.
- `CApplication` (only 3 vtable methods survived; the bulk is plain
  functions ~0x47e1e0–0x482xxx). Named in TSV, all self-identifying:
  - `Tic` @ 0x4804f0 (per-frame tick; profiler scope-enter
    `FUN_00528880(&static, "CApplication::Tic", 0)` + scope-exit
    `FUN_00528690` — the same scope-guard pattern names
    `TicPhysicsRecording`, both `Hook`s and `StepForward`)
  - `TicPhysicsRecording` @ 0x47f420 (steps physics + replay tick;
    calls `FUN_0048c500`, `FUN_00499ff0`, `FUN_0049af10`,
    replay-camera `(**DAT_01659b70)(1)`, emits `CSigEndRace`)
  - `Draw` @ 0x47e1e0 (big ApplicationState switch, cases 0xc/0xd/0xf…;
    logs own FUNCSIG on unhandled state)
  - `Init` @ 0x4800c0, `InitDemo` @ 0x47fee0 (loads `MISC/PHYSICS.LSP`,
    "Got Physics ini"), `InitQuickRace` @ 0x481d00 (`printf("
    InitQuickRace")`), `StartQuickRace` @ 0x481660, `DelayedExitHomeBase`
    @ 0x481c10.
- `CPC_Application` (PC platform layer, 6 methods ~0x447480–0x44e210):
  `PlatformInit` (called in a try block at ~0x44e1cf; "Error in
  CPC_Application::PlatformInit: %s" in the catch handler), `SessionInit`
  for `CPC_AnimationHandler` @ 0x446030 (named; prints own name +
  "error initilizing.." on failure), `LoadScene`/`LoadCharacter`/
  `LoadAnimation`/`AddAnimationToCharacter` etc. exposed to Lua (see
  Scripting).
- Views: `CPC_View` (56 methods, 0x46fd60–0x472ee0), `CView` (55 methods,
  scattered 0x46f9e0–0x4a5080), `CGfxDaemon` (0x4bf000–0x4bf0a0),
  `CPC_GraphicsDaemon` (0x4bfc30–0x4c1b90; `ScreenShot` = vf74 @ 0x4bfea0
  named via its two `OutputDebugStringA` failure messages),
  `CPC_TextureLoader` (0x465ba0), `CPC_TexturePage::AllocateMemory` @
  0x4c1dd0 (named; passes own name as debug tag to the texture
  allocator).

## Signal/slot bus (the game's component wiring)

Everything is decoupled through `CSignalManager::CMultiSignal1<T>`:
one `Emit` instantiation per signal type (~88 named in TSV via the
function logging its own `__FUNCSIG__`, e.g.
`CSignalManager::CMultiSignal1_CSigMenu::Emit` @ 0x44fdf0) plus 4
`Connect` instantiations (`CSigCarControl` @ 0x449440, `CSigInput` @
0x449650, `CSigMenuActivate` @ 0x4496e0, `CSigReplayCommand` @ 0x449770).
Emit bodies iterate connection lists filtered by ApplicationState
(0x20 states) and call each connection's slot-1 handler; recursion and
out-of-range states log through the signal's own FUNCSIG string.
Receivers derive from `CConnectionBase<TSig>` / `CConnection<TObj,TSig>`
— ~150 `__CConnection_*` RTTI classes (e.g. every particle system,
sound object and camera subscribes to `CSigCamera`,
`CSigCarGeneralState`, ...). `CDriveLine::SetDriveLine` @ 0x450aec0
named (dumps own FUNCSIG + N). No RTTI exists for `CSignalManager`
itself, so TSV names use a mechanical `CMultiSignal1_<T>` suffix; the
exact instantiation is in the evidence column.

## Physics `NPhysics::` (in-exe, data-driven)

465 vtable methods spanning 0x47b2b0–0x53f230 — the largest game-owned
block. Component pattern per class: `CDef` (static data) / `CSetup` +
`CSetupOptions` (car setup) / `CState` (sim state) / `CScratch`
(temporaries); `NTweak` reflection (`CReal`, `CUnsigned`, `CVector`,
`CDict`, factories).
Named in TSV: `CPhysics::StepForward` (vf08 @ 0x5273a0),
`CCar::Hook` (vf58 @ 0x4e9930), `CWheel::Hook` (vf58 @ 0x539a20).
Key classes: `CPhysics` (+`CDef::{CFlags,CLogFlags,CDbgFlags}`),
`CCar`, `CWheel`, `CTyre` (+`CTyreTypes`), `CEngine`, `CDrive`,
`CSpringDamper`, `CVehicleControlUnit`, `CCarBody`, `CHingedRigidBody`,
`CMountedRigidBody`, `CSolid`, `CBody`, `CGround`, `CPassenger`,
`CCarLightSystem`, `CBrakeWall(+Manager)`, `CCollisionSystem`,
`CCollisionSolver`, `CL1/CL2CollisionDetector`,
`CL1/CL2PenetrationDetector`, `CL2GiganticPenetrationDetector`,
`CMaterial(+Manager)`, `CDefManager`, `CDamageHandler`,
`CCarDamageHandler` (`ReadBinary` @ 0x4eee00 named: magic-number +
`nNumZones` checks), `CEntity(ContainerDict/OldFrame/Frame)`.
`NRepair::` (71 methods, 0x4f2bc0–0x501d40): ~30 damage classes
(`CEngineRadiatorDamage`, `CTurboDamage`, `CShockAbsorberDamage`,
`CWheelDamage`, `CTyreDamage`, ..., `CCarRepairHandler`).
`NRegulation::CTimeKeeper`, `NCarSetup::CDummyCar`.
Physics setup comes from `.lsp` scripts (`Physics/physics.lsp`,
`gravel|snow|tarmac.lsp`, `common.lsp`, `Physics/%s/tyres.lsp`,
`misc/prime.lsp`, `MISC/PHYSICS.LSP`, `Physics\school\sfgravel.lsp`).

## Replay `NMrReplay::` + managers

- `NMrReplay::` (32 methods, 0x609c90–0x621330): `CMrReplayManager`,
  `CMrReplayCar`, `CMrReplayBody`, `CMrReplayHingedBody`, `CTimeLine`,
  plus templated buffer/stream/event handlers
  (`__CMrReplayBufferHandler_*`, `__CMrReplayStreamHandler_*`,
  `__CMrEventBufferHandler_*`).
- `CReplayManager::LoadGeneric` @ 0x49a2b0 (named; "Damage handler
  readin failed" / "Car setting readin failed"); versioned loaders use
  `LoadReplay::Version%u` prefixes @ 0x499b60 (candidate name, not TSV).
- `CAnimatedReplayManager/Car/Body/Wheel` (0x5fba50–0x5ff380) with
  `__CAnimatedReplayBuffer_*` templates; the `.h` source path
  `Game\Application\AnimatedReplay\AnimatedReplayBuffer.h` survives in
  asserts. `CRescueHelicopter` (0x49bb10–0x49c5d0,
  `CPC_RescueHelicopter` 0x452290–0x452650) is driven by
  `scripts/load|run_rescuechopper.lua`.

## Sound

- `SOUND::` (59 methods, 0x4cea00–0x4dd9f0): `CSound` (27),
  `CMaster`/`CMixer`/`CGroup`/`CObject`/`CSlave`/`CPreRender`/`COgg`/
  `CWave`/`CSpeech`/`CEngine`/`CImportExport`.
- PC backend `CDX*` (~0x4ad090–0x4d8320): `CDXEngine`, `CDXSound`,
  `CDXMixer`, `CDXPrologicMixer` (15 methods), `CEAXMixer`, `CDXOgg`
  (.ogg music/speech: `Audio\Music\ba.ogg`,
  `audio\speech\broken\%s\%s.ogg`, `Audio\RS\%s\%d.ogg`), `CDXSpeech`,
  `CDXTransmission`, `CDXTurbo`, `CDXRain`, `CWaveEx`, `CWaveLoader`,
  `CEqualizer`, `CLowPass`, `CPan`, `CCycleMixer`, `CMixMaster`,
  `CSoundManager`, `CMusic` (plays `MUSICPC::FRONTEND|REPLAY|SERVICEPARK`
  keys @ 0x4b4760), `CSoundFX`, `CSoundEffects`.
- Game sound users: `CCarSound` (0x4ac150–0x4b1bc0), `CTireSnd`
  (0x4d6670–0x4d8320, `TIRE::SURFACES`/`CAR::SUSPENSION` keys),
  `CCollisionSnd` (`IMPACT::*` keys), `CServiceParkSnd`
  (`SFX::*` keys), `CAmbientSounds`, `CRainSnd`, `CPrizeCermonySnd`
  (sic), `CIncidentSnd`, `CRallyIncidentSnd`, `CRallySchoolSnd`,
  `CDriveLine` (`CSigDriveLine::operator[]` inlined at 6 sites —
  candidate, not TSV).
- Config-key strings (`SOUNDDEF::*`, `CAR::*`, `SFX::*`, `IMPACT::*`,
  `MUSICPC::*`, `TIRE::*`, `STEER::*`, `PACENOTE::*`, `EQ_*`,
  `AMP::SOUNDS`, `OPTIONS::SOUNDS`) are data, not function names.

## Input + force feedback

- `CInputManager` (`IsAnyForceFeedBackDevice` @ 0x4cb6b0 named),
  `CKeyFilter`, `CInputDaemon`, `CInputFilterUI`,
  `CControlConfiguratorDaemon`, `NPC_Input::CInputManager` /
  `CForceFeedBack`, `SInputDeviceImpl::{Get,Un,Pause}ForceFeedBack*`
  (@ 0x4c6d00/0x4c6d90, DirectInput joysticks), `CFFBController`,
  `NPC_Input::LoadDeviceImageTexture` @ 0x4cafa0 (named; device-image
  texture tag). `STEER::AXIS|STICK|BUTTON_DIGITAL` config keys @
  0x48bb70. Imports DINPUT8.dll; TrackIR via NPClient.dll
  (`FUN_004cd890` resolves all `NP_*` entry points).

## Track loading + collision data + weather

- `CTrackLoaderDaemon` (15 methods, 0x4a06f0–0x4a1990) /
  `CPC_TrackLoaderDaemon` (14, 0x467630–0x46f330), decorators
  `C3DTrackLoaderDecorator` (0x5b2ca0–0x5b3da0),
  `CQRTrackLoaderDecorator` (0x59acc0–0x59b4e0),
  `ITrackLoaderDecorator`, `CRallySchoolDecorator`,
  `CQRResultBaseUI`/`CQRResultUI`.
- `CWeatherDaemon` (13 methods, 0x4a8ee0–0x4aa1b0) /
  `IWeatherDaemon`, `CWeatherTable` (+`CWeather`), weather strings from
  `Game\Application\FrontEnd\WeatherTable.cpp`.
- Stick/vegetation system `NStickSystem::` (0x6009b0–0x605670):
  `CFenceRenderer::Init` @ 0x605100 (named; loads
  `Textures\car_dpshad2.tga`), `CFenceHandler`, `CDummyObject`.
- Post filters `NFilter::` (0x623840–0x625bb0): `CTrailFilter`,
  `CRadialFilter`, `CGaussianFilter`, `CRobertsFilter`, `CPaintingFilter`,
  `CNegativeFilter`, `CSepiaFilter`, `CTVFilter`, `CFilterBase`.
- Particles `NParticleSystem(::2D)::` (0x450cb0–0x5f7540):
  `CParticleSystem`, `CPC_ParticleRenderer` (14 methods
  0x5eb790–0x5f0210), `CWave`, emitters (incl. rain/wipers/bugsplash/
  camerdirt), `CTrackEmittersManager`, `CWiperData`.
  `RenderParticles`/`ParticleQuality` keys, `Textures\particles.dat`.
  `FUN_005e5aa0` talks to dev-only `ParticleTweakerD.dll`
  (`UnloadParticleTweaker`).

## Frontend/menus/game flow

- `CFrontEndDaemon` (0x55fef0–0x561660; source path
  `Game\Application\FrontEnd\FrontEnd.cpp`), `CGameMenuDaemon`
  (0x56f1f0–0x584145), `CMenuDaemon`, `CMenuPage`, ~150 menu-item/UI
  classes (`CMenuItem(UI)`, `CButtonMenuItem(UI)`, sliders, car/stage
  selectors, `CSeason*` screens, `CRBC*`, `CMP*` multiplayer, `CRS*`
  rally-school, `CQR*` quick-race...). Menu box IDs come from the
  auto-generated `gendef.h` ("Kangaroo" exporter, 1,359 IDs; same file
  the SDK ships); `CRBRGameStub::DrawCarIcon` maps car index →
  0x519..0x51f.
- `CGameflow::ReadDamageHandler` @ 0x567b90 (named),
  `GAMEFLOW::InitializeCarTweak` @ 0x566080 (named; uppercase = script-
  visible name), `InitQuickRaceDamageHandler:: Invalid Player ID` @
  0x567640 (candidate), `CGameflow::Couldn't read rbctimes.ini` @ 0x568970
  (candidate).
- `CMenuMap3D`/`CPC_MenuMap3D` (3D menu background map),
  `CHUD`/`CCamera` exist only as signal peers (`__CConnection_VCHUD_*`,
  `__CConnection_VCCamera_*`); `HUD::Init` @ 0x577840 (named;
  `minidash.ini`), `CDigiDash`.
- `CPaceNoteHandler` (0x4b64d0–0x4b74f0; `PACENOTE::*` keys),
  `CGhostCarManager` (signal peer) + `CGhostCarFile.cpp` source path,
  `CAttractDaemon`, `CCreditsDaemon`, `CCheatDaemon`.
- Season/career: `CAISeason::GetPlayerFalseStarts` @ 0x5d07a0 (named),
  `LCAISeason::GetPlayerFalseStarts`, `CAISeason::ReadSeasonFromStream`
  string, `NSeason::` (0x5d6020–0x5d68a0), `RBChallange::GetRBChallengeMap`
  @ 0x5a6e50 (named; sic — original typo), `CTimeCard`, `CRepairOrdersUI`.

## Save files / profiles / memory units

- `CPC_SaveFileManager` (34 methods, 0x452760–0x453fd0; `Init` = vf04 @
  0x452780 named via "LoadMenuSettings failed"), `CProfile(Manager)`
  (0x59a360/0x59a750; source path `...FrontEnd\Profile.cpp`),
  `CCarSaveFileManager::ReadFromBinaryStream` @ 0x48e1e0 (named; magic
  number + LoadFromMemory), `CCarSettingsManager::InitializeCarSetting`
  @ 0x547530 (named), `CMenuSaving::ReadFromBinStream` @ 0x585c10
  (named; checksum), `CMemoryUnitMessageDaemon`, `CMemoryUnitButton`,
  `CMemoryUnitLoaderUI`, `CImportExport`/`SOUND::CImportExport`,
  `CDirectoryFiles`, `CFileEncryptor`, `Crc32`. Xbox memory-unit
  leftovers: `SMenuSettings::SMemoryUnitSettings`.

## Lua scripting (game logic in Lua 4.0)

- Static Lua 4.0 (`"Lua 4.0"`, lparser/lstrlib/ldo error strings) +
  tolua (`tolua_tbl_*`, `tolua_tag_*`, `tolua_error` =
  `FUN_006d8ff0`, builtins) ~0x6d8000–0x6e98ff.
- 44 game Lua bindings 0x438850–0x43afd0, all `(lua_State*) -> int`,
  named `Lua_*` in TSV via their `#ferror in function 'X'` stub:
  `CPC_DebugSettings` render-debug toggles (`SetDrawTrack|Objects|
  Segment|World|SkyBox|Shadow|Wireframe|...`, `SetD3DStats`,
  `SetUseQuadTree`, ...), `CPC_Application` character/scene API
  (`LoadScene|Character|Animation`, `SetActiveAnimation`,
  `PlaceCharacterAtCar`, `Birdstrike`, `DestroyCharacter`, ...),
  `SetCamera`, `SetAppState`, `SetPDebug`, `LoadCar` (→ 0x447200),
  `LoadTrack`, `SPI/SPF/SPB`.
- Scripts (by string): `startup.lua`, `custom.lua`,
  `run_car_flip.lua`, `load_car_flip%d.lua`, `scripts/robert.lua`,
  `scripts/richard.lua`, `load|run_rescuechopper.lua`, `*.lsp`.
- `CScriptManager`/`CPC_ScriptManager` (0x49f5e0 / 0x5f1160–0x5f1450).

## Plugin system + engine/DLL glue

- `FUN_00625c10` (class unknown, no RTTI): `LoadLibraryA(path)` +
  `GetProcAddress("RBR_CreatePlugin")` → `IPlugin*`; `FUN_005c8470`
  enumerates the `Plugins` dir (`\Plugins\`, "No plugins loaded" @
  0x626870) — candidates, not TSV (no name evidence).
- `CRBRGameStub` = `IRBRGame` impl (15 slots @ 0x626a70–0x626d90,
  previously verified).
- HedgeHog3D glue `FUN_004bf2f0`: `LoadLibraryA` +
  `GetProcAddress("HH_Create")`; LocoMotion glue `FUN_004bf4e0`:
  `...("LM_Create")`; `CGfxDaemon`, `CPC_GraphicsDaemon`,
  `CPC_AnimationHandler` (drives LocoMotion characters).
- DirectPlay (dplayx/dpnet imports) + `CMultiplayer*` UI for
  multiplayer; DirectShow-style media errors only via dxerr (no Bink
  strings — video via `CPCVideoPlayer`/`CVideoPlayer`, backend TBD).

## Misc identified

- `NLBLib2::` (28 methods, 0x401a30–0x419680 — first game code in
  `.text`): Warthog base lib statically linked into the exe:
  `NFileCore::{CFile,CBinaryData,CTextData}`, `CDiskLoader`,
  `CD3DHandler` (vf00 @ 0x40b210 creates the "D3D Window"),
  `NLogger::{CLogger,CStreamBuf}`, exception types. (HedgeHog3D.dll
  contains its own copy.)
- `CDiskWriter::EndChunk` @ 0x41a5b0 (named; throws on unbalanced
  chunk), `CLocale::InitStringTable` @ 0x496a10 (named),
  `CSMLModel::CSMLModel` @ 0x5eb350 (self-named via allocator tag —
  candidate, call-shape unclear), `CCar::Hook`-style profiler scope
  helper `FUN_00528880`/`FUN_00528690`.
- `CErrorDaemon`/`CPC_ErrorDaemon`, `CLogger`, `CFileSystem` /
  `NPC_FileSystem::CPC_FileSystem` (+`CPC_FileOutputStream`),
  `CVersionDaemon`/`CPC_VersionDaemon`, `CSys_Console`.

## Statically linked libraries (exclude from hand decomp)

| Library | Evidence | Approx. range |
|---|---|---|
| D3DX9 (shader compiler + texture loaders) | `ps_1_1..ps_2_a` targets, HLSL errors ("recursive functions not yet implemented", "missing semantics"), `#?RADIANCE`, `"Microsoft (R) D3DX9 Shader Compiler 4.09.00.1126"`, `D3DX_VERSION`, `DebugSetMute` (also dynamically from d3d9.dll @ 0x632790) | ~0x630000–0x6cffff |
| libpng + zlib (inside/near D3DX tex loaders) | "Incompatible libpng version…", "No IDATs written", "Extra compressed data", "CRC error", "zlib 1.1.4 / inflate 1.2.1 / unzip 1.00" strings | ~0x664000–0x66bfff, ~0x6ac000–0x6b0fff |
| zlib (standalone) | `scan_tree` @ 0x6d04d4, "invalid stored block lengths", "oversubscribed dynamic bit lengths tree" | ~0x6d0000–0x6d7fff |
| tolua + Lua 4.0 | `"Lua 4.0"`, `tolua_tbl_*`/`tolua_tag_*`, lparser errors ("upvalue must be global…", "pragmas are no longer supported"), builtins @ 0x6e1090–0x6e12d0 | ~0x6d8000–0x6e98ff |
| dxerr9 (DXGetErrorStringA @ 0x6e9a66, DXGetErrorDescriptionA @ 0x6eedbc, DXTraceA @ 0x6f4144) | giant HRESULT switches (DDERR_/DIERR_/VFW_E_), "File: %s\nLine:…" + OutputDebugStringA | 0x6e9a66, 0x6eedbc, 0x6f4144 |
| libvorbis (Xiph.Org 20020717) + libogg framing | "Xiph.Org libVorbis I 20020717", "vorbis", `OggS`, string-free float DSP | ~0x6f5000–0x70c283 |
| MSVC 7.1 CRT startup + EH funclets | `entry` @ 0x70c6a4, `__security_check_cookie`, `__chkstk`, `__RTC_Initialize`, `__SEH_prolog/epilog`, thousands of `Unwind@` | 0x70c284–0x72874b |
| CRT imports (dynamic) | MSVCR71.dll/MSVCP71.dll imports; STL is header-only (`std::basic_string` inlined everywhere) | — |

DirectX HRESULT/COM strings at 0x6e8xxx/0x6ecxxx belong to dxerr's
tables. `Memcpy` @ 0x6ae26a and `scan_tree` @ 0x6d04d4 are the only two
Ghidra-pre-named static-lib functions. MSVCP71 STL, DirectPlay
(dplayx/dpnet), DINPUT8, DSOUND, ole32, GDI32, KERNEL32/USER32/ADVAPI32
are dynamic imports, as are HedgeHog3D.DLL, LocoMotion.DLL,
NPClient.dll (TrackIR) and misc\lng.dll.

## Candidates (unverified — NOT in TSV)

- `FUN_00528880`/`FUN_00528690`: profiler scope enter/exit (stores
  `Class::Method` name into a static slot). Name guesses:
  `CProfileScope::Enter/Exit` or hook slots — the `CCar::Hook` /
  `CWheel::Hook` names may literally mean these are hook points.
- `FUN_0043c8e0`/`FUN_00490f50`: the game log (levels 4–7 observed).
- `NPhysics::CWheel::vf98` @ 0x539040: calls obj->vf00 with own
  FUNCSIG `"NPhysics::CWheel::HandleWipeOut(void)"` then prints
  `"%s.%s"` — HandleWipeOut-related debug dump, not necessarily the
  method itself.
- `NPhysics::NTweak::CGetValueText_CTest::vf00` @ 0x52f570: trap stub
  logging `"%s doesn't make sense"` with its own `operator()` FUNCSIG.
- `FUN_0052f590`: real worker behind the CTest trap (string formatting).
- `FUN_004f2b40`: possible shared `CRepairPartTree::GetDamageSeverity`
  worker (called by 6 wrappers @ 0x5afcc0, 0x5b02e0, 0x5b0320, 0x5b0520,
  0x5b05b0, 0x5b1460 all logging that name); `RepairPartTree` ctor @
  0x5b03a0.
- `CSigDriveLine::operator[]` inlined at 6 sites (0x474150, 0x475ea0,
  0x4762a0, 0x50a530, 0x50ad40, 0x50b300).
- `FUN_00499b60`: versioned replay loader (`LoadReplay::Version%u`).
- `FUN_00625c10`/`FUN_00626870`: plugin DLL load (`RBR_CreatePlugin`)
  + `Plugins`-dir enumeration ("No plugins loaded").
- `FUN_00472ff0`: "class RBR BlankScreen" splash show/hide.
- `FUN_004732f0`: unhandled exception filter (`CRASHDUMP.TXT` +
  MiniDumpWriteDump); `FUN_00473280`: launches `RBR Startup.exe` via
  CreateProcessA (the Settings/RunStartup path in WinMain).
- `CPC_Application::PlatformInit` callee of the try block at ~0x44e1cf
  (string only in the catch handler).
- `CSMLModel::CSMLModel` @ 0x5eb350 (own name passed as allocator tag,
  but the allocator call shape is unclear).
- `FUN_00447200`/`FUN_00447240`: real `LoadCar`/`SetPDebug(?)` workers
  behind the Lua bindings; `FUN_00448210` behind Lua `SetCamera`;
  `FUN_004def60` behind Lua `SetAppState`; `FUN_004491f0` behind Lua
  `SPI`; sound-param setters `SPF`/`SPB` nearby.
- `FUN_0040e000`/`FUN_00416ca0`: texture allocator family taking a
  `Class::Method` debug-tag string (Ghidra's boundary/signature for
  0x40e000 looks wrong — varargs or multiple entries).

## Entry points / key addresses cheat sheet

WinMain 0x473490 · CApplication::Tic 0x4804f0 · TicPhysicsRecording
0x47f420 · Draw 0x47e1e0 · CPhysics::StepForward 0x5273a0 · Lua bindings
0x438850–0x43afd0 · tolua/Lua 0x6d8000–0x6e98ff · IRBRGame stub
0x626a70–0x626d90 · Emit instantiations (scattered, see TSV) · NPhysics
0x47b2b0–0x53f230 · SOUND 0x4cea00–0x4dd9f0 · NMrReplay 0x609c90–0x621330
· D3DX 0x63–0x6c · vorbis 0x6f5–0x70c · CRT 0x70c284–0x72874b.

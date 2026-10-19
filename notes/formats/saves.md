# Save files (`SavedGames/`)

`CPC_SaveFileManager` (0x452760–0x453fd0) writes and reads the game's saves in `./SavedGames/` (replays in
`./Replays/`, ghosts in `./GhostCars/`). Rust: `rbr-formats::saves` (`profile`, `season`, `results`).
Checked byte for byte against the files the original 1.02 wrote in the vanilla harness install
(`cachy:~/Games/rbr-vanilla-1.02/SavedGames/`, copied to `original/saves-1.02/`; tests read them through `RBR_SAVES`).

## Names and kinds

`E20MakeName` (0x49e230) builds the names from a kind:

| Kind | Name | Encrypted | Content |
| --- | --- | --- | --- |
| 1 | `pf<name>.rbr` | yes | the profile (below) |
| 2 | `<name>.rpl` | no | replay (`./Replays/`) |
| 3 | `<car>slot<n>_<name>.lsp` | no | car setup slot: the setup file's text, `(("CarSetup" ...))`, zero-padded to 4 bytes (`E16_BuildSetup`); read decrypted when it does not start with `((` (`LoadFromMemory` 0x48d940). Rust: `saves::setup_slots` |
| 4 | `pf<name>.rst` | no | `pfresult.rst`, the records table (below) |
| 5 | `<name>_<map:02>.ghc` | no | ghost (`./GhostCars/`, `rbr_formats::ghost`) |
| 6 | `pf<name>.acm` | no | controller action map (`AMAP`, device GUIDs and bindings; below). Rust: `saves::action_map` |

`CPC_SaveFileManager::Vf14` (0x4528c0) encrypts kind 1 before `WriteFile`; `Vf18` (0x452a80) decrypts after reading.

## Encryption and checksum

- `CFileEncryptor::Encrypt` (0x55cc70): `b[i] = ~((b[i] + 0xa0) ^ key[i % 500])`; `Decrypt` (0x55ccd0):
  `b[i] = (~b[i] ^ key[i % 500]) + 0x60`. The 500-byte key is at 0x7d12c8 (`saves::KEY`, checked against the exe).
- `Crc32::Run` (0x490c00): `crc = ~0; crc = crc >> 8 ^ t[(b ^ crc) & 0xff]; return ~crc`. The table builder
  (0x490c40) computes the CRC-32 (0x04c11db7) entry, stores it, then overwrites it with the bit reversal of the
  *index* (`mov eax, edx` at 0x490d40 loads `i`, not the entry). So `t[i] = reverse32(i)` and the checksum is not the
  standard CRC-32. Both saves store it in their first word, over the rest of the fixed-size buffer.

## Stream conventions

Every value is a little-endian word (bytes, `u16` and `bool` are widened). Tags are written as four separate bytes;
in the table below they are given as the bytes on disk (`ITPO` is the constant `'OPTI'`). Writers fill the buffer
with zeros after the end tag and put the checksum in word 0. A stream object has the cursor at +4 and a counting
flag at +8 (non-zero: advance without writing).

## Profile `pf<name>.rbr`

0x26000 bytes. `E20WriteProfile` (0x49ec70), `E20ReadProfile` (0x49ea80).

| Part | Writer | Reader | Layout |
| --- | --- | --- | --- |
| header | | | checksum over 0x25ffc bytes; version 5; game mode (`M00565470`, 3 = championship) |
| menu settings | `E66MenuSettings::Write` 0x5862f0 | 0x586410 | version 3, then 11 blocks (below) |
| controls | 0x55a640 | 0x55a7c0 | `FNOC`; +0x4, +0x58, +0x5c, +0x4c, +0x50; 8 keyboard keys (+0xc..+0x28, DIK codes) |
| profiles | `E69ProfileManager::SerializeB` 0x59a770 | 0x59a860 | `FORP`; count (≤ 4); selected (+0x11b8, skipped on read); each profile (below) |
| input filter | `E16Filter::WriteBin` 0x48c0a0 | `ReadBin` 0x48bd60 | `TLIF`; 5 channels × 4 cells × (channel selector, 9 cell words); `ELIF` |
| camera modes | 0x567d40 | 0x567e90 | `PMAC`; 4 words at gameflow +0xc98 → +0x17840 |
| season flag | | | 1 while a season is under way (`E80Daemon::M005d2d20`), else 0 |
| season | `CAISeason` 0x5d3e40 | 0x5d5330 | see below (only with flag 1) |
| after the season | | | word 0xfffffffe (read into `M005654a0`); car setup slot (`0x48a540` / `SetSlot`); damage carried (`0x567930` / `CGameflow::ReadDamageHandler`); setups (`CCarSettingsManager::ApplySetup` / `LoadStream`) |
| end | | | `PFOE` (`'EOFP'`; the reader does not check it), zeros to 0x26000 |

Menu settings blocks, in the order `E66MenuSettings::Write` calls them (`SCareerPageSettings`, `SMemoryUnitSettings`,
`SMultiplayerSettings` and `SReplaySettings` are called but write nothing; `SSeasonSettings`, physics debug and
developer settings are not called). Member offsets are in the settings object; `f` marks floats (movss in the writer).

| Block | Writer | Words |
| --- | --- | --- |
| `SQuickRaceSettings` | 0x586fc0 | +0x58 (read clamps to 3), +0x60, then the tag `TESQ` last |
| `SOptionsPageSettings` | 0x586cc0 | `ITPO`, +0x10, +0x14, +0xc |
| `SDualShockSettings` | 0x586e60 | `LAUD`, +0xc |
| `SControllerGenSettings` | 0x5871a0 | `TNOC`, +0xc..+0x20 |
| `SPacenoteSettings` | 0x588420 | `ECAP`, +0xc..+0x30, +0x38, +0x34 f, +0x3c, +0x40 f |
| `SDisplaySettings` | 0x587480 | `PSID`, +0xc..+0x38, +0x3c f, +0x40, +0x44, +0x50, +0x48, +0x54, +0x58 f |
| `SCameraSettings` | 0x587b40 | `EMAC`, +0xc, +0x10, +0x14, +0x18, +0x20, +0x28, +0x2c, +0x30, +0x34 (all f), +0x1c, +0x24 |
| `SSoundSettings` | 0x587ff0 | `NUOS`, +0xc..+0x24 f, +0x28, +0x2c, +0x38 |
| `SGearSettings` | 0x588f50 | `RAEG`, +0xc..+0x18 |
| `SForceFeedBackSettings` | 0x5891d0 | `RAEG` (the gear tag again), +0xc, +0x10 f, +0x14, +0x18 |
| `SPCControllerSettings` | 0x588de0 | `NOCP`, +0xc, +0x10 |

Named words (the options pages' items, 0x593a20-0x596877: each item pushes its localisation key and the word's absolute
address in the settings object at 0x16607f8; texts from `bstrings.eng`). The rest keep offset names in Rust.

| Block | Word | Key | Text |
| --- | --- | --- | --- |
| pace notes | +0xc | `PN_STACK` | Pacenote Stack |
| | +0x10 | `CHKP_TIMES` | Checkpoint Times (the split panel, `0x571730`) |
| | +0x14 | `IG_CLOCK` | In-Game Clock |
| | +0x1c | `AID_MARKER` | 3D Pacenotes |
| | +0x20 | `PACENOTES` | Pacenotes |
| | +0x28 | `DISTANCE` | Distance |
| | +0x2c | `DIST_CNTDWN` | Distance Countdown |
| | +0x34 f | `CALLOUTD` | Callout Distance (0x1660b74: `Load3b0`'s lookahead scale) |
| | +0x40 f | (none) | the pace-note speed gate (0x1660b80, `Load3b0`); no page edits it |
| display | +0x40 | `DIGIDASH` | Digidash |
| | +0x44 | `DIGIDASH_SZ` | Digidash Size |
| camera | +0xc f | `CAM_RADIUS` | Camera Radius |
| | +0x10 f | `CAM_ANGLE` | Camera Angle |
| | +0x14 f | `VW_ANGLE` | View Angle |
| sound | +0x10 f | `ENG_VOL` | Engine Sound Volume |
| | +0x14 f | `CODRV_VOL` | Co-Driver Sound Volume |
| | +0x18 f | `EFF_VOL` | Effects Sound Volume |
| | +0x1c f | `MUS_VOL` | Music Volume |
| | +0x20 f | `TIRE_VOL` | Tyre Sound Volume |
| | +0x24 f | `RPLMUS_VOL` | Replay Music Volume |
| | +0x30 | `OSND_INCAR` | Force In-Car Sound (not saved) |
| gear | +0xc | `DEF_GEARS` | Default Gears |
| | +0x10 | `CS_NEUTRALLOCK` | Neutral Lock |
| | +0x14 | `CLUTCH_HLP` | Clutch Help |
| | +0x18 | `GEAR_PROT` | Gear Protection |
| force feedback | +0xc | `FRCFEEDBK` | Force Feedback |
| | +0x10 f | `STRENGTH` | Strength |
| | +0x14 | `DIRECTION` | Direction |
| | +0x18 | `FF_AUTOCENTER` | Autocenter Wheel |
| replay (not saved) | +0xc, +0x10 | `TELEMETRY`, `C_TELEMETRY` | Telemetry, Comparative Telemetry |

The vanilla profile holds callout distance 4.0 and speed gate 25.0, the values the drives' snapshots show at
0x1660b74/0x1660b80.

Profile entry (`E69Profile::SerializeA` 0x59a380, `DeserializeA` 0x59a4d0; the object is 0x46c bytes):
name (+0x4, 32 bytes); 17 car unlock levels (+0x424); +0x468; 15 counters (+0x88, `u16`; `E69Shorts::WriteProf`
0x598f90 writes three inline and four `WriteS3` calls of three); record count (+0xa8); 37 records of five words (+0xac,
0x18 stride: id, word, three floats; the sixth word is zeroed on read); the marker 100; 100 flag bytes (+0x24); `FORP`.

The input filter's channel stride is 0x94 (selector word + four 0x24-byte cells), so the grid in `E16Filter` starts
each channel with its selector; `rbr-gameplay::filter::Cell` names the nine cell words. The writer stores the
selector before each of the four cells; the reader keeps the last.

## Season (inside the profile)

`CAISeason` write 0x5d3e40, read 0x5d5330 ("Invalid file format (RBR7 magic number not found)", "Invalid file
version!"). Offsets are in `E80Daemon` (the object at `*0x7d6128`).

| Part | Layout |
| --- | --- |
| magic, version | `7RBR` (`'RBR7'`), 0x13 |
| state | byte +0x4 (as a word); words +0x8e84..+0x8e9c, +0xa0c0, +0xa0c4, +0x8ea4..+0x8eb4 (14). +0x8e84 is the rally's assistance level (`E60Flow::M566640`), +0x8e88 the rally index, +0x8e8c's low byte the stage index |
| drivers | +0x24, 14 × 0x90c (`E80Entry`) |
| tables | +0x8 (0x1c), +0x7ecc (0x38), +0x7f04 (0xf48, `E80Blk28c` blocks), +0x8e4c (0x38) |
| globals | 0x1660804 (map), 0x1660808 (car), 0x1660824, 0x1660c4c..0x1660c6c, 0x1660814 |
| snapshot | `E80Daemon::Ser14b` 0x5d2e70: 14 words |
| events | `NEVE`; count + 6-word items (0x7d614c +0x8); count + 6-word items (+0x18) |
| pairs | `CCCA`; count + 2-word items of the vector at +0x8ea0 (count 0 when the pointer is null) |
| stores | per driver with id (+0x8f8 = entry +0x8d4) > -1: team store (0x5c8e80 → 0x5cd510); per driver: AI store by entry +0x904 (0x5c8ee0 → 0x5ccef0); 7 rally stores (0x5c8eb0 → 0x5d6b40); a closing tag. Not typed in Rust yet |

Driver entry (`E80Entry`, 0x90c): 6 rallies × `E80Sub` (0x178: 6 stage records, +0x168 the rally's total time,
+0x16c, +0x170, byte +0x174); byte +0x8d0; id +0x8d4; +0x8d8; name +0x8dc (32); +0x8fc, +0x900, +0x904 (AI index),
+0x908. Stage record (`E80Rec`, 0x3c): sectors +0x0..+0x8 (`M005d15b0`), time +0xc (`M005d14d0`), four 8-byte
elements +0x10, count byte +0x30, float +0x34, +0x38 set to 1 for a retire (time -1.0, the f32 at 0x795e20; the
rally total becomes -1.0, otherwise the time is added to it).

## Records `pfresult.rst`

0x4000 bytes, not encrypted. Written by `CMenuSaving::WriteSave` (stored through the save manager's kind-4 slot as
`result`), read at start-up by 0x5859f0 → `CMenuSaving::ReadFromBinStream` (0x585c10): checksum over 0x3ffc bytes,
`UNEM` (`'MENU'`), count 0x25, 37 records of 0x34 bytes (id, word, three floats, a 32-byte name; the bytes after the
name's NUL are whatever the buffer held), `UNEM`. Each record goes to `M00567330`, which stores it in the gameflow's
table; `M005653b0` hands them back for writing. The retail defaults are "Richard Burns" entries.

## Action map (`pf<name>.acm`, kind 6)

`E28FileMap::Save` 0x4c3ad0, `Load` 0x4c5800 (`rbr_formats::saves::action_map`). Not encrypted, not checksummed, no
fixed size. `PAMA` (`'AMAP'`), version 0x300, record count; per record: device count, binding count, the devices'
DirectInput instance GUIDs (16 bytes each), the bindings (device index, the object's `DIDFT_*` type word, action index).
`Load` ignores a file with another tag or version. `FillMaps` (0x4c4da0) writes one binding per bound action slot of
the 21 actions (`CActionMap`'s switch: `STEERING`, `LEFT`, `RIGHT`, `THROTTLE`, `ACCELERATE_AND_BRAKE`, `BRAKE`,
`HANDBRAKE`, `GEARUP`, `GEARDWN`, `CH_CAM`, `PAUSE`, `CLUTCH`, `IGNITION`, `REVERSE`, `NEUTRAL`, `FIRSTGEAR` ..
`SIXTHGEAR`). The vanilla `pfthomas.acm`: one record of three devices (two game controllers, then `GUID_SysKeyboard`)
and 12 bindings: device 1 holds STEERING (axis 0, with the force-feedback actuator flag), THROTTLE (axis 2), BRAKE
(axis 3), CLUTCH (axis 1) and the buttons HANDBRAKE, GEARUP, GEARDWN, CH_CAM, PAUSE and IGNITION; LEFT and RIGHT are the
keyboard's arrows (`DIK_LEFT` 0xcb, `DIK_RIGHT` 0xcd); device 0 has no binding.

## Not covered

- A golden file with a season: producing one needs the championship menus on the vanilla install (start a season,
  drive a stage, save), which no harness script does.
- The season's stores after the pairs, and the damage and setup blocks after the season: kept as bytes
  (`SeasonSave::rest`).
- The menu words without a page item (pace notes +0x18, +0x24, +0x30, +0x38, +0x3c; most display words; camera
  +0x18..+0x34; sound +0xc, +0x28, +0x2c, +0x38; the quick race, options and controller blocks).

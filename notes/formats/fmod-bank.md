# FMOD Studio banks (RSF cars' sound)

RSF ships each car's sound as an FMOD Studio bank pair, `AudioFMOD/<audio>.bank` and `<audio>.strings.bank`
(`<audio>` is `cars.json`'s `audio`), which NGP 7's `NgpFmodDaemon.exe` loads (`lead/reports/rsf-audio.md`). The
57 banks of the samples are plain banks (`RIFF....FEV `), none encrypted. Reader: `rbr_formats::fmod`; player:
`rbr_sound::fmod`. Everything below is read off the sample banks (ours); no FMOD code was traced.

## Layout

A RIFF file of chunks and `LIST`s. An object's body starts with its 16-byte id. A list inside a body is a u16
`count * 2 + 1` (an empty one: 1 with a u16 element size after it, or 0 without) and a u16 element size.

| Chunk | Body |
| --- | --- |
| `PRMB` (in `LIST PARM`) | parameter: id, 8 bytes, u16 name length, name, f32 min, f32 max, ... |
| `WAV ` | waveform: id, u16, u32 sound bank (`SND ` chunk, in file order), u32 sample in it, u32 |
| `LIST WAIT` | waveform instrument: `WAIB` (id, waveform id), `INST`, `PRPS` |
| `LIST MUIT` | multi instrument: `MUIB` (id), `PLST` (u32 mode, u32, list of {instrument id, f32 percent}), `INST`, `PRPS` |
| `INST` (123 bytes) | f32 volume dB at 16, f32 pitch semitones at 20, auto-pitch parameter id at 53, its reference value at 69 and the pitch at 0 at 73, the track id at 83 |
| `LIST GBUS` / `IBUS` / `RBUS` / `MBUS` | a bus of the mixing tree: an event's track or a mixer group (`GBSB`: id, u16, parent id), an event's input to the mixer (`IBSB`, the same), a return (`RBSB`, the same), an event's master track or the master bus `bus:/` (`MBSB`: id); each with `BUS ` and `PRPS` |
| `BUS ` | u8, u32, two lists of effect and send ids (taken as before and after the fader; an empty list is just its u16 `1`), u16 8, f32 fader dB, ... |
| `LIST SEFF` | a send: `SEFB` (id, return bus id, f32 level dB, u8) |
| `LIST BEFF` | effect: `BEFB` (id, u32 `FMOD_DSP_TYPE`, u8), `PMEF` (u32 count, per parameter a u32 type and its value: 0 f32, 1 i32, 2 a u8 bool, 3 data; then a byte), `PRPS` (property `1000 + k` automates parameter `k`). The reader stops at the first bool or data, except for the convolution reverb: its impulse response (3) is followed by the wet and dry levels (f32) and the linked flag, read from the end |
| `PROP` (in `PRPS`) | u32 property (0 volume, 1 pitch, 4 instrument volume, 11 and 1000+ effect ones), u32, `MAP` id, list of automation ids |
| `CTRL` | automation: id, target object id, curve id, u32 property |
| `CURV` | curve: id, id, list of {f32 x, f32 y, f32 shape, u32} |
| `MAP ` | value map: id, list of {f32 from, f32 to} (volume: the fader's -42 is -80 dB; pitch: -1..1 is -24..24 semitones) |
| `LIST PMLO` | parameter sheet: `PMLB` (id, parameter id, list of {instrument id, f32 start, f32 end, u8}) and `CTRO` (the automations over its parameter) |
| `EVTB` | event: id, 32 bytes, its input bus id at 48, its master track id at 64; its sheets' ids appear in its body (offsets vary with its lists); its user properties, a list (`u16 2 * count`, u16 24) of {u16 name length, name, f32 value}: every bank has one, `fluctuation_length`, on `engine_int`, `engine_ext` and `transmission` |
| `SND ` | padding, then an FSB5 sound bank (one bank has two: Vorbis and PCM16) |

FSB5: version 1 header 0x3c bytes; per sample a u64 (bit 0 extra chunks, 1-4 rate index, 5-6 channels 1/2/6/8,
7-33 data offset / 32, 34-63 frames) and chunks (u32: bit 0 more, 1-24 size, 25-31 type; 2 rate, 11 Vorbis
setup CRC32 and seek table). Codec 15 is FMOD's Vorbis: u16-sized audio packets, no Vorbis headers; the setup header
is FMOD's encoder's, found by the CRC (the 7 the banks use are in `rust/rbr-sound/data/fsb-vorbis/`), the
identification header is rebuilt (block sizes 256 and 2048).

The strings bank's `STDT`: u32 1, a list of 8-byte radix-tree nodes (u24 piece offset in the pool or 0xffffff, u8
first char, u24 first child, u8 child count), a list of 16-byte ids, u16 pool size and the pool (NUL-ended pieces),
u16 count and a u24 per id: the node ending its path (`event:/engine_int`).

## Events (all 57 banks)

`engine_int`/`engine_ext` (sheets `rpms`, `throttle`, `fluctuation`, automations on `Event Cone Angle`, `Distance`),
`turbo` (`boost`), `transmission` (`wheel_speed`, `brake`), `gear_int`/`gear_ext` (`gear`, `state`),
`blowoff_int`/`ext` (`valve`), `backfire_int`/`ext` (`exhaustpop`), `ignition` (`startup`, `battery`), `wind`
(`car_speed`), `wheel`, `skid_int`/`ext`, `gear_grind`. The engine is crossfaded loops on the `rpms` sheet: each
instrument's region, its fade curves (property 4) and its auto-pitch reference (the loop's recorded rpm); the
tracks take dB automations over throttle, rpm, the cone angle and distance (on and off throttle tracks).

Effects on the engine, turbo and transmission tracks (all 57 banks; DSP type: banks using it in `engine_int` /
`engine_ext`): `LOWPASS` 3 (4/0), `HIGHPASS` 5, `FADER` 7 (3/3), `DISTORTION` 9 (1/1), `PARAMEQ` 12 (2/0),
`SFXREVERB` 19 (31/17), `LOWPASS_SIMPLE` 20 (2/56), `HIGHPASS_SIMPLE` 26 (0/9), `PAN` 27, `THREE_EQ` 28 (34/29),
`LOUDNESS_METER` 30, `CONVOLUTIONREVERB` 32 (1/25), `CHANNELMIX` 33. The usual automations: `LOWPASS_SIMPLE` cutoff
over `Distance` (22 kHz to 55 m, 2.2 kHz at 250 m, 80 Hz at 1500 m in Fiesta_Rally2), `HIGHPASS_SIMPLE` cutoff and
`SFXREVERB` wet level over `Distance`, `THREE_EQ` gains over `rpms`, `throttle`, `Distance`.

The mixer is in each car bank: `bus:/` with groups `bus:/internal`, `bus:/external`, `bus:/common` and one group per
event under them (`bus:/internal/engine_int`, ...); each event's master track goes into its input bus (`IBUS`), and
that into its group. `VCAS` and `SNAS` are empty in all 57 banks (no VCAs, no snapshots).

Not read: modulators, curve shapes, timelines, nested events.

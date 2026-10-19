# Stage gameplay markers (start, splits, finish, end, pace notes)

Verdict: the markers are keys of the **`Driveline` animation set in each
stage's `.dls`** file, as 12-byte records `[u32 tag, u32 word1, f32
distance]`. They are not in `.trk`, `.col`, the stage `.ini` files or the Lua
scripts. Parser: `rust/rbr-formats/src/markers.rs` (`markers::parse`); retail
test `tests/markers.rs` (gated on `RBR_DATA`). Addresses are for the official
1.02 `RichardBurnsRally_SSE.exe`; decompile in
`out/decomp/RichardBurnsRally_SSE.exe.c`.

Evidence levels as in `col.md`: **loader** = read from the exe code; **retail**
= checked over the 86 retail `.dls` files; **inferred** = not proven.

## Where the markers are not

- `Maps/Tracks.ini`: `[General]` extension keys and `[MapNN] TrackName`
  (+ `Particles`) only.
- `Maps/track-<NN>_<X>.ini`: `[TextureInfo]` only (all 88 files).
- `Maps/TrackSettings.ini`: per-stage lighting, fog and sky values.
- `Root/Scripts.rbz` (749 `.lua`): animation and crowd loaders
  (`load_marshal_*`, `load_start_race01*`, `load_end_race01*`, …). A search for
  `checkpoint|split|finish|pace` finds nothing that places a marker.

## The `.dls` key list

Table/record layout is in `notes/formats/stage-files.md` (`.dls`). The
marker data is one of the 17-word animation records:

- Name: `fields[0]` is an offset into the table-7 string block. Retail
  entries are named `Driveline` (`FUN_004789b0`, `_stricmp`). Retail: 86 of
  86 files have it.
- Key list: count word `fields[6]`, offset word `fields[14]`, records of 3
  words. `FUN_00479020` (`0x479020`) copies them into a 12-byte-stride array
  at animation `+0x24` (count `+0x20`) and sorts it with `qsort(...,
  FUN_00474760)` (ascending float at `+8`). Loader evidence. Retail: 155 keys in
  track-10_O; 0 to 286 across the 86 files.
- `DlsAnimationEntry::arrays[4]` is this array (count word 6, offset word 14,
  3 words per record).
- Record in the file: `word0 = tag` (u32), `word1` (u32), `word2 = distance`
  (f32 bits). After loading, the tic sets bit 31 of `word1` once a key has
  fired (runtime only). Retail word1 of start/finish/split/end keys is always
  zero; pace-note word1 takes values such as `0x400`, `0x404`, `0x408`, `0x20`
  (unknown meaning; not resolved).

Retail keys by tag (all 86 files, 9,147 pace-note keys in total):

| Tag | Name used here | Files | Keys |
|---|---|---|---|
| `0x15` | start line | 82 | 1 each |
| `0x17` | split/checkpoint | 59 | 2 each |
| `0x16` | finish line | 74 | 1 each |
| `0x18` | end of stage | 67 (one has two) | 1 each |
| other (`0x00`–`0x20` minus above) | pace-note callouts | 61 | 9,147 total |

Stages without a `0x15` key: `track-21_O`, `track-71_M`, `track-79_M`
(`track-68_M` has no keys at all). `track-4_*`, `track-69_*` (9 files),
`track-72_M` and `track-85_M` have only a start key. Those with splits have
exactly two (retail).

## Units: distance along the driveline

The key distance is on the same axis as the driveline sample that the car
and the tic use:

- `E09_LoadDriveLine` @ `0x46a530` (loader, reads `.trk` kind `0x14`) emits
  `CSigDriveLine`; `NPhysics::CreateDriveLine` @ `0x50b970` builds the
  `CDriveLine` singleton (`0x1659184`).
- `E12_SampleX` @ `0x475ea0`: `floor(x)` index, linear interpolation of the
  float channel (`+0x30` in the sampler's stride arithmetic). It maps a
  driveline parameter to the axis value `x`. The `.trk` record `value` is
  monotone: it starts at `0.0` in all 86 stages and ends at the last driveline
  record (track-10_O: 0 to 6546.9).
- The tic `FUN_004783b0` (`0x4783b0`) takes `x = SampleX(...)`
  (`FUN_00478930` @ `0x478930` passes it). The timekeeper `f` is
  `SampleX(car+0xac)` (`FUN_0050a4f0` @ `0x50a4f0` jumps to `0x475ea0`),
  called from `NPhysics::CCar::Vf6cx` @ `0x4ea300`.

So the keys are metres along the driveline (the unit is inferred from the
stage lengths; the code only compares them with `x`). Retail check: every
start/finish/split/end key lies inside `[first, last]` of its `.trk` driveline
values. Pace-note keys can lie past the last value: track-42_M/O has two keys
3.4 m beyond it (tags `0x05`, `0x0d`).

The `.trk` value to CDriveLine-entry copy was not traced: `SetDriveLine`
(`0x50aec0`) decompiles to its log call only, so the store is unresolved
(inferred from the range and monotone checks above).

## What consumes the keys

### Tic dispatch (`FUN_004783b0`, `0x4783b0`)

The tic walks the current animation's key list (`t+0x20` count, `t+0x24`
records) from the last key `<= x` backwards. Each key fires once. Loader C++
name: `E12_AnimSet::Load3b0` (`src/rbr/RBR_E12_Load.cpp`).

- `0x15` (start line): calls the game host (`M66240`, `E62_M572fc0`,
  `M79430`); logs "Driving over the start line in the start scene is not
  good" when the app state is 10. Start-line effects beyond that were not
  traced.
- `0x16` (finish line): emits `CSigPaceNoteCallout` (signal at
  `g_signalManager+0x6ac8`). It also emits `CSigEndRace` (`+0x7970`) unless
  `E59_00565e80()` is nonzero (then it sets a menu flag and returns).
- `0x17` (split): calls `E12GameHost::M72da0(DAT_0165fc68+0x140)` and emits
  `CSigPaceNoteCallout`. This is the split's pace-note call, not a timing
  event. The timing is in the timekeeper (below).
- `0x18` (end of stage): emits `CSigPaceNoteCallout`, then `CSigEndRace`
  unconditionally.
- Other tags: a forward pass from `E12_KeyList::Find(x)` (`0x474c20`) with
  lookahead `f = 0x1660b74 * d + x` emits `CSigPaceNoteCallout` when that
  reaches the key and `g_1660b80 < d * 3.6` (`d` = `E12_SeekD`, speed-like;
  inferred).

### Timekeeper (`NRegulation::CTimeKeeper`)

- `CTimeKeeper::Load` @ `0x4a01f0` (loader for the `Driveline` set, after
  `FUN_004789b0`): `0x15` into the card's start field (`+0x30`), `0x16` into
  the finish field (`+0x48`), `0x17` pushed in order into a 16-entry list
  (`+0x60`, stride `0x18`). The loop writes `entries[count]` and stops at 16.
- `CTimeKeeper::Check` @ `0x4a0450` starts timing when `f > start` and emits
  `CSigCheckPointTime` (multisignal at `g_signalManager+0xc900`).
- `CTimeKeeper::Record` @ `0x4a0540` is called once per car step from
  `NPhysics::CCar::Vf6cx` (`0x4ea300`) with `f = SampleX(car+0xac)`. It
  emits `CSigCheckPointTime` when a split slot is filled and when the finish
  slot is passed.
- Open: the split threshold (`CTimeCard::Refresh` @ `0x4a0040`). The decompile
  and `src/rbr/RBR_E21.h` disagree on the names of some card fields (the
  decompile uses `m_count`/`m_a8` where the header uses `m_count2`/`m_cur`),
  so the exact per-split comparison is not stated here.

### Pace notes (tag to sound)

Pace-note tags go to `CSigPaceNoteCallout` as `{x, e, d}`, with `e` pointing
at the key record (`e[0]` = tag). The handler `E25PaceHandler`
(`0x4b65b0`; reconstruction in `src/rbr/RBR_E25_Handler.cpp`) maps:

1. tag to slot: `E25_SlotId` (`0x4b55e0`, `src/rbr/RBR_E25_PaceA.cpp`);
2. slot to word id, then word id to a bank name: `E25Handler::M5f70`
   (`0x4b5f70`, `src/rbr/RBR_E25_PaceB.cpp`), e.g. word 27 = `PACENOTE::FINISH`,
   word 14 = `PACENOTE::90_LEFT`;
3. bank name to samples: `LoadNote` (`0x4b69a0`) reads the `PACENOTE::<name>`
   section and opens `<path>.ogg` (`Audio\Speech\PaceNote\...`).

Caveat: the reconstruction and the decompile do not agree on what the handler
does with the second word (`*(tag+4)` in our code is not a valid read).
Steps 2–3 are reconstruction, unverified against the binary. The tag-to-name
table is therefore not final.

### Names from the Blender track exporter

The exporter (`notes/references/README.md`) names the tags. It agrees with
the marker tags above: 21 start, 22 finish, 23 checkpoint (split), 24 end
of stage. The pace notes are 0 hairpin left, 1 left 90, 2 K left, 3 medium
left, 4 fast left, 5 easy left, 6 easy right, 7 fast right, 8 medium
right, 9 K right, 10 right 90, 11 hairpin right, 12 twisty, 13 distance,
14 narrows, 15 wide out, 16 over crest, 17 ford, 18 care, 19 bump,
20 jump, 25 flat right, 26 flat left, 27 bridge, 28 go straight, 29 keep
right, 30 keep left, 31 keep middle, 32 caution.

Its `word1` flags: `0x1` narrows, `0x2` wide out, `0x4` tightens, `0x8`
sound file, `0x10` legacy sound, `0x20` don't cut, `0x40` cut, `0x80`
tightens bad, `0x400` long, `0x800` plus, `0x1000` minus, `0x2000` maybe,
and bit 31 processed. The `0x100`, `0x200`, `0x4000` flags and the sound
index and id fields in bits 16-30 come from later pace-note plugins, not
1.02. The retail values `0x400`, `0x404`, `0x408` and `0x20` read as long,
long + tightens, long + sound file and don't cut. None of these flag
names is checked against E25 code.

## Parser (`rust/rbr-formats/src/markers.rs`)

- `parse(bytes)` / `from_dls(&DlsFile)`: finds the entry named `Driveline`
  (case-insensitive, first match), takes `arrays[4]`, returns the keys sorted
  by distance (equal distances unordered, as in the loader's `qsort`).
- `start()`, `finish()`, `end()`: the last key of that tag in sorted order
  (the loader overwrites in the same order). `splits()`: at most 16, the
  loader's cap. `pace_notes()`: every other tag.
- Errors: `NoDrivelineSet`, `BadName`, or a `dls::Error`.
- Retail test: every marker is inside its driveline, splits lie strictly
  between start and finish, splits count `<= 16` (retail: exactly two).

## Unknowns

- Meaning of `word1` (zero for every marker key; `0x400`, `0x404`, `0x408`
  and others on pace notes, bit 31 set at runtime). Exporter names are
  above; game use not traced.
- Exact per-split comparison in `CTimeCard::Refresh` / `Record`
  (see above).
- Tags `0x00`–`0x14`, `0x19`–`0x20`: which callouts they select. The
  reconstructed table (E25 word ids 0x0d–0x36) is unverified; the
  exporter names are above.
- What the 1.02 game does for stages without a `0x15` key (`21_O`, `71_M`,
  `79_M`): the timekeeper keeps its constructor value; not traced.
- Start-line side effects in the `0x15` tic case beyond logging and the
  game-host calls.

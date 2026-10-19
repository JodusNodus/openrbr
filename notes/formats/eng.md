# Engine sound banks (`Audio/Cars/*.eng`)

The `.eng` files hold the samples of the granular engine sound: one short recording per rpm and load, with the
sample position of every firing in it. They live in `Root/audio.dat` ([audio.md](audio.md)). Addresses are
`RichardBurnsRally_SSE.exe` 1.02. Labels: **loader** = read from the decompile or our reconstruction of the named
function; **file** = checked on the 8 retail files; **inferred** = not proven. How the game plays them:
[../sound.md](../sound.md) section 4.

## Which file a car uses

`CCarSound::Vf48` (`0x4acd10`) builds `Audio\Cars\%s.eng` from the name in `Audio/Cars/Cars.ini` key `CarN`, where N
is the car index (`CCarSound::OnX` `0x4b1810`, **loader**). Retail `Cars.ini` sets `Car0` to `Car7` all to `subaru`,
so every car plays `subaru.eng` (**file**). The other seven files are unused leftovers.

## Layout

Little-endian. Readers: `CEngine::Vf04` (`0x4bc370`), `CEnginePart::Vf04` (`0x4bc0b0`), `SOUND::CWave::Vf04`
(`0x4dd6b0`, [../../src/rbr/RBR_E32_Speech.cpp](../../src/rbr/RBR_E32_Speech.cpp)) (**loader**). The readers skip
the tags without checking them. Offsets in brackets are the field in the in-memory object.

```
file:
  char[4]  "ENGI"
  u32      version            0x100 or 0x101
  u32      nbanks             CEngine +0x2cd4 ("Num diffrent rpms" in a log string)
  u32      nparts[nbanks]     pointer kept at CEngine +0x2cd8; retail always 2 (load 0 and load 100)
  part     parts[sum(nparts)] bank by bank; bank stride 0x140, part stride 0x50 in memory
  if version != 0x100:
    u32    n                  CEngine +0x105c
    u32    a[n]               table at +0x1050
    u32    b[n]               table at +0x1054

part:
  char[4]  "PART"
  u32      version            0x100
  u32      rpm                part +0x30
  u32      load               part +0x34, 0 or 100
  u32      unused             part +0x4c; uninitialised in retail (0xcdcdcdcd, "data", "labl", zero)
  wave

wave:
  char[4]  "WAVE"
  u32      version            0x100
  u32      rate               CWave +0x14, Hz
  u32      nsamples           CWave +0xc
  i16      samples[nsamples]  mono; CWave +8 points into the file buffer
  u32      ncycles            CWave +0x1c
  cycle    cycles[ncycles]    CWave +0x18

cycle (8 bytes):
  u32      whole              sample index of a firing
  u32      frac               fraction, /65536 (16.16 position)
```

Each cycle row is the start of one firing. The distance between rows is one firing period: at 2 firings per
revolution, `rate / (rpm / 30)` samples (**file**: the mean row spacing matches the bank's rpm within 1% for every
part of `subaru.eng` except the 1700 rpm load-0 part, which measures 1562 rpm).

`SOUND::CWave` converts the samples to float with 5 extra taps copied from the start (`WaveFloat` `0x4dd8e0`) for the
cubic resampler (**loader**).

## Retail survey (8 files, **file**)

All 8 parse to their exact size with the layout above, once two-wave parts are allowed for (below).

| File | Version | Banks (rpm) | Rate | Waves per part | Notes |
|---|---|---|---|---|---|
| `subaru.eng` (used) | 0x101 | 6: 1700, 3000, 4000, 5000, 6000, 7000 | 44100; 48000 for 6000/load 100 and both 7000 parts | 1 | 32-150 cycles per part; trailing table `n = 0` |
| `subaru2.eng` | 0x101 | 5: 1700, 3000, 5000, 6000, 7000 | 44100 | 1 | |
| `test.eng` | 0x101 | 6, as `subaru` | as `subaru` | 1 | near-copy of `subaru.eng` |
| `mitsu.eng` | 0x101 | 5: 2000, 3000, 5000, 6000, 7000 | 44100 | 1 | |
| `Saxo.eng` | 0x101 | 5: 2000-6000 | 44100 | 2 | trailing table `n = 3`, floats `a = {3.28, 1.36, 0.79}`, `b = {0, 0.587, 1.0}` |
| `bubbla.eng` | 0x101 | 6: 1850, 2600, 3135, 3868, 5000, 6000 | 44100 | 2 | |
| `fiat.eng` | 0x100 | 6: 1250-6000 | 48000 | 2 | no trailing table |
| `vaux.eng` | 0x101 | 6: 900-6000 | 22050 | 2 | 30 cycles per part |

In `Saxo`, `bubbla`, `fiat` and `vaux` every `PART` holds two `WAVE` blocks back to back. The 1.02 reader reads one
wave per part, so it would take the second wave's tag as the next `PART` and misread the file. These four files come
from an older format and cannot be loaded by 1.02 (**loader** plus **file**). In `Saxo` and `fiat` the load 0 and
load 100 parts are identical.

The meaning of the trailing `a`/`b` tables is not traced: only `Saxo.eng` has entries, and they look like float
curve points (**inferred**).

## Throwaway parser

`/tmp/snd/eng.py` (not committed) implements the layout above and the survey. A Rust parser would go in
`rbr-formats` next to `dat` (see [../sound.md](../sound.md), Rust plan).

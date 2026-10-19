# `misc/times.ini` (default stage reference times)

Verdict: `Root/misc.rbz` holds `Misc/times.ini`, the reference times for the quick race and such: for each of the
37 stages, two split times and a finish time. Parser: `rust/rbr-formats/src/times.rs` (`times::read`, `times::parse`);
retail test `all_retail_times_ini_parse` in `tests/real_data.rs` (gated on `RBR_DATA`). `misc/rbctimes.ini` (Richard
Burns Challenge) is not read.

Evidence: **loader** = the exe code, `E60_5680e0` (0x5680e0, `src/rbr/RBR_E60_Times.cpp`) and `E57Tokenizer::Parse`
(0x55b860, `src/rbr/RBR_E57_DashGS.cpp`); **retail** = checked over the file in `Misc/times.ini`.

## Layout

- Comments: `;` to the end of the line (loader: the tokenizer skips them, retail: the header and every block have some).
- The file ends with `$` (loader: the reader scans to it; retail: the last line is `$`).
- First token: the stage count, `37` (loader: `atoi`; retail: 37 blocks).
- Each stage block: the stage number (the `NN` of `Maps/track-NN_<X>.ini`), then three times: `CheckPoint 1 Time`,
  `CheckPoint 2 Time`, `Finish Time`.
- A time is `mm:ss:cc` (loader: `E57Tokenizer::Parse`): `(float)mm * 60 + (float)cc * 0.01 + (float)ss`, in f32.
  A token without two colons is read as a plain number.
- Retail stage 54 (`track-54_E`): `00:57:00`, `01:59:00`, `03:05:00`, that is splits 57.00 s and 119.00 s and a
  finish of 185.00 s.

## The loader's row

Each stage is written into the gameflow's table at `DAT_0165fa90 + 0x10` as 13 dwords: the stage number (at -8), the
constant 5 (at -4), the two splits and the finish (the first three words of the row), then the name `Richard Burns`.
What the other dwords hold is not traced. Ours reads only the stage number and the times.

## Use

- `rbr-standalone` reads the stage's reference (`StageData::reference`) from `Root/misc.rbz` and prints the split
  differences in the boot report; the viewer shows them (`rbr-render/src/hud/splits.rs`).
- The reference times include penalties? Not traced. The split comparison is ours (`notes/progress/systems.md`).

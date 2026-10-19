# Tyre port: plan and names

Scope: the tyre functions the reference drive reaches (`notes/progress/called.csv`, subsystem `tyre` of
`scripts/progress.py`: `RBR_E48_TyreMenu` CTyre code, `RBR_E49_CTyre`, `RBR_E49_TyreSim`, `RBR_E49_TyreStep`).
Excluded: the UI and tweak menu code in `RBR_E48_TyreMenu` (`CBitfieldMenuItem`, `CSliderMenuItem`, `CDeadAccessors`,
...), the spring-damper (`src/spring_damper*`), and the entities. 41 functions are reached; `ContactPatch`'s
callees and `Align`'s helpers are the exe's, not ours.

The port is built leaf first. A step is done when its vector test passes on every recorded call of the
original (`harness/vcu_vectors.py --set tyre`, `tests/tyre_vectors.rs`). `layout/` is the only code with raw
offsets (`CScratch` `this+N`, `CState` `m_state+N`, `CDef` `m_def+N`).

## Frame

The `Step` chain works on a `layout::Frame` (`Scratch`, `StateWords`, `Setup`). `load_frame` reads the footprint once
at entry, and `store_frame` writes it once at exit. Callees take `&mut Frame` and return typed values, and the
offsets are named in `layout/` only.

- The search and blend tables are `layout::Table`: their entries and the word below them (`read_table` loads both).
  A NaN search walks to index -1 and reads that word, so the view holds it.
- A port that cannot cover an input returns `Outside { index, len }`: `index` is outside the table's window (-1 and
  its `len` entries). `locate` returns it for a hint outside the window, or for a walk that reads outside it; `table_at`
  for an index outside it. The binding then runs the original. Nothing is stored on that path.
- Output pointers (the caller's vectors and out block) are written after the frame is stored, as the bindings do:
  a pointer can point into the footprint (`+0x3fc` and `+0x4e0` are targets of `ClampGrip` and `WriteOutputs`).
- The material block (`+0x1a0`, `layout::copy_material`) overlaps ten scalars: `+0x1e4`, `+0x1e8`, `+0x1f4`, `+0x1f8`,
  `+0x208`, `+0x218`, `+0x228`, `+0x258`, `+0x288`, `+0x298`. The copy sets them too, and `store_frame` writes the block
  first, so a scalar that a later write changed still wins.
- Tests replay each call on its recorded windows, with zeros over the rest of the footprint (`map_frame`). A call whose
  record has no window over an input the port reads (`SetState48` reads the def's `+0x7c` and `+0x8c`, and no record
  holds them) is counted as declined, not checked.

## Gaps for pure Rust

- The scratch's material flags (`CBitField`, at +0x14c of the material) hold a pointer that the destructors call through
  when it is not null. The records take the null case (their flags words are zeroed), so the non-null branch is not
  ported or verified. It is a host call in the original (`CBitField::~CBitField`, 0x52f8d0 calls the owned object's
  vtable slot 0). Pure Rust needs that object's release.
- Step and Locate decline on indices outside their table's window (a NaN walk that reads past -1, or a hint below -1).
  The fuzz records reach a few of these (3 Step calls and 3 Locate calls in `c-step2.txt` with the drive set); the
  test asserts that each declined index is outside the window. Pure Rust will need the def's raw words there.
- HeatStep reads the mix and shape words' indices with no check (`load_heat`), so an index past its table (25 calls in
  the drive and Step-fuzz records) is declined by the test, not by the port. The original reads the snapshot there.

## Not ported

- Groups 2 and 3 (tyre helpers the `Step` does not call): not ported now. The drive does not reach them; a port follows
  the first drive or direct-call record that does.
- Group 4: the E48 TyreMenu UI and `CSoundFXSignal::Emit`. Out of scope: UI and sound are not gameplay (AGENTS.md).
- The dead blocks (`CDeadAccessors` 0x530aa0-0x530bf0 and `DeadPoly` 0x530bd0): dead code in the lead's inventory, not
  ported.

## Lifecycle (load path)

Every constructor and tear-down of the tyre objects is ported in `layout/` (`construct.rs`, `scratch_ctor.rs`), with direct-call records: `CTyre::CDef::CDef` (0x530c50), `CopyDefFrom` (0x534760), the `CState` ctor (0x531500), `CScratch::Init` (0x5327d0), the scratch ctor (0x5328d0), `CreateScratch` (0x535220), the destructor body `CScratch::CScratch()` (0x532570), `~CScratch` (0x5329a0), `DeleteScratch` (0x5328b0), `DeleteContainer` (0x535280), and the two `NTweak::CBase` header blocks (0x5324a0, 0x532550). The constructors and tear-downs are generated default tables with their pointer words named. The scratch ctor runs `Init` on its sub-block; `~CScratch` runs the body on it. The def's release (the def's vtable slot 0x54, `CTyre::CDef::Unk54` at 0x5314e0) is a no-op, so it is not called. `CreateScratch` and the `Delete*` functions leave the allocation and the free to the host (the flags are returned as a bool). The `CDef` ctor, `CopyDefFrom`, the `CState` ctor, `CScratch::Init` and the scratch ctor are bound (identical in `bound.csv`); `CreateScratch`, the destructors, the `Delete*` functions and the header blocks are not bound.

## Record store

`scripts/rust-vectors.sh` runs `tests/tyre_vectors.rs` on `vectors/tyre/` (all files at once):

- `d-<snapshot>.txt` (21 files, `d-s*` and `d-t*`): the first drive records. They predate the Locate below-word window
  and the set_state48 def window, so their Locate calls zero the word below (`map_locate`) and their SetState48 calls are
  declined.
- `d3-<snapshot>.txt` (the same 21 snapshots, current recorder): drive records with the below-word window and the
  def window. `vcu_vectors.py <snap>.bin <out> --set tyre` with `RBR_REMOTE` set. One recorder at a time: a run that
  overlapped another on the same remote build dir once stopped with an unstubbed call.
- `c-fz5.txt`: the first fuzz records, `--tyre-fuzz 120 --fuzz 5` on the s10000 snapshot (older recorder).
- `c-fz5b.txt`: the same fuzz with the current recorder (BlendTable, RelaxCount, SetStateField, SetState48, ClearBias
  and the three getters are fuzzed too, and HeatStep's def window is 0x400).
- `c-step2.txt`: the Step fuzz, `--tyre-fuzz 300 --fuzz 7 --tyre-fuzz-addr 0x5360C0` on s10000.

The test reads every file in the directory, so the old and new sets both count. Old records are not overwritten.

- Lifecycle records (direct calls, `harness/direct_calls.py --set tyre`, 40 calls per seed, seeds 1 and 2):
  `l1-`/`l2-` `cdef` and `copydef`, `cstate`, `scratch-init`, `scratch-ctor` and `create`; `d1-`/`d2-` `block17`, `block3`,
  `body`, `dtor`, `delete` and `container`. Each file is a snapshot's direct calls from `vcu-cov.snap-10000.bin`.

## Port order

All five steps are ported (`ports.csv` has each function's module and test).

1. Leaves (no tyre callee): `force::signed_term` 0x531ba0, `tyre_clamp` 0x5322e0, `floor_s32` 0x5325f0, `compute_slip`
   0x532a30, `solve_dirs` 0x533640, `accumulate` 0x5338a0, `material::store_material` 0x535040 (`CTyreMatData::CopyFrom`),
   `friction_curve` 0x5334d0, `layout::store_414` 0x532490, `set_state_field` 0x532470, `set_state48` 0x533490,
   `clear_bias` 0x534000, `copy_def_from` 0x534760, `split_slip` 0x5346f0, `RelaxCount` 0x5358c0
   (`layout::store_relax_count` after `relax_step`), `emit_noise` 0x535890.
2. Mid level: `friction` 0x531c70, `clamp_grip` 0x533760 (ported in `grip.rs`), `table_step` 0x534480, `blend_table` 0x534e70,
   `relax_step` 0x5345a0, `heat_step` 0x534090, `solve_forces` 0x532b60 (ported in `forces.rs`), `fill_outputs` 0x532f10 (ported in `fill.rs`),
   `write_outputs` 0x533080 (ported in `write.rs`), `compute_outputs` 0x5320c0, `align` 0x531fc0 (ported in `align.rs`), `solve_a..d` 0x533ae0, 0x533bf0,
   0x533d40, 0x533e80, `solve_dispatch` 0x5351a0, `solve_pair` 0x5352a0 (ported in `grip.rs`).
3. Contact patch: `step_inner` 0x535310 (ported in `step_inner.rs`; its `E09Vec4::Clear` call 0x465d80 is
   a zeroed vector in `layout::store_step_inner`), `contact_patch` 0x5358f0 (ported in `contact_patch.rs`),
   `solve_patch` 0x535d60 (ported in `solve_patch.rs`). Raw-bit reads outside `layout/`: `solve_patch` reads
   the dword at `this+0x398` as a float (the second `TyreClamp`'s `b`); `contact_patch` copies the bits of its
   `out` float into `this+0x314`, `+0x318` and `+0x31c` (the bits, not the value). `clamp_grip` (its skip path) copies the
   dwords of the state's `+0x40` and `+0x44` into the out words (the bits, not the value). `fill_outputs` (`fill.rs`) copies the dwords at `this+0x378`, `+0x360`, `+0x314` and `def+0x1c`, and the three
   dwords of `pair`, to `this+0x43c`, `+0x444`, `+0x434`, `+0x420` and `+0x470..+0x478` (the bits, not the value).
   `write_outputs` (`write.rs`) copies `this+0x340` and `+0x388` to `out+0x90` and `+0x94`, and `out+0x88`, `this+0x36c`
   and `+0x374` to `this+0x4b0`, `+0x4b8` and `+0x4b4` (the bits, not the value). `step` (`step.rs`) stores the bits of
   the spin `a` at `this+0x378`, of `c` at `+0x3a8` and of `d` at `+0x3d8`, and copies the three dwords of `f` to
   `this+0x4c8` when the game flag is set (`layout::load_step_flag`: `g_1659b70 -> +0x24 -> +0x160e0`, bit 0x20).
   The flag word is an integer test (`testb $0x20`), not a float's bits.
4. Step: `step` 0x5360c0 (`step.rs`, the per-wheel tyre step, 16 stack words; calls the above and `CopyFrom`).
   Its table search `locate` 0x528980 is ported in `locate.rs` (cdecl; a hinted search of a `Table`, returning the
   segment; the binding loads and stores the out struct with `layout::load_segment` and `store_segment`). Locate's `lo`
   can be -1 (0xffffffff) when a NaN input walks below its table, and then reads the table's `below` word. Table reads
   go through `layout::table_at`, which returns `Outside` outside the window.
   `ContactPatch` returns its out float (`contact_patch` returns `Option<f32>`), which the binding stores at its out
   pointer when solved.
5. Constructors and init (load-time; see "Lifecycle"): `layout::construct_cdef` 0x530c50, `construct_state` 0x531500,
   `init_scratch` 0x5327d0, `construct_scratch` 0x5328d0.

## Names

Meanings are from `notes/physics/tyre.md` and `notes/physics/renames/tyre.csv`. Working names are ours.

| Name | Original | Meaning |
| --- | --- | --- |
| `Scratch.speed` | `CScratch+0x360` (`f[0xD8]`) | magnitude of the contact-plane slip, m/s |
| `Scratch.slip` | `CScratch+0x368..0x370` | slip velocity components `s0`, `s1`, `s2` |
| `Scratch.long_contact_vel` | `CScratch+0x374` (`f[0xDD]`) | `R*w + s1` |
| `Scratch.slip_ratio_term` | `CScratch+0x37c` (`f[0xDF]`) | `-s1/R - w` |
| `Scratch.spin` | `CScratch+0x378` | wheel spin rate w, rad/s (float bits in the original) |
| `State.effective_radius` | `m_state+0x4c` (`stf[0x13]`) | R |
| `State.lerp_weight` | `m_state+0x58` | weight of the lerp in `accumulate` |
| `State.temperature_k` | `m_state+0x64` | tyre temperature (kelvin) |
| `CopyFrom` | `CTyreMatData::CopyFrom` | the 39 material words, as a typed copy |
| `Scratch.f_1e8`, `factor_1f8`, `f_208`, `f_218` | `+0x1e8`, `+0x1f8`, `+0x208`, `+0x218` | ContactPatch and SolvePatch outputs (`L1`, `L2` by their formulas); the meaning is unknown |
| `Scratch.dirty_1e4`, `flag_1f4` | `+0x1e4`, `+0x1f4` | the dirty words ContactPatch sets to 1; the material block overlaps them |
| `Scratch.ratio_324`, `ratio_328` | `+0x324`, `+0x328` | the total-weight ratios of Step; the meaning is unknown |
| `Scratch.contact_p` | `+0x32c..+0x344` | the seven `p` words of the solve, by index (`p[5]` is the `t` input) |
| `Scratch.g_228`, `f96`, `heat_source`, `factor_298` | `+0x228`, `+0x258`, `+0x288`, `+0x298` | the names are by offset; `f96` is `H`, the contact offset |

## Unknown meaning

- `Scratch` factors `f[0xC3]` (`+0x30c`) and `f[0xC5]` (`+0x314`): tyre factors, used but not named (`tyre.md` 2a).
- `State` blend words (`+0x90..0xbc`) and `CDef` fields: the `CDef` names are in `tweak-keys.md`, but the offset
  map is not settled (`tyre.md` section 4, question 2).
- The `Step` argument `spin` is typed `void*` in the C++; it carries the float's bits (`notes/physics/renames`).
- `Step`'s `c` and `d` (stack words 2 and 3, stored at `this+0x3a8` and `+0x3d8` as bits): their meaning is not in the
  renames. `h` (stack word 7) is not read by the asm.

## Open questions (from notes/physics/tyre.md)

- `solve_a` and `solve_dispatch` call each other (mutual recursion); the bound is set by the data.
- `contact_patch` and `step`: the S1-S5 suspect items (`tyre.md` section 5) are behaviour, not port choices. The port
  keeps the original's values (e.g. `write_outputs` writes `0` at `+0x98`, `S5`).

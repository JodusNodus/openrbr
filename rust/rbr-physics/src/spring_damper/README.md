# Spring damper (exe E48)

Rust port of the reached `CSpringDamper` path and the `NSpringDamper` helpers in
`src/rbr/RBR_E48_SpringDamper.cpp`. The C++ reconstruction is the behavior
reference; the asm decides operation order and precision. Field meanings come
from `notes/physics/spring-damper.md` and
`notes/physics/renames/spring-damper.csv`. x87 operations use the PC=24 model in
`notes/rust-physics.md`.

## Drive-derived function plan

`notes/progress/called.csv` records 30 reached functions from this translation
unit. They are ported from leaves toward the per-step entry point:

| Batch | Original function (address) | Rust role |
|---|---|---|
| Config leaves | `CSetupOptions::Unk1C` (`0x52d710`), `CDef::Unk20` (`0x52d730`), `CDef::CDef` (`0x52d750`), `CU32Value::Clear` (`0x52d960`), `CSetup::Unk20` (`0x52d9e0`) | Tweak table access, default definition, and value reset |
| Math leaves | `DamperStep1` (`0x52e350`), `DamperStep2` (`0x52e370`), `DamperBiasSub` (`0x52e390`), `DamperAccumA/B` (`0x52e3a0`, `0x52e3d0`), `DamperClamp` (`0x52e400`), `DamperMerge` (`0x52e460`), `DamperUpdateA/B/C` (`0x52e4c0`, `0x52e4f0`, `0x52e530`), `DamperScale` (`0x52e590`), `DamperSqrt4` (`0x52e5a0`), `DamperCurve` (`0x52e5e0`), `DamperCombine` (`0x52e630`), `DamperRelax` (`0x52e6d0`), `DamperSolve` (`0x52e760`), `DamperForce` (`0x52e8a0`) | Typed scalar, accumulator, and four-value helpers |
| State and setup | `CState::SetUnk10` (`0x52da40`), `CState::CState` (`0x52da50`), `CopyStateToDef` (`0x52dbd0`), `UpdateForces` (`0x52dd70`), `CDamperSignal::Emit` (`0x52de60`), `CSpringDamper::CSpringDamper` (`0x52df20`), `ResetOutputs` (`0x52e010`) | Typed definition, setup, state, controller and signal values; byte access stays in `layout.rs` |
| Step | `CSpringDamper::Step` (`0x52e090`) | Main strut force and damping update |

Functions in the same file that `called.csv` does not reach are outside this
batch: `NormalizeTravel`, `TravelRange`, `ComputeTravel`, `StepA`,
`UpdateAndReset`, and the uncalled `NSpringDamper` aliases. The related
`NTweak::Parse` path is covered by the C++ behavior gate, but it is not part of
the spring-damper runtime port.

## Typed boundary

`layout.rs` is the only spring-damper code that reads or writes `Mem`, including
the value slots and bookkeeping words in the tweak objects. Math helpers and
controller methods take named Rust structs. The vector harness is allowed to
read original memory because it is the test boundary.

## Float representation audit

The 30 reached functions were checked for integer operations on an existing float's raw bits
(`movd`/integer tests, sign-bit masks, or `copysign`). None reads a float that way. The
`CState::SetUnk10` entry at `0x52da40` takes an integer argument and stores those supplied bits
as the effective rebound-damping float; `layout.rs` expresses that input conversion with
`f32::from_bits`. It does not inspect an existing float's representation. The Rust port has no
float-to-integer conversions.

Unknown meanings that need better names when evidence appears:

- `CState`'s owner word is read as a wheel pointer by reset, but the constructor
  and setter expose it as an integer.
- The two header words copied with each setup value are bookkeeping; their
  individual meanings are not established.
- `CSpringDamper`'s scratch pair, high-speed hold counter, and signal peak/stop
  velocities are described by their uses in the asm; their downstream consumer
  is not traced.
- `DamperAccumA/B`, `DamperCombine`, `DamperRelax`, `DamperSolve`, and
  `DamperForce` operate on named weighted values, but the physical quantity of
  each helper's accumulator is not known.

## Call vectors

`harness/vcu_vectors.py --set spring-damper SNAPSHOT OUT` records the original's
calls, arguments, input/output windows, and branch labels. The committed
`tests/vectors/spring-damper.txt` has 3,176 original-call records: 20 drive
snapshots from steps 300 through 11,000, 20 deterministic Step-fuzz runs, 30
SnapAt entry calls (one per reached function), and 192 root-helper fuzz calls.
These cover the observed Step stop/speed paths and helper clamp, merge, curve,
relax, solve, square-root, and zero-update branches. Each fixture section names
its capture source. `tests/spring_damper_vectors.rs` checks return values and
every recorded output byte; finite values are bit-exact, while NaN words are compared by NaN
class as documented in `notes/rust-physics.md` under “Platform: default NaN.”

## Function coverage

Every reached function has a per-function replay test. Counts below are calls
matched from the committed fixture. The branch column lists classifier labels
that were observed; the tests assert every listed label exists. The Step labels
cover the 19 outcomes named by its current classifier, but do not claim every
combination of its independent conditions was exercised. Functions with no
classifier branch have `—` in the branch column. No outcome is missing from
the explicit helper classifiers; unclassified functions do not claim branch
coverage.

| Function | Address | Calls | Observed branch labels |
|---|---:|---:|---|
| `CSetupOptions::Unk1C` | `0x52d710` | 1 | — |
| `CDef::Unk20` | `0x52d730` | 1 | — |
| `CDef::CDef` | `0x52d750` | 1 | — |
| `CU32Value::Clear` | `0x52d960` | 1 | — |
| `CSetup::Unk20` | `0x52d9e0` | 1 | — |
| `CState::SetUnk10` | `0x52da40` | 1 | — |
| `CState::CState` | `0x52da50` | 1 | — |
| `CopyStateToDef` | `0x52dbd0` | 1 | — |
| `UpdateForces` | `0x52dd70` | 4 | — |
| `CDamperSignal::Emit` | `0x52de60` | 1 | — |
| `CSpringDamper::CSpringDamper` | `0x52df20` | 1 | — |
| `ResetOutputs` | `0x52e010` | 1 | — |
| `CSpringDamper::Step` | `0x52e090` | 181 | 19 stop/speed labels; no other classifier outcomes recorded |
| `DamperStep1` | `0x52e350` | 649 | — |
| `DamperStep2` | `0x52e370` | 331 | — |
| `DamperBiasSub` | `0x52e390` | 11 | — |
| `DamperAccumA` | `0x52e3a0` | 1,125 | — |
| `DamperAccumB` | `0x52e3d0` | 11 | — |
| `DamperClamp` | `0x52e400` | 11 | `pass` (6), `clamp` (5) |
| `DamperMerge` | `0x52e460` | 11 | `weighted` (5), `take_a` (2), `sum` (4) |
| `DamperUpdateA` | `0x52e4c0` | 51 | `update` (48), `zero` (3) |
| `DamperUpdateB` | `0x52e4f0` | 11 | `update` (9), `zero` (2) |
| `DamperUpdateC` | `0x52e530` | 41 | `update` (39), `zero` (2) |
| `DamperScale` | `0x52e590` | 171 | — |
| `DamperSqrt4` | `0x52e5a0` | 91 | `positive` (66), `nonpositive` (25) |
| `DamperCurve` | `0x52e5e0` | 41 | `below_one` (38), `at_or_above_one` (3) |
| `DamperCombine` | `0x52e630` | 81 | — |
| `DamperRelax` | `0x52e6d0` | 42 | `relax` (36), `reset` (6) |
| `DamperSolve` | `0x52e760` | 131 | `release` (69), `idle` (58), `relax` (1), `clear` (3) |
| `DamperForce` | `0x52e8a0` | 171 | — |

One concrete before/after vector is `DamperClamp`'s `spring-direct-fuzz.txt`
call 50: parameters are `[-0.0, -1.0, -1.0, -0.00001]`, the target starts at
`[0.0, 1.0]`, and it returns `0.0` after storing `[0.0, -0.00001]`. Rust
matches both the return and the changed target bytes. `DamperScale` also checks
the x87 return path directly: input bits `0x439291ec` return ST0 bits
`0x417bf4bd20000000`.

The original call traces and the C++ reconstruction agreed on the calls in
this batch; no C++ disagreement was established, and `src/rbr` was not changed.

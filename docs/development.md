# Development

How ReRBR fits together, the full repository layout and the checks. The [README](../README.md#contributing) has the
short version.

The goal is RBR with NGP 7 physics and its NGP cars, on the original and the modded stages:
- **1.02:** every retail stage and car, physics and gameplay bit-exact against recordings of the original.
- **NGP 7.4.778:** NGP's physics (`rbr-standalone --ngp-physics`), checked against the original running NGP.
- **NGP cars:** the community's NGP car packs (as RallySimFans ships them; `rbr-standalone --rsf-car`,
  `notes/formats/rsf-cars.md`).
- **Modded stages:** community stages in the original stage format (as RallySimFans ships them).
- **Live play:** all of it in live play, working together: pacing, HUD, sound, force feedback.

The 1.02 UI, menus and renderer are not carried over (`notes/rust-plan.md`).

## How it fits together

1. **Original game: the oracle.** The official 1.02 binaries and data, run on a Linux or Windows machine with the
   original game.
2. **C++ reconstruction: the behaviour reference.** A separate repository, rbr-recomp, rebuilds the 1.02 binaries as
   C++98 for MSVC 7.1, compared function by function with reccmp. It is how we read and pin down what the game
   computes. It is frozen; Rust is translated from it and checked against the original directly.
3. **Harness: the check.** `harness/` records drives in the original game (its plugin), snapshots its physics step
   and replays it in an emulator, where the Rust ports run in place of the original's functions and the state is
   compared step by step (`harness/README.md`).
4. **Rust: the reimplementation.** `rust/` holds the format parsers (`rbr-formats`), the physics ports
   (`rbr-physics`), the emulator-hosted hybrid step that checks them inside the original's step (`rbr-hybrid`), the
   physics program (`rbr-standalone`), the renderer (`rbr-render`), sound (`rbr-sound`) and the launcher. The end
   product is a standalone Rust program with no original code running; done means bit-for-bit the original's state
   on every recorded drive (`notes/rust-plan.md`, "Done means").

Paths under `src/`, `symbols/` and `types/` in comments and notes (for example `src/rbr/RBR_E45_Material.cpp:149` as
evidence), and the matching notes and scripts no longer here (`notes/toolchain.md`, `scripts/rec.sh`,
`harness/transplant.py`, ...), refer to the rbr-recomp repository. `data/rbr.recomp.csv` keeps its last state for
the tools here: each function's source file and reccmp score and name.

## Layout

- `original/` – original binaries and game data (gitignored), with `SHA256SUMS`. Target:
  `original/patch-1.02/files/Main_group/` (official 1.02); retail data in `original/retail-1.0/install/`.
- `rust/` – Rust workspace: `rbr-formats` (data file parsers), `rbr-physics` (physics ports), `rbr-gameplay`,
  `rbr-hybrid` (bindings into the original's step), `rbr-standalone`, `rbr-render` (wgpu renderer and viewer),
  `rbr-sound` (gameplay sound), `rbr-launcher`, `rbr-recording` (harness recordings)
- `harness/` – runtime harness: RBR plugin, recorder and playback runs, divergence checker, step replay and the
  hybrid check (`rust_step.py`), vector capture
- `notes/` – findings per system and file format (`notes/formats/`, `notes/physics/`); progress in
  `notes/progress/`; old reports in `notes/archive/`
- `data/` – the original exe's function bounds (`rbr.functions.csv`), its library functions and globals
  (`rbr.csv`) and the C++ reconstruction's last state (`rbr.recomp.csv`)
- `scripts/` – the Rust checks, the generators of Rust tables from the original binaries, NGP and vector tools
- `build/` – local caches, e.g. the x87 results of the step replay (gitignored)
- `LOG.md` – current state, open items and decisions (kept short and trimmed regularly)

## Checks

From the repository root, before a merge:

```sh
RBR_DATA=<retail install> RBR_PATCH=<1.02 patch Main_group> RBR_RECORDINGS=<recordings> scripts/rust-check.sh
```

It runs `cargo fmt --check`, clippy with warnings as errors, the tests and the physics vectors for the crates a
change touches (`RUST_CHECK_ALL=1` every crate, `RUST_CHECK_VECTORS=none` no vectors). A physics change also needs
the hybrid check: `harness/rust_step.py` with every binding over the drive's snapshots and recording
(`harness/README.md`). A change to `rbr-standalone` or `rbr-hybrid` needs the boot of the reference drive to stay
equal on every step: `rbr-standalone --boot --recording $RBR_RECORDINGS/drive-tanner-corolla.bin --stage track-54_E
--steps 13000`.

# rbr-physics

Bit-exact ports of the 1.02 physics functions, plus NGP 7.4.778's own (`Variant` in `src/lib.rs`). Behaviour, precision
and porting rules: [notes/physics.md](../../notes/physics.md). Vector store and hybrid check:
[notes/verification.md](../../notes/verification.md).

## Modules

Raw offsets live only in each module's layout code. Tests are in `tests/`.

| Module | Ports | Layout | Vector test |
| --- | --- | --- | --- |
| `vcu` | CVehicleControlUnit | `layout.rs` | `vcu_vectors` |
| `spring_damper` | CSpringDamper, E48 | `layout.rs` | `spring_damper_vectors` |
| `tyre` | CTyre and the CScratch step | `layout/` | `tyre_vectors` |
| `wheel` | CWheel, E51Wheel | `layout/` | `wheel_vectors` |
| `car_body` | CCar, CCarBody, damage, lights, aero | `layout/` | `car_body_vectors`, `car_body_*` |
| `engine_driveline` | CDrive, drive line, CEngine, E39-E43 | `layout/` | `engine_driveline_vectors` |
| `rigid_body` | CSolid, E47 | `layout/` | `rigid_body_vectors` |
| `collision` | E33 bodies, E34 brake walls, E36-E38 | `layout/` | `collision_vectors`, `collision_*` |
| `entities` | CEntity, frames, CGround, CHingedRigidBody | `layout/` | `entities_vectors`, `entities_*` |
| `physics_core` | CPhysics step, detectors, mounted bodies, passenger, E45/E46 | `layout/` | `physics_core_vectors`, `detectors`, `contact` |
| `ngp` | NGP.dll's own functions | - | `ngp_*` |
| `load`, `ntweak` | car definitions and setups from `.lsp`, the NTweak apply walk | `layout/`, `ntweak/layout.rs` | `load_*` |
| `stage` | the stage's physics world from `.col` and `.mat` | `layout/` | `load_stage` |
| `math`, `fpu` | rsqrt/rcp tables (`data/`), ftol, quaternions, x87 rounding | - | `ftol`, unit tests |
| `mem`, `snapshot`, `store` | game memory, snapshot reader, the storage seam | `store/layout.rs` | `snapshot_reader` |

The per-module READMEs (port plans, working names, open questions) were removed; comments that cite them say
`<module>/README.md` (git 9c28c0b8), readable with `git show 9c28c0b8:rust/rbr-physics/src/<module>/README.md`.

The `poison` feature makes accesses to bytes another store owns panic (check builds only).

## ports.csv

One row per ported function: `addr,module,rust_item,test,status`. Status is `checked` (matches the original's
recorded calls), `unit` (unit tests only) or `partial` (some paths have no record). Add the row in the commit that
adds the port and keep rows sorted by address. `tests/ports_manifest.rs` checks that each module, item and test
exists. `scripts/progress.py report` reads the file.

Each vector test reads its record files from an environment variable (`RBR_ENT_VECTORS`, ...) and skips without it.
`scripts/rust-vectors.sh [module]` sets them from `$RBR_VECTOR_STORE`, one subdirectory per module.

## Related crates

- `crates/rbr-hybrid`: runs ports in place of original functions inside the emulated step. `bound.csv` lists each
  binding and its evidence.
- `crates/rbr-standalone`: runs the pure Rust step against a recording (`--boot`).

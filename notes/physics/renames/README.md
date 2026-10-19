# Physics rename maps

Machine-readable renames for the C++ cleanup, built from the semantics notes (`notes/physics/*.md`) and
the exe's tweak key tables (`notes/physics/tweak-keys.md`, `scripts/tweak-keys.py`). Brief:
`lead/briefs/rename-maps.md` in the lead workspace.

Columns: `kind, class, key, current, proposed, type, confidence, evidence`.

- kind: `field`, `function`, `param`, `local`, or `conflict` (two sources disagree, or the notes say the C++ is
  wrong: a behaviour bug, never papered over by a rename).
- key: fields by the value offset, with the tweak sub-object offset in parentheses (`0x16c (CReal 0x160)`);
  functions by original address; params `0xADDR:index` (0-based); locals `0xADDR:name`.
- current: today's name, or `raw +0xNNN at file:line` for raw-offset accesses.
- confidence: only `known` (key name, string, unambiguous equation) or `inferred` (argued in evidence);
  guesses are left out.

Applying a map to the C++ reconstruction (rbr-recomp; per subsystem, once its gate items are done and its
owners are idle): keep a copy of `build/rbr` as the baseline, apply the rows (rename members, replace raw-offset
accesses by typed members), rebuild, and require `python3 -I scripts/codegen-diff.py BASELINE build/rbr -- <changed files>` to report
0 differences. A row that changes code (e.g. a param type) is applied only if the code stays identical.

| Subsystem | File | Rows |
|---|---|---|
| car-body | car-body.csv | 60 |
| collision | collision.csv | 39 |
| engine-driveline | engine-driveline.csv | 61 |
| entities | entities.csv | 65 |
| physics-core | physics-core.csv | 52 |
| rigid-body | rigid-body.csv | 57 |
| spring-damper | spring-damper.csv | 59 |
| tyre | tyre.csv | 31 |
| vcu | vcu.csv | 74 |
| wheel | wheel.csv | 16 |

Open (left unmapped): CEntity 0x58 (owner_self in entities.md vs cell_link in rigid-body.md); CEntity 0x10/0x14/0x18 are int in the header but pointers in the notes (a type change: apply only with codegen-diff 0); the four CPhysics state lists 0x450-0x480 (RegWall distributes into all four by S80 category).

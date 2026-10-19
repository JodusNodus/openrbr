One file per subsystem's vector set for harness/vcu_vectors.py (`--set <SET_NAME>`). Each file defines `SET_NAME`
and `SPECS` (address -> spec dict, same keys as VCU_SPECS) and may use vcu_vectors.py's helpers (`no_branch` etc.).

`--set a+b` records the union of two sets (a later set's spec for an address wins).

Fuzzing: a spec may have `"fuzz": f(mu, args, rng, put)`. With `--fuzz SEED`, each call of that function (outside
Step/StepBasic/FlagDispatch, and the one call of a load-time entry dump) gets random inputs before its windows are taken: `put(address, value, code="<f")` writes
one value (struct code, e.g. "<I" for a word) and the old bytes go back when the call returns, so the rest of the step
runs on the drive's own values. `args` are the call's arguments (this first for thiscall).

Direct calls: a spec may have `"direct_fuzz": f(mu, heap, rng) -> (stack words, [(address, bytes)], this)`, the
recipe `harness/direct_calls.py` runs for that function (`--count N` calls, each from the entry). `heap` is the replay's
stub heap, free for the recipe's own objects and tables; `this` is ecx (None for stdcall). Use it for a function the
step does not reach with the values it needs. The recipe replaces `fuzz` and `direct_args` in those direct runs.

A spec may have `"fuzz_args": f(args, rng) -> {argument index: value}` (or None), with `--fuzz SEED`: each value replaces
that argument in its stack word before the record takes the arguments (a thiscall's index 1 is the first stack argument).
The old word goes back on return, like "fuzz" bytes. A thiscall's this (ecx, index 0) can't be replaced.

A spec may also have `"result": f(mu, args, rng, write) -> eax` (with `--fuzz SEED`): the call is not run. The spec
writes the callee's outputs with `write(address, value, code="<f")` (kept, not restored), returns eax, and the call
returns to its caller at once. The windows and "after" are read as for a run call.

Windows on inner and allocated memory: an external spec may have `windows` like any listed call; they are written
after its `inner` line as `iwin <k> <addr> <len> <pre hex> <post hex>`. Any spec may have `"after": f(mu, args, eax)
-> [(address, length)]`, read only on return (an object the call allocates, a stack buffer a callee filled):
`awin <addr> <len> <hex>` for a call, `iawin ...` after an inner line. Parsers that don't know these lines skip them.

The `k` of an `iwin` (and of `win`) counts the spec's `windows` in order, not the argument index: a spec with
`windows=[(1, ...), (3, ...)]` writes its windows as `iwin 0` (argument 1) and `iwin 1` (argument 3).

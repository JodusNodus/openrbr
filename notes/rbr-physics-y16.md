# Y16 RBR physics matching notes

The `CEngine::CState` member block at `+0x18c` contains nine `CReal` objects,
then a `CUnsigned` at `+0x228`, then three `CReal` objects at `+0x240`,
`+0x250`, and `+0x260`. Constructor calls confirm gaps at `+0x1dc..+0x1e3`,
`+0x224..+0x227`, and `+0x238..+0x23f`; the full state remains `0x280` bytes.

`CCar::Sample9800` emits a 16-dword sensor record. The ordered fields are the
engine state, drive getter, scaled drive getter, engine sample, zero,
`SimE970` result, two zeros, car word `+0xaec`, car scalar `+0x15f4 * 0.001`,
engine getter `Get48_58`, unsigned-converted word at E39 `+0x80`, E39 `+0x8c *
12`, `1.0f`, `SimE980`, and timer `Diff`. `SimE970` returns the float left in
ST0 by `DamperBiasSub`; its one observed caller consumes that result.

In `CCar::Vf28x`, the collision pair-walk call is `MB560(pu, this, loc160)`
on `g_7cbb00`; the global flag test follows `g_1659b70->m_unk24` to the flags
at `+0x160e0`. `CDrive::CState`'s `E60_566690` check receives the object held
at `DAT_0165fa90`.
